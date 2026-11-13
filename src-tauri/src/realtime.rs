//! xAI Realtime (speech-to-speech) session.
//!
//! Auth: the long-lived API key never goes on the WebSocket. The phone itself mints a
//! short-lived client secret (`POST /v1/realtime/client_secrets`) with the key from the
//! vault, then authenticates the socket with `Authorization: Bearer <ephemeral>`.
//! Audio: 16 kHz PCM16 uplink (`input_audio_buffer.append`), 24 kHz PCM16 downlink
//! (`response.output_audio.delta`) pushed straight into the cpal speaker queue.

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use serde_json::{Value, json};
use tauri::{AppHandle, Manager};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::{
    Message,
    client::IntoClientRequest,
    http::{HeaderValue, header::AUTHORIZATION},
};

use crate::{
    audio::{
        engine::UPLINK_RATE,
        playback::Player,
        resample::{Resampler, i16_to_f32},
    },
    diagnostics::{Hub, Phase, now_ms},
    error::{Result, msg},
    native::NativeBridge,
    actions,
    settings::Settings,
    vault::Vault,
};

const CLIENT_SECRETS_URL: &str = "https://api.x.ai/v1/realtime/client_secrets";
const REALTIME_URL: &str = "wss://api.x.ai/v1/realtime";
const OPENAI_CLIENT_SECRETS_URL: &str = "https://api.openai.com/v1/realtime/client_secrets";
const OPENAI_REALTIME_URL: &str = "wss://api.openai.com/v1/realtime";
const OPENAI_MODEL: &str = "gpt-realtime";
const OUTPUT_RATE: u32 = 24_000;
const TOKEN_TTL_SECS: u64 = 300;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Cloud {
    Xai,
    OpenAi,
}

impl Cloud {
    fn label(self) -> &'static str {
        match self {
            Cloud::Xai => "xAI",
            Cloud::OpenAi => "OpenAI",
        }
    }

    fn model_name(self, settings: &Settings) -> String {
        match self {
            Cloud::Xai => settings.model.clone(),
            Cloud::OpenAi => OPENAI_MODEL.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EphemeralToken {
    #[serde(skip)]
    pub value: String,
    pub expires_at: Option<u64>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TranscriptEvent<'a> {
    id: &'a str,
    role: &'static str,
    text: Option<&'a str>,
    delta: Option<&'a str>,
    is_final: bool,
}

pub fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .redirect(crate::http::api_redirect())
        .build()
        .expect("http client")
}

pub async fn mint_ephemeral(client: &reqwest::Client, api_key: &str) -> Result<EphemeralToken> {
    let resp = client
        .post(CLIENT_SECRETS_URL)
        .bearer_auth(api_key)
        .json(&json!({ "expires_after": { "seconds": TOKEN_TTL_SECS } }))
        .send()
        .await?;
    let status = resp.status();
    let text = resp.text().await?;
    let body: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    if !status.is_success() {
        let detail = body["error"]["message"]
            .as_str()
            .or(body["error"].as_str())
            .or(body["message"].as_str())
            .map(str::to_owned)
            .unwrap_or_else(|| text.chars().take(200).collect());
        let detail = crate::diagnostics::redact(&detail);
        return Err(msg(format!("xAI rejected the request ({status}): {detail}")));
    }
    let value = body["value"]
        .as_str()
        .or(body["client_secret"]["value"].as_str())
        .ok_or_else(|| msg("client_secrets response had no token value"))?
        .to_owned();
    let expires_at = unix_seconds(body["expires_at"].as_u64().or(body["client_secret"]["expires_at"].as_u64()));
    Ok(EphemeralToken { value, expires_at })
}

pub async fn mint_openai(client: &reqwest::Client, api_key: &str) -> Result<EphemeralToken> {
    let resp = client
        .post(OPENAI_CLIENT_SECRETS_URL)
        .bearer_auth(api_key)
        .json(&json!({
            "expires_after": { "anchor": "created_at", "seconds": TOKEN_TTL_SECS },
            "session": { "type": "realtime", "model": OPENAI_MODEL }
        }))
        .send()
        .await?;
    let status = resp.status();
    let text = resp.text().await?;
    let body: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    if !status.is_success() {
        let detail = body["error"]["message"]
            .as_str()
            .or(body["error"].as_str())
            .or(body["message"].as_str())
            .map(str::to_owned)
            .unwrap_or_else(|| text.chars().take(200).collect());
        let detail = crate::diagnostics::redact(&detail);
        return Err(msg(format!("OpenAI rejected the request ({status}): {detail}")));
    }
    let value = body["value"]
        .as_str()
        .or(body["client_secret"]["value"].as_str())
        .ok_or_else(|| msg("OpenAI client secret response had no token value"))?
        .to_owned();
    let expires_at = unix_seconds(body["expires_at"].as_u64().or(body["client_secret"]["expires_at"].as_u64()));
    Ok(EphemeralToken { value, expires_at })
}

/// xAI returns a unix timestamp. Accept either seconds or milliseconds.
fn unix_seconds(raw: Option<u64>) -> Option<u64> {
    raw.map(|n| if n > 10_000_000_000 { n / 1000 } else { n })
}

pub async fn run_session(
    app: AppHandle,
    mut mic: mpsc::Receiver<Vec<i16>>,
    player: Arc<Player>,
    active: Arc<AtomicBool>,
    settings: Settings,
    wake: bool,
) {
    let hub = app.state::<Arc<Hub>>().inner().clone();
    let result = Session { app: &app, hub: &hub, player: &player, active: &active, settings: &settings, wake }
        .run(&mut mic)
        .await;
    match result {
        Ok(reason) => hub.info(format!("session ended: {reason}")),
        Err(e) => hub.error(format!("session failed: {e}")),
    }
    hub.update(|s| {
        s.ws_state = "closed".into();
        s.provider = None;
    });
    hub.emit("voice://provider", "");
    // Dropping `mic` closes the channel; the engine notices and returns to wake listening.
}

struct UsageEnd<'a>(&'a AppHandle);

