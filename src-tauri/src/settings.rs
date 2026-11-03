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
            idle_timeout_secs: 30,
            onboarded: false,
            listen_armed: false,
        }
    }
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
        if s.model.trim().is_empty() {
            s.model = Settings::default().model;
        }
        if s.voice.trim().is_empty() {
            s.voice = Settings::default().voice;
        }
        fs::write(&self.path, serde_json::to_vec_pretty(&s)?)?;
        *self.inner.write().unwrap() = s.clone();
        Ok(s)
    }
}
