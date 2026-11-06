//! Pinned facts and short summaries of recent on-device Talk sessions.
//! This bundle is injected only into local Talk. Cloud sessions do not receive it.
//! It is not the 12-hour turn log in `memory`, and it never stores raw audio or a full transcript.

use std::{
    fs,
    path::PathBuf,
    sync::Mutex,
};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tauri::{AppHandle, Manager};

use crate::diagnostics::now_ms;

pub const MAX_FACTS: usize = 10;
pub const MAX_FACT_CHARS: usize = 180;
pub const MAX_SESSIONS: usize = 3;
pub const MAX_SUMMARY_CHARS: usize = 360;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Fact {
    pub id: String,
    pub text: String,
    pub updated_ms: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionNote {
    pub id: String,
    pub at_ms: i64,
    pub text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Bundle {
    #[serde(default)]
    pub facts: Vec<Fact>,
    #[serde(default)]
    pub sessions: Vec<SessionNote>,
    #[serde(default)]
    pub next_id: u32,
}

#[derive(Clone, Debug)]
pub struct ExchangeTurn {
    pub role: String,
    pub text: String,
}

pub struct LocalMemory {
    path: PathBuf,
    inner: Mutex<Bundle>,
}

impl LocalMemory {
    pub fn load(path: PathBuf) -> Self {
        let mut bundle = fs::read(&path).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default();
        normalize(&mut bundle);
        let store = Self { path, inner: Mutex::new(bundle) };
        store.persist();
        store
    }

    pub fn snapshot(&self) -> Bundle {
        self.lock().clone()
    }

    pub fn replace(&self, mut bundle: Bundle) {
        normalize(&mut bundle);
        *self.lock() = bundle;
        self.persist();
    }

    /// Clears pinned facts and local session summaries. Notes, People, and vault keys are other files.
    pub fn wipe(&self) -> Bundle {
        *self.lock() = Bundle::default();
        self.persist();
        Bundle::default()
    }

    pub fn remember(&self, id: Option<&str>, text: &str, locale: &str) -> Result<String, String> {
        let text = clip(&squash(text), MAX_FACT_CHARS);
        if text.is_empty() {
            return Err(crate::i18n::line(locale, "fact-empty"));
        }
        let mut bundle = self.lock();
        if let Some(id) = id.map(str::trim).filter(|id| !id.is_empty()) {
            let Some(fact) = bundle.facts.iter_mut().find(|fact| fact.id == id) else {
                return Err(crate::i18n::line(locale, "fact-missing"));
            };
            fact.text = text.clone();
            fact.updated_ms = now_ms() as i64;
            drop(bundle);
            self.persist();
            return Ok(crate::i18n::line(locale, "fact-updated").replace("{text}", &text));
        }
        let key = norm(&text);
        if let Some(fact) = bundle.facts.iter_mut().find(|fact| norm(&fact.text) == key) {
            fact.text = text.clone();
            fact.updated_ms = now_ms() as i64;
            drop(bundle);
            self.persist();
            return Ok(crate::i18n::line(locale, "fact-updated").replace("{text}", &text));
        }
        if bundle.facts.len() >= MAX_FACTS {
            return Err(crate::i18n::line(locale, "fact-full"));
        }
        let id = alloc_id(&mut bundle, "f");
        bundle.facts.push(Fact { id, text: text.clone(), updated_ms: now_ms() as i64 });
        drop(bundle);
        self.persist();
        Ok(crate::i18n::line(locale, "fact-remembered").replace("{text}", &text))
    }

    pub fn forget(&self, id: Option<&str>, text: Option<&str>, locale: &str) -> Result<String, String> {
        let mut bundle = self.lock();
        let id = id.map(str::trim).filter(|id| !id.is_empty()).map(str::to_string);
        let query = text.map(squash).filter(|text| !text.is_empty());
        let index = if let Some(id) = id.as_deref() {
            bundle.facts.iter().position(|fact| fact.id == id)
        } else if let Some(query) = query.as_deref() {
            let key = norm(query);
            let exact: Vec<usize> = bundle.facts.iter().enumerate().filter(|(_, fact)| norm(&fact.text) == key).map(|(i, _)| i).collect();
            if exact.len() == 1 {
                Some(exact[0])
            } else {
                let hits: Vec<usize> = bundle
                    .facts
                    .iter()
                    .enumerate()
                    .filter(|(_, fact)| norm(&fact.text).contains(&key))
                    .map(|(i, _)| i)
                    .collect();
                if hits.len() == 1 { Some(hits[0]) } else if hits.len() > 1 { return Err(crate::i18n::line(locale, "fact-which")) } else { None }
            }
        } else {
            return Err(crate::i18n::line(locale, "fact-which"));
        };
        let Some(index) = index else {
            return Err(crate::i18n::line(locale, "fact-missing"));
        };
        let removed = bundle.facts.remove(index);
        drop(bundle);
        self.persist();
        Ok(crate::i18n::line(locale, "fact-forgot").replace("{text}", &removed.text))
    }

    pub fn note_exchange(&self, turns: &[ExchangeTurn]) {
        let Some(text) = summarize_exchange(turns) else { return };
        let mut bundle = self.lock();
        if bundle.sessions.last().is_some_and(|note| note.text == text) {
            if let Some(note) = bundle.sessions.last_mut() {
                note.at_ms = now_ms() as i64;
            }
        } else {
            let id = alloc_id(&mut bundle, "s");
            bundle.sessions.push(SessionNote { id, at_ms: now_ms() as i64, text });
            if bundle.sessions.len() > MAX_SESSIONS {
                let drop_n = bundle.sessions.len() - MAX_SESSIONS;
                bundle.sessions.drain(..drop_n);
            }
        }
        drop(bundle);
        self.persist();
    }

    /// Block for the local model only. Empty when there is nothing to say.
    pub fn prompt(&self) -> String {
        let bundle = self.lock();
        if bundle.facts.is_empty() && bundle.sessions.is_empty() {
            return String::new();
        }
        let mut out = String::from(
            "\n\nOn-device memory for this phone only. Use it when it answers the question. Do not recite it unasked. Pinned facts are short lines the user asked to keep. Recent local sessions are a few sentences, not transcripts. remember_fact adds or updates one. forget_fact removes one and does not delete notes, People, reminders, or events.\n",
        );
        if !bundle.facts.is_empty() {
            out.push_str("Pinned facts:\n");
            for fact in &bundle.facts {
                out.push_str(&format!("- {}: {}\n", fact.id, fact.text));
            }
        }
        if !bundle.sessions.is_empty() {
            out.push_str("Recent local sessions:\n");
            for note in &bundle.sessions {
                out.push_str(&format!("- {}\n", note.text));
            }
        }
        out
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Bundle> {
        self.inner.lock().unwrap_or_else(|err| err.into_inner())
    }

    fn persist(&self) {
        let bundle = self.lock();
        if let Some(dir) = self.path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        if let Ok(bytes) = serde_json::to_vec_pretty(&*bundle) {
            let _ = fs::write(&self.path, bytes);
        }
    }
}

pub fn tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "type": "function",
            "name": "remember_fact",
            "description": "Pin or update one short fact the user asked to remember. text is one sentence. id changes that fact. Then confirm in one short line.",
            "parameters": {
                "type": "object",
                "properties": { "text": { "type": "string" }, "id": { "type": "string" } },
                "required": ["text"]
            }
        }),
        json!({
            "type": "function",
            "name": "forget_fact",
            "description": "Forget one pinned fact by id or text. Does not delete notes, People, reminders, or events. Then confirm in one short line.",
            "parameters": {
                "type": "object",
                "properties": { "id": { "type": "string" }, "text": { "type": "string" } }
            }
        }),
    ]
}

