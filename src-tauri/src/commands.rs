use std::{sync::Arc, time::Duration};

use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use zeroize::Zeroizing;

use crate::{
    actions::{self, Agenda, AgendaSnapshot},
    memory::{Memory, Turn},
    people::{self, People, Person, PhoneContact},
    audio::engine::{Ctl, Engine},
    diagnostics::{self, DebugSnapshot, Hub, LogLine, now_ms},
    error::{Result, msg},
    native::{NativeBridge, PermissionStatus},
    realtime::{http_client, mint_ephemeral, mint_openai},
    settings::{Settings, SettingsStore},
    vault::{AuthStatus, Vault},
    wakeword::{SAMPLE_COUNT, SampleInfo, WakewordFiles, WakewordStatus},
};

async fn blocking<T, F>(app: AppHandle, f: F) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce(AppHandle) -> Result<T> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || f(app))
        .await
        .map_err(|e| msg(e.to_string()))?
}

fn vault_status(app: &AppHandle) -> Result<AuthStatus> {
    app.state::<Vault>().status(&app.state::<NativeBridge>())
}

async fn ensure_mic(app: &AppHandle) -> Result<()> {
    let status = blocking(app.clone(), |app| {
        let bridge = app.state::<NativeBridge>();
        let current = bridge.check_permissions()?;
        if current.microphone == "granted" {
            Ok(current)
        } else {
            bridge.request_permissions()
        }
    })
    .await?;
    if status.microphone != "granted" {
        return Err(msg("Microphone permission is required for voice. Enable it in Android Settings → Apps → Grok Voice."));
    }
    Ok(())
}

#[tauri::command]
pub async fn ask_text(app: AppHandle, text: String) -> Result<String> {
    ensure_key(&app).await?;
    blocking(app, move |app| crate::search::ask(&app, &text).map_err(msg)).await
}

async fn ensure_key(app: &AppHandle) -> Result<()> {
    let status = blocking(app.clone(), |app| vault_status(&app)).await?;
    if !status.has_key {
        return Err(msg("Add a voice key first."));
    }
    Ok(())
}

/// Verifies the key against xAI (by minting a realtime client secret), then commits it to
/// the Keystore-wrapped Stronghold vault. The raw key is dropped from memory afterwards.
#[tauri::command]
pub async fn save_user_token(app: AppHandle, hub: State<'_, Arc<Hub>>, token: String) -> Result<AuthStatus> {
    let token = Zeroizing::new(token.trim().to_string());
    if token.len() < 20 || token.contains(char::is_whitespace) {
        return Err(msg("That doesn't look like an xAI API key (expected something like xai-…)."));
    }
    let probe = mint_ephemeral(&http_client(), &token).await?;
    hub.update(|s| {
        s.token_minted_at_ms = Some(now_ms());
        s.token_expires_at = probe.expires_at;
    });
    hub.info("API key verified: xAI issued a realtime client secret");
    let status = blocking(app, move |app| {
        app.state::<Vault>().add_account(&app.state::<NativeBridge>(), "xai", &token)?;
        vault_status(&app)
    })
    .await?;
    hub.info(format!(
        "API key stored in Stronghold vault (vault key protection: {})",
        status.security_level.as_deref().unwrap_or("unknown")
    ));
    Ok(status)
}

#[tauri::command]
pub async fn clear_user_token(app: AppHandle, engine: State<'_, Engine>, hub: State<'_, Arc<Hub>>) -> Result<AuthStatus> {
    engine.send(Ctl::EndSession);
    engine.send(Ctl::SetWakeEnabled { enabled: false, reason: "signed out" });
    let status = blocking(app, |app| {
        app.state::<Vault>().delete_api_key(&app.state::<NativeBridge>())?;
        vault_status(&app)
    })
    .await?;
    hub.info("API key removed from device");
    Ok(status)
}

#[tauri::command]
pub async fn add_voice_account(app: AppHandle, provider: String, secret: String) -> Result<AuthStatus> {
    let provider = provider.trim().to_string();
    let secret = Zeroizing::new(secret.trim().to_string());
    if !matches!(provider.as_str(), "xai" | "openai" | "local") {
        return Err(msg("That provider is not available yet."));
    }
    if provider == "local" {
        let secret = Zeroizing::new("on-device".to_string());
        return blocking(app, move |app| {
            app.state::<Vault>().add_account(&app.state::<NativeBridge>(), "local", &secret)?;
            vault_status(&app)
        })
        .await;
    }
    if secret.len() < 12 || secret.contains(char::is_whitespace) {
        return Err(msg("That key looks too short."));
    }
    if provider == "xai" {
        mint_ephemeral(&http_client(), &secret).await?;
    } else if provider == "openai" {
        mint_openai(&http_client(), &secret).await?;
    }
    blocking(app, move |app| {
        app.state::<Vault>().add_account(&app.state::<NativeBridge>(), &provider, &secret)?;
        vault_status(&app)
    })
    .await
}

