mod actions;
mod audio;
mod commands;
mod diagnostics;
mod memory;
mod error;
mod native;
mod realtime;
mod search;
mod settings;
mod vault;
mod wakeword;

use std::sync::Arc;

use tauri::Manager;

use crate::{
    audio::{engine::Engine, playback::Player},
    diagnostics::Hub,
    settings::SettingsStore,
    vault::Vault,
    wakeword::WakewordFiles,
};

/// Called by `RustBridge.stopListening()` when the user taps "Stop" on the persistent
/// listening notification.
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_ai_grokvoice_app_RustBridge_stopListening(
    _env: *mut std::ffi::c_void,
    _class: *mut std::ffi::c_void,
) {
    if let Some(tx) = audio::engine::control() {
        let _ = tx.send(audio::engine::Ctl::EndSession);
        let _ = tx.send(audio::engine::Ctl::SetWakeEnabled(false));
    }
}

/// True once the audio engine thread exists in this process.
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_ai_grokvoice_app_RustBridge_engineAlive(
    _env: *mut std::ffi::c_void,
    _class: *mut std::ffi::c_void,
) -> u8 {
    u8::from(audio::engine::control().is_some())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(target_os = "android")]
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Info)
            .with_tag("GrokVoice"),
    );
    let _ = rustls::crypto::ring::default_provider().install_default();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(native::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            app.manage(Arc::new(Hub::new(app.handle().clone())));
            app.manage(Arc::new(Player::new()));
            app.manage(SettingsStore::load(dir.join("settings.json")));
            app.manage(Vault::new(dir.join("vault")));
            app.manage(WakewordFiles::new(dir.join("wakeword")));
            app.manage(actions::Agenda::load(dir.join("agenda.json")));
            app.manage(memory::Memory::load(dir.join("memory.json")));
            let handle = app.handle().clone();
            std::thread::spawn(move || actions::reschedule(&handle));
            app.manage(Engine::spawn(app.handle().clone()));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::save_user_token,
            commands::clear_user_token,
            commands::auth_status,
            commands::add_voice_account,
            commands::remove_voice_account,
            commands::move_voice_account,
            commands::permissions_status,
            commands::request_permissions,
            commands::set_wake_enabled,
            commands::start_session,
            commands::end_session,
            commands::record_wake_sample,
            commands::train_wakeword,
            commands::wakeword_status,
            commands::delete_wakeword,
            commands::get_settings,
            commands::save_settings,
            commands::debug_snapshot,
            commands::debug_logs,
            commands::agenda_snapshot,
            commands::month_events,
            commands::save_note,
            commands::delete_note,
            commands::save_scheduled,
            commands::cancel_scheduled,
            commands::save_event,
            commands::delete_event,
            commands::request_calendar,
            commands::ask_text,
            commands::recent_memory,
            commands::clear_memory,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app, event| {
            // Swiping the app away destroys the activity. While hands-free is armed the
            // foreground service still owns the microphone, so the process has to stay up.
            if let tauri::RunEvent::ExitRequested { api, code, .. } = event {
                if code.is_none() && audio::engine::keep_alive() {
                    api.prevent_exit();
                }
            }
        });
}
