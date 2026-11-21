//! Message triage and explicit phone calls.
//!
//! Notification text lives in a short Android memory cache. This module ranks
//! it, speaks a summary, and refuses to send or dial until the user confirms.
//! Nothing here is written to disk.

use serde::Deserialize;
use serde_json::{Value, json};
use tauri::{AppHandle, Manager};

use crate::i18n::{self, fill};

/// Known messaging packages. Empty settings use this list and ignore the rest.
pub const MESSAGING_APPS: &[(&str, &str)] = &[
    ("com.whatsapp", "WhatsApp"),
    ("com.whatsapp.w4b", "WhatsApp Business"),
    ("org.thoughtcrime.securesms", "Signal"),
    ("org.telegram.messenger", "Telegram"),
    ("org.telegram.messenger.web", "Telegram"),
    ("org.thunderdog.challegram", "Telegram"),
    ("org.telegram.plus", "Telegram"),
    ("com.google.android.apps.messaging", "Messages"),
    ("com.android.mms", "Messages"),
    ("com.samsung.android.messaging", "Messages"),
    ("com.google.android.gm", "Gmail"),
    ("com.microsoft.office.outlook", "Outlook"),
    ("com.yahoo.mobile.client.android.mail", "Yahoo Mail"),
    ("ch.threema.app", "Threema"),
    ("com.facebook.orca", "Messenger"),
    ("com.discord", "Discord"),
    ("com.Slack", "Slack"),
    ("im.vector.app", "Element"),
    ("com.viber.voip", "Viber"),
];

const SPOKEN_LIMIT: usize = 3;
const TEXT_CAP: usize = 400;

#[derive(Clone, Debug)]
pub struct PersonHint {
    pub names: Vec<String>,
    pub phone: String,
    pub email: String,
    pub whatsapp: String,
    pub signal: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawMessage {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub package: String,
    #[serde(default)]
    pub app: String,
    #[serde(default)]
    pub sender: String,
    #[serde(default)]
    pub conversation: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub group: bool,
    #[serde(default)]
    pub message: bool,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub ongoing: bool,
    #[serde(default)]
    pub when_ms: i64,
    #[serde(default)]
    pub reply: bool,
    #[serde(default)]
    pub system: bool,
    #[serde(default)]
    pub promo: bool,
}

#[derive(Clone, Debug)]
pub struct Ranked {
    pub raw: RawMessage,
    pub score: i32,
    pub person: bool,
    pub who: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplyDraft {
    pub id: String,
    pub text: String,
    pub who: String,
    pub app: String,
    pub channel: String,
    pub package: String,
    pub phone: String,
    pub email: String,
    pub remote: bool,
}

#[derive(Clone, Debug)]
pub enum ReplyStep {
    Ask { detail: String },
    Send { draft: ReplyDraft },
    OpenDraft { draft: ReplyDraft },
    Stop { detail: String },
}

#[derive(Clone, Debug)]
pub enum CallStep {
    Ask { detail: String },
    Place { who: String, phone: String },
    Answer { who: String },
    Decline { who: String },
    End,
    Stop { detail: String },
}

#[derive(Default)]
pub struct ConfirmGate {
    reply: Option<ReplyDraft>,
    call_digits: Option<String>,
}

/// `None` means reading is allowed. A key means stop and speak that line.
pub fn read_block(listener: bool, triage: bool) -> Option<&'static str> {
    if !listener {
        return Some("inbox-listener");
    }
    if !triage {
        return Some("inbox-off");
    }
    None
}

pub fn package_allowed(custom: bool, apps: &str, package: &str) -> bool {
    let package = package.trim();
    if package.is_empty() {
        return false;
    }
    if !custom {
        return MESSAGING_APPS.iter().any(|(pkg, _)| *pkg == package);
    }
    apps.split(',').map(str::trim).filter(|item| !item.is_empty()).any(|item| item == package)
}

pub fn channel_for(package: &str) -> &'static str {
    match package {
        "com.whatsapp" | "com.whatsapp.w4b" => "whatsapp",
        "org.thoughtcrime.securesms" => "signal",
        "org.telegram.messenger" | "org.telegram.messenger.web" | "org.thunderdog.challegram" | "org.telegram.plus" => "telegram",
        "com.google.android.apps.messaging" | "com.android.mms" | "com.samsung.android.messaging" => "sms",
        "com.google.android.gm" | "com.microsoft.office.outlook" | "com.yahoo.mobile.client.android.mail" => "email",
        _ => "app",
    }
}

pub fn app_label(package: &str, given: &str) -> String {
    let given = given.trim();
    if !given.is_empty() {
        return given.to_string();
    }
    MESSAGING_APPS
        .iter()
        .find(|(pkg, _)| *pkg == package)
        .map(|(_, label)| (*label).to_string())
        .unwrap_or_else(|| package.to_string())
}

