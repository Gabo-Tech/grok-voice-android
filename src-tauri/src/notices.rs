//! What a posted notification may say out loud.
//!
//! Spoken mode nudges with a short sentence, using on-device Piper or Android
//! text-to-speech. It is not a Talk session and it does not touch the voice-minute budget.
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
    let locale = notice_locale(spec);
    let minutes = spec
        .get("dueInMinutes")
        .and_then(|v| v.as_i64().or_else(|| v.as_u64().map(|n| n as i64)))
        .unwrap_or(0);
    let hour = spec.get("dueHour").and_then(|v| v.as_i64()).filter(|h| (0..24).contains(h)).map(|h| h as u32);
    spoken_notice_line(&locale, kind, title, body, minutes, hour)
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

/// A short nudge in the app's language. Not a readout of the notification banner.
pub fn spoken_notice_line(
    locale: &str,
    kind: &str,
    title: &str,
    body: &str,
    due_in_minutes: i64,
    due_hour: Option<u32>,
) -> String {
    let locale = crate::i18n::normalize(locale);
    let kind_fold = fold(kind);
    if kind_fold == "update" {
        return say(locale, "say-update", &[]);
    }
    let mut subject = scrub(title);
    let extra = scrub(body);
    if (subject.is_empty() || kind_label(&subject)) && !extra.is_empty() && !kind_label(&extra) {
        subject = extra;
    }
    if kind_label(&subject) {
        subject.clear();
    }
    if subject.to_ascii_lowercase().starts_with("snoozed:") {
        subject = subject.split_once(':').map(|(_, rest)| rest.trim().to_string()).unwrap_or_default();
    }
    if subject.is_empty() {
        return match kind_fold.as_str() {
            "alarm" => say(locale, "say-alarm-plain", &[]),
            "timer" | "countdown" => say(locale, "say-timer-plain", &[]),
            _ => String::new(),
        };
    }
    if let Some(sentence) = ready_sentence(&subject) {
        return sentence;
    }
    subject = subject.trim_end_matches(['.', '!', '?']).trim().to_string();
    if let Some(name) = strip_prefix_ci(&subject, "Reach out to ") {
        return say(locale, "say-reach", &[("name", name)]);
    }
    if let Some(name) = person_name(strip_suffix_ci(&subject, "'s birthday")) {
        return occasion(locale, "bday", name, due_in_minutes, due_hour);
    }
    if let Some(name) = person_name(strip_suffix_ci(&subject, "'s anniversary")) {
        return occasion(locale, "anniv", name, due_in_minutes, due_hour);
    }
    let when = when_phrase(locale, due_in_minutes, due_hour);
    match kind_fold.as_str() {
        "alarm" => {
            if fold(&subject) == "wake up" && when.is_none() {
                say(locale, "say-alarm-wake", &[])
            } else if let Some(when) = when.as_deref() {
                say(locale, "say-task-short-later", &[("title", &subject), ("when", when)])
            } else {
                say(locale, "say-alarm", &[("title", &subject)])
            }
        }
        "timer" | "countdown" => {
            if when.is_some() {
                say(locale, "say-task-short-later", &[("title", &subject), ("when", when.as_deref().unwrap_or(""))])
            } else if fold(&subject) == "timer" {
                say(locale, "say-timer-plain", &[])
            } else {
                say(locale, "say-timer", &[("title", &subject)])
            }
        }
        "event" | "calendar" => {
            if let Some(when) = when.as_deref() {
                say(locale, "say-event-later", &[("title", &subject), ("when", when)])
            } else {
                say(locale, "say-event-now", &[("title", &subject)])
            }
        }
        _ => task_line(locale, &subject, when.as_deref()),
    }
}

