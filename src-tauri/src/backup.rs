//! On-device backup of everything the person saved.
//!
//! Version 2 is the saved phone: settings, agenda, people, finance, pinned
//! facts, keys, wake word, model choice, alarm audio, and web preferences.
//! Talk history (`memory.json`) and voice-time counters (`usage.json`) stay
//! on the phone. An older file that still has them opens, and restore ignores
//! those sections.
//! Version 1 files still open. Sections they lack are left as they are.
//!
//! A passphrase seals the file with AES-256-GCM. The key is PBKDF2-HMAC-SHA256.
//! With no passphrase the file is marked plain and the keys sit in the clear.

use std::{
    collections::BTreeMap,
    fs,
    path::{Component, Path, PathBuf},
};

use aes_gcm::{
    Aes256Gcm, Key, Nonce,
    aead::{Aead, KeyInit},
};
use base64::{Engine, engine::general_purpose::STANDARD as B64};
use pbkdf2::pbkdf2_hmac;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::Sha256;
use tauri::{AppHandle, Manager};
use zeroize::Zeroize;

use crate::{
    actions::{Agenda, AgendaDump},
    people::Person,
    settings::Settings,
};

const MAGIC: &[u8] = b"GVB1";
const PLAIN: &[u8] = b"GVBU";
const ITERATIONS: u32 = 210_000;
const EMBED_MAX: u64 = 1024 * 1024;
const EMBED_TOTAL: u64 = 6 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BackupKey {
    pub id: String,
    pub provider: String,
    pub secret: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Blob {
    pub name: String,
    #[serde(default)]
    pub bytes_b64: String,
    #[serde(default)]
    pub byte_len: u64,
    #[serde(default)]
    pub referenced: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct PackRecord {
    #[serde(default)]
    pub selected: String,
    /// Pack ids that were on the phone. The weight files are not in the backup.
    #[serde(default)]
    pub installed: Vec<String>,
    #[serde(default)]
    pub custom_meta: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Payload {
    pub kind: String,
    pub version: u32,
    pub notes: Vec<crate::actions::Note>,
    pub people: Vec<Person>,
    #[serde(default)]
    pub keys: Vec<BackupKey>,
    /// Absent in backups written before on-device memory. Restore leaves the phone's memory alone then.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory: Option<crate::local_memory::Bundle>,
    /// Absent in backups written before Finance. Restore leaves the phone's money alone then.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finance: Option<crate::finance::Bundle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<Settings>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agenda: Option<AgendaDump>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub talk: Option<Vec<crate::memory::Turn>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<crate::usage::Usage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub web: Option<BTreeMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub packs: Option<PackRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wakeword: Option<Vec<Blob>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alarm_audio: Option<Vec<Blob>>,
    /// Store ids this file claims to cover. Empty on a version 1 file.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub covered: Vec<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub notes: usize,
    pub people: usize,
    pub reminders: usize,
    pub keys: usize,
    #[serde(default)]
    pub facts: usize,
    #[serde(default)]
    pub sessions: usize,
    #[serde(default)]
    pub has_memory: bool,
    #[serde(default)]
    pub accounts: usize,
    #[serde(default)]
    pub transactions: usize,
    #[serde(default)]
    pub rules: usize,
    #[serde(default)]
    pub has_finance: bool,
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub encrypted: bool,
    #[serde(default)]
    pub events: usize,
    #[serde(default)]
    pub alarms: usize,
    #[serde(default)]
    pub countdowns: usize,
    #[serde(default)]
    pub has_settings: bool,
    #[serde(default)]
    pub talk_turns: usize,
    #[serde(default)]
    pub has_talk: bool,
    #[serde(default)]
    pub has_usage: bool,
    #[serde(default)]
    pub has_agenda: bool,
    #[serde(default)]
    pub web_keys: usize,
    #[serde(default)]
    pub has_web: bool,
    #[serde(default)]
    pub has_packs: bool,
    #[serde(default)]
    pub wake_files: usize,
    #[serde(default)]
    pub alarm_files: usize,
    #[serde(default)]
    pub referenced: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreReport {
    #[serde(flatten)]
    pub preview: Preview,
    pub restored: Vec<String>,
    pub errors: Vec<String>,
    pub web: BTreeMap<String, String>,
    pub appearance: String,
    pub custom_theme: String,
    pub locale: String,
}

pub struct Opened {
    pub payload: Payload,
    pub encrypted: bool,
}

impl Payload {
    pub fn notes_and_people(notes: Vec<crate::actions::Note>, people: Vec<Person>) -> Self {
        Self {
            kind: "grok-voice-backup".into(),
            version: 1,
            notes,
            people,
            keys: Vec::new(),
            memory: None,
            finance: None,
            settings: None,
            agenda: None,
            talk: None,
            usage: None,
            web: None,
            packs: None,
            wakeword: None,
            alarm_audio: None,
            covered: Vec::new(),
        }
    }

    pub fn preview(&self) -> Preview {
        let items = self
            .agenda
            .as_ref()
            .map(|dump| dump.items.as_slice())
            .unwrap_or(&[]);
        Preview {
            notes: self.notes.len(),
            people: self.people.len(),
            reminders: self.people.iter().map(|p| p.reminders.len()).sum::<usize>()
                + items.iter().filter(|item| item.kind == "reminder").count(),
            keys: self.keys.len(),
            facts: self
                .memory
                .as_ref()
                .map(|bundle| bundle.facts.len())
                .unwrap_or(0),
            sessions: self
                .memory
                .as_ref()
                .map(|bundle| bundle.sessions.len())
                .unwrap_or(0),
            has_memory: self.memory.is_some(),
            accounts: self
                .finance
                .as_ref()
                .map(|bundle| bundle.accounts.len())
                .unwrap_or(0),
            transactions: self
                .finance
                .as_ref()
                .map(|bundle| bundle.transactions.len())
                .unwrap_or(0),
            rules: self
                .finance
                .as_ref()
                .map(|bundle| bundle.rules.len())
                .unwrap_or(0),
            has_finance: self.finance.is_some(),
            version: self.version,
            encrypted: false,
            events: self
                .agenda
                .as_ref()
                .map(|dump| dump.events.len())
                .unwrap_or(0),
            alarms: items.iter().filter(|item| item.kind == "alarm").count(),
            countdowns: items.iter().filter(|item| item.kind == "timer").count(),
            has_settings: self.settings.is_some(),
            talk_turns: self.talk.as_ref().map(|turns| turns.len()).unwrap_or(0),
            has_talk: self.talk.is_some(),
            has_usage: self.usage.is_some(),
            has_agenda: self.agenda.is_some(),
            web_keys: self.web.as_ref().map(|map| map.len()).unwrap_or(0),
            has_web: self.web.is_some(),
            has_packs: self.packs.is_some(),
            wake_files: self.wakeword.as_ref().map(|rows| rows.len()).unwrap_or(0),
            alarm_files: self
                .alarm_audio
                .as_ref()
                .map(|rows| rows.len())
                .unwrap_or(0),
            referenced: referenced_names(self),
        }
    }
}

pub fn referenced_names(payload: &Payload) -> Vec<String> {
    let mut names = Vec::new();
    for blob in payload
        .wakeword
        .iter()
        .flatten()
        .chain(payload.alarm_audio.iter().flatten())
    {
        if blob.referenced {
            names.push(format!(
                "{} ({} bytes, not included)",
                blob.name, blob.byte_len
            ));
        }
    }
    if let Some(packs) = &payload.packs {
        if !packs.installed.is_empty() {
            names.push(format!(
                "on-device models not included: {}",
                packs.installed.join(", ")
            ));
        }
    }
    names
}

pub fn seal(payload: &Payload, passphrase: Option<&str>) -> Result<Vec<u8>, String> {
    if payload.kind != "grok-voice-backup" || !matches!(payload.version, 1 | 2) {
        return Err("This backup is not a Grok Voice file.".into());
    }
    match passphrase.map(str::trim).filter(|pass| !pass.is_empty()) {
        Some(pass) => seal_encrypted(payload, pass),
        None => {
            let plain = serde_json::to_vec(payload).map_err(|e| e.to_string())?;
            let mut out = Vec::with_capacity(4 + plain.len());
            out.extend_from_slice(PLAIN);
            out.extend_from_slice(&plain);
            Ok(out)
        }
    }
}

fn seal_encrypted(payload: &Payload, passphrase: &str) -> Result<Vec<u8>, String> {
    check_passphrase(passphrase)?;
    let plain = serde_json::to_vec(payload).map_err(|e| e.to_string())?;
    let mut salt = [0u8; 16];
    let mut nonce_bytes = [0u8; 12];
    rand::rngs::OsRng.fill_bytes(&mut salt);
    rand::rngs::OsRng.fill_bytes(&mut nonce_bytes);
    let mut key_bytes = [0u8; 32];
    pbkdf2_hmac::<Sha256>(passphrase.as_bytes(), &salt, ITERATIONS, &mut key_bytes);
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key_bytes));
    let nonce = Nonce::from_slice(&nonce_bytes);
    let cipher_text = cipher
        .encrypt(nonce, plain.as_ref())
        .map_err(|_| "Could not encrypt the backup.".to_string())?;
    key_bytes.zeroize();
    let mut out = Vec::with_capacity(4 + 4 + 16 + 12 + cipher_text.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&ITERATIONS.to_le_bytes());
    out.extend_from_slice(&salt);
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&cipher_text);
    Ok(out)
}

pub fn open(bytes: &[u8], passphrase: Option<&str>) -> Result<Opened, String> {
    if bytes.len() >= 4 && &bytes[..4] == PLAIN {
        let payload = parse_payload(&bytes[4..])?;
        return Ok(Opened {
            payload,
            encrypted: false,
        });
    }
    let pass = passphrase.map(str::trim).filter(|pass| !pass.is_empty());
    let Some(pass) = pass else {
        return Err("This backup is encrypted. Type the passphrase.".into());
    };
    check_passphrase(pass)?;
    if bytes.len() < 4 + 4 + 16 + 12 + 16 || &bytes[..4] != MAGIC {
        return Err("That file is not a Grok Voice backup.".into());
    }
    let iterations = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
    if iterations < 100_000 || iterations > 1_000_000 {
        return Err("That backup uses an unsupported key strength.".into());
    }
    let salt = &bytes[8..24];
    let nonce_bytes = &bytes[24..36];
    let cipher_text = &bytes[36..];
    let mut key_bytes = [0u8; 32];
    pbkdf2_hmac::<Sha256>(pass.as_bytes(), salt, iterations, &mut key_bytes);
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key_bytes));
    let nonce = Nonce::from_slice(nonce_bytes);
    let plain = cipher.decrypt(nonce, cipher_text).map_err(|_| {
        key_bytes.zeroize();
        "That passphrase did not open this backup.".to_string()
    })?;
    key_bytes.zeroize();
    Ok(Opened {
        payload: parse_payload(&plain)?,
        encrypted: true,
    })
}

