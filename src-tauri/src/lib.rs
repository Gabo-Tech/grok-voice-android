mod actions;
#[cfg(target_os = "android")]
mod android_mic;
#[cfg(target_os = "android")]
mod android_notice;
mod audio;
mod backup;
mod commands;
mod diagnostics;
mod error;
mod files;
mod http;
mod i18n;
mod local;
mod local_memory;
mod memory;
mod native;
mod notices;
mod people;
mod realtime;
mod recurrence;
mod search;
mod settings;
mod updates;
mod usage;
mod vault;
mod voices;
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
        let _ = tx.send(audio::engine::Ctl::SetWakeEnabled {
            enabled: false,
            reason: "notification Stop",
        });
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
            let hub = Arc::new(Hub::new(app.handle().clone()));
            diagnostics::install(hub.clone());
            app.manage(hub);
            app.manage(Arc::new(Player::new()));
            app.manage(SettingsStore::load(dir.join("settings.json")));
            app.manage(updates::UpdateStore::load(dir.join("update-check.json")));
            app.manage(Vault::new(dir.join("vault")));
            app.manage(WakewordFiles::new(dir.join("wakeword")));
            app.manage(actions::Agenda::load(dir.join("agenda.json")));
            app.manage(people::People::load(dir.join("people.json")));
            app.manage(memory::Memory::load(dir.join("memory.json")));
            app.manage(local_memory::LocalMemory::load(
                dir.join("local-memory.json"),
            ));
            app.manage(actions::Handoffs::default());
            app.manage(usage::UsageStore::load(dir.join("usage.json")));
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                let _ = actions::sync_person_reminders(&handle);
                actions::reschedule(&handle);
            });
            app.manage(Engine::spawn(app.handle().clone()));
            let update_app = app.handle().clone();
            let _ = std::thread::Builder::new()
                .name("update-check".into())
                .spawn(move || updates::run(update_app));
            #[cfg(target_os = "android")]
            {
                let _ = ANDROID_APP.set(app.handle().clone());
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::save_user_token,
            commands::clear_user_token,
            commands::auth_status,
            commands::add_voice_account,
            commands::update_voice_account,
            commands::remove_voice_account,
            commands::reorder_voice_accounts,
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
            commands::listen_health,
            commands::request_battery_exemption,
            commands::get_settings,
            commands::save_settings,
            commands::update_status,
            commands::dismiss_update,
            commands::debug_snapshot,
            commands::debug_logs,
            commands::agenda_snapshot,
            commands::month_events,
            commands::save_note,
            commands::delete_note,
            commands::save_scheduled,
            commands::cancel_scheduled,
            commands::complete_scheduled,
            commands::save_event,
            commands::delete_event,
            commands::request_calendar,
            commands::list_people,
            commands::save_person,
            commands::delete_person,
            commands::request_contacts,
            commands::list_phone_contacts,
            commands::phone_contact,
            commands::open_phone_contact,
            commands::ask_text,
            commands::usage_status,
            commands::export_backup,
            commands::preview_backup,
            commands::restore_backup,
            commands::recent_memory,
            commands::clear_memory,
            commands::local_memory,
            commands::pin_local_fact,
            commands::forget_local_fact,
            commands::wipe_local_memory,
            commands::confirm_handoff,
            commands::dismiss_handoff,
            commands::access_status,
            commands::open_access_settings,
            commands::request_history_access,
            commands::refresh_last_contact,
            commands::import_contact_birthday,
            commands::import_contact_dates,
            commands::local_pack_status,
            commands::download_local_pack,
            commands::delete_local_pack,
            commands::select_local_model,
            commands::import_local_model,
            commands::download_custom_local_model,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            // Clearing the task destroys the activity. Tauri would then exit the process
            // and take the microphone service with it. Hands-free has to keep running.
            if let tauri::RunEvent::ExitRequested { api, .. } = event {
                let armed = app.state::<SettingsStore>().get().listen_armed;
                let hold = audio::engine::hold_listener_process(armed, audio::engine::keep_alive());
                if hold {
                    api.prevent_exit();
                }
                if let Some(hub) = app.try_state::<Arc<Hub>>() {
                    if hold {
                        hub.info("activity destroyed, wake listening continues");
                    } else {
                        hub.info("process exiting because Listen is off");
                    }
                }
            }
        });
}

#[cfg(target_os = "android")]
static ANDROID_APP: std::sync::OnceLock<tauri::AppHandle> = std::sync::OnceLock::new();

/// The activity was created again in a process that is already listening.
/// Rebuild the window if the previous one was destroyed with the task.
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_ai_grokvoice_app_RustBridge_ensureUi() {
    let Some(app) = ANDROID_APP.get() else {
        return;
    };
    let app = app.clone();
    let _ = app.clone().run_on_main_thread(move || {
        if !app.webview_windows().is_empty() {
            return;
        }
        match tauri::webview::WebviewWindowBuilder::new(
            &app,
            "main",
            tauri::WebviewUrl::App("index.html".into()),
        )
        .title("Grok Voice")
        .build()
        {
            Ok(_) => log::info!("reopened the window for the wake listener"),
            Err(err) => log::warn!("could not reopen the window: {err}"),
        }
    });
}
