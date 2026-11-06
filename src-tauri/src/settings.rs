use std::{fs, path::PathBuf, sync::RwLock};

use serde::{Deserialize, Serialize};

use crate::error::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub model: String,
    pub voice: String,
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
    /// Wake-word sessions try a saved on-device account first when the packs are ready.
    /// Off keeps the saved key order. A missing local account is never invented.
    pub prefer_local_on_wake: bool,
    /// Soft monthly voice budget in minutes. 0 is off. Counts every provider, including failover.
    pub monthly_budget_minutes: u32,
    /// End the live session when the monthly budget is reached. Off unless the user opts in.
    pub monthly_budget_hard_stop: bool,
    /// On-device speech: `en` (Moonshine + Piper English) or `de` (German packs).
    pub local_speech: String,
    /// UI locale. Empty follows the phone. `ca-ES-valencia` is Valencian.
    pub locale: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            model: "grok-voice-latest".into(),
            voice: "eve".into(),
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
        let inner = fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
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
        s.local_speech = if s.local_speech == "de" { "de".into() } else { "en".into() };
        s.locale = if s.locale.trim().is_empty() {
            String::new()
        } else {
            crate::i18n::normalize(&s.locale).to_string()
        };
        s.model = safe_model_name(&s.model);
        if s.voice.trim().is_empty() {
            s.voice = Settings::default().voice;
        }
        crate::files::write_private(&self.path, &serde_json::to_vec_pretty(&s)?)?;
        *self.inner.write().unwrap() = s.clone();
        Ok(s)
    }
}