fn parse_payload(plain: &[u8]) -> Result<Payload, String> {
    let payload: Payload = serde_json::from_slice(plain)
        .map_err(|_| "The backup opened, but its contents were unreadable.".to_string())?;
    if payload.kind != "grok-voice-backup" || !matches!(payload.version, 1 | 2) {
        return Err("This backup is from a newer version.".into());
    }
    Ok(payload)
}

pub fn open_b64(encoded: &str, passphrase: Option<&str>) -> Result<Opened, String> {
    let bytes = B64
        .decode(encoded.trim())
        .map_err(|_| "The backup file was not readable.".to_string())?;
    open(&bytes, passphrase)
}

pub fn acceptable_key(key: &BackupKey) -> bool {
    let provider_ok = matches!(
        key.provider.as_str(),
        "xai" | "openai" | "local" | "huggingface" | "onemin"
    );
    let id_ok = !key.id.is_empty()
        && key.id.len() <= 80
        && key
            .id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    let secret = key.secret.trim();
    let secret_ok = !secret.is_empty()
        && secret.len() <= 512
        && !secret.chars().any(|c| c.is_whitespace() || c.is_control())
        && secret == key.secret;
    provider_ok && id_ok && secret_ok
}

fn check_passphrase(passphrase: &str) -> Result<(), String> {
    if passphrase.chars().count() < 8 {
        Err("Use a passphrase of at least 8 characters.".into())
    } else {
        Ok(())
    }
}