pub fn catalog_value(installed: &[String]) -> Value {
    let apps: Vec<Value> = MESSAGING_APPS
        .iter()
        .map(|(package, label)| {
            json!({
                "packageName": package,
                "label": label,
                "installed": installed.iter().any(|item| item == package),
                "messaging": true,
            })
        })
        .collect();
    json!({ "ok": true, "apps": apps })
}

pub fn rank(messages: &[RawMessage], people: &[PersonHint], custom: bool, apps: &str) -> Vec<Ranked> {
    let mut ranked = Vec::new();
    for raw in messages {
        if !relevant(raw, custom, apps) {
            continue;
        }
        let person = person_hit(&raw.sender, people).or_else(|| person_hit(&raw.conversation, people));
        let mut score = 1;
        if person.is_some() {
            score += 100;
        }
        if !raw.group {
            score += 40;
        }
        if MESSAGING_APPS.iter().any(|(pkg, _)| *pkg == raw.package) {
            score += 10;
        }
        if raw.reply {
            score += 5;
        }
        let who = if raw.group && !raw.conversation.trim().is_empty() {
            raw.conversation.trim().to_string()
        } else if !raw.sender.trim().is_empty() {
            raw.sender.trim().to_string()
        } else {
            raw.app.clone()
        };
        ranked.push(Ranked { raw: raw.clone(), score, person: person.is_some(), who });
    }
    ranked.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| b.raw.when_ms.cmp(&a.raw.when_ms)));
    ranked
}

fn relevant(raw: &RawMessage, custom: bool, apps: &str) -> bool {
    if raw.system || raw.promo {
        return false;
    }
    if raw.category == "call" || raw.category == "alarm" || raw.category == "reminder" {
        return false;
    }
    if !package_allowed(custom, apps, &raw.package) {
        return false;
    }
    let known = MESSAGING_APPS.iter().any(|(pkg, _)| *pkg == raw.package);
    let is_message = raw.message || known;
    if !is_message {
        return false;
    }
    if raw.ongoing && !raw.message && raw.category != "msg" && raw.category != "email" {
        return false;
    }
    !raw.text.trim().is_empty() || !raw.sender.trim().is_empty()
}

pub fn summary(locale: &str, ranked: &[Ranked], hidden: bool) -> String {
    if ranked.is_empty() {
        return i18n::line(locale, "inbox-none");
    }
    let shown = ranked.iter().take(SPOKEN_LIMIT).collect::<Vec<_>>();
    let extra = ranked.len().saturating_sub(shown.len());
    let body = if shown.len() == 1 {
        one_line(locale, shown[0], hidden)
    } else {
        let joined = shown.iter().map(|item| part_line(locale, item, hidden)).collect::<Vec<_>>().join(". ");
        let mut line = fill(locale, "inbox-count", &[("count", &ranked.len().to_string()), ("parts", &joined)]);
        if extra > 0 {
            line = format!("{line} {}", fill(locale, "inbox-more", &[("count", &extra.to_string())]));
        }
        line
    };
    if shown.len() == 1 && shown[0].person && !shown[0].raw.group && !hidden {
        format!("{body} {}", i18n::line(locale, "inbox-offer"))
    } else {
        body
    }
}

fn one_line(locale: &str, item: &Ranked, hidden: bool) -> String {
    let group = item.raw.group && !item.raw.conversation.trim().is_empty();
    let key = match (hidden, group) {
        (true, true) => "inbox-hidden-group",
        (true, false) => "inbox-hidden",
        (false, true) => "inbox-one-group",
        (false, false) => "inbox-one",
    };
    fill(
        locale,
        key,
        &[
            ("sender", speaker(item)),
            ("app", &label_of(item)),
            ("conversation", item.raw.conversation.trim()),
            ("text", &clip(&item.raw.text, 180)),
        ],
    )
}

fn part_line(locale: &str, item: &Ranked, hidden: bool) -> String {
    if hidden {
        let key = if item.raw.group && !item.raw.conversation.trim().is_empty() {
            "inbox-hidden-group"
        } else {
            "inbox-hidden"
        };
        return fill(locale, key, &[("sender", speaker(item)), ("app", &label_of(item)), ("conversation", item.raw.conversation.trim())]);
    }
    let key = if item.raw.group && !item.raw.conversation.trim().is_empty() {
        "inbox-part-group"
    } else {
        "inbox-part"
    };
    fill(
        locale,
        key,
        &[
            ("sender", speaker(item)),
            ("app", &label_of(item)),
            ("conversation", item.raw.conversation.trim()),
            ("text", &clip(&item.raw.text, 180)),
        ],
    )
}

fn speaker(item: &Ranked) -> &str {
    if item.raw.sender.trim().is_empty() { item.who.as_str() } else { item.raw.sender.trim() }
}

