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
        let msg = msg.into();
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