#[tauri::command]
pub async fn update_voice_account(
    app: AppHandle,
    id: String,
    provider: String,
    secret: Option<String>,
) -> Result<AuthStatus> {
    let provider = provider.trim().to_string();
    if !matches!(provider.as_str(), "xai" | "openai" | "local") {
        return Err(msg("That provider is not available yet."));
    }
    let replacement = secret.unwrap_or_default();
    let replacement = replacement.trim().to_string();
    if provider != "local" && !replacement.is_empty() {
        if replacement.len() < 12 || replacement.contains(char::is_whitespace) {
            return Err(msg("That key looks too short."));
        }
        if provider == "xai" {
            mint_ephemeral(&http_client(), &replacement).await?;
        } else {
            mint_openai(&http_client(), &replacement).await?;
        }
    }
    let secret = Zeroizing::new(if provider == "local" {
        "on-device".to_string()
    } else {
        replacement
    });
    blocking(app, move |app| {
        app.state::<Vault>().update_account(&app.state::<NativeBridge>(), &id, &provider, &secret)?;
        vault_status(&app)
    })
    .await
}

#[tauri::command]
pub async fn remove_voice_account(app: AppHandle, id: String) -> Result<AuthStatus> {
    blocking(app, move |app| {
        app.state::<Vault>().remove_account(&app.state::<NativeBridge>(), &id)?;
        vault_status(&app)
    })
    .await
}

#[tauri::command]
pub async fn reorder_voice_accounts(app: AppHandle, ids: Vec<String>) -> Result<AuthStatus> {
    blocking(app, move |app| {
        if !ids.is_empty() {
            app.state::<Vault>().reorder_accounts(&app.state::<NativeBridge>(), &ids)?;
        }
        vault_status(&app)
    })
    .await
}

#[tauri::command]
pub async fn move_voice_account(app: AppHandle, id: String, up: bool) -> Result<AuthStatus> {
    blocking(app, move |app| {
        app.state::<Vault>().move_account(&app.state::<NativeBridge>(), &id, up)?;
        vault_status(&app)
    })
    .await
}

#[tauri::command]
pub async fn auth_status(app: AppHandle) -> Result<AuthStatus> {
    blocking(app, |app| vault_status(&app)).await
}

#[tauri::command]
pub async fn permissions_status(app: AppHandle) -> Result<PermissionStatus> {
    blocking(app, |app| app.state::<NativeBridge>().check_permissions()).await
}

#[tauri::command]
pub async fn request_permissions(app: AppHandle) -> Result<PermissionStatus> {
    blocking(app, |app| app.state::<NativeBridge>().request_permissions()).await
}

#[tauri::command]
pub async fn set_wake_enabled(app: AppHandle, engine: State<'_, Engine>, enabled: bool) -> Result<bool> {
    if enabled {
        ensure_key(&app).await?;
        ensure_mic(&app).await?;
    }
    let mut settings = app.state::<SettingsStore>().get();
    settings.listen_armed = enabled;
    let _ = app.state::<SettingsStore>().save(settings);
    engine.send(Ctl::SetWakeEnabled {
        enabled,
        reason: if enabled { "user turned Listen on" } else { "user turned Listen off" },
    });
    Ok(enabled)
}

#[tauri::command]
pub async fn start_session(app: AppHandle, engine: State<'_, Engine>) -> Result<()> {
    ensure_key(&app).await?;
    ensure_mic(&app).await?;
    engine.send(Ctl::StartSession);
    Ok(())
}

#[tauri::command]
pub fn end_session(engine: State<'_, Engine>) {
    engine.send(Ctl::EndSession);
}

#[tauri::command]
pub async fn record_wake_sample(
    app: AppHandle,
    engine: State<'_, Engine>,
    index: usize,
    seconds: Option<f32>,
) -> Result<SampleInfo> {
    if index >= SAMPLE_COUNT {
        return Err(msg("invalid sample index"));
    }
    ensure_mic(&app).await?;
    let secs = seconds.unwrap_or(2.2);
    let (reply, rx) = std::sync::mpsc::channel();
    engine.send(Ctl::Record { secs, reply });
    let pcm = tauri::async_runtime::spawn_blocking(move || {
        rx.recv_timeout(Duration::from_secs_f32(secs + 6.0))
            .map_err(|_| msg("recording timed out — is the microphone available?"))
    })
    .await
    .map_err(|e| msg(e.to_string()))??
    .map_err(msg)?;
    blocking(app, move |app| app.state::<WakewordFiles>().save_sample(index, &pcm)).await
}