impl Drop for UsageEnd<'_> {
    fn drop(&mut self) {
        self.0.state::<crate::usage::UsageStore>().end();
    }
}

struct Session<'a> {
    app: &'a AppHandle,
    hub: &'a Hub,
    player: &'a Player,
    active: &'a AtomicBool,
    settings: &'a Settings,
    /// Kept so a wake start and a tap still share this session type.
    /// Provider order no longer depends on it: prefer-on-device applies to both.
    #[allow(dead_code)]
    wake: bool,
}

impl Session<'_> {
    fn phase(&self, p: Phase) {
        if self.active.load(Ordering::Acquire) {
            self.hub.set_phase(p);
        }
    }

    async fn run(&self, mic: &mut mpsc::Receiver<Vec<i16>>) -> Result<String> {
        self.phase(Phase::Connecting);
        self.hub.update(|s| s.ws_state = "minting token".into());
        if let Some(guard) = self.app.state::<crate::usage::UsageStore>().blocked(caps_of(self.settings), &self.settings.locale) {
            let message = guard.message.clone().unwrap_or_else(|| {
                format!("Today's voice limit of {} minutes is used up.", guard.budget_minutes)
            });
            self.emit_spend(&message, guard.minutes, guard.budget_minutes, true);
            return Ok(message);
        }
        self.app.state::<crate::usage::UsageStore>().begin();
        let _usage_end = UsageEnd(self.app);

        let app = self.app.clone();
        let accounts = tokio::task::spawn_blocking(move || {
            app.state::<Vault>().accounts(&app.state::<NativeBridge>())
        })
        .await
        .map_err(|e| msg(e.to_string()))??;
        if accounts.is_empty() {
            return Err(msg("No voice key saved. Add one in setup."));
        }

        let ready = crate::local::model_dir(self.app).map(|dir| crate::local::packs_ready(&dir)).unwrap_or(false);
        let providers: Vec<&str> = accounts.iter().map(|account| account.provider.as_str()).collect();
        let order = crate::local::attempt_order(&providers, self.settings.prefer_local_on_wake, ready);
        let http = http_client();
        let mut last_error = String::from("No voice provider connected.");
        let mut tried: Vec<String> = Vec::new();
        for index in order {
            let account = &accounts[index];
            let label = crate::local::provider_label(&account.provider);
            if !self.active.load(Ordering::Acquire) {
                return Ok("ended by user".into());
            }
            match crate::local::route(&account.provider, ready) {
                crate::local::Route::Local => {
                    tried.push(label.into());
                    self.app.state::<crate::usage::UsageStore>().note_provider(&account.provider);
                    self.announce(label);
                    self.hub.info("using on-device voice");
                    match crate::local::run_session(self.app, mic, self.player, self.active, self.settings, self.hub).await {
                        Ok(reason) => return Ok(reason),
                        Err(err) => {
                            last_error = err;
                            self.hub.error(format!("On-device voice failed: {last_error}"));
                            self.hub.warn(format!("on device failed, trying the next key: {last_error}"));
                        }
                    }
                }
                crate::local::Route::Xai | crate::local::Route::OpenAi => {
                    let cloud = if account.provider == "openai" { Cloud::OpenAi } else { Cloud::Xai };
                    tried.push(cloud.label().into());
                    self.app.state::<crate::usage::UsageStore>().note_provider(&account.provider);
                    self.announce(cloud.label());
                    let minted = match cloud {
                        Cloud::Xai => mint_ephemeral(&http, &account.secret).await,
                        Cloud::OpenAi => mint_openai(&http, &account.secret).await,
                    };
                    match minted {
                        Ok(token) => {
                            self.hub.info(format!("using {}", cloud.label()));
                            match self.run_cloud(mic, token, cloud).await {
                                Ok(reason) => return Ok(reason),
                                Err(err) => {
                                    if !self.active.load(Ordering::Acquire) {
                                        return Ok("ended by user".into());
                                    }
                                    last_error = err.to_string();
                                    self.hub.warn(format!("{} failed, trying the next key: {last_error}", cloud.label()));
                                }
                            }
                        }
                        Err(err) => {
                            last_error = err.to_string();
                            self.hub.warn(format!("{} failed, trying the next key: {last_error}", cloud.label()));
                        }
                    }
                }
                crate::local::Route::Skip(reason) => {
                    self.hub.warn(format!("{label} skipped: {reason}"));
                    if tried.is_empty() {
                        last_error = reason.clone();
                        if account.provider == "local" {
                            self.hub.error(reason);
                        }
                    }
                }
            }
        }
        let last_error = crate::diagnostics::redact(&last_error);
        let message = crate::local::exhausted_message(&tried, &last_error);
        self.speak_failure(&message).await;
        Err(msg(message))
    }

    fn announce(&self, label: &str) {
        self.hub.update(|state| state.provider = Some(label.to_string()));
        self.hub.emit("voice://provider", label.to_string());
    }

    async fn speak_failure(&self, last_error: &str) {
        let text: String = last_error.chars().take(180).collect();
        let app = self.app.clone();
        let _ = tokio::task::spawn_blocking(move || app.state::<NativeBridge>().speak_fallback(&text)).await;
    }

    async fn run_cloud(&self, mic: &mut mpsc::Receiver<Vec<i16>>, token: EphemeralToken, cloud: Cloud) -> Result<String> {
        let model = crate::settings::safe_model_name(&cloud.model_name(self.settings));
        self.hub.update(|s| {
            s.token_minted_at_ms = Some(now_ms());
            s.token_expires_at = token.expires_at;
            s.model = Some(model.clone());
        });
        self.hub.info("ephemeral token minted on-device");

        let url = match cloud {
            Cloud::Xai => format!("{REALTIME_URL}?model={model}"),
            Cloud::OpenAi => format!("{OPENAI_REALTIME_URL}?model={model}"),
        };
        let mut req = url.as_str().into_client_request()?;
        req.headers_mut().insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", token.value)).map_err(|e| msg(e.to_string()))?,
        );
        self.hub.update(|s| s.ws_state = "connecting".into());
        let (ws, _resp) = tokio::time::timeout(Duration::from_secs(12), tokio_tungstenite::connect_async(req))
            .await
            .map_err(|_| msg(format!("timed out connecting to {url}")))??;
        self.hub.update(|s| s.ws_state = "open".into());
        self.hub.info(format!("websocket open ({model})"));

        let (mut tx, mut rx) = ws.split();
        tx.send(Message::Text(self.session_update(cloud).to_string())).await?;

        let mut down_rate = self.player.rate();
        let mut down = Resampler::new(OUTPUT_RATE, down_rate);
        let mut last_activity = Instant::now();
        let mut response_active = false;
        let mut speaking = false;
        let idle = Duration::from_secs(self.settings.idle_timeout_secs);
        let mut tick = tokio::time::interval(Duration::from_millis(200));
        let mut spend_check = Instant::now();
        let mut logged_uplink = false;
        let mut configured = false;
        let mut heard_audio = false;
        let mut held: Vec<Vec<i16>> = Vec::new();
        let mut pending_tools: Vec<(String, String, String)> = Vec::new();
        let mut tool_followup = false;

        loop {
            tokio::select! {
                frame = mic.recv() => {
                    let Some(frame) = frame else {
                        let _ = tx.send(Message::Close(None)).await;
                        return Ok("ended by user".into());
                    };
                    if !configured {
                        // session.update clears the server buffer, so audio sent before
                        // session.updated never reaches VAD.
                        if held.len() < 200 {
                            held.push(frame);
                        }
                        continue;
                    }
                    Self::append_pcm(&self.hub, &mut tx, &frame, &mut logged_uplink).await?;
                }
                msg_in = rx.next() => {
                    let Some(msg_in) = msg_in else {
                        return self.retry_connect(heard_audio, "connection closed");
                    };
                    match msg_in? {
                        Message::Text(text) => {
                            let ev: Value = match serde_json::from_str(&text) {
                                Ok(v) => v,
                                Err(_) => continue,
                            };
                            let kind = ev["type"].as_str().unwrap_or("").to_owned();
                            self.hub.update(|s| { s.events_received += 1; s.last_event = Some(kind.clone()); });
                            match kind.as_str() {
                                "session.created" | "conversation.created" => self.hub.info(kind.clone()),
                                "session.updated" => {
                                    self.hub.info("session configured");
                                    self.phase(Phase::Listening);
                                    last_activity = Instant::now();
                                    configured = true;
                                    for frame in held.drain(..) {
                                        Self::append_pcm(&self.hub, &mut tx, &frame, &mut logged_uplink).await?;
                                    }
                                }
                                "input_audio_buffer.speech_started" => {
                                    self.hub.info("speech started");
                                    self.player.clear();
                                    speaking = false;
                                    last_activity = Instant::now();
                                    self.phase(Phase::Listening);
                                }
                                "input_audio_buffer.speech_stopped" => {
                                    last_activity = Instant::now();
                                    self.phase(Phase::Thinking);
                                }
                                "input_audio_buffer.committed" => {
                                    if let Some(id) = ev["item_id"].as_str() {
                                        self.transcript(id, "user", Some(""), None, false);
                                    }
                                }
                                "conversation.item.input_audio_transcription.delta" => {
                                    let id = ev["item_id"].as_str().unwrap_or("user");
                                    self.transcript(id, "user", None, ev["delta"].as_str(), false);
                                }
                                "conversation.item.input_audio_transcription.updated"
                                | "conversation.item.input_audio_transcription.completed" => {
                                    let id = ev["item_id"].as_str().unwrap_or("user");
                                    let text = ev["transcript"].as_str().unwrap_or("");
                                    self.transcript(id, "user", Some(text), None, kind.ends_with("completed"));
                                }
                                "response.created" => {
                                    response_active = true;
                                    self.phase(Phase::Thinking);
                                }
                                "response.output_audio.delta" | "response.audio.delta" => {
                                    if let Some(b64) = ev["delta"].as_str() {
                                        let bytes = B64.decode(b64).unwrap_or_default();
                                        let pcm: Vec<f32> = bytes
                                            .chunks_exact(2)
                                            .map(|b| i16_to_f32(i16::from_le_bytes([b[0], b[1]])))
                                            .collect();
                                        if self.player.rate() != down_rate {
                                            down_rate = self.player.rate();
                                            down = Resampler::new(OUTPUT_RATE, down_rate);
                                        }
                                        let mut out = Vec::with_capacity(pcm.len() * 2);
                                        down.process(&pcm, &mut out);
                                        self.player.push(&out);
                                        if !bytes.is_empty() {
                                            heard_audio = true;
                                        }
                                        let n = bytes.len() as u64;
                                        self.hub.update(|s| { s.audio_deltas += 1; s.bytes_received += n; });
                                        if !speaking {
                                            speaking = true;
                                            self.phase(Phase::Speaking);
                                        }
                                    }
                                }
                                "response.output_audio_transcript.delta" | "response.audio_transcript.delta" => {
                                    let id = ev["response_id"].as_str().or(ev["item_id"].as_str()).unwrap_or("assistant");
                                    self.transcript(id, "assistant", None, ev["delta"].as_str(), false);
                                }
                                "response.output_audio_transcript.done" | "response.audio_transcript.done" => {
                                    let id = ev["response_id"].as_str().or(ev["item_id"].as_str()).unwrap_or("assistant");
                                    self.transcript(id, "assistant", ev["transcript"].as_str(), None, true);
                                }
                                "response.function_call_arguments.done" => {
                                    let call_id = ev["call_id"].as_str().unwrap_or("").to_string();
                                    let name = ev["name"].as_str().unwrap_or("").to_string();
                                    let arguments = match ev.get("arguments") {
                                        Some(Value::String(s)) => s.clone(),
                                        Some(other) if !other.is_null() => other.to_string(),
                                        _ => "{}".into(),
                                    };
                                    if !call_id.is_empty() && !pending_tools.iter().any(|(id, _, _)| id == &call_id) {
                                        let output = self.invoke_tool(&name, &arguments).await;
                                        pending_tools.push((call_id, name, output));
                                        if !response_active {
                                            tool_followup = true;
                                        }
                                    }
                                    last_activity = Instant::now();
                                }
                                "response.done" => {
                                    if let Some(items) = ev["response"]["output"].as_array() {
                                        let calls: Vec<(String, String, String)> = items
                                            .iter()
                                            .filter(|item| item["type"] == "function_call")
                                            .filter_map(|item| {
                                                let call_id = item["call_id"].as_str().unwrap_or("").to_string();
                                                if call_id.is_empty() || pending_tools.iter().any(|(id, _, _)| id == &call_id) {
                                                    return None;
                                                }
                                                let name = item["name"].as_str().unwrap_or("").to_string();
                                                let arguments = item["arguments"].as_str().unwrap_or("{}").to_string();
                                                Some((call_id, name, arguments))
                                            })
                                            .collect();
                                        for (call_id, name, arguments) in calls {
                                            let output = self.invoke_tool(&name, &arguments).await;
                                            pending_tools.push((call_id, name, output));
                                        }
                                    }
                                    response_active = false;
                                    last_activity = Instant::now();
                                    if !pending_tools.is_empty() {
                                        tool_followup = true;
                                    }
                                    if let Some(status) = ev["response"]["status"].as_str() {
                                        if status == "failed" {
                                            let why = ev["response"]["status_details"].to_string();
                                            if !heard_audio && crate::local::retries_on_connect(&why) {
                                                return Err(msg(format!("{}: {why}", cloud.label())));
                                            }
                                            self.hub.error(format!("response failed: {why}"));
                                        }
                                    }
                                }
                                "error" => {
                                    let m = ev["error"]["message"].as_str().unwrap_or("unknown realtime error");
                                    let line = format!("{}: {m}", cloud.label());
                                    if !heard_audio && (crate::local::retries_on_connect(m) || !configured) {
                                        return Err(msg(line));
                                    }
                                    self.hub.error(line);
                                }
                                _ => {}
                            }
                        }
                        Message::Close(frame) => {
                            let why = frame.map(|f| format!("{} {}", f.code, f.reason)).unwrap_or_default();
                            return self.retry_connect(heard_audio, &format!("server closed {why}"));
                        }
                        _ => {}
                    }
                }
                _ = tick.tick() => {
                    if !self.active.load(Ordering::Acquire) {
                        let _ = tx.send(Message::Close(None)).await;
                        return Ok("ended by user".into());
                    }
                    if spend_check.elapsed() >= Duration::from_secs(20) {
                        spend_check = Instant::now();
                        let live = self.app.state::<crate::settings::SettingsStore>().get();
                        let guard = self.app.state::<crate::usage::UsageStore>().poll(caps_of(&live), &live.locale);
                        if let Some(message) = guard.message.clone() {
                            self.emit_spend(&message, guard.minutes, guard.budget_minutes, guard.block);
                            if !guard.block && configured && !response_active && !self.player.is_active() {
                                let say = json!({
                                    "type": "response.create",
                                    "response": { "instructions": format!("Say only this, then wait: {message}") }
                                });
                                if tx.send(Message::Text(say.to_string())).await.is_err() {
                                    return self.retry_connect(heard_audio, "connection closed");
                                }
                                response_active = true;
                                last_activity = Instant::now();
                                self.phase(Phase::Thinking);
                            }
                        }
                        if guard.block {
                            let _ = tx.send(Message::Close(None)).await;
                            return Ok("daily voice limit".into());
                        }
                    }
                    let playing = self.player.is_active();
                    if speaking && !playing && !response_active {
                        speaking = false;
                        last_activity = Instant::now();
                        self.phase(Phase::Listening);
                    }
                    let mut pending_reply = false;
                    for note in crate::search::take_context_notes() {
                        let pending = note.trim().strip_prefix("pending:").map(str::trim).filter(|text| !text.is_empty());
                        let body = if let Some(text) = pending {
                            pending_reply = true;
                            text.to_string()
                        } else {
                            format!("Typed just now. Use this and do not answer it by itself. {note}")
                        };
                        let item = json!({
                            "type": "conversation.item.create",
                            "item": {
                                "type": "message",
                                "role": "user",
                                "content": [{ "type": "input_text", "text": body }]
                            }
                        });
                        if tx.send(Message::Text(item.to_string())).await.is_err() {
                            return self.retry_connect(heard_audio, "connection closed");
                        }
                    }
                    if pending_reply && !response_active && !playing {
                        if tx.send(Message::Text(json!({ "type": "response.create" }).to_string())).await.is_err() {
                            return self.retry_connect(heard_audio, "connection closed");
                        }
                        response_active = true;
                        last_activity = Instant::now();
                        self.phase(Phase::Thinking);
                    }
                    if tool_followup && !response_active && !playing {
                        let mut searched = false;
                        for (call_id, name, output) in pending_tools.drain(..) {
                            searched |= name == "web_search";
                            let ev = json!({
                                "type": "conversation.item.create",
                                "item": { "type": "function_call_output", "call_id": call_id, "output": output }
                            });
                            if tx.send(Message::Text(ev.to_string())).await.is_err() {
                                return self.retry_connect(heard_audio, "connection closed");
                            }
                        }
                        let follow = if searched {
                            json!({
                                "type": "response.create",
                                "response": {
                                    "instructions": "Speak the web search result now in a few sentences. If it names a place, use that place. Do not ask the user to repeat a city or fact already in the result or the typed conversation. Do not say you are still checking."
                                }
                            })
                        } else {
                            json!({ "type": "response.create" })
                        };
                        if tx.send(Message::Text(follow.to_string())).await.is_err() {
                            return self.retry_connect(heard_audio, "connection closed");
                        }
                        tool_followup = false;
                        response_active = true;
                        last_activity = Instant::now();
                        self.phase(Phase::Thinking);
                    }
                    if configured && !playing && !response_active && !tool_followup && last_activity.elapsed() >= idle {
                        let _ = tx.send(Message::Close(None)).await;
                        return Ok(format!("idle for {}s", idle.as_secs()));
                    }
                }
            }
        }
    }

    fn retry_connect(&self, heard_audio: bool, reason: &str) -> Result<String> {
        if !self.active.load(Ordering::Acquire) || reason == "ended by user" || reason == "daily voice limit" || heard_audio {
            return Ok(reason.to_string());
        }
        Err(msg(reason))
    }

    async fn invoke_tool(&self, name: &str, arguments: &str) -> String {
        if name == "web_search" {
            self.hub.emit("voice://busy", "Searching the web…");
            self.phase(Phase::Thinking);
        }
        let app = self.app.clone();
        let tool_name = name.to_string();
        let arguments = arguments.to_string();
        let output = match tokio::task::spawn_blocking(move || actions::run(&app, &tool_name, &arguments)).await {
            Ok(output) => output,
            Err(err) => json!({ "ok": false, "error": err.to_string() }).to_string(),
        };
        if name == "web_search" {
            self.hub.emit("voice://busy", "");
        }
        output
    }

    async fn append_pcm(
        hub: &Hub,
        tx: &mut futures_util::stream::SplitSink<
            tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
            Message,
        >,
        frame: &[i16],
        logged_uplink: &mut bool,
    ) -> Result<()> {
        let mut bytes = Vec::with_capacity(frame.len() * 2);
        for s in frame {
            bytes.extend_from_slice(&s.to_le_bytes());
        }
        let n = bytes.len() as u64;
        let ev = json!({ "type": "input_audio_buffer.append", "audio": B64.encode(&bytes) });
        tx.send(Message::Text(ev.to_string())).await?;
        hub.update(|s| {
            s.frames_sent += 1;
            s.bytes_sent += n;
        });
        if !*logged_uplink && bytes.iter().any(|b| *b != 0) {
            *logged_uplink = true;
            hub.info("microphone audio is reaching the session");
        }
        Ok(())
    }

    fn session_update(&self, cloud: Cloud) -> Value {
        let voice = match cloud {
            Cloud::Xai => self.settings.voice.as_str(),
            Cloud::OpenAi => self.settings.openai_voice.as_str(),
        };
        cloud_session(cloud, voice, &self.spoken_instructions(), &actions::tool_definitions())
    }

    fn spoken_instructions(&self) -> String {
        let mut s = self.settings.instructions.trim().to_string();
        s.push_str(
            "\n\nIf a tool returns an error, speak that error in one short sentence. You can search the live web with web_search. For news, weather, sports, prices, or anything about today, call web_search first. When the result comes back, speak that summary. Do not ask for a city or fact the user already gave, including in the typed conversation. media_control and set_volume act immediately on whatever is playing. play_music uses the default music player immediately, with no confirmation. play_local_music is only for files stored on this phone. If they say YouTube, call open_youtube instead and wait for a clear yes. list_finance reads money saved on this phone, including balances, net worth, card debt, and the next 30 days. log_money records or edits an expense, income, or card payment. save_account saves a bank, cash wallet, card, loan, or investment. save_schedule and save_recurring edit plans, including the amount for future occurrences only. Do not invent a balance, a due date, or an account number. For a question about a person, call get_person and speak the answer. That includes preferred channel, birthday, anniversary, note, nickname, and favorite. get_person does not open any app. Do not call find_contact or any open_ tool for those questions. Call find_contact only when they want to reach someone. It resolves People nicknames, aliases, and favorites along with phone contacts. If preferredChannel is set, use it even when they said call, text, or email: call is open_call, sms is open_sms, whatsapp is open_whatsapp, signal is open_signal, email is open_email. Pass channelHandle as phone or to. For open_browser, open_youtube, open_whatsapp, open_signal, open_email, open_call, and open_sms, say exactly what will open and wait for a clear yes before calling with confirmed true. Nothing is sent, and a call is not placed, until the user confirms in that app. Add a person with create_person and change a name, note, channel, or date with update_person. Delete a person only after a clear yes, then call delete_person with confirmed true. Read settings with get_settings and change them with update_settings. Do not say you cannot edit People or settings. Never save or repeat an API key or backup password. Save a birthday or anniversary with set_person_reminder or create_person. A repeating reminder uses set_reminder with recurrence daily or weekly, time HH:MM, and weekdays such as mon,wed,fri. Leave recurrence off for a one-off. snooze_reminder delays the reminder that just rang, or an id they name. If it says to ask which one, ask. You can also use these phone tools: create_note, list_notes, update_note, delete_note, set_alarm, set_timer, set_reminder, list_reminders, update_reminder, cancel_reminder, snooze_reminder, get_person, list_people, set_person_reminder, create_person, update_person, delete_person, get_settings, update_settings, list_calendar_events, create_calendar_event, update_calendar_event, delete_calendar_event, log_money, save_recurring, save_schedule, save_account, list_finance, delete_finance. Always list first when the user wants to change or delete something, then use the id. Call the tool before saying it is done, then confirm in one short sentence. Reminders, alarms, and countdowns are saved in this app's Time list. Calendar events are saved in this app's Calendar. inMinutes or inHours means from now. A clock time is 7pm, tomorrow at 9, or YYYY-MM-DDTHH:MM. Alarms use hour 0-23 and minute. Timers use seconds.",
        );
        s.push_str(" share_item sends a note, reminder, or event as a draft after a clear yes. read_notifications, set_dnd, go_home, and phone_status run on this phone. Do not guess the battery or the next alarm.");
        s.push_str(crate::actions::lookup_hint());
        s.push_str(&self.app.state::<crate::memory::Memory>().prompt());
        s
    }

    fn emit_spend(&self, message: &str, minutes: u32, budget_minutes: u32, blocked: bool) {
        self.hub.emit(
            "voice://spend",
            json!({
                "message": message,
                "minutes": minutes,
                "budgetMinutes": budget_minutes,
                "blocked": blocked,
            }),
        );
    }

    fn transcript(&self, id: &str, role: &'static str, text: Option<&str>, delta: Option<&str>, is_final: bool) {
        if is_final {
            if let Some(text) = text.filter(|text| !text.trim().is_empty()) {
                self.app.state::<crate::memory::Memory>().remember(role, text);
            }
        }
        self.hub.emit("voice://transcript", TranscriptEvent { id, role, text, delta, is_final });
    }
}