fn label_of(item: &Ranked) -> String {
    app_label(&item.raw.package, &item.raw.app)
}

pub fn pick<'a>(ranked: &'a [Ranked], id: &str, sender: &str) -> Result<&'a Ranked, &'static str> {
    let id = id.trim();
    if !id.is_empty() {
        return ranked.iter().find(|item| item.raw.id == id).ok_or("inbox-which");
    }
    let query = normalize(sender);
    if query.is_empty() {
        return if ranked.len() == 1 { Ok(&ranked[0]) } else { Err("inbox-which") };
    }
    let hits: Vec<_> = ranked
        .iter()
        .filter(|item| {
            normalize(&item.raw.sender) == query || normalize(&item.raw.conversation) == query || normalize(&item.who) == query
        })
        .collect();
    if hits.len() == 1 {
        return Ok(hits[0]);
    }
    if hits.len() > 1 && hits.iter().all(|item| normalize(&item.raw.sender) == query) {
        return Ok(hits[0]);
    }
    Err("inbox-which")
}

impl ConfirmGate {
    /// A first call never sends, even with `confirmed`. The second call sends
    /// only when the text, recipient, and id match what was just read back.
    pub fn reply(&mut self, locale: &str, mut draft: ReplyDraft, confirmed: bool) -> ReplyStep {
        let text = draft.text.trim().to_string();
        if text.is_empty() {
            return ReplyStep::Stop { detail: i18n::line(locale, "inbox-text") };
        }
        draft.text = text.clone();
        let matches = self.reply.as_ref().is_some_and(|pending| {
            pending.id == draft.id && pending.text == draft.text && same_who(&pending.who, &draft.who)
        });
        if !confirmed || !matches {
            let detail = fill(locale, "inbox-confirm", &[("text", &draft.text), ("who", &draft.who), ("app", &draft.app)]);
            self.reply = Some(draft);
            return ReplyStep::Ask { detail };
        }
        let ready = self.reply.take().unwrap_or(draft);
        if ready.remote {
            ReplyStep::Send { draft: ready }
        } else {
            ReplyStep::OpenDraft { draft: ready }
        }
    }

    /// The first request only reads the number back. Dialing needs the same
    /// number again after a yes, and the call permission.
    pub fn place(&mut self, locale: &str, who: &str, phone: &str, confirmed: bool, permitted: bool) -> CallStep {
        if !permitted {
            return CallStep::Stop { detail: i18n::line(locale, "inbox-call-denied") };
        }
        let digits = digits_of(phone);
        if digits.len() < 3 {
            return CallStep::Stop { detail: fill(locale, "inbox-call-number", &[("who", who)]) };
        }
        let same = self.call_digits.as_deref() == Some(digits.as_str());
        if !confirmed || !same {
            self.call_digits = Some(digits);
            return CallStep::Ask { detail: fill(locale, "inbox-call-ask", &[("who", who), ("phone", phone.trim())]) };
        }
        self.call_digits = None;
        CallStep::Place { who: who.to_string(), phone: phone.trim().to_string() }
    }

    pub fn answer(&self, locale: &str, who: &str, confirmed: bool, permitted: bool, ringing: bool) -> CallStep {
        if !ringing {
            return CallStep::Stop { detail: i18n::line(locale, "inbox-answer-none") };
        }
        if !permitted {
            return CallStep::Stop { detail: i18n::line(locale, "inbox-answer-denied") };
        }
        if !confirmed {
            return CallStep::Ask { detail: fill(locale, "inbox-answer-ask", &[("who", who)]) };
        }
        CallStep::Answer { who: who.to_string() }
    }

    pub fn decline(&self, locale: &str, who: &str, confirmed: bool, permitted: bool, ringing: bool) -> CallStep {
        if !ringing {
            return CallStep::Stop { detail: i18n::line(locale, "inbox-answer-none") };
        }
        if !permitted {
            return CallStep::Stop { detail: i18n::line(locale, "inbox-answer-denied") };
        }
        if !confirmed {
            return CallStep::Ask { detail: fill(locale, "inbox-decline-ask", &[("who", who)]) };
        }
        CallStep::Decline { who: who.to_string() }
    }

    pub fn end(&self, locale: &str, confirmed: bool, permitted: bool, active: bool) -> CallStep {
        if !active {
            return CallStep::Stop { detail: i18n::line(locale, "inbox-end-none") };
        }
        if !permitted {
            return CallStep::Stop { detail: i18n::line(locale, "inbox-answer-denied") };
        }
        if !confirmed {
            return CallStep::Ask { detail: i18n::line(locale, "inbox-end-ask") };
        }
        CallStep::End
    }
}

