#pragma once

/* Thin C ABI over sherpa-onnx (Moonshine + Piper) and llama.cpp.
   The Android build links this into the app. Host tests do not compile it. */

#ifdef __cplusplus
extern "C" {
#endif

typedef struct GvAsr GvAsr;
typedef struct GvTts GvTts;
typedef struct GvLlm GvLlm;

GvAsr *gv_asr_load(
    const char *preprocess,
    const char *encoder,
    const char *uncached_decoder,
    const char *cached_decoder,
    const char *tokens,
    int threads,
    char *err,
    int err_len);
GvAsr *gv_asr_load_transducer(
    const char *encoder,
    const char *decoder,
    const char *joiner,
    const char *tokens,
    int threads,
    char *err,
    int err_len);
void gv_asr_free(GvAsr *asr);
int gv_asr_transcribe(
    GvAsr *asr,
    const float *samples,
    int n,
    int sample_rate,
    char *out,
    int out_len,
    char *err,
    int err_len);

GvTts *gv_tts_load(
    const char *model,
    const char *tokens,
    const char *data_dir,
    int threads,
    char *err,
    int err_len);
void gv_tts_free(GvTts *tts);
int gv_tts_synth(
    GvTts *tts,
    const char *text,
    float **samples,
    int *n,
    int *sample_rate,
    char *err,
    int err_len);

GvLlm *gv_llm_load(const char *model, int n_ctx, int threads, char *err, int err_len);
void gv_llm_free(GvLlm *llm);
/* 0 writes a reply. A grammar or sampler failure returns -2 and sets err.
   The C++ exception must be caught inside this call. It aborts the process
   if it reaches Rust. */
int gv_llm_complete(
    GvLlm *llm,
    const char *prompt,
    const char *grammar,
    int max_tokens,
    char *out,
    int out_len,
    char *err,
    int err_len);

void gv_free(void *ptr);

#ifdef __cplusplus
}
#endif
