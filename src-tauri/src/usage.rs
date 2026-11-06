//! On-device daily voice usage. Counters reset on the local day boundary.
//! A soft budget only warns. A hard stop is off unless the user turns it on.

use std::{
    fs,
    path::PathBuf,
    sync::Mutex,
};

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub day: String,
    pub session_ms: i64,
    pub sessions: u32,
    pub warned: bool,
    pub stopped: bool,
    #[serde(default)]
    pub open_since_ms: Option<i64>,
    /// Provider of the open session: `xai`, `openai`, or `local`.
    #[serde(default)]
    pub open_provider: String,
    #[serde(default)]
    pub xai_ms: i64,
    #[serde(default)]
    pub openai_ms: i64,
    #[serde(default)]
    pub local_ms: i64,
    #[serde(default)]
    pub month: String,
    #[serde(default)]
    pub month_ms: i64,
    #[serde(default)]
    pub month_xai_ms: i64,
    #[serde(default)]
    pub month_openai_ms: i64,
    #[serde(default)]
    pub month_local_ms: i64,
    #[serde(default)]
    pub month_warned: bool,
    #[serde(default)]
    pub month_stopped: bool,
}

impl Default for Usage {
    fn default() -> Self {
        Self {
            day: local_day(Local::now()),
            session_ms: 0,
            sessions: 0,
            warned: false,
            stopped: false,
            open_since_ms: None,
            open_provider: String::new(),
            xai_ms: 0,
            openai_ms: 0,
            local_ms: 0,
            month: local_month(Local::now()),
            month_ms: 0,
            month_xai_ms: 0,
            month_openai_ms: 0,
            month_local_ms: 0,
            month_warned: false,
            month_stopped: false,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Caps {
    pub daily_minutes: u32,
    pub daily_hard_stop: bool,
    pub monthly_minutes: u32,
    pub monthly_hard_stop: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Guard {
    pub minutes: u32,
    pub budget_minutes: u32,
    pub hard_stop: bool,
    pub month_minutes: u32,
    pub monthly_budget_minutes: u32,
    pub message: Option<String>,
    pub block: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageStatus {
    pub day: String,
    pub minutes: u32,
    pub sessions: u32,
    pub budget_minutes: u32,
    pub hard_stop: bool,
    pub xai_minutes: u32,
    pub openai_minutes: u32,
    pub local_minutes: u32,
    pub month: String,
    pub month_minutes: u32,
    pub month_xai_minutes: u32,
    pub month_openai_minutes: u32,
    pub month_local_minutes: u32,
    pub monthly_budget_minutes: u32,
    pub monthly_hard_stop: bool,
    pub open_since_ms: Option<i64>,
    pub open_provider: String,
    pub message: Option<String>,
}

pub struct UsageStore {
    path: PathBuf,
    inner: Mutex<Usage>,
}

impl UsageStore {
    pub fn load(path: PathBuf) -> Self {
        let mut usage = fs::read(&path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        close_orphan(&mut usage, Local::now());
        Self { path, inner: Mutex::new(usage) }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Usage> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn persist(path: &PathBuf, usage: &Usage) {
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        if let Ok(bytes) = serde_json::to_vec_pretty(usage) {
            let _ = crate::files::write_private(path, &bytes);
        }
    }

    pub fn status(&self, caps: Caps, locale: &str) -> UsageStatus {
        let now = Local::now();
        let mut usage = self.lock().clone();
        let (next, guard) = evaluate(&usage, caps, locale, now);
        if next.day != usage.day || next.month != usage.month {
            usage = next;
            Self::persist(&self.path, &usage);
            *self.lock() = usage.clone();
        }
        status_of(&usage, caps, guard, now)
    }

    /// True when a new session must not start.
    pub fn blocked(&self, caps: Caps, locale: &str) -> Option<Guard> {
        let (usage, guard) = evaluate(&self.lock(), caps, locale, Local::now());
        if usage.day != self.lock().day || usage.month != self.lock().month {
            Self::persist(&self.path, &usage);
            *self.lock() = usage;
        }
        guard.block.then_some(guard)
    }

    pub fn begin(&self) {
        let now = Local::now();
        let mut usage = self.lock().clone();
        roll(&mut usage, now);
        if usage.open_since_ms.is_some() {
            end_open(&mut usage, now);
        }
        usage.sessions = usage.sessions.saturating_add(1);
        usage.open_since_ms = Some(now.timestamp_millis());
        usage.open_provider.clear();
        Self::persist(&self.path, &usage);
        *self.lock() = usage;
    }

    /// Attribute the open slice to a provider. The first provider also gets the time since begin.
    pub fn note_provider(&self, provider: &str) {
        let provider = match provider {
            "xai" | "openai" | "local" => provider,
            _ => return,
        };
        let now = Local::now();
        let mut usage = self.lock().clone();
        roll(&mut usage, now);
        if usage.open_provider == provider {
            return;
        }
        if usage.open_provider.is_empty() {
            usage.open_provider = provider.to_string();
        } else {
            end_open(&mut usage, now);
            usage.open_since_ms = Some(now.timestamp_millis());
            usage.open_provider = provider.to_string();
        }
        Self::persist(&self.path, &usage);
        *self.lock() = usage;
    }

    pub fn end(&self) {
        let now = Local::now();
        let mut usage = self.lock().clone();
        roll(&mut usage, now);
        end_open(&mut usage, now);
        usage.open_provider.clear();
        Self::persist(&self.path, &usage);
        *self.lock() = usage;
    }

    /// Count the open session and return a cue the first time a threshold is crossed.
    pub fn poll(&self, caps: Caps, locale: &str) -> Guard {
        let now = Local::now();
        let mut usage = self.lock().clone();
        let before = usage.clone();
        let (next, guard) = evaluate(&usage, caps, locale, now);
        usage = next;
        if usage != before {
            Self::persist(&self.path, &usage);
            *self.lock() = usage;
        }
        guard
    }
}

fn status_of(usage: &Usage, caps: Caps, guard: Guard, now: DateTime<Local>) -> UsageStatus {
    UsageStatus {
        day: usage.day.clone(),
        minutes: guard.minutes,
        sessions: usage.sessions,
        budget_minutes: caps.daily_minutes,
        hard_stop: caps.daily_hard_stop,
        xai_minutes: provider_minutes(usage, "xai", now),
        openai_minutes: provider_minutes(usage, "openai", now),
        local_minutes: provider_minutes(usage, "local", now),
        month: if usage.month.is_empty() { local_month(now) } else { usage.month.clone() },
        month_minutes: guard.month_minutes,
        month_xai_minutes: month_provider_minutes(usage, "xai", now),
        month_openai_minutes: month_provider_minutes(usage, "openai", now),
        month_local_minutes: month_provider_minutes(usage, "local", now),
        monthly_budget_minutes: caps.monthly_minutes,
        monthly_hard_stop: caps.monthly_hard_stop,
        open_since_ms: usage.open_since_ms,
        open_provider: usage.open_provider.clone(),
        message: guard.message,
    }
}

pub fn evaluate(usage: &Usage, caps: Caps, locale: &str, now: DateTime<Local>) -> (Usage, Guard) {
    let mut usage = usage.clone();
    roll(&mut usage, now);
    let minutes = elapsed_minutes(&usage, now);
    let month_minutes = elapsed_month_minutes(&usage, now);
    let mut message = None;
    let mut block = false;
    if caps.monthly_minutes > 0 {
        let over = month_minutes >= caps.monthly_minutes;
        let nearing = month_minutes.saturating_mul(100) >= caps.monthly_minutes.saturating_mul(80);
        if caps.monthly_hard_stop && over {
            block = true;
            if !usage.month_stopped {
                message = Some(fill(&crate::i18n::line(locale, "month-stop"), &[("minutes", month_minutes), ("budget", caps.monthly_minutes)]));
                usage.month_stopped = true;
                usage.month_warned = true;
            }
        } else if over && !usage.month_warned {
            message = Some(fill(&crate::i18n::line(locale, "month-over"), &[("minutes", month_minutes), ("budget", caps.monthly_minutes)]));
            usage.month_warned = true;
        } else if nearing && !usage.month_warned {
            message = Some(fill(&crate::i18n::line(locale, "month-near"), &[("minutes", month_minutes), ("budget", caps.monthly_minutes)]));
            usage.month_warned = true;
        }
    }
    if message.is_none() && caps.daily_minutes > 0 {
        let over = minutes >= caps.daily_minutes;
        let nearing = minutes.saturating_mul(100) >= caps.daily_minutes.saturating_mul(80);
        if caps.daily_hard_stop && over {
            block = true;
            if !usage.stopped {
                message = Some(fill(&crate::i18n::line(locale, "budget-stop"), &[("minutes", minutes), ("budget", caps.daily_minutes)]));
                usage.stopped = true;
                usage.warned = true;
            }
        } else if over && !usage.warned {
            message = Some(fill(&crate::i18n::line(locale, "budget-over"), &[("minutes", minutes), ("budget", caps.daily_minutes)]));
            usage.warned = true;
        } else if nearing && !usage.warned {
            message = Some(fill(&crate::i18n::line(locale, "budget-near"), &[("minutes", minutes), ("budget", caps.daily_minutes)]));
            usage.warned = true;
        }
    }
    (
        usage,
        Guard {
            minutes,
            budget_minutes: caps.daily_minutes,
            hard_stop: caps.daily_hard_stop,
            month_minutes,
            monthly_budget_minutes: caps.monthly_minutes,
            message,
            block,
        },
    )
}

fn fill(template: &str, pairs: &[(&str, u32)]) -> String {
    let mut out = template.to_string();
    for (key, value) in pairs {
        out = out.replace(&format!("{{{key}}}"), &value.to_string());
    }
    out
}

fn elapsed_minutes(usage: &Usage, now: DateTime<Local>) -> u32 {
    let mut ms = usage.session_ms.max(0);
    if let Some(start) = usage.open_since_ms {
        ms += (now.timestamp_millis() - start).max(0);
    }
    (ms / 60_000) as u32
}

fn elapsed_month_minutes(usage: &Usage, now: DateTime<Local>) -> u32 {
    let mut ms = usage.month_ms.max(0);
    if let Some(start) = usage.open_since_ms {
        ms += (now.timestamp_millis() - start).max(0);
    }
    (ms / 60_000) as u32
}

fn provider_ms(usage: &Usage, provider: &str) -> i64 {
    match provider {
        "xai" => usage.xai_ms,
        "openai" => usage.openai_ms,
        "local" => usage.local_ms,
        _ => 0,
    }
}

fn month_provider_ms(usage: &Usage, provider: &str) -> i64 {
    match provider {
        "xai" => usage.month_xai_ms,
        "openai" => usage.month_openai_ms,
        "local" => usage.month_local_ms,
        _ => 0,
    }
}

fn add_provider(usage: &mut Usage, provider: &str, elapsed: i64) {
    match provider {
        "xai" => {
            usage.xai_ms += elapsed;
            usage.month_xai_ms += elapsed;
        }
        "openai" => {
            usage.openai_ms += elapsed;
            usage.month_openai_ms += elapsed;
        }
        "local" => {
            usage.local_ms += elapsed;
            usage.month_local_ms += elapsed;
        }
        _ => {}
    }
}

fn provider_minutes(usage: &Usage, provider: &str, now: DateTime<Local>) -> u32 {
    let mut ms = provider_ms(usage, provider).max(0);
    if usage.open_provider == provider {
        if let Some(start) = usage.open_since_ms {
            ms += (now.timestamp_millis() - start).max(0);
        }
    }
    (ms / 60_000) as u32
}

fn month_provider_minutes(usage: &Usage, provider: &str, now: DateTime<Local>) -> u32 {
    let mut ms = month_provider_ms(usage, provider).max(0);
    if usage.open_provider == provider {
        if let Some(start) = usage.open_since_ms {
            ms += (now.timestamp_millis() - start).max(0);
        }
    }
    (ms / 60_000) as u32
}

fn roll(usage: &mut Usage, now: DateTime<Local>) {
    let day = local_day(now);
    let month = local_month(now);
    if usage.month.is_empty() {
        usage.month = month.clone();
    }
    if usage.day == day && usage.month == month {
        return;
    }
    let open = usage.open_since_ms.is_some();
    let provider = usage.open_provider.clone();
    let mut month_ms = usage.month_ms;
    let mut month_xai_ms = usage.month_xai_ms;
    let mut month_openai_ms = usage.month_openai_ms;
    let mut month_local_ms = usage.month_local_ms;
    let mut month_warned = usage.month_warned;
    let mut month_stopped = usage.month_stopped;
    if usage.month != month {
        month_ms = 0;
        month_xai_ms = 0;
        month_openai_ms = 0;
        month_local_ms = 0;
        month_warned = false;
        month_stopped = false;
    }
    if usage.day != day {
        *usage = Usage {
            day,
            session_ms: 0,
            sessions: u32::from(open),
            warned: false,
            stopped: false,
            open_since_ms: open.then_some(now.timestamp_millis()),
            open_provider: provider,
            xai_ms: 0,
            openai_ms: 0,
            local_ms: 0,
            month,
            month_ms,
            month_xai_ms,
            month_openai_ms,
            month_local_ms,
            month_warned,
            month_stopped,
        };
        return;
    }
    usage.month = month;
    usage.month_ms = month_ms;
    usage.month_xai_ms = month_xai_ms;
    usage.month_openai_ms = month_openai_ms;
    usage.month_local_ms = month_local_ms;
    usage.month_warned = month_warned;
    usage.month_stopped = month_stopped;
}

fn end_open(usage: &mut Usage, now: DateTime<Local>) {
    if let Some(start) = usage.open_since_ms.take() {
        let elapsed = (now.timestamp_millis() - start).max(0);
        usage.session_ms += elapsed;
        usage.month_ms += elapsed;
        add_provider(usage, &usage.open_provider.clone(), elapsed);
    }
}

fn close_orphan(usage: &mut Usage, now: DateTime<Local>) {
    roll(usage, now);
    if let Some(start) = usage.open_since_ms.take() {
        let elapsed = (now.timestamp_millis() - start).clamp(0, 4 * 60 * 60 * 1000);
        usage.session_ms += elapsed;
        usage.month_ms += elapsed;
        add_provider(usage, &usage.open_provider.clone(), elapsed);
    }
}

pub fn local_day(now: DateTime<Local>) -> String {
    now.format("%Y-%m-%d").to_string()
}

pub fn local_month(now: DateTime<Local>) -> String {
    now.format("%Y-%m").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recurrence::local_on;

    fn usage_at(day: &str, minutes: i64, warned: bool) -> Usage {
        Usage {
            day: day.into(),
            session_ms: minutes * 60_000,
            sessions: 1,
            warned,
            stopped: false,
            open_since_ms: None,
            month: day.chars().take(7).collect(),
            month_ms: minutes * 60_000,
            ..Usage::default()
        }
    }

    fn daily(minutes: u32, hard: bool) -> Caps {
        Caps { daily_minutes: minutes, daily_hard_stop: hard, monthly_minutes: 0, monthly_hard_stop: false }
    }

    #[test]
    fn soft_budget_warns_once_and_hard_stop_blocks() {
        let now = local_on(2026, 3, 3, 12, 0);
        let day = local_day(now);
        let (under, guard) = evaluate(&usage_at(&day, 10, false), daily(30, false), "en", now);
        assert!(guard.message.is_none());
        assert!(!guard.block);
        assert!(!under.warned);

        let (near, guard) = evaluate(&usage_at(&day, 24, false), daily(30, false), "en", now);
        assert!(guard.message.unwrap().contains("24"));
        assert!(near.warned);
        let (_, again) = evaluate(&near, daily(30, false), "en", now);
        assert!(again.message.is_none());

        let (_, passed) = evaluate(&usage_at(&day, 30, false), daily(30, false), "en", now);
        assert!(passed.message.unwrap().to_lowercase().contains("passed"));
        assert!(!passed.block);

        let (stopped, guard) = evaluate(&usage_at(&day, 30, false), daily(30, true), "en", now);
        assert!(guard.block);
        assert!(guard.message.unwrap().contains("stops"));
        let (_, second) = evaluate(&stopped, daily(30, true), "en", now);
        assert!(second.block);
        assert!(second.message.is_none());

        let (_, off) = evaluate(&usage_at(&day, 500, false), daily(0, true), "en", now);
        assert!(off.message.is_none());
        assert!(!off.block);
    }

    #[test]
    fn day_boundary_resets_and_open_session_counts() {
        let monday = local_on(2026, 3, 2, 23, 50);
        let tuesday = local_on(2026, 3, 3, 0, 10);
        let mut usage = usage_at(&local_day(monday), 40, true);
        usage.open_since_ms = Some(monday.timestamp_millis());
        let (rolled, guard) = evaluate(&usage, daily(30, false), "en", tuesday);
        assert_eq!(rolled.day, local_day(tuesday));
        assert!(!rolled.warned);
        assert_eq!(guard.minutes, 0);

        let mut open = usage_at(&local_day(monday), 20, false);
        open.open_since_ms = Some((monday - chrono::Duration::minutes(10)).timestamp_millis());
        let (_, guard) = evaluate(&open, daily(30, false), "en", monday);
        assert_eq!(guard.minutes, 30);
        assert!(guard.message.unwrap().to_lowercase().contains("passed"));
    }

    #[test]
    fn monthly_cap_blocks_failover_spend_and_providers_split() {
        let now = local_on(2026, 3, 3, 12, 0);
        let day = local_day(now);
        let mut usage = usage_at(&day, 10, false);
        usage.month_ms = 100 * 60_000;
        let caps = Caps { daily_minutes: 0, daily_hard_stop: false, monthly_minutes: 100, monthly_hard_stop: true };
        let (stopped, guard) = evaluate(&usage, caps, "en", now);
        assert!(guard.block);
        assert!(guard.message.unwrap().contains("month"));
        let (_, again) = evaluate(&stopped, caps, "en", now);
        assert!(again.block);
        assert!(again.message.is_none());

        let path = std::env::temp_dir().join(format!("grok-usage-split-{}.json", std::process::id()));
        let _ = fs::remove_file(&path);
        let store = UsageStore::load(path.clone());
        store.begin();
        store.note_provider("xai");
        store.note_provider("openai");
        store.end();
        let status = store.status(Caps { daily_minutes: 0, daily_hard_stop: false, monthly_minutes: 0, monthly_hard_stop: false }, "en");
        assert_eq!(status.sessions, 1);
        assert!(status.xai_minutes + status.openai_minutes + status.local_minutes <= status.minutes + 1);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn store_round_trip_resets_after_end() {
        let path = std::env::temp_dir().join(format!("grok-usage-{}.json", std::process::id()));
        let _ = fs::remove_file(&path);
        let store = UsageStore::load(path.clone());
        store.begin();
        store.end();
        let loaded = UsageStore::load(path.clone());
        let status = loaded.status(Caps { daily_minutes: 0, daily_hard_stop: false, monthly_minutes: 0, monthly_hard_stop: false }, "en");
        assert_eq!(status.sessions, 1);
        assert_eq!(status.minutes, 0);
        let _ = fs::remove_file(&path);
    }
}