pub fn capture(
    app: &AppHandle,
    web_override: Option<BTreeMap<String, String>>,
) -> (Payload, Vec<String>) {
    let mut warnings = Vec::new();
    if let Some(web) = web_override {
        app.state::<crate::persist::WebPrefs>().replace(web);
    }
    let root = app
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."));
    let agenda = app.state::<Agenda>().dump();
    let notes = agenda.notes.clone();
    let mut keys = Vec::new();
    match app
        .state::<crate::vault::Vault>()
        .accounts(&app.state::<crate::native::NativeBridge>())
    {
        Ok(rows) => {
            keys.extend(rows.into_iter().map(|account| BackupKey {
                id: account.id,
                provider: account.provider,
                secret: account.secret,
            }));
        }
        Err(err) => warnings.push(format!("API keys could not be read. {err}")),
    }
    match app
        .state::<crate::vault::Vault>()
        .hf_token(&app.state::<crate::native::NativeBridge>())
    {
        Ok(Some(token)) => keys.push(BackupKey {
            id: "hf-token".into(),
            provider: "huggingface".into(),
            secret: token.to_string(),
        }),
        Ok(None) => {}
        Err(err) => warnings.push(format!("The Hugging Face token could not be read. {err}")),
    }
    let payload = Payload {
        kind: "grok-voice-backup".into(),
        version: 2,
        notes,
        people: app.state::<crate::people::People>().list(),
        keys,
        memory: Some(pinned_facts(
            app.state::<crate::local_memory::LocalMemory>().snapshot(),
        )),
        finance: Some(app.state::<crate::finance::Finance>().bundle()),
        settings: Some(app.state::<crate::settings::SettingsStore>().get()),
        agenda: Some(agenda),
        talk: None,
        usage: None,
        web: Some(app.state::<crate::persist::WebPrefs>().get()),
        packs: Some(read_packs(&root.join("local-models"))),
        wakeword: Some(read_blobs(&root.join("wakeword"), "wakeword")),
        alarm_audio: Some(read_blobs(
            &root.join(crate::alarm_sound::DIR),
            crate::alarm_sound::DIR,
        )),
        covered: crate::persist::STORES
            .iter()
            .map(|id| (*id).to_string())
            .collect(),
    };
    (payload, warnings)
}

/// Drops Talk history, voice-time counters, and local session notes.
/// Pinned facts, keys, and the other saved stores stay.
pub fn without_history(payload: &Payload) -> Payload {
    let mut next = payload.clone();
    next.talk = None;
    next.usage = None;
    if let Some(memory) = next.memory.as_mut() {
        memory.sessions.clear();
    }
    next
}

fn pinned_facts(mut bundle: crate::local_memory::Bundle) -> crate::local_memory::Bundle {
    bundle.sessions.clear();
    bundle
}

pub fn restore_app(app: &AppHandle, payload: &Payload) -> RestoreReport {
    let payload = without_history(payload);
    let payload = &payload;
    let mut restored = Vec::new();
    let mut errors = Vec::new();
    let mut appearance = String::new();
    let mut custom_theme = String::new();
    let mut locale = String::new();
    let full = payload.version >= 2;

    if let Some(settings) = payload.settings.clone() {
        appearance = settings.appearance.clone();
        custom_theme = settings.custom_theme.clone();
        locale = settings.locale.clone();
        match app.state::<crate::settings::SettingsStore>().save(settings) {
            Ok(saved) => {
                appearance = saved.appearance.clone();
                custom_theme = saved.custom_theme.clone();
                locale = saved.locale.clone();
                crate::actions::publish_settings(app, &saved, Some(saved.listen_armed));
                restored.push("settings.json".into());
            }
            Err(err) => errors.push(format!("settings.json: {err}")),
        }
    }

    if let Some(web) = payload.web.clone() {
        app.state::<crate::persist::WebPrefs>().replace(web);
        restored.push("web-prefs.json".into());
    }

    if let Err(err) = app
        .state::<crate::people::People>()
        .replace_all(payload.people.clone())
    {
        errors.push(format!("people.json: {err}"));
    } else {
        restored.push("people.json".into());
    }

    if let Some(dump) = payload.agenda.clone() {
        match app.state::<Agenda>().replace_dump(app, dump) {
            Ok(()) => restored.push("agenda.json".into()),
            Err(err) => errors.push(format!("agenda.json: {err}")),
        }
    } else if let Err(err) = crate::actions::replace_notes(app, payload.notes.clone()) {
        errors.push(format!("agenda.json: {err}"));
    } else {
        restored.push("agenda.json".into());
    }

    if let Some(bundle) = payload.finance.clone() {
        match app.state::<crate::finance::Finance>().replace(app, bundle) {
            Ok(()) => restored.push("finance.json".into()),
            Err(err) => errors.push(format!("finance.json: {err}")),
        }
    }

    if let Some(memory) = payload.memory.clone() {
        app.state::<crate::local_memory::LocalMemory>()
            .replace(pinned_facts(memory));
        restored.push("local-memory.json".into());
    }

    if let Ok(root) = app.path().app_data_dir() {
        if let Some(packs) = &payload.packs {
            if let Err(err) = write_packs(&root.join("local-models"), packs) {
                errors.push(format!("local-models: {err}"));
            } else {
                restored.push("local-models".into());
            }
        }
        if let Some(blobs) = &payload.wakeword {
            match write_blobs(&root, blobs) {
                Ok(missed) => {
                    restored.push("wakeword".into());
                    for name in missed {
                        errors.push(format!(
                            "{name} was not in the file. Record Hey Grok again."
                        ));
                    }
                }
                Err(err) => errors.push(format!("wakeword: {err}")),
            }
        }
        if let Some(blobs) = &payload.alarm_audio {
            match write_blobs(&root, blobs) {
                Ok(missed) => {
                    restored.push("alarm-sounds".into());
                    for name in missed {
                        errors.push(format!(
                            "{name} was too large to include. Pick that alarm sound again."
                        ));
                    }
                }
                Err(err) => errors.push(format!("alarm-sounds: {err}")),
            }
        }
    }

    if full || !payload.keys.is_empty() {
        match restore_keys(app, payload) {
            Ok(None) => restored.push("vault".into()),
            Ok(Some(warning)) => {
                restored.push("vault".into());
                errors.push(format!("vault: {warning}"));
            }
            Err(err) => errors.push(format!("vault: {err}")),
        }
    }

    if let Err(err) = crate::actions::sync_person_reminders(app) {
        errors.push(format!("reminders: {err}"));
    }
    crate::actions::reschedule(app);
    app.state::<crate::native::NativeBridge>().refresh_widgets();

    let preview = payload.preview();
    let report = RestoreReport {
        preview,
        restored,
        errors,
        web: payload.web.clone().unwrap_or_default(),
        appearance,
        custom_theme,
        locale,
    };
    if let Some(hub) = app.try_state::<std::sync::Arc<crate::diagnostics::Hub>>() {
        hub.emit("voice://restored", &report);
    }
    report
}

