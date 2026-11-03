//! Phone actions the voice session can call: notes, reminders, alarms, timers, calendar.
//! Notes, reminders, and local calendar events live in `agenda.json`. Ringing is Android.

use std::{
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use chrono::{DateTime, Duration, Local, NaiveDate, NaiveDateTime};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tauri::{AppHandle, Manager};

use crate::{diagnostics::Hub, native::NativeBridge};

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    pub id: String,
    #[serde(default)]
    pub title: String,
    pub text: String,
    pub created_ms: i64,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: i32,
    pub kind: String,
    pub title: String,
    pub note: String,
    pub at_ms: i64,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalEvent {
    #[serde(default)]
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub start_ms: i64,
    #[serde(default)]
    pub end_ms: i64,
    #[serde(default)]
    pub location: String,
    #[serde(default)]
    pub all_day: bool,
    #[serde(default = "phone_source")]
    pub source: String,
}

fn phone_source() -> String {
    "phone".into()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgendaSnapshot {
    pub notes: Vec<Note>,
    pub reminders: Vec<Item>,
    pub events: Vec<CalEvent>,
    pub calendar: String,
}

#[derive(Serialize, Deserialize)]
#[serde(default)]
struct Store {
    notes: Vec<Note>,
    items: Vec<Item>,
    /// Events kept in the app when the phone calendar is missing or denied write.
    events: Vec<CalEvent>,
    next_id: i32,
}

impl Default for Store {
    fn default() -> Self {
        Self { notes: Vec::new(), items: Vec::new(), events: Vec::new(), next_id: 1 }
    }
}

pub struct Agenda {
    path: PathBuf,
    inner: Mutex<Store>,
}

pub fn tool_definitions() -> Value {
    json!([
        fn_tool("create_note", "Save a note on this phone. title is a short name. text may be markdown.", json!({
            "type": "object",
            "properties": { "title": { "type": "string" }, "text": { "type": "string" } },
            "required": ["text"]
        })),
        fn_tool("list_notes", "Read the notes saved on this phone.", json!({ "type": "object", "properties": {} })),
        fn_tool("update_note", "Change a note. id comes from list_notes. Pass title, text, or both.", json!({
            "type": "object",
            "properties": { "id": { "type": "string" }, "title": { "type": "string" }, "text": { "type": "string" } },
            "required": ["id"]
        })),
        fn_tool("delete_note", "Delete one saved note by id from list_notes.", json!({
            "type": "object",
            "properties": { "id": { "type": "string" } },
            "required": ["id"]
        })),
        fn_tool("set_alarm", "Set a clock alarm for the next time this hour and minute occurs. Hour is 0-23.", json!({
            "type": "object",
            "properties": {
                "hour": { "type": "integer" },
                "minute": { "type": "integer" },
                "label": { "type": "string" }
            },
            "required": ["hour", "minute"]
        })),
        fn_tool("set_timer", "Start a countdown timer. seconds is the length, such as 300 for five minutes.", json!({
            "type": "object",
            "properties": {
                "seconds": { "type": "integer" },
                "label": { "type": "string" }
            },
            "required": ["seconds"]
        })),
        fn_tool("set_reminder", "Schedule a one-off notification at a local date and time. at is YYYY-MM-DDTHH:MM.", json!({
            "type": "object",
            "properties": {
                "title": { "type": "string" },
                "at": { "type": "string" },
                "note": { "type": "string" }
            },
            "required": ["title", "at"]
        })),
        fn_tool("list_reminders", "List upcoming reminders, alarms, and timers this app scheduled.", json!({ "type": "object", "properties": {} })),
        fn_tool("update_reminder", "Change a reminder's title, note, or time. id from list_reminders. at is YYYY-MM-DDTHH:MM.", json!({
            "type": "object",
            "properties": {
                "id": { "type": "integer" },
                "title": { "type": "string" },
                "at": { "type": "string" },
                "note": { "type": "string" }
            },
            "required": ["id"]
        })),
        fn_tool("cancel_reminder", "Cancel one scheduled reminder, alarm, or timer by id from list_reminders.", json!({
            "type": "object",
            "properties": { "id": { "type": "integer" } },
            "required": ["id"]
        })),
        fn_tool("list_calendar_events", "Read events for a day. day is YYYY-MM-DD; omit it for today. Each event has an id for updates or deletes.", json!({
            "type": "object",
            "properties": { "day": { "type": "string" } }
        })),
        fn_tool("create_calendar_event", "Add a calendar event. start and end are local YYYY-MM-DDTHH:MM. end may be omitted for one hour.", json!({
            "type": "object",
            "properties": {
                "title": { "type": "string" },
                "start": { "type": "string" },
                "end": { "type": "string" },
                "location": { "type": "string" }
            },
            "required": ["title", "start"]
        })),
        fn_tool("update_calendar_event", "Change a calendar event. id comes from list_calendar_events. Pass only fields that change; start/end are YYYY-MM-DDTHH:MM.", json!({
            "type": "object",
            "properties": {
                "id": { "type": "string" },
                "title": { "type": "string" },
                "start": { "type": "string" },
                "end": { "type": "string" },
                "location": { "type": "string" }
            },
            "required": ["id"]
        })),
        fn_tool("media_control", "Control whatever is already playing: play, pause, stop, next, or previous. Call immediately. No confirmation.", json!({
            "type": "object",
            "properties": { "action": { "type": "string", "enum": ["play", "pause", "stop", "next", "previous"] } },
            "required": ["action"]
        })),
        fn_tool("set_volume", "Raise or lower the music volume. direction is up or down. steps is 1 to 5. Call immediately.", json!({
            "type": "object",
            "properties": {
                "direction": { "type": "string", "enum": ["up", "down"] },
                "steps": { "type": "integer" }
            },
            "required": ["direction"]
        })),
        fn_tool("find_contact", "Look up a person in the phone contacts by name. Returns phone and email. Call this before asking the user to spell a number. Then speak the match and wait for yes before opening WhatsApp, Signal, or email.", json!({
            "type": "object",
            "properties": { "name": { "type": "string" } },
            "required": ["name"]
        })),
        fn_tool("open_browser", "Open the phone browser on a website or a web search. Speak the site or search and wait for a clear yes, then call with confirmed true.", json!({
            "type": "object",
            "properties": {
                "query": { "type": "string" },
                "url": { "type": "string" },
                "confirmed": { "type": "boolean" }
            },
            "required": ["confirmed"]
        })),
        fn_tool("open_youtube", "Open YouTube on a search or a link. Speak the query and wait for a clear yes, then call with confirmed true. Does not send anything.", json!({
            "type": "object",
            "properties": {
                "query": { "type": "string" },
                "url": { "type": "string" },
                "confirmed": { "type": "boolean" }
            },
            "required": ["confirmed"]
        })),
        fn_tool("open_whatsapp", "Open a WhatsApp draft. phone is digits with country code. text is the message. Speak the number and the message, wait for a clear yes, then call with confirmed true. The user taps Send in WhatsApp.", json!({
            "type": "object",
            "properties": {
                "phone": { "type": "string" },
                "text": { "type": "string" },
                "confirmed": { "type": "boolean" }
            },
            "required": ["phone", "text", "confirmed"]
        })),
        fn_tool("open_signal", "Open a Signal chat for a phone number with country code. text is the message to tell the user. Speak the number and the message, wait for a clear yes, then call with confirmed true. The user sends it in Signal.", json!({
            "type": "object",
            "properties": {
                "phone": { "type": "string" },
                "text": { "type": "string" },
                "confirmed": { "type": "boolean" }
            },
            "required": ["phone", "confirmed"]
        })),
        fn_tool("open_email", "Open an email draft. Speak the address, subject, and body, wait for a clear yes, then call with confirmed true. The user taps Send in their mail app.", json!({
            "type": "object",
            "properties": {
                "to": { "type": "string" },
                "subject": { "type": "string" },
                "body": { "type": "string" },
                "confirmed": { "type": "boolean" }
            },
            "required": ["to", "confirmed"]
        })),
        fn_tool("web_search", "Search the live web and return a sourced summary. Use this for news, weather, scores, prices, and anything about today or recently. query is a specific search, such as 'most relevant news Zurich today'.", json!({
            "type": "object",
            "properties": { "query": { "type": "string" } },
            "required": ["query"]
        })),
        fn_tool("delete_calendar_event", "Delete a calendar event by id from list_calendar_events.", json!({
            "type": "object",
            "properties": { "id": { "type": "string" } },
            "required": ["id"]
        }))
    ])
}

fn fn_tool(name: &str, description: &str, parameters: Value) -> Value {
    json!({ "type": "function", "name": name, "description": description, "parameters": parameters })
}

impl Agenda {
    pub fn load(path: PathBuf) -> Self {
        let inner = fs::read(&path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        Self { path, inner: Mutex::new(inner) }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Store> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn persist(path: &PathBuf, store: &Store) -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(store).map_err(|e| e.to_string())?;
        fs::write(path, bytes).map_err(|e| e.to_string())
    }

    pub fn snapshot(&self, app: &AppHandle) -> AgendaSnapshot {
        self.sweep(app);
        let store = self.lock();
        let (mut events, calendar) = match calendar_range(app, None) {
            Ok((events, calendar)) => (events, calendar),
            Err(_) => (Vec::new(), "denied".into()),
        };
        let (start, end, _) = day_bounds(None).unwrap_or((0, 0, String::new()));
        for e in store.events.iter().filter(|e| e.start_ms < end && e.end_ms > start) {
            events.push(e.clone());
        }
        events.sort_by_key(|e| e.start_ms);
        AgendaSnapshot {
            notes: store.notes.clone(),
            reminders: store.items.clone(),
            events,
            calendar,
        }
    }

    /// Events overlapping [start, end), from the phone calendar when it is readable
    /// and always from events stored in the app.
    pub fn events_between(&self, app: &AppHandle, start: i64, end: i64) -> (Vec<CalEvent>, String) {
        let (mut events, calendar) = match read_phone_range(app, start, end) {
            Ok((events, calendar)) => (events, calendar),
            Err(_) => (Vec::new(), "unavailable".into()),
        };
        let store = self.lock();
        for event in store.events.iter().filter(|e| e.start_ms < end && e.end_ms > start) {
            events.push(event.clone());
        }
        events.sort_by_key(|e| e.start_ms);
        (events, calendar)
    }

    pub fn upsert_note(
        &self,
        app: &AppHandle,
        id: Option<&str>,
        title: &str,
        text: &str,
    ) -> Result<Note, String> {
        let text = text.trim();
        if text.is_empty() && title.trim().is_empty() {
            return Err("The note was empty.".into());
        }
        let text = text.chars().take(8000).collect::<String>();
        let title = note_title(title, &text);
        let mut store = self.lock();
        let note = if let Some(id) = id.filter(|s| !s.is_empty()) {
            let Some(existing) = store.notes.iter_mut().find(|n| n.id == id) else {
                return Err("No note with that id.".into());
            };
            if !text.is_empty() {
                existing.text = text;
            }
            existing.title = if title.is_empty() { note_title("", &existing.text) } else { title };
            existing.clone()
        } else {
            let note = Note {
                id: format!("n{}", Local::now().timestamp_millis()),
                title,
                text,
                created_ms: Local::now().timestamp_millis(),
            };
            store.notes.push(note.clone());
            if store.notes.len() > 200 {
                let drop_n = store.notes.len() - 200;
                store.notes.drain(0..drop_n);
            }
            note
        };
        Self::persist(&self.path, &store)?;
        drop(store);
        touch(app);
        Ok(note)
    }

    pub fn delete_note(&self, app: &AppHandle, id: &str) -> Result<(), String> {
        let mut store = self.lock();
        let before = store.notes.len();
        store.notes.retain(|n| n.id != id);
        if store.notes.len() == before {
            return Err("No note with that id.".into());
        }
        Self::persist(&self.path, &store)?;
        drop(store);
        touch(app);
        Ok(())
    }

    pub fn upsert_item(
        &self,
        app: &AppHandle,
        id: Option<i32>,
        kind: &str,
        title: &str,
        note: &str,
        at_ms: i64,
    ) -> Result<Item, String> {
        if title.trim().is_empty() {
            return Err("A title is required.".into());
        }
        if at_ms <= Local::now().timestamp_millis() + 5_000 {
            return Err("That time is in the past.".into());
        }
        let kind = if kind.is_empty() { "reminder" } else { kind };
        let mut store = self.lock();
        let item = if let Some(id) = id {
            let Some(existing) = store.items.iter_mut().find(|i| i.id == id) else {
                return Err("Nothing scheduled with that id.".into());
            };
            existing.title = title.chars().take(120).collect();
            existing.note = note.chars().take(500).collect();
            existing.at_ms = at_ms;
            existing.kind = kind.into();
            existing.clone()
        } else {
            let id = Self::alloc_id(&mut store);
            let item = Item {
                id,
                kind: kind.into(),
                title: title.chars().take(120).collect(),
                note: note.chars().take(500).collect(),
                at_ms,
            };
            store.items.push(item.clone());
            item
        };
        store.items.sort_by_key(|i| i.at_ms);
        Self::persist(&self.path, &store)?;
        drop(store);
        let _ = app.state::<NativeBridge>().schedule_item(
            item.id,
            &item.kind,
            &item.title,
            &item.note,
            item.at_ms,
        );
        touch(app);
        Ok(item)
    }

    pub fn cancel(&self, app: &AppHandle, id: i32) -> Result<(), String> {
        app.state::<NativeBridge>().cancel_item(id).map_err(|e| e.to_string())?;
        let mut store = self.lock();
        let before = store.items.len();
        store.items.retain(|i| i.id != id);
        if store.items.len() == before {
            return Err("Nothing scheduled with that id.".into());
        }
        Self::persist(&self.path, &store)?;
        drop(store);
        touch(app);
        Ok(())
    }

    pub fn upsert_local_event(&self, app: &AppHandle, event: CalEvent) -> Result<CalEvent, String> {
        let mut store = self.lock();
        if let Some(existing) = store.events.iter_mut().find(|e| e.id == event.id) {
            *existing = event.clone();
        } else {
            store.events.push(event.clone());
        }
        store.events.sort_by_key(|e| e.start_ms);
        Self::persist(&self.path, &store)?;
        drop(store);
        touch(app);
        Ok(event)
    }

    pub fn delete_local_event(&self, app: &AppHandle, id: &str) -> Result<(), String> {
        let mut store = self.lock();
        let before = store.events.len();
        store.events.retain(|e| e.id != id);
        if store.events.len() == before {
            return Err("No local event with that id.".into());
        }
        Self::persist(&self.path, &store)?;
        drop(store);
        touch(app);
        Ok(())
    }

    pub fn local_event(&self, id: &str) -> Option<CalEvent> {
        self.lock().events.iter().find(|e| e.id == id).cloned()
    }

    pub fn item(&self, id: i32) -> Option<Item> {
        self.lock().items.iter().find(|i| i.id == id).cloned()
    }

    fn sweep(&self, app: &AppHandle) {
        let now = Local::now().timestamp_millis();
        let mut store = self.lock();
        let stale: Vec<i32> = store.items.iter().filter(|i| i.at_ms + 60_000 < now).map(|i| i.id).collect();
        if stale.is_empty() {
            return;
        }
        store.items.retain(|i| i.at_ms + 60_000 >= now);
        let _ = Self::persist(&self.path, &store);
        drop(store);
        let bridge = app.state::<NativeBridge>();
        for id in stale {
            let _ = bridge.cancel_item(id);
        }
    }

    fn alloc_id(store: &mut Store) -> i32 {
        let id = store.next_id.max(1);
        store.next_id = id.saturating_add(1);
        id
    }

    fn take_id(&self) -> Result<i32, String> {
        let mut store = self.lock();
        let id = Self::alloc_id(&mut store);
        Self::persist(&self.path, &store)?;
        Ok(id)
    }
}

pub fn reschedule(app: &AppHandle) {
    let agenda = app.state::<Agenda>();
    agenda.sweep(app);
    let items = agenda.lock().items.clone();
    let bridge = app.state::<NativeBridge>();
    for item in items {
        let _ = bridge.schedule_item(item.id, &item.kind, &item.title, &item.note, item.at_ms);
    }
}

pub fn run(app: &AppHandle, name: &str, arguments: &str) -> String {
    let args: Value = serde_json::from_str(arguments).unwrap_or(Value::Null);
    let result = dispatch(app, name, &args);
    let out = match result {
        Ok(v) => v.to_string(),
        Err(e) => json!({ "ok": false, "error": e }).to_string(),
    };
    if let Some(hub) = app.try_state::<Arc<Hub>>() {
        let brief = serde_json::from_str::<Value>(&out)
            .ok()
            .and_then(|v| {
                v["detail"].as_str().map(str::to_owned).or_else(|| v["error"].as_str().map(str::to_owned))
            })
            .unwrap_or_else(|| "done".into());
        hub.info(format!("tool {name}: {brief}"));
    }
    out
}

fn phone_tool(app: &AppHandle, name: &str, args: &Value) -> Result<Option<Value>, String> {
    let bridge = app.state::<NativeBridge>();
    let raw = match name {
        "media_control" => bridge
            .media_control(args["action"].as_str().unwrap_or(""))
            .map_err(|e| e.to_string())?,
        "set_volume" => {
            let steps = args["steps"].as_i64().unwrap_or(1).clamp(1, 5) as i32;
            bridge
                .adjust_volume(args["direction"].as_str().unwrap_or(""), steps)
                .map_err(|e| e.to_string())?
        }
        "find_contact" => {
            let name = args["name"].as_str().unwrap_or("").trim().to_string();
            let mut raw = bridge.lookup_contacts(&name).map_err(|e| e.to_string())?;
            if raw.contains("not granted") {
                let _ = bridge.request_contacts();
                raw = bridge.lookup_contacts(&name).map_err(|e| e.to_string())?;
            }
            return serde_json::from_str(&raw).map(Some).map_err(|e| e.to_string());
        }
        "open_youtube" | "open_whatsapp" | "open_signal" | "open_email" | "open_browser" => {
            if args["confirmed"] != true {
                let plan = match name {
                    "open_browser" => format!(
                        "Open the browser for {}?",
                        args["query"].as_str().filter(|s| !s.is_empty()).unwrap_or(args["url"].as_str().unwrap_or("that page"))
                    ),
                    "open_youtube" => format!(
                        "Open YouTube for {}?",
                        args["query"].as_str().filter(|s| !s.is_empty()).unwrap_or(args["url"].as_str().unwrap_or("that video"))
                    ),
                    "open_whatsapp" => format!(
                        "Open WhatsApp to {} with: {}?",
                        args["phone"].as_str().unwrap_or(""),
                        args["text"].as_str().unwrap_or("")
                    ),
                    "open_signal" => format!(
                        "Open Signal for {}? Message: {}",
                        args["phone"].as_str().unwrap_or(""),
                        args["text"].as_str().unwrap_or("")
                    ),
                    _ => format!(
                        "Open an email to {} about {}? {}",
                        args["to"].as_str().unwrap_or(""),
                        args["subject"].as_str().unwrap_or(""),
                        args["body"].as_str().unwrap_or("")
                    ),
                };
                return Ok(Some(json!({
                    "ok": true,
                    "opened": false,
                    "detail": format!("{plan} Nothing was opened. Wait for a clear yes, then call this again with confirmed true.")
                })));
            }
            let kind = name.trim_start_matches("open_");
            bridge
                .open_hand_off(
                    kind,
                    args["query"].as_str().unwrap_or(""),
                    args["url"].as_str().unwrap_or(""),
                    args["phone"].as_str().unwrap_or(""),
                    args["text"].as_str().unwrap_or(""),
                    args["to"].as_str().unwrap_or(""),
                    args["subject"].as_str().unwrap_or(""),
                    args["body"].as_str().unwrap_or(""),
                )
                .map_err(|e| e.to_string())?
        }
        _ => return Ok(None),
    };
    serde_json::from_str(&raw).map(Some).map_err(|e| e.to_string())
}

fn dispatch(app: &AppHandle, name: &str, args: &Value) -> Result<Value, String> {
    if name == "web_search" {
        return crate::search::lookup(app, args["query"].as_str().unwrap_or(""));
    }
    if let Some(value) = phone_tool(app, name, args)? {
        return Ok(value);
    }
    let agenda = app.state::<Agenda>();
    match name {
        "create_note" => {
            let note = agenda.upsert_note(app, None, args["title"].as_str().unwrap_or(""), args["text"].as_str().unwrap_or(""))?;
            Ok(json!({ "ok": true, "id": note.id, "title": note.title, "detail": format!("Saved a note: {}", note.title) }))
        }
        "list_notes" => {
            let notes = agenda.lock().notes.clone();
            Ok(json!({
                "ok": true,
                "count": notes.len(),
                "notes": notes.iter().map(|n| json!({ "id": n.id, "title": note_title(&n.title, &n.text), "text": n.text })).collect::<Vec<_>>(),
                "detail": if notes.is_empty() { "There are no saved notes.".into() } else { format!("{} saved notes.", notes.len()) }
            }))
        }
        "update_note" => {
            let id = args["id"].as_str().unwrap_or("").trim();
            let note = agenda.upsert_note(
                app,
                Some(id),
                args["title"].as_str().unwrap_or(""),
                args["text"].as_str().unwrap_or(""),
            )?;
            Ok(json!({ "ok": true, "id": note.id, "detail": format!("Updated the note: {}", note.title) }))
        }
        "delete_note" => {
            let id = args["id"].as_str().unwrap_or("").trim();
            if id.is_empty() {
                return Err("Which note? Call list_notes and use an id.".into());
            }
            agenda.delete_note(app, id)?;
            Ok(json!({ "ok": true, "detail": "Deleted that note." }))
        }
        "set_alarm" => {
            let hour = arg_u32(args, "hour").ok_or("hour 0-23 is required")?;
            let minute = arg_u32(args, "minute").unwrap_or(0);
            if hour > 23 || minute > 59 {
                return Err("hour must be 0-23 and minute 0-59.".into());
            }
            let label = label_or(args, "label", "Alarm");
            let when = next_occurrence(hour, minute)?;
            let id = agenda.take_id()?;
            let raw = app
                .state::<NativeBridge>()
                .set_alarm(hour, minute, &label, id, when.timestamp_millis())
                .map_err(|e| e.to_string())?;
            finish_schedule(app, &agenda, id, "alarm", &label, "", when, &raw)
        }
        "set_timer" => {
            let seconds = arg_u32(args, "seconds").ok_or("seconds is required")?;
            if !(1..=86_400).contains(&seconds) {
                return Err("A timer has to be between 1 second and 24 hours.".into());
            }
            let label = label_or(args, "label", "Timer");
            let when = Local::now() + Duration::seconds(seconds as i64);
            let id = agenda.take_id()?;
            let raw = app
                .state::<NativeBridge>()
                .set_timer(seconds, &label, id, when.timestamp_millis())
                .map_err(|e| e.to_string())?;
            finish_schedule(app, &agenda, id, "timer", &label, "", when, &raw)
        }
        "set_reminder" => {
            let title = label_or(args, "title", "");
            if title.is_empty() {
                return Err("A reminder needs a title.".into());
            }
            let when = parse_local_when(args["at"].as_str().unwrap_or(""))?;
            let note = args["note"].as_str().unwrap_or("").trim();
            let item = agenda.upsert_item(app, None, "reminder", &title, note, when.timestamp_millis())?;
            Ok(json!({
                "ok": true,
                "id": item.id,
                "detail": format!("Reminder set for {}: {title}", when.format("%a %-d %b %-H:%M"))
            }))
        }
        "list_reminders" => {
            agenda.sweep(app);
            let items = agenda.lock().items.clone();
            Ok(json!({
                "ok": true,
                "count": items.len(),
                "reminders": items.iter().map(|i| json!({
                    "id": i.id,
                    "kind": i.kind,
                    "title": i.title,
                    "note": i.note,
                    "at": DateTime::from_timestamp_millis(i.at_ms).map(|t| t.with_timezone(&Local).format("%a %-d %b %-H:%M").to_string()).unwrap_or_default()
                })).collect::<Vec<_>>(),
                "detail": if items.is_empty() { "Nothing is scheduled.".into() } else { format!("{} things scheduled.", items.len()) }
            }))
        }
        "update_reminder" => {
            let id = args["id"].as_i64().ok_or("id from list_reminders is required")? as i32;
            let existing = agenda.item(id).ok_or("Nothing scheduled with that id.")?;
            let title = args["title"].as_str().map(str::trim).filter(|s| !s.is_empty()).unwrap_or(&existing.title);
            let note = args["note"].as_str().map(str::trim).unwrap_or(&existing.note);
            let at_ms = match args["at"].as_str().map(str::trim).filter(|s| !s.is_empty()) {
                Some(s) => parse_local_when(s)?.timestamp_millis(),
                None => existing.at_ms,
            };
            let item = agenda.upsert_item(app, Some(id), &existing.kind, title, note, at_ms)?;
            let when = DateTime::from_timestamp_millis(item.at_ms)
                .map(|t| t.with_timezone(&Local).format("%a %-d %b %-H:%M").to_string())
                .unwrap_or_default();
            Ok(json!({ "ok": true, "id": item.id, "detail": format!("Updated: {} at {when}", item.title) }))
        }
        "cancel_reminder" => {
            let id = args["id"].as_i64().ok_or("id from list_reminders is required")? as i32;
            agenda.cancel(app, id)?;
            Ok(json!({ "ok": true, "detail": "Cancelled." }))
        }
        "list_calendar_events" => {
            let (start, end, day) = day_bounds(args["day"].as_str())?;
            let mut events = match ensure_calendar(app, start, end) {
                Ok(e) => e,
                Err(err) if err.contains("denied") || err.contains("permission") => Vec::new(),
                Err(err) => return Err(err),
            };
            for e in agenda.lock().events.iter().filter(|e| e.start_ms < end && e.end_ms > start) {
                events.push(e.clone());
            }
            events.sort_by_key(|e| e.start_ms);
            Ok(json!({
                "ok": true,
                "day": day,
                "count": events.len(),
                "events": events.iter().map(|e| json!({
                    "id": e.id,
                    "title": e.title,
                    "source": e.source,
                    "when": if e.all_day { "all day".to_string() } else { format_span(e.start_ms, e.end_ms) },
                    "location": e.location
                })).collect::<Vec<_>>(),
                "detail": if events.is_empty() { format!("Nothing on the calendar for {day}.") } else { format!("{} events on {day}.", events.len()) }
            }))
        }
        "create_calendar_event" => create_or_local(app, &agenda, args),
        "update_calendar_event" => update_event(app, &agenda, args),
        "delete_calendar_event" => delete_event(app, &agenda, args),
        _ => Err(format!("Unknown tool {name}")),
    }
}

fn create_or_local(app: &AppHandle, agenda: &Agenda, args: &Value) -> Result<Value, String> {
    let title = label_or(args, "title", "");
    if title.is_empty() {
        return Err("The event needs a title.".into());
    }
    let start = parse_local_when(args["start"].as_str().unwrap_or(""))?;
    let end = match args["end"].as_str().map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => parse_local_when(s)?,
        None => start + Duration::hours(1),
    };
    if end <= start {
        return Err("The event ends before it starts.".into());
    }
    let location = args["location"].as_str().unwrap_or("").trim().chars().take(200).collect::<String>();

    if ensure_calendar_permission(app).is_ok() {
        let raw = app
            .state::<NativeBridge>()
            .create_event(&title, start.timestamp_millis(), end.timestamp_millis(), &location)
            .map_err(|e| e.to_string())?;
        let v: Value = serde_json::from_str(&raw).unwrap_or(json!({ "ok": false }));
        if v["ok"] == true {
            touch(app);
            let id = v["id"].as_str().unwrap_or("").to_string();
            return Ok(json!({
                "ok": true,
                "id": id,
                "source": "phone",
                "detail": format!("Added \"{title}\" on {}.", start.format("%a %-d %b %-H:%M"))
            }));
        }
    }

    let event = CalEvent {
        id: format!("local-{}", Local::now().timestamp_millis()),
        title: title.clone(),
        start_ms: start.timestamp_millis(),
        end_ms: end.timestamp_millis(),
        location,
        all_day: false,
        source: "local".into(),
    };
    agenda.upsert_local_event(app, event.clone())?;
    Ok(json!({
        "ok": true,
        "id": event.id,
        "source": "local",
        "detail": format!("Saved \"{title}\" on {} in the Plan tab.", start.format("%a %-d %b %-H:%M"))
    }))
}

fn update_event(app: &AppHandle, agenda: &Agenda, args: &Value) -> Result<Value, String> {
    let id = args["id"].as_str().unwrap_or("").trim();
    if id.is_empty() {
        return Err("Which event? Call list_calendar_events and use an id.".into());
    }

    if id.starts_with("local-") || agenda.local_event(id).is_some() {
        let mut event = agenda.local_event(id).ok_or("No local event with that id.")?;
        if let Some(title) = args["title"].as_str().map(str::trim).filter(|s| !s.is_empty()) {
            event.title = title.chars().take(120).collect();
        }
        if let Some(loc) = args["location"].as_str() {
            event.location = loc.trim().chars().take(200).collect();
        }
        if let Some(s) = args["start"].as_str().map(str::trim).filter(|s| !s.is_empty()) {
            event.start_ms = parse_local_when(s)?.timestamp_millis();
        }
        if let Some(s) = args["end"].as_str().map(str::trim).filter(|s| !s.is_empty()) {
            event.end_ms = parse_local_when(s)?.timestamp_millis();
        } else if event.end_ms <= event.start_ms {
            event.end_ms = event.start_ms + 3_600_000;
        }
        agenda.upsert_local_event(app, event.clone())?;
        return Ok(json!({ "ok": true, "id": event.id, "detail": format!("Updated \"{}\".", event.title) }));
    }

    ensure_calendar_permission(app)?;
    let title = args["title"].as_str().unwrap_or("").trim();
    let location = args["location"].as_str().unwrap_or("").trim();
    let start_ms = match args["start"].as_str().map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => parse_local_when(s)?.timestamp_millis(),
        None => 0,
    };
    let end_ms = match args["end"].as_str().map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => parse_local_when(s)?.timestamp_millis(),
        None => 0,
    };
    let raw = app
        .state::<NativeBridge>()
        .update_event(id, title, start_ms, end_ms, location)
        .map_err(|e| e.to_string())?;
    let v: Value = serde_json::from_str(&raw).unwrap_or(json!({ "ok": false, "error": "bad calendar response" }));
    if v["ok"] == true {
        touch(app);
        Ok(json!({ "ok": true, "id": id, "detail": "Updated that calendar event." }))
    } else {
        Ok(v)
    }
}

