//! Live web search through xAI chat completions. Voice calls this as a tool.
//! Typed messages use the same search, plus the phone tools, so a loud room
//! still gets notes, calendar, and current news.

use std::sync::Mutex;

use serde_json::{Value, json};
use tauri::{AppHandle, Manager};

use crate::{actions, diagnostics::Hub, native::NativeBridge, vault::Vault};

const RESPONSES_URL: &str = "https://api.x.ai/v1/responses";
const MODEL: &str = "grok-4.7";

static CONTEXT_NOTES: Mutex<Vec<String>> = Mutex::new(Vec::new());

pub fn take_context_notes() -> Vec<String> {
    let mut notes = CONTEXT_NOTES.lock().unwrap_or_else(|e| e.into_inner());
    std::mem::take(&mut *notes)
}

fn remember_for_voice(text: String) {
    CONTEXT_NOTES.lock().unwrap_or_else(|e| e.into_inner()).push(text.to_string());
}

fn client() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(90))
        .build()
        .expect("search client")
}

fn key(app: &AppHandle) -> Result<String, String> {
    let vault = app.state::<Vault>();
    let bridge = app.state::<NativeBridge>();
    vault
        .api_key(&bridge)
        .map_err(|e| e.to_string())?
        .map(|k| k.to_string())
        .ok_or_else(|| "No xAI key stored on this phone.".into())
}

/// One live-web lookup. Returned to the voice model so it can speak a summary.
pub fn lookup(app: &AppHandle, query: &str) -> Result<Value, String> {
    let query = query.trim();
    if query.is_empty() {
        return Err("A search query is required.".into());
    }
    let body = json!({
        "model": MODEL,
        "temperature": 0.2,
        "instructions": "Answer from current web results. Be factual and compact. Include the date when the question is about today. Name the outlet or site for each claim. Do not invent stories.",
        "tools": [{ "type": "web_search" }],
        "input": [{ "role": "user", "content": query }]
    });
    let value = post(app, &body)?;
    let summary = output_text(&value);
    if summary.is_empty() {
        return Err("web search returned no text".into());
    }
    let citations = citations_of(&value);
    let brief: String = summary.chars().take(1800).collect();
    Ok(json!({
        "ok": true,
        "summary": brief,
        "citations": citations,
        "detail": format!("Searched the web for: {query}")
    }))
}

fn post(app: &AppHandle, body: &Value) -> Result<Value, String> {
    let resp = client()
        .post(RESPONSES_URL)
        .bearer_auth(key(app)?)
        .json(body)
        .send()
        .map_err(|e| format!("xAI request failed: {e}"))?;
    let status = resp.status();
    let value: Value = resp.json().map_err(|e| format!("xAI response: {e}"))?;
    if !status.is_success() {
        let why = value["error"]["message"].as_str().unwrap_or("request rejected");
        return Err(format!("xAI {status}: {why}"));
    }
    Ok(value)
}

fn output_text(value: &Value) -> String {
    let mut text = String::new();
    if let Some(items) = value["output"].as_array() {
        for item in items {
            let chunks = item["content"].as_array().cloned().unwrap_or_default();
            for chunk in chunks {
                if let Some(t) = chunk["text"].as_str() {
                    text.push_str(t);
                }
            }
        }
    }
    text.trim().to_string()
}

fn citations_of(value: &Value) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(list) = value["citations"].as_array() {
        for c in list.iter().take(5) {
            if let Some(s) = c.as_str() {
                out.push(s.to_string());
            } else if let Some(s) = c["url"].as_str() {
                out.push(s.to_string());
            }
        }
    }
    if let Some(items) = value["output"].as_array() {
        for item in items {
            let Some(chunks) = item["content"].as_array() else { continue };
            for chunk in chunks {
                let Some(notes) = chunk["annotations"].as_array() else { continue };
                for note in notes {
                    if let Some(url) = note["url"].as_str() {
                        if !out.iter().any(|u| u == url) && out.len() < 5 {
                            out.push(url.to_string());
                        }
                    }
                }
            }
        }
    }
    out
}

