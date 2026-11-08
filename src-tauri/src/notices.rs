//! What a posted notification may say out loud.
//!
//! Spoken mode reads the alert with on-device Piper or Android text-to-speech.
//! It is not a Talk session and it does not touch the voice-minute budget.
//! Normal is the default, so an older settings file stays quiet.

use serde_json::Value;

pub const DELIVERY_NORMAL: &str = "normal";
pub const DELIVERY_SPOKEN: &str = "spoken";

/// Missing JSON becomes empty. Load and save turn that into Normal, and a save
/// that omitted the field keeps whatever is already stored.
pub fn omitted_delivery() -> String {
    String::new()
}

/// Canonical value stored on disk. Empty and anything unrecognized are Normal.
pub fn stored_delivery(value: &str) -> String {
    if is_spoken_word(value) {
        DELIVERY_SPOKEN.to_string()
    } else {
        DELIVERY_NORMAL.to_string()
    }
}

pub fn is_spoken_word(value: &str) -> bool {
    matches!(
        fold(value).as_str(),
        "spoken" | "speak" | "aloud" | "outloud" | "out loud" | "read" | "readaloud" | "read aloud" | "voice" | "talking"
    )
}

/// Voice and the settings screen. Unknown words are an error so a typo cannot
/// silently flip the mode.
pub fn parse_delivery(value: &str) -> Result<String, String> {
    let folded = fold(value);
    if matches!(folded.as_str(), "normal" | "silent" | "quiet" | "banner" | "visual" | "off" | "false") {
        return Ok(DELIVERY_NORMAL.to_string());
    }
    if is_spoken_word(value) || matches!(folded.as_str(), "on" | "true") {
        return Ok(DELIVERY_SPOKEN.to_string());
    }
    Err("Notifications are normal or spoken out loud.".into())
}

/// User-facing alerts that already post a notification. Status chrome is refused.
pub fn should_speak_kind(delivery: &str, kind: &str) -> bool {
    stored_delivery(delivery) == DELIVERY_SPOKEN && is_alert_kind(kind)
}

pub fn is_alert_kind(kind: &str) -> bool {
    matches!(
        fold(kind).as_str(),
        "" | "reminder" | "alarm" | "timer" | "countdown" | "event" | "calendar" | "update" | "reach" | "reachout" | "birthday" | "anniversary"
    )
}

/// Piper only when the configured language has a pack already in the app.
/// Spanish, Portuguese, and Valencian stay on Android text-to-speech.
pub fn piper_pack(locale: &str, phone_lang: &str) -> Option<&'static str> {
    match spoken_lang(locale, phone_lang) {
        "de" => Some("tts-de"),
        "en" => Some("tts"),
        _ => None,
    }
}

pub fn piper_dir(pack: &str) -> Option<&'static str> {
    match pack {
        "tts" => Some("tts/vits-piper-en_US-lessac-low"),
        "tts-de" => Some("tts-de/vits-piper-de_DE-thorsten-low"),
        _ => None,
    }
}

pub fn piper_model_name(pack: &str) -> Option<&'static str> {
    match pack {
        "tts" => Some("en_US-lessac-low.onnx"),
        "tts-de" => Some("de_DE-thorsten-low.onnx"),
        _ => None,
    }
}

/// Empty means do not speak. The notification is still posted by the caller.
pub fn plan_spoken_notice(spec: &Value, title: &str, body: &str) -> String {
    if !spec.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true) {
        return String::new();
    }
    if spec.get("channelBlocked").and_then(|v| v.as_bool()).unwrap_or(false) {
        return String::new();
    }
    let delivery = spec.get("delivery").and_then(|v| v.as_str()).unwrap_or("");
    let kind = spec.get("kind").and_then(|v| v.as_str()).unwrap_or("");
    if !should_speak_kind(delivery, kind) {
        return String::new();
    }
    if !interruption_allows(spec, spec.get("category").and_then(|v| v.as_str()).unwrap_or("reminder")) {
        return String::new();
    }
    spoken_notice_line(title, body)
}

#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub fn plan_spoken_notice_json(spec_json: &str, title: &str, body: &str) -> String {
    let spec = serde_json::from_str(spec_json).unwrap_or(Value::Null);
    plan_spoken_notice(&spec, title, body)
}

fn interruption_allows(spec: &Value, category: &str) -> bool {
    if spec.get("bypass").and_then(|v| v.as_bool()).unwrap_or(false) {
        return true;
    }
    let filter = fold(spec.get("filter").and_then(|v| v.as_str()).unwrap_or("all"));
    match filter.as_str() {
        "none" | "total" => false,
        "alarms" | "alarm" => fold(category) == "alarm",
        "priority" => {
            if !spec.get("policyKnown").and_then(|v| v.as_bool()).unwrap_or(false) {
                return false;
            }
            let key = match fold(category).as_str() {
                "alarm" => "allowAlarms",
                "reminder" => "allowReminders",
                "event" => "allowEvents",
                _ => return false,
            };
            spec.get(key).and_then(|v| v.as_bool()).unwrap_or(false)
        }
        _ => true,
    }
}