fn occasion(locale: &str, which: &str, name: &str, minutes: i64, hour: Option<u32>) -> String {
    let name = name.trim().trim_end_matches('\'').trim();
    if name.is_empty() {
        return String::new();
    }
    let key = match classify(minutes, hour) {
        Span::Now => format!("say-{which}-today"),
        Span::Tomorrow => format!("say-{which}-tomorrow"),
        _ => format!("say-{which}-later"),
    };
    let when = when_phrase(locale, minutes, hour).unwrap_or_else(|| say(locale, "when-today", &[]));
    say(locale, &key, &[("name", name), ("when", &when)])
}

fn task_line(locale: &str, title: &str, when: Option<&str>) -> String {
    let action = title.split_whitespace().count() >= 3;
    let softened = if action { soften_task(title) } else { title.to_string() };
    match (action, when) {
        (true, None) => say(locale, "say-task", &[("title", &softened)]),
        (true, Some(when)) => say(locale, "say-task-later", &[("title", &softened), ("when", when)]),
        (false, None) => say(locale, "say-task-short", &[("title", title)]),
        (false, Some(when)) => say(locale, "say-task-short-later", &[("title", title), ("when", when)]),
    }
}

fn soften_task(title: &str) -> String {
    let mut words: Vec<String> = title.split_whitespace().map(|w| w.to_string()).collect();
    let later_name = words.iter().skip(1).any(|word| word.chars().next().map(|c| c.is_uppercase()).unwrap_or(false));
    if later_name {
        return title.to_string();
    }
    if let Some(first) = words.first_mut() {
        let mut chars = first.chars();
        if let Some(ch) = chars.next() {
            *first = format!("{}{}", ch.to_lowercase(), chars.as_str());
        }
    }
    words.join(" ")
}

fn person_name(name: Option<&str>) -> Option<&str> {
    let name = name?.trim().trim_end_matches(['.', '!', '?']).trim();
    let words = name.split_whitespace().count();
    if name.is_empty() || words > 3 {
        return None;
    }
    let folded = fold(name);
    if folded.contains(" is ") || folded.contains(" es ") || folded.contains(" ist ") {
        return None;
    }
    Some(name)
}

fn ready_sentence(title: &str) -> Option<String> {
    let trimmed = title.trim();
    let words = trimmed.split_whitespace().count();
    let end = trimmed.ends_with(['.', '!', '?']);
    let folded = fold(trimmed);
    let clause = folded.contains(" is ") || folded.contains(" es ") || folded.contains(" ist ");
    if end || (clause && words >= 4) || words >= 8 {
        let mut line = title.trim().trim_end_matches(['.', '!', '?']).trim().to_string();
        if line.is_empty() {
            return None;
        }
        line.push('.');
        return Some(line);
    }
    None
}

enum Span {
    Now,
    Minute,
    Minutes(u32),
    Hour,
    Hours(u32),
    ThisMorning,
    ThisAfternoon,
    ThisEvening,
    Tonight,
    Tomorrow,
    Days(u32),
}

fn classify(minutes: i64, due_hour: Option<u32>) -> Span {
    if minutes <= 10 {
        return Span::Now;
    }
    if minutes < 50 {
        let n = minutes.max(1) as u32;
        return if n <= 1 { Span::Minute } else { Span::Minutes(n) };
    }
    if minutes <= 75 {
        return Span::Hour;
    }
    if minutes < 20 * 60 {
        if let Some(hour) = due_hour {
            if (5..12).contains(&hour) {
                return Span::ThisMorning;
            }
            if (12..17).contains(&hour) {
                return Span::ThisAfternoon;
            }
            if (17..21).contains(&hour) {
                return Span::ThisEvening;
            }
            return Span::Tonight;
        }
        let hours = ((minutes + 30) / 60).clamp(2, 12) as u32;
        return Span::Hours(hours);
    }
    if minutes < 36 * 60 {
        return Span::Tomorrow;
    }
    let days = ((minutes + 12 * 60) / (24 * 60)).clamp(2, 30) as u32;
    Span::Days(days)
}

