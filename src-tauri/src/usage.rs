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
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Guard {
    pub minutes: u32,
    pub budget_minutes: u32,
    pub hard_stop: bool,
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
            let _ = fs::write(path, bytes);
        }
    }

    pub fn status(&self, budget_minutes: u32, hard_stop: bool) -> UsageStatus {
        let now = Local::now();
        let mut usage = self.lock().clone();
        let (next, guard) = evaluate(&usage, budget_minutes, hard_stop, now);
        if next.day != usage.day {
            usage = next;
            Self::persist(&self.path, &usage);
            *self.lock() = usage.clone();
        }
        UsageStatus {
            day: usage.day,
            minutes: guard.minutes,
            sessions: usage.sessions,
            budget_minutes,
            hard_stop,
            message: guard.message,
        }
    }

    /// True when a new session must not start.
    pub fn blocked(&self, budget_minutes: u32, hard_stop: bool) -> Option<Guard> {
        let (usage, guard) = evaluate(&self.lock(), budget_minutes, hard_stop, Local::now());
        if usage.day != self.lock().day {
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
        Self::persist(&self.path, &usage);
        *self.lock() = usage;
    }

    pub fn end(&self) {
        let now = Local::now();
        let mut usage = self.lock().clone();
        roll(&mut usage, now);
        end_open(&mut usage, now);
        Self::persist(&self.path, &usage);
        *self.lock() = usage;
    }

    /// Count the open session and return a cue the first time a threshold is crossed.
    pub fn poll(&self, budget_minutes: u32, hard_stop: bool) -> Guard {
        let now = Local::now();
        let mut usage = self.lock().clone();
        let before = usage.clone();
        let (next, guard) = evaluate(&usage, budget_minutes, hard_stop, now);
        usage = next;
        if usage != before {
            Self::persist(&self.path, &usage);
            *self.lock() = usage;
        }
        guard
    }
}

pub fn evaluate(usage: &Usage, budget_minutes: u32, hard_stop: bool, now: DateTime<Local>) -> (Usage, Guard) {
    let mut usage = usage.clone();
    roll(&mut usage, now);
    let minutes = elapsed_minutes(&usage, now);
    let mut message = None;
    let mut block = false;
    if budget_minutes > 0 {
        let over = minutes >= budget_minutes;
        let nearing = minutes.saturating_mul(100) >= budget_minutes.saturating_mul(80);
        if hard_stop && over {
            block = true;
            if !usage.stopped {
                message = Some(format!(
                    "You've reached today's voice limit of {budget_minutes} minutes, so this call stops."
                ));
                usage.stopped = true;
                usage.warned = true;
            }
        } else if over && !usage.warned {
            message = Some(format!("You've passed today's soft voice budget of {budget_minutes} minutes."));
            usage.warned = true;
        } else if nearing && !usage.warned {
            message = Some(format!(
                "You've used about {minutes} minutes of today's {budget_minutes}-minute voice budget."
            ));
            usage.warned = true;
        }
    }
    (
        usage,
        Guard { minutes, budget_minutes, hard_stop, message, block },
    )
}

fn elapsed_minutes(usage: &Usage, now: DateTime<Local>) -> u32 {
    let mut ms = usage.session_ms.max(0);
    if let Some(start) = usage.open_since_ms {
        ms += (now.timestamp_millis() - start).max(0);
    }
    (ms / 60_000) as u32
}

fn roll(usage: &mut Usage, now: DateTime<Local>) {
    let day = local_day(now);
    if usage.day == day {
        return;
    }
    let open = usage.open_since_ms.is_some();
    *usage = Usage {
        day,
        session_ms: 0,
        sessions: u32::from(open),
        warned: false,
        stopped: false,
        open_since_ms: open.then_some(now.timestamp_millis()),
    };
}

fn end_open(usage: &mut Usage, now: DateTime<Local>) {
    if let Some(start) = usage.open_since_ms.take() {
        usage.session_ms += (now.timestamp_millis() - start).max(0);
    }
}

fn close_orphan(usage: &mut Usage, now: DateTime<Local>) {
    roll(usage, now);
    if let Some(start) = usage.open_since_ms.take() {
        let elapsed = (now.timestamp_millis() - start).clamp(0, 4 * 60 * 60 * 1000);
        usage.session_ms += elapsed;
    }
}

pub fn local_day(now: DateTime<Local>) -> String {
    now.format("%Y-%m-%d").to_string()
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
        }
    }

    #[test]
    fn soft_budget_warns_once_and_hard_stop_blocks() {
        let now = local_on(2026, 3, 3, 12, 0);
        let day = local_day(now);
        let (under, guard) = evaluate(&usage_at(&day, 10, false), 30, false, now);
        assert!(guard.message.is_none());
        assert!(!guard.block);
        assert!(!under.warned);

        let (near, guard) = evaluate(&usage_at(&day, 24, false), 30, false, now);
        assert!(guard.message.unwrap().contains("24 minutes"));
        assert!(near.warned);
        let (_, again) = evaluate(&near, 30, false, now);
        assert!(again.message.is_none());

        let (_, passed) = evaluate(&usage_at(&day, 30, false), 30, false, now);
        assert!(passed.message.unwrap().contains("passed"));
        assert!(!passed.block);

        let (stopped, guard) = evaluate(&usage_at(&day, 30, false), 30, true, now);
        assert!(guard.block);
        assert!(guard.message.unwrap().contains("stops"));
        let (_, second) = evaluate(&stopped, 30, true, now);
        assert!(second.block);
        assert!(second.message.is_none());

        let (_, off) = evaluate(&usage_at(&day, 500, false), 0, true, now);
        assert!(off.message.is_none());
        assert!(!off.block);
    }

    #[test]
    fn day_boundary_resets_and_open_session_counts() {
        let monday = local_on(2026, 3, 2, 23, 50);
        let tuesday = local_on(2026, 3, 3, 0, 10);
        let mut usage = usage_at(&local_day(monday), 40, true);
        usage.open_since_ms = Some(monday.timestamp_millis());
        let (rolled, guard) = evaluate(&usage, 30, false, tuesday);
        assert_eq!(rolled.day, local_day(tuesday));
        assert!(!rolled.warned);
        assert_eq!(guard.minutes, 0);

        let mut open = usage_at(&local_day(monday), 20, false);
        open.open_since_ms = Some((monday - chrono::Duration::minutes(10)).timestamp_millis());
        let (_, guard) = evaluate(&open, 30, false, monday);
        assert_eq!(guard.minutes, 30);
        assert!(guard.message.unwrap().contains("passed"));
    }

    #[test]
    fn store_round_trip_resets_after_end() {
        let path = std::env::temp_dir().join(format!("grok-usage-{}.json", std::process::id()));
        let _ = fs::remove_file(&path);
        let store = UsageStore::load(path.clone());
        store.begin();
        store.end();
        let loaded = UsageStore::load(path.clone());
        let status = loaded.status(0, false);
        assert_eq!(status.sessions, 1);
        assert_eq!(status.minutes, 0);
        let _ = fs::remove_file(&path);
    }
}
