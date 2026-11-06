#include "local_engine.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "c-api.h"
#include "llama.h"

static void gv_set_err(char *err, int err_len, const char *msg) {
    if (!err || err_len <= 0) {
        return;
    }
    snprintf(err, (size_t)err_len, "%s", msg ? msg : "error");
}

static char *gv_dup(const char *text) {
    if (!text) {
        return NULL;
    }
    size_t n = strlen(text);
    char *out = (char *)malloc(n + 1);
    if (!out) {
        return NULL;
    }
    memcpy(out, text, n + 1);
    return out;
}

struct GvAsr {
    const SherpaOnnxOfflineRecognizer *recognizer;
    char *preprocess;
    char *encoder;
    char *uncached;
    char *cached;
    char *tokens;
};

struct GvTts {
    const SherpaOnnxOfflineTts *tts;
    char *model;
    char *tokens;
    char *data_dir;
};

struct GvLlm {
    struct llama_model *model;
    struct llama_context *ctx;
    const struct llama_vocab *vocab;
    int n_batch;
};

GvAsr *gv_asr_load(
    const char *preprocess,
    const char *encoder,
    const char *uncached_decoder,
    const char *cached_decoder,
    const char *tokens,
    int threads,
    char *err,
    int err_len) {
    GvAsr *asr = (GvAsr *)calloc(1, sizeof(GvAsr));
    if (!asr) {
        gv_set_err(err, err_len, "out of memory");
        return NULL;
    }
    asr->preprocess = gv_dup(preprocess);
    asr->encoder = gv_dup(encoder);
    asr->uncached = gv_dup(uncached_decoder);
    asr->cached = gv_dup(cached_decoder);
    asr->tokens = gv_dup(tokens);
    if (!asr->preprocess || !asr->encoder || !asr->uncached || !asr->cached || !asr->tokens) {
        gv_asr_free(asr);
        gv_set_err(err, err_len, "out of memory");
        return NULL;
    }

    SherpaOnnxOfflineRecognizerConfig config;
    memset(&config, 0, sizeof(config));
    config.feat_config.sample_rate = 16000;
    config.feat_config.feature_dim = 80;
    config.model_config.moonshine.preprocessor = asr->preprocess;
    config.model_config.moonshine.encoder = asr->encoder;
    config.model_config.moonshine.uncached_decoder = asr->uncached;
    config.model_config.moonshine.cached_decoder = asr->cached;
    config.model_config.tokens = asr->tokens;
    config.model_config.provider = "cpu";
    config.model_config.model_type = "moonshine";
    config.model_config.num_threads = threads > 0 ? threads : 2;
    config.decoding_method = "greedy_search";

    asr->recognizer = SherpaOnnxCreateOfflineRecognizer(&config);
    if (!asr->recognizer) {
        gv_asr_free(asr);
        gv_set_err(err, err_len, "could not load the Moonshine speech model");
        return NULL;
    }
    return asr;
}

GvAsr *gv_asr_load_transducer(
    const char *encoder,
    const char *decoder,
    const char *joiner,
    const char *tokens,
    int threads,
    char *err,
    int err_len) {
    GvAsr *asr = (GvAsr *)calloc(1, sizeof(GvAsr));
    if (!asr) {
        gv_set_err(err, err_len, "out of memory");
        return NULL;
    }
    asr->encoder = gv_dup(encoder);
    asr->uncached = gv_dup(decoder);
    asr->cached = gv_dup(joiner);
    asr->tokens = gv_dup(tokens);
    asr->preprocess = gv_dup("");
    if (!asr->encoder || !asr->uncached || !asr->cached || !asr->tokens || !asr->preprocess) {
        gv_asr_free(asr);
        gv_set_err(err, err_len, "out of memory");
        return NULL;
    }

    SherpaOnnxOfflineRecognizerConfig config;
    memset(&config, 0, sizeof(config));
    config.feat_config.sample_rate = 16000;
    config.feat_config.feature_dim = 80;
    config.model_config.transducer.encoder = asr->encoder;
    config.model_config.transducer.decoder = asr->uncached;
    config.model_config.transducer.joiner = asr->cached;
    config.model_config.tokens = asr->tokens;
    config.model_config.provider = "cpu";
    config.model_config.model_type = "nemo_transducer";
    config.model_config.num_threads = threads > 0 ? threads : 2;
    config.decoding_method = "greedy_search";

    asr->recognizer = SherpaOnnxCreateOfflineRecognizer(&config);
    if (!asr->recognizer) {
        gv_asr_free(asr);
        gv_set_err(err, err_len, "could not load the German speech model");
        return NULL;
    }
    return asr;
}

void gv_asr_free(GvAsr *asr) {
    if (!asr) {
        return;
    }
    if (asr->recognizer) {
        SherpaOnnxDestroyOfflineRecognizer(asr->recognizer);
    }
    free(asr->preprocess);
    free(asr->encoder);
    free(asr->uncached);
    free(asr->cached);
    free(asr->tokens);
    free(asr);
}