pub fn tool_names() -> Vec<String> {
    tool_definitions().iter().filter_map(|tool| tool["name"].as_str().map(str::to_string)).collect()
}

pub fn run_tool(app: &AppHandle, name: &str, args: &Value) -> Option<String> {
    if name != "remember_fact" && name != "forget_fact" {
        return None;
    }
    let locale = app.state::<crate::settings::SettingsStore>().get().locale;
    let store = app.state::<LocalMemory>();
    let result = match name {
        "remember_fact" => store.remember(args["id"].as_str(), args["text"].as_str().unwrap_or(""), &locale),
        "forget_fact" => store.forget(args["id"].as_str(), args["text"].as_str(), &locale),
        _ => return None,
    };
    let out = match result {
        Ok(detail) => json!({ "ok": true, "detail": detail }).to_string(),
        Err(error) => json!({ "ok": false, "error": error }).to_string(),
    };
    if let Some(hub) = app.try_state::<std::sync::Arc<crate::diagnostics::Hub>>() {
        let brief = serde_json::from_str::<Value>(&out)
            .ok()
            .and_then(|value| value["detail"].as_str().or(value["error"].as_str()).map(str::to_string))
            .unwrap_or_else(|| "done".into());
        hub.info(format!("tool {name}: {brief}"));
    }
    Some(out)
}