fn when_phrase(locale: &str, minutes: i64, due_hour: Option<u32>) -> Option<String> {
    let phrase = match classify(minutes, due_hour) {
        Span::Now => return None,
        Span::Minute => say(locale, "when-minute", &[]),
        Span::Minutes(n) => say(locale, "when-minutes", &[("n", &n.to_string())]),
        Span::Hour => say(locale, "when-hour", &[]),
        Span::Hours(n) => say(locale, "when-hours", &[("n", &number_word(locale, n))]),
        Span::ThisMorning => say(locale, "when-morning", &[]),
        Span::ThisAfternoon => say(locale, "when-afternoon", &[]),
        Span::ThisEvening => say(locale, "when-evening", &[]),
        Span::Tonight => say(locale, "when-tonight", &[]),
        Span::Tomorrow => say(locale, "when-tomorrow", &[]),
        Span::Days(n) => say(locale, "when-days", &[("n", &number_word(locale, n))]),
    };
    Some(phrase)
}

fn number_word(locale: &str, n: u32) -> String {
    let words: &[&str] = match crate::i18n::normalize(locale) {
        "es" => &["cero", "un", "dos", "tres", "cuatro", "cinco", "seis", "siete", "ocho", "nueve", "diez", "once", "doce"],
        "de" => &["null", "ein", "zwei", "drei", "vier", "fünf", "sechs", "sieben", "acht", "neun", "zehn", "elf", "zwölf"],
        "pt" => &["zero", "um", "dois", "três", "quatro", "cinco", "seis", "sete", "oito", "nove", "dez", "onze", "doze"],
        "ca-ES-valencia" => &["zero", "un", "dos", "tres", "quatre", "cinc", "sis", "set", "huit", "nou", "deu", "onze", "dotze"],
        _ => &["zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten", "eleven", "twelve"],
    };
    match words.get(n as usize) {
        Some(word) if n > 0 && !word.is_empty() => (*word).to_string(),
        _ => n.to_string(),
    }
}

fn notice_locale(spec: &Value) -> String {
    let locale = spec.get("locale").and_then(|v| v.as_str()).unwrap_or("");
    let phone = spec.get("phoneLang").and_then(|v| v.as_str()).unwrap_or("");
    let tag = if locale.trim().is_empty() { phone } else { locale };
    crate::i18n::normalize(tag).to_string()
}

fn say(locale: &str, key: &str, pairs: &[(&str, &str)]) -> String {
    let line = crate::i18n::fill(locale, key, pairs);
    let line = collapse(&line);
    if !line.chars().any(|c| c.is_alphanumeric()) {
        return String::new();
    }
    clip(&line, 220)
}

fn strip_prefix_ci<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let folded = text.to_ascii_lowercase();
    let prefix = prefix.to_ascii_lowercase();
    if folded.starts_with(&prefix) {
        Some(text[prefix.len()..].trim())
    } else {
        None
    }
}