/// A short sentence from the title and the useful part of the body.
pub fn spoken_notice_line(title: &str, body: &str) -> String {
    let title = scrub(title);
    let body = scrub(body);
    let line = if title.is_empty() {
        body
    } else if body.is_empty() || loose_same(&title, &body) {
        title
    } else if kind_label(&body) && !kind_label(&title) {
        format!("{}: {title}", label_word(&body))
    } else if kind_label(&title) && !kind_label(&body) {
        format!("{}: {body}", label_word(&title))
    } else if contains_loose(&title, &body) {
        title
    } else if contains_loose(&body, &title) {
        body
    } else if title.ends_with(['.', '!', '?']) {
        format!("{title} {body}")
    } else {
        format!("{title}. {body}")
    };
    let line = collapse(&line);
    if !line.chars().any(|c| c.is_alphanumeric()) {
        return String::new();
    }
    clip(&line, 180)
}

fn spoken_lang(locale: &str, phone_lang: &str) -> &'static str {
    let tag = if locale.trim().is_empty() { phone_lang } else { locale };
    let tag = tag.trim().to_ascii_lowercase().replace('_', "-");
    if tag.starts_with("de") {
        "de"
    } else if tag.is_empty() || tag.starts_with("en") {
        "en"
    } else {
        "other"
    }
}

fn kind_label(text: &str) -> bool {
    matches!(
        fold(text).as_str(),
        "reminder" | "alarm" | "timer" | "countdown" | "event" | "calendar" | "update" | "notification" | "alert"
    )
}

fn label_word(text: &str) -> &'static str {
    match fold(text).as_str() {
        "alarm" => "Alarm",
        "timer" | "countdown" => "Timer",
        "event" | "calendar" => "Event",
        "update" => "Update",
        "notification" | "alert" => "Notification",
        _ => "Reminder",
    }
}

fn scrub(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() || looks_like_json(trimmed) {
        return String::new();
    }
    let tokens: Vec<&str> = trimmed.split_whitespace().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let token = tokens[i];
        let folded = fold_token(token);
        if folded.is_empty() {
            i += 1;
            continue;
        }
        if is_secret_token(&folded) || is_uuid(&folded) {
            i += 1;
            continue;
        }
        if matches!(folded.as_str(), "password" | "passphrase" | "apikey" | "secret") {
            i += 1;
            if i < tokens.len() {
                i += 1;
            }
            continue;
        }
        if folded == "api" && i + 1 < tokens.len() && fold_token(tokens[i + 1]) == "key" {
            i += 2;
            if i < tokens.len() && is_secret_token(&fold_token(tokens[i])) {
                i += 1;
            }
            continue;
        }
        if folded == "id" && i + 1 < tokens.len() && fold_token(tokens[i + 1]).chars().all(|c| c.is_ascii_digit()) {
            i += 2;
            continue;
        }
        out.push(strip_edges(token));
        i += 1;
    }
    collapse(&out.join(" "))
}

fn looks_like_json(text: &str) -> bool {
    let t = text.trim();
    (t.starts_with('{') && t.ends_with('}')) || (t.starts_with('[') && t.ends_with(']'))
}

fn is_secret_token(folded: &str) -> bool {
    let bare = folded.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '-');
    bare.starts_with("sk-")
        || bare.starts_with("xai-")
        || bare.starts_with("bearer")
        || (bare.len() >= 20 && bare.chars().all(|c| c.is_ascii_alphanumeric()))
}

fn is_uuid(folded: &str) -> bool {
    let t = folded.trim_matches(|c: char| c == '(' || c == ')' || c == ',' || c == '.');
    let parts: Vec<&str> = t.split('-').collect();
    parts.len() == 5
        && [8, 4, 4, 4, 12] == [parts[0].len(), parts[1].len(), parts[2].len(), parts[3].len(), parts[4].len()]
        && parts.iter().all(|part| part.chars().all(|c| c.is_ascii_hexdigit()))
}

fn fold_token(token: &str) -> String {
    token
        .trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '-')
        .to_ascii_lowercase()
}

fn strip_edges(token: &str) -> &str {
    token.trim_matches(|c: char| c == '"' || c == '\'' || c == '`' || c == ',')
}

fn loose_same(a: &str, b: &str) -> bool {
    fold(a) == fold(b)
}

