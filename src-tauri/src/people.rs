//! On-device People overlay. Android ContactsContract stays the source of truth
//! for phone numbers and email. Nicknames, aliases, a preferred channel, handle
//! overrides, favorites, notes, and dated reminders live in `people.json`.
//!
//! Person reminders are birthdays, anniversaries, and custom dates. `actions`
//! schedules the notification from this store. The date stays on the person
//! all year and joins the Time list only inside its notify window.

use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    sync::Mutex,
};

use chrono::Datelike;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tauri::{AppHandle, Manager};

use crate::{native::NativeBridge, recurrence};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Handles {
    #[serde(default)]
    pub phone: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub whatsapp: String,
    #[serde(default)]
    pub signal: String,
}

impl Default for Handles {
    fn default() -> Self {
        Self { phone: String::new(), email: String::new(), whatsapp: String::new(), signal: String::new() }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Person {
    pub id: String,
    #[serde(default)]
    pub android_contact_id: Option<String>,
    pub display_name: String,
    #[serde(default)]
    pub nickname: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    /// `call`, `sms`, `whatsapp`, `signal`, `email`, or empty.
    #[serde(default)]
    pub preferred_channel: String,
    #[serde(default)]
    pub handles: Handles,
    #[serde(default)]
    pub favorite: bool,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub reminders: Vec<PersonReminder>,
    /// 0 is off. Otherwise nudge to reach out after this many days.
    #[serde(default)]
    pub reach_out_days: u32,
    /// Latest call or text the user allowed this phone to read. Not a message body.
    #[serde(default)]
    pub last_contacted_ms: Option<i64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PersonReminder {
    #[serde(default)]
    pub id: String,
    /// `birthday`, `anniversary`, or `custom`.
    #[serde(default = "custom_label")]
    pub label: String,
    #[serde(default)]
    pub title: String,
    /// `MM-DD`, or `YYYY-MM-DD` when it happens once in a known year.
    #[serde(default)]
    pub date: String,
    /// `yearly` (default) or `once`.
    #[serde(default = "yearly_recurrence")]
    pub recurrence: String,
    #[serde(default)]
    pub lead_days: u32,
    /// Local `HH:MM`. Empty becomes 09:00.
    #[serde(default = "morning")]
    pub time: String,
    #[serde(default = "enabled_default")]
    pub enabled: bool,
}

fn upcoming_year(month: u32, day: u32) -> i32 {
    let today = chrono::Local::now().date_naive();
    let this_year = chrono::NaiveDate::from_ymd_opt(today.year(), month, day.min(28)).unwrap_or(today);
    if this_year >= today { today.year() } else { today.year() + 1 }
}

fn custom_label() -> String {
    "custom".into()
}
fn yearly_recurrence() -> String {
    "yearly".into()
}
fn morning() -> String {
    "09:00".into()
}
fn enabled_default() -> bool {
    true
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonDraft {
    #[serde(default)]
    pub id: String,
    pub android_contact_id: Option<String>,
    pub display_name: String,
    #[serde(default)]
    pub nickname: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub preferred_channel: String,
    #[serde(default)]
    pub handles: Handles,
    #[serde(default)]
    pub favorite: bool,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub reminders: Vec<PersonReminder>,
    #[serde(default)]
    pub reach_out_days: u32,
    #[serde(default)]
    pub last_contacted_ms: Option<i64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PhoneContact {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub phone: String,
    #[serde(default)]
    pub email: String,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct Store {
    people: Vec<Person>,
}

pub struct People {
    path: PathBuf,
    inner: Mutex<Store>,
}

impl People {
    pub fn load(path: PathBuf) -> Self {
        let bytes = fs::read(&path).unwrap_or_default();
        let (inner, dirty) = load_store(&bytes);
        if dirty {
            let _ = Self::persist(&path, &inner);
        }
        Self { path, inner: Mutex::new(inner) }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Store> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn persist(path: &PathBuf, store: &Store) -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(store).map_err(|e| e.to_string())?;
        crate::files::write_private(path, &bytes).map_err(|e| e.to_string())
    }

    pub fn list(&self) -> Vec<Person> {
        self.lock().people.clone()
    }

    pub fn save(&self, draft: PersonDraft) -> Result<Person, String> {
        let display_name = clamp(&draft.display_name, 120);
        if display_name.is_empty() {
            return Err("A person needs a name.".into());
        }
        let id = draft.id.trim().to_string();
        let android_contact_id = draft.android_contact_id.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
        let mut store = self.lock();
        if let Some(contact_id) = &android_contact_id {
            let clash = store.people.iter().any(|p| p.id != id && p.android_contact_id.as_deref() == Some(contact_id.as_str()));
            if clash {
                return Err("That phone contact is already in People.".into());
            }
        }
        let person = Person {
            id: if id.is_empty() { format!("p{}", chrono::Local::now().timestamp_millis()) } else { id },
            android_contact_id,
            display_name,
            nickname: clamp(&draft.nickname, 80),
            aliases: clean_aliases(&draft.aliases),
            preferred_channel: normalize_channel(&draft.preferred_channel),
            handles: clean_handles(&draft.handles),
            favorite: draft.favorite,
            note: clamp(&draft.note, 4000),
            reminders: clean_reminders(&draft.reminders)?,
            reach_out_days: draft.reach_out_days.min(365),
            last_contacted_ms: draft.last_contacted_ms.filter(|ms| *ms > 0),
        };
        if let Some(existing) = store.people.iter_mut().find(|p| p.id == person.id) {
            *existing = person.clone();
        } else {
            if store.people.len() >= 200 {
                return Err("People is full. Delete one first.".into());
            }
            store.people.push(person.clone());
        }
        Self::persist(&self.path, &store)?;
        Ok(person)
    }

    pub fn set_last_contacted(&self, id: &str, at_ms: i64) -> Result<Person, String> {
        let mut store = self.lock();
        let Some(person) = store.people.iter_mut().find(|person| person.id == id) else {
            return Err("No person with that id.".into());
        };
        person.last_contacted_ms = Some(at_ms);
        let saved = person.clone();
        Self::persist(&self.path, &store)?;
        Ok(saved)
    }

    pub fn add_birthday_if_missing(&self, id: &str, date: &str) -> Result<Person, String> {
        let date = date.trim();
        if date.len() < 5 {
            return Err("That contact has no birthday on this phone.".into());
        }
        let mut store = self.lock();
        let Some(person) = store.people.iter_mut().find(|person| person.id == id) else {
            return Err("No person with that id.".into());
        };
        if person.reminders.iter().any(|reminder| reminder.label == "birthday") {
            return Err("A birthday is already saved for this person.".into());
        }
        let mut reminders = person.reminders.clone();
        reminders.push(PersonReminder {
            id: format!("b{}", chrono::Local::now().timestamp_millis()),
            label: "birthday".into(),
            title: String::new(),
            date: date.to_string(),
            recurrence: "yearly".into(),
            lead_days: 0,
            time: "09:00".into(),
            enabled: true,
        });
        person.reminders = clean_reminders(&reminders)?;
        let saved = person.clone();
        Self::persist(&self.path, &store)?;
        Ok(saved)
    }

    /// Add phone-contact dates that are not already stored. Birthdays stay unique.
    pub fn merge_imported_dates(&self, id: &str, incoming: Vec<PersonReminder>) -> Result<(Person, usize), String> {
        let mut store = self.lock();
        let Some(person) = store.people.iter_mut().find(|person| person.id == id) else {
            return Err("No person with that id.".into());
        };
        if incoming.is_empty() {
            return Err("No dates on that contact.".into());
        }
        let mut reminders = person.reminders.clone();
        let before = reminders.len();
        for reminder in incoming {
            let duplicate = reminders.iter().any(|existing| {
                same_date(&existing.date, &reminder.date)
                    && existing.label == reminder.label
                    && (reminder.label != "custom" || same_title(&existing.title, &reminder.title))
            });
            if reminder.label == "birthday" && reminders.iter().any(|existing| existing.label == "birthday") {
                continue;
            }
            if duplicate {
                continue;
            }
            reminders.push(reminder);
        }
        let added = reminders.len().saturating_sub(before);
        person.reminders = clean_reminders(&reminders)?;
        let saved = person.clone();
        Self::persist(&self.path, &store)?;
        Ok((saved, added))
    }

    pub fn delete(&self, id: &str) -> Result<(), String> {
        let mut store = self.lock();
        let before = store.people.len();
        store.people.retain(|p| p.id != id);
        if store.people.len() == before {
            return Err("No person with that id.".into());
        }
        Self::persist(&self.path, &store)
    }

    pub fn replace_all(&self, people: Vec<Person>) -> Result<(), String> {
        let mut cleaned = Vec::new();
        for mut person in people {
            person.display_name = clamp(&person.display_name, 120);
            if person.display_name.is_empty() {
                continue;
            }
            person.nickname = clamp(&person.nickname, 80);
            person.aliases = clean_aliases(&person.aliases);
            person.preferred_channel = normalize_channel(&person.preferred_channel);
            person.handles = clean_handles(&person.handles);
            person.note = clamp(&person.note, 4000);
            person.reminders = clean_reminders(&person.reminders)?;
            person.reach_out_days = person.reach_out_days.min(365);
            cleaned.push(person);
            if cleaned.len() == 200 {
                break;
            }
        }
        let mut store = self.lock();
        store.people = cleaned;
        Self::persist(&self.path, &store)
    }

    pub fn set_reminders(&self, id: &str, reminders: Vec<PersonReminder>) -> Result<Person, String> {
        let reminders = clean_reminders(&reminders)?;
        let mut store = self.lock();
        let Some(person) = store.people.iter_mut().find(|p| p.id == id) else {
            return Err("No person with that id.".into());
        };
        person.reminders = reminders;
        let person = person.clone();
        Self::persist(&self.path, &store)?;
        Ok(person)
    }
}

/// Voice lookup: People nicknames, aliases, and favorites, then phone contacts.
pub fn find_contact(app: &AppHandle, name: &str) -> Result<Value, String> {
    let query = name.trim();
    if query.chars().count() < 2 && !is_favorites_query(query) {
        return Err("Say a person's name.".into());
    }
    let people = app.state::<People>().list();
    let hits = rank_people(&people, query);
    let (system, system_error) = read_system_contacts(app, query);
    let mut linked = HashMap::new();
    for person in &hits {
        let Some(id) = person.android_contact_id.as_ref().filter(|id| !id.is_empty()) else { continue };
        if system.iter().any(|hit| &hit.id == id) {
            continue;
        }
        if let Some(contact) = read_one_contact(app, id) {
            linked.insert(id.clone(), contact);
        }
    }
    if hits.is_empty() && system.is_empty() {
        if let Some(error) = system_error {
            return Ok(json!({ "ok": false, "error": error, "matches": [] }));
        }
        return Ok(json!({
            "ok": true,
            "matches": [],
            "detail": format!("No person matched {query}.")
        }));
    }
    Ok(assemble(&hits, &system, &linked))
}

pub fn rank_people(people: &[Person], query: &str) -> Vec<Person> {
    let q = normalize(query);
    if q.is_empty() {
        return Vec::new();
    }
    let mut scored: Vec<(i32, usize, Person)> = people
        .iter()
        .enumerate()
        .filter_map(|(index, person)| score_person(person, &q).map(|score| (score, index, person.clone())))
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    scored.into_iter().take(6).map(|(_, _, person)| person).collect()
}

fn score_person(person: &Person, query: &str) -> Option<i32> {
    if is_favorites_query(query) {
        return person.favorite.then_some(100);
    }
    let mut best = 0;
    best = best.max(label_score(&person.nickname, query, 400, 280));
    for alias in &person.aliases {
        best = best.max(label_score(alias, query, 380, 260));
    }
    best = best.max(label_score(&person.display_name, query, 220, 140));
    if best == 0 {
        return None;
    }
    if person.favorite {
        best += 30;
    }
    Some(best)
}

fn label_score(label: &str, query: &str, exact: i32, partial: i32) -> i32 {
    let label = normalize(label);
    if label.is_empty() || query.is_empty() {
        return 0;
    }
    if label == query {
        exact
    } else if label.contains(query) || (query.contains(&label) && label.chars().count() >= 2) {
        partial
    } else {
        0
    }
}

fn is_favorites_query(query: &str) -> bool {
    matches!(normalize(query).as_str(), "favorite" | "favorites" | "my favorite" | "my favorites")
}

pub fn assemble(people: &[Person], system: &[PhoneContact], linked: &HashMap<String, PhoneContact>) -> Value {
    let mut used_ids = Vec::new();
    let mut matches = Vec::new();
    for person in people.iter().take(6) {
        if let Some(id) = person.android_contact_id.as_ref().filter(|id| !id.is_empty()) {
            used_ids.push(id.clone());
        }
        let system_hit = person.android_contact_id.as_ref().and_then(|id| {
            system.iter().find(|hit| &hit.id == id).or_else(|| linked.get(id))
        });
        matches.push(person_match(person, system_hit));
    }
    for hit in system {
        if matches.len() >= 6 {
            break;
        }
        if !hit.id.is_empty() && used_ids.iter().any(|id| id == &hit.id) {
            continue;
        }
        matches.push(system_match(hit));
    }
    let detail = describe_matches(&matches);
    json!({ "ok": true, "matches": matches, "detail": detail })
}

fn person_match(person: &Person, system: Option<&PhoneContact>) -> Value {
    let sys_phone = system.map(|s| s.phone.as_str()).unwrap_or("");
    let sys_email = system.map(|s| s.email.as_str()).unwrap_or("");
    let phone = pick(&person.handles.phone, sys_phone);
    let email = pick(&person.handles.email, sys_email);
    let whatsapp = pick(&person.handles.whatsapp, &phone);
    let signal = pick(&person.handles.signal, &phone);
    let channel = person.preferred_channel.clone();
    let open_tool = channel_tool(&channel);
    let channel_handle = handle_for(&channel, &phone, &email, &whatsapp, &signal);
    json!({
        "name": person.display_name,
        "nickname": person.nickname,
        "aliases": person.aliases,
        "phone": phone,
        "email": email,
        "whatsapp": whatsapp,
        "signal": signal,
        "preferredChannel": channel,
        "favorite": person.favorite,
        "personId": person.id,
        "androidContactId": person.android_contact_id,
        "source": "people",
        "openTool": open_tool,
        "channelHandle": channel_handle,
        "note": clamp(&person.note, 280),
        "linkedName": system.map(|s| s.name.clone()).unwrap_or_default(),
        "reachOutDays": person.reach_out_days,
        "lastContactedMs": person.last_contacted_ms,
        "reminders": person.reminders.iter().map(reminder_value).collect::<Vec<_>>(),
    })
}

fn system_match(hit: &PhoneContact) -> Value {
    json!({
        "name": hit.name,
        "nickname": "",
        "aliases": [],
        "phone": hit.phone,
        "email": hit.email,
        "whatsapp": hit.phone,
        "signal": hit.phone,
        "preferredChannel": "",
        "favorite": false,
        "personId": Value::Null,
        "androidContactId": hit.id,
        "source": "contacts",
        "openTool": "",
        "channelHandle": if !hit.phone.is_empty() { hit.phone.clone() } else { hit.email.clone() },
        "note": "",
        "linkedName": hit.name,
        "reminders": [],
    })
}

fn describe_matches(matches: &[Value]) -> String {
    if matches.is_empty() {
        return "No person matched.".into();
    }
    if matches.len() == 1 {
        return describe_one(&matches[0]);
    }
    let names = matches
        .iter()
        .map(|m| {
            let name = m["name"].as_str().unwrap_or("someone");
            let nick = m["nickname"].as_str().unwrap_or("");
            let channel = channel_label(m["preferredChannel"].as_str().unwrap_or(""));
            let who = if nick.is_empty() { name.to_string() } else { format!("{name} ({nick})") };
            if channel.is_empty() { who } else { format!("{who}, prefers {channel}") }
        })
        .collect::<Vec<_>>()
        .join("; ");
    format!(
        "Found {} matches. Favorites, nicknames, and aliases are listed first. Ask which person before opening anything. {names}",
        matches.len()
    )
}

fn describe_one(match_: &Value) -> String {
    let name = match_["name"].as_str().unwrap_or("that person");
    let nick = match_["nickname"].as_str().unwrap_or("");
    let who = if nick.is_empty() { name.to_string() } else { format!("{name}, nickname {nick}") };
    let channel = match_["preferredChannel"].as_str().unwrap_or("");
    let tool = match_["openTool"].as_str().unwrap_or("");
    let handle = match_["channelHandle"].as_str().unwrap_or("");
    if !channel.is_empty() && !tool.is_empty() {
        let gate = match channel {
            "call" => "The dialer opens. The call is not placed until they tap Call.",
            "email" => "Nothing is sent until they tap Send.",
            _ => "Nothing is sent until they tap Send.",
        };
        return format!(
            "Found {who}. Preferred channel is {} at {handle}. If they asked to call, text, or email, still use this channel. Speak the plan, wait for a clear yes, then call {tool} with confirmed true. {gate}",
            channel_label(channel)
        );
    }
    let phone = match_["phone"].as_str().unwrap_or("");
    let email = match_["email"].as_str().unwrap_or("");
    format!(
        "Found {who}. Phone {phone}. Email {email}. No preferred channel. Speak the match and wait for a clear yes before open_call, open_sms, open_whatsapp, open_signal, or open_email. Nothing is sent, and a call is not placed, until they confirm in that app."
    )
}

/// Facts only. The caller speaks this and must not open a handoff.
pub fn describe_person(app: &AppHandle, name: &str) -> Result<Value, String> {
    let people = app.state::<People>().list();
    let hits = query_reminders(&people, name);
    let mut found = find_contact(app, name)?;
    let mut matches = found.get("matches").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    if matches.is_empty() {
        let shorter = name_part(name);
        if shorter.chars().count() >= 2 && shorter != normalize(name) {
            found = find_contact(app, &shorter)?;
            matches = found.get("matches").and_then(|v| v.as_array()).cloned().unwrap_or_default();
        }
    }
    let detail = if matches.is_empty() && !hits.is_empty() {
        let lines = hits.iter().take(4).map(|hit| speak_reminder(&hit.person_name, &hit.reminder)).collect::<Vec<_>>().join(" ");
        format!("{lines} Answer in this conversation. Do not open any app.")
    } else {
        facts_detail(&matches)
    };
    found["detail"] = json!(detail);
    found["handoff"] = json!(false);
    Ok(found)
}

pub fn facts_detail(matches: &[Value]) -> String {
    if matches.is_empty() {
        return "No person matched. Do not open any app.".into();
    }
    if matches.len() > 1 {
        let names = matches
            .iter()
            .map(|m| {
                let name = m["name"].as_str().unwrap_or("someone");
                let channel = channel_label(m["preferredChannel"].as_str().unwrap_or(""));
                if channel.is_empty() { name.to_string() } else { format!("{name}, prefers {channel}") }
            })
            .collect::<Vec<_>>()
            .join("; ");
        return format!("Several people match. Ask which one. Do not open any app. {names}");
    }
    format!(
        "{} Answer in this conversation. Do not open WhatsApp, Signal, email, the dialer, or a text.",
        speak_one(&matches[0])
    )
}

fn speak_one(match_: &Value) -> String {
    let name = match_["name"].as_str().unwrap_or("that person");
    let nick = match_["nickname"].as_str().unwrap_or("");
    let mut parts = Vec::new();
    if nick.is_empty() {
        parts.push(name.to_string());
    } else {
        parts.push(format!("{name}, nickname {nick}"));
    }
    let aliases = match_["aliases"]
        .as_array()
        .map(|rows| rows.iter().filter_map(|v| v.as_str()).filter(|s| !s.is_empty()).collect::<Vec<_>>())
        .unwrap_or_default();
    if !aliases.is_empty() {
        parts.push(format!("Also called {}.", aliases.join(", ")));
    }
    let channel = channel_label(match_["preferredChannel"].as_str().unwrap_or(""));
    if channel.is_empty() {
        parts.push("No preferred channel.".into());
    } else {
        parts.push(format!("Preferred channel is {channel}."));
    }
    if match_["favorite"] == true {
        parts.push("Favorite.".into());
    }
    let note = match_["note"].as_str().unwrap_or("").trim();
    if note.is_empty() {
        parts.push("No note.".into());
    } else {
        parts.push(format!("Note: {note}."));
    }
    let reach = match_["reachOutDays"].as_u64().unwrap_or(0);
    if reach > 0 {
        parts.push(format!("Reach out every {reach} days."));
    }
    if let Some(ms) = match_["lastContactedMs"].as_i64().filter(|ms| *ms > 0) {
        if let Some(when) = chrono::DateTime::from_timestamp_millis(ms) {
            parts.push(format!("Last call or text {}.", when.with_timezone(&chrono::Local).format("%Y-%m-%d")));
        }
    }
    let linked = match_["linkedName"].as_str().unwrap_or("").trim();
    if !linked.is_empty() {
        parts.push(format!("Linked contact {linked}."));
    }
    match match_["reminders"].as_array() {
        Some(rows) if rows.is_empty() => parts.push("No birthday or other dated reminder.".into()),
        Some(rows) => {
            for reminder in rows {
                parts.push(speak_reminder_value(reminder));
            }
        }
        None => {}
    }
    parts.join(" ")
}

fn reminder_value(reminder: &PersonReminder) -> Value {
    json!({
        "id": reminder.id,
        "label": reminder.label,
        "title": reminder.title,
        "date": reminder.date,
        "recurrence": reminder.recurrence,
        "leadDays": reminder.lead_days,
        "time": reminder.time,
        "enabled": reminder.enabled,
        "speak": speak_reminder("", reminder),
    })
}

/// People saved in the app, including dates that stay on the person all year.
/// Phone numbers stay out of this list so it cannot place a call.
pub fn saved_people_payload(people: &[Person], query: &str) -> Value {
    let q = normalize(query);
    let mut rows: Vec<&Person> = people.iter().filter(|person| person_query_hit(person, &q)).collect();
    rows.sort_by(|a, b| a.display_name.to_ascii_lowercase().cmp(&b.display_name.to_ascii_lowercase()));
    let total = rows.len();
    let truncated = total > 40;
    let listed: Vec<Value> = rows.into_iter().take(40).map(person_listing).collect();
    let count = listed.len();
    let detail = if count == 0 {
        "No people are saved in the app.".to_string()
    } else if truncated {
        format!("Showing {count} of {total} people saved in the app. Dates stay on the person all year. This does not open any app.")
    } else {
        format!("{count} people saved in the app. Dates stay on the person all year. This does not open any app.")
    };
    json!({
        "ok": true,
        "count": count,
        "total": total,
        "truncated": truncated,
        "people": listed,
        "detail": detail
    })
}

fn person_query_hit(person: &Person, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let mut hay = format!("{} {} {} {}", person.display_name, person.nickname, person.aliases.join(" "), person.note);
    for reminder in &person.reminders {
        hay.push(' ');
        hay.push_str(&reminder.label);
        hay.push(' ');
        hay.push_str(&reminder.title);
        hay.push(' ');
        hay.push_str(&reminder.date);
    }
    normalize(&hay).contains(query)
}

fn person_listing(person: &Person) -> Value {
    let dates: Vec<Value> = person
        .reminders
        .iter()
        .filter(|reminder| reminder.enabled)
        .map(|reminder| {
            json!({
                "id": reminder.id,
                "label": reminder.label,
                "title": reminder.title,
                "date": reminder.date,
                "recurrence": reminder.recurrence,
                "leadDays": reminder.lead_days,
                "time": reminder.time,
            })
        })
        .collect();
    json!({
        "id": person.id,
        "name": person.display_name,
        "nickname": person.nickname,
        "aliases": person.aliases,
        "note": person.note,
        "favorite": person.favorite,
        "preferredChannel": person.preferred_channel,
        "reachOutDays": person.reach_out_days,
        "dates": dates,
    })
}

pub fn speak_reminder(person_name: &str, reminder: &PersonReminder) -> String {

    let when = recurrence::parse_month_day(&reminder.date)
        .map(|(_, month, day)| recurrence::speak_date(month, day))
        .unwrap_or_else(|| reminder.date.clone());
    let label = if reminder.title.trim().is_empty() { reminder.label.as_str() } else { reminder.title.trim() };
    let every = if reminder.recurrence == "once" { "once" } else { "every year" };
    let lead = match reminder.lead_days {
        0 => String::new(),
        1 => ", one day before".into(),
        n => format!(", {n} days before"),
    };
    let who = if person_name.is_empty() { String::new() } else { format!("{person_name}: ") };
    format!("{who}{label} on {when}, {every}, at {}{lead}.", reminder.time)
}

fn speak_reminder_value(reminder: &Value) -> String {
    let stored = PersonReminder {
        id: reminder["id"].as_str().unwrap_or("").into(),
        label: reminder["label"].as_str().unwrap_or("custom").into(),
        title: reminder["title"].as_str().unwrap_or("").into(),
        date: reminder["date"].as_str().unwrap_or("").into(),
        recurrence: reminder["recurrence"].as_str().unwrap_or("yearly").into(),
        lead_days: reminder["leadDays"].as_u64().unwrap_or(0) as u32,
        time: reminder["time"].as_str().unwrap_or("09:00").into(),
        enabled: reminder["enabled"].as_bool().unwrap_or(true),
    };
    speak_reminder("", &stored)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReminderHit {
    pub person_id: String,
    pub person_name: String,
    pub nickname: String,
    pub reminder: PersonReminder,
}

/// Rank people the same way as contact lookup, then keep reminders that match
/// words such as "birthday", "anniversary", or a custom date's name.
pub fn query_reminders(people: &[Person], query: &str) -> Vec<ReminderHit> {
    let q = normalize(query);
    let want_birthday = q.contains("birthday") || q.contains("born");
    let want_anniversary = q.contains("anniversary");
    let name_query = name_part(&q);
    let person_query = strip_reminder_titles(people, &name_query);
    let by_name = if person_query.chars().count() >= 2 {
        let hits = rank_people(people, &person_query);
        if !hits.is_empty() {
            hits
        } else if want_birthday || want_anniversary {
            people.to_vec()
        } else {
            hits
        }
    } else if want_birthday || want_anniversary {
        people.to_vec()
    } else {
        rank_people(people, &q)
    };
    let title_people = people_with_title(people, &q);
    let title_only = by_name.is_empty();
    let ranked = if title_only { title_people } else { by_name };
    let mut hits = Vec::new();
    for person in ranked {
        for reminder in &person.reminders {
            if !reminder.enabled {
                continue;
            }
            let titled = title_matches(reminder, &q);
            if title_only && !titled {
                continue;
            }
            if !title_only && titled {
                hits.push(hit(&person, reminder));
                continue;
            }
            if want_birthday && !want_anniversary && reminder.label != "birthday" {
                continue;
            }
            if want_anniversary && !want_birthday && reminder.label != "anniversary" {
                continue;
            }
            if !want_birthday && !want_anniversary && query_names_a_title(people, &q) && !titled {
                continue;
            }
            hits.push(hit(&person, reminder));
        }
    }
    hits
}

fn hit(person: &Person, reminder: &PersonReminder) -> ReminderHit {
    ReminderHit {
        person_id: person.id.clone(),
        person_name: person.display_name.clone(),
        nickname: person.nickname.clone(),
        reminder: reminder.clone(),
    }
}

pub fn channel_tool(channel: &str) -> &'static str {
    match channel {
        "call" => "open_call",
        "sms" => "open_sms",
        "whatsapp" => "open_whatsapp",
        "signal" => "open_signal",
        "email" => "open_email",
        _ => "",
    }
}

fn channel_label(channel: &str) -> &'static str {
    match channel {
        "call" => "call",
        "sms" => "text",
        "whatsapp" => "WhatsApp",
        "signal" => "Signal",
        "email" => "email",
        _ => "",
    }
}

fn handle_for(channel: &str, phone: &str, email: &str, whatsapp: &str, signal: &str) -> String {
    match channel {
        "email" => email.to_string(),
        "whatsapp" => whatsapp.to_string(),
        "signal" => signal.to_string(),
        "call" | "sms" => phone.to_string(),
        _ => {
            if !phone.is_empty() { phone.to_string() } else { email.to_string() }
        }
    }
}

fn pick(override_value: &str, system: &str) -> String {
    let chosen = if override_value.trim().is_empty() { system } else { override_value };
    chosen.trim().to_string()
}

fn read_system_contacts(app: &AppHandle, name: &str) -> (Vec<PhoneContact>, Option<String>) {
    let bridge = app.state::<NativeBridge>();
    let mut raw = match bridge.lookup_contacts(name) {
        Ok(raw) => raw,
        Err(e) => return (Vec::new(), Some(e.to_string())),
    };
    if raw.contains("not granted") {
        let _ = bridge.request_contacts();
        raw = match bridge.lookup_contacts(name) {
            Ok(raw) => raw,
            Err(e) => return (Vec::new(), Some(e.to_string())),
        };
    }
    parse_contact_payload(&raw)
}

fn read_one_contact(app: &AppHandle, id: &str) -> Option<PhoneContact> {
    let raw = app.state::<NativeBridge>().contact_handles(id).ok()?;
    let (mut rows, error) = parse_contact_payload(&raw);
    if error.is_some() {
        return None;
    }
    rows.pop()
}

fn parse_contact_payload(raw: &str) -> (Vec<PhoneContact>, Option<String>) {
    let value: Value = match serde_json::from_str(raw) {
        Ok(value) => value,
        Err(e) => return (Vec::new(), Some(e.to_string())),
    };
    if value["ok"] == false {
        return (Vec::new(), Some(value["error"].as_str().unwrap_or("Contacts lookup failed.").into()));
    }
    let matches = value.get("matches").or_else(|| value.get("contacts")).cloned().unwrap_or(Value::Array(Vec::new()));
    let rows = serde_json::from_value(matches).unwrap_or_default();
    (rows, None)
}

pub fn normalize_channel(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "call" | "phone" | "dial" => "call".into(),
        "sms" | "text" | "message" => "sms".into(),
        "whatsapp" | "whats app" => "whatsapp".into(),
        "signal" => "signal".into(),
        "email" | "mail" | "e-mail" => "email".into(),
        _ => String::new(),
    }
}

pub fn normalize_reminder(reminder: &PersonReminder, index: usize) -> Result<PersonReminder, String> {
    let date_raw = reminder.date.trim();
    if date_raw.is_empty() {
        return Err("A reminder needs a date as MM-DD.".into());
    }
    let (year, month, day) = recurrence::parse_month_day(date_raw).ok_or_else(|| format!("Could not read the date '{date_raw}'. Use MM-DD."))?;
    let label = match reminder.label.trim().to_ascii_lowercase().as_str() {
        "birthday" | "bday" | "born" => "birthday",
        "anniversary" => "anniversary",
        _ => "custom",
    };
    if label == "custom" && reminder.title.trim().is_empty() {
        return Err("A custom date needs a name.".into());
    }
    let recurrence = if recurrence::normalize_kind(&reminder.recurrence) == "once" { "once" } else { "yearly" };
    let clock = if reminder.time.trim().is_empty() { "09:00" } else { reminder.time.trim() };
    let (hour, minute) = recurrence::parse_clock(clock).ok_or_else(|| "Time should look like 09:00.".to_string())?;
    let date = if recurrence == "once" {
        let year = year.unwrap_or_else(|| upcoming_year(month, day));
        recurrence::format_month_day(Some(year), month, day)
    } else {
        recurrence::format_month_day(None, month, day)
    };
    let id = reminder.id.trim();
    Ok(PersonReminder {
        id: if id.is_empty() { format!("r{}{index}", chrono::Local::now().timestamp_millis()) } else { id.to_string() },
        label: label.into(),
        title: if label == "custom" { clamp(&reminder.title, 80) } else { String::new() },
        date,
        recurrence: recurrence.into(),
        lead_days: reminder.lead_days.min(30),
        time: format!("{hour:02}:{minute:02}"),
        enabled: reminder.enabled,
    })
}

fn clean_reminders(reminders: &[PersonReminder]) -> Result<Vec<PersonReminder>, String> {
    let mut out = Vec::new();
    for reminder in reminders {
        if reminder.date.trim().is_empty() && reminder.title.trim().is_empty() && reminder.label.trim().is_empty() {
            continue;
        }
        out.push(normalize_reminder(reminder, out.len())?);
        if out.len() == 12 {
            break;
        }
    }
    Ok(out)
}

fn clean_aliases(aliases: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for alias in aliases {
        for part in alias.split([',', '\n']) {
            let part = clamp(part, 80);
            if part.is_empty() || out.iter().any(|existing: &String| existing.eq_ignore_ascii_case(&part)) {
                continue;
            }
            out.push(part);
            if out.len() == 12 {
                return out;
            }
        }
    }
    out
}

fn clean_handles(handles: &Handles) -> Handles {
    Handles {
        phone: clamp(&handles.phone, 80),
        email: clamp(&handles.email, 120),
        whatsapp: clamp(&handles.whatsapp, 80),
        signal: clamp(&handles.signal, 80),
    }
}

fn clamp(value: &str, max: usize) -> String {
    value.trim().chars().take(max).collect()
}

fn normalize(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

fn load_store(bytes: &[u8]) -> (Store, bool) {
    if bytes.is_empty() {
        return (Store::default(), false);
    }
    let raw: Value = match serde_json::from_slice(bytes) {
        Ok(value) => value,
        Err(_) => return (Store::default(), false),
    };
    let mut store = serde_json::from_slice::<Store>(bytes).unwrap_or_else(|_| store_from_loose(&raw));
    let dirty = migrate_legacy_birthdays(&mut store, &raw) | fill_reminder_ids(&mut store);
    (store, dirty)
}

fn store_from_loose(raw: &Value) -> Store {
    let Some(people) = raw.get("people").and_then(|value| value.as_array()) else {
        return Store::default();
    };
    let mut store = Store::default();
    for raw_person in people {
        let Some(mut clone) = raw_person.as_object().cloned() else { continue };
        if let Some(reminders) = clone.get_mut("reminders").and_then(|value| value.as_array_mut()) {
            reminders.retain(|reminder| reminder.get("date").and_then(|date| date.as_str()).is_some_and(|date| !date.trim().is_empty()));
            for (index, reminder) in reminders.iter_mut().enumerate() {
                let Some(object) = reminder.as_object_mut() else { continue };
                let missing = object.get("id").and_then(|id| id.as_str()).is_none_or(|id| id.trim().is_empty());
                if missing {
                    object.insert("id".into(), json!(format!("m{index}")));
                }
            }
        } else {
            clone.insert("reminders".into(), json!([]));
        }
        clone.remove("birthday");
        let Ok(person) = serde_json::from_value::<Person>(Value::Object(clone)) else { continue };
        if person.display_name.trim().is_empty() {
            continue;
        }
        store.people.push(person);
    }
    store
}

fn migrate_legacy_birthdays(store: &mut Store, raw: &Value) -> bool {
    let Some(people) = raw.get("people").and_then(|value| value.as_array()) else {
        return false;
    };
    let mut dirty = false;
    for raw_person in people {
        let Some(date) = raw_person.get("birthday").and_then(|value| value.as_str()).map(str::trim).filter(|date| !date.is_empty()) else {
            continue;
        };
        let id = raw_person.get("id").and_then(|value| value.as_str()).unwrap_or("").trim();
        let name = raw_person.get("displayName").and_then(|value| value.as_str()).unwrap_or("").trim();
        let Some(person) = store.people.iter_mut().find(|person| {
            (!id.is_empty() && person.id == id) || (id.is_empty() && !name.is_empty() && person.display_name == name)
        }) else {
            continue;
        };
        if person.reminders.iter().any(|reminder| reminder.label == "birthday") {
            continue;
        }
        person.reminders.push(PersonReminder {
            id: format!("bday-{}", person.id),
            label: "birthday".into(),
            title: String::new(),
            date: date.to_string(),
            recurrence: "yearly".into(),
            lead_days: 0,
            time: "09:00".into(),
            enabled: true,
        });
        dirty = true;
    }
    dirty
}

fn fill_reminder_ids(store: &mut Store) -> bool {
    let mut dirty = false;
    for person in &mut store.people {
        for (index, reminder) in person.reminders.iter_mut().enumerate() {
            if reminder.id.trim().is_empty() {
                reminder.id = format!("r-{}-{index}", person.id);
                dirty = true;
            }
        }
    }
    dirty
}

fn same_date(left: &str, right: &str) -> bool {
    let (Some((year_left, month_left, day_left)), Some((year_right, month_right, day_right))) =
        (recurrence::parse_month_day(left), recurrence::parse_month_day(right))
    else {
        return left.trim().eq_ignore_ascii_case(right.trim());
    };
    if month_left != month_right || day_left != day_right {
        return false;
    }
    match (year_left, year_right) {
        (Some(left), Some(right)) => left == right,
        _ => true,
    }
}

fn same_title(left: &str, right: &str) -> bool {
    normalize(left) == normalize(right)
}

fn title_matches(reminder: &PersonReminder, query: &str) -> bool {
    let title = normalize(&reminder.title);
    let title_words: Vec<&str> = title.split_whitespace().filter(|word| word.chars().count() >= 3).collect();
    if title_words.is_empty() {
        return false;
    }
    let words: Vec<&str> = query.split_whitespace().collect();
    words.windows(title_words.len()).any(|window| window == title_words.as_slice())
}

fn people_with_title(people: &[Person], query: &str) -> Vec<Person> {
    people.iter().filter(|person| person.reminders.iter().any(|reminder| reminder.enabled && title_matches(reminder, query))).cloned().collect()
}

fn query_names_a_title(people: &[Person], query: &str) -> bool {
    people.iter().any(|person| person.reminders.iter().any(|reminder| title_matches(reminder, query)))
}

fn strip_reminder_titles(people: &[Person], query: &str) -> String {
    let mut words: Vec<&str> = query.split_whitespace().collect();
    for person in people {
        for reminder in &person.reminders {
            let title = normalize(&reminder.title);
            if title.chars().count() < 3 {
                continue;
            }
            for part in title.split_whitespace() {
                words.retain(|word| *word != part);
            }
        }
    }
    words.join(" ")
}

fn name_part(query: &str) -> String {
    const DROP: &[&str] = &[
        "when", "whats", "what", "is", "the", "my", "a", "for", "on", "of", "me", "her", "his", "their", "reminder",
        "reminders", "birthday", "anniversary", "born", "note", "channel", "preferred",
    ];
    query
        .split_whitespace()
        .filter_map(|word| {
            let word = word.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '\'');
            let word = word.strip_suffix("'s").unwrap_or(word);
            if DROP.contains(&word) || word.chars().count() < 2 { None } else { Some(word) }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Create or, when that exact name is already saved, update the People record.
/// This writes `people.json` only. It does not touch the phone address book.
pub fn create_from_tool(store: &People, args: &Value) -> Result<Value, String> {
    let name = required_name(args)?;
    let people = store.list();
    match resolve_saved(&people, &name) {
        SavedHit::Many(names) => return Err(many_message(&names)),
        SavedHit::One(existing) if exact_identity(existing, &name) => {
            if !has_person_edit(&args) {
                return Ok(voice_payload(existing, false, false, &format!("{name} is already in People.")));
            }
            let mut args = args.clone();
            if let Some(object) = args.as_object_mut() {
                object.insert("id".into(), json!(existing.id));
            }
            let mut value = update_existing(store, existing, &args)?;
            let detail = value["detail"].as_str().unwrap_or("").to_string();
            value["detail"] = json!(format!("{name} is already in People. {detail}"));
            value["created"] = json!(false);
            return Ok(value);
        }
        SavedHit::One(_) | SavedHit::None => {}
    }
    let mut draft = blank_draft(&name);
    overlay(&mut draft, args)?;
    draft.display_name = display_from(args).unwrap_or(name);
    let date_clause = apply_date(&mut draft, args)?;
    let saved = store.save(draft)?;
    Ok(voice_payload(&saved, true, true, &ack_added(&saved.display_name, date_clause.as_deref())))
}

pub fn update_from_tool(store: &People, args: &Value) -> Result<Value, String> {
    if arg_string(args.get("name")).is_none() && arg_string(args.get("person")).is_none() && arg_string(args.get("id")).is_none() {
        if let Some(days) = arg_u32(args.get("leadDays")) {
            let label = arg_string(args.get("label")).unwrap_or_else(|| "birthday".into());
            return set_lead_days(store, &label, days);
        }
        return Err("Say which person to change.".into());
    }
    let people = store.list();
    let person = saved_target(&people, args)?;
    update_existing(store, &person, args)
}

pub fn delete_from_tool(store: &People, args: &Value) -> Result<Value, String> {
    let people = store.list();
    let person = saved_target(&people, args)?;
    if arg_bool(args.get("confirmed")) != Some(true) {
        return Ok(json!({
            "ok": true,
            "deleted": false,
            "changed": false,
            "needsConfirm": true,
            "confirmKind": "delete_person",
            "personId": person.id,
            "name": person.display_name,
            "detail": format!("Delete {} from People? This does not change the phone's Contacts app.", person.display_name)
        }));
    }
    store.delete(&person.id)?;
    Ok(json!({
        "ok": true,
        "deleted": true,
        "changed": true,
        "needsConfirm": false,
        "name": person.display_name,
        "detail": format!("Deleted {} from People. The phone's Contacts app was not changed.", person.display_name)
    }))
}

/// Add or replace one date. Creates the person when they are not saved yet.
pub fn write_person_date(store: &People, args: &Value) -> Result<Value, String> {
    match update_from_tool(store, args) {
        Err(err) if err.contains("Call create_person") => create_from_tool(store, args),
        other => other,
    }
}

pub fn set_lead_days(store: &People, label: &str, days: u32) -> Result<Value, String> {
    let days = days.min(30);
    let want = canonical_label(label);
    let title_query = if want.is_empty() { normalize(label) } else { String::new() };
    let mut matched = 0;
    let mut changed_people = 0;
    for person in store.list() {
        let mut draft = draft_of(&person);
        let mut touched = false;
        for reminder in &mut draft.reminders {
            let matches = if !title_query.is_empty() && title_query != "custom" {
                reminder.label == "custom" && (normalize(&reminder.title) == title_query || normalize(&reminder.title).contains(&title_query))
            } else if want.is_empty() {
                true
            } else {
                reminder.label == want
            };
            if !matches {
                continue;
            }
            matched += 1;
            if reminder.lead_days != days {
                reminder.lead_days = days;
                touched = true;
            }
        }
        if touched {
            store.save(draft)?;
            changed_people += 1;
        }
    }
    let which = if !title_query.is_empty() && title_query != "custom" {
        label.trim().to_string()
    } else if want.is_empty() {
        "Dates".into()
    } else if want == "birthday" {
        "Birthdays".into()
    } else if want == "anniversary" {
        "Anniversaries".into()
    } else {
        want
    };
    if matched == 0 {
        return Ok(json!({
            "ok": true,
            "changed": false,
            "detail": format!("No saved {which} to update. Add the date on a person first.")
        }));
    }
    let when = match days {
        0 => "on the day".into(),
        1 => "1 day before".into(),
        n => format!("{n} days before"),
    };
    if changed_people == 0 {
        return Ok(json!({
            "ok": true,
            "changed": false,
            "detail": format!("{which} already notify {when}. The date stays on the person all year.")
        }));
    }
    Ok(json!({
        "ok": true,
        "changed": true,
        "detail": format!("{which} now notify {when}. The date stays on the person all year.")
    }))
}

fn update_existing(store: &People, person: &Person, args: &Value) -> Result<Value, String> {
    if !has_person_edit(args) {
        return Err("Say what to change.".into());
    }
    let previous = person.display_name.clone();
    let mut draft = draft_of(person);
    overlay(&mut draft, args)?;
    let date_clause = apply_date(&mut draft, args)?;
    let saved = store.save(draft)?;
    Ok(voice_payload(&saved, true, false, &ack_updated(&previous, &saved, date_clause.as_deref(), args.get("note").is_some())))
}

fn saved_target(people: &[Person], args: &Value) -> Result<Person, String> {
    if let Some(id) = arg_string(args.get("id")).filter(|id| !id.is_empty()) {
        return people.iter().find(|person| person.id == id).cloned().ok_or_else(|| "No person with that id.".to_string());
    }
    let name = required_name(args)?;
    match resolve_saved(people, &name) {
        SavedHit::One(person) => Ok(person.clone()),
        SavedHit::None => Err(format!("No one named {name} is in People. Call create_person to add them.")),
        SavedHit::Many(names) => Err(many_message(&names)),
    }
}

fn blank_draft(name: &str) -> PersonDraft {
    PersonDraft {
        id: String::new(),
        android_contact_id: None,
        display_name: name.to_string(),
        nickname: String::new(),
        aliases: Vec::new(),
        preferred_channel: String::new(),
        handles: Handles::default(),
        favorite: false,
        note: String::new(),
        reminders: Vec::new(),
        reach_out_days: 0,
        last_contacted_ms: None,
    }
}

fn draft_of(person: &Person) -> PersonDraft {
    PersonDraft {
        id: person.id.clone(),
        android_contact_id: person.android_contact_id.clone(),
        display_name: person.display_name.clone(),
        nickname: person.nickname.clone(),
        aliases: person.aliases.clone(),
        preferred_channel: person.preferred_channel.clone(),
        handles: person.handles.clone(),
        favorite: person.favorite,
        note: person.note.clone(),
        reminders: person.reminders.clone(),
        reach_out_days: person.reach_out_days,
        last_contacted_ms: person.last_contacted_ms,
    }
}

fn overlay(draft: &mut PersonDraft, args: &Value) -> Result<(), String> {
    if args.get("androidContactId").is_some() || args.get("android_contact_id").is_some() {
        return Err("Phone contacts stay import-only. Say the name, and People keeps its own record.".into());
    }
    if args.get("lastContacted").is_some() || args.get("lastContactedMs").is_some() {
        return Err("Last contacted comes from call and text history in People.".into());
    }
    if let Some(name) = display_from(args) {
        if name.is_empty() {
            return Err("A person needs a name.".into());
        }
        draft.display_name = name;
    }
    if let Some(nickname) = arg_string(args.get("nickname")) {
        draft.nickname = nickname;
    }
    if let Some(aliases) = args.get("aliases") {
        draft.aliases = aliases_from(aliases);
    }
    if let Some(note) = arg_string(args.get("note")) {
        draft.note = note;
    }
    if let Some(favorite) = arg_bool(args.get("favorite")) {
        draft.favorite = favorite;
    }
    if let Some(channel) = arg_string(args.get("preferredChannel")).or_else(|| arg_string(args.get("channel"))) {
        draft.preferred_channel = channel;
    }
    if let Some(days) = arg_u32(args.get("reachOutDays")) {
        draft.reach_out_days = days;
    }
    if let Some(phone) = arg_string(args.get("phone")) {
        draft.handles.phone = phone;
    }
    if let Some(email) = arg_string(args.get("email")) {
        draft.handles.email = email;
    }
    if let Some(whatsapp) = arg_string(args.get("whatsapp")) {
        draft.handles.whatsapp = whatsapp;
    }
    if let Some(signal) = arg_string(args.get("signal")) {
        draft.handles.signal = signal;
    }
    Ok(())
}

fn apply_date(draft: &mut PersonDraft, args: &Value) -> Result<Option<String>, String> {
    if !date_args(args) {
        return Ok(None);
    }
    let remove = arg_bool(args.get("removeDate")) == Some(true);
    let id = arg_string(args.get("dateId")).unwrap_or_default();
    let mut label = arg_string(args.get("label")).unwrap_or_default();
    let mut title = arg_string(args.get("title")).or_else(|| arg_string(args.get("dateTitle"))).unwrap_or_default();
    promote_custom(&mut label, &mut title);
    let date = arg_string(args.get("date")).unwrap_or_default();
    if date.is_empty() && canonical_label(&label).is_empty() && title.is_empty() && label.is_empty() {
        if let Some(days) = arg_u32(args.get("leadDays")) {
            if draft.reminders.is_empty() {
                return Err("No dates on that person.".into());
            }
            for reminder in &mut draft.reminders {
                reminder.lead_days = days.min(30);
            }
            return Ok(Some(format!("Notify {}.", match days.min(30) {
                0 => "on the day".into(),
                1 => "1 day before".into(),
                n => format!("{n} days before"),
            })));
        }
    }
    let index = reminder_index(&draft.reminders, &id, &label, &title)?;
    if remove {
        let Some(index) = index else {
            return Err("No matching date to remove.".into());
        };
        let removed = draft.reminders.remove(index);
        return Ok(Some(format!("Removed {}.", date_name(&removed))));
    }
    if date.is_empty() && index.is_none() && arg_string(args.get("label")).is_some() && args.get("leadDays").is_none() && args.get("title").is_none() {
        return Err("Say the date, such as June 15 or 6/15.".into());
    }
    if date.is_empty() && index.is_none() {
        return Err("Say the date, such as June 15, 6/15, or --06-15.".into());
    }
    let mut reminder = index.map(|index| draft.reminders[index].clone()).unwrap_or_else(|| PersonReminder {
        id: String::new(),
        label: if canonical_label(&label).is_empty() { "custom".into() } else { canonical_label(&label) },
        title: title.clone(),
        date: String::new(),
        recurrence: "yearly".into(),
        lead_days: 0,
        time: String::new(),
        enabled: true,
    });
    if !canonical_label(&label).is_empty() {
        reminder.label = canonical_label(&label);
    } else if !label.is_empty() {
        reminder.label = "custom".into();
    }
    if !title.is_empty() {
        reminder.title = title;
    }
    if !date.is_empty() {
        reminder.date = date;
    }
    if let Some(recurrence) = arg_string(args.get("recurrence")) {
        reminder.recurrence = recurrence;
    }
    if let Some(time) = arg_string(args.get("time")) {
        reminder.time = time;
    }
    if let Some(days) = arg_u32(args.get("leadDays")) {
        reminder.lead_days = days;
    }
    if let Some(enabled) = arg_bool(args.get("enabled")) {
        reminder.enabled = enabled;
    }
    let normalized = normalize_reminder(&reminder, index.unwrap_or(draft.reminders.len()))?;
    let clause = format!("{} on {}.", date_name(&normalized), spoken_month_day(&normalized.date));
    if let Some(index) = index {
        draft.reminders[index] = normalized;
    } else {
        draft.reminders.push(normalized);
    }
    Ok(Some(clause))
}

fn date_args(args: &Value) -> bool {
    ["date", "label", "title", "dateTitle", "recurrence", "time", "leadDays", "removeDate", "enabled", "dateId"]
        .iter()
        .any(|key| args.get(*key).is_some())
}

fn has_person_edit(args: &Value) -> bool {
    ["displayName", "rename", "nickname", "aliases", "note", "favorite", "preferredChannel", "channel", "reachOutDays", "phone", "email", "whatsapp", "signal"]
        .iter()
        .any(|key| args.get(*key).is_some())
        || date_args(args)
}

fn reminder_index(reminders: &[PersonReminder], id: &str, label: &str, title: &str) -> Result<Option<usize>, String> {
    if !id.is_empty() {
        return reminders.iter().position(|reminder| reminder.id == id).map(Some).ok_or_else(|| "No date with that id.".to_string());
    }
    let label_key = canonical_label(label);
    if label_key == "birthday" || label_key == "anniversary" {
        return Ok(reminders.iter().position(|reminder| reminder.label == label_key));
    }
    if !title.is_empty() {
        let query = normalize(&title);
        let exact: Vec<usize> = reminders
            .iter()
            .enumerate()
            .filter(|(_, reminder)| normalize(&reminder.title) == query)
            .map(|(index, _)| index)
            .collect();
        if exact.len() == 1 {
            return Ok(Some(exact[0]));
        }
        if exact.len() > 1 {
            return Err("Several dates use that name. Say which one.".into());
        }
        let partial: Vec<usize> = reminders
            .iter()
            .enumerate()
            .filter(|(_, reminder)| {
                let stored = normalize(&reminder.title);
                !stored.is_empty() && (stored.contains(&query) || query.contains(&stored))
            })
            .map(|(index, _)| index)
            .collect();
        if partial.len() == 1 {
            return Ok(Some(partial[0]));
        }
        if partial.len() > 1 {
            return Err("Several dates match that name. Say which one.".into());
        }
        return Ok(None);
    }
    if label_key == "custom" {
        let custom: Vec<usize> = reminders.iter().enumerate().filter(|(_, reminder)| reminder.label == "custom").map(|(index, _)| index).collect();
        if custom.len() <= 1 {
            return Ok(custom.first().copied());
        }
        return Err("Say the name of the date, such as visa.".into());
    }
    Ok(None)
}

fn promote_custom(label: &mut String, title: &mut String) {
    if label.trim().is_empty() || !canonical_label(label).is_empty() {
        return;
    }
    if title.trim().is_empty() {
        *title = label.trim().to_string();
    }
    *label = "custom".into();
}

fn canonical_label(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "birthday" | "birthdays" | "bday" | "born" => "birthday".into(),
        "anniversary" | "anniversaries" => "anniversary".into(),
        "custom" => "custom".into(),
        _ => String::new(),
    }
}

fn date_name(reminder: &PersonReminder) -> String {
    if reminder.label == "custom" {
        if reminder.title.trim().is_empty() { "custom date".into() } else { reminder.title.trim().to_string() }
    } else {
        reminder.label.clone()
    }
}

fn spoken_month_day(date: &str) -> String {
    recurrence::parse_month_day(date).map(|(_, month, day)| recurrence::speak_date(month, day)).unwrap_or_else(|| date.to_string())
}

fn ack_added(name: &str, date: Option<&str>) -> String {
    match date {
        Some(date) => format!("Added {name}. {date}"),
        None => format!("Added {name}."),
    }
}

fn ack_updated(previous: &str, person: &Person, date: Option<&str>, note_saved: bool) -> String {
    let mut parts = Vec::new();
    if !same_name(previous, &person.display_name) {
        parts.push(format!("Renamed {previous} to {}.", person.display_name));
    } else {
        parts.push(format!("Updated {}.", person.display_name));
    }
    if let Some(date) = date {
        parts.push(date.to_string());
    }
    if note_saved {
        parts.push("Note saved.".into());
    }
    parts.join(" ")
}

fn exact_identity(person: &Person, query: &str) -> bool {
    same_name(&person.display_name, query) || same_name(&person.nickname, query) || person.aliases.iter().any(|alias| same_name(alias, query))
}

fn voice_payload(person: &Person, changed: bool, created: bool, detail: &str) -> Value {
    json!({
        "ok": true,
        "changed": changed,
        "created": created,
        "saved": true,
        "personId": person.id,
        "person": person_voice_view(person),
        "detail": detail
    })
}

fn person_voice_view(person: &Person) -> Value {
    json!({
        "id": person.id,
        "name": person.display_name,
        "nickname": person.nickname,
        "aliases": person.aliases,
        "note": person.note,
        "favorite": person.favorite,
        "preferredChannel": person.preferred_channel,
        "reachOutDays": person.reach_out_days,
        "dates": person.reminders.iter().filter(|reminder| reminder.enabled).map(reminder_value).collect::<Vec<_>>()
    })
}

enum SavedHit<'a> {
    One(&'a Person),
    None,
    Many(Vec<String>),
}

fn resolve_saved<'a>(people: &'a [Person], query: &str) -> SavedHit<'a> {
    let query = query.trim();
    if query.chars().count() < 2 {
        return SavedHit::None;
    }
    let folded = normalize(query);
    let exact: Vec<&Person> = people
        .iter()
        .filter(|person| same_name(&person.display_name, query) || same_name(&person.nickname, query) || person.aliases.iter().any(|alias| same_name(alias, query)))
        .collect();
    if exact.len() == 1 {
        return SavedHit::One(exact[0]);
    }
    if exact.len() > 1 {
        return SavedHit::Many(exact.into_iter().map(|person| person.display_name.clone()).collect());
    }
    let hits = rank_people(people, &folded);
    match hits.len() {
        0 => SavedHit::None,
        1 => people.iter().find(|person| person.id == hits[0].id).map(SavedHit::One).unwrap_or(SavedHit::None),
        _ => SavedHit::Many(hits.iter().take(4).map(|person| person.display_name.clone()).collect()),
    }
}

fn same_name(label: &str, query: &str) -> bool {
    normalize(label) == normalize(query)
}

fn many_message(names: &[String]) -> String {
    format!("Several people match. Ask which one. {}", names.join(", "))
}

fn required_name(args: &Value) -> Result<String, String> {
    arg_string(args.get("name")).or_else(|| arg_string(args.get("person"))).filter(|name| !name.is_empty()).ok_or_else(|| "Say the person's name.".to_string())
}

fn display_from(args: &Value) -> Option<String> {
    arg_string(args.get("displayName")).or_else(|| arg_string(args.get("rename")))
}

fn aliases_from(value: &Value) -> Vec<String> {
    match value {
        Value::Array(rows) => rows.iter().filter_map(|row| row.as_str().map(|text| text.to_string())).collect(),
        Value::String(text) => vec![text.clone()],
        _ => Vec::new(),
    }
}

fn arg_string(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(text) => Some(text.trim().to_string()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(on) => Some(if *on { "true".into() } else { "false".into() }),
        _ => None,
    }
}

fn arg_bool(value: Option<&Value>) -> Option<bool> {
    match value? {
        Value::Bool(on) => Some(*on),
        Value::Number(number) => number.as_i64().map(|n| n != 0),
        Value::String(text) => match text.trim().to_ascii_lowercase().as_str() {
            "true" | "yes" | "on" | "1" => Some(true),
            "false" | "no" | "off" | "0" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

fn arg_u32(value: Option<&Value>) -> Option<u32> {
    match value? {
        Value::Number(number) => number.as_u64().map(|n| n.min(u32::MAX as u64) as u32),
        Value::String(text) => {
            if text.trim().eq_ignore_ascii_case("off") {
                return Some(0);
            }
            let digits: String = text.chars().skip_while(|c| !c.is_ascii_digit()).take_while(|c| c.is_ascii_digit()).collect();
            digits.parse().ok()
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn person(name: &str, nick: &str, aliases: &[&str], channel: &str, favorite: bool) -> Person {
        Person {
            id: format!("p-{name}"),
            android_contact_id: Some("42".into()),
            display_name: name.into(),
            nickname: nick.into(),
            aliases: aliases.iter().map(|s| (*s).to_string()).collect(),
            preferred_channel: channel.into(),
            handles: Handles { phone: String::new(), email: String::new(), whatsapp: "+1999".into(), signal: String::new() },
            favorite: favorite,
            note: String::new(),
            reminders: Vec::new(),
            reach_out_days: 0,
            last_contacted_ms: None,
        }
    }

    fn birthday(person: &mut Person, date: &str) {
        person.reminders.push(PersonReminder {
            id: format!("r-{}", person.id),
            label: "birthday".into(),
            title: String::new(),
            date: date.into(),
            recurrence: "yearly".into(),
            lead_days: 0,
            time: "09:00".into(),
            enabled: true,
        });
    }

    #[test]
    fn nickname_and_alias_outrank_display_name() {
        let people = vec![
            person("Ana Lopez", "", &[], "", false),
            person("Sam Rivera", "Mom", &["mummy"], "whatsapp", false),
        ];
        let hits = rank_people(&people, "mom");
        assert_eq!(hits[0].display_name, "Sam Rivera");
        let alias = rank_people(&people, "Mummy");
        assert_eq!(alias[0].nickname, "Mom");
        let display = rank_people(&people, "ana");
        assert_eq!(display[0].display_name, "Ana Lopez");
    }

    #[test]
    fn favorites_rank_ahead_of_a_similar_name() {
        let people = vec![
            person("Mina Cole", "Min", &[], "", false),
            person("Mina Cole Jr", "Min", &[], "sms", true),
        ];
        let hits = rank_people(&people, "min");
        assert!(hits[0].favorite);
        let favs = rank_people(&people, "favorites");
        assert_eq!(favs.len(), 1);
        assert_eq!(favs[0].display_name, "Mina Cole Jr");
    }

    #[test]
    fn preferred_channel_uses_override_and_system_phone() {
        let mut saved = person("Sam Rivera", "Mom", &["mummy"], "whatsapp", true);
        saved.reminders.clear();
        let system = PhoneContact { id: "42".into(), name: "Sam Rivera".into(), phone: "+1555".into(), email: "sam@example.com".into() };
        let other = PhoneContact { id: "7".into(), name: "Sam Other".into(), phone: "+1000".into(), email: String::new() };
        let alone = assemble(std::slice::from_ref(&saved), &[], &HashMap::new());
        assert!(alone["detail"].as_str().unwrap().contains("open_whatsapp"));
        assert!(alone["detail"].as_str().unwrap().contains("tap Send"));
        let value = assemble(&[saved], &[system, other], &HashMap::new());
        let matches = value["matches"].as_array().unwrap();
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0]["source"], "people");
        assert_eq!(matches[0]["phone"], "+1555");
        assert_eq!(matches[0]["whatsapp"], "+1999");
        assert_eq!(matches[0]["openTool"], "open_whatsapp");
        assert_eq!(matches[0]["channelHandle"], "+1999");
        assert!(value["detail"].as_str().unwrap().contains("prefers WhatsApp"));
        assert_eq!(matches[1]["androidContactId"], "7");
    }

    #[test]
    fn linked_handles_fill_when_the_name_search_misses() {
        let saved = person("Sam Rivera", "Mom", &[], "call", false);
        let mut linked = HashMap::new();
        linked.insert("42".into(), PhoneContact { id: "42".into(), name: "Sam Rivera".into(), phone: "+1555".into(), email: "sam@example.com".into() });
        let value = assemble(&[saved], &[], &linked);
        let row = &value["matches"][0];
        assert_eq!(row["phone"], "+1555");
        assert_eq!(row["openTool"], "open_call");
        assert!(value["detail"].as_str().unwrap().contains("not placed"));
    }

    #[test]
    fn birthday_query_picks_the_person_and_facts_do_not_open_apps() {
        let mut sam = person("Sam Rivera", "Mom", &["mummy"], "whatsapp", true);
        sam.note = "call after 5".into();
        birthday(&mut sam, "03-03");
        let mut ana = person("Ana Lopez", "", &[], "sms", false);
        ana.reminders.push(PersonReminder {
            id: "r-ana".into(),
            label: "anniversary".into(),
            title: String::new(),
            date: "06-01".into(),
            recurrence: "yearly".into(),
            lead_days: 0,
            time: "09:00".into(),
            enabled: true,
        });
        let hits = query_reminders(&[ana.clone(), sam.clone()], "when's Sam's birthday");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].person_name, "Sam Rivera");
        assert_eq!(hits[0].reminder.date, "03-03");
        let all = query_reminders(&[ana.clone(), sam.clone()], "birthday");
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].person_name, "Sam Rivera");
        let anniversaries = query_reminders(&[ana, sam.clone()], "anniversary");
        assert_eq!(anniversaries.len(), 1);
        assert_eq!(anniversaries[0].person_name, "Ana Lopez");

        let system = PhoneContact { id: "42".into(), name: "Samantha Rivera".into(), phone: "+1555".into(), email: String::new() };
        let value = assemble(std::slice::from_ref(&sam), std::slice::from_ref(&system), &HashMap::new());
        let detail = facts_detail(value["matches"].as_array().unwrap());
        assert!(detail.contains("Preferred channel is WhatsApp"));
        assert!(detail.contains("birthday on March 3"));
        assert!(detail.contains("Note: call after 5"));
        assert!(detail.contains("Linked contact Samantha Rivera"));
        assert!(detail.contains("Do not open"));
        assert!(!detail.contains("open_whatsapp"));
        assert!(!detail.contains("confirmed"));
    }

    #[test]
    fn save_keeps_reminders_and_round_trips() {
        let path = std::env::temp_dir().join(format!("grok-people-test-{}.json", std::process::id()));
        let _ = fs::remove_file(&path);
        let people = People::load(path.clone());
        let saved = people
            .save(PersonDraft {
                id: String::new(),
                android_contact_id: Some(" 15 ".into()),
                display_name: " Ana ".into(),
                nickname: "Mom".into(),
                aliases: vec!["mummy, ma".into()],
                preferred_channel: "Text".into(),
                handles: Handles { phone: " +1 ".into(), email: String::new(), whatsapp: String::new(), signal: String::new() },
                favorite: true,
                note: "call after 5".into(),
                reach_out_days: 14,
                last_contacted_ms: None,
                reminders: vec![PersonReminder {
                    id: String::new(),
                    label: "Birthday".into(),
                    title: String::new(),
                    date: "3-3".into(),
                    recurrence: "every year".into(),
                    lead_days: 40,
                    time: "9:00".into(),
                    enabled: true,
                }],
            })
            .unwrap();
        assert_eq!(saved.preferred_channel, "sms");
        assert_eq!(saved.aliases, vec!["mummy".to_string(), "ma".to_string()]);
        assert_eq!(saved.reminders.len(), 1);
        assert_eq!(saved.reminders[0].label, "birthday");
        assert_eq!(saved.reminders[0].date, "03-03");
        assert_eq!(saved.reminders[0].time, "09:00");
        assert_eq!(saved.reminders[0].lead_days, 30);
        assert_eq!(saved.android_contact_id.as_deref(), Some("15"));
        let again = People::load(path.clone());
        let loaded = &again.list()[0];
        assert_eq!(loaded.nickname, "Mom");
        assert_eq!(loaded.reminders[0].recurrence, "yearly");
        let raw = fs::read_to_string(&path).unwrap();
        assert!(raw.contains("\"date\": \"03-03\""));
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn custom_dates_save_beside_a_birthday_and_round_trip() {
        let path = std::env::temp_dir().join(format!("grok-people-custom-{}.json", std::process::id()));
        let _ = fs::remove_file(&path);
        let people = People::load(path.clone());
        let saved = people
            .save(PersonDraft {
                id: "ana".into(),
                android_contact_id: None,
                display_name: "Ana".into(),
                nickname: String::new(),
                aliases: vec![],
                preferred_channel: String::new(),
                handles: Handles::default(),
                favorite: false,
                note: String::new(),
                reach_out_days: 0,
                last_contacted_ms: None,
                reminders: vec![
                    PersonReminder {
                        id: "b".into(),
                        label: "birthday".into(),
                        title: String::new(),
                        date: "03-03".into(),
                        recurrence: "yearly".into(),
                        lead_days: 0,
                        time: "09:00".into(),
                        enabled: true,
                    },
                    PersonReminder {
                        id: String::new(),
                        label: "custom".into(),
                        title: "Visa".into(),
                        date: "2026-06-01".into(),
                        recurrence: "once".into(),
                        lead_days: 1,
                        time: "9:30".into(),
                        enabled: true,
                    },
                    PersonReminder {
                        id: "ann".into(),
                        label: "anniversary".into(),
                        title: String::new(),
                        date: "June 12".into(),
                        recurrence: "every year".into(),
                        lead_days: 0,
                        time: "09:00".into(),
                        enabled: true,
                    },
                ],
            })
            .unwrap();
        assert_eq!(saved.reminders.len(), 3);
        let visa = saved.reminders.iter().find(|reminder| reminder.title == "Visa").unwrap();
        assert_eq!(visa.date, "2026-06-01");
        assert_eq!(visa.recurrence, "once");
        assert_eq!(visa.time, "09:30");
        let anniversary = saved.reminders.iter().find(|reminder| reminder.label == "anniversary").unwrap();
        assert_eq!(anniversary.date, "06-12");
        assert!(people.save(PersonDraft {
            id: "ana".into(),
            android_contact_id: None,
            display_name: "Ana".into(),
            nickname: String::new(),
            aliases: vec![],
            preferred_channel: String::new(),
            handles: Handles::default(),
            favorite: false,
            note: String::new(),
            reach_out_days: 0,
            last_contacted_ms: None,
            reminders: vec![PersonReminder {
                id: String::new(),
                label: "custom".into(),
                title: "  ".into(),
                date: "01-02".into(),
                recurrence: "yearly".into(),
                lead_days: 0,
                time: "09:00".into(),
                enabled: true,
            }],
        }).is_err());
        let again = People::load(path.clone());
        assert_eq!(again.list()[0].reminders.len(), 3);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn legacy_birthday_and_unnamed_custom_date_survive_load() {
        let path = std::env::temp_dir().join(format!("grok-people-legacy-{}.json", std::process::id()));
        let raw = r#"{
            "people": [{
                "id": "p1",
                "displayName": "Ana",
                "birthday": "04-04",
                "reminders": [{ "label": "custom", "title": "Visa", "date": "06-01", "recurrence": "yearly" }]
            }]
        }"#;
        fs::write(&path, raw).unwrap();
        let people = People::load(path.clone());
        let person = &people.list()[0];
        assert!(person.reminders.iter().any(|reminder| reminder.label == "birthday" && reminder.date == "04-04"));
        let visa = person.reminders.iter().find(|reminder| reminder.title == "Visa").unwrap();
        assert!(!visa.id.is_empty());
        assert_eq!(visa.label, "custom");
        let stored = fs::read_to_string(&path).unwrap();
        assert!(stored.contains("Visa"));
        assert!(stored.contains("04-04"));
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn custom_title_is_found_without_hiding_the_birthday() {
        let mut sam = person("Sam Rivera", "Mom", &[], "", false);
        birthday(&mut sam, "03-03");
        sam.reminders.push(PersonReminder {
            id: "visa".into(),
            label: "custom".into(),
            title: "Visa".into(),
            date: "06-01".into(),
            recurrence: "yearly".into(),
            lead_days: 0,
            time: "09:00".into(),
            enabled: true,
        });
        let visa = query_reminders(std::slice::from_ref(&sam), "when's Sam's visa");
        assert_eq!(visa.len(), 1);
        assert_eq!(visa[0].reminder.title, "Visa");
        let birthday = query_reminders(std::slice::from_ref(&sam), "when's Sam's birthday");
        assert_eq!(birthday.len(), 1);
        assert_eq!(birthday[0].reminder.label, "birthday");
        let named = query_reminders(std::slice::from_ref(&sam), "visa");
        assert_eq!(named.len(), 1);
        assert_eq!(named[0].reminder.date, "06-01");
    }

    #[test]
    fn saved_people_list_keeps_dates_all_year_and_hides_handles() {
        let mut sam = person("Sam Rivera", "Mom", &["mummy"], "whatsapp", true);
        birthday(&mut sam, "03-03");
        sam.note = "call after 5".into();
        let ana = person("Ana Lopez", "", &[], "", false);
        let listed = saved_people_payload(&[sam, ana], "mom");
        assert_eq!(listed["count"], 1);
        assert_eq!(listed["people"][0]["name"], "Sam Rivera");
        assert_eq!(listed["people"][0]["dates"][0]["date"], "03-03");
        assert_eq!(listed["people"][0]["dates"][0]["label"], "birthday");
        let raw = listed.to_string();
        assert!(!raw.contains("+1999"));
        assert!(!raw.contains("open_call"));
        let all = saved_people_payload(&[person("Ana Lopez", "", &[], "", false)], "");
        assert_eq!(all["count"], 1);
        assert!(all["detail"].as_str().unwrap().contains("all year"));
    }

    fn temp_people(name: &str) -> (People, std::path::PathBuf) {
        let path = std::env::temp_dir().join(format!("grok-people-{name}-{}.json", std::process::id()));
        let _ = fs::remove_file(&path);
        (People::load(path.clone()), path)
    }

    #[test]
    fn voice_create_and_edit_person_dates_notes_and_rename() {
        let (store, path) = temp_people("voice");
        let created = create_from_tool(
            &store,
            &json!({"name": "Ana", "label": "birthday", "date": "June 15", "note": "college friend", "phone": "+15551212", "preferredChannel": "whatsapp", "reachOutDays": 14}),
        )
        .unwrap();
        assert_eq!(created["changed"], true);
        assert!(created["detail"].as_str().unwrap().contains("Added Ana"));
        assert!(created["detail"].as_str().unwrap().contains("June 15"));
        assert!(!created.to_string().contains("+15551212"));
        let ana = &store.list()[0];
        assert_eq!(ana.display_name, "Ana");
        assert_eq!(ana.note, "college friend");
        assert_eq!(ana.preferred_channel, "whatsapp");
        assert_eq!(ana.reach_out_days, 14);
        assert_eq!(ana.handles.phone, "+15551212");
        assert!(ana.android_contact_id.is_none());
        assert_eq!(ana.reminders[0].label, "birthday");
        assert_eq!(ana.reminders[0].date, "06-15");
        assert_eq!(ana.reminders[0].recurrence, "yearly");

        let anniversary = update_from_tool(&store, &json!({"name": "Ana", "label": "anniversary", "date": "March 3"})).unwrap();
        assert!(anniversary["detail"].as_str().unwrap().contains("March 3"));
        let visa = update_from_tool(&store, &json!({"name": "Ana", "label": "visa renewal", "date": "April 10"})).unwrap();
        assert!(visa["detail"].as_str().unwrap().contains("visa renewal"));
        let slash = update_from_tool(&store, &json!({"name": "Ana", "title": "visa renewal", "date": "6/1"})).unwrap();
        assert!(slash["changed"] == true);
        let yearless = update_from_tool(&store, &json!({"name": "Ana", "label": "birthday", "date": "--06-15"})).unwrap();
        assert!(yearless["detail"].as_str().unwrap().contains("June 15"));
        let once = update_from_tool(&store, &json!({"name": "Ana", "title": "visa renewal", "date": "2026-04-10", "recurrence": "once"})).unwrap();
        assert!(once["changed"] == true);

        let renamed = update_from_tool(&store, &json!({"name": "Ana", "rename": "Ana C.", "note": "lives in Zurich"})).unwrap();
        assert!(renamed["detail"].as_str().unwrap().contains("Renamed Ana to Ana C."));
        assert!(renamed["detail"].as_str().unwrap().contains("Note saved"));
        let saved = &store.list()[0];
        assert_eq!(saved.display_name, "Ana C.");
        assert_eq!(saved.note, "lives in Zurich");
        let birthday = saved.reminders.iter().find(|reminder| reminder.label == "birthday").unwrap();
        assert_eq!(birthday.date, "06-15");
        let anniversary = saved.reminders.iter().find(|reminder| reminder.label == "anniversary").unwrap();
        assert_eq!(anniversary.date, "03-03");
        let visa = saved.reminders.iter().find(|reminder| reminder.title == "visa renewal").unwrap();
        assert_eq!(visa.label, "custom");
        assert_eq!(visa.date, "2026-04-10");
        assert_eq!(visa.recurrence, "once");
        assert_eq!(saved.reminders.len(), 3);

        let listed = saved_people_payload(&store.list(), "");
        let raw = listed.to_string();
        assert!(!raw.contains("+15551212"));
        assert!(raw.contains("06-15"));
        assert!(raw.contains("visa renewal"));

        let held = delete_from_tool(&store, &json!({"name": "Ana C."})).unwrap();
        assert_eq!(held["needsConfirm"], true);
        assert_eq!(held["deleted"], false);
        assert_eq!(store.list().len(), 1);
        let removed = delete_from_tool(&store, &json!({"name": "Ana C.", "confirmed": true})).unwrap();
        assert_eq!(removed["deleted"], true);
        assert!(store.list().is_empty());
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn voice_birthday_lead_stays_on_the_person_all_year() {
        let (store, path) = temp_people("lead");
        create_from_tool(&store, &json!({"name": "Ana", "label": "birthday", "date": "June 15"})).unwrap();
        let updated = set_lead_days(&store, "birthday", 7).unwrap();
        assert_eq!(updated["changed"], true);
        assert!(updated["detail"].as_str().unwrap().contains("7 days before"));
        let ana = &store.list()[0];
        assert_eq!(ana.reminders[0].date, "06-15");
        assert_eq!(ana.reminders[0].lead_days, 7);
        assert_eq!(ana.reminders[0].recurrence, "yearly");
        let again = set_lead_days(&store, "birthday", 7).unwrap();
        assert_eq!(again["changed"], false);
        let _ = fs::remove_file(&path);
    }
}