int gv_asr_transcribe(
    GvAsr *asr,
    const float *samples,
    int n,
    int sample_rate,
    char *out,
    int out_len,
    char *err,
    int err_len) {
    if (!asr || !asr->recognizer || !samples || n <= 0 || !out || out_len <= 1) {
        gv_set_err(err, err_len, "speech recognizer is not loaded");
        return -1;
    }
    const SherpaOnnxOfflineStream *stream = SherpaOnnxCreateOfflineStream(asr->recognizer);
    if (!stream) {
        gv_set_err(err, err_len, "could not start speech recognition");
        return -1;
    }
    SherpaOnnxAcceptWaveformOffline(stream, sample_rate > 0 ? sample_rate : 16000, samples, n);
    SherpaOnnxDecodeOfflineStream(asr->recognizer, stream);
    const SherpaOnnxOfflineRecognizerResult *result = SherpaOnnxGetOfflineStreamResult(stream);
    out[0] = '\0';
    if (result && result->text) {
        snprintf(out, (size_t)out_len, "%s", result->text);
    }
    if (result) {
        SherpaOnnxDestroyOfflineRecognizerResult(result);
    }
    SherpaOnnxDestroyOfflineStream(stream);
    return 0;
}

GvTts *gv_tts_load(
    const char *model,
    const char *tokens,
    const char *data_dir,
    int threads,
    char *err,
    int err_len) {
    GvTts *tts = (GvTts *)calloc(1, sizeof(GvTts));
    if (!tts) {
        gv_set_err(err, err_len, "out of memory");
        return NULL;
    }
    tts->model = gv_dup(model);
    tts->tokens = gv_dup(tokens);
    tts->data_dir = gv_dup(data_dir);
    if (!tts->model || !tts->tokens || !tts->data_dir) {
        gv_tts_free(tts);
        gv_set_err(err, err_len, "out of memory");
        return NULL;
    }

    SherpaOnnxOfflineTtsConfig config;
    memset(&config, 0, sizeof(config));
    config.model.vits.model = tts->model;
    config.model.vits.tokens = tts->tokens;
    config.model.vits.data_dir = tts->data_dir;
    config.model.vits.noise_scale = 0.667f;
    config.model.vits.noise_scale_w = 0.8f;
    config.model.vits.length_scale = 1.0f;
    config.model.num_threads = threads > 0 ? threads : 2;
    config.model.provider = "cpu";
    config.max_num_sentences = 1;

    tts->tts = SherpaOnnxCreateOfflineTts(&config);
    if (!tts->tts) {
        gv_tts_free(tts);
        gv_set_err(err, err_len, "could not load the Piper voice");
        return NULL;
    }
    return tts;
}

void gv_tts_free(GvTts *tts) {
    if (!tts) {
        return;
    }
    if (tts->tts) {
        SherpaOnnxDestroyOfflineTts(tts->tts);
    }
    free(tts->model);
    free(tts->tokens);
    free(tts->data_dir);
    free(tts);
}

int gv_tts_synth(
    GvTts *tts,
    const char *text,
    float **samples,
    int *n,
    int *sample_rate,
    char *err,
    int err_len) {
    if (!tts || !tts->tts || !text || !text[0] || !samples || !n || !sample_rate) {
        gv_set_err(err, err_len, "voice is not loaded");
        return -1;
    }
    const SherpaOnnxGeneratedAudio *audio = SherpaOnnxOfflineTtsGenerate(tts->tts, text, 0, 1.0f);
    if (!audio || !audio->samples || audio->n <= 0) {
        if (audio) {
            SherpaOnnxDestroyOfflineTtsGeneratedAudio(audio);
        }
        gv_set_err(err, err_len, "could not speak that reply");
        return -1;
    }
    float *copy = (float *)malloc(sizeof(float) * (size_t)audio->n);
    if (!copy) {
        SherpaOnnxDestroyOfflineTtsGeneratedAudio(audio);
        gv_set_err(err, err_len, "out of memory");
        return -1;
    }
    memcpy(copy, audio->samples, sizeof(float) * (size_t)audio->n);
    *samples = copy;
    *n = audio->n;
    *sample_rate = audio->sample_rate > 0 ? audio->sample_rate : 16000;
    SherpaOnnxDestroyOfflineTtsGeneratedAudio(audio);
    return 0;
}

static int gv_decode(struct llama_context *ctx, llama_token *tokens, int n, int n_batch) {
    int done = 0;
    if (n_batch < 1) {
        n_batch = 1;
    }
    while (done < n) {
        int chunk = n - done;
        if (chunk > n_batch) {
            chunk = n_batch;
        }
        struct llama_batch batch = llama_batch_get_one(tokens + done, chunk);
        int rc = llama_decode(ctx, batch);
        if (rc != 0) {
            return rc;
        }
        done += chunk;
    }
    return 0;
}

