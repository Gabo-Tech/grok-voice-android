//! Every place user data or a preference is stored.
//!
//! A new file, vault, web key, or native preference must be listed here.
//! `backup` round-trips each id, and the node test fails when a new key
//! shows up in the source without a matching entry.

use std::{collections::BTreeMap, fs, path::PathBuf, sync::RwLock};

/// Durable stores. Names match `dir.join("...")` in `lib.rs` where that is how they are opened.
pub const STORES: &[&str] = &[
    "settings.json",
    "agenda.json",
    "people.json",
    "finance.json",
    "memory.json",
    "local-memory.json",
    "usage.json",
    "vault",
    "wakeword",
    "local-models",
    "alarm-sounds",
    "web-prefs.json",
];

/// `dir.join` targets that are not a person's data.
pub const EXCLUDED_JOINS: &[(&str, &str)] = &[(
    "update-check.json",
    "last update check, not something the person entered",
)];

/// localStorage keys the backup writes back. Session-only keys are listed separately.
pub const WEB_KEYS: &[&str] = &[
    "grok.appearance",
    "grok.customTheme",
    "grok-dashboard-layout",
    "grok-finance-allocation",
    "grok-flow-options",
    "grok-flow-colors",
    "grok-chart-period",
    "grok-chart-hidden",
    "grok-mic-open",
    "grok-listen-hint",
];

/// sessionStorage. It dies with the page, so a backup does not carry it.
pub const SESSION_KEYS: &[&str] = &[
    "grok-finance-tab",
    "grok-flow-panel",
    "grok-talk-focus",
    "grok-focus-hint",
    "grok-restore-summary",
];

/// Android SharedPreferences. `derived` is rewritten when alarms or Listen are restored.
pub const NATIVE_PREFS: &[(&str, &str)] = &[
    ("grok_agenda", "derived"),
    ("listen", "derived"),
    ("listen_trace", "diagnostic"),
    ("grok_music", "permission-flag"),
    ("widget_gate", "ephemeral"),
];

const WEB_VALUE_MAX: usize = 32_000;

pub fn filter_web(raw: BTreeMap<String, String>) -> BTreeMap<String, String> {
    raw.into_iter()
        .filter(|(key, _)| WEB_KEYS.contains(&key.as_str()))
        .map(|(key, value)| {
            let value = value.chars().take(WEB_VALUE_MAX).collect::<String>();
            (key, value)
        })
        .collect()
}

/// Mirror of the webview localStorage keys, so Talk can back them up without the page.
pub struct WebPrefs {
    path: PathBuf,
    inner: RwLock<BTreeMap<String, String>>,
}

impl WebPrefs {
    pub fn load(path: PathBuf) -> Self {
        let map = fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Self {
            path,
            inner: RwLock::new(filter_web(map)),
        }
    }

    pub fn get(&self) -> BTreeMap<String, String> {
        self.inner.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn replace(&self, raw: BTreeMap<String, String>) {
        let map = filter_web(raw);
        if let Ok(bytes) = serde_json::to_vec_pretty(&map) {
            let _ = crate::files::write_private(&self.path, &bytes);
        }
        *self.inner.write().unwrap_or_else(|e| e.into_inner()) = map;
    }
}
