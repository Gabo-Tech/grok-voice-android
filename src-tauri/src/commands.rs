use std::{sync::Arc, time::Duration};

use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use zeroize::Zeroizing;

use crate::{
    actions::{self, Agenda, AgendaSnapshot},
    audio::engine::{Ctl, Engine},
    diagnostics::{self, DebugSnapshot, Hub, LogLine, now_ms},
    error::{Result, msg},
    native::{NativeBridge, PermissionStatus},
    realtime::{http_client, mint_ephemeral},
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
        return Err(msg("Add your xAI API key first."));
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
    engine.send(Ctl::SetWakeEnabled(false));
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
    if secret.len() < 12 || secret.contains(char::is_whitespace) {
        return Err(msg("That key looks too short."));
    }
    if provider == "xai" {
        mint_ephemeral(&http_client(), &secret).await?;
    }
    blocking(app, move |app| {
        app.state::<Vault>().add_account(&app.state::<NativeBridge>(), &provider, &secret)?;
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
    engine.send(Ctl::SetWakeEnabled(enabled));
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
    let saved = store.save(settings)?;
    engine.send(Ctl::ReloadSettings);
    Ok(saved)
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
    let snapshot = app.state::<Arc<Hub>>().snapshot();
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
pub fn save_note(app: AppHandle, text: String, id: Option<String>) -> Result<AgendaSnapshot> {
    app.state::<Agenda>()
        .upsert_note(&app, id.as_deref(), &text)
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
) -> Result<AgendaSnapshot> {
    app.state::<Agenda>()
        .upsert_item(
            &app,
            id,
            kind.as_deref().unwrap_or("reminder"),
            &title,
            note.as_deref().unwrap_or(""),
            at_ms,
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
pub fn save_event(
    app: AppHandle,
    title: String,
    start_ms: i64,
    end_ms: i64,
    location: Option<String>,
    id: Option<String>,
) -> Result<AgendaSnapshot> {
    let location = location.unwrap_or_default();
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
            });
            event.title = title;
            event.start_ms = start_ms;
            event.end_ms = end_ms.max(start_ms + 60_000);
            event.location = location;
            agenda.upsert_local_event(&app, event).map_err(msg)?;
            return Ok(agenda.snapshot(&app));
        }
        let raw = app
            .state::<NativeBridge>()
            .update_event(&id, &title, start_ms, end_ms, &location)
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
            .create_event(&title, start_ms, end_ms, &location)
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