fn caps_of(settings: &crate::settings::Settings) -> crate::usage::Caps {
    crate::usage::Caps {
        daily_minutes: settings.daily_budget_minutes,
        daily_hard_stop: settings.daily_budget_hard_stop,
        monthly_minutes: settings.monthly_budget_minutes,
        monthly_hard_stop: settings.monthly_budget_hard_stop,
    }
}

fn cloud_session(cloud: Cloud, voice: &str, instructions: &str, tools: &Value) -> Value {
    match cloud {
        Cloud::Xai => json!({
            "type": "session.update",
            "session": {
                "voice": crate::voices::xai_voice(voice),
                "instructions": instructions,
                "tools": tools,
                "turn_detection": {
                    "type": "server_vad",
                    "threshold": 0.2,
                    "prefix_padding_ms": 400,
                    "silence_duration_ms": 700
                },
                "audio": {
                    "input": {
                        "format": { "type": "audio/pcm", "rate": UPLINK_RATE },
                        "transcription": { "model": "grok-transcribe" }
                    },
                    "output": {
                        "format": { "type": "audio/pcm", "rate": OUTPUT_RATE }
                    }
                }
            }
        }),
        Cloud::OpenAi => json!({
            "type": "session.update",
            "session": {
                "type": "realtime",
                "instructions": instructions,
                "tools": tools,
                "tool_choice": "auto",
                "audio": {
                    "input": {
                        "format": { "type": "audio/pcm", "rate": OUTPUT_RATE },
                        "transcription": { "model": "gpt-4o-mini-transcribe" },
                        "turn_detection": {
                            "type": "server_vad",
                            "threshold": 0.5,
                            "prefix_padding_ms": 300,
                            "silence_duration_ms": 500
                        }
                    },
                    "output": {
                        "format": { "type": "audio/pcm", "rate": OUTPUT_RATE },
                        "voice": crate::voices::openai_voice(voice)
                    }
                }
            }
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openai_session_uses_24khz_pcm_and_a_known_voice() {
        let tools = json!([{ "type": "function", "name": "create_note" }]);
        let session = cloud_session(Cloud::OpenAi, "eve", "Be brief.", &tools);
        assert_eq!(session["session"]["type"], "realtime");
        assert_eq!(session["session"]["audio"]["input"]["format"]["rate"], 24_000);
        assert_eq!(session["session"]["audio"]["input"]["format"]["type"], "audio/pcm");
        assert_eq!(session["session"]["audio"]["output"]["format"]["rate"], 24_000);
        assert_eq!(session["session"]["audio"]["output"]["voice"], "marin");
        assert_eq!(session["session"]["audio"]["input"]["turn_detection"]["type"], "server_vad");
        assert_eq!(session["session"]["tools"][0]["name"], "create_note");
        let named = cloud_session(Cloud::OpenAi, "cedar", "Be brief.", &tools);
        assert_eq!(named["session"]["audio"]["output"]["voice"], "cedar");
        let crossed = cloud_session(Cloud::OpenAi, "ara", "Be brief.", &tools);
        assert_eq!(crossed["session"]["audio"]["output"]["voice"], "marin");
    }

    #[test]
    fn xai_session_keeps_the_existing_talk_loop() {
        let tools = json!([]);
        let session = cloud_session(Cloud::Xai, "eve", "Be brief.", &tools);
        assert_eq!(session["session"]["voice"], "eve");
        assert_eq!(cloud_session(Cloud::Xai, "Ara", "Be brief.", &tools)["session"]["voice"], "ara");
        assert_eq!(cloud_session(Cloud::Xai, "cedar", "Be brief.", &tools)["session"]["voice"], "eve");
        assert_eq!(session["session"]["audio"]["input"]["transcription"]["model"], "grok-transcribe");
        assert_eq!(session["session"]["audio"]["input"]["format"]["rate"], 24_000);
        assert_eq!(session["session"]["audio"]["output"]["format"]["rate"], 24_000);
        assert_eq!(session["session"]["turn_detection"]["type"], "server_vad");
        assert_eq!(session["session"]["instructions"], "Be brief.");
        assert!(!session.to_string().contains("remember_fact"));
        assert!(!session.to_string().contains("Pinned facts"));
    }
}
