//! Central event hub: phase changes, debug counters and a bounded log ring that the
//! frontend debug panel renders.

use std::{
    collections::VecDeque,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::Serialize;
use tauri::{AppHandle, Emitter};

const MAX_LOGS: usize = 300;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    #[default]
    Idle,
    Wake,
    Recording,
    Connecting,
    Listening,
    Thinking,
    Speaking,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DebugSnapshot {
    pub phase: Phase,
    pub wake_enabled: bool,
    pub wakeword_trained: bool,
    pub ws_state: String,
    pub model: Option<String>,
    pub token_minted_at_ms: Option<u64>,
    pub token_expires_at: Option<u64>,
    pub frames_sent: u64,
    pub bytes_sent: u64,
    pub frames_dropped: u64,
    pub audio_deltas: u64,
    pub bytes_received: u64,
    pub events_received: u64,
    pub last_event: Option<String>,
    pub last_error: Option<String>,
    pub mic_rate: Option<u32>,
    pub speaker_rate: Option<u32>,
    pub last_wake_score: Option<f32>,
    pub sessions: u64,
    /// Talk badge: Local, xAI, or OpenAI. Empty when no session is up.
    pub provider: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogLine {
    pub ts_ms: u64,
    pub level: &'static str,
    pub msg: String,
}

pub struct Hub {
    app: AppHandle,
    snap: Mutex<DebugSnapshot>,
    logs: Mutex<VecDeque<LogLine>>,
}

/// Strip credential-shaped text before it reaches logcat, the debug panel, or speech.
pub fn redact(msg: &str) -> String {
    let mut out = String::with_capacity(msg.len().min(400));
    let mut rest = msg;
    while !rest.is_empty() {
        let lower = rest.to_ascii_lowercase();
        let bearer = lower.find("bearer ");
        let sk = find_key(&lower, "sk-");
        let xai = find_key(&lower, "xai-");
        let Some(at) = [bearer, sk, xai].into_iter().flatten().min() else {
            push_limited(&mut out, rest);
            break;
        };
        push_limited(&mut out, &rest[..at]);
        if out.chars().count() >= 400 {
            break;
        }
        if bearer == Some(at) {
            let after = at + "bearer ".len();
            let end = rest[after..].find(char::is_whitespace).map(|n| after + n).unwrap_or(rest.len());
            push_limited(&mut out, "Bearer [redacted]");
            rest = &rest[end..];
        } else {
            let prefix = if sk == Some(at) { 3 } else { 4 };
            let after = at + prefix;
            let token_bytes: usize = rest[after..]
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
                .map(|c| c.len_utf8())
                .sum();
            push_limited(&mut out, "[redacted]");
            rest = &rest[after + token_bytes..];
        }
    }
    out
}

fn find_key(lower: &str, prefix: &str) -> Option<usize> {
    let mut from = 0;
    while let Some(rel) = lower[from..].find(prefix) {
        let at = from + rel;
        let after = at + prefix.len();
        let n = lower[after..]
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
            .count();
        if n >= 8 {
            return Some(at);
        }
        from = after.max(from + 1);
    }
    None
}

fn push_limited(out: &mut String, text: &str) {
    for ch in text.chars() {
        if out.chars().count() >= 400 {
            break;
        }
        out.push(ch);
    }
}

pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

impl Hub {
    pub fn new(app: AppHandle) -> Self {
        Self {
            app,
            snap: Mutex::new(DebugSnapshot { ws_state: "closed".into(), ..Default::default() }),
            logs: Mutex::new(VecDeque::with_capacity(MAX_LOGS)),
        }
    }

    pub fn emit<S: Serialize + Clone>(&self, event: &str, payload: S) {
        let _ = self.app.emit(event, payload);
    }

    fn push(&self, level: &'static str, msg: String) {
        let msg = redact(&msg);
        match level {
            "error" => log::error!("{msg}"),
            "warn" => log::warn!("{msg}"),
            _ => log::info!("{msg}"),
        }
        let line = LogLine { ts_ms: now_ms(), level, msg };
        {
            let mut logs = self.logs.lock().unwrap();
            if logs.len() == MAX_LOGS {
                logs.pop_front();
            }
            logs.push_back(line.clone());
        }
        self.emit("voice://log", line);
    }

    pub fn info(&self, msg: impl Into<String>) {
        self.push("info", msg.into());
    }

    pub fn warn(&self, msg: impl Into<String>) {
        self.push("warn", msg.into());
    }

    pub fn error(&self, msg: impl Into<String>) {
        let msg = redact(&msg.into());
        self.update(|s| s.last_error = Some(msg.clone()));
        self.push("error", msg);
    }

    pub fn set_phase(&self, phase: Phase) {
        let changed = {
            let mut s = self.snap.lock().unwrap();
            let changed = s.phase != phase;
            s.phase = phase;
            changed
        };
        if changed {
            self.emit("voice://phase", phase);
        }
    }

    pub fn update(&self, f: impl FnOnce(&mut DebugSnapshot)) {
        f(&mut self.snap.lock().unwrap());
    }

    pub fn snapshot(&self) -> DebugSnapshot {
        self.snap.lock().unwrap().clone()
    }

    pub fn logs(&self) -> Vec<LogLine> {
        self.logs.lock().unwrap().iter().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_strips_credential_shapes_and_leaves_ordinary_text() {
        let line = redact("OpenAI rejected the request (401): bad sk-proj-abcdefghij123456 key");
        assert!(line.contains("401"));
        assert!(!line.contains("abcdefghij"));
        assert!(line.contains("[redacted]"));
        assert_eq!(redact("using xAI"), "using xAI");
        assert!(redact("Authorization: Bearer super-secret-token").contains("[redacted]"));
        assert!(!redact("Authorization: Bearer super-secret-token").to_lowercase().contains("super-secret"));
    }
}