fn person_hit<'a>(label: &str, people: &'a [PersonHint]) -> Option<&'a PersonHint> {
    let label = normalize(label);
    if label.is_empty() {
        return None;
    }
    people.iter().find(|person| person.names.iter().any(|name| normalize(name) == label))
}

fn same_who(left: &str, right: &str) -> bool {
    normalize(left) == normalize(right)
}

fn normalize(value: &str) -> String {
    let mut out = String::new();
    let mut space = false;
    for ch in value.chars() {
        if ch.is_alphanumeric() {
            for lower in ch.to_lowercase() {
                out.push(lower);
            }
            space = false;
        } else if !out.is_empty() && !space {
            out.push(' ');
            space = true;
        }
    }
    out.trim().to_string()
}

fn digits_of(phone: &str) -> String {
    phone.chars().filter(|ch| ch.is_ascii_digit()).collect()
}

fn clip(text: &str, max: usize) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= max {
        return trimmed.to_string();
    }
    let mut out: String = trimmed.chars().take(max).collect();
    out.push('…');
    out
}

pub fn tool_read(app: &AppHandle) -> Result<Value, String> {
    let settings = settings_of(app);
    let locale = settings.locale.clone();
    let snapshot = read_snapshot(app)?;
    let listener = snapshot["listener"] != false && snapshot.get("ok").and_then(|flag| flag.as_bool()).unwrap_or(true);
    if let Some(key) = read_block(listener, settings.message_triage) {
        return Ok(json!({ "ok": false, "error": i18n::line(&locale, key), "detail": i18n::line(&locale, key), "read": false }));
    }
    let hidden = snapshot["locked"] == true && settings.redact_locked;
    let messages = parse_messages(&snapshot);
    let ranked = rank(&messages, &people_hints(app), settings.message_apps_custom, &settings.message_apps);
    let detail = summary(&locale, &ranked, hidden);
    let public_rows: Vec<Value> = ranked
        .iter()
        .take(8)
        .map(|item| {
            json!({
                "id": item.raw.id,
                "sender": item.raw.sender,
                "who": item.who,
                "app": label_of(item),
                "package": item.raw.package,
                "text": if hidden { Value::String(String::new()) } else { json!(clip(&item.raw.text, TEXT_CAP)) },
                "group": item.raw.group,
                "reply": item.raw.reply,
                "person": item.person,
            })
        })
        .collect();
    Ok(json!({
        "ok": true,
        "read": true,
        "hidden": hidden,
        "count": ranked.len(),
        "messages": public_rows,
        "detail": detail,
    }))
}

pub fn tool_reply(app: &AppHandle, args: &Value) -> Result<Value, String> {
    let settings = settings_of(app);
    let locale = settings.locale.clone();
    let snapshot = read_snapshot(app)?;
    let listener = snapshot["listener"] != false && snapshot.get("ok").and_then(|flag| flag.as_bool()).unwrap_or(false);
    if let Some(key) = read_block(listener, settings.message_triage) {
        return Ok(stop(&locale, key));
    }
    let messages = parse_messages(&snapshot);
    let ranked = rank(&messages, &people_hints(app), settings.message_apps_custom, &settings.message_apps);
    let id = args["id"].as_str().unwrap_or("");
    let sender = args["sender"].as_str().or_else(|| args["name"].as_str()).or_else(|| args["who"].as_str()).unwrap_or("");
    let item = match pick(&ranked, id, sender) {
        Ok(item) => item.clone(),
        Err(key) => return Ok(stop(&locale, key)),
    };
    let text = args["text"].as_str().or_else(|| args["message"].as_str()).unwrap_or("").trim().to_string();
    let hints = people_hints(app);
    let person = person_hit(&item.raw.sender, &hints).or_else(|| person_hit(&item.who, &hints));
    let channel = channel_for(&item.raw.package);
    let phone = person.map(|hint| handle_phone(hint, channel)).unwrap_or_default();
    let email = person.map(|hint| hint.email.clone()).unwrap_or_default();
    let draft = ReplyDraft {
        id: item.raw.id.clone(),
        text,
        who: item.who.clone(),
        app: label_of(&item),
        channel: channel.to_string(),
        package: item.raw.package.clone(),
        phone,
        email,
        remote: item.raw.reply,
    };
    let confirmed = args["confirmed"] == true;
    let step = gate().reply(&locale, draft, confirmed);
    match step {
        ReplyStep::Ask { detail } => Ok(json!({ "ok": true, "needsConfirm": true, "sent": false, "detail": detail })),
        ReplyStep::Stop { detail } => Ok(json!({ "ok": false, "sent": false, "error": detail, "detail": detail })),
        ReplyStep::Send { draft } => send_remote(app, &locale, &draft),
        ReplyStep::OpenDraft { draft } => open_draft(app, &locale, &draft),
    }
}