fn strip_suffix_ci<'a>(text: &'a str, suffix: &str) -> Option<&'a str> {
    let folded = text.to_ascii_lowercase();
    let suffix = suffix.to_ascii_lowercase();
    if folded.ends_with(&suffix) && text.len() > suffix.len() {
        Some(text[..text.len() - suffix.len()].trim())
    } else {
        None
    }
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
        "reminder" | "alarm" | "timer" | "countdown" | "event" | "calendar" | "update" | "notification" | "alert" | "grok"
    )
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
    fn plan_speaks_a_nudge_only_in_spoken_mode() {
        assert_eq!(
            plan_spoken_notice(&spoken("reminder"), "renew travel card", "Reminder"),
            "Hey, remember you have to renew travel card."
        );
        let mut quiet = spoken("reminder");
        quiet["delivery"] = json!("normal");
        assert_eq!(plan_spoken_notice(&quiet, "renew travel card", "Reminder"), "");
        assert_eq!(plan_spoken_notice(&spoken("listening"), "Hey Grok is listening", "Listening"), "");
        assert_eq!(plan_spoken_notice(&spoken("pack"), "Downloading", "42%"), "");
        assert_eq!(plan_spoken_notice(&spoken("confirm"), "Turn Listen off", "Say yes"), "");
        let update = plan_spoken_notice(&spoken("update"), "Grok Voice update", "Version 0.1.29 is ready to download.");
        assert_eq!(update, "Hey, a new version of Grok Voice is available.");
        assert!(!update.contains("0.1.29"));
        assert!(!update.contains("Reminder:"));
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
        assert_eq!(plan_spoken_notice(&alarm, "Wake up", "Alarm"), "Hey, it's time to wake up.");

        let mut priority = spoken("reminder");
        priority["filter"] = json!("priority");
        priority["policyKnown"] = json!(true);
        priority["allowReminders"] = json!(false);
        assert_eq!(plan_spoken_notice(&priority, "Meds", "Reminder"), "");
        priority["allowReminders"] = json!(true);
        assert_eq!(plan_spoken_notice(&priority, "Meds", "Reminder"), "Hey, remember Meds.");

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
        assert_eq!(plan_spoken_notice(&bypass, "Meds", "Reminder"), "Hey, remember Meds.");
    }

    #[test]
    fn spoken_lines_are_natural_and_skip_secrets() {
        let line = |kind: &str, title: &str, body: &str, minutes: i64, hour: Option<u32>| {
            spoken_notice_line("en", kind, title, body, minutes, hour)
        };
        assert_eq!(line("reminder", "renew travel card", "Reminder", 0, None), "Hey, remember you have to renew travel card.");
        assert_eq!(line("event", "Dentist", "", 2 * 24 * 60, None), "Remember, you have Dentist in two days.");
        assert_eq!(line("event", "Dentist", "", 20, None), "Remember, you have Dentist in 20 minutes.");
        assert_eq!(line("event", "Dentist", "", 60, None), "Remember, you have Dentist in an hour.");
        assert_eq!(line("event", "Dentist", "", 180, Some(15)), "Remember, you have Dentist this afternoon.");
        assert_eq!(line("reminder", "Ana's birthday", "college friend", 24 * 60, None), "Hey, remember Ana's birthday is tomorrow.");
        assert_eq!(line("reminder", "Ana's birthday", "", 0, None), "Today is Ana's birthday.");
        assert_eq!(line("reminder", "Reach out to Ana", "Reminder", 0, None), "Hey, remember to reach out to Ana.");
        assert_eq!(line("alarm", "Wake up", "Alarm", 0, None), "Hey, it's time to wake up.");
        assert_eq!(line("alarm", "Standup", "", 0, None), "Hey, it's time for Standup.");
        assert_eq!(line("timer", "Pasta", "Timer", 0, None), "Hey, the Pasta timer is done.");
        assert_eq!(
            line("update", "Grok Voice update", "Version 0.1.29 is ready to download.", 0, None),
            "Hey, a new version of Grok Voice is available."
        );
        let openers = [
            line("reminder", "renew travel card", "", 0, None),
            line("event", "Dentist", "", 2 * 24 * 60, None),
            line("reminder", "Ana's birthday", "", 0, None),
        ];
        assert!(openers[0].starts_with("Hey,"));
        assert!(openers[1].starts_with("Remember,"));
        assert!(openers[2].starts_with("Today"));
        for sample in &openers {
            assert!(!sample.contains("Reminder:"), "{sample}");
            assert!(!sample.contains("Alarm:"), "{sample}");
        }
        assert_eq!(line("reminder", "Tomorrow is Ana's birthday.", "", 0, None), "Tomorrow is Ana's birthday.");
        assert_eq!(line("reminder", "Gate", r#"{"id":"p_1","leadDays":7}"#, 0, None), "Hey, remember Gate.");
        assert_eq!(line("reminder", "", r#"{"password":"hunter2"}"#, 0, None), "");
        assert_eq!(line("alarm", "Alarm", "sk-live-secretvalue123456", 0, None), "Hey, an alarm is going off.");
        assert_eq!(line("reminder", "Backup", "password hunter2", 0, None), "Hey, remember Backup.");
        let uuid = "550e8400-e29b-41d4-a716-446655440000";
        let met = line("reminder", "Meet Sam", &format!("note {uuid}"), 0, None);
        assert_eq!(met, "Hey, remember Meet Sam.");
        assert!(!met.contains(&uuid));
        assert!(!line("reminder", "Meds", "id 42", 0, None).contains("42"));
        assert!(line("reminder", "sk-abcdefghijklmnopqrst", "", 0, None).is_empty());
    }

    #[test]
    fn spoken_lines_follow_the_app_language() {
        assert_eq!(
            spoken_notice_line("es", "reminder", "renovar la tarjeta de viaje", "", 0, None),
            "Oye, recuerda que tienes que renovar la tarjeta de viaje."
        );
        assert_eq!(
            spoken_notice_line("de", "reminder", "Ana's birthday", "", 24 * 60, None),
            "Hey, denk daran, Ana hat morgen Geburtstag."
        );
        assert_eq!(
            spoken_notice_line("pt", "event", "Dentista", "", 2 * 24 * 60, None),
            "Lembra que você tem Dentista em dois dias."
        );
        assert_eq!(
            spoken_notice_line("ca-ES-valencia", "reminder", "Ana's birthday", "", 0, None),
            "Hui és l'aniversari de Ana."
        );
        assert_eq!(
            spoken_notice_line("ca-ES-valencia", "update", "Actualització", "Versió 1", 0, None),
            "Ei, hi ha una versió nova de Grok Voice."
        );
        let mut phone = spoken("reminder");
        phone["locale"] = json!("");
        phone["phoneLang"] = json!("es-ES");
        assert!(plan_spoken_notice(&phone, "Meds", "").starts_with("Oye,"));
    }

    #[test]
    fn spoken_mode_covers_in_app_timers_reminders_alarms_and_events() {
        assert_eq!(
            spoken_notice_line("en", "timer", "Check EURUSD price", "Timer", 0, None),
            "Hey, the Check EURUSD price timer is done."
        );
        assert_eq!(
            spoken_notice_line("en", "countdown", "Check EURUSD price", "Countdown", 0, None),
            "Hey, the Check EURUSD price timer is done."
        );
        assert_eq!(
            spoken_notice_line("en", "reminder", "Check EURUSD price", "Reminder", 0, None),
            "Hey, remember you have to Check EURUSD price."
        );
        assert_eq!(spoken_notice_line("en", "alarm", "Wake up", "Alarm", 0, None), "Hey, it's time to wake up.");
        let event = plan_spoken_notice(&spoken("event"), "Lunch", "");
        assert!(!event.is_empty());
        assert_eq!(event, spoken_notice_line("en", "event", "Lunch", "", 0, None));
        let mut quiet = spoken("timer");
        quiet["delivery"] = json!("normal");
        assert_eq!(plan_spoken_notice(&quiet, "Check EURUSD price", "Timer"), "");
        let mut missing = spoken("reminder");
        missing["delivery"] = json!("");
        assert_eq!(plan_spoken_notice(&missing, "Check EURUSD price", "Reminder"), "");
        assert_eq!(
            plan_spoken_notice(&spoken("countdown"), "Check EURUSD price", "Countdown"),
            "Hey, the Check EURUSD price timer is done."
        );
        let phone = include_str!("../gen/android/app/src/main/java/ai/grokvoice/app/PhoneActions.kt");
        let notify = phone.split("fun notify").nth(1).unwrap().split("fun listEvents").next().unwrap();
        assert!(notify.contains("SpokenAlerts.enqueue"));
        assert!(notify.contains("\"event\""));
        assert!(notify.contains("countdown"));
    }

    #[test]
    fn voice_budget_fields_do_not_change_the_plan() {
        let mut spec = spoken("timer");
        spec["dailyBudgetMinutes"] = json!(0);
        spec["monthlyBudgetMinutes"] = json!(1);
        spec["monthlyHardStop"] = json!(true);
        assert_eq!(plan_spoken_notice(&spec, "Pasta", "Timer"), "Hey, the Pasta timer is done.");
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