fn delete_event(app: &AppHandle, agenda: &Agenda, args: &Value) -> Result<Value, String> {
    let id = args["id"].as_str().unwrap_or("").trim();
    if id.is_empty() {
        return Err("Which event? Call list_calendar_events and use an id.".into());
    }
    if id.starts_with("local-") || agenda.local_event(id).is_some() {
        agenda.delete_local_event(app, id)?;
        return Ok(json!({ "ok": true, "detail": "Deleted that event." }));
    }
    ensure_calendar_permission(app)?;
    let raw = app.state::<NativeBridge>().delete_event(id).map_err(|e| e.to_string())?;
    let v: Value = serde_json::from_str(&raw).unwrap_or(json!({ "ok": false, "error": "bad calendar response" }));
    if v["ok"] == true {
        touch(app);
        Ok(json!({ "ok": true, "detail": "Deleted that calendar event." }))
    } else {
        Ok(v)
    }
}

fn finish_schedule(
    app: &AppHandle,
    agenda: &Agenda,
    id: i32,
    kind: &str,
    title: &str,
    note: &str,
    when: DateTime<Local>,
    raw: &str,
) -> Result<Value, String> {
    let v: Value = serde_json::from_str(raw).unwrap_or(json!({ "ok": false, "error": "the phone did not answer" }));
    if v["ok"] != true {
        return Ok(v);
    }
    if v["via"] == "local" {
        let mut store = agenda.lock();
        store.items.retain(|i| i.id != id);
        store.items.push(Item {
            id,
            kind: kind.into(),
            title: title.into(),
            note: note.into(),
            at_ms: when.timestamp_millis(),
        });
        store.items.sort_by_key(|i| i.at_ms);
        Agenda::persist(&agenda.path, &store)?;
        drop(store);
        touch(app);
    }
    let spoken = v["detail"].as_str().map(str::to_owned).unwrap_or_else(|| match kind {
        "alarm" => format!("Alarm set for {}.", when.format("%-H:%M")),
        "timer" => format!("Timer set for {}.", when.format("%-H:%M")),
        _ => format!("Set for {}.", when.format("%a %-H:%M")),
    });
    Ok(json!({ "ok": true, "id": id, "detail": spoken }))
}