#[tauri::command]
pub async fn train_wakeword(app: AppHandle, engine: State<'_, Engine>, hub: State<'_, Arc<Hub>>) -> Result<WakewordStatus> {
    let status = blocking(app, |app| {
        let files = app.state::<WakewordFiles>();
        files.train()?;
        Ok(files.status())
    })
    .await?;
    engine.send(Ctl::ReloadWakeword);
    hub.info("“Hey Grok” enrolled from 3 on-device samples");
    Ok(status)
}

#[tauri::command]
pub fn wakeword_status(files: State<'_, WakewordFiles>) -> WakewordStatus {
    files.status()
}

#[tauri::command]
pub fn delete_wakeword(files: State<'_, WakewordFiles>, engine: State<'_, Engine>) -> Result<WakewordStatus> {
    files.delete()?;
    engine.send(Ctl::ReloadWakeword);
    Ok(files.status())
}

#[tauri::command]
pub fn get_settings(store: State<'_, SettingsStore>) -> Settings {
    store.get()
}

#[tauri::command]
pub fn save_settings(store: State<'_, SettingsStore>, engine: State<'_, Engine>, settings: Settings) -> Result<Settings> {
    let settings = crate::settings::keep_listen_preference(&store.get(), settings);
    let saved = store.save(settings)?;
    engine.send(Ctl::ReloadSettings);
    Ok(saved)
}

#[tauri::command]
pub async fn update_status(app: AppHandle) -> crate::updates::UpdateStatus {
    blocking(app, |app| Ok(crate::updates::sync_notice(&app)))
        .await
        .unwrap_or_else(|_| crate::updates::UpdateStatus::hidden(env!("CARGO_PKG_VERSION")))
}