fn restore_keys(app: &AppHandle, payload: &Payload) -> Result<Option<String>, String> {
    let bridge = app.state::<crate::native::NativeBridge>();
    let vault = app.state::<crate::vault::Vault>();
    let mut rejected = Vec::new();
    let mut hf_token = None;
    let mut rows = Vec::new();
    for key in &payload.keys {
        if !acceptable_key(key) {
            rejected.push(key.id.clone());
            continue;
        }
        if key.provider == "huggingface" {
            hf_token = Some(key.secret.clone());
        } else {
            rows.push((key.id.clone(), key.provider.clone(), key.secret.clone()));
        }
    }
    vault
        .replace_secrets(&bridge, &rows)
        .map_err(|err| err.to_string())?;
    match hf_token {
        Some(token) => vault
            .set_hf_token(&bridge, &token)
            .map_err(|err| err.to_string())?,
        None if payload.version >= 2 => {
            let _ = vault.delete_hf_token(&bridge);
        }
        None => {}
    }
    if rejected.is_empty() {
        Ok(None)
    } else {
        Ok(Some(format!(
            "These keys were skipped: {}.",
            rejected.join(", ")
        )))
    }
}

pub fn voice_export(app: &AppHandle) -> Result<serde_json::Value, String> {
    let (payload, mut warnings) = capture(app, None);
    let bytes = seal(&payload, None)?;
    let path = write_backup_file(app, &bytes)?;
    let shared = path.to_str().is_some_and(|path| {
        app.state::<crate::native::NativeBridge>()
            .share_file(path, "application/octet-stream", "Grok Voice backup")
            .is_ok()
    });
    let locale = app.state::<crate::settings::SettingsStore>().get().locale;
    let mut detail = crate::i18n::line(&locale, "backup-saved");
    if !shared {
        warnings.push("The share sheet did not open. The file is still on this phone.".into());
    }
    if !warnings.is_empty() {
        detail.push(' ');
        detail.push_str(&warnings.join(" "));
    }
    Ok(
        json!({ "ok": true, "detail": detail, "file": path.file_name().and_then(|n| n.to_str()).unwrap_or("") }),
    )
}

pub fn voice_restore(app: &AppHandle, confirmed: bool) -> Result<serde_json::Value, String> {
    let locale = app.state::<crate::settings::SettingsStore>().get().locale;
    let Some(path) = latest_backup(app) else {
        return Ok(json!({ "ok": false, "detail": crate::i18n::line(&locale, "backup-none") }));
    };
    let bytes = fs::read(&path).map_err(|err| err.to_string())?;
    if bytes.len() >= 4 && &bytes[..4] == MAGIC {
        return Ok(
            json!({ "ok": false, "detail": crate::i18n::line(&locale, "backup-encrypted") }),
        );
    }
    let when = path
        .file_stem()
        .and_then(|name| name.to_str())
        .and_then(|name| name.rsplit('-').next())
        .unwrap_or("this phone");
    if !confirmed {
        let ask = crate::i18n::fill(&locale, "backup-ask", &[("when", when)]);
        return Ok(json!({ "ok": true, "needsConfirm": true, "changed": false, "detail": ask }));
    }
    let opened = open(&bytes, None)?;
    let report = restore_app(app, &opened.payload);
    let summary = format!(
        "{} notes, {} people, {} events, {} keys, {} pinned facts.",
        report.preview.notes,
        report.preview.people,
        report.preview.events,
        report.preview.keys,
        report.preview.facts
    );
    let mut detail = crate::i18n::fill(&locale, "backup-restored", &[("summary", &summary)]);
    detail.push(' ');
    detail.push_str(&crate::i18n::line(&locale, "backup-history-kept"));
    if !report.errors.is_empty() {
        detail.push(' ');
        detail.push_str(&crate::i18n::fill(
            &locale,
            "backup-failed",
            &[("error", &report.errors.join(" "))],
        ));
    }
    if !report.preview.referenced.is_empty() {
        detail.push(' ');
        detail.push_str(&report.preview.referenced.join(" "));
    }
    Ok(json!({ "ok": report.errors.is_empty(), "changed": true, "detail": detail }))
}

pub fn write_backup_file(app: &AppHandle, bytes: &[u8]) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_cache_dir()
        .map_err(|err| err.to_string())?
        .join("backups");
    fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("grok-voice-backup-") && name.ends_with(".gvb") {
                let _ = fs::remove_file(entry.path());
            }
        }
    }
    let path = dir.join(format!(
        "grok-voice-backup-{}.gvb",
        chrono::Local::now().format("%Y-%m-%d")
    ));
    crate::files::write_private(&path, bytes).map_err(|err| err.to_string())?;
    Ok(path)
}

fn latest_backup(app: &AppHandle) -> Option<PathBuf> {
    let dir = app.path().app_cache_dir().ok()?.join("backups");
    let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
    for entry in fs::read_dir(dir).ok()?.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !(name.starts_with("grok-voice-backup-") && name.ends_with(".gvb")) {
            continue;
        }
        let modified = entry.metadata().and_then(|meta| meta.modified()).ok();
        let stamp = modified.unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        if best.as_ref().is_none_or(|(at, _)| stamp >= *at) {
            best = Some((stamp, entry.path()));
        }
    }
    best.map(|(_, path)| path)
}