fn read_phone_range(app: &AppHandle, start: i64, end: i64) -> Result<(Vec<CalEvent>, String), String> {
    let bridge = app.state::<NativeBridge>();
    let status = bridge.check_permissions().map_err(|e| e.to_string())?;
    if status.calendar != "granted" {
        return Ok((Vec::new(), status.calendar));
    }
    let events = read_events(&bridge, start, end)?;
    Ok((events, "granted".into()))
}

fn calendar_range(app: &AppHandle, day: Option<&str>) -> Result<(Vec<CalEvent>, String), String> {
    let bridge = app.state::<NativeBridge>();
    let status = bridge.check_permissions().map_err(|e| e.to_string())?;
    if status.calendar != "granted" {
        return Ok((Vec::new(), status.calendar));
    }
    let (start, end, _) = day_bounds(day)?;
    let events = read_events(&bridge, start, end)?;
    Ok((events, "granted".into()))
}

fn ensure_calendar_permission(app: &AppHandle) -> Result<(), String> {
    let bridge = app.state::<NativeBridge>();
    let status = bridge.check_permissions().map_err(|e| e.to_string())?;
    if status.calendar == "granted" {
        return Ok(());
    }
    let status = bridge.request_calendar().map_err(|e| e.to_string())?;
    if status.calendar == "granted" {
        Ok(())
    } else {
        Err("Calendar access was denied. Allow it in Android settings for Grok Voice.".into())
    }
}