pub fn record_exchange(app: &AppHandle, turns: &[ExchangeTurn]) {
    app.state::<LocalMemory>().note_exchange(turns);
}

pub fn summarize_exchange(turns: &[ExchangeTurn]) -> Option<String> {
    let mut users = Vec::new();
    let mut replies = Vec::new();
    for turn in turns {
        let text = squash(&turn.text);
        if text.is_empty() || skip_turn(&turn.role, &text) {
            continue;
        }
        if turn.role == "assistant" {
            replies.push(text);
        } else {
            users.push(text);
        }
    }
    if users.is_empty() || replies.is_empty() {
        return None;
    }
    let mut sentences = Vec::new();
    sentences.push(format!("You said {}.", clip(&users[0], 120)));
    if users.len() > 1 {
        let last = users.last().unwrap();
        if last != &users[0] {
            sentences.push(format!("Later you said {}.", clip(last, 100)));
        }
    }
    sentences.push(format!("The reply was {}.", clip(replies.last().unwrap(), 140)));
    let summary = clip(&sentences.join(" "), MAX_SUMMARY_CHARS);
    if summary.chars().count() < 12 { None } else { Some(summary) }
}

fn skip_turn(role: &str, text: &str) -> bool {
    if role == "assistant" {
        return text.starts_with('{') && text.contains("\"tool\"");
    }
    text.starts_with("Tool ") || text.starts_with("That was not usable JSON") || text.starts_with("Typed just now.")
}

fn normalize(bundle: &mut Bundle) {
    bundle.facts.retain(|fact| !squash(&fact.text).is_empty());
    for fact in &mut bundle.facts {
        fact.text = clip(&squash(&fact.text), MAX_FACT_CHARS);
    }
    if bundle.facts.len() > MAX_FACTS {
        bundle.facts.sort_by_key(|fact| fact.updated_ms);
        let drop_n = bundle.facts.len() - MAX_FACTS;
        bundle.facts.drain(..drop_n);
    }
    bundle.sessions.retain(|note| !squash(&note.text).is_empty());
    for note in &mut bundle.sessions {
        note.text = clip(&squash(&note.text), MAX_SUMMARY_CHARS);
    }
    if bundle.sessions.len() > MAX_SESSIONS {
        let drop_n = bundle.sessions.len() - MAX_SESSIONS;
        bundle.sessions.drain(..drop_n);
    }
    let mut max_id = bundle.next_id;
    for fact in &bundle.facts {
        max_id = max_id.max(numeric_id(&fact.id));
    }
    for note in &bundle.sessions {
        max_id = max_id.max(numeric_id(&note.id));
    }
    bundle.next_id = max_id;
}

