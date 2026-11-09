//! Phone actions the voice session can call: notes, reminders, alarms, timers, calendar.
//! Notes, reminders, and local calendar events live in `agenda.json`. Ringing is Android.

use std::{
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, NaiveDateTime, Timelike};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tauri::{AppHandle, Manager};

use crate::{diagnostics::Hub, native::NativeBridge, people::{Person, PersonReminder}, recurrence};

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
    /// `none`, `once`, `daily`, `weekly`, or `yearly`. Empty means one-off.
    #[serde(default)]
    pub recurrence: String,
    /// ISO weekdays, 1 = Monday … 7 = Sunday. Used when recurrence is weekly.
    #[serde(default)]
    pub weekdays: Vec<u8>,
    /// Local HH:MM for a repeating reminder.
    #[serde(default)]
    pub time: String,
    /// MM-DD or YYYY-MM-DD for a yearly or one-shot date.
    #[serde(default)]
    pub date: String,
    #[serde(default)]
    pub lead_days: u32,
    #[serde(default)]
    pub person_id: String,
    #[serde(default)]
    pub person_reminder_id: String,
    /// Last moment the series may fire. 0 means it does not end.
    #[serde(default)]
    pub until_ms: i64,
}

impl Item {
    fn basic(id: i32, kind: &str, title: &str, note: &str, at_ms: i64) -> Self {
        Self {
            id,
            kind: kind.into(),
            title: title.into(),
            note: note.into(),
            at_ms,
            recurrence: String::new(),
            weekdays: Vec::new(),
            time: String::new(),
            date: String::new(),
            lead_days: 0,
            person_id: String::new(),
            person_reminder_id: String::new(),
            until_ms: 0,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct ScheduleMeta {
    pub recurrence: String,
    pub weekdays: Vec<u8>,
    pub time: String,
    pub date: String,
    pub lead_days: u32,
    pub person_id: String,
    pub person_reminder_id: String,
    pub until_ms: Option<i64>,
}

#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct RecentFire {
    id: i32,
    title: String,
    at_ms: i64,
    recorded_ms: i64,
    recurrence: String,
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
    /// `daily`, `weekly`, `monthly`, or `yearly`. Empty means it does not repeat.
    #[serde(default)]
    pub recurrence: String,
    #[serde(default)]
    pub weekdays: Vec<u8>,
    /// Last moment an occurrence may start. 0 means the series does not end.
    #[serde(default)]
    pub until_ms: i64,
    /// AlarmManager id for this app's own notification. 0 means it is not armed yet.
    #[serde(default)]
    pub alarm_id: i32,
    #[serde(default, skip_serializing)]
    pub rrule: String,
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
    #[serde(default)]
    recent: Option<RecentFire>,
}

impl Default for Store {
    fn default() -> Self {
        Self { notes: Vec::new(), items: Vec::new(), events: Vec::new(), next_id: 1, recent: None }
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
        fn_tool("list_notes", "Read or search the notes saved on this phone. query matches title or text. id returns one note.", json!({
            "type": "object",
            "properties": { "query": { "type": "string" }, "id": { "type": "string" } }
        })),
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
        fn_tool("set_alarm", "Save an alarm in this app's Time list. Hour is 0-23 and minute is 0-59, for the next time that clock time occurs. A delay such as in 5 minutes is set_reminder with inMinutes, or set_timer with seconds. It is not the phone Clock.", json!({
            "type": "object",
            "properties": {
                "hour": { "type": "integer" },
                "minute": { "type": "integer" },
                "label": { "type": "string" }
            },
            "required": ["hour", "minute"]
        })),
        fn_tool("set_timer", "Start in-app countdown in Time. seconds is the length, such as 300 for five minutes. inMinutes and inHours work too. label or title is what it is for, such as Check EURUSD price. It shows on the Time list under Countdowns and in list_reminders, and this app notifies when it finishes. A request to remind them uses set_reminder with inMinutes.", json!({
            "type": "object",
            "properties": {
                "seconds": { "type": "integer" },
                "inMinutes": { "type": "integer" },
                "inHours": { "type": "integer" },
                "label": { "type": "string" },
                "title": { "type": "string" }
            }
        })),
        fn_tool("set_reminder", "Save a reminder, including inMinutes. For 'in 5 minutes' or 'in 2 hours', pass inMinutes or inHours and omit at. The app computes the fire time. A clock time uses at as YYYY-MM-DDTHH:MM, 7pm, or tomorrow at 9. The reminder shows on the Time screen and in list_reminders until it fires, and this app posts the notification. For a repeating reminder, recurrence is daily or weekly, time is HH:MM, and weekdays is mon,wed,fri. Omit recurrence for a one-off.", json!({
            "type": "object",
            "properties": {
                "title": { "type": "string" },
                "label": { "type": "string" },
                "at": { "type": "string" },
                "in": { "type": "string" },
                "inMinutes": { "type": "integer" },
                "inHours": { "type": "integer" },
                "seconds": { "type": "integer" },
                "note": { "type": "string" },
                "recurrence": { "type": "string" },
                "time": { "type": "string" },
                "weekdays": { "type": "array", "items": { "type": "string" } }
            },
            "required": ["title"]
        })),
        fn_tool("snooze_reminder", "Snooze a reminder. minutes is a delay such as 10. until is a clock time such as 17:00 or 5. id is optional. With no id, snooze the reminder that is ringing or just rang. If the result says to ask, ask which reminder. Do not open another app.", json!({
            "type": "object",
            "properties": {
                "minutes": { "type": "integer" },
                "until": { "type": "string" },
                "id": { "type": "integer" }
            }
        })),
        fn_tool("list_reminders", "List reminders, alarms, and timers, including repeats. Omit dates for every saved reminder. from and to are inclusive YYYY-MM-DD, month is YYYY-MM, span is week, month, or upcoming, and day is one YYYY-MM-DD. A range expands repeating reminders into each time they fire. query searches title and note. Person birthdays and other dates appear only inside their notify window. Each row has an id.", json!({
            "type": "object",
            "properties": {
                "from": { "type": "string" },
                "to": { "type": "string" },
                "month": { "type": "string" },
                "span": { "type": "string" },
                "day": { "type": "string" },
                "query": { "type": "string" },
                "recurring": { "type": "boolean" }
            }
        })),
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
        fn_tool("list_calendar_events", "List events for a day, week, month, or date range in one call. Recurring events expand into each occurrence in that range. month is YYYY-MM. span is week, month, or upcoming. from and to are inclusive YYYY-MM-DD. day is one YYYY-MM-DD; omit every date argument for today. recurring true with no dates lists each repeating series once. query matches title or location. Do not call this once per day. Each event has an id.", json!({
            "type": "object",
            "properties": {
                "day": { "type": "string" },
                "from": { "type": "string" },
                "to": { "type": "string" },
                "month": { "type": "string" },
                "span": { "type": "string" },
                "recurring": { "type": "boolean" },
                "query": { "type": "string" }
            }
        })),
        fn_tool("create_calendar_event", "Save an event in this app's Calendar. start and end are local YYYY-MM-DDTHH:MM, or a phrase such as tomorrow at 9. end may be omitted for one hour. recurrence is daily, weekly, monthly, or yearly. weekdays is mon,wed,fri. until is YYYY-MM-DD. It shows on the Calendar screen and this app notifies when it starts.", json!({
            "type": "object",
            "properties": {
                "title": { "type": "string" },
                "start": { "type": "string" },
                "end": { "type": "string" },
                "location": { "type": "string" },
                "recurrence": { "type": "string" },
                "weekdays": { "type": "array", "items": { "type": "string" } },
                "until": { "type": "string" }
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
        fn_tool("find_contact", "Look up a person when the user wants to call, text, or open WhatsApp, Signal, or email. Checks People nicknames, aliases, and favorites, then phone contacts. Returns phone, email, preferredChannel, openTool, and channelHandle. If preferredChannel is set, use that channel. Speak the match and wait for a clear yes before opening. Never send or place the call yourself. Do not use this to answer a question about a birthday, note, nickname, or preferred channel; use get_person for that.", json!({
            "type": "object",
            "properties": { "name": { "type": "string" } },
            "required": ["name"]
        })),
        fn_tool("get_person", "Answer a question about a person without opening any app. Use this for preferred channel, birthday, anniversary, note, nickname, aliases, favorite, or the linked contact name. Resolves nicknames the same way as find_contact. Speak the detail. Never call open_call, open_sms, open_whatsapp, open_signal, open_email, or open_browser after this.", json!({
            "type": "object",
            "properties": { "name": { "type": "string" } },
            "required": ["name"]
        })),
        fn_tool("list_people", "List people saved in this app, with birthdays and other dates. query matches a name, nickname, or date. Dates stay on the person all year. Does not open any app. Use get_person for one person's preferred channel.", json!({
            "type": "object",
            "properties": { "query": { "type": "string" } }
        })),
        fn_tool("set_person_reminder", "Add or change a birthday, anniversary, or custom date on a person. name is who. label is birthday, anniversary, or a custom title such as visa renewal. date accepts June 15, 6/15, MM-DD, YYYY-MM-DD, or --MM-DD. An existing birthday, anniversary, or same custom title is updated. Creates the person in this app when they are not saved yet. Does not write the phone address book.", json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" },
                "label": { "type": "string" },
                "title": { "type": "string" },
                "date": { "type": "string" },
                "recurrence": { "type": "string" },
                "time": { "type": "string" },
                "leadDays": { "type": "integer" }
            },
            "required": ["name", "date"]
        })),
        fn_tool("create_person", "Add a person saved in this app, with a birthday or other date. name is required. Also nickname, aliases, note, favorite, channel, phone, email, whatsapp, signal, reachOutDays, label, title, date, recurrence, and leadDays. Dates accept June 15, 6/15, MM-DD, and --MM-DD. Does not write the phone address book.", json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" },
                "nickname": { "type": "string" },
                "aliases": { "type": "string" },
                "note": { "type": "string" },
                "favorite": { "type": "boolean" },
                "channel": { "type": "string" },
                "phone": { "type": "string" },
                "email": { "type": "string" },
                "whatsapp": { "type": "string" },
                "signal": { "type": "string" },
                "reachOutDays": { "type": "integer" },
                "label": { "type": "string" },
                "title": { "type": "string" },
                "date": { "type": "string" },
                "recurrence": { "type": "string" },
                "time": { "type": "string" },
                "leadDays": { "type": "integer" }
            },
            "required": ["name"]
        })),
        fn_tool("update_person", "Change a saved person's name, note, channel, or a date. name finds them. rename or displayName changes the name. note, nickname, aliases, favorite, channel, phone, email, whatsapp, signal, and reachOutDays edit those fields. label, title, and date add or replace a birthday, anniversary, or custom date. removeDate true deletes that date. leadDays is how many days early to notify. Omit name and pass label birthday with leadDays to update every birthday. Does not write the phone address book.", json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" },
                "id": { "type": "string" },
                "rename": { "type": "string" },
                "displayName": { "type": "string" },
                "nickname": { "type": "string" },
                "aliases": { "type": "string" },
                "note": { "type": "string" },
                "favorite": { "type": "boolean" },
                "channel": { "type": "string" },
                "phone": { "type": "string" },
                "email": { "type": "string" },
                "whatsapp": { "type": "string" },
                "signal": { "type": "string" },
                "reachOutDays": { "type": "integer" },
                "label": { "type": "string" },
                "title": { "type": "string" },
                "date": { "type": "string" },
                "recurrence": { "type": "string" },
                "time": { "type": "string" },
                "leadDays": { "type": "integer" },
                "removeDate": { "type": "boolean" }
            }
        })),
        fn_tool("delete_person", "Delete one saved person. Pass confirmed true only after a yes. Say who will be deleted and call first with confirmed false. This does not change the phone address book.", json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" },
                "id": { "type": "string" },
                "confirmed": { "type": "boolean" }
            },
            "required": ["confirmed"]
        })),
        fn_tool("get_settings", "Read settings on this phone. It never returns keys or passwords. Report language, local speech, Prefer on device, Listen, quiet gap seconds, daily and monthly voice budget minutes, voice order, whether notifications are normal or spoken out loud, and spokenName when one is saved.", json!({
            "type": "object",
            "properties": {}
        })),
        fn_tool("update_settings", "Change language, Listen, budgets in minutes, Prefer on device, or whether alerts are spoken. language is English, Spanish, German, Portuguese, Valencian, or follow the phone. localSpeech is English or German. preferOnDevice is the Prefer on device switch. listen turns Listen on immediately, and off only after confirmed true. notificationDelivery is normal or spoken. spoken reads reminders and other alerts out loud and does not need confirmation. speakNotifications true is the same as spoken. spokenName is the optional name spoken alerts use, and an empty spokenName clears it. Never invent a name. quietGapSeconds is the quiet gap. dailyBudgetMinutes and monthlyBudgetMinutes are minutes, 0 is off, with dailyHardStop and monthlyHardStop. Also voice, model, instructions, echoGuard, and wakeThreshold. notifyDays or birthdayLeadDays sets how many days early birthday dates notify. Refuse API keys, backup passwords, and reordering or deleting voice keys.", json!({
            "type": "object",
            "properties": {
                "language": { "type": "string" },
                "localSpeech": { "type": "string" },
                "preferOnDevice": { "type": "boolean" },
                "listen": { "type": "boolean" },
                "quietGapSeconds": { "type": "integer" },
                "dailyBudgetMinutes": { "type": "integer" },
                "dailyHardStop": { "type": "boolean" },
                "monthlyBudgetMinutes": { "type": "integer" },
                "monthlyHardStop": { "type": "boolean" },
                "voice": { "type": "string" },
                "model": { "type": "string" },
                "instructions": { "type": "string" },
                "echoGuard": { "type": "boolean" },
                "wakeThreshold": { "type": "number" },
                "notificationDelivery": { "type": "string" },
                "spokenName": { "type": "string" },
                "speakNotifications": { "type": "boolean" },
                "notifyDays": { "type": "integer" },
                "birthdayLeadDays": { "type": "integer" },
                "wipeMemory": { "type": "boolean" },
                "confirmed": { "type": "boolean" }
            }
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
        fn_tool("open_call", "Open the phone dialer with a number filled in. Speak the number, wait for a clear yes, then call with confirmed true. This uses the dialer only. The call is not placed until the user taps Call.", json!({
            "type": "object",
            "properties": {
                "phone": { "type": "string" },
                "confirmed": { "type": "boolean" }
            },
            "required": ["phone", "confirmed"]
        })),
        fn_tool("open_sms", "Open the texting app with a draft. phone is the number. text is the message. Speak both, wait for a clear yes, then call with confirmed true. The message is not sent until the user taps Send.", json!({
            "type": "object",
            "properties": {
                "phone": { "type": "string" },
                "text": { "type": "string" },
                "confirmed": { "type": "boolean" }
            },
            "required": ["phone", "confirmed"]
        })),
        fn_tool("share_item", "Share a note, reminder, or calendar event as text. kind is note, reminder, or event. id comes from list_notes, list_reminders, or list_calendar_events. phone is optional. Speak the text and wait for a clear yes, then call with confirmed true. Nothing is sent until the user confirms in the other app.", json!({
            "type": "object",
            "properties": {
                "kind": { "type": "string" },
                "id": { "type": "string" },
                "phone": { "type": "string" },
                "confirmed": { "type": "boolean" }
            },
            "required": ["kind", "id", "confirmed"]
        })),
        fn_tool("read_notifications", "Read the titles of notifications currently posted on this phone. Does not open them. If access is off, say so.", json!({ "type": "object", "properties": {} })),
        fn_tool("set_dnd", "Set Do Not Disturb. mode is off, priority, alarms, or none. Call immediately. If access is off, say so.", json!({
            "type": "object",
            "properties": { "mode": { "type": "string" } },
            "required": ["mode"]
        })),
        fn_tool("go_home", "Leave this app and show the phone's home screen. Call immediately.", json!({ "type": "object", "properties": {} })),
        fn_tool("phone_status", "Say the battery level and the next alarm from the phone clock. Call this instead of guessing.", json!({ "type": "object", "properties": {} })),
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

/// Spoken once per cloud session. It tells the model how to read local stores.
/// It does not include the stores themselves.
pub fn lookup_hint() -> &'static str {
    " When they ask what is saved on this phone, call the matching list tool once and speak what it returns. list_notes reads notes; pass query to search. list_reminders reads reminders, alarms, and timers. Omit its dates for every saved reminder, or pass from and to, month, or span to cover a range. Repeating reminders expand inside a range. A person's birthday, anniversary, or custom date is included in list_reminders only inside its notify window. list_people and get_person keep those dates all year and do not open an app. list_calendar_events returns every calendar event in one range, including each occurrence of a repeating event. For this month or the rest of the month, pass month as YYYY-MM, or from and to as YYYY-MM-DD. span may be week, month, or upcoming. Pass recurring true with no dates to list each repeating series once. Do not look up one day at a time. create_person adds someone in People, including a birthday or custom date such as a visa renewal. update_person renames them or changes a note, channel, reach-out cadence, or date. delete_person removes them only after a clear yes, then call it again with confirmed true. Do not say you cannot edit People. get_settings reads settings. update_settings changes language, Prefer on device, Listen, the quiet gap in seconds, the daily or monthly voice budget in minutes, and notificationDelivery. notificationDelivery is normal or spoken. Spoken reads alerts out loud and does not need confirmation. Turning Listen off and wiping memory need confirmed true. Never save, read back, or reorder an API key or a backup password. Days before a birthday are leadDays on that date, not a separate store. Call the tool instead of saying a person or setting cannot be changed. Reminders, alarms, and countdowns are saved in this app's Time list. Calendar events are saved in this app's Calendar. 'Remind me in N minutes' or 'in N hours' is set_reminder with inMinutes or inHours. A clock time such as 7pm or tomorrow at 9 is set_reminder with at, or set_alarm with hour and minute. A countdown is set_timer with seconds. create_calendar_event saves one-shot and repeating events in this app. list_reminders and list_calendar_events see those items until they fire or are removed. Do not create them in the phone Clock or the phone calendar, and do not say a reminder needs a clock time. Do not say one is set unless the tool result says it was saved in this app."
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
        crate::files::write_private(path, &bytes).map_err(|e| e.to_string())
    }

    pub fn snapshot(&self, app: &AppHandle) -> AgendaSnapshot {
        self.sweep(app);
        let store = self.lock();
        let (mut events, calendar) = match calendar_range(app, None) {
            Ok((events, calendar)) => (events, calendar),
            Err(_) => (Vec::new(), "denied".into()),
        };
        let (start, end, _) = day_bounds(None).unwrap_or((0, 0, String::new()));
        events.extend(local_in_range(&store.events, start, end));
        events.sort_by_key(|e| e.start_ms);
        let now = Local::now();
        AgendaSnapshot {
            notes: store.notes.clone(),
            reminders: store.items.iter().filter(|item| shows_on_time_list(item, now)).cloned().collect(),
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
        events.extend(local_in_range(&store.events, start, end));
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
        schedule: Option<ScheduleMeta>,
    ) -> Result<Item, String> {
        if title.trim().is_empty() {
            return Err("A title is required.".into());
        }
        let kind = if kind.is_empty() { "reminder" } else { kind };
        if let Some(id) = id {
            if let Some(existing) = self.item(id) {
                if !existing.person_id.is_empty() {
                    return self.update_person_item(app, &existing, title, note, at_ms, schedule);
                }
            }
        }
        let mut store = self.lock();
        let mut item = if let Some(id) = id {
            let Some(existing) = store.items.iter().find(|i| i.id == id).cloned() else {
                return Err("Nothing scheduled with that id.".into());
            };
            existing
        } else {
            let id = Self::alloc_id(&mut store);
            Item::basic(id, kind, "", "", at_ms)
        };
        item.kind = kind.into();
        item.title = title.chars().take(120).collect();
        item.note = note.chars().take(500).collect();
        if let Some(schedule) = schedule {
            apply_schedule(&mut item, &schedule, at_ms)?;
        } else if item.recurrence.is_empty() {
            item.at_ms = at_ms;
        } else {
            let kept = ScheduleMeta {
                recurrence: item.recurrence.clone(),
                weekdays: item.weekdays.clone(),
                time: item.time.clone(),
                date: item.date.clone(),
                lead_days: item.lead_days,
                ..ScheduleMeta::default()
            };
            apply_schedule(&mut item, &kept, at_ms)?;
        }
        if item.at_ms <= Local::now().timestamp_millis() + 5_000 {
            if id.is_none() {
                store.next_id = store.next_id.saturating_sub(1).max(1);
            }
            return Err("That time is in the past.".into());
        }
        if let Some(existing) = store.items.iter_mut().find(|i| i.id == item.id) {
            *existing = item.clone();
        } else {
            store.items.push(item.clone());
        }
        store.items.sort_by_key(|i| i.at_ms);
        Self::persist(&self.path, &store)?;
        drop(store);
        let _ = app.state::<NativeBridge>().schedule_item(item.id, &item.kind, &item.title, &item.note, item.at_ms);
        touch(app);
        Ok(item)
    }

    fn update_person_item(
        &self,
        app: &AppHandle,
        existing: &Item,
        title: &str,
        note: &str,
        at_ms: i64,
        schedule: Option<ScheduleMeta>,
    ) -> Result<Item, String> {
        let people = app.state::<crate::people::People>();
        let Some(mut person) = people.list().into_iter().find(|p| p.id == existing.person_id) else {
            return Err("That person is no longer saved.".into());
        };
        let Some(reminder) = person.reminders.iter_mut().find(|r| r.id == existing.person_reminder_id) else {
            return Err("That person reminder is gone.".into());
        };
        if let Some(schedule) = &schedule {
            let kind = recurrence::normalize_kind(&schedule.recurrence);
            if matches!(kind.as_str(), "daily" | "weekly") {
                return Err("This date stays on the person. Daily and weekly reminders are separate.".into());
            }
            if kind == "once" || kind == "yearly" {
                reminder.recurrence = kind;
            }
            if !schedule.date.is_empty() {
                reminder.date = schedule.date.clone();
            }
            if !schedule.time.is_empty() {
                reminder.time = schedule.time.clone();
            }
            reminder.lead_days = schedule.lead_days;
        } else if let Some(at) = DateTime::from_timestamp_millis(at_ms) {
            let at = at.with_timezone(&Local);
            reminder.time = format!("{:02}:{:02}", at.hour(), at.minute());
        }
        if !title.trim().is_empty() {
            reminder.title = title.trim().chars().take(80).collect();
        }
        let _ = note;
        let person_id = person.id.clone();
        let reminders = person.reminders.clone();
        people.set_reminders(&person_id, reminders)?;
        sync_person_reminders(app)?;
        self.item(existing.id).ok_or_else(|| "The reminder was removed.".into())
    }

    pub fn cancel(&self, app: &AppHandle, id: i32) -> Result<(), String> {
        let linked = self.item(id).filter(|item| !item.person_id.is_empty());
        if let Some(item) = linked {
            let people = app.state::<crate::people::People>();
            if let Some(mut person) = people.list().into_iter().find(|p| p.id == item.person_id) {
                person.reminders.retain(|r| r.id != item.person_reminder_id);
                people.set_reminders(&person.id, person.reminders)?;
            }
            sync_person_reminders(app)?;
            return Ok(());
        }
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

    pub fn complete(&self, app: &AppHandle, id: i32) -> Result<String, String> {
        let Some(item) = self.item(id) else {
            return Err("Nothing scheduled with that id.".into());
        };
        if !recurrence::is_repeating(&item.recurrence) {
            self.cancel(app, id)?;
            return Ok(format!("Done with {}.", item.title));
        }
        let spec = spec_from_item(&item)?;
        let after = DateTime::from_timestamp_millis(item.at_ms)
            .map(|t| t.with_timezone(&Local))
            .unwrap_or_else(Local::now);
        let Some(next) = recurrence::next_after(&spec, after) else {
            self.cancel(app, id)?;
            return Ok("That was the last one.".into());
        };
        self.retarget(app, id, next.timestamp_millis())?;
        Ok(format!("Next {} is {}.", item.title, next.format("%a %-d %b %-H:%M")))
    }

    fn retarget(&self, app: &AppHandle, id: i32, at_ms: i64) -> Result<(), String> {
        let mut store = self.lock();
        let Some(item) = store.items.iter_mut().find(|i| i.id == id) else {
            return Err("Nothing scheduled with that id.".into());
        };
        item.at_ms = at_ms;
        let item = item.clone();
        store.items.sort_by_key(|i| i.at_ms);
        Self::persist(&self.path, &store)?;
        drop(store);
        let _ = app.state::<NativeBridge>().schedule_item(item.id, &item.kind, &item.title, &item.note, item.at_ms);
        touch(app);
        Ok(())
    }

    pub fn upsert_local_event(&self, app: &AppHandle, mut event: CalEvent) -> Result<CalEvent, String> {
        let mut store = self.lock();
        if let Some(existing) = store.events.iter().find(|e| e.id == event.id) {
            if event.alarm_id <= 0 {
                event.alarm_id = existing.alarm_id;
            }
        }
        if event.alarm_id <= 0 {
            event.alarm_id = Self::alloc_id(&mut store);
        }
        if let Some(existing) = store.events.iter_mut().find(|e| e.id == event.id) {
            *existing = event.clone();
        } else {
            store.events.push(event.clone());
        }
        store.events.sort_by_key(|e| e.start_ms);
        Self::persist(&self.path, &store)?;
        drop(store);
        arm_local_event(app, &event);
        touch(app);
        Ok(event)
    }

    pub fn delete_local_event(&self, app: &AppHandle, id: &str) -> Result<(), String> {
        let mut store = self.lock();
        let alarm_id = store.events.iter().find(|e| e.id == id).map(|e| e.alarm_id).unwrap_or(0);
        let before = store.events.len();
        store.events.retain(|e| e.id != id);
        if store.events.len() == before {
            return Err("No local event with that id.".into());
        }
        Self::persist(&self.path, &store)?;
        drop(store);
        if alarm_id > 0 {
            let _ = app.state::<NativeBridge>().cancel_item(alarm_id);
        }
        touch(app);
        Ok(())
    }

    pub fn local_event(&self, id: &str) -> Option<CalEvent> {
        self.lock().events.iter().find(|e| e.id == id).cloned()
    }

    pub fn item(&self, id: i32) -> Option<Item> {
        self.lock().items.iter().find(|i| i.id == id).cloned()
    }

    pub fn notes(&self) -> Vec<Note> {
        self.lock().notes.clone()
    }

    fn sweep(&self, app: &AppHandle) {
        let now = Local::now();
        let now_ms = now.timestamp_millis();
        let mut store = self.lock();
        let mut cancel = Vec::new();
        let mut arm = Vec::new();
        let mut kept = Vec::new();
        let mut recent: Option<RecentFire> = None;
        for mut item in store.items.drain(..) {
            if item.at_ms + 60_000 >= now_ms {
                kept.push(item);
                continue;
            }
            // A person date stays on this occurrence through the event day so
            // Time can show it for the whole heads-up window. The next year's
            // alarm is armed once that day has passed.
            if person_date_still_open(&item, now) {
                kept.push(item);
                continue;
            }
            let fire = RecentFire {
                id: item.id,
                title: item.title.clone(),
                at_ms: item.at_ms,
                recorded_ms: now_ms,
                recurrence: item.recurrence.clone(),
            };
            if recent.as_ref().map(|r| fire.at_ms >= r.at_ms).unwrap_or(true) {
                recent = Some(fire);
            }
            if recurrence::is_repeating(&item.recurrence) {
                if let Some(next) = spec_from_item(&item).ok().and_then(|spec| recurrence::next_after(&spec, now)) {
                    item.at_ms = next.timestamp_millis();
                    arm.push(item.clone());
                    kept.push(item);
                    continue;
                }
            }
            cancel.push(item.id);
        }
        if let Some(recent) = recent {
            store.recent = Some(recent);
        }
        if cancel.is_empty() && arm.is_empty() {
            store.items = kept;
            return;
        }
        store.items = kept;
        store.items.sort_by_key(|i| i.at_ms);
        let _ = Self::persist(&self.path, &store);
        drop(store);
        let bridge = app.state::<NativeBridge>();
        for id in cancel {
            let _ = bridge.cancel_item(id);
        }
        for item in arm {
            let _ = bridge.schedule_item(item.id, &item.kind, &item.title, &item.note, item.at_ms);
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
    rearm_events(app);
}

pub fn run(app: &AppHandle, name: &str, arguments: &str) -> String {
    let args: Value = serde_json::from_str(arguments).unwrap_or(Value::Null);
    let result = dispatch(app, name, &args);
    let out = match result {
        Ok(v) => v.to_string(),
        Err(e) => json!({ "ok": false, "error": e }).to_string(),
    };
    if let Some(hub) = app.try_state::<Arc<Hub>>() {
        let ok = serde_json::from_str::<Value>(&out).ok().and_then(|v| v["ok"].as_bool()).unwrap_or(false);
        if !ok && out.contains("needs a network connection") {
            hub.error("tool needs a network connection");
        }
        hub.info(format!("tool {name}: {}", if ok { "ok" } else { "failed" }));
    }
    out
}

fn unconfirmed_handoff(name: &str, args: &Value) -> Option<Value> {
    if args["confirmed"] == true {
        return None;
    }
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
        "open_call" => format!(
            "Open the dialer for {}? The call is not placed until you tap Call.",
            args["phone"].as_str().unwrap_or("")
        ),
        "open_sms" => {
            let phone = args["phone"].as_str().unwrap_or("");
            let text = args["text"].as_str().unwrap_or("").trim();
            if text.is_empty() {
                format!("Open a text to {phone}? Nothing is sent until you tap Send.")
            } else {
                format!("Open a text to {phone}: {text}? Nothing is sent until you tap Send.")
            }
        }
        "open_email" => format!(
            "Open an email to {} about {}? {}",
            args["to"].as_str().unwrap_or(""),
            args["subject"].as_str().unwrap_or(""),
            args["body"].as_str().unwrap_or("")
        ),
        _ => return None,
    };
    Some(json!({
        "ok": true,
        "opened": false,
        "needsConfirm": true,
        "detail": format!("{plan} {}", crate::i18n::line("en", "handoff-wait"))
    }))
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HandoffDraft {
    pub id: String,
    pub summary: String,
    pub kind: String,
    pub query: String,
    pub url: String,
    pub phone: String,
    pub text: String,
    pub to: String,
    pub subject: String,
    pub body: String,
}

#[derive(Default)]
pub struct Handoffs {
    inner: Mutex<Option<HandoffDraft>>,
}

impl Handoffs {
    pub fn stage(&self, draft: HandoffDraft) {
        *self.inner.lock().unwrap_or_else(|err| err.into_inner()) = Some(draft);
    }

    pub fn take(&self) -> Option<HandoffDraft> {
        self.inner.lock().unwrap_or_else(|err| err.into_inner()).take()
    }

    pub fn clear(&self) {
        *self.inner.lock().unwrap_or_else(|err| err.into_inner()) = None;
    }
}

fn stage_handoff(app: &AppHandle, kind: &str, summary: &str, args: &Value) -> Result<(), String> {
    let prepared = prepare_handoff(kind, args)?;
    let draft = HandoffDraft {
        id: format!("h{}", crate::diagnostics::now_ms()),
        summary: summary.to_string(),
        kind: kind.to_string(),
        query: prepared.query,
        url: prepared.url,
        phone: prepared.phone,
        text: prepared.text,
        to: prepared.to,
        subject: prepared.subject,
        body: prepared.body,
    };
    if let Some(handoffs) = app.try_state::<Handoffs>() {
        handoffs.stage(draft.clone());
    }
    if let Some(hub) = app.try_state::<Arc<Hub>>() {
        hub.emit("voice://handoff", &draft);
    }
    Ok(())
}

fn clear_handoff(app: &AppHandle) {
    if let Some(handoffs) = app.try_state::<Handoffs>() {
        handoffs.clear();
    }
    if let Some(hub) = app.try_state::<Arc<Hub>>() {
        hub.emit("voice://handoff", Value::Null);
    }
}

pub fn confirm_pending(app: &AppHandle) -> Result<Value, String> {
    let Some(handoffs) = app.try_state::<Handoffs>() else {
        return Err("Nothing is waiting to send.".into());
    };
    let Some(draft) = handoffs.take() else {
        return Err("Nothing is waiting to send.".into());
    };
    if let Some(done) = confirm_local(app, &draft) {
        clear_handoff(app);
        return done;
    }
    let prepared = prepare_handoff(
        &draft.kind,
        &json!({
            "query": draft.query,
            "url": draft.url,
            "phone": draft.phone,
            "text": draft.text,
            "to": draft.to,
            "subject": draft.subject,
            "body": draft.body,
        }),
    )?;
    let raw = app
        .state::<NativeBridge>()
        .open_hand_off(
            &draft.kind,
            &prepared.query,
            &prepared.url,
            &prepared.phone,
            &prepared.text,
            &prepared.to,
            &prepared.subject,
            &prepared.body,
        )
        .map_err(|err| err.to_string())?;
    clear_handoff(app);
    serde_json::from_str(&raw).map_err(|err| err.to_string())
}

fn locale_of(app: &AppHandle) -> String {
    app.try_state::<crate::settings::SettingsStore>().map(|store| store.get().locale).unwrap_or_default()
}

fn confirm_local(app: &AppHandle, draft: &HandoffDraft) -> Option<Result<Value, String>> {
    match draft.kind.as_str() {
        "delete_person" => Some(commit_people(app, crate::people::delete_from_tool(&app.state::<crate::people::People>(), &json!({"id": draft.query, "confirmed": true})))),
        "listen_off" => Some(apply_settings(app, &json!({"listen": false, "confirmed": true}))),
        "wipe_memory" => Some(apply_settings(app, &json!({"wipeMemory": true, "confirmed": true}))),
        _ => None,
    }
}

fn stage_confirm(app: &AppHandle, kind: &str, summary: &str, id: &str) {
    let draft = HandoffDraft {
        id: format!("h{}", crate::diagnostics::now_ms()),
        summary: summary.to_string(),
        kind: kind.to_string(),
        query: id.to_string(),
        url: String::new(),
        phone: String::new(),
        text: String::new(),
        to: String::new(),
        subject: String::new(),
        body: String::new(),
    };
    if let Some(handoffs) = app.try_state::<Handoffs>() {
        handoffs.stage(draft.clone());
    }
    if let Some(hub) = app.try_state::<Arc<Hub>>() {
        hub.emit("voice://handoff", &draft);
    }
}

fn commit_people(app: &AppHandle, result: Result<Value, String>) -> Result<Value, String> {
    let mut value = result?;
    if value["changed"] == true {
        sync_person_reminders(app)?;
        emit_people(app);
    }
    if value["needsConfirm"] == true {
        let name = value["name"].as_str().unwrap_or("this person").to_string();
        let detail = crate::i18n::fill(&locale_of(app), "person-delete-ask", &[("name", &name)]);
        value["detail"] = json!(detail.clone());
        stage_confirm(app, "delete_person", &detail, value["personId"].as_str().unwrap_or(""));
    }
    Ok(value)
}

fn emit_people(app: &AppHandle) {
    if let Some(hub) = app.try_state::<Arc<Hub>>() {
        hub.emit("voice://people", json!({}));
    }
}

fn read_settings(app: &AppHandle) -> Result<Value, String> {
    let settings = app.state::<crate::settings::SettingsStore>().get();
    Ok(crate::settings::describe_settings(&settings, &voice_order(app)))
}

fn voice_order(app: &AppHandle) -> Vec<String> {
    let Some(vault) = app.try_state::<crate::vault::Vault>() else {
        return Vec::new();
    };
    let Some(bridge) = app.try_state::<NativeBridge>() else {
        return Vec::new();
    };
    match vault.status(&bridge) {
        Ok(status) => status
            .accounts
            .into_iter()
            .map(|account| match account.provider.as_str() {
                "xai" => "xAI".into(),
                "openai" => "OpenAI".into(),
                "local" => "On device".into(),
                other => other.to_string(),
            })
            .collect(),
        Err(_) => Vec::new(),
    }
}

fn apply_settings(app: &AppHandle, args: &Value) -> Result<Value, String> {
    let store = app.state::<crate::settings::SettingsStore>();
    let edit = match crate::settings::edit_settings(&store.get(), args) {
        Ok(edit) => edit,
        Err(err) => return Err(localize_tool_error(app, err)),
    };
    let mut detail = edit.detail;
    if let Some(next) = edit.settings {
        let saved = store.save(next).map_err(|err| err.to_string())?;
        publish_settings(app, &saved, edit.listen);
    }
    if let Some(lead) = edit.person_lead.clone() {
        let value = crate::people::set_lead_days(&app.state::<crate::people::People>(), &lead.label, lead.days)?;
        if value["changed"] == true {
            sync_person_reminders(app)?;
            emit_people(app);
        }
        let lead_detail = value["detail"].as_str().unwrap_or("").to_string();
        if !lead_detail.is_empty() {
            if !detail.is_empty() {
                detail.push(' ');
            }
            detail.push_str(&lead_detail);
        }
    }
    if edit.wipe_memory {
        let _ = app.state::<crate::local_memory::LocalMemory>().wipe();
    }
    if let Some(key) = edit.pending {
        let ask = crate::i18n::line(&locale_of(app), key);
        if !detail.is_empty() {
            detail.push(' ');
        }
        detail.push_str(&ask);
        let kind = match key {
            "listen-off-ask" => "listen_off",
            "wipe-ask" => "wipe_memory",
            _ => "confirm",
        };
        stage_confirm(app, kind, &ask, "");
        return Ok(json!({ "ok": true, "needsConfirm": true, "changed": false, "detail": detail }));
    }
    if detail.is_empty() {
        detail = "Saved.".into();
    }
    Ok(json!({ "ok": true, "changed": true, "needsConfirm": false, "detail": detail }))
}

fn publish_settings(app: &AppHandle, saved: &crate::settings::Settings, listen: Option<bool>) {
    if let Some(engine) = app.try_state::<crate::audio::engine::Engine>() {
        engine.send(crate::audio::engine::Ctl::ReloadSettings);
        if let Some(on) = listen {
            engine.send(crate::audio::engine::Ctl::SetWakeEnabled {
                enabled: on,
                reason: if on { "voice turned Listen on" } else { "voice turned Listen off" },
            });
        }
    }
    if let Some(hub) = app.try_state::<Arc<Hub>>() {
        hub.emit("voice://settings", saved);
        if let Some(on) = listen {
            hub.emit("voice://wake_enabled", on);
        }
    }
}

fn localize_tool_error(app: &AppHandle, err: String) -> String {
    if err == crate::settings::SECRET_REFUSAL || err == crate::settings::KEYS_REFUSAL {
        crate::i18n::line(&locale_of(app), &err)
    } else {
        err
    }
}

struct Handoff {
    query: String,
    url: String,
    phone: String,
    text: String,
    to: String,
    subject: String,
    body: String,
}

fn prepare_handoff(kind: &str, args: &Value) -> Result<Handoff, String> {
    let mut handoff = Handoff {
        query: String::new(),
        url: String::new(),
        phone: String::new(),
        text: clip(args["text"].as_str().unwrap_or(""), 1000),
        to: String::new(),
        subject: clip(args["subject"].as_str().unwrap_or(""), 200),
        body: clip(args["body"].as_str().unwrap_or(""), 4000),
    };
    match kind {
        "browser" => {
            let (query, url) = browser_target(args["query"].as_str().unwrap_or(""), args["url"].as_str().unwrap_or(""))?;
            handoff.query = query;
            handoff.url = url;
        }
        "youtube" => {
            let (query, url) = youtube_target(args["query"].as_str().unwrap_or(""), args["url"].as_str().unwrap_or(""))?;
            handoff.query = query;
            handoff.url = url;
        }
        "whatsapp" | "signal" => handoff.phone = phone_number(args["phone"].as_str().unwrap_or(""), 8)?,
        "call" | "sms" => handoff.phone = phone_number(args["phone"].as_str().unwrap_or(""), 3)?,
        "email" => handoff.to = email_address(args["to"].as_str().unwrap_or(""))?,
        "share" => {
            if handoff.text.trim().is_empty() && handoff.body.trim().is_empty() {
                return Err("There is nothing to share.".into());
            }
        }
        _ => return Err("Unknown app.".into()),
    }
    Ok(handoff)
}

fn browser_target(query: &str, url: &str) -> Result<(String, String), String> {
    let raw = if !url.trim().is_empty() { url.trim() } else { query.trim() };
    if raw.is_empty() {
        return Err("Say a site or a search.".into());
    }
    if raw.contains("://") {
        return Ok((String::new(), require_public_https(raw)?));
    }
    if raw.contains(':') {
        return Err("Only https links can be opened.".into());
    }
    if raw.contains(' ') || !looks_like_host(raw) {
        if raw.chars().count() > 300 || raw.chars().any(|c| c.is_control()) {
            return Err("That search is too long.".into());
        }
        return Ok((raw.to_string(), String::new()));
    }
    Ok((String::new(), require_public_https(&format!("https://{raw}"))?))
}

fn youtube_target(query: &str, url: &str) -> Result<(String, String), String> {
    if !url.trim().is_empty() {
        let url = require_public_https(url.trim())?;
        let host = host_of(&url).ok_or("That is not a YouTube link.")?;
        if !youtube_host(&host) {
            return Err("That is not a YouTube link.".into());
        }
        return Ok((String::new(), url));
    }
    let query = query.trim();
    if query.is_empty() {
        return Err("Say what to play on YouTube.".into());
    }
    if query.contains("://") || query.chars().any(|c| c.is_control()) || query.chars().count() > 300 {
        return Err("Say a YouTube search, not a link.".into());
    }
    Ok((query.to_string(), String::new()))
}

fn require_public_https(raw: &str) -> Result<String, String> {
    let err = "That link is not a safe https address.";
    if raw.chars().count() > 2000 || raw.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(err.into());
    }
    let Some(rest) = raw.strip_prefix("https://") else {
        return Err("Only https links can be opened.".into());
    };
    if rest.is_empty() {
        return Err(err.into());
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.is_empty() || authority.contains('@') || authority.contains('\\') {
        return Err(err.into());
    }
    let host = match authority.rsplit_once(':') {
        Some((host, port)) if !host.contains(':') && port.chars().all(|c| c.is_ascii_digit()) && !port.is_empty() => host,
        _ => authority,
    };
    if host.contains(':') {
        return Err(err.into());
    }
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty()
        || !host.contains('.')
        || host == "localhost"
        || host.ends_with(".localhost")
        || host.ends_with(".local")
        || host.parse::<std::net::Ipv4Addr>().is_ok()
        || host.parse::<std::net::Ipv6Addr>().is_ok()
    {
        return Err(err.into());
    }
    Ok(raw.to_string())
}

fn host_of(url: &str) -> Option<String> {
    let rest = url.strip_prefix("https://")?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = match authority.rsplit_once(':') {
        Some((host, port)) if port.chars().all(|c| c.is_ascii_digit()) => host,
        _ => authority,
    };
    Some(host.trim_end_matches('.').to_ascii_lowercase())
}

fn youtube_host(host: &str) -> bool {
    host == "youtu.be"
        || host == "youtube.com"
        || host.ends_with(".youtube.com")
        || host == "youtube-nocookie.com"
        || host.ends_with(".youtube-nocookie.com")
}

fn looks_like_host(raw: &str) -> bool {
    raw.len() <= 253
        && raw.contains('.')
        && !raw.contains('/')
        && !raw.contains('@')
        && raw.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-'))
}

fn phone_number(raw: &str, min_digits: usize) -> Result<String, String> {
    let raw = raw.trim();
    if raw.chars().any(|c| !(c.is_ascii_digit() || c.is_whitespace() || matches!(c, '+' | '-' | '(' | ')'))) {
        return Err("A phone number is required.".into());
    }
    let plus = raw.starts_with('+');
    let digits: String = raw.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() < min_digits || digits.len() > 15 {
        return Err("A phone number is required.".into());
    }
    Ok(if plus { format!("+{digits}") } else { digits })
}

fn email_address(raw: &str) -> Result<String, String> {
    let raw = raw.trim();
    if raw.len() > 254 || raw.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("Email needs an address.".into());
    }
    let Some((local, host)) = raw.split_once('@') else {
        return Err("Email needs an address.".into());
    };
    let local_ok = !local.is_empty() && local.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '%' | '+' | '-'));
    let host_ok = host.contains('.')
        && !host.starts_with('.')
        && !host.ends_with('.')
        && host.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-'));
    if !local_ok || !host_ok {
        return Err("Email needs an address.".into());
    }
    Ok(raw.to_string())
}

fn clip(value: &str, max: usize) -> String {
    value.chars().filter(|c| !c.is_control() || *c == '\n' || *c == '\t').take(max).collect()
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
        "find_contact" => return crate::people::find_contact(app, args["name"].as_str().unwrap_or("")).map(Some),
        "get_person" => return crate::people::describe_person(app, args["name"].as_str().unwrap_or("")).map(Some),
        "read_notifications" => bridge.read_notifications().map_err(|e| e.to_string())?,
        "set_dnd" => bridge.set_dnd(args["mode"].as_str().unwrap_or("")).map_err(|e| e.to_string())?,
        "go_home" => bridge.go_home().map_err(|e| e.to_string())?,
        "phone_status" => bridge.phone_status().map_err(|e| e.to_string())?,
        "open_youtube" | "open_whatsapp" | "open_signal" | "open_email" | "open_browser" | "open_call" | "open_sms" => {
            if let Some(wait) = unconfirmed_handoff(name, args) {
                stage_handoff(app, name.trim_start_matches("open_"), wait["detail"].as_str().unwrap_or(""), args)?;
                return Ok(Some(wait));
            }
            clear_handoff(app);
            let kind = name.trim_start_matches("open_");
            let handoff = prepare_handoff(kind, args)?;
            bridge
                .open_hand_off(
                    kind,
                    &handoff.query,
                    &handoff.url,
                    &handoff.phone,
                    &handoff.text,
                    &handoff.to,
                    &handoff.subject,
                    &handoff.body,
                )
                .map_err(|e| e.to_string())?
        }
        _ => return Ok(None),
    };
    serde_json::from_str(&raw).map(Some).map_err(|e| e.to_string())
}

fn dispatch(app: &AppHandle, name: &str, args: &Value) -> Result<Value, String> {
    let online = app
        .state::<crate::native::NativeBridge>()
        .device_facts()
        .map(|facts| facts.online)
        .unwrap_or(true);
    if let Some(message) = crate::local::network_block(name, online) {
        return Err(message.into());
    }
    if name == "web_search" {
        return crate::search::lookup(app, args["query"].as_str().unwrap_or(""));
    }
    if let Some(value) = phone_tool(app, name, args)? {
        return Ok(value);
    }
    if name == "share_item" {
        return share_item(app, args);
    }
    let agenda = app.state::<Agenda>();
    match name {
        "create_note" => {
            let note = agenda.upsert_note(app, None, args["title"].as_str().unwrap_or(""), args["text"].as_str().unwrap_or(""))?;
            Ok(json!({ "ok": true, "id": note.id, "title": note.title, "detail": format!("Saved a note: {}", note.title) }))
        }
        "list_notes" => Ok(notes_payload(&agenda.notes(), args)),
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
            let seconds = timer_seconds(args)?;
            if !(1..=86_400).contains(&seconds) {
                return Err("A timer has to be between 1 second and 24 hours.".into());
            }
            let label = timer_label(args);
            let when = Local::now() + Duration::seconds(i64::from(seconds));
            let id = agenda.take_id()?;
            let raw = app
                .state::<NativeBridge>()
                .set_timer(seconds, &label, id, when.timestamp_millis())
                .map_err(|e| e.to_string())?;
            finish_schedule(app, &agenda, id, "timer", &label, "", when, &raw)
        }
        "set_reminder" => {
            let title = request_title(args);
            if title.is_empty() {
                return Err("A reminder needs a title.".into());
            }
            let note = args["note"].as_str().unwrap_or("").trim();
            let schedule = schedule_from_args(args)?;
            let at_ms = reminder_anchor_ms(args, &schedule)?;
            let item = agenda.upsert_item(app, None, "reminder", &title, note, at_ms, Some(schedule))?;
            let when = DateTime::from_timestamp_millis(item.at_ms)
                .map(|t| t.with_timezone(&Local).format("%a %-d %b %-H:%M").to_string())
                .unwrap_or_default();
            let pattern = recurrence_phrase(&item);
            Ok(json!({
                "ok": true,
                "id": item.id,
                "kind": "reminder",
                "detail": format!("Reminder saved in this app's Time list for {when}: {title}.{pattern}")
            }))
        }
        "snooze_reminder" => snooze_reminder(app, &agenda, args),
        "get_person" => crate::people::describe_person(app, args["name"].as_str().unwrap_or("")),
        "set_person_reminder" | "create_person" | "update_person" | "delete_person" => {
            let result = {
                let people = app.state::<crate::people::People>();
                match name {
                    "create_person" => crate::people::create_from_tool(&people, args),
                    "update_person" => crate::people::update_from_tool(&people, args),
                    "delete_person" => crate::people::delete_from_tool(&people, args),
                    _ => crate::people::write_person_date(&people, args),
                }
            };
            commit_people(app, result)
        }
        "get_settings" => read_settings(app),
        "update_settings" => apply_settings(app, args),
        "list_reminders" => {
            agenda.sweep(app);
            let now = Local::now();
            let items = agenda.lock().items.clone();
            reminders_payload(&items, args, now)
        }
        "list_people" => Ok(crate::people::saved_people_payload(
            &app.state::<crate::people::People>().list(),
            args["query"].as_str().unwrap_or(""),
        )),
        "update_reminder" => {
            let id = args["id"].as_i64().ok_or("id from list_reminders is required")? as i32;
            let existing = agenda.item(id).ok_or("Nothing scheduled with that id.")?;
            let title = args["title"].as_str().map(str::trim).filter(|s| !s.is_empty()).unwrap_or(&existing.title);
            let note = args["note"].as_str().map(str::trim).unwrap_or(&existing.note);
            let at_ms = match args["at"].as_str().map(str::trim).filter(|s| !s.is_empty()) {
                Some(s) => parse_local_when(s)?.timestamp_millis(),
                None => existing.at_ms,
            };
            let schedule = if args.get("recurrence").is_some() || args.get("weekdays").is_some() || args.get("time").is_some() {
                Some(schedule_from_args(args)?)
            } else {
                None
            };
            let item = agenda.upsert_item(app, Some(id), &existing.kind, title, note, at_ms, schedule)?;
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
        "list_calendar_events" => list_calendar_events(app, &agenda, args),
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
    let start = parse_flexible_when(args["start"].as_str().unwrap_or(""))?;
    let end = match args["end"].as_str().map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => parse_flexible_when(s)?,
        None => start + Duration::hours(1),
    };
    if end <= start {
        return Err("The event ends before it starts.".into());
    }
    let location = args["location"].as_str().unwrap_or("").trim().chars().take(200).collect::<String>();
    let kind = recurrence::normalize_kind(args["recurrence"].as_str().unwrap_or(""));
    let event = CalEvent {
        id: format!("local-{}", Local::now().timestamp_millis()),
        title: title.clone(),
        start_ms: start.timestamp_millis(),
        end_ms: end.timestamp_millis(),
        location,
        all_day: false,
        source: "local".into(),
        recurrence: if kind == "none" { String::new() } else { kind },
        weekdays: weekday_args(args),
        until_ms: until_end_ms(args["until"].as_str().unwrap_or("")),
        alarm_id: 0,
        rrule: String::new(),
    };
    let event = agenda.upsert_local_event(app, event)?;
    let pattern = if recurrence::is_repeating(&event.recurrence) {
        format!(" Repeats {}.", event.recurrence)
    } else {
        String::new()
    };
    Ok(json!({
        "ok": true,
        "id": event.id,
        "source": "local",
        "detail": format!("Saved \"{title}\" on {} in this app's Calendar.{pattern}", start.format("%a %-d %b %-H:%M"))
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
            event.start_ms = parse_flexible_when(s)?.timestamp_millis();
        }
        if let Some(s) = args["end"].as_str().map(str::trim).filter(|s| !s.is_empty()) {
            event.end_ms = parse_flexible_when(s)?.timestamp_millis();
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
        .update_event(id, title, start_ms, end_ms, location, &recurrence::to_rrule(args["recurrence"].as_str().unwrap_or(""), &weekday_args(args), until_end_ms(args["until"].as_str().unwrap_or(""))))
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
    if v["via"] == "clock" {
        return Ok(json!({
            "ok": false,
            "error": "That was not saved in this app. It is not on the Time screen."
        }));
    }
    let mut store = agenda.lock();
    store.items.retain(|i| i.id != id);
    store.items.push(Item::basic(id, kind, title, note, when.timestamp_millis()));
    store.items.sort_by_key(|i| i.at_ms);
    Agenda::persist(&agenda.path, &store)?;
    drop(store);
    touch(app);
    let spoken = v["detail"].as_str().map(str::to_owned).unwrap_or_else(|| match kind {
        "alarm" => format!("Alarm saved in this app's Time list for {}.", when.format("%-H:%M")),
        "timer" => format!("Countdown saved in this app's Time list for {}.", when.format("%-H:%M")),
        _ => format!("Saved in this app for {}.", when.format("%a %-H:%M")),
    });
    Ok(json!({ "ok": true, "id": id, "detail": spoken }))
}

fn read_phone_range(app: &AppHandle, start: i64, end: i64) -> Result<(Vec<CalEvent>, String), String> {
    let bridge = app.state::<NativeBridge>();
    let status = bridge.check_permissions().map_err(|e| e.to_string())?;
    if status.calendar != "granted" {
        return Ok((Vec::new(), status.calendar));
    }
    let (events, _) = read_events(&bridge, start, end)?;
    Ok((events, "granted".into()))
}

fn calendar_range(app: &AppHandle, day: Option<&str>) -> Result<(Vec<CalEvent>, String), String> {
    let bridge = app.state::<NativeBridge>();
    let status = bridge.check_permissions().map_err(|e| e.to_string())?;
    if status.calendar != "granted" {
        return Ok((Vec::new(), status.calendar));
    }
    let (start, end, _) = day_bounds(day)?;
    let (events, _) = read_events(&bridge, start, end)?;
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

fn ensure_calendar(app: &AppHandle, start: i64, end: i64) -> Result<(Vec<CalEvent>, bool), String> {
    ensure_calendar_permission(app)?;
    read_events(&app.state::<NativeBridge>(), start, end)
}

fn read_events(bridge: &NativeBridge, start: i64, end: i64) -> Result<(Vec<CalEvent>, bool), String> {
    let raw = bridge.list_calendar(start, end).map_err(|e| e.to_string())?;
    let v: Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    if v["ok"] != true {
        return Err(v["error"].as_str().unwrap_or("Could not read the calendar.").into());
    }
    let mut events: Vec<CalEvent> = serde_json::from_value(v["events"].clone()).map_err(|e| e.to_string())?;
    for event in &mut events {
        if event.recurrence.is_empty() && !event.rrule.is_empty() {
            let (kind, days, until) = recurrence::from_rrule(&event.rrule);
            event.recurrence = kind;
            event.weekdays = days;
            event.until_ms = until;
        }
    }
    Ok((events, v["truncated"].as_bool().unwrap_or(false)))
}

fn local_in_range(events: &[CalEvent], start: i64, end: i64) -> Vec<CalEvent> {
    let mut out = Vec::new();
    for event in events {
        if recurrence::is_repeating(&event.recurrence) {
            out.extend(expand_event(event, start, end));
        } else if event.start_ms < end && event.end_ms > start {
            out.push(event.clone());
        }
    }
    out
}

fn expand_event(event: &CalEvent, start: i64, end: i64) -> Vec<CalEvent> {
    let span = (event.end_ms - event.start_ms).max(60_000);
    let mut cursor = if event.end_ms <= start {
        next_event_start(event, start.saturating_sub(span)).unwrap_or(event.start_ms)
    } else {
        event.start_ms
    };
    let mut out = Vec::new();
    for _ in 0..400 {
        if cursor >= end {
            break;
        }
        if event.until_ms > 0 && cursor > event.until_ms {
            break;
        }
        let instance_end = cursor + span;
        if instance_end > start && cursor < end {
            let mut copy = event.clone();
            copy.start_ms = cursor;
            copy.end_ms = instance_end;
            out.push(copy);
        }
        let Some(next) = next_event_start(event, cursor) else { break };
        if next <= cursor {
            break;
        }
        cursor = next;
    }
    out
}

fn next_event_start(event: &CalEvent, after_ms: i64) -> Option<i64> {
    let after = DateTime::from_timestamp_millis(after_ms)?.with_timezone(&Local);
    let spec = event_spec(event)?;
    Some(recurrence::next_after(&spec, after)?.timestamp_millis())
}

fn event_spec(event: &CalEvent) -> Option<recurrence::Spec> {
    let start = DateTime::from_timestamp_millis(event.start_ms)?.with_timezone(&Local);
    let until = (event.until_ms > 0)
        .then(|| DateTime::from_timestamp_millis(event.until_ms).map(|when| when.with_timezone(&Local).date_naive()))
        .flatten();
    let mut weekdays = event.weekdays.clone();
    if weekdays.is_empty() && recurrence::normalize_kind(&event.recurrence) == "weekly" {
        weekdays.push(start.weekday().number_from_monday() as u8);
    }
    Some(recurrence::Spec {
        kind: event.recurrence.clone(),
        weekdays,
        hour: start.hour(),
        minute: start.minute(),
        month: start.month(),
        day: start.day(),
        year: Some(start.year()),
        lead_days: 0,
        until,
    })
}

fn touch(app: &AppHandle) {
    if let Some(hub) = app.try_state::<Arc<Hub>>() {
        hub.emit("voice://agenda", json!({}));
    }
    if let Some(bridge) = app.try_state::<NativeBridge>() {
        bridge.refresh_widgets();
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
        Some(d) => parse_day(d)?,
        None => Local::now().date_naive(),
    };
    let start = start_of_local_day(date)?;
    let end = start + Duration::days(1);
    Ok((start.timestamp_millis(), end.timestamp_millis(), date.format("%Y-%m-%d").to_string()))
}

const LIST_CAP: usize = 400;
const NOTE_CAP: usize = 40;
const NOTE_TEXT_CAP: usize = 1200;
const NOTE_FULL_CAP: usize = 8000;
const MAX_RANGE_DAYS: i64 = 400;

struct ListWindow {
    start_ms: i64,
    end_ms: i64,
    label: String,
    series: bool,
}

fn list_calendar_events(app: &AppHandle, agenda: &Agenda, args: &Value) -> Result<Value, String> {
    let now = Local::now();
    let window = list_window(args, now)?;
    let recurring_only = flag(args, "recurring");
    let (mut events, phone_truncated) = if window.series {
        let start = start_of_local_day(now.date_naive())?;
        let end = start + Duration::days(366);
        let (phone, truncated) = phone_events(app, start.timestamp_millis(), end.timestamp_millis())?;
        let local = agenda.lock().events.clone();
        (series_events(&local, &phone), truncated)
    } else {
        let (phone, truncated) = phone_events(app, window.start_ms, window.end_ms)?;
        let local = agenda.lock().events.clone();
        (collect_calendar(&phone, &local, window.start_ms, window.end_ms, recurring_only), truncated)
    };
    if let Some(query) = arg_text(args, "query") {
        let query = query.to_ascii_lowercase();
        events.retain(|event| {
            event.title.to_ascii_lowercase().contains(&query) || event.location.to_ascii_lowercase().contains(&query)
        });
    }
    events.sort_by(|a, b| a.start_ms.cmp(&b.start_ms).then_with(|| a.title.cmp(&b.title)).then_with(|| a.id.cmp(&b.id)));
    Ok(events_payload(&events, &window.label, phone_truncated, window.series))
}

fn phone_events(app: &AppHandle, start: i64, end: i64) -> Result<(Vec<CalEvent>, bool), String> {
    match ensure_calendar(app, start, end) {
        Ok(read) => Ok(read),
        Err(err) if calendar_unavailable(&err) => Ok((Vec::new(), false)),
        Err(err) => Err(err),
    }
}

fn calendar_unavailable(err: &str) -> bool {
    let err = err.to_ascii_lowercase();
    err.contains("denied") || err.contains("permission") || err.contains("only on the android")
}

fn collect_calendar(phone: &[CalEvent], local: &[CalEvent], start: i64, end: i64, recurring_only: bool) -> Vec<CalEvent> {
    let mut events = phone.to_vec();
    events.extend(local_in_range(local, start, end));
    if recurring_only {
        events.retain(|event| recurrence::is_repeating(&event.recurrence));
    }
    events
}

fn series_events(local: &[CalEvent], phone: &[CalEvent]) -> Vec<CalEvent> {
    let mut out: Vec<CalEvent> = local.iter().filter(|event| recurrence::is_repeating(&event.recurrence)).cloned().collect();
    let mut seen = std::collections::HashSet::new();
    for event in phone {
        if !recurrence::is_repeating(&event.recurrence) || !seen.insert(event.id.clone()) {
            continue;
        }
        out.push(event.clone());
    }
    out
}

fn events_payload(events: &[CalEvent], label: &str, phone_truncated: bool, series: bool) -> Value {
    let total = events.len();
    let truncated = phone_truncated || total > LIST_CAP;
    let shown: Vec<&CalEvent> = events.iter().take(LIST_CAP).collect();
    let multi = !series && label.contains("through");
    let repeating = shown.iter().filter(|event| recurrence::is_repeating(&event.recurrence)).count();
    let rows: Vec<Value> = shown.iter().map(|event| event_row(event, multi || series)).collect();
    json!({
        "ok": true,
        "range": label,
        "count": rows.len(),
        "total": total,
        "truncated": truncated,
        "events": rows,
        "detail": event_detail(rows.len(), total, label, truncated, phone_truncated, repeating)
    })
}

fn event_row(event: &CalEvent, with_date: bool) -> Value {
    let date = date_of(event.start_ms);
    let clock = if event.all_day { "all day".to_string() } else { format_span(event.start_ms, event.end_ms) };
    let when = if with_date && !date.is_empty() {
        if event.all_day { format!("{date} all day") } else { format!("{date} {clock}") }
    } else {
        clock
    };
    json!({
        "id": event.id,
        "title": event.title,
        "source": event.source,
        "date": date,
        "start": local_stamp(event.start_ms),
        "end": local_stamp(event.end_ms),
        "when": when,
        "location": event.location,
        "allDay": event.all_day,
        "recurrence": event.recurrence,
    })
}

fn event_detail(shown: usize, total: usize, label: &str, truncated: bool, phone_truncated: bool, repeating: usize) -> String {
    if total == 0 {
        return format!("No calendar events for {label}.");
    }
    let mut detail = if truncated && shown < total {
        format!("Showing {shown} of {total} events for {label}.")
    } else {
        format!("{shown} events for {label}.")
    };
    if repeating > 0 {
        detail.push_str(&format!(" {repeating} of them repeat."));
    }
    if phone_truncated {
        detail.push_str(" The phone calendar returned a partial list.");
    }
    if truncated {
        detail.push_str(" Ask for a shorter range for the rest.");
    }
    detail
}

fn reminders_payload(items: &[Item], args: &Value, now: DateTime<Local>) -> Result<Value, String> {
    let query = arg_text(args, "query").unwrap_or("").to_ascii_lowercase();
    let recurring_only = flag(args, "recurring");
    let bounded = has_range(args);
    let (rows, label) = if bounded {
        let window = list_window(args, now)?;
        if window.series {
            (upcoming_reminders(items, now, &query, true), "repeating reminders".into())
        } else {
            (ranged_reminders(items, window.start_ms, window.end_ms, now, &query, recurring_only), window.label)
        }
    } else {
        let label = if recurring_only { "repeating reminders" } else { "saved reminders" };
        (upcoming_reminders(items, now, &query, recurring_only), label.into())
    };
    let total = rows.len();
    let truncated = total > LIST_CAP;
    let shown: Vec<Value> = rows.into_iter().take(LIST_CAP).collect();
    let count = shown.len();
    let detail = if count == 0 {
        format!("Nothing is scheduled for {label}.")
    } else if truncated {
        format!("Showing {count} of {total} for {label}. Ask for a shorter range for the rest.")
    } else {
        format!("{count} scheduled for {label}.")
    };
    Ok(json!({
        "ok": true,
        "range": label,
        "count": count,
        "total": total,
        "truncated": truncated,
        "reminders": shown,
        "detail": detail
    }))
}

fn upcoming_reminders(items: &[Item], now: DateTime<Local>, query: &str, recurring_only: bool) -> Vec<Value> {
    let mut rows: Vec<Value> = items
        .iter()
        .filter(|item| shows_on_time_list(item, now))
        .filter(|item| !recurring_only || recurrence::is_repeating(&item.recurrence))
        .filter(|item| text_hit(&item.title, &item.note, query))
        .map(|item| reminder_row(item, item.at_ms))
        .collect();
    rows.sort_by_key(|row| row["at"].as_str().unwrap_or("").to_string());
    rows
}

fn ranged_reminders(
    items: &[Item],
    start: i64,
    end: i64,
    now: DateTime<Local>,
    query: &str,
    recurring_only: bool,
) -> Vec<Value> {
    let mut rows = Vec::new();
    for item in items {
        if !text_hit(&item.title, &item.note, query) {
            continue;
        }
        if recurring_only && !recurrence::is_repeating(&item.recurrence) {
            continue;
        }
        if is_person_date(item) {
            if shows_on_time_list(item, now) && person_window_overlaps(item, start, end) {
                rows.push(reminder_row(item, item.at_ms));
            }
            continue;
        }
        if recurrence::is_repeating(&item.recurrence) {
            for at_ms in expand_item(item, start, end) {
                rows.push(reminder_row(item, at_ms));
            }
        } else if item.at_ms >= start && item.at_ms < end {
            rows.push(reminder_row(item, item.at_ms));
        }
    }
    rows.sort_by_key(|row| row["at"].as_str().unwrap_or("").to_string());
    rows
}

fn reminder_row(item: &Item, at_ms: i64) -> Value {
    let at = DateTime::from_timestamp_millis(at_ms)
        .map(|when| when.with_timezone(&Local).format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_default();
    json!({
        "id": item.id,
        "kind": item.kind,
        "title": item.title,
        "note": item.note,
        "at": at,
        "recurrence": item.recurrence,
        "weekdays": item.weekdays,
        "time": item.time,
        "date": item.date,
        "leadDays": item.lead_days,
        "personId": item.person_id,
    })
}

fn expand_item(item: &Item, start: i64, end: i64) -> Vec<i64> {
    let Ok(mut spec) = spec_from_item(item) else { return Vec::new() };
    if spec.weekdays.is_empty() && recurrence::normalize_kind(&spec.kind) == "weekly" {
        if let Some(at) = DateTime::from_timestamp_millis(item.at_ms).map(|when| when.with_timezone(&Local)) {
            spec.weekdays.push(at.weekday().number_from_monday() as u8);
        }
    }
    let mut cursor = start.saturating_sub(60_000);
    let mut out = Vec::new();
    for _ in 0..LIST_CAP {
        let Some(after) = DateTime::from_timestamp_millis(cursor).map(|when| when.with_timezone(&Local)) else { break };
        let Some(next) = recurrence::next_after(&spec, after) else { break };
        let ms = next.timestamp_millis();
        if ms >= end || (item.until_ms > 0 && ms > item.until_ms) {
            break;
        }
        if ms <= cursor {
            break;
        }
        if ms >= start {
            out.push(ms);
        }
        cursor = ms;
    }
    out
}

fn person_window_overlaps(item: &Item, start: i64, end: i64) -> bool {
    let Some(at) = DateTime::from_timestamp_millis(item.at_ms).map(|when| when.with_timezone(&Local)) else {
        return false;
    };
    let notify = at.date_naive();
    let event = notify + Duration::days(i64::from(item.lead_days));
    let Ok(win_start) = start_of_local_day(notify) else { return false };
    let Ok(win_end) = start_of_local_day(event + Duration::days(1)) else { return false };
    win_start.timestamp_millis() < end && win_end.timestamp_millis() > start
}

fn notes_payload(notes: &[Note], args: &Value) -> Value {
    let query = arg_text(args, "query").unwrap_or("").to_ascii_lowercase();
    let id = arg_text(args, "id").unwrap_or("");
    let mut matched: Vec<&Note> = notes
        .iter()
        .filter(|note| {
            if !id.is_empty() {
                return note.id == id;
            }
            if query.is_empty() {
                return true;
            }
            note_title(&note.title, &note.text).to_ascii_lowercase().contains(&query) || note.text.to_ascii_lowercase().contains(&query)
        })
        .collect();
    matched.sort_by_key(|note| std::cmp::Reverse(note.created_ms));
    let total = matched.len();
    let truncated = id.is_empty() && total > NOTE_CAP;
    let cap = if id.is_empty() { NOTE_TEXT_CAP } else { NOTE_FULL_CAP };
    let rows: Vec<Value> = matched
        .into_iter()
        .take(if id.is_empty() { NOTE_CAP } else { 1 })
        .map(|note| {
            let chars = note.text.chars().count();
            json!({
                "id": note.id,
                "title": note_title(&note.title, &note.text),
                "text": clip_text(&note.text, cap),
                "clipped": chars > cap,
                "createdMs": note.created_ms,
            })
        })
        .collect();
    let count = rows.len();
    let clipped = rows.iter().any(|row| row["clipped"] == true);
    let detail = if count == 0 {
        "There are no saved notes.".to_string()
    } else if truncated {
        format!("Showing {count} of {total} saved notes. Pass query to narrow them.")
    } else if clipped {
        format!("{count} saved notes. Some text was shortened. Pass id for one note.")
    } else if id.is_empty() {
        format!("{count} saved notes.")
    } else {
        format!("Note {}.", rows.first().and_then(|row| row["title"].as_str()).unwrap_or("saved"))
    };
    json!({
        "ok": true,
        "count": count,
        "total": total,
        "truncated": truncated,
        "notes": rows,
        "detail": detail
    })
}

fn list_window(args: &Value, now: DateTime<Local>) -> Result<ListWindow, String> {
    let recurring = flag(args, "recurring");
    if recurring && !has_range(args) {
        return Ok(ListWindow { start_ms: 0, end_ms: 0, label: "repeating series".into(), series: true });
    }
    if let Some(month) = arg_text(args, "month") {
        let (start, end, label) = month_window(month)?;
        return Ok(ListWindow { start_ms: start, end_ms: end, label, series: false });
    }
    if arg_text(args, "from").is_some() || arg_text(args, "to").is_some() {
        return from_to_window(args, now);
    }
    if let Some(span) = arg_text(args, "span") {
        return span_window(span, arg_text(args, "day"), now);
    }
    let (start, end, day) = day_bounds(arg_text(args, "day"))?;
    Ok(ListWindow { start_ms: start, end_ms: end, label: day, series: false })
}

fn has_range(args: &Value) -> bool {
    arg_text(args, "from").is_some()
        || arg_text(args, "to").is_some()
        || arg_text(args, "month").is_some()
        || arg_text(args, "span").is_some()
        || arg_text(args, "day").is_some()
}

fn month_window(value: &str) -> Result<(i64, i64, String), String> {
    let (year, month) = parse_month(value)?;
    let start = start_of_local_day(NaiveDate::from_ymd_opt(year, month, 1).ok_or_else(|| format!("bad month '{value}'."))?)?;
    let (next_year, next_month) = if month == 12 { (year + 1, 1) } else { (year, month + 1) };
    let end = start_of_local_day(NaiveDate::from_ymd_opt(next_year, next_month, 1).ok_or_else(|| "bad month".to_string())?)?;
    check_span(start, end)?;
    let label = range_label(start, end);
    Ok((start.timestamp_millis(), end.timestamp_millis(), label))
}

fn from_to_window(args: &Value, now: DateTime<Local>) -> Result<ListWindow, String> {
    let from = match arg_text(args, "from") {
        Some(day) => parse_day(day)?,
        None => now.date_naive(),
    };
    let to = match arg_text(args, "to") {
        Some(day) => parse_day(day)?,
        None => {
            let (year, month) = (from.year(), from.month());
            let (next_year, next_month) = if month == 12 { (year + 1, 1) } else { (year, month + 1) };
            NaiveDate::from_ymd_opt(next_year, next_month, 1).ok_or_else(|| "bad month".to_string())? - Duration::days(1)
        }
    };
    let start = start_of_local_day(from)?;
    let end = start_of_local_day(to)? + Duration::days(1);
    check_span(start, end)?;
    Ok(ListWindow { start_ms: start.timestamp_millis(), end_ms: end.timestamp_millis(), label: range_label(start, end), series: false })
}

fn span_window(span: &str, day: Option<&str>, now: DateTime<Local>) -> Result<ListWindow, String> {
    let today = match day {
        Some(day) => parse_day(day)?,
        None => now.date_naive(),
    };
    match span.trim().to_ascii_lowercase().as_str() {
        "day" | "today" => {
            let label_day = today.format("%Y-%m-%d").to_string();
            let (start, end, label) = day_bounds(Some(&label_day))?;
            Ok(ListWindow { start_ms: start, end_ms: end, label, series: false })
        }
        "week" => {
            let start = start_of_local_day(today)?;
            let end = start + Duration::days(7);
            Ok(ListWindow { start_ms: start.timestamp_millis(), end_ms: end.timestamp_millis(), label: range_label(start, end), series: false })
        }
        "month" => {
            let (start, end, label) = month_window(&format!("{:04}-{:02}", today.year(), today.month()))?;
            Ok(ListWindow { start_ms: start, end_ms: end, label, series: false })
        }
        "upcoming" => {
            let start = start_of_local_day(today)?;
            let end = start + Duration::days(31);
            Ok(ListWindow { start_ms: start.timestamp_millis(), end_ms: end.timestamp_millis(), label: range_label(start, end), series: false })
        }
        _ => Err("span is day, week, month, or upcoming.".into()),
    }
}

fn parse_month(value: &str) -> Result<(i32, u32), String> {
    let value = value.trim();
    let ym = if value.len() >= 7 { &value[..7] } else { value };
    let date = NaiveDate::parse_from_str(&format!("{ym}-01"), "%Y-%m-%d")
        .map_err(|_| format!("bad month '{value}'. Use YYYY-MM."))?;
    Ok((date.year(), date.month()))
}

fn parse_day(value: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d").map_err(|_| format!("bad day '{value}'. Use YYYY-MM-DD."))
}

fn start_of_local_day(date: NaiveDate) -> Result<DateTime<Local>, String> {
    let start_naive = date.and_hms_opt(0, 0, 0).ok_or_else(|| "bad day".to_string())?;
    start_naive
        .and_local_timezone(Local)
        .earliest()
        .or_else(|| start_naive.and_local_timezone(Local).latest())
        .ok_or_else(|| "bad day".to_string())
}

fn check_span(start: DateTime<Local>, end: DateTime<Local>) -> Result<(), String> {
    if end <= start {
        return Err("The end date is before the start.".into());
    }
    if (end - start).num_days() > MAX_RANGE_DAYS {
        return Err(format!("Ask for {MAX_RANGE_DAYS} days or fewer."));
    }
    Ok(())
}

fn range_label(start: DateTime<Local>, end_exclusive: DateTime<Local>) -> String {
    let first = start.date_naive();
    let last = (end_exclusive - Duration::days(1)).date_naive();
    if first == last {
        first.format("%Y-%m-%d").to_string()
    } else {
        format!("{} through {}", first.format("%Y-%m-%d"), last.format("%Y-%m-%d"))
    }
}

fn arg_text<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(|value| value.as_str()).map(str::trim).filter(|text| !text.is_empty())
}

fn flag(args: &Value, key: &str) -> bool {
    match args.get(key) {
        Some(Value::Bool(value)) => *value,
        Some(Value::String(value)) => matches!(value.trim().to_ascii_lowercase().as_str(), "true" | "yes" | "1"),
        Some(Value::Number(value)) => value.as_i64() == Some(1),
        _ => false,
    }
}

fn text_hit(title: &str, note: &str, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    format!("{title} {note}").to_ascii_lowercase().contains(query)
}

fn clip_text(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max).collect();
    out.push('…');
    out
}

fn date_of(ms: i64) -> String {
    DateTime::from_timestamp_millis(ms)
        .map(|when| when.with_timezone(&Local).format("%Y-%m-%d").to_string())
        .unwrap_or_default()
}

fn local_stamp(ms: i64) -> String {
    DateTime::from_timestamp_millis(ms)
        .map(|when| when.with_timezone(&Local).format("%Y-%m-%dT%H:%M").to_string())
        .unwrap_or_default()
}

fn apply_schedule(item: &mut Item, meta: &ScheduleMeta, requested_at: i64) -> Result<(), String> {
    if !meta.recurrence.is_empty() {
        let kind = recurrence::normalize_kind(&meta.recurrence);
        item.recurrence = if kind == "none" { String::new() } else { kind };
    }
    if !meta.weekdays.is_empty() {
        item.weekdays = meta.weekdays.clone();
    }
    if !meta.time.is_empty() {
        item.time = meta.time.clone();
    }
    if !meta.date.is_empty() {
        item.date = meta.date.clone();
    }
    if !meta.person_id.is_empty() {
        item.person_id = meta.person_id.clone();
        item.person_reminder_id = meta.person_reminder_id.clone();
    }
    if meta.lead_days > 0 || !meta.date.is_empty() {
        item.lead_days = meta.lead_days.min(30);
    }
    if let Some(until) = meta.until_ms {
        item.until_ms = until.max(0);
    }
    if item.kind == "alarm" || item.kind == "timer" {
        item.recurrence.clear();
        item.at_ms = requested_at;
        return Ok(());
    }
    let kind = recurrence::normalize_kind(&item.recurrence);
    if recurrence::is_repeating(&kind) || kind == "once" {
        item.recurrence = kind;
        let spec = spec_from_item(item)?;
        let next = recurrence::next_after(&spec, Local::now()).ok_or_else(|| "That date has no upcoming time.".to_string())?;
        item.at_ms = next.timestamp_millis();
        if item.time.is_empty() {
            item.time = format!("{:02}:{:02}", spec.hour, spec.minute);
        }
        return Ok(());
    }
    item.recurrence.clear();
    item.at_ms = requested_at;
    Ok(())
}

fn spec_from_item(item: &Item) -> Result<recurrence::Spec, String> {
    let fallback = DateTime::from_timestamp_millis(item.at_ms).map(|t| t.with_timezone(&Local));
    let (hour, minute) = recurrence::parse_clock(&item.time)
        .or_else(|| fallback.map(|t| (t.hour(), t.minute())))
        .unwrap_or((9, 0));
    let (year, month, day) = if let Some(parsed) = recurrence::parse_month_day(&item.date) {
        parsed
    } else if let Some(at) = fallback {
        (Some(at.year()), at.month(), at.day())
    } else {
        (None, 0, 0)
    };
    let kind = if item.recurrence.is_empty() { "none".into() } else { item.recurrence.clone() };
    let until = (item.until_ms > 0).then(|| DateTime::from_timestamp_millis(item.until_ms).map(|t| t.with_timezone(&Local).date_naive())).flatten();
    Ok(recurrence::Spec { kind, weekdays: item.weekdays.clone(), hour, minute, month, day, year, lead_days: item.lead_days, until })
}

fn schedule_from_args(args: &Value) -> Result<ScheduleMeta, String> {
    let raw = args["recurrence"].as_str().unwrap_or("").trim();
    let mut recurrence = if raw.is_empty() { String::new() } else { recurrence::normalize_kind(raw) };
    let weekdays = weekday_args(args);
    if recurrence.is_empty() && !weekdays.is_empty() {
        recurrence = "weekly".into();
    }
    Ok(ScheduleMeta {
        recurrence,
        weekdays,
        time: args["time"].as_str().unwrap_or("").trim().to_string(),
        date: args["date"].as_str().unwrap_or("").trim().to_string(),
        lead_days: args["leadDays"].as_u64().unwrap_or(0) as u32,
        until_ms: args.get("until").and_then(|value| value.as_str()).map(str::trim).filter(|value| !value.is_empty()).and_then(|value| recurrence::parse_month_day(value).map(|(_, _, _)| value.to_string())).map(|value| until_end_ms(&value)).or_else(|| args.get("untilMs").and_then(|value| value.as_i64())),
        ..ScheduleMeta::default()
    })
}

fn until_end_ms(value: &str) -> i64 {
    let Some((year, month, day)) = recurrence::parse_month_day(value) else { return 0 };
    let Some(year) = year else { return 0 };
    let Some(date) = chrono::NaiveDate::from_ymd_opt(year, month, day) else { return 0 };
    let Some(naive) = date.and_hms_opt(23, 59, 0) else { return 0 };
    naive.and_local_timezone(Local).earliest().map(|when| when.timestamp_millis()).unwrap_or(0)
}

fn weekday_args(args: &Value) -> Vec<u8> {
    let mut raw = Vec::new();
    match &args["weekdays"] {
        Value::Array(rows) => {
            for row in rows {
                if let Some(text) = row.as_str() {
                    raw.push(text.to_string());
                } else if let Some(n) = row.as_u64() {
                    raw.push(n.to_string());
                }
            }
        }
        Value::String(text) => raw.push(text.clone()),
        _ => {}
    }
    recurrence::parse_weekdays(&raw)
}

fn reminder_anchor_ms(args: &Value, schedule: &ScheduleMeta) -> Result<i64, String> {
    let kind = recurrence::normalize_kind(&schedule.recurrence);
    let repeating = recurrence::is_repeating(&kind) || kind == "once" || !schedule.time.is_empty();
    if !repeating {
        if let Some(offset) = relative_delay_ms(args)? {
            return Ok(Local::now().timestamp_millis().saturating_add(offset));
        }
    }
    let at = args["at"].as_str().unwrap_or("").trim();
    if !at.is_empty() {
        return Ok(parse_flexible_when(at)?.timestamp_millis());
    }
    if repeating {
        if schedule.time.is_empty() && kind != "once" {
            return Err("A repeating reminder needs a time like 08:00.".into());
        }
        return Ok(Local::now().timestamp_millis() + 120_000);
    }
    Err("A one-off reminder needs inMinutes, inHours, or a time such as 7pm, tomorrow at 9, or YYYY-MM-DDTHH:MM.".into())
}

fn request_title(args: &Value) -> String {
    for key in ["title", "label"] {
        let value = args[key].as_str().unwrap_or("").trim();
        if !value.is_empty() {
            return value.chars().take(120).collect();
        }
    }
    String::new()
}

fn timer_label(args: &Value) -> String {
    for key in ["label", "title"] {
        let value = args[key].as_str().unwrap_or("").trim();
        if !value.is_empty() {
            return value.chars().take(120).collect();
        }
    }
    "Timer".into()
}

fn timer_seconds(args: &Value) -> Result<u32, String> {
    if let Some(seconds) = arg_u32(args, "seconds").filter(|n| *n > 0) {
        return Ok(seconds);
    }
    if let Some(minutes) = arg_u32(args, "inMinutes").or_else(|| arg_u32(args, "minutes")).filter(|n| *n > 0) {
        return minutes.checked_mul(60).ok_or_else(|| "A timer has to be between 1 second and 24 hours.".into());
    }
    if let Some(hours) = arg_u32(args, "inHours").or_else(|| arg_u32(args, "hours")).filter(|n| *n > 0) {
        return hours.checked_mul(3600).ok_or_else(|| "A timer has to be between 1 second and 24 hours.".into());
    }
    Err("seconds is required, such as 300 for five minutes.".into())
}

fn relative_delay_ms(args: &Value) -> Result<Option<i64>, String> {
    let minutes = arg_u32(args, "inMinutes").or_else(|| arg_u32(args, "minutes"));
    let hours = arg_u32(args, "inHours").or_else(|| arg_u32(args, "hours"));
    let seconds = arg_u32(args, "seconds").or_else(|| arg_u32(args, "inSeconds"));
    if minutes.is_some() || hours.is_some() || seconds.is_some() {
        let total = i64::from(seconds.unwrap_or(0)) + i64::from(minutes.unwrap_or(0)) * 60 + i64::from(hours.unwrap_or(0)) * 3600;
        if total < 1 {
            return Err("Say how many minutes or hours from now.".into());
        }
        if total > 366 * 24 * 3600 {
            return Err("That delay is too far away.".into());
        }
        return Ok(Some(total * 1000));
    }
    let phrase = args["in"].as_str().or_else(|| args["delay"].as_str()).unwrap_or("").trim();
    if phrase.is_empty() {
        return Ok(None);
    }
    relative_phrase_ms(phrase).map(Some).ok_or_else(|| format!("could not read '{phrase}'. Use inMinutes or inHours."))
}

pub fn parse_flexible_when(s: &str) -> Result<DateTime<Local>, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("Give a time as inMinutes, 7pm, tomorrow at 9, or YYYY-MM-DDTHH:MM.".into());
    }
    if let Ok(when) = parse_local_when(s) {
        return Ok(when);
    }
    if let Some(offset) = relative_phrase_ms(s) {
        return Ok(Local::now() + Duration::milliseconds(offset));
    }
    if let Some(when) = parse_clock_phrase(s) {
        return Ok(when);
    }
    Err(format!("could not read '{s}'. Use inMinutes, 7pm, tomorrow at 9, or YYYY-MM-DDTHH:MM."))
}

fn relative_phrase_ms(raw: &str) -> Option<i64> {
    let mut folded = raw.trim().trim_matches(|c: char| matches!(c, '.' | '!' | '?' | ',')).to_ascii_lowercase();
    if let Some(rest) = folded.strip_prefix("in ") {
        folded = rest.trim().to_string();
    } else if let Some(rest) = folded.strip_prefix("after ") {
        folded = rest.trim().to_string();
    }
    if let Some(rest) = folded.strip_suffix(" from now") {
        folded = rest.trim().to_string();
    }
    let mut parts = folded.split_whitespace();
    let first = parts.next()?;
    let second = parts.next();
    if parts.next().is_some() {
        return None;
    }
    let (count, unit) = if let Some(unit) = second {
        let count = if first == "a" || first == "an" { 1 } else { first.parse::<u32>().ok()? };
        (count, unit)
    } else {
        return compact_duration_ms(first);
    };
    unit_to_ms(count, unit)
}

fn compact_duration_ms(token: &str) -> Option<i64> {
    let split = token.find(|c: char| !c.is_ascii_digit())?;
    if split == 0 {
        return None;
    }
    let count: u32 = token[..split].parse().ok()?;
    unit_to_ms(count, &token[split..])
}

fn unit_to_ms(count: u32, unit: &str) -> Option<i64> {
    if count == 0 {
        return None;
    }
    let unit = unit.trim().trim_matches('.');
    let seconds = match unit {
        "s" | "sec" | "secs" | "second" | "seconds" => i64::from(count),
        "m" | "min" | "mins" | "minute" | "minutes" => i64::from(count) * 60,
        "h" | "hr" | "hrs" | "hour" | "hours" => i64::from(count) * 3600,
        "d" | "day" | "days" => i64::from(count) * 86_400,
        _ => return None,
    };
    if seconds > 366 * 24 * 3600 {
        return None;
    }
    Some(seconds * 1000)
}

fn parse_clock_phrase(raw: &str) -> Option<DateTime<Local>> {
    let folded = raw.trim().trim_matches(|c: char| matches!(c, '.' | '!' | '?' | ',')).to_ascii_lowercase();
    let mut rest = folded.as_str();
    let mut day_offset = 0i64;
    let mut forced_day = false;
    if let Some(after) = rest.strip_prefix("tomorrow") {
        day_offset = 1;
        forced_day = true;
        rest = after.trim();
    } else if let Some(after) = rest.strip_prefix("today") {
        forced_day = true;
        rest = after.trim();
    }
    rest = rest.trim_start_matches("at").trim();
    if rest.is_empty() {
        return None;
    }
    let (hour, minute) = parse_hm_token(rest)?;
    let now = Local::now();
    let date = now.date_naive() + Duration::days(day_offset);
    let when = local_hms(date, hour, minute)?;
    if !forced_day && when <= now + Duration::seconds(45) {
        return local_hms(date + Duration::days(1), hour, minute);
    }
    if when <= now {
        return None;
    }
    Some(when)
}

fn parse_hm_token(raw: &str) -> Option<(u32, u32)> {
    let compact: String = raw.chars().filter(|c| !c.is_whitespace()).collect();
    let lower = compact.trim_matches(|c: char| matches!(c, '.' | '!' | '?' | ',')).to_ascii_lowercase();
    if lower.is_empty() {
        return None;
    }
    let (body, meridiem) = if let Some(body) = lower.strip_suffix("a.m.") {
        (body, Some(false))
    } else if let Some(body) = lower.strip_suffix("p.m.") {
        (body, Some(true))
    } else if let Some(body) = lower.strip_suffix("am") {
        (body, Some(false))
    } else if let Some(body) = lower.strip_suffix("pm") {
        (body, Some(true))
    } else {
        (lower.as_str(), None)
    };
    if body.is_empty() || body.chars().any(|c| !c.is_ascii_digit() && c != ':') {
        return None;
    }
    let (hour, minute) = if let Some((h, m)) = body.split_once(':') {
        if m.len() != 2 {
            return None;
        }
        (h.parse::<u32>().ok()?, m.parse::<u32>().ok()?)
    } else {
        (body.parse::<u32>().ok()?, 0)
    };
    if minute > 59 {
        return None;
    }
    let hour = match meridiem {
        Some(is_pm) => {
            if !(1..=12).contains(&hour) {
                return None;
            }
            match (hour, is_pm) {
                (12, false) => 0,
                (12, true) => 12,
                (h, true) => h + 12,
                (h, false) => h,
            }
        }
        None => {
            if hour > 23 {
                return None;
            }
            hour
        }
    };
    Some((hour, minute))
}

fn local_hms(date: NaiveDate, hour: u32, minute: u32) -> Option<DateTime<Local>> {
    let naive = date.and_hms_opt(hour, minute, 0)?;
    naive.and_local_timezone(Local).earliest().or_else(|| naive.and_local_timezone(Local).latest())
}

fn event_alarm_at(event: &CalEvent, now: DateTime<Local>) -> Option<i64> {
    let now_ms = now.timestamp_millis();
    if event.start_ms > now_ms {
        return Some(event.start_ms);
    }
    if !recurrence::is_repeating(&event.recurrence) {
        return None;
    }
    let spec = event_spec(event)?;
    let next = recurrence::next_after(&spec, now)?.timestamp_millis();
    if event.until_ms > 0 && next > event.until_ms {
        return None;
    }
    (next > now_ms).then_some(next)
}

fn arm_local_event(app: &AppHandle, event: &CalEvent) {
    if event.alarm_id <= 0 {
        return;
    }
    let bridge = app.state::<NativeBridge>();
    match event_alarm_at(event, Local::now()) {
        Some(at) => {
            let _ = bridge.schedule_item(event.alarm_id, "event", &event.title, &event.location, at);
        }
        None => {
            let _ = bridge.cancel_item(event.alarm_id);
        }
    }
}

fn rearm_events(app: &AppHandle) {
    let agenda = app.state::<Agenda>();
    let mut store = agenda.lock();
    let mut changed = false;
    for i in 0..store.events.len() {
        if store.events[i].alarm_id <= 0 {
            let id = Agenda::alloc_id(&mut store);
            store.events[i].alarm_id = id;
            changed = true;
        }
    }
    let events = store.events.clone();
    if changed {
        let _ = Agenda::persist(&agenda.path, &store);
    }
    drop(store);
    for event in events {
        arm_local_event(app, &event);
    }
}

fn recurrence_phrase(item: &Item) -> String {
    match recurrence::normalize_kind(&item.recurrence).as_str() {
        "daily" => format!(" Repeats daily at {}.", display_clock(&item.time)),
        "weekly" => {
            let days = item.weekdays.iter().map(|d| recurrence::weekday_label(*d)).filter(|s| !s.is_empty()).collect::<Vec<_>>();
            if days.is_empty() {
                format!(" Repeats weekly at {}.", display_clock(&item.time))
            } else {
                format!(" Repeats {} at {}.", days.join(" "), display_clock(&item.time))
            }
        }
        "monthly" => " Repeats every month.".into(),
        "yearly" => " Repeats every year.".into(),
        _ => String::new(),
    }
}

fn display_clock(time: &str) -> String {
    if time.is_empty() { "that time".into() } else { time.into() }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FireRef {
    id: i32,
    title: String,
    at_ms: i64,
    recurrence: String,
}

#[derive(Debug)]
enum SnoozeChoice {
    One(FireRef),
    Ask(Vec<FireRef>),
    None,
}

fn fire_from(item: &Item) -> FireRef {
    FireRef { id: item.id, title: item.title.clone(), at_ms: item.at_ms, recurrence: item.recurrence.clone() }
}

fn choose_snooze(items: &[Item], recent: Option<&FireRef>, now_ms: i64, explicit: Option<i32>) -> SnoozeChoice {
    if let Some(id) = explicit {
        if let Some(item) = items.iter().find(|i| i.id == id) {
            return SnoozeChoice::One(fire_from(item));
        }
        if let Some(recent) = recent.filter(|r| r.id == id) {
            return SnoozeChoice::One(recent.clone());
        }
        return SnoozeChoice::None;
    }
    let start = now_ms - 2 * 60 * 60 * 1000;
    let end = now_ms + 90_000;
    let mut due: Vec<&Item> = items.iter().filter(|i| i.kind != "alarm" && i.at_ms >= start && i.at_ms <= end).collect();
    due.sort_by(|a, b| b.at_ms.cmp(&a.at_ms));
    if due.len() == 1 {
        return SnoozeChoice::One(fire_from(due[0]));
    }
    if due.len() > 1 {
        if due[0].at_ms - due[1].at_ms > 30 * 60 * 1000 {
            return SnoozeChoice::One(fire_from(due[0]));
        }
        return SnoozeChoice::Ask(due.into_iter().take(6).map(fire_from).collect());
    }
    if let Some(recent) = recent {
        if now_ms >= recent.at_ms && now_ms - recent.at_ms <= 2 * 60 * 60 * 1000 {
            return SnoozeChoice::One(recent.clone());
        }
    }
    let upcoming: Vec<FireRef> = items.iter().filter(|i| i.at_ms > now_ms).take(6).map(fire_from).collect();
    if upcoming.is_empty() { SnoozeChoice::None } else { SnoozeChoice::Ask(upcoming) }
}

fn snooze_reminder(app: &AppHandle, agenda: &Agenda, args: &Value) -> Result<Value, String> {
    let minutes = args["minutes"].as_u64().map(|n| n as u32);
    let until = args["until"].as_str().unwrap_or("");
    let explicit = args["id"].as_i64().map(|n| n as i32);
    let now = Local::now();
    let target = recurrence::snooze_when(minutes, until, now)?;
    let (items, recent) = {
        let store = agenda.lock();
        let recent = store.recent.clone().map(|r| FireRef { id: r.id, title: r.title, at_ms: r.at_ms, recurrence: r.recurrence });
        (store.items.clone(), recent)
    };
    let choice = choose_snooze(&items, recent.as_ref(), now.timestamp_millis(), explicit);
    let fire = match choice {
        SnoozeChoice::One(fire) => fire,
        SnoozeChoice::Ask(list) => {
            let names = list.iter().map(|r| format!("{} (id {})", r.title, r.id)).collect::<Vec<_>>().join("; ");
            return Ok(json!({
                "ok": true,
                "snoozed": false,
                "detail": format!("Which reminder should I snooze? {names}")
            }));
        }
        SnoozeChoice::None => return Err("Nothing is ringing. Say which reminder to snooze.".into()),
    };
    let when = target.format("%-H:%M").to_string();
    if let Some(item) = items.iter().find(|i| i.id == fire.id).cloned() {
        let repeating = recurrence::is_repeating(&item.recurrence) || !item.person_id.is_empty();
        if repeating {
            if item.at_ms <= now.timestamp_millis() + 60_000 {
                if let Some(next) = spec_from_item(&item).ok().and_then(|spec| recurrence::next_after(&spec, now)) {
                    agenda.retarget(app, item.id, next.timestamp_millis())?;
                }
            }
            let snoozed = agenda.upsert_item(
                app,
                None,
                "reminder",
                &format!("Snoozed: {}", item.title),
                "",
                target.timestamp_millis(),
                Some(ScheduleMeta::default()),
            )?;
            return Ok(json!({
                "ok": true,
                "snoozed": true,
                "id": snoozed.id,
                "detail": format!("Snoozed {} until {when}. The usual time stays.", item.title)
            }));
        }
        let updated = agenda.upsert_item(
            app,
            Some(item.id),
            &item.kind,
            &item.title,
            &item.note,
            target.timestamp_millis(),
            Some(ScheduleMeta { recurrence: "none".into(), ..ScheduleMeta::default() }),
        )?;
        return Ok(json!({
            "ok": true,
            "snoozed": true,
            "id": updated.id,
            "detail": format!("Snoozed {} until {when}.", item.title)
        }));
    }
    let snoozed = agenda.upsert_item(
        app,
        None,
        "reminder",
        &format!("Snoozed: {}", fire.title),
        "",
        target.timestamp_millis(),
        Some(ScheduleMeta::default()),
    )?;
    Ok(json!({
        "ok": true,
        "snoozed": true,
        "id": snoozed.id,
        "detail": format!("Snoozed {} until {when}.", fire.title)
    }))
}

pub fn project_people(existing: &[Item], people: &[Person], now: DateTime<Local>, mut next_id: i32) -> (Vec<Item>, i32, Vec<i32>) {
    let mut cancel = Vec::new();
    let mut kept: Vec<Item> = existing.iter().filter(|i| i.person_id.is_empty()).cloned().collect();
    let mut used = Vec::new();
    for person in people {
        for reminder in &person.reminders {
            let existing_item = existing.iter().find(|i| i.person_id == person.id && i.person_reminder_id == reminder.id);
            if !reminder.enabled {
                if let Some(item) = existing_item {
                    cancel.push(item.id);
                }
                continue;
            }
            let draft = item_from_reminder(person, reminder, existing_item.map(|i| i.id).unwrap_or(0), now);
            let Some(mut draft) = draft else {
                if let Some(item) = existing_item {
                    cancel.push(item.id);
                }
                continue;
            };
            if draft.id == 0 {
                let id = next_id.max(1);
                next_id = id.saturating_add(1);
                draft.id = id;
            } else {
                used.push(draft.id);
                if let Some(existing_item) = existing_item {
                    let same = existing_item.time == reminder.time
                        && existing_item.date == draft.date
                        && existing_item.lead_days == reminder.lead_days
                        && recurrence::normalize_kind(&existing_item.recurrence) == draft.recurrence;
                    if same && existing_item.at_ms > draft.at_ms {
                        draft.at_ms = existing_item.at_ms;
                    }
                }
            }
            kept.push(draft);
        }
        if person.reach_out_days > 0 {
            if let Some(mut nudge) = reach_item(person, existing, now) {
                if nudge.id == 0 {
                    let id = next_id.max(1);
                    next_id = id.saturating_add(1);
                    nudge.id = id;
                } else {
                    used.push(nudge.id);
                }
                kept.push(nudge);
            }
        }
    }
    for item in existing {
        if !item.person_id.is_empty() && !used.contains(&item.id) && !cancel.contains(&item.id) {
            cancel.push(item.id);
        }
    }
    kept.sort_by_key(|i| i.at_ms);
    (kept, next_id, cancel)
}

fn share_item(app: &AppHandle, args: &Value) -> Result<Value, String> {
    let kind = args["kind"].as_str().unwrap_or("").trim();
    let id = args["id"].as_str().unwrap_or("").trim();
    if id.is_empty() {
        return Err("Which one? List them first and use an id.".into());
    }
    let (title, body) = share_text(app, kind, id)?;
    let phone = args["phone"].as_str().unwrap_or("").trim().to_string();
    let handoff_kind = if phone.is_empty() { "share" } else { "sms" };
    let summary = format!("{title}. {body}");
    let staged = json!({
        "phone": phone,
        "text": body,
        "subject": title,
        "body": body,
    });
    if args["confirmed"] != true {
        stage_handoff(app, handoff_kind, &summary, &staged)?;
        let locale = locale_of(app);
        return Ok(json!({
            "ok": true,
            "opened": false,
            "needsConfirm": true,
            "detail": format!("{summary} {}", crate::i18n::line(&locale, "handoff-wait"))
        }));
    }
    clear_handoff(app);
    let prepared = prepare_handoff(handoff_kind, &staged)?;
    let raw = app
        .state::<NativeBridge>()
        .open_hand_off(
            handoff_kind,
            &prepared.query,
            &prepared.url,
            &prepared.phone,
            &prepared.text,
            &prepared.to,
            &prepared.subject,
            &prepared.body,
        )
        .map_err(|err| err.to_string())?;
    serde_json::from_str(&raw).map_err(|err| err.to_string())
}

fn share_text(app: &AppHandle, kind: &str, id: &str) -> Result<(String, String), String> {
    let agenda = app.state::<Agenda>();
    match kind {
        "note" => {
            let note = agenda.notes().into_iter().find(|note| note.id == id).ok_or_else(|| "That note is not on this phone.".to_string())?;
            let title = if note.title.trim().is_empty() { crate::i18n::line(&locale_of(app), "share-note") } else { note.title };
            Ok((title, note.text))
        }
        "reminder" => {
            let id_num: i32 = id.parse().map_err(|_| "That reminder id is not a number.".to_string())?;
            let item = agenda.lock().items.iter().find(|item| item.id == id_num).cloned().ok_or_else(|| "That reminder is not on this phone.".to_string())?;
            let body = if item.note.trim().is_empty() { item.title.clone() } else { format!("{} {}", item.title, item.note) };
            Ok((item.title, body))
        }
        "event" => {
            let snap = agenda.snapshot(app);
            let event = snap.events.into_iter().find(|event| event.id == id).ok_or_else(|| "That event is not on this phone.".to_string())?;
            let body = if event.location.trim().is_empty() { event.title.clone() } else { format!("{} {}", event.title, event.location) };
            Ok((event.title, body))
        }
        _ => Err("kind is note, reminder, or event.".into()),
    }
}

fn reach_item(person: &Person, existing: &[Item], now: DateTime<Local>) -> Option<Item> {
    let days = person.reach_out_days.clamp(1, 365) as i64;
    let interval = days * 86_400_000;
    let existing_item = existing.iter().find(|item| item.person_id == person.id && item.person_reminder_id == "reach");
    let at_ms = if let Some(item) = existing_item {
        if item.at_ms > now.timestamp_millis() {
            item.at_ms
        } else {
            now.timestamp_millis() + interval
        }
    } else {
        now.timestamp_millis() + 60_000
    };
    let mut item = Item::basic(existing_item.map(|item| item.id).unwrap_or(0), "reminder", &format!("Reach out to {}", person.display_name), "", at_ms);
    item.person_id = person.id.clone();
    item.person_reminder_id = "reach".into();
    item.recurrence = "none".into();
    Some(item)
}

fn item_from_reminder(person: &Person, reminder: &PersonReminder, id: i32, now: DateTime<Local>) -> Option<Item> {
    let mut item = Item::basic(id, "reminder", &reminder_title(&person.display_name, reminder), &person.note.chars().take(140).collect::<String>(), now.timestamp_millis());
    item.recurrence = if reminder.recurrence == "once" { "once".into() } else { "yearly".into() };
    item.time = reminder.time.clone();
    item.date = reminder.date.clone();
    item.lead_days = reminder.lead_days;
    item.person_id = person.id.clone();
    item.person_reminder_id = reminder.id.clone();
    let spec = spec_from_item(&item).ok()?;
    item.at_ms = recurrence::notify_anchor(&spec, now)?.timestamp_millis();
    Some(item)
}

/// Birthdays, anniversaries, and named person dates. Reach-out nudges are not dates.
fn is_person_date(item: &Item) -> bool {
    !item.person_id.is_empty() && item.person_reminder_id != "reach"
}

/// Time and the voice reminder list hide a person date until today is on or
/// after the notify day, and through the event day. The stored alarm is unchanged.
fn shows_on_time_list(item: &Item, now: DateTime<Local>) -> bool {
    if !is_person_date(item) {
        return true;
    }
    let Some(at) = DateTime::from_timestamp_millis(item.at_ms).map(|t| t.with_timezone(&Local)) else {
        return false;
    };
    let today = now.date_naive();
    let notify = at.date_naive();
    let event = notify + Duration::days(i64::from(item.lead_days));
    today >= notify && today <= event
}

fn person_date_still_open(item: &Item, now: DateTime<Local>) -> bool {
    if !is_person_date(item) {
        return false;
    }
    let Some(at) = DateTime::from_timestamp_millis(item.at_ms).map(|t| t.with_timezone(&Local)) else {
        return false;
    };
    let event = at.date_naive() + Duration::days(i64::from(item.lead_days));
    now.date_naive() <= event
}

fn reminder_title(name: &str, reminder: &PersonReminder) -> String {
    if !reminder.title.trim().is_empty() {
        return reminder.title.trim().chars().take(120).collect();
    }
    let label = match reminder.label.as_str() {
        "birthday" => "birthday",
        "anniversary" => "anniversary",
        _ => "reminder",
    };
    format!("{name}'s {label}")
}

pub fn sync_person_reminders(app: &AppHandle) -> Result<(), String> {
    let people = app.state::<crate::people::People>().list();
    let agenda = app.state::<Agenda>();
    let mut store = agenda.lock();
    let (items, next_id, cancel) = project_people(&store.items, &people, Local::now(), store.next_id);
    store.items = items;
    store.next_id = next_id.max(store.next_id);
    Agenda::persist(&agenda.path, &store)?;
    let arm: Vec<Item> = store.items.iter().filter(|i| !i.person_id.is_empty()).cloned().collect();
    drop(store);
    let bridge = app.state::<NativeBridge>();
    for id in cancel {
        let _ = bridge.cancel_item(id);
    }
    for item in arm {
        let _ = bridge.schedule_item(item.id, &item.kind, &item.title, &item.note, item.at_ms);
    }
    touch(app);
    Ok(())
}

pub fn replace_notes(app: &AppHandle, notes: Vec<Note>) -> Result<(), String> {
    let agenda = app.state::<Agenda>();
    let mut store = agenda.lock();
    store.notes = notes
        .into_iter()
        .filter_map(|mut note| {
            note.id = note.id.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').take(40).collect();
            note.title = note.title.chars().take(120).collect();
            note.text = note.text.chars().take(20_000).collect();
            if note.id.is_empty() || note.text.trim().is_empty() { None } else { Some(note) }
        })
        .take(200)
        .collect();
    Agenda::persist(&agenda.path, &store)?;
    drop(store);
    touch(app);
    Ok(())
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
    fn relative_and_clock_phrases_become_a_fire_time() {
        let schedule = ScheduleMeta::default();
        let before = Local::now().timestamp_millis();
        let five = reminder_anchor_ms(&json!({"inMinutes": 5, "title": "Check EURUSD price"}), &schedule).unwrap();
        let after = Local::now().timestamp_millis();
        assert!(five >= before + 5 * 60_000 - 1_000, "{five}");
        assert!(five <= after + 5 * 60_000 + 2_000, "{five}");
        let phrase = reminder_anchor_ms(&json!({"at": "in 2 hours"}), &schedule).unwrap();
        assert!(phrase >= before + 2 * 3_600_000 - 1_000);
        let tomorrow = parse_flexible_when("tomorrow at 9").unwrap();
        assert_eq!(tomorrow.date_naive(), Local::now().date_naive() + Duration::days(1));
        assert_eq!((tomorrow.hour(), tomorrow.minute()), (9, 0));
        let evening = parse_flexible_when("7:30 pm").unwrap();
        assert_eq!((evening.hour(), evening.minute()), (19, 30));
        assert!(evening > Local::now());
        let clock = reminder_anchor_ms(&json!({"at": "2030-01-02T15:04"}), &schedule).unwrap();
        assert_eq!(parse_local_when("2030-01-02T15:04").unwrap().timestamp_millis(), clock);
        let err = reminder_anchor_ms(&json!({}), &schedule).unwrap_err();
        assert!(err.contains("inMinutes"));
        assert!(!err.to_ascii_lowercase().contains("specific clock"));
        assert_eq!(timer_seconds(&json!({"inMinutes": 5})).unwrap(), 300);
        assert_eq!(timer_seconds(&json!({"seconds": 90})).unwrap(), 90);
        assert_eq!(timer_label(&json!({"title": "Check EURUSD price"})), "Check EURUSD price");
    }

    #[test]
    fn reminders_alarms_and_countdowns_stay_on_the_time_list() {
        let now = Local::now();
        let reminder = Item::basic(3, "reminder", "Check EURUSD price", "", now.timestamp_millis() + 5 * 60_000);
        let alarm = Item::basic(4, "alarm", "Wake up", "", now.timestamp_millis() + 3_600_000);
        let timer = Item::basic(5, "timer", "Pasta", "", now.timestamp_millis() + 600_000);
        let payload = reminders_payload(&[reminder, alarm, timer], &json!({}), now).unwrap();
        assert_eq!(payload["count"], 3);
        let kinds: Vec<_> = payload["reminders"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["kind"].as_str().unwrap().to_string())
            .collect();
        assert!(kinds.contains(&"reminder".into()));
        assert!(kinds.contains(&"alarm".into()));
        assert!(kinds.contains(&"timer".into()));
    }

    #[test]
    fn talk_creates_stay_in_the_app() {
        let tools = tool_definitions();
        let desc = |name: &str| {
            tools.as_array().unwrap().iter().find(|tool| tool["name"] == name).unwrap()["description"]
                .as_str()
                .unwrap()
                .to_string()
        };
        assert!(desc("set_reminder").contains("inMinutes"));
        assert!(desc("set_timer").contains("Time list"));
        assert!(desc("set_alarm").contains("Time list"));
        assert!(desc("create_calendar_event").contains("this app's Calendar"));
        assert!(!desc("set_timer").contains("Clock"));
        assert!(!desc("set_alarm").contains("Clock app"));
        let hint = lookup_hint();
        assert!(hint.contains("inMinutes"));
        assert!(hint.contains("create_calendar_event"));
        assert!(hint.contains("phone Clock"));
        let phone = include_str!("../gen/android/app/src/main/java/ai/grokvoice/app/PhoneActions.kt");
        assert!(!phone.contains("ACTION_SET_TIMER"));
        assert!(!phone.contains("ACTION_SET_ALARM"));
        assert!(!phone.contains("openClock"));
        let create = include_str!("actions.rs").split("fn create_or_local").nth(1).unwrap().split("fn update_event").next().unwrap();
        assert!(!create.contains("create_event"));
        assert!(create.contains("upsert_local_event"));
        let now = Local::now();
        let soon = now + Duration::minutes(5);
        let event = ev("local-1", "Lunch", soon, soon + Duration::hours(1), "");
        assert_eq!(event_alarm_at(&event, now), Some(event.start_ms));
        let past = now - Duration::hours(2);
        let done = ev("local-2", "Old", past, past + Duration::hours(1), "");
        assert_eq!(event_alarm_at(&done, now), None);
        let daily_start = now + Duration::days(2);
        let daily = ev("local-3", "Standup", daily_start, daily_start + Duration::hours(1), "daily");
        assert_eq!(event_alarm_at(&daily, now), Some(daily.start_ms));
    }

    #[test]
    fn call_and_sms_wait_for_confirmation() {
        let call = unconfirmed_handoff("open_call", &json!({"phone": "+1555", "confirmed": false})).unwrap();
        assert_eq!(call["opened"], false);
        assert!(call["detail"].as_str().unwrap().contains("not placed"));
        let sms = unconfirmed_handoff("open_sms", &json!({"phone": "+1555", "text": "running late", "confirmed": false})).unwrap();
        assert!(sms["detail"].as_str().unwrap().contains("Nothing is sent"));
        assert!(unconfirmed_handoff("open_call", &json!({"phone": "+1555", "confirmed": true})).is_none());
        assert!(unconfirmed_handoff("set_alarm", &json!({})).is_none());
    }

    #[test]
    fn handoff_keeps_https_and_drops_other_schemes() {
        let browser = prepare_handoff("browser", &json!({"url": "https://example.com/a", "confirmed": true})).unwrap();
        assert_eq!(browser.url, "https://example.com/a");
        assert!(prepare_handoff("browser", &json!({"url": "http://example.com"})).is_err());
        assert!(prepare_handoff("browser", &json!({"url": "https://user:pw@example.com"})).is_err());
        assert!(prepare_handoff("browser", &json!({"url": "https://127.0.0.1/admin"})).is_err());
        assert!(prepare_handoff("browser", &json!({"url": "javascript:alert(1)"})).is_err());
        let search = prepare_handoff("browser", &json!({"query": "weather zurich"})).unwrap();
        assert_eq!(search.query, "weather zurich");
        assert!(search.url.is_empty());
        let video = prepare_handoff("youtube", &json!({"url": "https://youtu.be/abc"})).unwrap();
        assert_eq!(video.url, "https://youtu.be/abc");
        assert!(prepare_handoff("youtube", &json!({"url": "https://evil.example/watch"})).is_err());
        assert!(prepare_handoff("youtube", &json!({"url": "https://youtube.com.evil.example/watch"})).is_err());
        let call = prepare_handoff("call", &json!({"phone": "+1 (555) 010-0100"})).unwrap();
        assert_eq!(call.phone, "+15550100100");
        assert!(prepare_handoff("sms", &json!({"phone": "12"})).is_err());
        assert!(prepare_handoff("email", &json!({"to": "a@b.com?bcc=x"})).is_err());
    }

    #[test]
    fn weekly_projection_and_person_birthday_share_the_list() {
        let now = recurrence::local_on(2026, 3, 3, 10, 0);
        let meds = Item {
            recurrence: "daily".into(),
            time: "08:00".into(),
            ..Item::basic(1, "reminder", "Meds", "", now.timestamp_millis())
        };
        let mut person = crate::people::Person {
            id: "p1".into(),
            android_contact_id: None,
            display_name: "Sam Rivera".into(),
            nickname: "Mom".into(),
            aliases: vec![],
            preferred_channel: "whatsapp".into(),
            handles: crate::people::Handles::default(),
            favorite: true,
            note: "call after 5".into(),
            reach_out_days: 0,
            last_contacted_ms: None,
            reminders: vec![crate::people::PersonReminder {
                id: "bday".into(),
                label: "birthday".into(),
                title: String::new(),
                date: "03-03".into(),
                recurrence: "yearly".into(),
                lead_days: 0,
                time: "09:00".into(),
                enabled: true,
            }],
        };
        let (items, next, cancel) = project_people(std::slice::from_ref(&meds), std::slice::from_ref(&person), now, 2);
        assert!(cancel.is_empty());
        assert_eq!(next, 3);
        let birthday = items.iter().find(|i| i.person_reminder_id == "bday").unwrap();
        assert_eq!(birthday.title, "Sam Rivera's birthday");
        assert_eq!(day_of(birthday.at_ms), "2026-03-03");
        assert!(shows_on_time_list(birthday, now));
        assert!(items.iter().any(|i| i.title == "Meds"));
        let next_year = recurrence::local_on(2026, 3, 4, 10, 0);
        let (rolled, _, _) = project_people(std::slice::from_ref(&meds), std::slice::from_ref(&person), next_year, 2);
        let rolled_birthday = rolled.iter().find(|i| i.person_reminder_id == "bday").unwrap();
        assert_eq!(day_of(rolled_birthday.at_ms), "2027-03-03");
        assert!(!shows_on_time_list(rolled_birthday, next_year));

        person.reminders[0].enabled = false;
        let (_items, _, cancel) = project_people(&items, std::slice::from_ref(&person), now, next);
        assert_eq!(cancel, vec![birthday.id]);
    }

    #[test]
    fn custom_person_date_is_scheduled_like_a_birthday() {
        let now = recurrence::local_on(2026, 1, 1, 8, 0);
        let person = crate::people::Person {
            id: "p1".into(),
            android_contact_id: None,
            display_name: "Ana".into(),
            nickname: String::new(),
            aliases: vec![],
            preferred_channel: String::new(),
            handles: crate::people::Handles::default(),
            favorite: false,
            note: String::new(),
            reach_out_days: 0,
            last_contacted_ms: None,
            reminders: vec![
                crate::people::PersonReminder {
                    id: "bday".into(),
                    label: "birthday".into(),
                    title: String::new(),
                    date: "03-03".into(),
                    recurrence: "yearly".into(),
                    lead_days: 0,
                    time: "09:00".into(),
                    enabled: true,
                },
                crate::people::PersonReminder {
                    id: "visa".into(),
                    label: "custom".into(),
                    title: "Visa".into(),
                    date: "06-01".into(),
                    recurrence: "yearly".into(),
                    lead_days: 0,
                    time: "09:00".into(),
                    enabled: true,
                },
            ],
        };
        let (items, _, cancel) = project_people(&[], std::slice::from_ref(&person), now, 1);
        assert!(cancel.is_empty());
        let visa = items.iter().find(|item| item.person_reminder_id == "visa").unwrap();
        assert_eq!(visa.title, "Visa");
        assert_eq!(visa.recurrence, "yearly");
        assert_eq!(
            DateTime::from_timestamp_millis(visa.at_ms).unwrap().with_timezone(&Local).format("%Y-%m-%d").to_string(),
            "2026-06-01"
        );
        let birthday = items.iter().find(|item| item.person_reminder_id == "bday").unwrap();
        assert_eq!(birthday.title, "Ana's birthday");
        assert!(birthday.at_ms != visa.at_ms);
    }

    #[test]
    fn person_dates_join_the_time_list_only_inside_the_notify_window() {
        let march = recurrence::local_on(2026, 3, 3, 10, 0);
        let mut person = sample_person(vec![
            dated("bday", "birthday", "", "06-15", "yearly", 7),
            dated("anniv", "anniversary", "", "11-02", "yearly", 0),
            dated("visa", "custom", "Visa", "2026-04-01", "once", 3),
        ]);
        let meds = Item {
            recurrence: "daily".into(),
            time: "08:00".into(),
            ..Item::basic(9, "reminder", "Meds", "", recurrence::local_on(2026, 3, 4, 8, 0).timestamp_millis())
        };
        let (items, _, cancel) = project_people(std::slice::from_ref(&meds), std::slice::from_ref(&person), march, 1);
        assert!(cancel.is_empty());
        let birthday = items.iter().find(|item| item.person_reminder_id == "bday").unwrap();
        let anniversary = items.iter().find(|item| item.person_reminder_id == "anniv").unwrap();
        let visa = items.iter().find(|item| item.person_reminder_id == "visa").unwrap();
        assert_eq!(day_of(birthday.at_ms), "2026-06-08");
        assert_eq!(day_of(anniversary.at_ms), "2026-11-02");
        assert_eq!(day_of(visa.at_ms), "2026-03-29");
        assert!(!shows_on_time_list(birthday, march));
        assert!(!shows_on_time_list(anniversary, march));
        assert!(!shows_on_time_list(visa, march));
        assert!(shows_on_time_list(items.iter().find(|item| item.title == "Meds").unwrap(), march));

        let opening = recurrence::local_on(2026, 6, 8, 8, 0);
        let (open_items, _, _) = project_people(&[], std::slice::from_ref(&person), opening, 1);
        let open_birthday = open_items.iter().find(|item| item.person_reminder_id == "bday").unwrap();
        assert_eq!(day_of(open_birthday.at_ms), "2026-06-08");
        assert!(shows_on_time_list(open_birthday, opening));

        let during = recurrence::local_on(2026, 6, 12, 15, 0);
        let (during_items, _, _) = project_people(&open_items, std::slice::from_ref(&person), during, 4);
        let during_birthday = during_items.iter().find(|item| item.person_reminder_id == "bday").unwrap();
        assert_eq!(day_of(during_birthday.at_ms), "2026-06-08");
        assert!(shows_on_time_list(during_birthday, during));
        assert!(person_date_still_open(during_birthday, during));
        assert_eq!(day_of(during_items.iter().find(|item| item.person_reminder_id == "anniv").unwrap().at_ms), "2026-11-02");
        assert!(!shows_on_time_list(during_items.iter().find(|item| item.person_reminder_id == "anniv").unwrap(), during));

        let mut dismissed = during_birthday.clone();
        dismissed.at_ms = recurrence::local_on(2027, 6, 8, 9, 0).timestamp_millis();
        let mut held = during_items.clone();
        if let Some(slot) = held.iter_mut().find(|item| item.person_reminder_id == "bday") {
            *slot = dismissed;
        }
        let (after_done, _, _) = project_people(&held, std::slice::from_ref(&person), during, 4);
        let done = after_done.iter().find(|item| item.person_reminder_id == "bday").unwrap();
        assert_eq!(day_of(done.at_ms), "2027-06-08");
        assert!(!shows_on_time_list(done, during));

        let event_day = recurrence::local_on(2026, 6, 15, 20, 0);
        assert!(shows_on_time_list(during_birthday, event_day));
        assert!(person_date_still_open(during_birthday, event_day));
        let passed = recurrence::local_on(2026, 6, 16, 8, 0);
        assert!(!shows_on_time_list(during_birthday, passed));
        assert!(!person_date_still_open(during_birthday, passed));
        let (passed_items, _, _) = project_people(&[], std::slice::from_ref(&person), passed, 1);
        let passed_birthday = passed_items.iter().find(|item| item.person_reminder_id == "bday").unwrap();
        assert_eq!(day_of(passed_birthday.at_ms), "2027-06-08");
        assert!(!shows_on_time_list(passed_birthday, passed));

        let visa_day = recurrence::local_on(2026, 4, 1, 11, 0);
        let (visa_items, _, _) = project_people(&[], std::slice::from_ref(&person), visa_day, 1);
        let due_visa = visa_items.iter().find(|item| item.person_reminder_id == "visa").unwrap();
        assert!(shows_on_time_list(due_visa, visa_day));
        let after_visa = recurrence::local_on(2026, 4, 2, 8, 0);
        let (gone, _, visa_cancel) = project_people(&visa_items, std::slice::from_ref(&person), after_visa, 4);
        assert!(gone.iter().all(|item| item.person_reminder_id != "visa"));
        assert_eq!(visa_cancel, vec![due_visa.id]);

        person.reminders.clear();
        person.reach_out_days = 14;
        let (nudges, _, _) = project_people(&[], std::slice::from_ref(&person), march, 1);
        let reach = nudges.iter().find(|item| item.person_reminder_id == "reach").unwrap();
        assert!(shows_on_time_list(reach, march));
        assert!(!person_date_still_open(reach, march));
    }

    fn day_of(ms: i64) -> String {
        DateTime::from_timestamp_millis(ms).unwrap().with_timezone(&Local).format("%Y-%m-%d").to_string()
    }

    fn sample_person(reminders: Vec<crate::people::PersonReminder>) -> crate::people::Person {
        crate::people::Person {
            id: "p1".into(),
            android_contact_id: None,
            display_name: "Sam Rivera".into(),
            nickname: String::new(),
            aliases: vec![],
            preferred_channel: String::new(),
            handles: crate::people::Handles::default(),
            favorite: false,
            note: String::new(),
            reach_out_days: 0,
            last_contacted_ms: None,
            reminders,
        }
    }

    fn dated(id: &str, label: &str, title: &str, date: &str, recurrence: &str, lead_days: u32) -> crate::people::PersonReminder {
        crate::people::PersonReminder {
            id: id.into(),
            label: label.into(),
            title: title.into(),
            date: date.into(),
            recurrence: recurrence.into(),
            lead_days,
            time: "09:00".into(),
            enabled: true,
        }
    }

    #[test]
    fn snooze_picks_the_firing_reminder_or_asks() {
        let now = recurrence::local_on(2026, 3, 3, 15, 0).timestamp_millis();
        let ringing = Item::basic(4, "reminder", "Standup", "", now - 60_000);
        let later = Item::basic(5, "reminder", "Meds", "", now + 3_600_000);
        match choose_snooze(&[ringing.clone(), later.clone()], None, now, None) {
            SnoozeChoice::One(fire) => assert_eq!(fire.title, "Standup"),
            other => panic!("expected the ringing reminder, got {other:?}"),
        }
        let recent = FireRef { id: 4, title: "Standup".into(), at_ms: now - 5 * 60_000, recurrence: "daily".into() };
        match choose_snooze(std::slice::from_ref(&later), Some(&recent), now, None) {
            SnoozeChoice::One(fire) => assert_eq!(fire.id, 4),
            other => panic!("expected the recent fire, got {other:?}"),
        }
        let other = Item::basic(6, "reminder", "Stretch", "", now - 2 * 60_000);
        match choose_snooze(&[ringing, other, later.clone()], None, now, None) {
            SnoozeChoice::Ask(list) => assert_eq!(list.len(), 2),
            other => panic!("expected a question, got {other:?}"),
        }
        match choose_snooze(std::slice::from_ref(&later), None, now, None) {
            SnoozeChoice::Ask(list) => assert_eq!(list[0].title, "Meds"),
            other => panic!("expected upcoming choices, got {other:?}"),
        }
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

    #[test]
    fn calendar_and_reminder_tools_cover_a_range_in_one_call() {
        let tools = tool_definitions().to_string();
        assert!(tools.contains("date range in one call"));
        assert!(tools.contains("list_people"));
        assert!(!tools.contains("Read events for a day"));
        assert!(!tools.to_ascii_lowercase().contains("only look up one day"));
        let hint = lookup_hint();
        assert!(hint.contains("Do not look up one day at a time"));
        assert!(hint.contains("list_notes"));
        assert!(hint.contains("list_reminders"));
        assert!(hint.contains("list_people"));
        assert!(hint.contains("create_person"));
        assert!(hint.contains("update_settings"));
        assert!(hint.contains("API key"));
        assert!(tools.contains("create_person"));
        assert!(tools.contains("update_person"));
        assert!(tools.contains("delete_person"));
        assert!(tools.contains("get_settings"));
        assert!(tools.contains("update_settings"));
        assert!(tools.contains("notificationDelivery"));
        assert!(tools.contains("does not need confirmation"));
        assert!(hint.contains("notificationDelivery"));
        assert!(tools.contains("confirmed true"));
        assert!(tools.contains("Never") || tools.contains("Refuse API keys"));
    }

    #[test]
    fn voice_edited_person_dates_keep_the_notify_window() {
        let path = std::env::temp_dir().join(format!("grok-people-window-{}.json", std::process::id()));
        let _ = fs::remove_file(&path);
        let people = crate::people::People::load(path.clone());
        crate::people::create_from_tool(&people, &json!({"name": "Ana", "label": "birthday", "date": "June 15"})).unwrap();
        let lead = crate::settings::edit_settings(&crate::settings::Settings::default(), &json!({"notifyDays": 7})).unwrap();
        assert!(lead.settings.is_none());
        let days = lead.person_lead.unwrap().days;
        crate::people::set_lead_days(&people, "birthday", days).unwrap();
        let person = people.list().pop().unwrap();
        assert_eq!(person.reminders[0].date, "06-15");
        assert_eq!(person.reminders[0].lead_days, 7);
        let march = recurrence::local_on(2026, 3, 3, 10, 0);
        let (items, _, _) = project_people(&[], std::slice::from_ref(&person), march, 1);
        let birthday = items.iter().find(|item| item.person_reminder_id == person.reminders[0].id).unwrap();
        assert_eq!(day_of(birthday.at_ms), "2026-06-08");
        assert!(!shows_on_time_list(birthday, march));
        let opening = recurrence::local_on(2026, 6, 8, 9, 0);
        assert!(shows_on_time_list(birthday, opening));
        let event_day = recurrence::local_on(2026, 6, 15, 18, 0);
        assert!(shows_on_time_list(birthday, event_day));
        let after = recurrence::local_on(2026, 6, 16, 8, 0);
        assert!(!shows_on_time_list(birthday, after));
        assert_eq!(person.reminders[0].date, "06-15");
        let listed = crate::people::saved_people_payload(std::slice::from_ref(&person), "");
        assert_eq!(listed["people"][0]["dates"][0]["date"], "06-15");
        assert!(!listed.to_string().contains("phone"));
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn month_range_lists_a_recurring_event_and_a_one_off() {
        let now = recurrence::local_on(2026, 11, 7, 12, 0);
        let window = list_window(&json!({ "month": "2026-11" }), now).unwrap();
        assert!(!window.series);
        assert_eq!(day_of(window.start_ms), "2026-11-01");
        assert_eq!(day_of(window.end_ms), "2026-12-01");
        assert_eq!(window.label, "2026-11-01 through 2026-11-30");

        let rest = list_window(&json!({ "from": "2026-11-07" }), now).unwrap();
        assert_eq!(rest.label, "2026-11-07 through 2026-11-30");
        assert!(list_window(&json!({ "recurring": true }), now).unwrap().series);
        assert!(list_window(&json!({ "from": "2026-01-01", "to": "2027-06-01" }), now).is_err());

        let renew = ev("local-renew", "Renew travel card", recurrence::local_on(2026, 11, 7, 7, 0), recurrence::local_on(2026, 11, 7, 18, 0), "monthly");
        let rent = ev("local-rent", "Rent", recurrence::local_on(2026, 10, 15, 9, 0), recurrence::local_on(2026, 10, 15, 10, 0), "monthly");
        let dentist = ev("local-dentist", "Dentist", recurrence::local_on(2026, 11, 20, 14, 0), recurrence::local_on(2026, 11, 20, 15, 0), "");
        let later = ev("local-later", "December trip", recurrence::local_on(2026, 12, 3, 11, 0), recurrence::local_on(2026, 12, 3, 12, 0), "");
        let stretch = ev("local-stretch", "Stretch", recurrence::local_on(2026, 10, 1, 21, 0), recurrence::local_on(2026, 10, 1, 21, 30), "daily");
        let mut flight = ev("42", "Flight", recurrence::local_on(2026, 11, 18, 7, 0), recurrence::local_on(2026, 11, 18, 8, 0), "");
        flight.source = "phone".into();
        let local = [renew.clone(), rent, dentist.clone(), later, stretch];
        let events = collect_calendar(std::slice::from_ref(&flight), &local, window.start_ms, window.end_ms, false);
        let on = |title: &str| -> Vec<String> {
            events.iter().filter(|event| event.title == title).map(|event| day_of(event.start_ms)).collect()
        };
        assert_eq!(on("Renew travel card"), vec!["2026-11-07".to_string()]);
        assert_eq!(on("Rent"), vec!["2026-11-15".to_string()]);
        assert_eq!(on("Dentist"), vec!["2026-11-20".to_string()]);
        assert_eq!(on("Flight"), vec!["2026-11-18".to_string()]);
        assert!(on("December trip").is_empty());
        assert_eq!(on("Stretch").len(), 30, "daily stretch should fill November and leave other days empty of extra events");
        assert_eq!(events.iter().filter(|event| day_of(event.start_ms) == "2026-11-03").map(|event| event.title.as_str()).collect::<Vec<_>>(), vec!["Stretch"]);
        let repeating = collect_calendar(&[], &local, window.start_ms, window.end_ms, true);
        assert!(repeating.iter().all(|event| event.title != "Dentist" && event.title != "December trip"));
        assert!(repeating.iter().any(|event| event.title == "Renew travel card"));
        let series = series_events(&[renew, dentist], &[]);
        assert_eq!(series.len(), 1);
        assert_eq!(series[0].title, "Renew travel card");
        assert_eq!(series[0].recurrence, "monthly");
    }

    #[test]
    fn reminder_range_expands_repeats_and_hides_person_dates_outside_the_window() {
        let now = recurrence::local_on(2026, 11, 7, 12, 0);
        let meds = Item {
            recurrence: "daily".into(),
            time: "08:00".into(),
            ..Item::basic(1, "reminder", "Meds", "", recurrence::local_on(2026, 11, 8, 8, 0).timestamp_millis())
        };
        let dentist = Item::basic(2, "reminder", "Dentist", "checkup", recurrence::local_on(2026, 11, 20, 14, 0).timestamp_millis());
        let old = Item::basic(3, "reminder", "Old bill", "", recurrence::local_on(2026, 10, 2, 9, 0).timestamp_millis());
        let mut open = Item::basic(4, "reminder", "Sam's birthday", "", recurrence::local_on(2026, 11, 6, 9, 0).timestamp_millis());
        open.recurrence = "yearly".into();
        open.date = "11-09".into();
        open.lead_days = 3;
        open.time = "09:00".into();
        open.person_id = "p1".into();
        open.person_reminder_id = "bday".into();
        let mut june = Item::basic(5, "reminder", "Ana's birthday", "", recurrence::local_on(2026, 6, 8, 9, 0).timestamp_millis());
        june.recurrence = "yearly".into();
        june.date = "06-15".into();
        june.lead_days = 7;
        june.time = "09:00".into();
        june.person_id = "p2".into();
        june.person_reminder_id = "bday".into();
        let mut visa = Item::basic(6, "reminder", "Visa", "", recurrence::local_on(2026, 11, 20, 9, 0).timestamp_millis());
        visa.recurrence = "once".into();
        visa.date = "2026-11-20".into();
        visa.time = "09:00".into();
        visa.person_id = "p2".into();
        visa.person_reminder_id = "visa".into();
        let mut reach = Item::basic(7, "reminder", "Reach out to Sam", "", recurrence::local_on(2026, 11, 12, 10, 0).timestamp_millis());
        reach.person_id = "p1".into();
        reach.person_reminder_id = "reach".into();
        let items = [meds, dentist, old, open, june, visa, reach];

        assert!(shows_on_time_list(items.iter().find(|item| item.title == "Sam's birthday").unwrap(), now));
        assert!(!shows_on_time_list(items.iter().find(|item| item.title == "Ana's birthday").unwrap(), now));
        assert!(!shows_on_time_list(items.iter().find(|item| item.title == "Visa").unwrap(), now));
        assert!(shows_on_time_list(items.iter().find(|item| item.title == "Reach out to Sam").unwrap(), now));

        let month = reminders_payload(&items, &json!({ "month": "2026-11" }), now).unwrap();
        let rows = reminder_rows(&month);
        let meds_days: Vec<_> = rows.iter().filter(|(title, _)| title == "Meds").map(|(_, day)| day.clone()).collect();
        assert_eq!(meds_days.len(), 30, "{meds_days:?}");
        assert_eq!(meds_days.first().map(String::as_str), Some("2026-11-01"));
        assert_eq!(meds_days.last().map(String::as_str), Some("2026-11-30"));
        assert!(rows.iter().any(|(title, day)| title == "Dentist" && day == "2026-11-20"));
        assert!(rows.iter().any(|(title, day)| title == "Sam's birthday" && day == "2026-11-06"));
        assert!(rows.iter().any(|(title, _)| title == "Reach out to Sam"));
        assert!(rows.iter().all(|(title, _)| title != "Visa" && title != "Ana's birthday" && title != "Old bill"));
        assert_eq!(rows.iter().filter(|(_, day)| day == "2026-11-03").count(), 1);

        let day = reminders_payload(&items, &json!({ "day": "2026-11-20" }), now).unwrap();
        let day_rows = reminder_rows(&day);
        assert!(day_rows.iter().any(|(title, _)| title == "Dentist"));
        assert!(day_rows.iter().any(|(title, _)| title == "Meds"));
        assert!(day_rows.iter().all(|(title, _)| title != "Visa"));

        let upcoming = reminders_payload(&items, &json!({}), now).unwrap();
        let upcoming_rows = reminder_rows(&upcoming);
        assert_eq!(upcoming_rows.iter().filter(|(title, _)| title == "Meds").count(), 1);
        assert!(upcoming_rows.iter().any(|(title, _)| title == "Old bill"));
        assert!(upcoming_rows.iter().any(|(title, _)| title == "Sam's birthday"));
        assert!(upcoming_rows.iter().all(|(title, _)| title != "Visa" && title != "Ana's birthday"));

        let found = reminders_payload(&items, &json!({ "month": "2026-11", "query": "check" }), now).unwrap();
        assert_eq!(reminder_rows(&found), vec![("Dentist".into(), "2026-11-20".into())]);
    }

    #[test]
    fn notes_can_be_searched_without_listing_every_store_in_the_prompt() {
        let notes = [
            Note { id: "a".into(), title: "Milk".into(), text: "buy oat milk".into(), created_ms: 2 },
            Note { id: "b".into(), title: "Keys".into(), text: "spare key".into(), created_ms: 1 },
        ];
        let all = notes_payload(&notes, &json!({}));
        assert_eq!(all["count"], 2);
        let milk = notes_payload(&notes, &json!({ "query": "oat" }));
        assert_eq!(milk["count"], 1);
        assert_eq!(milk["notes"][0]["title"], "Milk");
        let one = notes_payload(&notes, &json!({ "id": "b" }));
        assert_eq!(one["notes"][0]["text"], "spare key");
        assert!(!lookup_hint().contains("buy oat milk"));
    }

    fn ev(id: &str, title: &str, start: DateTime<Local>, end: DateTime<Local>, recurrence: &str) -> CalEvent {
        CalEvent {
            id: id.into(),
            title: title.into(),
            start_ms: start.timestamp_millis(),
            end_ms: end.timestamp_millis(),
            location: String::new(),
            all_day: false,
            source: "local".into(),
            recurrence: recurrence.into(),
            weekdays: Vec::new(),
            until_ms: 0,
            alarm_id: 0,
            rrule: String::new(),
        }
    }

    fn reminder_rows(payload: &Value) -> Vec<(String, String)> {
        payload["reminders"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| {
                let at = row["at"].as_str().unwrap_or("");
                let date = at.split(' ').next().unwrap_or("").to_string();
                (row["title"].as_str().unwrap_or("").to_string(), date)
            })
            .collect()
    }
}