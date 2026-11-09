//! JNI entry points for the microphone foreground service.
//!
//! The service reads AudioRecord on its own thread and pushes 16 kHz PCM here.
//! That capture outlives the activity, which cpal's AAudio stream does not.

use jni::{
    JNIEnv,
    objects::{JClass, JShortArray, JString},
};

use crate::{
    audio::{
        engine::{mark_service_failed, poll_present_talk, poll_reopen_mic, push_service_mic, service_mic_stalled},
        resample::i16_to_f32,
    },
    diagnostics,
};

#[unsafe(no_mangle)]
pub extern "system" fn Java_ai_grokvoice_app_RustBridge_feedMic<'local>(
    env: JNIEnv<'local>,
    _class: JClass<'local>,
    data: JShortArray<'local>,
    n: i32,
) {
    let Ok(len) = env.get_array_length(&data) else {
        return;
    };
    let n = (n.max(0) as usize).min(len.max(0) as usize).min(16_000);
    if n == 0 {
        return;
    }
    let mut buf = vec![0i16; n];
    if env.get_short_array_region(&data, 0, &mut buf).is_err() {
        let _ = env.exception_clear();
        return;
    }
    let pcm: Vec<f32> = buf.into_iter().map(i16_to_f32).collect();
    let _ = push_service_mic(pcm);
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_ai_grokvoice_app_RustBridge_pollPresentTalk(
    _env: JNIEnv<'_>,
    _class: JClass<'_>,
) -> u8 {
    u8::from(poll_present_talk())
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_ai_grokvoice_app_RustBridge_pollMicStalled(
    _env: JNIEnv<'_>,
    _class: JClass<'_>,
) -> u8 {
    u8::from(service_mic_stalled())
}

pub extern "system" fn Java_ai_grokvoice_app_RustBridge_pollReopenMic(
    _env: JNIEnv<'_>,
    _class: JClass<'_>,
) -> u8 {
    u8::from(poll_reopen_mic())
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_ai_grokvoice_app_RustBridge_note<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    msg: JString<'local>,
) -> u8 {
    let Ok(text) = env.get_string(&msg) else {
        let _ = env.exception_clear();
        return 0;
    };
    let mut line = String::from(text);
    if line.len() > 400 {
        let mut end = 400;
        while !line.is_char_boundary(end) {
            end -= 1;
        }
        line.truncate(end);
    }
    u8::from(diagnostics::note_line(&line))
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_ai_grokvoice_app_RustBridge_serviceFailed(
    _env: JNIEnv<'_>,
    _class: JClass<'_>,
) {
    mark_service_failed();
}