#[tauri::command]
pub async fn dismiss_update(app: AppHandle) -> crate::updates::UpdateStatus {
    blocking(app, |app| Ok(crate::updates::dismiss(&app)))
        .await
        .unwrap_or_else(|_| crate::updates::UpdateStatus::hidden(env!("CARGO_PKG_VERSION")))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugInfo {
    #[serde(flatten)]
    snapshot: DebugSnapshot,
    vault_security_level: Option<String>,
    key_hint: Option<String>,
    platform: &'static str,
}

#[tauri::command]
pub async fn debug_snapshot(app: AppHandle) -> Result<DebugInfo> {
    #[cfg_attr(not(target_os = "android"), allow(unused_mut))]
    let mut snapshot = app.state::<Arc<Hub>>().snapshot();
    snapshot.listen_preference = app.state::<SettingsStore>().get().listen_armed;
    #[cfg(target_os = "android")]
    {
        let bridge = app.state::<NativeBridge>();
        if let Ok(status) = bridge.listener_status() {
            snapshot.foreground_service = status.service_running;
            snapshot.service_armed = status.armed;
            snapshot.activity_resumed = status.activity_resumed;
            snapshot.screen_interactive = status.screen_interactive;
            if status.mic_open {
                snapshot.mic_source = "audiorecord".into();
                if snapshot.mic_rate.is_none() {
                    snapshot.mic_rate = Some(16_000);
                }
            } else if snapshot.mic_source.is_empty() || snapshot.mic_source == "audiorecord" {
                snapshot.mic_source = if snapshot.mic_rate.is_some() { "cpal".into() } else { "closed".into() };
            }
            if !status.last_stop_reason.is_empty() {
                snapshot.last_stop_reason = Some(status.last_stop_reason);
            }
            snapshot.listener_history = status
                .history
                .lines()
                .filter(|line| !line.is_empty())
                .map(|line| line.to_string())
                .collect();
        }
        if let Ok(lines) = bridge.drain_listener_events() {
            let hub = app.state::<Arc<Hub>>();
            for line in lines {
                hub.info(line);
            }
        }
    }
    let auth = blocking(app.clone(), |app| vault_status(&app)).await.ok();
    Ok(DebugInfo {
        snapshot,
        vault_security_level: auth
            .as_ref()
            .and_then(|a| a.security_level.clone())
            .or_else(|| app.state::<Vault>().security_level()),
        key_hint: auth.and_then(|a| a.key_hint),
        platform: std::env::consts::OS,
    })
}

#[tauri::command]
pub fn debug_logs(hub: State<'_, Arc<Hub>>) -> Vec<LogLine> {
    hub.logs()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonthEvents {
    pub events: Vec<crate::actions::CalEvent>,
    pub calendar: String,
}

#[tauri::command]
pub fn agenda_snapshot(app: AppHandle) -> AgendaSnapshot {
    app.state::<Agenda>().snapshot(&app)
}

#[tauri::command]
pub fn month_events(app: AppHandle, start_ms: i64, end_ms: i64) -> MonthEvents {
    let (events, calendar) = app.state::<Agenda>().events_between(&app, start_ms, end_ms);
    MonthEvents { events, calendar }
}

#[tauri::command]
pub fn save_note(app: AppHandle, text: String, id: Option<String>, title: Option<String>) -> Result<AgendaSnapshot> {
    app.state::<Agenda>()
        .upsert_note(&app, id.as_deref(), title.as_deref().unwrap_or(""), &text)
        .map_err(msg)?;
    Ok(app.state::<Agenda>().snapshot(&app))
}

#[tauri::command]
pub fn delete_note(app: AppHandle, id: String) -> Result<AgendaSnapshot> {
    app.state::<Agenda>().delete_note(&app, &id).map_err(msg)?;
    Ok(app.state::<Agenda>().snapshot(&app))
}

#[tauri::command]
pub fn save_scheduled(
    app: AppHandle,
    title: String,
    at_ms: i64,
    note: Option<String>,
    kind: Option<String>,
    id: Option<i32>,
    recurrence: Option<String>,
    weekdays: Option<Vec<u8>>,
    time: Option<String>,
    date: Option<String>,
    lead_days: Option<u32>,
    until_ms: Option<i64>,
) -> Result<AgendaSnapshot> {
    let schedule = actions::ScheduleMeta {
        recurrence: recurrence.unwrap_or_default(),
        weekdays: weekdays.unwrap_or_default(),
        time: time.unwrap_or_default(),
        date: date.unwrap_or_default(),
        lead_days: lead_days.unwrap_or(0),
        until_ms,
        ..actions::ScheduleMeta::default()
    };
    let has_schedule = !schedule.recurrence.is_empty() || !schedule.weekdays.is_empty() || !schedule.time.is_empty() || !schedule.date.is_empty();
    app.state::<Agenda>()
        .upsert_item(
            &app,
            id,
            kind.as_deref().unwrap_or("reminder"),
            &title,
            note.as_deref().unwrap_or(""),
            at_ms,
            has_schedule.then_some(schedule),
        )
        .map_err(msg)?;
    Ok(app.state::<Agenda>().snapshot(&app))
}

#[tauri::command]
pub fn cancel_scheduled(app: AppHandle, id: i32) -> Result<AgendaSnapshot> {
    app.state::<Agenda>().cancel(&app, id).map_err(msg)?;
    Ok(app.state::<Agenda>().snapshot(&app))
}

#[tauri::command]
pub fn complete_scheduled(app: AppHandle, id: i32) -> Result<AgendaSnapshot> {
    app.state::<Agenda>().complete(&app, id).map_err(msg)?;
    Ok(app.state::<Agenda>().snapshot(&app))
}

#[tauri::command]
pub fn save_event(
    app: AppHandle,
    title: String,
    start_ms: i64,
    end_ms: i64,
    location: Option<String>,
    id: Option<String>,
    recurrence: Option<String>,
    weekdays: Option<Vec<u8>>,
    until_ms: Option<i64>,
) -> Result<AgendaSnapshot> {
    let location = location.unwrap_or_default();
    let recurrence = crate::recurrence::normalize_kind(recurrence.as_deref().unwrap_or(""));
    let recurrence = if recurrence == "none" { String::new() } else { recurrence };
    let weekdays = weekdays.unwrap_or_default();
    let until_ms = until_ms.unwrap_or(0).max(0);
    let rrule = crate::recurrence::to_rrule(&recurrence, &weekdays, until_ms);
    let agenda = app.state::<Agenda>();
    if let Some(id) = id.filter(|s| !s.is_empty()) {
        if id.starts_with("local-") || agenda.local_event(&id).is_some() {
            let mut event = agenda.local_event(&id).unwrap_or(actions::CalEvent {
                id: id.clone(),
                title: title.clone(),
                start_ms,
                end_ms,
                location: location.clone(),
                all_day: false,
                source: "local".into(),
                recurrence: String::new(),
                weekdays: Vec::new(),
                until_ms: 0,
                alarm_id: 0,
                rrule: String::new(),
            });
            event.title = title;
            event.start_ms = start_ms;
            event.end_ms = end_ms.max(start_ms + 60_000);
            event.location = location;
            event.recurrence = recurrence;
            event.weekdays = weekdays;
            event.until_ms = until_ms;
            agenda.upsert_local_event(&app, event).map_err(msg)?;
            return Ok(agenda.snapshot(&app));
        }
        let raw = app
            .state::<NativeBridge>()
            .update_event(&id, &title, start_ms, end_ms, &location, &rrule)
            .map_err(|e| msg(e.to_string()))?;
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap_or_default();
        if v["ok"] != true {
            return Err(msg(v["error"].as_str().unwrap_or("Could not update the event.")));
        }
        return Ok(agenda.snapshot(&app));
    }

    if app.state::<NativeBridge>().check_permissions().ok().is_some_and(|p| p.calendar == "granted") {
        let raw = app
            .state::<NativeBridge>()
            .create_event(&title, start_ms, end_ms, &location, &rrule)
            .map_err(|e| msg(e.to_string()))?;
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap_or_default();
        if v["ok"] == true {
            return Ok(agenda.snapshot(&app));
        }
    }

    let event = actions::CalEvent {
        id: format!("local-{}", diagnostics::now_ms()),
        title,
        start_ms,
        end_ms: end_ms.max(start_ms + 60_000),
        location,
        all_day: false,
        source: "local".into(),
        recurrence,
        weekdays,
        until_ms,
        alarm_id: 0,
        rrule: String::new(),
    };
    agenda.upsert_local_event(&app, event).map_err(msg)?;
    Ok(agenda.snapshot(&app))
}

#[tauri::command]
pub fn delete_event(app: AppHandle, id: String) -> Result<AgendaSnapshot> {
    let agenda = app.state::<Agenda>();
    if id.starts_with("local-") || agenda.local_event(&id).is_some() {
        agenda.delete_local_event(&app, &id).map_err(msg)?;
        return Ok(agenda.snapshot(&app));
    }
    let raw = app.state::<NativeBridge>().delete_event(&id).map_err(|e| msg(e.to_string()))?;
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap_or_default();
    if v["ok"] != true {
        return Err(msg(v["error"].as_str().unwrap_or("Could not delete the event.")));
    }
    Ok(agenda.snapshot(&app))
}

#[tauri::command]
pub async fn request_calendar(app: AppHandle) -> Result<PermissionStatus> {
    blocking(app, |app| app.state::<NativeBridge>().request_calendar()).await
}

#[tauri::command]
pub fn list_people(app: AppHandle) -> Vec<Person> {
    app.state::<People>().list()
}

#[tauri::command]
pub fn save_person(app: AppHandle, person: people::PersonDraft) -> Result<Person> {
    let saved = app.state::<People>().save(person).map_err(msg)?;
    actions::sync_person_reminders(&app).map_err(msg)?;
    Ok(saved)
}

#[tauri::command]
pub fn delete_person(app: AppHandle, id: String) -> Result<Vec<Person>> {
    app.state::<People>().delete(&id).map_err(msg)?;
    actions::sync_person_reminders(&app).map_err(msg)?;
    Ok(app.state::<People>().list())
}

#[tauri::command]
pub async fn request_contacts(app: AppHandle) -> Result<PermissionStatus> {
    blocking(app, |app| app.state::<NativeBridge>().request_contacts()).await
}

fn phone_payload(raw: &str) -> Result<serde_json::Value> {
    let value: serde_json::Value = serde_json::from_str(raw).map_err(|e| msg(e.to_string()))?;
    if value["ok"] == false {
        return Err(msg(value["error"].as_str().unwrap_or("Could not read phone contacts.")));
    }
    Ok(value)
}

#[tauri::command]
pub fn list_phone_contacts(app: AppHandle, query: Option<String>) -> Result<Vec<PhoneContact>> {
    let raw = app
        .state::<NativeBridge>()
        .list_phone_contacts(query.unwrap_or_default().trim())
        .map_err(|e| msg(e.to_string()))?;
    let value = phone_payload(&raw)?;
    serde_json::from_value(value["contacts"].clone()).map_err(|e| msg(e.to_string()))
}

#[tauri::command]
pub fn phone_contact(app: AppHandle, id: String) -> Result<PhoneContact> {
    let raw = app.state::<NativeBridge>().contact_handles(id.trim()).map_err(|e| msg(e.to_string()))?;
    let value = phone_payload(&raw)?;
    let rows: Vec<PhoneContact> = serde_json::from_value(value["matches"].clone()).unwrap_or_default();
    rows.into_iter().next().ok_or_else(|| msg("That phone contact is not on this device."))
}

#[tauri::command]
pub fn open_phone_contact(app: AppHandle, id: String) -> Result<()> {
    let raw = app.state::<NativeBridge>().open_phone_contact(id.trim()).map_err(|e| msg(e.to_string()))?;
    phone_payload(&raw)?;
    Ok(())
}

#[tauri::command]
pub fn usage_status(app: AppHandle) -> crate::usage::UsageStatus {
    let settings = app.state::<SettingsStore>().get();
    let caps = crate::usage::Caps {
        daily_minutes: settings.daily_budget_minutes,
        daily_hard_stop: settings.daily_budget_hard_stop,
        monthly_minutes: settings.monthly_budget_minutes,
        monthly_hard_stop: settings.monthly_budget_hard_stop,
    };
    app.state::<crate::usage::UsageStore>().status(caps, &settings.locale)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportedBackup {
    pub file_name: String,
    pub data_b64: String,
    pub shared: bool,
}

#[tauri::command]
pub fn export_backup(app: AppHandle, passphrase: String, include_keys: Option<bool>) -> Result<ExportedBackup> {
    let include_keys = include_keys.unwrap_or(false);
    let mut payload = crate::backup::Payload::notes_and_people(app.state::<Agenda>().notes(), app.state::<People>().list());
    payload.memory = Some(app.state::<crate::local_memory::LocalMemory>().snapshot());
    if include_keys {
        let secrets = app.state::<Vault>().accounts(&app.state::<NativeBridge>()).map_err(|e| msg(e.to_string()))?;
        payload.keys = secrets
            .into_iter()
            .map(|account| crate::backup::BackupKey { id: account.id, provider: account.provider, secret: account.secret })
            .collect();
    }
    let bytes = crate::backup::seal(&payload, &passphrase).map_err(msg)?;
    let dir = app.path().app_cache_dir().map_err(|e| msg(e.to_string()))?.join("backups");
    std::fs::create_dir_all(&dir).map_err(|e| msg(e.to_string()))?;
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("grok-voice-backup-") && name.ends_with(".gvb") {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
    let file_name = format!("grok-voice-backup-{}.gvb", chrono::Local::now().format("%Y-%m-%d"));
    let path = dir.join(&file_name);
    crate::files::write_private(&path, &bytes).map_err(|e| msg(e.to_string()))?;
    let shared = path.to_str().is_some_and(|path| {
        app.state::<NativeBridge>().share_file(path, "application/octet-stream", "Grok Voice backup").is_ok()
    });
    Ok(ExportedBackup {
        file_name,
        data_b64: if shared {
            String::new()
        } else {
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes)
        },
        shared,
    })
}

#[tauri::command]
pub fn preview_backup(data_b64: String, passphrase: String) -> Result<crate::backup::Preview> {
    crate::backup::open_b64(&data_b64, &passphrase).map(|payload| payload.preview()).map_err(msg)
}

#[tauri::command]
pub fn restore_backup(
    app: AppHandle,
    data_b64: String,
    passphrase: String,
    confirm: bool,
    restore_keys: Option<bool>,
) -> Result<crate::backup::Preview> {
    if !confirm {
        return Err(msg("Confirm before replacing notes and people on this phone."));
    }
    let payload = crate::backup::open_b64(&data_b64, &passphrase).map_err(msg)?;
    let preview = payload.preview();
    app.state::<People>().replace_all(payload.people).map_err(msg)?;
    actions::replace_notes(&app, payload.notes).map_err(msg)?;
    if let Some(memory) = payload.memory {
        app.state::<crate::local_memory::LocalMemory>().replace(memory);
    }
    actions::sync_person_reminders(&app).map_err(msg)?;
    if restore_keys.unwrap_or(false) && !payload.keys.is_empty() {
        let rows: Vec<(String, String, String)> = payload
            .keys
            .into_iter()
            .filter(|key| crate::backup::acceptable_key(key))
            .map(|key| (key.id, key.provider, key.secret))
            .collect();
        if rows.is_empty() {
            return Err(msg("That backup's voice keys could not be restored."));
        }
        app.state::<Vault>().replace_secrets(&app.state::<NativeBridge>(), &rows).map_err(|e| msg(e.to_string()))?;
    }
    Ok(preview)
}

#[tauri::command]
pub fn recent_memory(memory: State<'_, Memory>) -> Vec<Turn> {
    memory.recent()
}

#[tauri::command]
pub fn clear_memory(memory: State<'_, Memory>) {
    memory.clear();
}

#[tauri::command]
pub fn local_memory(memory: State<'_, crate::local_memory::LocalMemory>) -> crate::local_memory::Bundle {
    memory.snapshot()
}

#[tauri::command]
pub fn pin_local_fact(app: AppHandle, text: String, id: Option<String>) -> Result<crate::local_memory::Bundle> {
    let locale = app.state::<SettingsStore>().get().locale;
    app.state::<crate::local_memory::LocalMemory>()
        .remember(id.as_deref(), &text, &locale)
        .map_err(msg)?;
    Ok(app.state::<crate::local_memory::LocalMemory>().snapshot())
}

#[tauri::command]
pub fn forget_local_fact(app: AppHandle, id: Option<String>, text: Option<String>) -> Result<crate::local_memory::Bundle> {
    let locale = app.state::<SettingsStore>().get().locale;
    app.state::<crate::local_memory::LocalMemory>()
        .forget(id.as_deref(), text.as_deref(), &locale)
        .map_err(msg)?;
    Ok(app.state::<crate::local_memory::LocalMemory>().snapshot())
}

#[tauri::command]
pub fn wipe_local_memory(memory: State<'_, crate::local_memory::LocalMemory>) -> crate::local_memory::Bundle {
    memory.wipe()
}

#[tauri::command]
pub fn confirm_handoff(app: AppHandle) -> Result<serde_json::Value> {
    actions::confirm_pending(&app).map_err(msg)
}

#[tauri::command]
pub fn dismiss_handoff(app: AppHandle) -> Result<()> {
    if let Some(handoffs) = app.try_state::<actions::Handoffs>() {
        handoffs.clear();
    }
    app.state::<Arc<Hub>>().emit("voice://handoff", serde_json::Value::Null);
    Ok(())
}

#[tauri::command]
pub fn access_status(app: AppHandle) -> Result<serde_json::Value> {
    let raw = app.state::<NativeBridge>().access_status().map_err(|e| msg(e.to_string()))?;
    serde_json::from_str(&raw).map_err(|e| msg(e.to_string()))
}

#[tauri::command]
pub fn open_access_settings(app: AppHandle, kind: String) -> Result<serde_json::Value> {
    let raw = app.state::<NativeBridge>().open_access_settings(&kind).map_err(|e| msg(e.to_string()))?;
    serde_json::from_str(&raw).map_err(|e| msg(e.to_string()))
}

#[tauri::command]
pub fn request_history_access(app: AppHandle) -> Result<PermissionStatus> {
    app.state::<NativeBridge>().request_history_access().map_err(|e| msg(e.to_string()))
}

#[tauri::command]
pub fn refresh_last_contact(app: AppHandle, id: String) -> Result<crate::people::Person> {
    let people = app.state::<People>().list();
    let person = people.into_iter().find(|person| person.id == id).ok_or_else(|| msg("No person with that id."))?;
    let raw = app
        .state::<NativeBridge>()
        .last_contact(&person.id, &person.handles.phone, &person.display_name)
        .map_err(|e| msg(e.to_string()))?;
    let value: serde_json::Value = serde_json::from_str(&raw).map_err(|e| msg(e.to_string()))?;
    if value["ok"] == false {
        return Err(msg(value["error"].as_str().unwrap_or("Could not read call or text history.")));
    }
    let at_ms = value["atMs"].as_i64().ok_or_else(|| msg("No recent call or text for that person."))?;
    let saved = app.state::<People>().set_last_contacted(&id, at_ms).map_err(msg)?;
    actions::sync_person_reminders(&app).map_err(msg)?;
    Ok(saved)
}

#[tauri::command]
pub fn import_contact_birthday(app: AppHandle, id: String) -> Result<crate::people::Person> {
    let people = app.state::<People>().list();
    let person = people.into_iter().find(|person| person.id == id).ok_or_else(|| msg("No person with that id."))?;
    let contact = person.android_contact_id.clone().ok_or_else(|| msg("Link a phone contact first."))?;
    let raw = app.state::<NativeBridge>().contact_birthday(&contact).map_err(|e| msg(e.to_string()))?;
    let value: serde_json::Value = serde_json::from_str(&raw).map_err(|e| msg(e.to_string()))?;
    if value["ok"] == false {
        return Err(msg(value["error"].as_str().unwrap_or("Could not read that birthday.")));
    }
    let date = value["date"].as_str().unwrap_or("").trim();
    let saved = app.state::<People>().add_birthday_if_missing(&id, date).map_err(msg)?;
    actions::sync_person_reminders(&app).map_err(msg)?;
    Ok(saved)
}

#[tauri::command]
pub fn import_contact_dates(app: AppHandle, id: String) -> Result<crate::people::Person> {
    let people = app.state::<People>().list();
    let person = people.into_iter().find(|person| person.id == id).ok_or_else(|| msg("No person with that id."))?;
    let contact = person.android_contact_id.clone().ok_or_else(|| msg("Link a phone contact first."))?;
    let raw = app.state::<NativeBridge>().contact_dates(&contact).map_err(|e| msg(e.to_string()))?;
    let value: serde_json::Value = serde_json::from_str(&raw).map_err(|e| msg(e.to_string()))?;
    if value["ok"] == false {
        return Err(msg(value["error"].as_str().unwrap_or("Could not read dates on that contact.")));
    }
    let incoming = value["dates"].as_array().cloned().unwrap_or_default();
    let mut reminders = Vec::new();
    for row in incoming {
        reminders.push(crate::people::PersonReminder {
            id: String::new(),
            label: row["label"].as_str().unwrap_or("custom").into(),
            title: row["title"].as_str().unwrap_or("").into(),
            date: row["date"].as_str().unwrap_or("").into(),
            recurrence: "yearly".into(),
            lead_days: 0,
            time: "09:00".into(),
            enabled: true,
        });
    }
    let (saved, _added) = app.state::<People>().merge_imported_dates(&id, reminders).map_err(msg)?;
    actions::sync_person_reminders(&app).map_err(msg)?;
    Ok(saved)
}

#[tauri::command]
pub async fn local_pack_status(app: AppHandle) -> Result<Vec<crate::local::PackInfo>> {
    blocking(app, |app| {
        let dir = crate::local::model_dir(&app).map_err(msg)?;
        Ok(crate::local::pack_status(&dir))
    })
    .await
}

#[tauri::command]
pub async fn download_local_pack(app: AppHandle, id: String) -> Result<Vec<crate::local::PackInfo>> {
    let hub = app.state::<Arc<Hub>>().inner().clone();
    let dir = crate::local::model_dir(&app).map_err(msg)?;
    tokio::task::spawn_blocking(move || {
        crate::local::download_pack(&dir, &id, |progress| hub.emit("voice://pack", progress))
    })
    .await
    .map_err(|e| msg(e.to_string()))?
    .map_err(msg)?;
    local_pack_status(app).await
}

#[tauri::command]
pub async fn delete_local_pack(app: AppHandle, id: String) -> Result<Vec<crate::local::PackInfo>> {
    blocking(app, move |app| {
        let dir = crate::local::model_dir(&app).map_err(msg)?;
        crate::local::delete_pack(&dir, &id).map_err(msg)?;
        Ok(crate::local::pack_status(&dir))
    })
    .await
}

fn use_local_for_talk(app: &AppHandle) {
    let store = app.state::<crate::settings::SettingsStore>();
    let mut settings = store.get();
    if settings.prefer_local_on_wake {
        return;
    }
    settings.prefer_local_on_wake = true;
    let _ = store.save(settings);
}

#[tauri::command]
pub async fn select_local_model(app: AppHandle, id: String) -> Result<Vec<crate::local::PackInfo>> {
    blocking(app, move |app| {
        let dir = crate::local::model_dir(&app).map_err(msg)?;
        crate::local::select_model(&dir, &id).map_err(msg)?;
        use_local_for_talk(&app);
        Ok(crate::local::pack_status(&dir))
    })
    .await
}

#[tauri::command]
pub async fn import_local_model(app: AppHandle, name: String, family: String) -> Result<Vec<crate::local::PackInfo>> {
    blocking(app, move |app| {
        let dir = crate::local::model_dir(&app).map_err(msg)?;
        let incoming = dir.join("llm-custom").join("incoming.gguf");
        if let Some(parent) = incoming.parent() {
            std::fs::create_dir_all(parent).map_err(|err| msg(err.to_string()))?;
        }
        let dest = incoming.to_string_lossy().to_string();
        let picked = app.state::<crate::native::NativeBridge>().pick_gguf(&dest)?;
        crate::local::import_custom_file(&dir, std::path::Path::new(&picked), &name, &family).map_err(msg)?;
        use_local_for_talk(&app);
        Ok(crate::local::pack_status(&dir))
    })
    .await
}

#[tauri::command]
pub async fn download_custom_local_model(
    app: AppHandle,
    url: String,
    sha256: String,
    name: String,
    family: String,
) -> Result<Vec<crate::local::PackInfo>> {
    let hub = app.state::<Arc<Hub>>().inner().clone();
    let dir = crate::local::model_dir(&app).map_err(msg)?;
    tokio::task::spawn_blocking(move || {
        crate::local::download_custom_model(&dir, &url, &sha256, &name, &family, |progress| hub.emit("voice://pack", progress))
    })
    .await
    .map_err(|e| msg(e.to_string()))?
    .map_err(msg)?;
    use_local_for_talk(&app);
    local_pack_status(app).await
}