pub fn tool_place(app: &AppHandle, args: &Value) -> Result<Value, String> {
    let locale = settings_of(app).locale.clone();
    let permitted = call_permission(app, "placeCall");
    let explicit = args["phone"].as_str().unwrap_or("").trim().to_string();
    let name = args["name"].as_str().or_else(|| args["who"].as_str()).unwrap_or("").trim().to_string();
    let (who, phone) = match resolve_phone(app, &locale, &name, &explicit) {
        Ok(pair) => pair,
        Err(detail) => return Ok(json!({ "ok": false, "placed": false, "error": detail, "detail": detail })),
    };
    let confirmed = args["confirmed"] == true;
    match gate().place(&locale, &who, &phone, confirmed, permitted) {
        CallStep::Ask { detail } => Ok(json!({ "ok": true, "needsConfirm": true, "placed": false, "detail": detail })),
        CallStep::Stop { detail } => Ok(json!({ "ok": false, "placed": false, "error": detail, "detail": detail })),
        CallStep::Place { who, phone } => {
            let raw = app.state::<crate::native::NativeBridge>().phone_call("place", &phone).map_err(|err| err.to_string())?;
            let value: Value = serde_json::from_str(&raw).unwrap_or_else(|_| json!({ "ok": false }));
            if value["ok"] == true {
                let detail = fill(&locale, "inbox-call-placed", &[("who", &who), ("phone", &phone)]);
                Ok(json!({ "ok": true, "placed": true, "detail": detail }))
            } else {
                let detail = value["error"].as_str().unwrap_or("inbox-call-denied");
                let detail = if detail.starts_with("inbox-") { i18n::line(&locale, detail) } else { detail.to_string() };
                Ok(json!({ "ok": false, "placed": false, "error": detail, "detail": detail }))
            }
        }
        _ => Ok(stop(&locale, "inbox-call-denied")),
    }
}

pub fn tool_answer(app: &AppHandle, args: &Value) -> Result<Value, String> {
    call_action(app, args, "answer")
}

pub fn tool_decline(app: &AppHandle, args: &Value) -> Result<Value, String> {
    call_action(app, args, "decline")
}

pub fn tool_end(app: &AppHandle, args: &Value) -> Result<Value, String> {
    let locale = settings_of(app).locale.clone();
    let state = call_state(app)?;
    let permitted = state["answerPermission"] == true || state["hangupAction"] == true || state["declineAction"] == true;
    let active = state["offhook"] == true || state["ringing"] == true;
    let confirmed = args["confirmed"] == true;
    match gate().end(&locale, confirmed, permitted, active) {
        CallStep::Ask { detail } => Ok(json!({ "ok": true, "needsConfirm": true, "ended": false, "detail": detail })),
        CallStep::Stop { detail } => Ok(json!({ "ok": false, "ended": false, "error": detail, "detail": detail })),
        CallStep::End => finish_call(app, &locale, "end", "inbox-ended", &[]),
        _ => Ok(stop(&locale, "inbox-end-none")),
    }
}

fn call_action(app: &AppHandle, args: &Value, action: &str) -> Result<Value, String> {
    let locale = settings_of(app).locale.clone();
    let state = call_state(app)?;
    let who = state["name"].as_str().filter(|name| !name.is_empty()).unwrap_or("the caller");
    let ringing = state["ringing"] == true;
    let permitted = if action == "answer" {
        state["answerPermission"] == true || state["answerAction"] == true
    } else {
        state["answerPermission"] == true || state["declineAction"] == true
    };
    let confirmed = args["confirmed"] == true;
    let step = if action == "answer" {
        gate().answer(&locale, who, confirmed, permitted, ringing)
    } else {
        gate().decline(&locale, who, confirmed, permitted, ringing)
    };
    match step {
        CallStep::Ask { detail } => Ok(json!({ "ok": true, "needsConfirm": true, "done": false, "detail": detail })),
        CallStep::Stop { detail } => Ok(json!({ "ok": false, "done": false, "error": detail, "detail": detail })),
        CallStep::Answer { who } => finish_call(app, &locale, "answer", "inbox-answered", &[("who", &who)]),
        CallStep::Decline { who } => finish_call(app, &locale, "decline", "inbox-declined", &[("who", &who)]),
        _ => Ok(stop(&locale, "inbox-answer-none")),
    }
}

fn finish_call(app: &AppHandle, locale: &str, action: &str, key: &str, pairs: &[(&str, &str)]) -> Result<Value, String> {
    let raw = app.state::<crate::native::NativeBridge>().phone_call(action, "").map_err(|err| err.to_string())?;
    let value: Value = serde_json::from_str(&raw).unwrap_or_else(|_| json!({ "ok": false }));
    if value["ok"] == true {
        let detail = if pairs.is_empty() { i18n::line(locale, key) } else { fill(locale, key, pairs) };
        Ok(json!({ "ok": true, "done": true, "detail": detail }))
    } else {
        let detail = value["error"].as_str().map(|item| if item.starts_with("inbox-") { i18n::line(locale, item) } else { item.to_string() }).unwrap_or_else(|| i18n::line(locale, "inbox-answer-denied"));
        Ok(json!({ "ok": false, "done": false, "error": detail, "detail": detail }))
    }
}

