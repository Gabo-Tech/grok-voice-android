use std::{fs, path::PathBuf, sync::RwLock};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::error::Result;

/// Returned as the tool error key. The caller speaks the localized line.
pub const SECRET_REFUSAL: &str = "secret-refused";
pub const KEYS_REFUSAL: &str = "keys-refused";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PersonLead {
    pub label: String,
    pub days: u32,
}

#[derive(Clone, Debug)]
pub struct SettingsEdit {
    pub settings: Option<Settings>,
    pub listen: Option<bool>,
    pub person_lead: Option<PersonLead>,
    pub wipe_memory: bool,
    /// i18n key. The change is not applied until the user confirms.
    pub pending: Option<&'static str>,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub model: String,
    /// Grok / xAI realtime voice. Unknown stored values become `eve`.
    pub voice: String,
    /// OpenAI Realtime voice. Unknown stored values become `marin`. Never an xAI id.
    pub openai_voice: String,
    pub instructions: String,
    /// Mute the mic uplink while assistant audio is playing so a loudspeaker
    /// doesn't make Grok answer itself. Disables barge-in.
    pub echo_guard: bool,
    /// Rustpotter detection threshold (0..1). Higher = fewer false wakes.
    pub wake_threshold: f32,
    /// Close the realtime session after this many seconds without speech.
    pub idle_timeout_secs: u64,
    /// First-run setup tour has been finished on this device.
    pub onboarded: bool,
    /// Hands-free wake listening should survive leaving or clearing the app.
    pub listen_armed: bool,
    /// Soft daily voice budget in minutes. 0 is off.
    pub daily_budget_minutes: u32,
    /// End the live session when the soft budget is reached. Off unless the user opts in.
    pub daily_budget_hard_stop: bool,
    /// Talk tries a saved on-device account first when the packs are ready.
    /// Off keeps the saved key order. A missing local account is never invented.
    pub prefer_local_on_wake: bool,
    /// Soft monthly voice budget in minutes. 0 is off. Counts every provider, including failover.
    pub monthly_budget_minutes: u32,
    /// End the live session when the monthly budget is reached. Off unless the user opts in.
    pub monthly_budget_hard_stop: bool,
    /// On-device speech: `en` (Moonshine + Piper English), `es` (Spanish packs), or `de` (German packs).
    pub local_speech: String,
    /// UI locale. Empty follows the phone. `ca-ES-valencia` is Valencian.
    pub locale: String,
    /// `normal` posts the notification. `spoken` also reads it out loud.
    /// Empty means the payload omitted the field. Load and save treat that as
    /// Normal, and a save that omits it keeps the stored mode.
    #[serde(default = "crate::notices::omitted_delivery")]
    pub notification_delivery: String,
    /// Optional name spoken alerts may use. Empty keeps a plain "Hey".
    #[serde(default)]
    pub spoken_name: String,
    /// `local`, `musify`, `youtube`, `youtube_music`, or `system`.
    /// Empty and unknown values stay on files on this phone.
    #[serde(default)]
    pub music_player: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            model: "grok-voice-latest".into(),
            voice: crate::voices::XAI_DEFAULT.into(),
            openai_voice: crate::voices::OPENAI_DEFAULT.into(),
            instructions: "You are Grok, a witty, concise voice assistant running on the user's \
                           Android phone. Answer in one to three short spoken sentences unless \
                           asked for more detail."
                .into(),
            echo_guard: true,
            wake_threshold: 0.5,
            idle_timeout_secs: 10,
            onboarded: false,
            listen_armed: false,
            daily_budget_minutes: 0,
            daily_budget_hard_stop: false,
            prefer_local_on_wake: false,
            monthly_budget_minutes: 0,
            monthly_budget_hard_stop: false,
            local_speech: "en".into(),
            locale: String::new(),
            notification_delivery: crate::notices::DELIVERY_NORMAL.into(),
            spoken_name: String::new(),
            music_player: crate::music::PLAYER_LOCAL.into(),
        }
    }
}

/// Query values that are placed on the realtime URL. Letters, digits, and ._- only.
pub fn safe_model_name(model: &str) -> String {
    let cleaned: String = model
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        .take(64)
        .collect();
    if cleaned.is_empty() { Settings::default().model } else { cleaned }
}

pub struct SettingsStore {
    path: PathBuf,
    inner: RwLock<Settings>,
}

