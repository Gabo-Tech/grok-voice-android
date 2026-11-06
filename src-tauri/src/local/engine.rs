//! Android links Moonshine, Piper, and llama.cpp. Host builds keep the same
//! call shape so routing tests run without a device.

use super::packs::ModelPaths;

pub struct Engines {
    #[cfg(target_os = "android")]
    inner: Native,
}

#[cfg(target_os = "android")]
struct Native {
    asr: *mut (),
    tts: *mut (),
    llm: *mut (),
}

#[cfg(target_os = "android")]
unsafe impl Send for Engines {}

impl Engines {
    pub fn load(paths: &ModelPaths, threads: i32) -> Result<Self, String> {
        #[cfg(target_os = "android")]
        {
            Ok(Self { inner: Native::load(paths, threads)? })
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = (paths, threads);
            Err("On-device speech runs in the Android app.".into())
        }
    }

    pub fn transcribe(&mut self, samples: &[f32]) -> Result<String, String> {
        #[cfg(target_os = "android")]
        {
            self.inner.transcribe(samples)
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = samples;
            Err("On-device speech runs in the Android app.".into())
        }
    }

    pub fn complete(&mut self, prompt: &str, grammar: &str) -> Result<String, String> {
        #[cfg(target_os = "android")]
        {
            self.inner.complete(prompt, grammar)
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = (prompt, grammar);
            Err("On-device speech runs in the Android app.".into())
        }
    }

    pub fn synthesize(&mut self, text: &str) -> Result<(Vec<f32>, u32), String> {
        #[cfg(target_os = "android")]
        {
            self.inner.synthesize(text)
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = text;
            Err("On-device speech runs in the Android app.".into())
        }
    }
}

#[cfg(target_os = "android")]
mod ffi {
    use std::os::raw::{c_char, c_int};

    unsafe extern "C" {
        pub fn gv_asr_load(
            preprocess: *const c_char,
            encoder: *const c_char,
            uncached: *const c_char,
            cached: *const c_char,
            tokens: *const c_char,
            threads: c_int,
            err: *mut c_char,
            err_len: c_int,
        ) -> *mut ();
        pub fn gv_asr_free(asr: *mut ());
        pub fn gv_asr_transcribe(
            asr: *mut (),
            samples: *const f32,
            n: c_int,
            sample_rate: c_int,
            out: *mut c_char,
            out_len: c_int,
            err: *mut c_char,
            err_len: c_int,
        ) -> c_int;
        pub fn gv_tts_load(
            model: *const c_char,
            tokens: *const c_char,
            data_dir: *const c_char,
            threads: c_int,
            err: *mut c_char,
            err_len: c_int,
        ) -> *mut ();
        pub fn gv_tts_free(tts: *mut ());
        pub fn gv_tts_synth(
            tts: *mut (),
            text: *const c_char,
            samples: *mut *mut f32,
            n: *mut c_int,
            sample_rate: *mut c_int,
            err: *mut c_char,
            err_len: c_int,
        ) -> c_int;
        pub fn gv_llm_load(
            model: *const c_char,
            n_ctx: c_int,
            threads: c_int,
            err: *mut c_char,
            err_len: c_int,
        ) -> *mut ();
        pub fn gv_llm_free(llm: *mut ());
        pub fn gv_llm_complete(
            llm: *mut (),
            prompt: *const c_char,
            grammar: *const c_char,
            max_tokens: c_int,
            out: *mut c_char,
            out_len: c_int,
            err: *mut c_char,
            err_len: c_int,
        ) -> c_int;
        pub fn gv_free(ptr: *mut ());
    }
}

#[cfg(target_os = "android")]
impl Native {
    fn load(paths: &ModelPaths, threads: i32) -> Result<Self, String> {
        use ffi::*;
        use std::os::raw::c_int;

        let preprocess = c_path(&paths.preprocess)?;
        let encoder = c_path(&paths.encoder)?;
        let uncached = c_path(&paths.uncached)?;
        let cached = c_path(&paths.cached)?;
        let tokens = c_path(&paths.stt_tokens)?;
        let mut err = err_buf();
        let asr = unsafe {
            gv_asr_load(
                preprocess.as_ptr(),
                encoder.as_ptr(),
                uncached.as_ptr(),
                cached.as_ptr(),
                tokens.as_ptr(),
                threads,
                err.as_mut_ptr(),
                err.len() as c_int,
            )
        };
        if asr.is_null() {
            return Err(read_buf(&err).unwrap_or_else(|| "could not load speech recognition".into()));
        }
        let model = c_path(&paths.tts_model)?;
        let tts_tokens = c_path(&paths.tts_tokens)?;
        let data = c_path(&paths.tts_data)?;
        let tts = unsafe {
            gv_tts_load(model.as_ptr(), tts_tokens.as_ptr(), data.as_ptr(), threads, err.as_mut_ptr(), err.len() as c_int)
        };
        if tts.is_null() {
            unsafe { gv_asr_free(asr) };
            return Err(read_buf(&err).unwrap_or_else(|| "could not load the voice".into()));
        }
        let llm_path = c_path(&paths.llm)?;
        let llm = unsafe { gv_llm_load(llm_path.as_ptr(), 2048, threads, err.as_mut_ptr(), err.len() as c_int) };
        if llm.is_null() {
            unsafe {
                gv_tts_free(tts);
                gv_asr_free(asr);
            }
            return Err(read_buf(&err).unwrap_or_else(|| "could not load the language model".into()));
        }
        Ok(Self { asr, tts, llm })
    }

