#include "local_engine.h"

#include <android/log.h>
#include <exception>
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
    /* Beam search is supported for these NeMo transducers (German and Spanish)
       and catches more words than greedy. Moonshine stays greedy; it has no beam. */
    config.decoding_method = "modified_beam_search";
    config.max_active_paths = 4;

    asr->recognizer = SherpaOnnxCreateOfflineRecognizer(&config);
    if (!asr->recognizer) {
        gv_asr_free(asr);
        gv_set_err(err, err_len, "could not load the speech model");
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
    /* 256 keeps the prompt-eval scratch smaller than 512 so a short voice
       turn is less likely to get the process killed on a 6 GB phone. */
    cp.n_batch = 256;
    cp.n_ubatch = 256;
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
    llm->n_batch = 256;
    return llm;
}

/* ChatML, Gemma, and Llama 3 do not share one end-of-turn string.
   SmolLM and Qwen emit <|im_end|>, Gemma emits <end_of_turn>, Llama 3
   emits <|eot_id|>. Stopping on only the ChatML marker let the other
   packs run out the token cap and look like a dead turn. */
static int gv_is_stop_text(const char *text) {
    if (!text || !text[0]) {
        return 0;
    }
    return strstr(text, "<|im_end|>") != NULL || strstr(text, "<end_of_turn>") != NULL || strstr(text, "<|eot_id|>") != NULL;
}

static void gv_strip_stop(char *text) {
    if (!text) {
        return;
    }
    const char *stops[] = {"<|im_end|>", "<end_of_turn>", "<|eot_id|>"};
    for (int i = 0; i < 3; i++) {
        char *hit = strstr(text, stops[i]);
        if (hit) {
            *hit = '\0';
        }
    }
}

/* Stop once the spoken JSON object is closed, or the chat turn ends.
   The caller's token cap is only the backstop. Waiting out a long cap
   is what made a short reply feel hung. */