impl SettingsStore {
    pub fn load(path: PathBuf) -> Self {
        let mut inner = fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice::<Settings>(&b).ok())
            .unwrap_or_default();
        inner.notification_delivery = crate::notices::stored_delivery(&inner.notification_delivery);
        inner.music_player = crate::music::stored_player(&inner.music_player).to_string();
        inner.voice = crate::voices::xai_voice(&inner.voice);
        inner.openai_voice = crate::voices::openai_voice(&inner.openai_voice);
        Self { path, inner: RwLock::new(inner) }
    }

    pub fn get(&self) -> Settings {
        self.inner.read().unwrap().clone()
    }

    pub fn save(&self, mut s: Settings) -> Result<Settings> {
        s.wake_threshold = s.wake_threshold.clamp(0.2, 0.95);
        s.idle_timeout_secs = s.idle_timeout_secs.clamp(5, 600);
        s.daily_budget_minutes = s.daily_budget_minutes.min(24 * 60);
        s.monthly_budget_minutes = s.monthly_budget_minutes.min(31 * 24 * 60);
        s.local_speech = stored_speech(&s.local_speech);
        s.locale = if s.locale.trim().is_empty() {
            String::new()
        } else {
            crate::i18n::normalize(&s.locale).to_string()
        };
        s.model = safe_model_name(&s.model);
        s.voice = crate::voices::xai_voice(&s.voice);
        s.openai_voice = crate::voices::openai_voice(&s.openai_voice);
        if s.notification_delivery.trim().is_empty() {
            s.notification_delivery = crate::notices::stored_delivery(&self.get().notification_delivery);
        } else {
            s.notification_delivery = crate::notices::stored_delivery(&s.notification_delivery);
        }
        s.spoken_name = crate::notices::sanitize_spoken_name(&s.spoken_name);
        s.music_player = crate::music::stored_player(&s.music_player).to_string();
        crate::files::write_private(&self.path, &serde_json::to_vec_pretty(&s)?)?;
        *self.inner.write().unwrap() = s.clone();
        Ok(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_save_cannot_clear_listen() {
        let stored = Settings { listen_armed: true, ..Settings::default() };
        let incoming = Settings { listen_armed: false, daily_budget_minutes: 15, ..Settings::default() };
        let kept = keep_listen_preference(&stored, incoming);
        assert!(kept.listen_armed);
        assert_eq!(kept.daily_budget_minutes, 15);
        let off = Settings { listen_armed: false, ..Settings::default() };
        assert!(!keep_listen_preference(&off, stored).listen_armed);
    }

    #[test]
    fn voice_settings_change_language_budget_and_keep_listen() {
        let current = Settings { listen_armed: true, daily_budget_minutes: 0, ..Settings::default() };
        let edit = edit_settings(&current, &json!({"language": "German", "preferOnDevice": true, "dailyBudgetMinutes": 15, "dailyHardStop": true})).unwrap();
        let next = edit.settings.unwrap();
        assert!(next.listen_armed);
        assert_eq!(next.locale, "de");
        assert!(next.prefer_local_on_wake);
        assert_eq!(next.daily_budget_minutes, 15);
        assert!(next.daily_budget_hard_stop);
        assert!(edit.listen.is_none());
        assert!(edit.detail.contains("German"));
        assert!(edit.detail.contains("15 minutes"));

        let path = std::env::temp_dir().join(format!("grok-settings-voice-{}.json", std::process::id()));
        let _ = fs::remove_file(&path);
        let store = SettingsStore::load(path.clone());
        store.save(current.clone()).unwrap();
        let saved = store.save(next).unwrap();
        assert!(saved.listen_armed);
        let raw = fs::read_to_string(&path).unwrap();
        assert!(raw.contains("\"listenArmed\": true"));
        assert!(raw.contains("\"locale\": \"de\""));
        assert!(!raw.contains("birthdayLeadDays"));
        assert!(!raw.contains("apiKey"));
        let _ = fs::remove_file(&path);

        let described = describe_settings(&saved, &["xAI".into(), "On device".into()]);
        let text = described.to_string();
        assert!(text.contains("German"));
        assert!(text.contains("xAI"));
        assert!(text.contains("On device"));
        assert!(!text.contains("sk-"));
        assert!(!text.contains("password"));
        assert!(!text.contains("hint"));
    }

    #[test]
    fn voice_settings_reject_secrets_and_key_admin() {
        let current = Settings { listen_armed: true, locale: "es".into(), ..Settings::default() };
        for args in [
            json!({"apiKey": "sk-live-secret"}),
            json!({"password": "hunter2"}),
            json!({"passphrase": "backup-pass"}),
            json!({"token": "abcdef"}),
            json!({"voice": "sk-abcdef123456"}),
            json!({"model": "xai-secret-key"}),
        ] {
            let err = edit_settings(&current, &args).unwrap_err();
            assert_eq!(err, SECRET_REFUSAL, "{args}");
        }
        assert_eq!(edit_settings(&current, &json!({"reorder": ["a", "b"]})).unwrap_err(), KEYS_REFUSAL);
        assert_eq!(edit_settings(&current, &json!({"deleteKey": "a"})).unwrap_err(), KEYS_REFUSAL);
        assert_eq!(current.locale, "es");
        assert!(current.listen_armed);
        let described = describe_settings(&current, &["xAI".into()]);
        assert!(!described.to_string().contains("sk-"));
    }

    #[test]
    fn voice_settings_confirm_listen_off_and_map_birthday_lead() {
        let armed = Settings { listen_armed: true, ..Settings::default() };
        let held = edit_settings(&armed, &json!({"listen": false})).unwrap();
        assert!(held.settings.is_none());
        assert_eq!(held.pending, Some("listen-off-ask"));
        assert!(held.listen.is_none());
        let off = edit_settings(&armed, &json!({"listen": "off", "confirmed": true})).unwrap();
        assert!(!off.settings.unwrap().listen_armed);
        assert_eq!(off.listen, Some(false));
        assert!(off.pending.is_none());

        let quiet = Settings { listen_armed: false, ..Settings::default() };
        let on = edit_settings(&quiet, &json!({"listen": true, "quietGapSeconds": 25, "monthlyBudget": "300", "localSpeech": "German"})).unwrap();
        let next = on.settings.unwrap();
        assert!(next.listen_armed);
        assert_eq!(next.idle_timeout_secs, 25);
        assert_eq!(next.monthly_budget_minutes, 300);
        assert_eq!(next.local_speech, "de");
        assert_eq!(on.listen, Some(true));
        let spanish = edit_settings(&quiet, &json!({"localSpeech": "español"})).unwrap();
        let spanish_settings = spanish.settings.unwrap();
        assert_eq!(spanish_settings.local_speech, "es");
        assert!(spanish.detail.contains("Spanish"));
        assert!(describe_settings(&spanish_settings, &[]).to_string().contains("\"localSpeech\":\"Spanish\""));
        assert!(
            edit_settings(&quiet, &json!({"localSpeech": "Portuguese"}))
                .unwrap_err()
                .contains("Spanish")
        );
        let speech_path = std::env::temp_dir().join(format!("grok-settings-speech-{}.json", std::process::id()));
        let _ = fs::remove_file(&speech_path);
        let speech_store = SettingsStore::load(speech_path.clone());
        let saved_speech = speech_store.save(spanish_settings).unwrap();
        assert_eq!(saved_speech.local_speech, "es");
        assert!(fs::read_to_string(&speech_path).unwrap().contains("\"localSpeech\": \"es\""));
        let mut other = saved_speech.clone();
        other.local_speech = "pt".into();
        assert_eq!(speech_store.save(other).unwrap().local_speech, "en");
        let _ = fs::remove_file(&speech_path);

        let lead = edit_settings(&armed, &json!({"notifyDays": 7})).unwrap();
        assert!(lead.settings.is_none());
        let person_lead = lead.person_lead.unwrap();
        assert_eq!(person_lead.label, "birthday");
        assert_eq!(person_lead.days, 7);
        let wipe = edit_settings(&armed, &json!({"wipeMemory": true})).unwrap();
        assert_eq!(wipe.pending, Some("wipe-ask"));
        assert!(!wipe.wipe_memory);
        let wiped = edit_settings(&armed, &json!({"wipeMemory": true, "confirmed": true})).unwrap();
        assert!(wiped.wipe_memory);
        assert!(edit_settings(&armed, &json!({"notASetting": 1})).is_err());
        assert!(edit_settings(&armed, &json!({"language": "Klingon"})).is_err());
    }

    #[test]
    fn notification_delivery_defaults_to_normal_and_survives_a_partial_save() {
        let raw = r#"{"listenArmed":true,"locale":"de","dailyBudgetMinutes":4}"#;
        let parsed: Settings = serde_json::from_str(raw).unwrap();
        assert!(parsed.notification_delivery.is_empty());
        assert!(parsed.listen_armed);
        assert_eq!(parsed.locale, "de");
        assert_eq!(Settings::default().notification_delivery, "normal");

        let path = std::env::temp_dir().join(format!("grok-settings-alerts-{}.json", std::process::id()));
        let _ = fs::remove_file(&path);
        fs::write(&path, raw).unwrap();
        let store = SettingsStore::load(path.clone());
        assert_eq!(store.get().notification_delivery, "normal");
        assert!(store.get().listen_armed);
        assert_eq!(store.get().locale, "de");
        assert_eq!(store.get().daily_budget_minutes, 4);
        assert_eq!(store.get().voice, Settings::default().voice);

        let spoken = edit_settings(&store.get(), &json!({"notificationDelivery": "spoken"})).unwrap();
        assert!(spoken.pending.is_none());
        assert!(spoken.listen.is_none());
        let next = spoken.settings.unwrap();
        assert_eq!(next.notification_delivery, "spoken");
        assert!(next.listen_armed);
        store.save(next).unwrap();

        let partial: Settings = serde_json::from_str(r#"{"dailyBudgetMinutes":9,"locale":"es","listenArmed":false}"#).unwrap();
        assert!(partial.notification_delivery.is_empty());
        let kept = keep_listen_preference(&store.get(), partial);
        let saved = store.save(kept).unwrap();
        assert_eq!(saved.notification_delivery, "spoken");
        assert!(saved.listen_armed);
        assert_eq!(saved.daily_budget_minutes, 9);
        assert_eq!(saved.locale, "es");
        let again = SettingsStore::load(path.clone());
        assert_eq!(again.get().notification_delivery, "spoken");
        assert!(again.get().listen_armed);

        let quiet = edit_settings(&again.get(), &json!({"speakNotifications": false})).unwrap();
        assert!(quiet.pending.is_none());
        assert_eq!(quiet.settings.unwrap().notification_delivery, "normal");
        let aloud = edit_settings(&again.get(), &json!({"readAloud": true})).unwrap();
        let described = describe_settings(&aloud.settings.unwrap(), &[]);
        let text = described.to_string();
        assert!(text.contains("\"notificationDelivery\":\"spoken\""));
        assert!(text.contains("spoken out loud"));
        assert!(!text.contains("password"));
        assert!(edit_settings(&again.get(), &json!({"notificationDelivery": "sideways"})).is_err());
        let named = edit_settings(&again.get(), &json!({"spokenName": "  Gabriel  "})).unwrap();
        assert_eq!(named.settings.unwrap().spoken_name, "Gabriel");
        let kept = edit_settings(
            &Settings { spoken_name: "Gabriel".into(), ..Settings::default() },
            &json!({"notificationDelivery": "spoken"}),
        )
        .unwrap();
        assert_eq!(kept.settings.unwrap().spoken_name, "Gabriel");
        let cleared = edit_settings(
            &Settings { spoken_name: "Gabriel".into(), ..Settings::default() },
            &json!({"spokenName": ""}),
        )
        .unwrap();
        assert!(cleared.settings.unwrap().spoken_name.is_empty());
        assert_eq!(crate::notices::sanitize_spoken_name("Gabriel"), "Gabriel");
        assert!(describe_settings(&Settings { spoken_name: "Gabriel".into(), ..Settings::default() }, &[]).to_string().contains("Gabriel"));
        assert!(!describe_settings(&Settings::default(), &[]).to_string().contains("Gabriel"));
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn voice_names_follow_the_provider_and_unknown_values_migrate() {
        let stale = Settings { voice: "NotAVoice".into(), openai_voice: "fable".into(), ..Settings::default() };
        let path = std::env::temp_dir().join(format!("grok-settings-voices-{}.json", std::process::id()));
        let _ = fs::remove_file(&path);
        fs::write(&path, serde_json::to_vec_pretty(&stale).unwrap()).unwrap();
        let store = SettingsStore::load(path.clone());
        assert_eq!(store.get().voice, "eve");
        assert_eq!(store.get().openai_voice, "marin");

        let named = edit_settings(&store.get(), &json!({"voice": "Ara"})).unwrap();
        assert_eq!(named.settings.unwrap().voice, "ara");
        let openai = edit_settings(&store.get(), &json!({"voice": "cedar"})).unwrap();
        let next = openai.settings.unwrap();
        assert_eq!(next.voice, "eve");
        assert_eq!(next.openai_voice, "cedar");
        assert!(openai.detail.contains("OpenAI voice is cedar"));
        let explicit = edit_settings(&next, &json!({"openaiVoice": "Ash"})).unwrap();
        assert_eq!(explicit.settings.unwrap().openai_voice, "ash");
        assert!(edit_settings(&store.get(), &json!({"voice": "not-a-voice"})).is_err());
        assert!(edit_settings(&store.get(), &json!({"openaiVoice": "eve"})).is_err());
        let described = describe_settings(&Settings { voice: "rex".into(), openai_voice: "coral".into(), ..Settings::default() }, &[]);
        let text = described.to_string();
        assert!(text.contains("\"voice\":\"rex\""));
        assert!(text.contains("\"openaiVoice\":\"coral\""));
        assert!(text.contains("Grok voice is rex"));
        let _ = fs::remove_file(&path);
    }
}

/// The Listen switch is saved by `set_wake_enabled`. Other settings screens must
/// not clear it when their payload omits the field.
pub fn keep_listen_preference(stored: &Settings, mut incoming: Settings) -> Settings {
    incoming.listen_armed = stored.listen_armed;
    incoming
}

/// Settings Talk can read. Voice order is provider names only.
pub fn describe_settings(settings: &Settings, voice_order: &[String]) -> Value {
    let language = language_name(&settings.locale);
    let speech = speech_name(&settings.local_speech);
    let daily = minutes_phrase(settings.daily_budget_minutes);
    let monthly = minutes_phrase(settings.monthly_budget_minutes);
    let order = if voice_order.is_empty() {
        "No voice is saved.".to_string()
    } else {
        format!("Voice order: {}.", voice_order.join(", then "))
    };
    let prefer = if settings.prefer_local_on_wake { "on" } else { "off" };
    let listen = if settings.listen_armed { "on" } else { "off" };
    let alerts = if settings.notification_delivery == crate::notices::DELIVERY_SPOKEN {
        "spoken out loud"
    } else {
        "normal"
    };
    let named = if settings.spoken_name.is_empty() {
        "No spoken name is saved.".to_string()
    } else {
        format!("Spoken alerts say {}.", settings.spoken_name)
    };
    let player = crate::music::player_name(&settings.music_player);
    let detail = format!(
        "Language is {language}. Local speech is {speech}. Prefer on device is {prefer}. Listen is {listen}. Notifications are {alerts}. {named} Quiet gap is {} seconds. Daily voice budget is {daily}. Monthly voice budget is {monthly}. Default music player is {player}. Grok voice is {}. OpenAI voice is {}. {order} Days before a birthday stay on that person.",
        settings.idle_timeout_secs, settings.voice, settings.openai_voice
    );
    json!({
        "ok": true,
        "language": language,
        "locale": settings.locale,
        "localSpeech": speech,
        "preferOnDevice": settings.prefer_local_on_wake,
        "listen": settings.listen_armed,
        "quietGapSeconds": settings.idle_timeout_secs,
        "dailyBudgetMinutes": settings.daily_budget_minutes,
        "dailyHardStop": settings.daily_budget_hard_stop && settings.daily_budget_minutes > 0,
        "monthlyBudgetMinutes": settings.monthly_budget_minutes,
        "monthlyHardStop": settings.monthly_budget_hard_stop && settings.monthly_budget_minutes > 0,
        "voice": settings.voice,
        "openaiVoice": settings.openai_voice,
        "model": settings.model,
        "echoGuard": settings.echo_guard,
        "wakeThreshold": settings.wake_threshold,
        "instructions": clip_text(&settings.instructions, 280),
        "voiceOrder": voice_order,
        "notificationDelivery": if settings.notification_delivery == crate::notices::DELIVERY_SPOKEN {
            crate::notices::DELIVERY_SPOKEN
        } else {
            crate::notices::DELIVERY_NORMAL
        },
        "spokenName": settings.spoken_name,
        "musicPlayer": crate::music::stored_player(&settings.music_player),
        "detail": detail
    })
}

/// Apply one Talk settings change. Starts from the saved settings, so a missing
/// field cannot clear Listen. Secrets and key order are refused.
pub fn edit_settings(current: &Settings, args: &Value) -> std::result::Result<SettingsEdit, String> {
    let Some(object) = args.as_object() else {
        return Err("Say which setting to change.".into());
    };
    if object.is_empty() {
        return Err("Say which setting to change.".into());
    }
    if object.keys().any(|key| is_secret_key(key)) || object.values().any(value_looks_secret) {
        return Err(SECRET_REFUSAL.into());
    }
    if object.keys().any(|key| is_key_admin(key)) {
        return Err(KEYS_REFUSAL.into());
    }
    let unknown: Vec<&str> = object.keys().filter(|key| !known_setting(key)).map(|key| key.as_str()).collect();
    if !unknown.is_empty() {
        return Err(format!(
            "That is not a setting ({}). You can change language, local speech, Prefer on device, Listen, notification delivery, spoken name, the default music player, quiet gap seconds, daily voice budget minutes, monthly voice budget minutes, hard stop, voice, model, instructions, echo guard, or wake threshold. Days before a birthday are lead days on that person.",
            unknown.join(", ")
        ));
    }

    let confirmed = arg_bool(args.get("confirmed")) == Some(true);
    let wipe = arg_bool(args.get("wipeMemory")) == Some(true);
    if wipe && !confirmed {
        return Ok(SettingsEdit {
            settings: None,
            listen: None,
            person_lead: None,
            wipe_memory: false,
            pending: Some("wipe-ask"),
            detail: String::new(),
        });
    }

    let mut next = current.clone();
    let mut changed = false;
    let mut notes = Vec::new();

    if let Some(language) = first_string(args, &["language", "locale"]) {
        next.locale = parse_locale(&language)?;
        changed = true;
        notes.push(format!("Language is {}.", language_name(&next.locale)));
    }
    if let Some(speech) = first_string(args, &["localSpeech", "speech"]) {
        next.local_speech = parse_speech(&speech)?;
        changed = true;
        notes.push(format!("Local speech is {}.", speech_name(&next.local_speech)));
    }
    if let Some(on) = first_bool(args, &["preferOnDevice", "preferLocal", "preferLocalOnWake"]) {
        next.prefer_local_on_wake = on;
        changed = true;
        notes.push(format!("Prefer on device is {}.", on_off(on)));
    }
    if let Some(seconds) = first_u64(args, &["quietGapSeconds", "quietGap", "idleTimeoutSecs"]) {
        next.idle_timeout_secs = seconds;
        changed = true;
        notes.push(format!("Quiet gap is {seconds} seconds."));
    }
    if let Some(minutes) = first_u32(args, &["dailyBudgetMinutes", "dailyBudget"]) {
        next.daily_budget_minutes = minutes;
        if minutes == 0 {
            next.daily_budget_hard_stop = false;
        }
        changed = true;
        notes.push(format!("Daily voice budget is {}.", minutes_phrase(minutes)));
    }
    if let Some(on) = first_bool(args, &["dailyHardStop", "dailyBudgetHardStop"]) {
        next.daily_budget_hard_stop = on && next.daily_budget_minutes > 0;
        changed = true;
        notes.push(format!("Daily hard stop is {}.", on_off(next.daily_budget_hard_stop)));
    }
    if let Some(minutes) = first_u32(args, &["monthlyBudgetMinutes", "monthlyBudget"]) {
        next.monthly_budget_minutes = minutes;
        if minutes == 0 {
            next.monthly_budget_hard_stop = false;
        }
        changed = true;
        notes.push(format!("Monthly voice budget is {}.", minutes_phrase(minutes)));
    }
    if let Some(on) = first_bool(args, &["monthlyHardStop", "monthlyBudgetHardStop"]) {
        next.monthly_budget_hard_stop = on && next.monthly_budget_minutes > 0;
        changed = true;
        notes.push(format!("Monthly hard stop is {}.", on_off(next.monthly_budget_hard_stop)));
    }
    if let Some(voice) = arg_string(args.get("voice")).filter(|voice| !voice.is_empty()) {
        match crate::voices::assign_voice(&voice)? {
            crate::voices::VoiceChoice::Xai(id) => {
                next.voice = id;
                notes.push(format!("Voice is {}.", next.voice));
            }
            crate::voices::VoiceChoice::OpenAi(id) => {
                next.openai_voice = id;
                notes.push(format!("OpenAI voice is {}.", next.openai_voice));
            }
        }
        changed = true;
    }
    if let Some(voice) = arg_string(args.get("openaiVoice")).filter(|voice| !voice.is_empty()) {
        let Some(id) = crate::voices::known_openai(&voice) else {
            return Err(
                "That OpenAI voice is not available. Use alloy, ash, ballad, coral, echo, sage, shimmer, verse, marin, or cedar."
                    .into(),
            );
        };
        next.openai_voice = id.to_string();
        changed = true;
        notes.push(format!("OpenAI voice is {}.", next.openai_voice));
    }
    if let Some(model) = arg_string(args.get("model")) {
        next.model = model;
        changed = true;
        notes.push(format!("Model is {}.", next.model));
    }
    if let Some(instructions) = arg_string(args.get("instructions")) {
        next.instructions = clip_text(&instructions, 2000);
        changed = true;
        notes.push("Instructions saved.".into());
    }
    if let Some(on) = arg_bool(args.get("echoGuard")) {
        next.echo_guard = on;
        changed = true;
        notes.push(format!("Echo guard is {}.", on_off(on)));
    }
    if let Some(threshold) = arg_f32(args.get("wakeThreshold")) {
        next.wake_threshold = threshold;
        changed = true;
        notes.push(format!("Wake threshold is {threshold:.2}."));
    }
    if let Some(mode) = notification_mode_from(args)? {
        next.notification_delivery = mode;
        changed = true;
        let alerts = if next.notification_delivery == crate::notices::DELIVERY_SPOKEN {
            "spoken out loud"
        } else {
            "normal"
        };
        notes.push(format!("Notifications are {alerts}."));
    }
    if args.get("spokenName").is_some() || args.get("userName").is_some() || args.get("myName").is_some() {
        let raw = first_string(args, &["spokenName", "userName", "myName"]).unwrap_or_default();
        next.spoken_name = crate::notices::sanitize_spoken_name(&raw);
        changed = true;
        if next.spoken_name.is_empty() {
            notes.push("Spoken alerts will not use a name.".into());
        } else {
            notes.push(format!("Spoken alerts will say {}.", next.spoken_name));
        }
    }
    if let Some(player) = first_string(args, &["musicPlayer", "defaultMusicPlayer", "music"]) {
        next.music_player = crate::music::parse_player(&player)?.to_string();
        changed = true;
        notes.push(format!("Default music player is {}.", crate::music::player_name(&next.music_player)));
    }

    let mut pending = None;
    let mut listen = None;
    if let Some(on) = first_bool(args, &["listen", "listenArmed"]) {
        if !on && current.listen_armed && !confirmed {
            pending = Some("listen-off-ask");
        } else if on == current.listen_armed {
            notes.push(format!("Listen is already {}.", on_off(on)));
        } else {
            next.listen_armed = on;
            listen = Some(on);
            changed = true;
            notes.push(format!("Listen is {}.", on_off(on)));
        }
    }

    let person_lead = person_lead_from(args)?;
    if wipe && confirmed {
        notes.push("Wiped pinned facts and recent on-device notes. Notes, People, Finance, and voice keys stay.".into());
    }
    if pending.is_some() && notes.iter().all(|note| note.is_empty()) && !changed && person_lead.is_none() {
        notes.clear();
    }
    if !changed && pending.is_none() && person_lead.is_none() && !wipe && notes.is_empty() {
        return Err("Say which setting to change.".into());
    }
    Ok(SettingsEdit {
        settings: changed.then_some(next),
        listen,
        person_lead,
        wipe_memory: wipe && confirmed,
        pending,
        detail: notes.join(" "),
    })
}

fn notification_mode_from(args: &Value) -> std::result::Result<Option<String>, String> {
    let mut mode = None;
    if let Some(text) = first_string(args, &["notificationDelivery", "notificationMode", "notifications"]) {
        mode = Some(crate::notices::parse_delivery(&text)?);
    }
    if let Some(on) = first_bool(args, &["speakNotifications", "readAloud", "spokenNotifications"]) {
        mode = Some(
            if on {
                crate::notices::DELIVERY_SPOKEN.to_string()
            } else {
                crate::notices::DELIVERY_NORMAL.to_string()
            },
        );
    }
    Ok(mode)
}

fn person_lead_from(args: &Value) -> std::result::Result<Option<PersonLead>, String> {
    let days = first_u32(args, &["birthdayLeadDays", "notifyDays", "leadDays"]);
    let Some(days) = days else { return Ok(None) };
    if days > 30 {
        return Err("Notify can be from the day itself up to 30 days before.".into());
    }
    let label = first_string(args, &["dateLabel", "label"]).unwrap_or_else(|| {
        if args.get("birthdayLeadDays").is_some() || args.get("notifyDays").is_some() { "birthday".into() } else { "birthday".into() }
    });
    let label = match label.trim().to_ascii_lowercase().as_str() {
        "" | "birthday" | "birthdays" | "bday" => "birthday".into(),
        "anniversary" | "anniversaries" => "anniversary".into(),
        "custom" | "all" | "dates" => {
            if label.eq_ignore_ascii_case("custom") { "custom".into() } else { String::new() }
        }
        other => other.to_string(),
    };
    Ok(Some(PersonLead { label, days }))
}

fn known_setting(key: &str) -> bool {
    matches!(
        key,
        "language"
            | "locale"
            | "localSpeech"
            | "speech"
            | "preferOnDevice"
            | "preferLocal"
            | "preferLocalOnWake"
            | "listen"
            | "listenArmed"
            | "confirmed"
            | "quietGap"
            | "quietGapSeconds"
            | "idleTimeoutSecs"
            | "dailyBudgetMinutes"
            | "dailyBudget"
            | "dailyHardStop"
            | "dailyBudgetHardStop"
            | "monthlyBudgetMinutes"
            | "monthlyBudget"
            | "monthlyHardStop"
            | "monthlyBudgetHardStop"
            | "voice"
            | "openaiVoice"
            | "model"
            | "instructions"
            | "echoGuard"
            | "wakeThreshold"
            | "birthdayLeadDays"
            | "notifyDays"
            | "leadDays"
            | "dateLabel"
            | "label"
            | "wipeMemory"
            | "notificationDelivery"
            | "notificationMode"
            | "notifications"
            | "speakNotifications"
            | "readAloud"
            | "spokenNotifications"
            | "spokenName"
            | "userName"
            | "myName"
            | "musicPlayer"
            | "defaultMusicPlayer"
            | "music"
    )
}

fn is_secret_key(key: &str) -> bool {
    let key = fold_key(key);
    matches!(
        key.as_str(),
        "key" | "apikey" | "token" | "secret" | "password" | "passphrase" | "backuppassword" | "voicekey" | "openaikey" | "xaikey" | "credentials"
    ) || key.contains("apikey")
        || key.contains("password")
        || key.contains("passphrase")
        || key.contains("secret")
}

fn is_key_admin(key: &str) -> bool {
    let key = fold_key(key);
    matches!(key.as_str(), "reorder" | "accountids" | "removekey" | "deletekey" | "voicekeys" | "clearkey" | "signout")
}

fn fold_key(key: &str) -> String {
    key.chars().filter(|c| c.is_ascii_alphanumeric()).flat_map(|c| c.to_lowercase()).collect()
}

fn value_looks_secret(value: &Value) -> bool {
    match value {
        Value::String(text) => looks_like_secret(text),
        Value::Array(rows) => rows.iter().any(value_looks_secret),
        Value::Object(map) => map.values().any(value_looks_secret),
        _ => false,
    }
}

fn looks_like_secret(value: &str) -> bool {
    let lower = value.trim().to_ascii_lowercase();
    lower.starts_with("sk-") || lower.starts_with("xai-") || lower.starts_with("bearer ")
}

fn parse_locale(value: &str) -> std::result::Result<String, String> {
    let folded = value.trim().to_ascii_lowercase();
    let locale = match folded.as_str() {
        "" | "system" | "phone" | "follow" | "follow the phone" | "default" => String::new(),
        "en" | "english" => "en".into(),
        "es" | "spanish" | "espanol" | "español" => "es".into(),
        "de" | "german" | "deutsch" => "de".into(),
        "pt" | "portuguese" | "portugues" | "português" => "pt".into(),
        "valencian" | "valencia" | "ca" | "ca-es-valencia" | "ca_es_valencia" => "ca-ES-valencia".into(),
        _ => return Err("Language can be English, Spanish, German, Portuguese, Valencian, or follow the phone.".into()),
    };
    Ok(locale)
}

fn stored_speech(value: &str) -> String {
    match value {
        "de" => "de".into(),
        "es" => "es".into(),
        _ => "en".into(),
    }
}

fn speech_name(code: &str) -> &'static str {
    match code {
        "de" => "German",
        "es" => "Spanish",
        _ => "English",
    }
}

fn parse_speech(value: &str) -> std::result::Result<String, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "en" | "english" => Ok("en".into()),
        "es" | "spanish" | "espanol" | "español" => Ok("es".into()),
        "de" | "german" | "deutsch" => Ok("de".into()),
        _ => Err("On-device speech is English, Spanish, or German.".into()),
    }
}

