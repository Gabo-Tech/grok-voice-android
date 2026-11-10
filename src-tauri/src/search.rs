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
        .redirect(crate::http::api_redirect())
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
    let online = app
        .state::<NativeBridge>()
        .device_facts()
        .map(|facts| facts.online)
        .unwrap_or(true);
    if !online {
        return Err(crate::local::OFFLINE.into());
    }
    if crate::local::public_search_enabled() {
        return crate::local::public_search(query, true);
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
        .map_err(|e| {
            if e.is_connect() || e.is_timeout() || e.is_request() {
                crate::local::OFFLINE.to_string()
            } else {
                format!("xAI request failed: {e}")
            }
        })?;
    let status = resp.status();
    let value: Value = resp.json().map_err(|e| format!("xAI response: {e}"))?;
    if !status.is_success() {
        let why = crate::diagnostics::redact(value["error"]["message"].as_str().unwrap_or("request rejected"));
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

fn text_uses_local(app: &AppHandle) -> bool {
    let settings = app.state::<crate::settings::SettingsStore>().get();
    let ready = crate::local::model_dir(app)
        .map(|dir| crate::local::packs_ready(&dir))
        .unwrap_or(false);
    let Ok(accounts) = app.state::<Vault>().accounts(&app.state::<NativeBridge>()) else {
        return false;
    };
    let providers: Vec<&str> = accounts.iter().map(|account| account.provider.as_str()).collect();
    crate::local::text_uses_local(&providers, settings.prefer_local_on_wake, ready)
}

fn has_xai(app: &AppHandle) -> bool {
    app.state::<Vault>()
        .accounts(&app.state::<NativeBridge>())
        .map(|accounts| accounts.iter().any(|account| account.provider == "xai"))
        .unwrap_or(false)
}

fn system_prompt() -> String {
    let mut prompt = "You are Grok on the user's phone. The messages below are the shared memory of spoken and typed talk from the last 12 hours. Use them when the user refers to something already said. Use live web results for news, weather, scores, prices, and anything that changes. Summarize in a few short paragraphs. For notes, reminders, alarms, timers, and calendar, call the matching tool before saying it is done. media_control and set_volume act immediately. play_local_music plays audio files and playlists stored on this phone, immediately. If they say YouTube, call open_youtube instead and wait for a clear yes. Questions about a person's preferred channel, birthday, anniversary, note, or nickname use get_person. Speak that answer and do not open any app. find_contact is only for reaching someone. It checks People nicknames, aliases, and favorites before phone contacts. If a match has preferredChannel, use that channel. create_person adds someone in People. update_person changes their name, note, channel, or date. delete_person waits for a clear yes. get_settings and update_settings change language, Listen, voice budgets, and whether alerts are spoken out loud. Do not say you cannot edit People or settings. Never save or repeat an API key. set_person_reminder saves a birthday or anniversary. set_reminder repeats when recurrence is daily or weekly. snooze_reminder delays the one that just rang. For the browser, YouTube, WhatsApp, Signal, email, a call, and a text, ask for a clear yes before calling the open tool with confirmed true. Nothing is sent, and a call is not placed, until the user confirms in that app. Reminders, alarms, and countdowns are saved in this app's Time list. Calendar events are saved in this app's Calendar. inMinutes or inHours means from now. A clock time is 7pm, tomorrow at 9, or YYYY-MM-DDTHH:MM. Alarms use hour 0-23 and minute. Timers use seconds. share_item shares a note, reminder, or event as a draft after a clear yes. read_notifications, set_dnd, go_home, and phone_status run on this phone. Do not guess the battery or the next alarm.".to_string();
    prompt.push_str(crate::actions::lookup_hint());
    prompt
}

/// Typed question. Shares the Talk transcript and can call phone tools.
pub fn ask(app: &AppHandle, text: &str) -> Result<String, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("Type a message first.".into());
    }
    let hub = app.state::<std::sync::Arc<Hub>>();
    let user_id = format!("text-{}", crate::diagnostics::now_ms());
    let outbound = crate::memory::for_model(text);
    hub.emit("voice://busy", "Searching the web…");
    hub.emit(
        "voice://transcript",
        json!({ "id": user_id, "role": "user", "text": outbound, "delta": null, "isFinal": true }),
    );
    let memory = app.state::<crate::memory::Memory>();
    memory.remember("user", &outbound);
    let mut messages = vec![json!({ "role": "system", "content": system_prompt() })];
    for turn in memory.recent() {
        let role = if turn.role == "assistant" { "assistant" } else { "user" };
        messages.push(json!({ "role": role, "content": turn.text }));
    }
    match messages.last_mut() {
        Some(last) if last["role"] == "user" => last["content"] = json!(outbound),
        _ => messages.push(json!({ "role": "user", "content": outbound })),
    }

    let online = app
        .state::<NativeBridge>()
        .device_facts()
        .map(|facts| facts.online)
        .unwrap_or(true);
    if crate::local::public_search_enabled() {
        remember_for_voice(format!("pending: {outbound}"));
        hub.emit("voice://busy", "");
        return Ok("sent to the on-device session".into());
    }
    remember_for_voice(format!("user: {outbound}"));
    if text_uses_local(app) {
        hub.emit("voice://busy", "On this phone…");
        match crate::local::answer_text(app, &outbound) {
            Ok(answer) => {
                remember_for_voice(format!("assistant: {}", answer.chars().take(2_000).collect::<String>()));
                app.state::<crate::memory::Memory>().remember("assistant", &answer);
                hub.emit("voice://busy", "");
                let id = format!("text-a-{}", crate::diagnostics::now_ms());
                hub.emit(
                    "voice://transcript",
                    json!({ "id": id, "role": "assistant", "text": answer, "delta": null, "isFinal": true }),
                );
                return Ok(answer);
            }
            Err(err) => {
                hub.error(err.clone());
                if !has_xai(app) {
                    hub.emit("voice://busy", "");
                    let _ = app.state::<NativeBridge>().speak_fallback(&err);
                    return Err(err);
                }
                hub.warn(format!("on device failed, trying the next key: {err}"));
                hub.emit("voice://busy", "Searching the web…");
            }
        }
    }
    if !online {
        return ask_offline(app, &outbound);
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
                if e.contains("needs a network connection") {
                    hub.error(e.clone());
                }
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

fn ask_offline(app: &AppHandle, text: &str) -> Result<String, String> {
    let hub = app.state::<std::sync::Arc<Hub>>();
    hub.emit("voice://busy", "On this phone…");
    match crate::local::answer_text(app, text) {
        Ok(answer) => {
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
        Err(err) => {
            hub.emit("voice://busy", "");
            hub.error(err.clone());
            let _ = app.state::<NativeBridge>().speak_fallback(&err);
            Err(err)
        }
    }
}