fn ensure_calendar(app: &AppHandle, start: i64, end: i64) -> Result<Vec<CalEvent>, String> {
    ensure_calendar_permission(app)?;
    read_events(&app.state::<NativeBridge>(), start, end)
}

fn read_events(bridge: &NativeBridge, start: i64, end: i64) -> Result<Vec<CalEvent>, String> {
    let raw = bridge.list_calendar(start, end).map_err(|e| e.to_string())?;
    let v: Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    if v["ok"] != true {
        return Err(v["error"].as_str().unwrap_or("Could not read the calendar.").into());
    }
    serde_json::from_value(v["events"].clone()).map_err(|e| e.to_string())
}

fn touch(app: &AppHandle) {
    if let Some(hub) = app.try_state::<Arc<Hub>>() {
        hub.emit("voice://agenda", json!({}));
    }
}

fn note_title(title: &str, text: &str) -> String {
    let given = title.trim();
    let raw = if given.is_empty() {
        text.lines().find(|line| !line.trim().is_empty()).unwrap_or("Note")
    } else {
        given
    };
    raw.trim().trim_start_matches('#').trim().chars().take(80).collect()
}

fn label_or(args: &Value, key: &str, fallback: &str) -> String {
    let s = args[key].as_str().unwrap_or("").trim();
    let s = if s.is_empty() { fallback } else { s };
    s.chars().take(120).collect()
}

