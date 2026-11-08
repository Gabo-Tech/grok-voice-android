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
/// A typed or transcribed turn has to reach the model intact. 160 cut a
/// calendar list off at "On the 7" while the bubble still showed the rest.
const MAX_CHARS: usize = 8_000;
/// The next voice session only needs a reminder of earlier turns.
const PROMPT_TURN_CHARS: usize = 2_000;

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
        let text = for_model(text);
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
        let _ = crate::files::write_private(&self.path, b"[]");
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
            let body = turn.text.split_whitespace().collect::<Vec<_>>().join(" ");
            out.push_str(&format!("[{stamp}] {}: {}\n", turn.role, cap(&body, PROMPT_TURN_CHARS)));
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
            let _ = crate::files::write_private(&self.path, &bytes);
        }
    }
}

/// The text a model should see for this turn. Internal newlines stay, so a
/// list is not flattened into a snippet, and the cap is far past one sentence.
pub fn for_model(text: &str) -> String {
    cap(text.trim(), MAX_CHARS)
}

fn cap(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_turn_is_not_cut_at_on_the_7() {
        let text = "Can you create these recurring monthly events? You have seven events this month, all monthly repeats, running from November 1 through November 30, 2026. On the 7th, renew your travel card from 7:00 to 18:00. On the 10th, pay rent of 2080 CHF and receive salary, both from 7:00 to 19:00. On the 29th, the ZKB credit bill from 7:00 to 19:00.";
        assert!(text.chars().count() > 160);
        let old_clip: String = text.chars().take(160).collect();
        assert!(!old_clip.contains("ZKB"), "the old 160-character clip was {old_clip}");
        assert!(old_clip.contains("On the "));
        let kept = for_model(text);
        assert!(kept.contains("On the 29th, the ZKB credit bill"));
        assert!(kept.contains("2080 CHF"));
        let dir = std::env::temp_dir().join(format!("gv-mem-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let memory = Memory::load(dir.join("memory.json"));
        memory.remember("user", text);
        let recent = memory.recent();
        assert_eq!(recent.len(), 1);
        assert!(recent[0].text.contains("ZKB"));
        assert!(recent[0].text.contains("On the 29th"));
        let prompt = memory.prompt();
        assert!(prompt.contains("ZKB"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
