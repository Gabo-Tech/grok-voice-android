//! On-device People overlay. Android ContactsContract stays the source of truth
//! for phone numbers and email. Nicknames, aliases, a preferred channel, handle
//! overrides, favorites, notes, and dated reminders live in `people.json`.
//!
//! Person reminders are birthdays, anniversaries, and custom dates. Scheduling
//! them onto the Time list happens in `actions`, which reads this store.

use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    sync::Mutex,
};

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
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PersonReminder {
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
        let inner = fs::read(&path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        Self { path, inner: Mutex::new(inner) }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Store> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn persist(path: &PathBuf, store: &Store) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let bytes = serde_json::to_vec_pretty(store).map_err(|e| e.to_string())?;
        fs::write(path, bytes).map_err(|e| e.to_string())
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
/// words such as "birthday" or "anniversary".
pub fn query_reminders(people: &[Person], query: &str) -> Vec<ReminderHit> {
    let q = normalize(query);
    let want_birthday = q.contains("birthday") || q.contains("born");
    let want_anniversary = q.contains("anniversary");
    let name_query = name_part(&q);
    let ranked = if name_query.chars().count() >= 2 {
        let hits = rank_people(people, &name_query);
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
    let mut hits = Vec::new();
    for person in ranked {
        for reminder in &person.reminders {
            if !reminder.enabled {
                continue;
            }
            if want_birthday && !want_anniversary && reminder.label != "birthday" {
                continue;
            }
            if want_anniversary && !want_birthday && reminder.label != "anniversary" {
                continue;
            }
            hits.push(ReminderHit {
                person_id: person.id.clone(),
                person_name: person.display_name.clone(),
                nickname: person.nickname.clone(),
                reminder: reminder.clone(),
            });
        }
    }
    hits
}

pub fn add_person_reminder(app: &AppHandle, name: &str, reminder: PersonReminder) -> Result<Value, String> {
    let found = find_contact(app, name)?;
    let matches = found["matches"].as_array().cloned().unwrap_or_default();
    let people_hits: Vec<Value> = matches.iter().filter(|m| m["source"] == "people" && m["personId"].is_string()).cloned().collect();
    let pool = if people_hits.is_empty() { matches } else { people_hits };
    if pool.len() != 1 {
        let detail = if pool.is_empty() {
            format!("No person matched {}. Add them in People first. Nothing was opened.", name.trim())
        } else {
            format!("{} Nothing was saved or opened.", facts_detail(&pool))
        };
        return Ok(json!({ "ok": true, "saved": false, "detail": detail }));
    }
    let Some(id) = pool[0]["personId"].as_str() else {
        return Ok(json!({
            "ok": true,
            "saved": false,
            "detail": "That match is only in phone contacts. Add them in People, then the date can be saved. Nothing was opened."
        }));
    };
    let people = app.state::<People>();
    let Some(mut person) = people.list().into_iter().find(|p| p.id == id) else {
        return Err("No person with that id.".into());
    };
    person.reminders.push(reminder);
    let saved = people.set_reminders(&person.id, person.reminders)?;
    let last = saved.reminders.last();
    let spoken = last.map(|r| speak_reminder(&saved.display_name, r)).unwrap_or_else(|| "Saved.".into());
    Ok(json!({
        "ok": true,
        "saved": true,
        "personId": saved.id,
        "reminderId": last.map(|r| r.id.clone()).unwrap_or_default(),
        "detail": format!("{spoken} Nothing was opened.")
    }))
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
    let recurrence = if recurrence::normalize_kind(&reminder.recurrence) == "once" { "once" } else { "yearly" };
    let clock = if reminder.time.trim().is_empty() { "09:00" } else { reminder.time.trim() };
    let (hour, minute) = recurrence::parse_clock(clock).ok_or_else(|| "Time should look like 09:00.".to_string())?;
    let date = if recurrence == "once" {
        recurrence::format_month_day(year, month, day)
    } else {
        recurrence::format_month_day(None, month, day)
    };
    let id = reminder.id.trim();
    Ok(PersonReminder {
        id: if id.is_empty() { format!("r{}{index}", chrono::Local::now().timestamp_millis()) } else { id.to_string() },
        label: label.into(),
        title: clamp(&reminder.title, 80),
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
}
