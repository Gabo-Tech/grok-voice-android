//! Web search for on-device Talk. This path never calls xAI.
//! Online, it reads DuckDuckGo. Offline, it returns an error the model can speak.

use serde_json::{Value, json};

pub const OFFLINE: &str = "Web search needs a network connection, and this phone is offline.";
pub const OFFLINE_VOICE: &str = "This phone is offline. Notes, reminders, and People still work in the app. Web search needs a network connection, and the on-device packs are not ready for voice.";

/// Network tools stop with a short sentence. Notes, time, and People stay available.
pub fn network_block(name: &str, online: bool) -> Option<&'static str> {
    if online {
        return None;
    }
    match name {
        "web_search" => Some(OFFLINE),
        "open_browser" => Some("The browser needs a network connection, and this phone is offline."),
        "open_youtube" => Some("YouTube needs a network connection, and this phone is offline."),
        _ => None,
    }
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn offline_tool_error() -> String {
    json!({ "ok": false, "error": OFFLINE }).to_string()
}

#[derive(Debug, PartialEq, Eq)]
pub struct Hit {
    pub title: String,
    pub snippet: String,
    pub url: String,
}

pub fn extract_html(html: &str) -> Vec<Hit> {
    let mut hits = Vec::new();
    let mut rest = html;
    while hits.len() < 4 {
        let Some(anchor) = find_class(rest, "result__a") else { break };
        let (title, after_title) = text_of(anchor);
        let Some(snippet_at) = find_class(after_title, "result__snippet") else { break };
        let (snippet, after_snippet) = text_of(snippet_at);
        let url = attr(anchor, "href").unwrap_or_default();
        if !title.is_empty() || !snippet.is_empty() {
            hits.push(Hit { title: decode_entities(&title), snippet: decode_entities(&snippet), url: decode_entities(&url) });
        }
        rest = after_snippet;
    }
    hits
}

pub fn summary_from_instant(body: &str) -> Option<Value> {
    let value: Value = serde_json::from_str(body).ok()?;
    let mut parts = Vec::new();
    if let Some(text) = value["Answer"].as_str().filter(|text| !text.trim().is_empty()) {
        parts.push(text.trim().to_string());
    }
    if let Some(text) = value["AbstractText"].as_str().filter(|text| !text.trim().is_empty()) {
        parts.push(text.trim().to_string());
    }
    if let Some(topics) = value["RelatedTopics"].as_array() {
        for topic in topics.iter().take(3) {
            if let Some(text) = topic["Text"].as_str().filter(|text| !text.trim().is_empty()) {
                parts.push(text.trim().to_string());
            }
        }
    }
    if parts.is_empty() {
        return None;
    }
    let mut citations = Vec::new();
    if let Some(url) = value["AbstractURL"].as_str().filter(|url| !url.is_empty()) {
        citations.push(url.to_string());
    }
    Some(pack_summary(&parts.join(" "), citations))
}

pub fn summary_from_hits(query: &str, hits: &[Hit]) -> Result<Value, String> {
    if hits.is_empty() {
        return Err(format!("Web search found nothing for {query}."));
    }
    let mut lines = Vec::new();
    let mut citations = Vec::new();
    for hit in hits.iter().take(4) {
        let line = match (hit.title.trim(), hit.snippet.trim()) {
            ("", snippet) => snippet.to_string(),
            (title, "") => title.to_string(),
            (title, snippet) => format!("{title}. {snippet}"),
        };
        if !line.is_empty() {
            lines.push(line);
        }
        if !hit.url.is_empty() && citations.len() < 3 {
            citations.push(hit.url.clone());
        }
    }
    if lines.is_empty() {
        return Err(format!("Web search found nothing for {query}."));
    }
    Ok(pack_summary(&lines.join(" "), citations))
}

fn pack_summary(summary: &str, citations: Vec<String>) -> Value {
    let summary: String = summary.chars().take(900).collect();
    json!({
        "ok": true,
        "summary": summary,
        "citations": citations,
        "detail": "Searched the open web."
    })
}

pub fn lookup(query: &str, online: bool) -> Result<Value, String> {
    let query = query.trim();
    if query.is_empty() {
        return Err("A search query is required.".into());
    }
    if !online {
        return Err(OFFLINE.into());
    }
    match fetch_instant(query) {
        Ok(Some(value)) => Ok(value),
        Ok(None) => fetch_html(query),
        Err(err) if err == OFFLINE => Err(err),
        Err(_) => fetch_html(query),
    }
}

fn fetch_instant(query: &str) -> Result<Option<Value>, String> {
    let url = format!(
        "https://api.duckduckgo.com/?q={}&format=json&no_html=1&skip_disambig=1",
        encode(query)
    );
    let body = get(&url)?;
    Ok(summary_from_instant(&body))
}

