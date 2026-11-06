//! On-device People overlay. Android ContactsContract stays the source of truth
//! for phone numbers and email. Nicknames, aliases, a preferred channel, handle
//! overrides, favorites, and notes live in `people.json`, same as notes.
//!
//! `reminders` is reserved for a later yearly birthday/anniversary engine. P0
//! always stores an empty array and does not schedule anything.

use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    sync::Mutex,
};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tauri::{AppHandle, Manager};

use crate::native::NativeBridge;

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
    /// Reserved. Always empty until person reminders exist.
    #[serde(default)]
    pub reminders: Vec<Value>,
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
        let mut people = self.lock().people.clone();
        for person in &mut people {
            person.reminders.clear();
        }
        people
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
            reminders: Vec::new(),
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
            reminders: vec![json!({"kind": "birthday"})],
        }
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
    fn save_strips_reminders_and_round_trips() {
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
            })
            .unwrap();
        assert_eq!(saved.preferred_channel, "sms");
        assert_eq!(saved.aliases, vec!["mummy".to_string(), "ma".to_string()]);
        assert!(saved.reminders.is_empty());
        assert_eq!(saved.android_contact_id.as_deref(), Some("15"));
        let again = People::load(path.clone());
        let loaded = &again.list()[0];
        assert_eq!(loaded.nickname, "Mom");
        assert!(loaded.reminders.is_empty());
        let raw = fs::read_to_string(&path).unwrap();
        assert!(raw.contains("\"reminders\": []"));
        let _ = fs::remove_file(&path);
    }
}
