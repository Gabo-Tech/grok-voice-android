//! Short memory of recent talk. Kept on the phone for a few hours and trimmed
//! before it is added to a new voice session, so the next “Hey Grok” is not blank
//! and the request stays small. Longer facts belong in notes, reminders, or events.

use std::{
    fs,
    path::PathBuf,
    sync::Mutex,
};

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

use crate::diagnostics::now_ms;

const KEEP_MS: i64 = 12 * 60 * 60 * 1000;
const MAX_TURNS: usize = 12;
const MAX_CHARS: usize = 160;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Turn {
    pub at_ms: i64,
    pub role: String,
    pub text: String,
}

pub struct Memory {
    path: PathBuf,
    turns: Mutex<Vec<Turn>>,
}

impl Memory {
    pub fn load(path: PathBuf) -> Self {
        let turns = fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        let memory = Self {
            path,
            turns: Mutex::new(turns),
        };
        memory.prune_and_save();
        memory
    }

    pub fn remember(&self, role: &str, text: &str) {
        let text = squash(text);
        if text.is_empty() {
            return;
        }
        let mut turns = self.turns.lock().unwrap_or_else(|e| e.into_inner());
        if turns.last().is_some_and(|turn| turn.role == role && turn.text == text) {
            return;
        }
        turns.push(Turn {
            at_ms: now_ms() as i64,
            role: role.to_string(),
            text,
        });
        drop(turns);
        self.prune_and_save();
    }

    pub fn recent(&self) -> Vec<Turn> {
        self.prune_and_save();
        self.turns.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn clear(&self) {
        if let Ok(mut turns) = self.turns.lock() {
            turns.clear();
        }
        let _ = fs::write(&self.path, b"[]");
    }

    /// A short block for the next voice session. Empty when there is nothing recent.
    pub fn prompt(&self) -> String {
        let turns = self.recent();
        if turns.is_empty() {
            return String::new();
        }
        let mut out = String::from(
            "\n\nRecent spoken and typed talk on this phone from the last 12 hours, newest last. It is one shared memory. Use it only when the user refers to something already said. Do not recite it. To keep something longer, save a note, reminder, or calendar event.\n",
        );
        for turn in turns {
            let stamp = DateTime::from_timestamp_millis(turn.at_ms)
                .map(|time| time.with_timezone(&Local).format("%H:%M").to_string())
                .unwrap_or_default();
            out.push_str(&format!("[{stamp}] {}: {}\n", turn.role, turn.text));
        }
        out
    }

    fn prune_and_save(&self) {
        let cutoff = (now_ms() as i64).saturating_sub(KEEP_MS);
        let mut turns = self.turns.lock().unwrap_or_else(|e| e.into_inner());
        turns.retain(|turn| turn.at_ms >= cutoff && !turn.text.is_empty());
        if turns.len() > MAX_TURNS {
            let drop_n = turns.len() - MAX_TURNS;
            turns.drain(..drop_n);
        }
        if let Ok(bytes) = serde_json::to_vec_pretty(&*turns) {
            let _ = fs::write(&self.path, bytes);
        }
    }
}

fn squash(text: &str) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    flat.chars().take(MAX_CHARS).collect()
}