static int gv_reply_ready(const char *text) {
    if (!text || !text[0]) {
        return 0;
    }
    if (gv_is_stop_text(text)) {
        return 1;
    }
    int depth = 0;
    int started = 0;
    int in_string = 0;
    int escape = 0;
    for (const char *p = text; *p; ++p) {
        unsigned char c = (unsigned char)*p;
        if (in_string) {
            if (escape) {
                escape = 0;
                continue;
            }
            if (c == '\\') {
                escape = 1;
                continue;
            }
            if (c == '"') {
                in_string = 0;
            }
            continue;
        }
        if (c == '"') {
            in_string = 1;
            continue;
        }
        if (c == '{') {
            depth++;
            started = 1;
            continue;
        }
        if (c == '}') {
            if (depth > 0) {
                depth--;
            }
            if (started && depth == 0) {
                return 1;
            }
        }
    }
    return 0;
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

struct GvMalloc {
    void *ptr;
    GvMalloc() : ptr(nullptr) {}
    ~GvMalloc() { free(ptr); }
    GvMalloc(const GvMalloc &) = delete;
    GvMalloc &operator=(const GvMalloc &) = delete;
};

struct GvSamplerFree {
    llama_sampler *chain;
    GvSamplerFree() : chain(nullptr) {}
    ~GvSamplerFree() {
        if (chain) {
            llama_sampler_free(chain);
        }
    }
    GvSamplerFree(const GvSamplerFree &) = delete;
    GvSamplerFree &operator=(const GvSamplerFree &) = delete;
};

static void gv_llm_reset(GvLlm *llm) {
    if (!llm || !llm->ctx) {
        return;
    }
    try {
        llama_memory_clear(llama_get_memory(llm->ctx), true);
    } catch (...) {
    }
}

static int gv_llm_complete_body(
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

    GvMalloc tokens;
    int cap = 4096;
    tokens.ptr = malloc(sizeof(llama_token) * (size_t)cap);
    if (!tokens.ptr) {
        gv_set_err(err, err_len, "out of memory");
        return -1;
    }
    llama_token *toks = static_cast<llama_token *>(tokens.ptr);
    int n = llama_tokenize(llm->vocab, prompt, (int)strlen(prompt), toks, cap, false, true);
    if (n < 0) {
        int need = -n;
        if (need > 0 && need < 16384) {
            void *grown = realloc(tokens.ptr, sizeof(llama_token) * (size_t)need);
            if (!grown) {
                gv_set_err(err, err_len, "out of memory");
                return -1;
            }
            tokens.ptr = grown;
            toks = static_cast<llama_token *>(tokens.ptr);
            n = llama_tokenize(llm->vocab, prompt, (int)strlen(prompt), toks, need, false, true);
        }
    }
    if (n <= 0) {
        gv_set_err(err, err_len, "could not tokenize the prompt");
        return -1;
    }
    /* Leave room for the reply. The old fixed 1900 cut left almost no
       tokens when the context was 2048 and the cap was 192. */
    int room = (int)llama_n_ctx(llm->ctx) - max_tokens - 8;
    if (room < 128) {
        room = 128;
    }
    if (n > room) {
        gv_set_err(err, err_len, "that turn is too long for the on-device model");
        return -1;
    }
    if (gv_decode(llm->ctx, toks, n, llm->n_batch) != 0) {
        gv_set_err(err, err_len, "the on-device model failed while reading the turn");
        return -1;
    }

    struct llama_sampler_chain_params params = llama_sampler_chain_default_params();
    GvSamplerFree sampler;
    sampler.chain = llama_sampler_chain_init(params);
    if (!sampler.chain) {
        gv_set_err(err, err_len, "could not sample a reply");
        return -1;
    }
    if (grammar && grammar[0]) {
        struct llama_sampler *g = llama_sampler_init_grammar(llm->vocab, grammar, "root");
        if (!g) {
            gv_set_err(err, err_len, "grammar rejected");
            return -2;
        }
        llama_sampler_chain_add(sampler.chain, g);
    }
    llama_sampler_chain_add(sampler.chain, llama_sampler_init_greedy());

    out[0] = '\0';
    int used = 0;
    for (int i = 0; i < max_tokens; i++) {
        /* The first sample after a grammar is attached is where a combined
           piece such as {" used to throw and abort the process. */
        llama_token id = llama_sampler_sample(sampler.chain, llm->ctx, -1);
        llama_sampler_accept(sampler.chain, id);
        if (llama_vocab_is_eog(llm->vocab, id) || gv_is_stop_text(llama_vocab_get_text(llm->vocab, id))) {
            break;
        }
        char piece[256];
        int np = llama_token_to_piece(llm->vocab, id, piece, (int)sizeof(piece), 0, false);
        if (np > 0 && used + np < out_len - 1) {
            memcpy(out + used, piece, (size_t)np);
            used += np;
            out[used] = '\0';
        }
        if (gv_reply_ready(out)) {
            break;
        }
        llama_token next = id;
        if (gv_decode(llm->ctx, &next, 1, llm->n_batch) != 0) {
            break;
        }
    }
    gv_strip_stop(out);
    return 0;
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
    try {
        return gv_llm_complete_body(llm, prompt, grammar, max_tokens, out, out_len, err, err_len);
    } catch (const std::exception &ex) {
        const char *what = ex.what() ? ex.what() : "the reply stopped";
        __android_log_print(ANDROID_LOG_ERROR, "GrokVoice", "on-device grammar failed: %s", what);
        gv_llm_reset(llm);
        char buf[480];
        snprintf(buf, sizeof(buf), "grammar failed: %s", what);
        gv_set_err(err, err_len, buf);
        return -2;
    } catch (...) {
        __android_log_print(ANDROID_LOG_ERROR, "GrokVoice", "on-device grammar failed");
        gv_llm_reset(llm);
        gv_set_err(err, err_len, "grammar failed: the on-device model stopped the reply");
        return -2;
    }
}

void gv_free(void *ptr) {
    free(ptr);
}