fn read_packs(root: &Path) -> PackRecord {
    let selected = fs::read_to_string(root.join("selected-llm.txt")).unwrap_or_default();
    let selected = selected.trim().chars().take(80).collect();
    let mut installed = Vec::new();
    if let Ok(entries) = fs::read_dir(root) {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                let name = entry.file_name().to_string_lossy().to_string();
                if !name.starts_with('.') && name.len() <= 80 {
                    installed.push(name);
                }
            }
        }
    }
    installed.sort();
    let custom_meta =
        fs::read_to_string(root.join("llm-custom").join("meta.json")).unwrap_or_default();
    let custom_meta = if custom_meta.len() > 64_000 {
        String::new()
    } else {
        custom_meta
    };
    PackRecord {
        selected,
        installed,
        custom_meta,
    }
}

fn write_packs(root: &Path, packs: &PackRecord) -> Result<(), String> {
    fs::create_dir_all(root).map_err(|err| err.to_string())?;
    let selected: String = packs
        .selected
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-' || *ch == '_')
        .take(80)
        .collect();
    crate::files::write_private(&root.join("selected-llm.txt"), selected.as_bytes())
        .map_err(|err| err.to_string())?;
    if !packs.custom_meta.is_empty() && packs.custom_meta.len() <= 64_000 {
        let dir = root.join("llm-custom");
        fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
        crate::files::write_private(&dir.join("meta.json"), packs.custom_meta.as_bytes())
            .map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn read_blobs(dir: &Path, prefix: &str) -> Vec<Blob> {
    let mut out = Vec::new();
    let mut used = 0u64;
    let mut files = Vec::new();
    collect_files(dir, &mut files);
    files.sort();
    for path in files {
        let Ok(rel) = path.strip_prefix(dir) else {
            continue;
        };
        let name = format!("{prefix}/{}", rel.to_string_lossy().replace('\\', "/"));
        let len = fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
        if len > EMBED_MAX || used.saturating_add(len) > EMBED_TOTAL {
            out.push(Blob {
                name,
                bytes_b64: String::new(),
                byte_len: len,
                referenced: true,
            });
            continue;
        }
        match fs::read(&path) {
            Ok(bytes) => {
                used += len;
                out.push(Blob {
                    name,
                    bytes_b64: B64.encode(bytes),
                    byte_len: len,
                    referenced: false,
                });
            }
            Err(_) => out.push(Blob {
                name,
                bytes_b64: String::new(),
                byte_len: len,
                referenced: true,
            }),
        }
    }
    out
}

fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, out);
        } else if path.is_file() {
            out.push(path);
        }
    }
}

fn write_blobs(root: &Path, blobs: &[Blob]) -> Result<Vec<String>, String> {
    let mut missed = Vec::new();
    for blob in blobs {
        if blob.referenced || blob.bytes_b64.is_empty() {
            missed.push(blob.name.clone());
            continue;
        }
        let Some(rel) = safe_rel(&blob.name) else {
            return Err(format!("{} is not a safe file name.", blob.name));
        };
        let bytes = B64
            .decode(blob.bytes_b64.trim())
            .map_err(|_| format!("{} could not be read.", blob.name))?;
        if bytes.len() as u64 > EMBED_MAX {
            missed.push(blob.name.clone());
            continue;
        }
        crate::files::write_private(&root.join(rel), &bytes).map_err(|err| err.to_string())?;
    }
    Ok(missed)
}

fn safe_rel(name: &str) -> Option<PathBuf> {
    if name.is_empty() || name.len() > 180 || name.contains('\\') {
        return None;
    }
    let path = Path::new(name);
    if path.is_absolute() {
        return None;
    }
    if path
        .components()
        .any(|part| !matches!(part, Component::Normal(_)))
    {
        return None;
    }
    Some(path.to_path_buf())
}

/// Writes the file-backed sections. Vault secrets stay in the sealed payload only.
pub fn write_tree(root: &Path, payload: &Payload) -> Result<(), String> {
    if let Some(settings) = &payload.settings {
        write_json(&root.join("settings.json"), settings)?;
    }
    if let Some(agenda) = &payload.agenda {
        write_json(&root.join("agenda.json"), agenda)?;
    }
    write_json(
        &root.join("people.json"),
        &json!({ "people": payload.people }),
    )?;
    if let Some(finance) = &payload.finance {
        write_json(&root.join("finance.json"), finance)?;
    }
    if let Some(talk) = &payload.talk {
        write_json(&root.join("memory.json"), talk)?;
    }
    if let Some(memory) = &payload.memory {
        write_json(&root.join("local-memory.json"), memory)?;
    }
    if let Some(usage) = &payload.usage {
        write_json(&root.join("usage.json"), usage)?;
    }
    if let Some(web) = &payload.web {
        write_json(&root.join("web-prefs.json"), web)?;
    }
    if let Some(packs) = &payload.packs {
        write_packs(&root.join("local-models"), packs)?;
    }
    if let Some(blobs) = &payload.wakeword {
        write_blobs(root, blobs)?;
    }
    if let Some(blobs) = &payload.alarm_audio {
        write_blobs(root, blobs)?;
    }
    Ok(())
}

pub fn read_tree(root: &Path) -> Result<Payload, String> {
    let agenda: Option<AgendaDump> = read_json(&root.join("agenda.json"))?;
    let people = read_json::<serde_json::Value>(&root.join("people.json"))?
        .and_then(|value| serde_json::from_value::<Vec<Person>>(value["people"].clone()).ok())
        .unwrap_or_default();
    Ok(Payload {
        kind: "grok-voice-backup".into(),
        version: 2,
        notes: agenda
            .as_ref()
            .map(|dump| dump.notes.clone())
            .unwrap_or_default(),
        people,
        keys: Vec::new(),
        memory: read_json(&root.join("local-memory.json"))?,
        finance: read_json(&root.join("finance.json"))?,
        settings: read_json(&root.join("settings.json"))?,
        agenda,
        talk: read_json(&root.join("memory.json"))?,
        usage: read_json(&root.join("usage.json"))?,
        web: read_json(&root.join("web-prefs.json"))?,
        packs: Some(read_packs(&root.join("local-models"))),
        wakeword: Some(read_blobs(&root.join("wakeword"), "wakeword")),
        alarm_audio: Some(read_blobs(
            &root.join(crate::alarm_sound::DIR),
            crate::alarm_sound::DIR,
        )),
        covered: crate::persist::STORES
            .iter()
            .map(|id| (*id).to_string())
            .collect(),
    })
}

