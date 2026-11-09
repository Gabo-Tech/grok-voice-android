//! JNI for spoken notifications. Piper synthesis never loads the language model.

use jni::{
    JNIEnv,
    objects::{JClass, JString},
    sys::{jboolean, jint, jshortArray, jstring},
};

use crate::{audio::playback, local::engine, notices};

fn java_string(env: &mut JNIEnv, value: &JString) -> String {
    match env.get_string(value) {
        Ok(text) => String::from(text),
        Err(_) => {
            let _ = env.exception_clear();
            String::new()
        }
    }
}

fn return_string(env: &mut JNIEnv, text: &str) -> jstring {
    match env.new_string(text) {
        Ok(value) => value.into_raw(),
        Err(_) => {
            let _ = env.exception_clear();
            std::ptr::null_mut()
        }
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_ai_grokvoice_app_RustBridge_planSpokenNotice<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    spec: JString<'local>,
    title: JString<'local>,
    body: JString<'local>,
) -> jstring {
    let spec = java_string(&mut env, &spec);
    let title = java_string(&mut env, &title);
    let body = java_string(&mut env, &body);
    let line = notices::plan_spoken_notice_json(&spec, &title, &body);
    return_string(&mut env, &line)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_ai_grokvoice_app_RustBridge_noticePiperPack<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    locale: JString<'local>,
    phone_lang: JString<'local>,
) -> jstring {
    let locale = java_string(&mut env, &locale);
    let phone_lang = java_string(&mut env, &phone_lang);
    let pack = notices::piper_pack(&locale, &phone_lang).unwrap_or("");
    return_string(&mut env, pack)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_ai_grokvoice_app_RustBridge_synthesizeNotice<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    model: JString<'local>,
    tokens: JString<'local>,
    data_dir: JString<'local>,
    text: JString<'local>,
) -> jshortArray {
    let model = java_string(&mut env, &model);
    let tokens = java_string(&mut env, &tokens);
    let data_dir = java_string(&mut env, &data_dir);
    let text = java_string(&mut env, &text);
    let pcm = match engine::synthesize_notice(&model, &tokens, &data_dir, &text) {
        Ok(pcm) => pcm,
        Err(_) => return std::ptr::null_mut(),
    };
    if pcm.is_empty() || pcm.len() > i32::MAX as usize {
        return std::ptr::null_mut();
    }
    let array = match env.new_short_array(pcm.len() as i32) {
        Ok(array) => array,
        Err(_) => {
            let _ = env.exception_clear();
            return std::ptr::null_mut();
        }
    };
    if env.set_short_array_region(&array, 0, &pcm).is_err() {
        let _ = env.exception_clear();
        return std::ptr::null_mut();
    }
    array.into_raw() as jshortArray
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_ai_grokvoice_app_RustBridge_noticeSampleRate(
    _env: JNIEnv<'_>,
    _class: JClass<'_>,
) -> jint {
    engine::notice_sample_rate()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_ai_grokvoice_app_RustBridge_talkOutputBusy(
    _env: JNIEnv<'_>,
    _class: JClass<'_>,
) -> jboolean {
    u8::from(playback::talk_output_busy())
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_ai_grokvoice_app_RustBridge_setNoticeSpeaking(
    _env: JNIEnv<'_>,
    _class: JClass<'_>,
    on: jboolean,
) {
    playback::set_notice_speaking(on != 0);
}