fn arg_u32(args: &Value, key: &str) -> Option<u32> {
    args[key].as_u64().or_else(|| args[key].as_str().and_then(|s| s.parse().ok())).map(|n| n as u32)
}

pub fn parse_local_when(s: &str) -> Result<DateTime<Local>, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("Give a local time as YYYY-MM-DDTHH:MM.".into());
    }
    if let Ok(fixed) = DateTime::parse_from_rfc3339(s) {
        return Ok(fixed.with_timezone(&Local));
    }
    let naive = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M")
        .or_else(|_| NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M"))
        .or_else(|_| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S"))
        .map_err(|_| format!("could not read '{s}'. Use YYYY-MM-DDTHH:MM."))?;
    naive
        .and_local_timezone(Local)
        .earliest()
        .or_else(|| naive.and_local_timezone(Local).latest())
        .ok_or_else(|| "that local time does not exist".into())
}

fn next_occurrence(hour: u32, minute: u32) -> Result<DateTime<Local>, String> {
    let now = Local::now();
    let naive = now
        .date_naive()
        .and_hms_opt(hour, minute, 0)
        .ok_or_else(|| "that clock time does not exist".to_string())?;
    let mut when = naive
        .and_local_timezone(Local)
        .earliest()
        .or_else(|| naive.and_local_timezone(Local).latest())
        .ok_or_else(|| "that clock time does not exist".to_string())?;
    if when <= now + Duration::seconds(45) {
        let tomorrow = (now.date_naive() + Duration::days(1))
            .and_hms_opt(hour, minute, 0)
            .ok_or_else(|| "that clock time does not exist".to_string())?;
        when = tomorrow
            .and_local_timezone(Local)
            .earliest()
            .or_else(|| tomorrow.and_local_timezone(Local).latest())
            .ok_or_else(|| "that clock time does not exist".to_string())?;
    }
    Ok(when)
}

fn day_bounds(day: Option<&str>) -> Result<(i64, i64, String), String> {
    let date = match day.map(str::trim).filter(|d| !d.is_empty()) {
        Some(d) => NaiveDate::parse_from_str(d, "%Y-%m-%d").map_err(|_| format!("bad day '{d}'. Use YYYY-MM-DD."))?,
        None => Local::now().date_naive(),
    };
    let start_naive = date.and_hms_opt(0, 0, 0).ok_or_else(|| "bad day".to_string())?;
    let start = start_naive
        .and_local_timezone(Local)
        .earliest()
        .or_else(|| start_naive.and_local_timezone(Local).latest())
        .ok_or_else(|| "bad day".to_string())?;
    let end = start + Duration::days(1);
    Ok((start.timestamp_millis(), end.timestamp_millis(), date.format("%Y-%m-%d").to_string()))
}

fn format_span(start_ms: i64, end_ms: i64) -> String {
    let Some(start) = DateTime::from_timestamp_millis(start_ms) else {
        return String::new();
    };
    let start = start.with_timezone(&Local);
    let end = DateTime::from_timestamp_millis(end_ms).map(|t| t.with_timezone(&Local));
    match end {
        Some(end) => format!("{}–{}", start.format("%-H:%M"), end.format("%-H:%M")),
        None => start.format("%-H:%M").to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_local_minute_precision() {
        let dt = parse_local_when("2030-01-02T15:04").unwrap();
        assert_eq!(dt.format("%Y-%m-%dT%H:%M").to_string(), "2030-01-02T15:04");
    }

    #[test]
    fn rejects_blank_time() {
        assert!(parse_local_when("  ").is_err());
    }

    #[test]
    fn notes_round_trip_on_disk() {
        let path = std::env::temp_dir().join(format!("grok-agenda-test-{}.json", std::process::id()));
        let _ = fs::remove_file(&path);
        let agenda = Agenda::load(path.clone());
        {
            let mut store = agenda.lock();
            store.notes.push(Note { id: "n1".into(), title: "Milk".into(), text: "buy milk".into(), created_ms: 1 });
            Agenda::persist(&agenda.path, &store).unwrap();
        }
        let again = Agenda::load(path.clone());
        assert_eq!(again.lock().notes[0].text, "buy milk");
        let _ = fs::remove_file(&path);
    }
}