GvLlm *gv_llm_load(const char *model, int n_ctx, int threads, char *err, int err_len) {
    static int backend = 0;
    if (!backend) {
        llama_backend_init();
        backend = 1;
    }
    if (n_ctx < 512) {
        n_ctx = 2048;
    }
    if (threads < 1) {
        threads = 2;
    }
    struct llama_model_params mp = llama_model_default_params();
    mp.n_gpu_layers = 0;
    struct llama_model *loaded = llama_model_load_from_file(model, mp);
    if (!loaded) {
        gv_set_err(err, err_len, "could not load the on-device language model");
        return NULL;
    }
    struct llama_context_params cp = llama_context_default_params();
    cp.n_ctx = (uint32_t)n_ctx;
    cp.n_batch = 512;
    cp.n_ubatch = 512;
    cp.n_threads = threads;
    cp.n_threads_batch = threads;
    cp.offload_kqv = false;
    struct llama_context *ctx = llama_init_from_model(loaded, cp);
    if (!ctx) {
        llama_model_free(loaded);
        gv_set_err(err, err_len, "could not start the on-device language model");
        return NULL;
    }
    GvLlm *llm = (GvLlm *)calloc(1, sizeof(GvLlm));
    if (!llm) {
        llama_free(ctx);
        llama_model_free(loaded);
        gv_set_err(err, err_len, "out of memory");
        return NULL;
    }
    llm->model = loaded;
    llm->ctx = ctx;
    llm->vocab = llama_model_get_vocab(loaded);
    llm->n_batch = 512;
    return llm;
}

void gv_llm_free(GvLlm *llm) {
    if (!llm) {
        return;
    }
    if (llm->ctx) {
        llama_free(llm->ctx);
    }
    if (llm->model) {
        llama_model_free(llm->model);
    }
    free(llm);
}

int gv_llm_complete(
    GvLlm *llm,
    const char *prompt,
    const char *grammar,
    int max_tokens,
    char *out,
    int out_len,
    char *err,
    int err_len) {
    if (!llm || !llm->ctx || !prompt || !out || out_len <= 1) {
        gv_set_err(err, err_len, "language model is not loaded");
        return -1;
    }
    if (max_tokens < 16) {
        max_tokens = 16;
    }
    if (max_tokens > 512) {
        max_tokens = 512;
    }
    llama_memory_clear(llama_get_memory(llm->ctx), true);

    int cap = 4096;
    llama_token *tokens = (llama_token *)malloc(sizeof(llama_token) * (size_t)cap);
    if (!tokens) {
        gv_set_err(err, err_len, "out of memory");
        return -1;
    }
    int n = llama_tokenize(llm->vocab, prompt, (int)strlen(prompt), tokens, cap, false, true);
    if (n < 0) {
        int need = -n;
        if (need > 0 && need < 16384) {
            llama_token *grown = (llama_token *)realloc(tokens, sizeof(llama_token) * (size_t)need);
            if (!grown) {
                free(tokens);
                gv_set_err(err, err_len, "out of memory");
                return -1;
            }
            tokens = grown;
            n = llama_tokenize(llm->vocab, prompt, (int)strlen(prompt), tokens, need, false, true);
        }
    }
    if (n <= 0) {
        free(tokens);
        gv_set_err(err, err_len, "could not tokenize the prompt");
        return -1;
    }
    if (n > 1900) {
        free(tokens);
        gv_set_err(err, err_len, "that turn is too long for the on-device model");
        return -1;
    }
    if (gv_decode(llm->ctx, tokens, n, llm->n_batch) != 0) {
        free(tokens);
        gv_set_err(err, err_len, "the on-device model failed while reading the turn");
        return -1;
    }
    free(tokens);

    struct llama_sampler_chain_params params = llama_sampler_chain_default_params();
    struct llama_sampler *chain = llama_sampler_chain_init(params);
    if (!chain) {
        gv_set_err(err, err_len, "could not sample a reply");
        return -1;
    }
    if (grammar && grammar[0]) {
        struct llama_sampler *g = llama_sampler_init_grammar(llm->vocab, grammar, "root");
        if (!g) {
            llama_sampler_free(chain);
            gv_set_err(err, err_len, "grammar rejected");
            return -2;
        }
        llama_sampler_chain_add(chain, g);
    }
    llama_sampler_chain_add(chain, llama_sampler_init_greedy());

    out[0] = '\0';
    int used = 0;
    for (int i = 0; i < max_tokens; i++) {
        llama_token id = llama_sampler_sample(chain, llm->ctx, -1);
        llama_sampler_accept(chain, id);
        if (llama_vocab_is_eog(llm->vocab, id)) {
            break;
        }
        char piece[256];
        int np = llama_token_to_piece(llm->vocab, id, piece, (int)sizeof(piece), 0, false);
        if (np > 0 && used + np < out_len - 1) {
            memcpy(out + used, piece, (size_t)np);
            used += np;
            out[used] = '\0';
        }
        llama_token next = id;
        if (gv_decode(llm->ctx, &next, 1, llm->n_batch) != 0) {
            break;
        }
    }
    llama_sampler_free(chain);
    return 0;
}

void gv_free(void *ptr) {
    free(ptr);
}
