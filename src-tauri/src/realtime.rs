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
const OUTPUT_RATE: u32 = 24_000;
const TOKEN_TTL_SECS: u64 = 300;

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
        .build()
        .expect("http client")
}

async fn connect_account(
    http: &reqwest::Client,
    provider: &str,
    secret: &str,
    _model: &str,
) -> Result<EphemeralToken> {
    match provider {
        "xai" => mint_ephemeral(http, secret).await,
        "local" => Err(msg(
            "on device is saved on this phone, and its voice connection is not ready yet",
        )),
        other => Err(msg(format!(
            "{other} is saved on this phone, and its voice connection is not ready yet"
        ))),
    }
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
) {
    let hub = app.state::<Arc<Hub>>().inner().clone();
    let result = Session { app: &app, hub: &hub, player: &player, active: &active, settings: &settings }
        .run(&mut mic)
        .await;
    match result {
        Ok(reason) => hub.info(format!("session ended: {reason}")),
        Err(e) => hub.error(format!("session failed: {e}")),
    }
    hub.update(|s| s.ws_state = "closed".into());
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
        if let Some(guard) = self.app.state::<crate::usage::UsageStore>().blocked(
            self.settings.daily_budget_minutes,
            self.settings.daily_budget_hard_stop,
        ) {
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

        let http = http_client();
        let mut token = None;
        let mut last_error = String::from("no voice provider connected");
        for account in &accounts {
            match connect_account(&http, &account.provider, &account.secret, &self.settings.model).await {
                Ok(minted) => {
                    self.hub.info(format!("using {} ({})", account.provider, crate::vault::key_hint(&account.secret)));
                    token = Some(minted);
                    break;
                }
                Err(e) => {
                    last_error = e.to_string();
                    self.hub.warn(format!("{} failed, trying the next key: {last_error}", account.provider));
                }
            }
        }
        let token = token.ok_or_else(|| msg(last_error))?;
        self.hub.update(|s| {
            s.token_minted_at_ms = Some(now_ms());
            s.token_expires_at = token.expires_at;
            s.model = Some(self.settings.model.clone());
        });
        self.hub.info("ephemeral token minted on-device");

        let url = format!("{REALTIME_URL}?model={}", self.settings.model);
        let mut req = url.as_str().into_client_request()?;
        req.headers_mut().insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", token.value)).map_err(|e| msg(e.to_string()))?,
        );
        self.hub.update(|s| s.ws_state = "connecting".into());
        let (ws, _resp) = tokio::time::timeout(Duration::from_secs(12), tokio_tungstenite::connect_async(req))
            .await
            .map_err(|_| msg("timed out connecting to wss://api.x.ai"))??;
        self.hub.update(|s| s.ws_state = "open".into());
        self.hub.info(format!("websocket open ({})", self.settings.model));

        let (mut tx, mut rx) = ws.split();
        tx.send(Message::Text(self.session_update().to_string())).await?;

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
                    let Some(msg_in) = msg_in else { return Ok("connection closed".into()) };
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
                                    if name == "web_search" {
                                        self.hub.emit("voice://busy", "Searching the web…");
                                        self.phase(Phase::Thinking);
                                    }
                                    let app = self.app.clone();
                                    let tool_name = name.clone();
                                    let output = match tokio::task::spawn_blocking(move || actions::run(&app, &tool_name, &arguments)).await {
                                        Ok(s) => s,
                                        Err(e) => json!({ "ok": false, "error": e.to_string() }).to_string(),
                                    };
                                    if name == "web_search" {
                                        self.hub.emit("voice://busy", "");
                                    }
                                    if !call_id.is_empty() {
                                        pending_tools.push((call_id, name, output));
                                        if !response_active {
                                            tool_followup = true;
                                        }
                                    }
                                    last_activity = Instant::now();
                                }
                                "response.done" => {
                                    response_active = false;
                                    last_activity = Instant::now();
                                    if !pending_tools.is_empty() {
                                        tool_followup = true;
                                    }
                                    if let Some(status) = ev["response"]["status"].as_str() {
                                        if status == "failed" {
                                            let why = ev["response"]["status_details"].to_string();
                                            self.hub.error(format!("response failed: {why}"));
                                        }
                                    }
                                }
                                "error" => {
                                    let m = ev["error"]["message"].as_str().unwrap_or("unknown realtime error");
                                    self.hub.error(format!("xAI: {m}"));
                                }
                                _ => {}
                            }
                        }
                        Message::Close(frame) => {
                            let why = frame.map(|f| format!("{} {}", f.code, f.reason)).unwrap_or_default();
                            return Ok(format!("server closed {why}"));
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
                        let guard = self.app.state::<crate::usage::UsageStore>().poll(
                            live.daily_budget_minutes,
                            live.daily_budget_hard_stop,
                        );
                        if let Some(message) = guard.message.clone() {
                            self.emit_spend(&message, guard.minutes, guard.budget_minutes, guard.block);
                            if !guard.block && configured && !response_active && !self.player.is_active() {
                                let say = json!({
                                    "type": "response.create",
                                    "response": { "instructions": format!("Say only this, then wait: {message}") }
                                });
                                if tx.send(Message::Text(say.to_string())).await.is_err() {
                                    return Ok("connection closed".into());
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
                    for note in crate::search::take_context_notes() {
                        let item = json!({
                            "type": "conversation.item.create",
                            "item": {
                                "type": "message",
                                "role": "user",
                                "content": [{ "type": "input_text", "text": format!("Typed just now. Use this and do not answer it by itself. {note}") }]
                            }
                        });
                        if tx.send(Message::Text(item.to_string())).await.is_err() {
                            return Ok("connection closed".into());
                        }
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
                                return Ok("connection closed".into());
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
                            return Ok("connection closed".into());
                        }
                        tool_followup = false;
                        response_active = true;
                        last_activity = Instant::now();
                        self.phase(Phase::Thinking);
                    }
                    if !playing && !response_active && !tool_followup && last_activity.elapsed() >= idle {
                        let _ = tx.send(Message::Close(None)).await;
                        return Ok(format!("idle for {}s", idle.as_secs()));
                    }
                }
            }
        }
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

    fn session_update(&self) -> Value {
        json!({
            "type": "session.update",
            "session": {
                "voice": self.settings.voice,
                "instructions": self.spoken_instructions(),
                "tools": actions::tool_definitions(),
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
        })
    }

    fn spoken_instructions(&self) -> String {
        let mut s = self.settings.instructions.trim().to_string();
        s.push_str(
            "\n\nYou can search the live web with web_search. For news, weather, sports, prices, or anything about today, call web_search first. When the result comes back, speak that summary. Do not ask for a city or fact the user already gave, including in the typed conversation. media_control and set_volume act immediately on whatever is playing. For a question about a person, call get_person and speak the answer. That includes preferred channel, birthday, anniversary, note, nickname, and favorite. get_person does not open any app. Do not call find_contact or any open_ tool for those questions. Call find_contact only when they want to reach someone. It resolves People nicknames, aliases, and favorites along with phone contacts. If preferredChannel is set, use it even when they said call, text, or email: call is open_call, sms is open_sms, whatsapp is open_whatsapp, signal is open_signal, email is open_email. Pass channelHandle as phone or to. For open_browser, open_youtube, open_whatsapp, open_signal, open_email, open_call, and open_sms, say exactly what will open and wait for a clear yes before calling with confirmed true. Nothing is sent, and a call is not placed, until the user confirms in that app. Save a birthday or anniversary with set_person_reminder. A repeating reminder uses set_reminder with recurrence daily or weekly, time HH:MM, and weekdays such as mon,wed,fri. Leave recurrence off for a one-off. snooze_reminder delays the reminder that just rang, or an id they name. If it says to ask which one, ask. You can also use these phone tools: create_note, list_notes, update_note, delete_note, set_alarm, set_timer, set_reminder, list_reminders, update_reminder, cancel_reminder, snooze_reminder, get_person, set_person_reminder, list_calendar_events, create_calendar_event, update_calendar_event, delete_calendar_event. Always list first when the user wants to change or delete something, then use the id. Call the tool before saying it is done, then confirm in one short sentence. Times are local YYYY-MM-DDTHH:MM. Alarms use hour 0-23 and minute. Timers use seconds.",
        );
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