fn contains_loose(hay: &str, needle: &str) -> bool {
    let needle = fold(needle);
    !needle.is_empty() && fold(hay).contains(&needle)
}

fn fold(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ").trim().to_ascii_lowercase()
}

fn collapse(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn clip(text: &str, max: usize) -> String {
    let mut out: String = text.chars().take(max).collect();
    if text.chars().count() > max {
        if let Some(space) = out.rfind(' ') {
            out.truncate(space);
        }
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn spoken(kind: &str) -> Value {
        json!({
            "delivery": "spoken",
            "kind": kind,
            "enabled": true,
            "channelBlocked": false,
            "bypass": false,
            "filter": "all",
            "policyKnown": true,
            "category": if kind == "alarm" || kind == "timer" { "alarm" } else if kind == "update" { "status" } else { "reminder" }
        })
    }

    #[test]
    fn normal_is_silent_and_spoken_reads_alerts_only() {
        assert!(!should_speak_kind("normal", "reminder"));
        assert!(!should_speak_kind("", "alarm"));
        assert!(!should_speak_kind("spoken", "listening"));
        assert!(!should_speak_kind("spoken", "foreground"));
        assert!(!should_speak_kind("spoken", "pack"));
        assert!(!should_speak_kind("spoken", "progress"));
        assert!(!should_speak_kind("spoken", "download"));
        assert!(!should_speak_kind("spoken", "confirm"));
        assert!(!should_speak_kind("spoken", "handoff"));
        assert!(!should_speak_kind("spoken", "wake"));
        for kind in ["reminder", "alarm", "timer", "countdown", "event", "calendar", "update", "reach", "birthday", "anniversary"] {
            assert!(should_speak_kind("spoken", kind), "{kind}");
            assert!(should_speak_kind("Spoken", kind), "{kind}");
        }
    }

    #[test]
    fn plan_speaks_the_sentence_only_in_spoken_mode() {
        assert_eq!(
            plan_spoken_notice(&spoken("reminder"), "renew travel card", "Reminder"),
            "Reminder: renew travel card"
        );
        let mut quiet = spoken("reminder");
        quiet["delivery"] = json!("normal");
        assert_eq!(plan_spoken_notice(&quiet, "renew travel card", "Reminder"), "");
        assert_eq!(plan_spoken_notice(&spoken("listening"), "Hey Grok is listening", "Listening"), "");
        assert_eq!(plan_spoken_notice(&spoken("pack"), "Downloading", "42%"), "");
        assert_eq!(plan_spoken_notice(&spoken("confirm"), "Turn Listen off", "Say yes"), "");
        assert_eq!(
            plan_spoken_notice(&spoken("update"), "Grok Voice update", "Version 0.1.29 is ready to download."),
            "Grok Voice update. Version 0.1.29 is ready to download."
        );
    }

    #[test]
    fn dnd_suppresses_speech_when_it_would_suppress_the_alert() {
        let mut none = spoken("reminder");
        none["filter"] = json!("none");
        assert_eq!(plan_spoken_notice(&none, "Meds", "Reminder"), "");

        let mut alarms = spoken("reminder");
        alarms["filter"] = json!("alarms");
        assert_eq!(plan_spoken_notice(&alarms, "Meds", "Reminder"), "");
        let mut alarm = spoken("alarm");
        alarm["filter"] = json!("alarms");
        assert_eq!(plan_spoken_notice(&alarm, "Wake up", "Alarm"), "Alarm: Wake up");

        let mut priority = spoken("reminder");
        priority["filter"] = json!("priority");
        priority["policyKnown"] = json!(true);
        priority["allowReminders"] = json!(false);
        assert_eq!(plan_spoken_notice(&priority, "Meds", "Reminder"), "");
        priority["allowReminders"] = json!(true);
        assert_eq!(plan_spoken_notice(&priority, "Meds", "Reminder"), "Reminder: Meds");

        let mut unknown = spoken("reminder");
        unknown["filter"] = json!("priority");
        unknown["policyKnown"] = json!(false);
        assert_eq!(plan_spoken_notice(&unknown, "Meds", "Reminder"), "");

        let mut status = spoken("update");
        status["filter"] = json!("priority");
        status["policyKnown"] = json!(true);
        status["allowReminders"] = json!(true);
        assert_eq!(plan_spoken_notice(&status, "Grok Voice update", "Version 1 is ready."), "");

        let mut blocked = spoken("alarm");
        blocked["channelBlocked"] = json!(true);
        assert_eq!(plan_spoken_notice(&blocked, "Wake up", "Alarm"), "");
        blocked["channelBlocked"] = json!(false);
        blocked["enabled"] = json!(false);
        assert_eq!(plan_spoken_notice(&blocked, "Wake up", "Alarm"), "");

        let mut bypass = spoken("reminder");
        bypass["filter"] = json!("none");
        bypass["bypass"] = json!(true);
        assert_eq!(plan_spoken_notice(&bypass, "Meds", "Reminder"), "Reminder: Meds");
    }

    #[test]
    fn spoken_line_is_a_sentence_without_json_ids_or_secrets() {
        assert_eq!(spoken_notice_line("Tomorrow is Ana's birthday.", ""), "Tomorrow is Ana's birthday.");
        assert_eq!(
            spoken_notice_line("Tomorrow is Ana's birthday.", "Tomorrow is Ana's birthday."),
            "Tomorrow is Ana's birthday."
        );
        assert_eq!(spoken_notice_line("Reach out to Ana", "Reminder"), "Reminder: Reach out to Ana");
        assert_eq!(spoken_notice_line("Ana's birthday", "college friend"), "Ana's birthday. college friend");
        assert_eq!(spoken_notice_line("Gate", r#"{"id":"p_1","leadDays":7}"#), "Gate");
        assert_eq!(spoken_notice_line("", r#"{"password":"hunter2"}"#), "");
        assert_eq!(spoken_notice_line("Alarm", "sk-live-secretvalue123456"), "Alarm");
        assert_eq!(spoken_notice_line("Backup", "password hunter2"), "Backup");
        let uuid = "550e8400-e29b-41d4-a716-446655440000";
        assert_eq!(spoken_notice_line("Meet Sam", &format!("note {uuid}")), "Meet Sam. note");
        assert_eq!(spoken_notice_line("Meds", "id 42"), "Meds");
        assert!(spoken_notice_line("sk-abcdefghijklmnopqrst", "").is_empty());
    }

    #[test]
    fn voice_budget_fields_do_not_change_the_plan() {
        let mut spec = spoken("timer");
        spec["dailyBudgetMinutes"] = json!(0);
        spec["monthlyBudgetMinutes"] = json!(1);
        spec["monthlyHardStop"] = json!(true);
        assert_eq!(plan_spoken_notice(&spec, "Pasta", "Timer"), "Timer: Pasta");
        let body = include_str!("notices.rs").split("#[cfg(test)]").next().unwrap();
        assert!(!body.to_ascii_lowercase().contains("usage"));
        assert!(!body.contains("session_ms"));
    }

    #[test]
    fn piper_follows_the_configured_language_pack() {
        assert_eq!(piper_pack("en", "es"), Some("tts"));
        assert_eq!(piper_pack("de", "en"), Some("tts-de"));
        assert_eq!(piper_pack("", "de"), Some("tts-de"));
        assert_eq!(piper_pack("", "en"), Some("tts"));
        assert_eq!(piper_pack("", ""), Some("tts"));
        assert_eq!(piper_pack("es", "en"), None);
        assert_eq!(piper_pack("pt", "de"), None);
        assert_eq!(piper_pack("ca-ES-valencia", "ca"), None);
        assert_eq!(piper_dir("tts"), Some("tts/vits-piper-en_US-lessac-low"));
        assert_eq!(piper_model_name("tts-de"), Some("de_DE-thorsten-low.onnx"));
        let packs = include_str!("local/packs.rs");
        assert!(packs.contains("tts/vits-piper-en_US-lessac-low"));
        assert!(packs.contains("en_US-lessac-low.onnx"));
        assert!(packs.contains("tts-de/vits-piper-de_DE-thorsten-low"));
        assert!(packs.contains("de_DE-thorsten-low.onnx"));
    }

    #[test]
    fn callers_leave_status_chrome_alone() {
        let phone = include_str!("../gen/android/app/src/main/java/ai/grokvoice/app/PhoneActions.kt");
        let updates = include_str!("../gen/android/app/src/main/java/ai/grokvoice/app/UpdateNotice.kt");
        let listener = include_str!("../gen/android/app/src/main/java/ai/grokvoice/app/VoiceListenerService.kt");
        let handoff = include_str!("../gen/android/app/src/main/java/ai/grokvoice/app/HandOff.kt");
        let spoken = include_str!("../gen/android/app/src/main/java/ai/grokvoice/app/SpokenAlerts.kt");
        assert!(phone.contains("SpokenAlerts.enqueue"));
        assert!(updates.contains("SpokenAlerts.enqueue"));
        assert!(!listener.contains("SpokenAlerts"));
        assert!(!handoff.contains("SpokenAlerts"));
        assert!(!spoken.contains("UsageStore"));
        assert!(!spoken.contains("startSession"));
        assert!(!spoken.contains("gv_llm"));
        assert!(spoken.contains("vits-piper-en_US-lessac-low"));
        assert!(spoken.contains("vits-piper-de_DE-thorsten-low"));
    }
}