fn chat_tools() -> Value {
    let mut tools = vec![json!({ "type": "web_search" })];
    if let Some(list) = actions::tool_definitions().as_array() {
        tools.extend(list.iter().filter(|t| t["name"] != "web_search").cloned());
    }
    json!(tools)
}

fn system_prompt() -> String {
    "You are Grok on the user's phone. The messages below are the shared memory of spoken and typed talk from the last 12 hours. Use them when the user refers to something already said. Use live web results for news, weather, scores, prices, and anything that changes. Summarize in a few short paragraphs. For notes, reminders, alarms, timers, and calendar, call the matching tool before saying it is done. media_control and set_volume act immediately. Questions about a person's preferred channel, birthday, anniversary, note, or nickname use get_person. Speak that answer and do not open any app. find_contact is only for reaching someone. It checks People nicknames, aliases, and favorites before phone contacts. If a match has preferredChannel, use that channel. set_person_reminder saves a birthday or anniversary. set_reminder repeats when recurrence is daily or weekly. snooze_reminder delays the one that just rang. For the browser, YouTube, WhatsApp, Signal, email, a call, and a text, ask for a clear yes before calling the open tool with confirmed true. Nothing is sent, and a call is not placed, until the user confirms in that app. Times are local YYYY-MM-DDTHH:MM.".into()
}

/// Typed question. Shares the Talk transcript and can call phone tools.
pub fn ask(app: &AppHandle, text: &str) -> Result<String, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("Type a message first.".into());
    }
    let hub = app.state::<std::sync::Arc<Hub>>();
    let user_id = format!("text-{}", crate::diagnostics::now_ms());
    hub.emit("voice://busy", "Searching the web…");
    hub.emit(
        "voice://transcript",
        json!({ "id": user_id, "role": "user", "text": text, "delta": null, "isFinal": true }),
    );
    remember_for_voice(format!("user: {text}"));
    let memory = app.state::<crate::memory::Memory>();
    memory.remember("user", text);
    let mut messages = vec![json!({ "role": "system", "content": system_prompt() })];
    for turn in memory.recent() {
        let role = if turn.role == "assistant" { "assistant" } else { "user" };
        messages.push(json!({ "role": role, "content": turn.text }));
    }

    let tools = chat_tools();
    let mut input = messages;
    let mut answer = String::new();

    for _ in 0..4 {
        let body = json!({
            "model": MODEL,
            "temperature": 0.3,
            "tools": tools,
            "input": input
        });
        let value = match post(app, &body) {
            Ok(v) => v,
            Err(e) => {
                hub.emit("voice://busy", "");
                return Err(e);
            }
        };
        let calls: Vec<Value> = value["output"]
            .as_array()
            .map(|items| items.iter().filter(|i| i["type"] == "function_call").cloned().collect())
            .unwrap_or_default();
        if calls.is_empty() {
            answer = output_text(&value);
            let cites = citations_of(&value);
            if !cites.is_empty() {
                answer.push_str("\n\n");
                for c in cites {
                    answer.push_str(&c);
                    answer.push('\n');
                }
            }
            break;
        }
        for call in &calls {
            input.push(call.clone());
        }
        for call in calls {
            let id = call["call_id"].as_str().unwrap_or("").to_string();
            let name = call["name"].as_str().unwrap_or("").to_string();
            let arguments = call["arguments"].as_str().unwrap_or("{}").to_string();
            let output = actions::run(app, &name, &arguments);
            input.push(json!({
                "type": "function_call_output",
                "call_id": id,
                "output": output
            }));
        }
    }

    if answer.is_empty() {
        hub.emit("voice://busy", "");
        return Err("No answer came back.".into());
    }
    remember_for_voice(format!("assistant: {}", answer.chars().take(500).collect::<String>()));
    app.state::<crate::memory::Memory>().remember("assistant", &answer);
    hub.emit("voice://busy", "");
    let id = format!("text-a-{}", crate::diagnostics::now_ms());
    hub.emit(
        "voice://transcript",
        json!({ "id": id, "role": "assistant", "text": answer, "delta": null, "isFinal": true }),
    );
    Ok(answer)
}