fn alloc_id(bundle: &mut Bundle, prefix: &str) -> String {
    bundle.next_id = bundle.next_id.saturating_add(1).max(1);
    format!("{prefix}{}", bundle.next_id)
}

fn numeric_id(id: &str) -> u32 {
    id.chars().skip_while(|ch| !ch.is_ascii_digit()).collect::<String>().parse().unwrap_or(0)
}

fn squash(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn norm(text: &str) -> String {
    squash(text).to_lowercase()
}

fn clip(text: &str, max: usize) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= max {
        return trimmed.to_string();
    }
    let mut out: String = trimmed.chars().take(max.saturating_sub(1)).collect();
    if let Some(space) = out.rfind(' ') {
        if space > max / 2 {
            out.truncate(space);
        }
    }
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> LocalMemory {
        let path = std::env::temp_dir().join(format!("gv-local-mem-{}-{}.json", std::process::id(), now_ms()));
        let _ = fs::remove_file(&path);
        LocalMemory::load(path)
    }

    #[test]
    fn facts_cap_at_ten_and_updates_do_not_grow() {
        let store = store();
        for n in 1..=MAX_FACTS {
            store.remember(None, &format!("fact {n}"), "en").unwrap();
        }
        assert_eq!(store.snapshot().facts.len(), MAX_FACTS);
        let err = store.remember(None, "one more fact", "en").unwrap_err();
        assert!(err.contains("10"));
        let id = store.snapshot().facts[0].id.clone();
        store.remember(Some(&id), "I live in Zurich", "en").unwrap();
        assert_eq!(store.snapshot().facts.len(), MAX_FACTS);
        assert_eq!(store.snapshot().facts.iter().find(|fact| fact.id == id).unwrap().text, "I live in Zurich");
        store.forget(Some(&id), None, "en").unwrap();
        assert_eq!(store.snapshot().facts.len(), MAX_FACTS - 1);
    }

    #[test]
    fn session_notes_keep_the_last_three_and_are_not_transcripts() {
        let store = store();
        for n in 0..5 {
            let turns = vec![
                ExchangeTurn { role: "user".into(), text: format!("question {n} about the train") },
                ExchangeTurn { role: "assistant".into(), text: r#"{"say":"ok","tool":"create_note","args":{}}"#.into() },
                ExchangeTurn { role: "user".into(), text: "Tool create_note result: saved".into() },
                ExchangeTurn { role: "assistant".into(), text: format!("The train answer is {n}.") },
            ];
            store.note_exchange(&turns);
        }
        let notes = store.snapshot().sessions;
        assert_eq!(notes.len(), MAX_SESSIONS);
        assert!(notes.iter().all(|note| note.text.contains("You said") && note.text.contains("The reply was")));
        assert!(notes.iter().all(|note| !note.text.contains("Tool create_note")));
        assert!(notes.last().unwrap().text.contains("question 4"));
        assert!(store.prompt().len() < 4096);
    }

    #[test]
    fn wipe_clears_only_this_store() {
        let store = store();
        store.remember(None, "I live in Zurich", "en").unwrap();
        store.note_exchange(&[
            ExchangeTurn { role: "user".into(), text: "hello".into() },
            ExchangeTurn { role: "assistant".into(), text: "hello there".into() },
        ]);
        let wiped = store.wipe();
        assert!(wiped.facts.is_empty());
        assert!(wiped.sessions.is_empty());
        assert!(store.prompt().is_empty());
    }

    #[test]
    fn a_one_sided_exchange_is_not_stored() {
        assert!(summarize_exchange(&[ExchangeTurn { role: "user".into(), text: "only me".into() }]).is_none());
    }

    #[test]
    fn local_tools_are_not_named_like_cloud_phone_tools() {
        let names = tool_names();
        assert_eq!(names, vec!["remember_fact".to_string(), "forget_fact".to_string()]);
    }
}