fn language_name(locale: &str) -> &'static str {
    match locale {
        "" => "Follow the phone",
        "es" => "Spanish",
        "de" => "German",
        "pt" => "Portuguese",
        "ca-ES-valencia" => "Valencian",
        _ => "English",
    }
}

fn minutes_phrase(minutes: u32) -> String {
    if minutes == 0 { "off".into() } else { format!("{minutes} minutes") }
}

fn on_off(on: bool) -> &'static str {
    if on { "on" } else { "off" }
}

fn clip_text(value: &str, max: usize) -> String {
    value.trim().chars().take(max).collect()
}

fn first_string(args: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| arg_string(args.get(*key)))
}

fn first_bool(args: &Value, keys: &[&str]) -> Option<bool> {
    keys.iter().find_map(|key| arg_bool(args.get(*key)))
}

fn first_u32(args: &Value, keys: &[&str]) -> Option<u32> {
    keys.iter().find_map(|key| arg_u32(args.get(*key)))
}

fn first_u64(args: &Value, keys: &[&str]) -> Option<u64> {
    first_u32(args, keys).map(|n| n as u64)
}

fn arg_string(value: Option<&Value>) -> Option<String> {
    let value = value?;
    match value {
        Value::String(text) => Some(text.trim().to_string()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(on) => Some(if *on { "true".into() } else { "false".into() }),
        _ => None,
    }
}

fn arg_bool(value: Option<&Value>) -> Option<bool> {
    match value? {
        Value::Bool(on) => Some(*on),
        Value::Number(number) => number.as_i64().map(|n| n != 0),
        Value::String(text) => match text.trim().to_ascii_lowercase().as_str() {
            "true" | "yes" | "on" | "1" => Some(true),
            "false" | "no" | "off" | "0" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

fn arg_u32(value: Option<&Value>) -> Option<u32> {
    match value? {
        Value::Number(number) => number.as_u64().map(|n| n.min(u32::MAX as u64) as u32),
        Value::String(text) => {
            if text.trim().eq_ignore_ascii_case("off") {
                return Some(0);
            }
            let digits: String = text.chars().skip_while(|c| !c.is_ascii_digit()).take_while(|c| c.is_ascii_digit()).collect();
            digits.parse().ok()
        }
        _ => None,
    }
}

fn arg_f32(value: Option<&Value>) -> Option<f32> {
    match value? {
        Value::Number(number) => number.as_f64().map(|n| n as f32),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }
}