fn fetch_html(query: &str) -> Result<Value, String> {
    let url = format!("https://html.duckduckgo.com/html/?q={}", encode(query));
    let body = get(&url)?;
    summary_from_hits(query, &extract_html(&body))
}

fn get(url: &str) -> Result<String, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(12))
        .redirect(crate::http::search_redirect())
        .build()
        .map_err(|err| err.to_string())?;
    let response = client
        .get(url)
        .header("user-agent", "GrokVoice/0.1 (on-device search)")
        .send()
        .map_err(net_err)?;
    if !response.status().is_success() {
        return Err(format!("web search returned {}", response.status()));
    }
    response.text().map_err(net_err)
}

fn net_err(err: reqwest::Error) -> String {
    if err.is_connect() || err.is_timeout() || err.is_request() {
        OFFLINE.to_string()
    } else {
        format!("web search failed: {err}")
    }
}

fn encode(text: &str) -> String {
    let mut out = String::new();
    for byte in text.as_bytes() {
        match *byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(*byte as char),
            b' ' => out.push('+'),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

fn find_class<'a>(html: &'a str, class: &str) -> Option<&'a str> {
    let marker = format!("class=\"{class}\"");
    let at = html.find(&marker).or_else(|| html.find(&format!("class='{class}'")))?;
    Some(&html[at..])
}

fn text_of(at: &str) -> (String, &str) {
    let Some(open) = at.find('>') else { return (String::new(), at) };
    let rest = &at[open + 1..];
    let Some(close) = rest.find('<') else { return (String::new(), rest) };
    (rest[..close].trim().to_string(), &rest[close..])
}

fn attr(at: &str, name: &str) -> Option<String> {
    let key = format!("{name}=\"");
    let start = at.find(&key)? + key.len();
    let slice = &at[start..];
    let end = slice.find('"')?;
    Some(slice[..end].to_string())
}

fn decode_entities(text: &str) -> String {
    text.replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offline_error_is_specific() {
        let err = lookup("weather in zurich", false).unwrap_err();
        assert_eq!(err, OFFLINE);
        assert!(offline_tool_error().contains(OFFLINE));
        assert!(network_block("web_search", false).unwrap().contains("offline"));
        assert!(network_block("open_browser", false).unwrap().contains("offline"));
        assert!(network_block("open_youtube", false).unwrap().contains("offline"));
        assert!(network_block("web_search", true).is_none());
        for name in [
            "create_note",
            "list_notes",
            "update_note",
            "delete_note",
            "set_alarm",
            "set_timer",
            "set_reminder",
            "list_reminders",
            "snooze_reminder",
            "list_calendar_events",
            "create_calendar_event",
            "list_people",
            "get_person",
            "find_contact",
            "set_person_reminder",
            "create_person",
            "update_person",
            "delete_person",
            "get_settings",
            "update_settings",
            "open_call",
            "open_sms",
            "share_item",
            "read_notifications",
            "set_dnd",
            "go_home",
            "phone_status",
            "media_control",
            "play_music",
            "play_local_music",
            "log_money",
            "save_recurring",
            "save_schedule",
            "save_account",
            "list_finance",
            "delete_finance",
            "mark_paid",
            "export_finance_pdf",
        ] {
            assert!(network_block(name, false).is_none(), "{name} stays available offline");
        }
    }

    #[test]
    fn html_results_become_a_summary() {
        let html = r#"
            <a class="result__a" href="https://example.com/a">Zurich &amp; rain</a>
            <a class="result__snippet">It rained today.</a>
            <a class="result__a" href="https://example.com/b">Later</a>
            <a class="result__snippet">Clearing by evening.</a>
        "#;
        let hits = extract_html(html);
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].title, "Zurich & rain");
        let summary = summary_from_hits("weather", &hits).unwrap();
        assert_eq!(summary["ok"], true);
        assert!(summary["summary"].as_str().unwrap().contains("rained"));
        assert!(summary["summary"].as_str().unwrap().contains("Clearing"));
    }

    #[test]
    fn instant_answer_is_used_when_it_has_text() {
        let body = r#"{"Answer":"12 C","AbstractText":"","AbstractURL":"https://example.com","RelatedTopics":[]}"#;
        let summary = summary_from_instant(body).unwrap();
        assert_eq!(summary["summary"], "12 C");
        assert_eq!(summary["citations"][0], "https://example.com");
    }

    #[test]
    fn empty_instant_answer_is_not_a_result() {
        assert!(summary_from_instant(r#"{"Answer":"","AbstractText":"","RelatedTopics":[]}"#).is_none());
    }
}