fn send_remote(app: &AppHandle, locale: &str, draft: &ReplyDraft) -> Result<Value, String> {
    let raw = app
        .state::<crate::native::NativeBridge>()
        .reply_inbox(&draft.id, &draft.text)
        .map_err(|err| err.to_string())?;
    let value: Value = serde_json::from_str(&raw).unwrap_or_else(|_| json!({ "ok": false, "code": "no-reply" }));
    if value["ok"] == true {
        let detail = fill(locale, "inbox-sent", &[("text", &draft.text), ("who", &draft.who), ("app", &draft.app)]);
        return Ok(json!({ "ok": true, "sent": true, "via": "remote", "detail": detail }));
    }
    if value["code"].as_str() == Some("no-reply") {
        return open_draft(app, locale, draft);
    }
    let detail = value["error"].as_str().unwrap_or("inbox-no-target");
    let detail = if detail.starts_with("inbox-") { i18n::line(locale, detail) } else { detail.to_string() };
    Ok(json!({ "ok": false, "sent": false, "error": detail, "detail": detail }))
}

fn open_draft(app: &AppHandle, locale: &str, draft: &ReplyDraft) -> Result<Value, String> {
    let bridge = app.state::<crate::native::NativeBridge>();
    let kind = match draft.channel.as_str() {
        "whatsapp" | "signal" | "sms" | "email" => draft.channel.as_str(),
        _ => "",
    };
    let has_target = match kind {
        "email" => !draft.email.trim().is_empty(),
        "whatsapp" | "signal" | "sms" => !draft.phone.trim().is_empty(),
        _ => false,
    };
    if has_target {
        let raw = bridge
            .open_hand_off(kind, "", "", &draft.phone, &draft.text, &draft.email, "", &draft.text)
            .map_err(|err| err.to_string())?;
        let value: Value = serde_json::from_str(&raw).unwrap_or_else(|_| json!({ "ok": false }));
        if value["ok"] == true {
            let detail = fill(locale, "inbox-fallback", &[("app", &draft.app), ("who", &draft.who)]);
            return Ok(json!({ "ok": true, "sent": false, "opened": true, "via": "draft", "detail": detail }));
        }
    }
    let raw = bridge.share_message(&draft.package, &draft.text).map_err(|err| err.to_string())?;
    let value: Value = serde_json::from_str(&raw).unwrap_or_else(|_| json!({ "ok": false }));
    if value["ok"] == true {
        let detail = fill(locale, "inbox-fallback", &[("app", &draft.app), ("who", &draft.who)]);
        return Ok(json!({ "ok": true, "sent": false, "opened": true, "via": "draft", "detail": detail }));
    }
    let detail = fill(locale, "inbox-no-target", &[("who", &draft.who)]);
    Ok(json!({ "ok": false, "sent": false, "error": detail, "detail": detail }))
}

fn resolve_phone(app: &AppHandle, locale: &str, name: &str, explicit: &str) -> Result<(String, String), String> {
    if digits_of(explicit).len() >= 3 {
        let who = if name.is_empty() { explicit.to_string() } else { name.to_string() };
        return Ok((who, explicit.to_string()));
    }
    if name.chars().count() < 2 {
        return Err(i18n::line(locale, "inbox-call-which"));
    }
    let found = crate::people::find_contact(app, name)?;
    let matches = found["matches"].as_array().cloned().unwrap_or_default();
    if matches.len() != 1 {
        return Err(if matches.is_empty() {
            fill(locale, "inbox-call-number", &[("who", name)])
        } else {
            i18n::line(locale, "inbox-call-which")
        });
    }
    let phone = matches[0]["phone"].as_str().unwrap_or("").trim().to_string();
    if digits_of(&phone).len() < 3 {
        return Err(fill(locale, "inbox-call-number", &[("who", name)]));
    }
    let who = matches[0]["nickname"].as_str().filter(|item| !item.is_empty()).unwrap_or(name).to_string();
    Ok((who, phone))
}

fn handle_phone(hint: &PersonHint, channel: &str) -> String {
    let preferred = match channel {
        "whatsapp" => hint.whatsapp.as_str(),
        "signal" => hint.signal.as_str(),
        "email" => "",
        _ => hint.phone.as_str(),
    };
    if preferred.trim().is_empty() { hint.phone.clone() } else { preferred.to_string() }
}