pub fn wipe_tree(root: &Path) {
    for name in [
        "settings.json",
        "agenda.json",
        "people.json",
        "finance.json",
        "memory.json",
        "local-memory.json",
        "usage.json",
        "web-prefs.json",
    ] {
        let _ = fs::remove_file(root.join(name));
    }
    for name in ["wakeword", "local-models", crate::alarm_sound::DIR] {
        let _ = fs::remove_dir_all(root.join(name));
    }
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|err| err.to_string())?;
    crate::files::write_private(path, &bytes).map_err(|err| err.to_string())
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<Option<T>, String> {
    match fs::read(path) {
        Ok(bytes) if bytes.is_empty() => Ok(None),
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|err| err.to_string()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::people::{Handles, Person, PersonReminder};
    use std::collections::BTreeMap;

    fn sample() -> Payload {
        let note = crate::actions::Note {
            id: "n1".into(),
            title: "Milk".into(),
            text: "buy milk".into(),
            created_ms: 5,
        };
        let person = Person {
            id: "p1".into(),
            android_contact_id: None,
            display_name: "Sam Rivera".into(),
            nickname: "Mom".into(),
            aliases: vec!["mummy".into()],
            preferred_channel: "whatsapp".into(),
            handles: Handles::default(),
            favorite: true,
            note: "call after 5".into(),
            reach_out_days: 0,
            last_contacted_ms: None,
            reminders: vec![PersonReminder {
                id: "r1".into(),
                label: "birthday".into(),
                title: String::new(),
                date: "03-03".into(),
                recurrence: "yearly".into(),
                lead_days: 1,
                time: "09:00".into(),
                enabled: true,
            }],
        };
        Payload::notes_and_people(vec![note], vec![person])
    }

    fn sample_full() -> Payload {
        let mut payload = sample();
        payload.version = 2;
        payload.covered = crate::persist::STORES
            .iter()
            .map(|id| (*id).to_string())
            .collect();
        payload.keys = vec![
            BackupKey {
                id: "xai-1".into(),
                provider: "xai".into(),
                secret: "xai-round-trip-secret".into(),
            },
            BackupKey {
                id: "openai-1".into(),
                provider: "openai".into(),
                secret: "sk-round-trip-secret".into(),
            },
            BackupKey {
                id: "onemin-1".into(),
                provider: "onemin".into(),
                secret: "onemin-round-trip-secret".into(),
            },
            BackupKey {
                id: "local-1".into(),
                provider: "local".into(),
                secret: "on-device".into(),
            },
            BackupKey {
                id: "hf-token".into(),
                provider: "huggingface".into(),
                secret: "hf_round_trip_token".into(),
            },
        ];
        payload.memory = Some(crate::local_memory::Bundle {
            facts: vec![crate::local_memory::Fact {
                id: "f1".into(),
                text: "I live in Zurich".into(),
                updated_ms: 1,
            }],
            sessions: Vec::new(),
            next_id: 2,
        });
        payload.finance = Some(serde_json::from_str(
            r#"{"accounts":[{"id":"cash1","name":"Wallet","kind":"cash","currency":"EUR","balanceMinor":4200}],"transactions":[{"id":"tx1","amountMinor":500,"currency":"EUR","direction":"expense","date":"2099-03-01","createdMs":1}],"rules":[{"id":"rule1","name":"Rent","direction":"expense","cadence":"monthly","currency":"EUR"}],"defaultCurrency":"EUR","budgets":[{"id":"plan1","period":"2099-03","currency":"EUR"}],"fx":{"manual":[{"base":"EUR","quote":"USD","rate":1.08,"asOf":"2099-03-01","overrideLive":true}]},"nextId":40}"#,
        ).unwrap());
        let mut settings = Settings::default();
        settings.locale = "de".into();
        settings.appearance = "sand".into();
        settings.custom_theme = "#112233,#223344,#334455,#445566,#556677,#667788,#778899".into();
        settings.voice = "eve".into();
        settings.daily_budget_minutes = 15;
        settings.daily_budget_hard_stop = true;
        settings.monthly_budget_minutes = 120;
        settings.onboarded = true;
        settings.setup_step = "done".into();
        settings.music_player = "local".into();
        settings.listen_armed = true;
        payload.settings = Some(settings);
        let reminder: crate::actions::Item = serde_json::from_str(
            r#"{"id":1,"kind":"reminder","title":"Call","note":"","atMs":10}"#,
        )
        .unwrap();
        let alarm: crate::actions::Item = serde_json::from_str(
            r#"{"id":2,"kind":"alarm","title":"Wake","note":"","atMs":20,"soundKind":"file","soundRef":"alarm-sounds/chime.mp3","soundLabel":"chime.mp3"}"#,
        )
        .unwrap();
        let timer: crate::actions::Item =
            serde_json::from_str(r#"{"id":3,"kind":"timer","title":"Eggs","note":"","atMs":30}"#)
                .unwrap();
        let event: crate::actions::CalEvent = serde_json::from_str(
            r#"{"id":"ev1","title":"Dentist","startMs":40,"endMs":50,"source":"app"}"#,
        )
        .unwrap();
        payload.agenda = Some(AgendaDump {
            notes: payload.notes.clone(),
            items: vec![reminder, alarm, timer],
            events: vec![event],
            next_id: 8,
            recent: None,
        });
        let mut web = BTreeMap::new();
        for key in crate::persist::WEB_KEYS {
            web.insert((*key).to_string(), format!("value-{key}"));
        }
        payload.web = Some(web);
        payload.packs = Some(PackRecord {
            selected: "llm-custom".into(),
            installed: vec!["llm-custom".into(), "llm-gemma".into()],
            custom_meta: r#"{"name":"Mine"}"#.into(),
        });
        payload.wakeword = Some(vec![Blob {
            name: "wakeword/hey_grok.rpw".into(),
            bytes_b64: B64.encode(b"rpw-sample-bytes"),
            byte_len: 16,
            referenced: false,
        }]);
        payload.alarm_audio = Some(vec![
            Blob {
                name: "alarm-sounds/chime.mp3".into(),
                bytes_b64: B64.encode(b"tiny-audio"),
                byte_len: 10,
                referenced: false,
            },
            Blob {
                name: "alarm-sounds/huge.wav".into(),
                bytes_b64: String::new(),
                byte_len: 9_000_000,
                referenced: true,
            },
        ]);
        payload
    }

    fn marker_ok(payload: &Payload, id: &str) -> bool {
        match id {
            "settings.json" => payload
                .settings
                .as_ref()
                .is_some_and(|row| row.locale == "de" && row.appearance == "sand"),
            "agenda.json" => payload
                .agenda
                .as_ref()
                .is_some_and(|row| row.items.len() == 3 && row.events.len() == 1),
            "people.json" => payload.people.iter().any(|person| person.nickname == "Mom"),
            "finance.json" => payload.finance.as_ref().is_some_and(|row| {
                row.default_currency == "EUR"
                    && row.accounts.len() == 1
                    && row.rules.len() == 1
                    && !row.fx.manual.is_empty()
                    && !row.budgets.is_empty()
            }),
            "local-memory.json" => payload.memory.as_ref().is_some_and(|row| {
                row.facts.iter().any(|fact| fact.text.contains("Zurich")) && row.sessions.is_empty()
            }),
            "vault" => {
                payload.keys.iter().any(|key| key.provider == "xai")
                    && payload.keys.iter().any(|key| key.provider == "openai")
                    && payload.keys.iter().any(|key| key.provider == "onemin")
                    && payload.keys.iter().any(|key| key.provider == "huggingface")
            }
            "wakeword" => payload
                .wakeword
                .as_ref()
                .is_some_and(|rows| rows.iter().any(|blob| blob.name.contains("hey_grok"))),
            "local-models" => payload
                .packs
                .as_ref()
                .is_some_and(|row| row.selected == "llm-custom" && row.installed.len() == 2),
            "alarm-sounds" => payload
                .alarm_audio
                .as_ref()
                .is_some_and(|rows| rows.len() == 2),
            "web-prefs.json" => payload.web.as_ref().is_some_and(|map| {
                crate::persist::WEB_KEYS
                    .iter()
                    .all(|key| map.contains_key(*key))
            }),
            _ => false,
        }
    }

    #[test]
    fn every_store_is_marked_and_classified() {
        let payload = sample_full();
        assert_eq!(
            payload.covered,
            crate::persist::STORES
                .iter()
                .map(|id| (*id).to_string())
                .collect::<Vec<_>>()
        );
        for id in crate::persist::STORES {
            assert!(marker_ok(&payload, id), "{id} is not covered by the sample");
        }
        assert!(crate::persist::SESSION_KEYS.len() >= 4);
        assert!(crate::persist::NATIVE_PREFS.len() >= 5);
        assert!(crate::persist::WEB_KEYS.len() >= 8);
        let lib = include_str!("lib.rs");
        let mut joins = Vec::new();
        let mut rest = lib;
        let needle = ".join(\"";
        while let Some(at) = rest.find(needle) {
            let start = at + needle.len();
            let end = rest[start..].find('"').expect("join string");
            joins.push(&rest[start..start + end]);
            rest = &rest[start + end..];
        }
        for name in joins {
            let known = crate::persist::STORES.contains(&name)
                || crate::persist::EXCLUDED_JOINS
                    .iter()
                    .any(|(id, _)| *id == name);
            assert!(known, "unclassified persistence {name}");
        }
    }

    #[test]
    fn round_trip_every_store() {
        let original = sample_full();
        let encrypted = seal(&original, Some("correct horse")).unwrap();
        let plain = seal(&original, None).unwrap();
        assert!(encrypted.windows(4).any(|mark| mark == MAGIC));
        assert_eq!(&plain[..4], PLAIN);
        assert!(
            plain
                .windows(b"xai-round-trip-secret".len())
                .any(|window| window == b"xai-round-trip-secret")
        );
        assert!(
            !encrypted
                .windows(b"xai-round-trip-secret".len())
                .any(|window| window == b"xai-round-trip-secret")
        );
        assert!(
            !encrypted
                .windows(b"GGUF-WEIGHTS".len())
                .any(|window| window == b"GGUF-WEIGHTS")
        );
        let from_enc = open(&encrypted, Some("correct horse")).unwrap();
        let from_plain = open(&plain, None).unwrap();
        assert!(from_enc.encrypted);
        assert!(!from_plain.encrypted);
        let left = serde_json::to_value(&original).unwrap();
        assert_eq!(serde_json::to_value(&from_enc.payload).unwrap(), left);
        assert_eq!(serde_json::to_value(&from_plain.payload).unwrap(), left);
        assert!(open(&encrypted, None).is_err());
        assert!(open(&encrypted, Some("wrong passphrase")).is_err());

        let dir = std::env::temp_dir().join(format!(
            "grok-backup-round-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        write_tree(&dir, &original).unwrap();
        let before = fingerprint(&dir);
        assert!(before.iter().any(|(name, _)| name == "settings.json"));
        assert!(before.iter().any(|(name, _)| name == "web-prefs.json"));
        assert!(!before.iter().any(|(name, _)| name == "memory.json" || name == "usage.json"));
        assert!(plain.windows(b"I live in Zurich".len()).any(|window| window == b"I live in Zurich"));
        assert!(!plain.windows(b"remember the backup".len()).any(|window| window == b"remember the backup"));
        assert!(
            before
                .iter()
                .any(|(name, bytes)| name.ends_with("chime.mp3") && bytes == b"tiny-audio")
        );
        assert!(before.iter().any(|(name, bytes)| name.ends_with("hey_grok.rpw") && bytes == b"rpw-sample-bytes"));
        assert!(!before.iter().any(|(name, _)| name.ends_with("huge.wav")));
        assert!(dir.join("local-models").join("selected-llm.txt").exists());
        wipe_tree(&dir);
        assert!(fingerprint(&dir).is_empty());
        write_tree(&dir, &from_enc.payload).unwrap();
        assert_eq!(fingerprint(&dir), before);
        let again = read_tree(&dir).unwrap();
        assert_eq!(again.settings.unwrap().locale, "de");
        assert_eq!(again.agenda.unwrap().items.len(), 3);
        assert_eq!(again.people[0].nickname, "Mom");
        assert_eq!(again.finance.unwrap().default_currency, "EUR");
        assert_eq!(
            again
                .web
                .unwrap()
                .get("grok.appearance")
                .map(String::as_str),
            Some("value-grok.appearance")
        );
        assert_eq!(again.packs.unwrap().selected, "llm-custom");
        assert!(
            again.keys.is_empty(),
            "vault secrets are not written as a plaintext file"
        );
        assert_eq!(from_enc.payload.keys.len(), 5);
        let _ = fs::remove_dir_all(&dir);
    }

    fn fingerprint(root: &Path) -> Vec<(String, Vec<u8>)> {
        let mut out = Vec::new();
        collect_fingerprint(root, root, &mut out);
        out.sort();
        out
    }

    fn collect_fingerprint(root: &Path, dir: &Path, out: &mut Vec<(String, Vec<u8>)>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_fingerprint(root, &path, out);
            } else if let Ok(bytes) = fs::read(&path) {
                let name = path
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                out.push((name, bytes));
            }
        }
    }

    #[test]
    fn older_backup_with_history_opens_and_history_is_not_restored() {
        let mut payload = sample_full();
        payload.talk = Some(vec![crate::memory::Turn {
            at_ms: 9,
            role: "user".into(),
            text: "remember the backup".into(),
        }]);
        payload.usage = Some(crate::usage::Usage {
            day: "2099-03-01".into(),
            session_ms: 60_000,
            sessions: 4,
            warned: true,
            ..crate::usage::Usage::default()
        });
        payload.memory.as_mut().unwrap().sessions.push(crate::local_memory::SessionNote {
            id: "s1".into(),
            at_ms: 2,
            text: "You said hello.".into(),
        });
        let opened = open(&seal(&payload, None).unwrap(), None).unwrap();
        assert_eq!(opened.payload.talk.as_ref().unwrap()[0].text, "remember the backup");
        assert_eq!(opened.payload.usage.as_ref().unwrap().sessions, 4);
        let kept = without_history(&opened.payload);
        assert!(kept.talk.is_none());
        assert!(kept.usage.is_none());
        let memory = kept.memory.unwrap();
        assert!(memory.sessions.is_empty());
        assert_eq!(memory.facts[0].text, "I live in Zurich");
        assert!(kept.keys.iter().any(|key| key.provider == "onemin"));
    }

    #[test]
    fn round_trip_keeps_notes_people_and_drops_keys_by_default() {
        let payload = sample();
        assert!(payload.keys.is_empty());
        let bytes = seal(&payload, Some("correct horse")).unwrap();
        let opened = open(&bytes, Some("correct horse")).unwrap();
        assert_eq!(opened.payload.notes[0].text, "buy milk");
        assert_eq!(opened.payload.people[0].nickname, "Mom");
        assert_eq!(opened.payload.people[0].reminders[0].date, "03-03");
        assert!(opened.payload.keys.is_empty());
        assert_eq!(opened.payload.preview().reminders, 1);
        assert!(open(&bytes, Some("wrong passphrase")).is_err());
        assert!(seal(&payload, Some("short")).is_err());
    }

    #[test]
    fn keys_round_trip_only_when_present() {
        let mut payload = sample();
        payload.keys.push(BackupKey {
            id: "k1".into(),
            provider: "xai".into(),
            secret: "xai-test-secret".into(),
        });
        let bytes = seal(&payload, Some("another secret")).unwrap();
        let encoded = B64.encode(&bytes);
        let opened = open_b64(&encoded, Some("another secret")).unwrap();
        assert_eq!(opened.payload.keys.len(), 1);
        assert_eq!(opened.payload.keys[0].secret, "xai-test-secret");
        assert_eq!(opened.payload.preview().keys, 1);
    }

    #[test]
    fn old_backups_without_memory_still_open() {
        let old = r#"{"kind":"grok-voice-backup","version":1,"notes":[],"people":[],"keys":[]}"#;
        let payload: Payload = serde_json::from_str(old).unwrap();
        assert!(payload.memory.is_none());
        assert!(!payload.preview().has_memory);
        assert!(payload.finance.is_none());
        assert!(!payload.preview().has_finance);
        assert!(payload.settings.is_none());
        assert!(!payload.preview().has_settings);
        let bytes = seal(&payload, Some("correct horse")).unwrap();
        let opened = open(&bytes, Some("correct horse")).unwrap();
        assert!(opened.payload.memory.is_none());
        assert!(opened.payload.agenda.is_none());
    }

    #[test]
    fn memory_round_trips_inside_version_one() {
        let mut payload = sample();
        payload.memory = Some(crate::local_memory::Bundle {
            facts: vec![crate::local_memory::Fact {
                id: "f1".into(),
                text: "I live in Zurich".into(),
                updated_ms: 1,
            }],
            sessions: vec![crate::local_memory::SessionNote {
                id: "s1".into(),
                at_ms: 2,
                text: "You said hello. The reply was hi.".into(),
            }],
            next_id: 2,
        });
        let opened = open(
            &seal(&payload, Some("correct horse")).unwrap(),
            Some("correct horse"),
        )
        .unwrap();
        assert_eq!(opened.payload.preview().facts, 1);
        assert!(opened.payload.preview().has_memory);
        assert_eq!(
            opened.payload.memory.as_ref().unwrap().facts[0].text,
            "I live in Zurich"
        );
    }

    #[test]
    fn restore_rejects_unknown_providers_and_whitespace_secrets() {
        assert!(acceptable_key(&BackupKey {
            id: "xai-1".into(),
            provider: "xai".into(),
            secret: "xai-test-secret".into()
        }));
        assert!(acceptable_key(&BackupKey {
            id: "legacy-xai".into(),
            provider: "xai".into(),
            secret: "xai-test-secret".into()
        }));
        assert!(acceptable_key(&BackupKey {
            id: "hf-token".into(),
            provider: "huggingface".into(),
            secret: "hf_test_token".into()
        }));
        assert!(acceptable_key(&BackupKey {
            id: "one-1".into(),
            provider: "onemin".into(),
            secret: "onemin-test-secret".into()
        }));
        assert!(!acceptable_key(&BackupKey {
            id: "k1".into(),
            provider: "other".into(),
            secret: "xai-test-secret".into()
        }));
        assert!(!acceptable_key(&BackupKey {
            id: "k 1".into(),
            provider: "xai".into(),
            secret: "xai-test-secret".into()
        }));
        assert!(!acceptable_key(&BackupKey {
            id: "k1".into(),
            provider: "xai".into(),
            secret: "has space".into()
        }));
    }
}