    fn transcribe(&self, samples: &[f32]) -> Result<String, String> {
        use ffi::*;
        use std::os::raw::c_int;
        let mut out = text_buf(4096);
        let mut err = err_buf();
        let rc = unsafe {
            gv_asr_transcribe(
                self.asr,
                samples.as_ptr(),
                samples.len() as c_int,
                16_000,
                out.as_mut_ptr(),
                out.len() as c_int,
                err.as_mut_ptr(),
                err.len() as c_int,
            )
        };
        if rc != 0 {
            return Err(read_buf(&err).unwrap_or_else(|| "speech recognition failed".into()));
        }
        Ok(read_buf(&out).unwrap_or_default())
    }

    fn complete(&self, prompt: &str, grammar: &str) -> Result<String, String> {
        use ffi::*;
        use std::ffi::CString;
        use std::os::raw::c_int;
        let prompt = CString::new(prompt).map_err(|_| "prompt contained a null".to_string())?;
        let grammar = CString::new(grammar).map_err(|_| "grammar contained a null".to_string())?;
        let mut out = text_buf(8192);
        let mut err = err_buf();
        let rc = unsafe {
            gv_llm_complete(
                self.llm,
                prompt.as_ptr(),
                grammar.as_ptr(),
                192,
                out.as_mut_ptr(),
                out.len() as c_int,
                err.as_mut_ptr(),
                err.len() as c_int,
            )
        };
        if rc != 0 {
            return Err(read_buf(&err).unwrap_or_else(|| "the language model failed".into()));
        }
        Ok(read_buf(&out).unwrap_or_default())
    }

    fn synthesize(&self, text: &str) -> Result<(Vec<f32>, u32), String> {
        use ffi::*;
        use std::ffi::CString;
        use std::os::raw::c_int;
        let text = CString::new(text).map_err(|_| "text contained a null".to_string())?;
        let mut samples = std::ptr::null_mut();
        let mut n: c_int = 0;
        let mut rate: c_int = 0;
        let mut err = err_buf();
        let rc = unsafe {
            gv_tts_synth(self.tts, text.as_ptr(), &mut samples, &mut n, &mut rate, err.as_mut_ptr(), err.len() as c_int)
        };
        if rc != 0 || samples.is_null() || n <= 0 {
            return Err(read_buf(&err).unwrap_or_else(|| "speech synthesis failed".into()));
        }
        let pcm = unsafe { std::slice::from_raw_parts(samples, n as usize) }.to_vec();
        unsafe { gv_free(samples.cast()) };
        Ok((pcm, rate.max(0) as u32))
    }
}

#[cfg(target_os = "android")]
impl Drop for Native {
    fn drop(&mut self) {
        use ffi::*;
        unsafe {
            if !self.llm.is_null() {
                gv_llm_free(self.llm);
            }
            if !self.tts.is_null() {
                gv_tts_free(self.tts);
            }
            if !self.asr.is_null() {
                gv_asr_free(self.asr);
            }
        }
    }
}

#[cfg(target_os = "android")]
fn err_buf() -> Vec<std::os::raw::c_char> {
    vec![0; 512]
}

#[cfg(target_os = "android")]
fn text_buf(n: usize) -> Vec<std::os::raw::c_char> {
    vec![0; n]
}

#[cfg(target_os = "android")]
fn c_path(path: &std::path::Path) -> Result<std::ffi::CString, String> {
    std::ffi::CString::new(path.to_string_lossy().as_bytes()).map_err(|_| "model path contained a null".into())
}

#[cfg(target_os = "android")]
fn read_buf(buf: &[std::os::raw::c_char]) -> Option<String> {
    let text = unsafe { std::ffi::CStr::from_ptr(buf.as_ptr()) }
        .to_string_lossy()
        .trim_matches('\0')
        .trim()
        .to_string();
    if text.is_empty() { None } else { Some(text) }
}