fn people_hints(app: &AppHandle) -> Vec<PersonHint> {
    app.try_state::<crate::people::People>()
        .map(|people| {
            people
                .list()
                .into_iter()
                .map(|person| {
                    let mut names = vec![person.display_name, person.nickname];
                    names.extend(person.aliases);
                    PersonHint {
                        names: names.into_iter().filter(|name| !name.trim().is_empty()).collect(),
                        phone: person.handles.phone,
                        email: person.handles.email,
                        whatsapp: person.handles.whatsapp,
                        signal: person.handles.signal,
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

fn parse_messages(snapshot: &Value) -> Vec<RawMessage> {
    snapshot["messages"]
        .as_array()
        .map(|rows| rows.iter().filter_map(|row| serde_json::from_value(row.clone()).ok()).collect())
        .unwrap_or_default()
}

fn read_snapshot(app: &AppHandle) -> Result<Value, String> {
    let raw = app.state::<crate::native::NativeBridge>().read_notifications().map_err(|err| err.to_string())?;
    serde_json::from_str(&raw).map_err(|err| err.to_string())
}

fn call_state(app: &AppHandle) -> Result<Value, String> {
    let raw = app.state::<crate::native::NativeBridge>().phone_call("state", "").map_err(|err| err.to_string())?;
    serde_json::from_str(&raw).map_err(|err| err.to_string())
}

fn call_permission(app: &AppHandle, field: &str) -> bool {
    app.state::<crate::native::NativeBridge>()
        .access_status()
        .ok()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        .and_then(|value| value.get(field).and_then(|item| item.as_str()).map(|item| item == "granted"))
        .unwrap_or(false)
}

fn settings_of(app: &AppHandle) -> crate::settings::Settings {
    app.try_state::<crate::settings::SettingsStore>().map(|store| store.get()).unwrap_or_default()
}

fn stop(locale: &str, key: &str) -> Value {
    let detail = i18n::line(locale, key);
    json!({ "ok": false, "error": detail, "detail": detail, "sent": false, "placed": false })
}

fn gate() -> std::sync::MutexGuard<'static, ConfirmGate> {
    GATE.lock().unwrap_or_else(|err| err.into_inner())
}

static GATE: std::sync::Mutex<ConfirmGate> = std::sync::Mutex::new(ConfirmGate { reply: None, call_digits: None });

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(id: &str, package: &str, sender: &str, text: &str, group: bool, when_ms: i64) -> RawMessage {
        RawMessage {
            id: id.into(),
            package: package.into(),
            app: String::new(),
            sender: sender.into(),
            conversation: if group { "Family".into() } else { String::new() },
            text: text.into(),
            group,
            message: true,
            category: "msg".into(),
            ongoing: false,
            when_ms,
            reply: true,
            system: false,
            promo: false,
        }
    }

    fn mom() -> PersonHint {
        PersonHint {
            names: vec!["Sam Rivera".into(), "Mom".into()],
            phone: "+15551212".into(),
            email: "mom@example.com".into(),
            whatsapp: "+15551212".into(),
            signal: String::new(),
        }
    }

    #[test]
    fn relevance_ranks_people_then_direct_messages_and_drops_noise() {
        let mut promo = msg("p", "com.whatsapp", "Shop", "sale today", false, 50);
        promo.promo = true;
        let mut system = msg("s", "com.whatsapp", "Android", "usb", false, 40);
        system.system = true;
        system.package = "android".into();
        let mut ongoing = msg("o", "com.spotify.music", "Song", "playing", false, 30);
        ongoing.ongoing = true;
        ongoing.message = false;
        ongoing.category = "transport".into();
        let stranger = msg("a", "com.whatsapp", "Alex", "hello", false, 20);
        let group = msg("g", "com.whatsapp", "Mom", "bring dessert", true, 80);
        let direct = msg("d", "com.whatsapp", "Mom", "when are you visiting", false, 10);
        let other_app = msg("x", "com.spotify.music", "Mom", "playlist", false, 90);
        let people = [mom()];
        let ranked = rank(&[promo, system, ongoing, stranger, group, direct, other_app], &people, false, "");
        let ids: Vec<_> = ranked.iter().map(|item| item.raw.id.as_str()).collect();
        assert_eq!(ids, vec!["d", "g", "a"]);
        assert!(ranked[0].score > ranked[1].score);
        assert!(ranked[1].person);
        assert!(!ranked[2].person);
        assert!(ranked.iter().all(|item| item.raw.package == "com.whatsapp"));
    }

    #[test]
    fn summary_names_sender_app_and_offers_a_reply() {
        let people = [mom()];
        let ranked = rank(&[msg("d", "com.whatsapp", "Mom", "when are you visiting", false, 1)], &people, false, "");
        let line = summary("en", &ranked, false);
        assert!(line.contains("Mom"));
        assert!(line.contains("WhatsApp"));
        assert!(line.contains("when are you visiting"));
        assert!(line.contains("Should I answer?"));
        let spanish = summary("es", &ranked, false);
        assert!(spanish.contains("WhatsApp"));
        assert!(spanish.contains("¿Le contesto?"));
        let hidden = summary("en", &ranked, true);
        assert!(!hidden.contains("when are you visiting"));
        assert!(hidden.contains("Mom"));
    }

    #[test]
    fn allow_list_defaults_to_messaging_apps_until_customized() {
        assert!(package_allowed(false, "", "com.whatsapp"));
        assert!(package_allowed(false, "", "org.thoughtcrime.securesms"));
        assert!(!package_allowed(false, "", "com.spotify.music"));
        assert!(package_allowed(true, "com.spotify.music", "com.spotify.music"));
        assert!(!package_allowed(true, "com.spotify.music", "com.whatsapp"));
        assert!(!package_allowed(true, "", "com.whatsapp"));
    }

    #[test]
    fn reading_is_blocked_until_access_and_the_switch_are_on() {
        assert_eq!(read_block(false, true), Some("inbox-listener"));
        assert_eq!(read_block(true, false), Some("inbox-off"));
        assert_eq!(read_block(false, false), Some("inbox-listener"));
        assert_eq!(read_block(true, true), None);
        let off = i18n::line("de", "inbox-off");
        assert!(off.contains("Nachrichtenlesen"));
        let locked = i18n::line("ca-ES-valencia", "inbox-listener");
        assert!(locked.contains("notificacions"));
    }

    #[test]
    fn reply_sends_only_after_the_exact_text_is_confirmed() {
        let mut gate = ConfirmGate::default();
        let draft = ReplyDraft {
            id: "d".into(),
            text: "Next Saturday".into(),
            who: "Mom".into(),
            app: "WhatsApp".into(),
            channel: "whatsapp".into(),
            package: "com.whatsapp".into(),
            phone: "+15551212".into(),
            email: String::new(),
            remote: true,
        };
        let first = gate.reply("en", draft.clone(), true);
        match first {
            ReplyStep::Ask { detail } => {
                assert!(detail.contains("\"Next Saturday\""));
                assert!(detail.contains("Mom"));
                assert!(detail.contains("WhatsApp"));
            }
            other => panic!("first call must ask, got {other:?}"),
        }
        let changed = ReplyDraft { text: "Sunday".into(), ..draft.clone() };
        assert!(matches!(gate.reply("en", changed, true), ReplyStep::Ask { .. }));
        let mut again = ConfirmGate::default();
        assert!(matches!(again.reply("en", draft.clone(), false), ReplyStep::Ask { .. }));
        match again.reply("en", draft.clone(), true) {
            ReplyStep::Send { draft } => assert_eq!(draft.text, "Next Saturday"),
            other => panic!("matching yes must send, got {other:?}"),
        }
        let mut plain = ConfirmGate::default();
        let no_button = ReplyDraft { remote: false, ..draft };
        assert!(matches!(plain.reply("en", no_button.clone(), false), ReplyStep::Ask { .. }));
        assert!(matches!(plain.reply("en", no_button, true), ReplyStep::OpenDraft { .. }));
    }

    #[test]
    fn calls_require_permission_and_an_explicit_yes() {
        let mut gate = ConfirmGate::default();
        assert!(matches!(gate.place("en", "Mom", "+15551212", true, false), CallStep::Stop { .. }));
        assert!(matches!(gate.place("en", "Mom", "+15551212", true, true), CallStep::Ask { .. }));
        match gate.place("en", "Mom", "+1 555-1212", true, true) {
            CallStep::Place { phone, .. } => assert_eq!(phone, "+1 555-1212"),
            other => panic!("same number after yes should dial, got {other:?}"),
        }
        assert!(matches!(gate.place("en", "Mom", "+1999", true, true), CallStep::Ask { .. }));
        assert!(matches!(gate.answer("en", "Mom", true, true, false), CallStep::Stop { .. }));
        assert!(matches!(gate.answer("en", "Mom", true, false, true), CallStep::Stop { .. }));
        assert!(matches!(gate.answer("en", "Mom", false, true, true), CallStep::Ask { .. }));
        assert!(matches!(gate.answer("en", "Mom", true, true, true), CallStep::Answer { .. }));
        assert!(matches!(gate.decline("en", "Mom", false, true, true), CallStep::Ask { .. }));
        assert!(matches!(gate.decline("en", "Mom", true, true, true), CallStep::Decline { .. }));
        assert!(matches!(gate.end("en", true, true, false), CallStep::Stop { .. }));
        assert!(matches!(gate.end("en", false, true, true), CallStep::Ask { .. }));
        assert!(matches!(gate.end("en", true, true, true), CallStep::End));
    }
}
