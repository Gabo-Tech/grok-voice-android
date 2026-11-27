//! Utterance session: listen, transcribe, run phone tools, speak.

use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use serde::Serialize;
use tauri::{AppHandle, Manager};
use tokio::sync::mpsc;

use crate::{
    actions,
    audio::{
        engine::UPLINK_RATE,
        playback::Player,
        resample::{Resampler, i16_to_f32},
    },
    diagnostics::{Hub, Phase, now_ms},
    native::NativeBridge,
    settings::Settings,
};

use super::{
    engine::Engines,
    memory_enough_for,
    mode::set_public_search,
    packs::{model_dir, model_paths_for},
    required_free_mb,
    turn::{self, ChatTurn},
    vad::Vad,
};

/// German and Spanish each need their own packs. English falls through to the
/// caller's message, which is different for a typed turn and a live session.
fn optional_speech_missing(speech: &str) -> Option<&'static str> {
    match speech {
        "de" => Some(
            "German on-device speech is not downloaded. English Moonshine Base does not understand German. Download the German packs in Settings.",
        ),
        "es" => Some(
            "Spanish on-device speech is not downloaded. English Moonshine Base does not understand Spanish. Download the Spanish packs in Settings.",
        ),
        _ => None,
    }
}

struct SearchGuard;
impl Drop for SearchGuard {
    fn drop(&mut self) {
        set_public_search(false);
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Transcript<'a> {
    id: String,
    role: &'a str,
    text: &'a str,
    delta: Option<&'a str>,
    is_final: bool,
}

/// One typed turn when the phone is offline and the packs can load.
/// Notes, reminders, and People run through the same tool path as Talk.
pub fn answer_text(app: &AppHandle, text: &str) -> Result<String, String> {
    let dir = model_dir(app)?;
    let settings = app.state::<crate::settings::SettingsStore>().get();
    let speech = settings.local_speech.clone();
    if !super::speech::talk_ready(
        &dir,
        &speech,
        &settings.speech_engines,
        &settings.phonon_models,
    ) {
        return Err(optional_speech_missing(&speech)
            .unwrap_or(super::web::OFFLINE_VOICE)
            .into());
    }
    let facts = app
        .state::<NativeBridge>()
        .device_facts()
        .unwrap_or_default();
    let paths = model_paths_for(&dir, &speech);
    let family = super::packs::active_model(&dir).family;
    if let Err(why) = memory_gate(
        facts.avail_mb,
        facts.low_memory,
        model_file_bytes(&paths.llm),
    ) {
        return Err(why);
    }
    let threads = std::thread::available_parallelism()
        .map(|n| n.get().clamp(2, 4) as i32)
        .unwrap_or(2);
    let mut engines = Engines::load(&paths, threads)?;
    let system = local_system(app);
    let mut history = Vec::new();
    let spoken = turn::run_turns_with(
        &system,
        &mut history,
        text,
        &family,
        |prompt, grammar| engines.complete(prompt, grammar),
        |name, args| call_tool(app, name, args),
    )?;
    record_exchange(app, &history);
    let _ = app.state::<NativeBridge>().speak_fallback(&spoken);
    Ok(spoken)
}

pub struct RemoteBrain {
    pub key: String,
    pub model: String,
}

pub async fn run_onemin_session(
    app: &AppHandle,
    mic: &mut mpsc::Receiver<Vec<i16>>,
    player: &Player,
    active: &AtomicBool,
    settings: &Settings,
    hub: &Hub,
    key: String,
    model: String,
) -> Result<String, String> {
    run_session_inner(
        app,
        mic,
        player,
        active,
        settings,
        hub,
        Some(RemoteBrain { key, model }),
    )
    .await
}

pub async fn run_session(
    app: &AppHandle,
    mic: &mut mpsc::Receiver<Vec<i16>>,
    player: &Player,
    active: &AtomicBool,
    settings: &Settings,
    hub: &Hub,
) -> Result<String, String> {
    run_session_inner(app, mic, player, active, settings, hub, None).await
}

async fn run_session_inner(
    app: &AppHandle,
    mic: &mut mpsc::Receiver<Vec<i16>>,
    player: &Player,
    active: &AtomicBool,
    settings: &Settings,
    hub: &Hub,
    remote: Option<RemoteBrain>,
) -> Result<String, String> {
    let _guard = SearchGuard;
    set_public_search(true);
    let dir = model_dir(app)?;
    let chosen = super::packs::active_model(&dir);
    hub.update(|state| {
        state.ws_state = "on device".into();
        state.model = Some(remote.as_ref().map(|brain| brain.model.clone()).unwrap_or_else(|| chosen.name.clone()));
        state.token_minted_at_ms = None;
        state.token_expires_at = None;
    });
    if active.load(Ordering::Acquire) {
        hub.set_phase(Phase::Connecting);
    }
    let facts = device_facts(app).await;
    let speech = settings.local_speech.as_str();
    let speech_ok = if remote.is_some() {
        super::speech::speech_ready(&dir, speech, &settings.speech_engines, &settings.phonon_models)
    } else {
        super::speech::talk_ready(&dir, speech, &settings.speech_engines, &settings.phonon_models)
    };
    if !speech_ok {
        return Err(optional_speech_missing(speech).unwrap_or(
            if remote.is_some() {
                "On-device speech packs are not downloaded yet. 1min.ai still hears and speaks on this phone. Download speech to text and the voice in Settings."
            } else {
                "On-device voice packs are not downloaded yet. Download them in Settings, under Voice and AI."
            },
        ).into());
    }
    let paths = model_paths_for(&dir, speech);
    let family = chosen.family;
    let model_bytes = if remote.is_some() { 0 } else { model_file_bytes(&paths.llm) };
    if let Err(why) = memory_gate(facts.avail_mb, facts.low_memory, model_bytes) {
        return Err(why);
    }
    let threads = std::thread::available_parallelism()
        .map(|n| n.get().clamp(2, 4) as i32)
        .unwrap_or(2);
    let remote_model = remote.as_ref().map(|brain| brain.model.clone());
    hub.info(match &remote_model {
        Some(model) => format!("loading on-device speech and voice, reply from {model}"),
        None => format!("loading on-device speech, {}, and voice", chosen.name),
    });
    let with_llm = remote.is_none();
    let engines = tokio::task::spawn_blocking(move || {
        if with_llm {
            Engines::load(&paths, threads)
        } else {
            Engines::load_speech(&paths, threads)
        }
    })
    .await
    .map_err(|err| err.to_string())??;
    let engines = Arc::new(Mutex::new(engines));
    let system = if remote.is_some() { onemin_system(app) } else { local_system(app) };
    let mut history = Vec::new();
    let mut vad = Vad::new(UPLINK_RATE);
    let mut last = Instant::now();
    let idle = Duration::from_secs(settings.idle_timeout_secs.max(5));
    let mut tick = tokio::time::interval(Duration::from_millis(200));
    if active.load(Ordering::Acquire) {
        hub.set_phase(Phase::Listening);
    }
    hub.info("on-device voice is listening");

    let outcome = loop {
        tokio::select! {
            frame = mic.recv() => {
                let Some(frame) = frame else { break Ok("ended by user".into()) };
                if !active.load(Ordering::Acquire) {
                    break Ok("ended by user".into());
                }
                if let Some(utterance) = vad.push(&frame) {
                    if let Err(err) = handle_audio(app, hub, player, active, &engines, &system, &family, remote.as_ref(), &mut history, &utterance).await {
                        hub.error(format!("on-device turn failed: {err}"));
                        speak_line(app, hub, player, active, &engines, &format!("I couldn't finish that. {err}")).await;
                    }
                    while mic.try_recv().is_ok() {}
                    last = Instant::now();
                    if active.load(Ordering::Acquire) {
                        hub.set_phase(Phase::Listening);
                    }
                }
            }
            _ = tick.tick() => {
                if !active.load(Ordering::Acquire) {
                    break Ok("ended by user".into());
                }
                for note in crate::search::take_context_notes() {
                    let Some(text) = typed_user_note(&note) else { continue };
                    if let Err(err) = handle_text(app, hub, player, active, &engines, &system, &family, remote.as_ref(), &mut history, &text, true).await {
                        hub.error(format!("typed message failed: {err}"));
                    }
                    last = Instant::now();
                }
                if player.is_active() {
                    last = Instant::now();
                }
                if last.elapsed() >= idle {
                    break Ok(format!("idle for {}s", idle.as_secs()));
                }
            }
        }
    };
    record_exchange(app, &history);
    outcome
}

fn model_file_bytes(path: &std::path::Path) -> u64 {
    std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0)
}

fn memory_gate(avail_mb: u64, low_memory: bool, model_bytes: u64) -> Result<(), String> {
    if memory_enough_for(avail_mb, low_memory, model_bytes) {
        return Ok(());
    }
    if low_memory {
        return Err("This phone is low on memory, so the on-device voice cannot load.".into());
    }
    Err(format!(
        "This language model could not load. On-device voice needs about {} MB free, and this phone has {avail_mb} MB free.",
        required_free_mb(model_bytes)
    ))
}

/// A typed Talk message queued while On device is already listening.
/// Assistant notes are context only. The user note is the message to answer.
fn typed_user_note(note: &str) -> Option<String> {
    let note = note.trim();
    let text = note
        .strip_prefix("pending:")
        .or_else(|| note.strip_prefix("user:"))?
        .trim();
    if text.is_empty() {
        None
    } else {
        Some(text.to_string())
    }
}

fn onemin_system(app: &AppHandle) -> String {
    let settings = app.state::<crate::settings::SettingsStore>().get();
    format!(
        "{}\n\nYou are answering on this phone through 1min.ai. Speech to text and the voice stay on this phone. Reply in one to three short spoken sentences. Call a tool before saying a phone action is done. Never repeat an API key.",
        settings.instructions.trim()
    )
}

fn answer_with_onemin(app: &AppHandle, system: &str, text: &str, key: &str, model: &str) -> Result<String, String> {
    let mut tools = crate::actions::tool_definitions();
    if let Some(list) = tools.as_array_mut() {
        list.extend(crate::local_memory::tool_definitions());
    }
    let client = crate::onemin::client();
    crate::onemin::converse(
        model,
        system,
        text,
        &tools,
        |body| crate::onemin::post_chat(&client, key, body),
        |name, args| call_tool(app, name, args),
    )
    .map_err(|err| err.failover_text())
}

pub fn answer_onemin(app: &AppHandle, text: &str, key: &str, model: &str) -> Result<String, String> {
    answer_with_onemin(app, &onemin_system(app), text, key, model)
}

fn local_system(app: &AppHandle) -> String {
    let settings = app.state::<crate::settings::SettingsStore>().get();
    turn::system_prompt(
        &settings.instructions,
        &app.state::<crate::memory::Memory>().prompt(),
        &app.state::<crate::local_memory::LocalMemory>().prompt(),
        &settings.local_speech,
    )
}

fn record_exchange(app: &AppHandle, history: &[ChatTurn]) {
    let turns = history
        .iter()
        .map(|turn| crate::local_memory::ExchangeTurn {
            role: turn.role.to_string(),
            text: turn.text.clone(),
        })
        .collect::<Vec<_>>();
    crate::local_memory::record_exchange(app, &turns);
}

async fn device_facts(app: &AppHandle) -> crate::native::DeviceFacts {
    let app = app.clone();
    match tokio::task::spawn_blocking(move || app.state::<NativeBridge>().device_facts()).await {
        Ok(Ok(facts)) => facts,
        _ => crate::native::DeviceFacts::default(),
    }
}

async fn handle_audio(
    app: &AppHandle,
    hub: &Hub,
    player: &Player,
    active: &AtomicBool,
    engines: &Arc<Mutex<Engines>>,
    system: &str,
    family: &str,
    remote: Option<&RemoteBrain>,
    history: &mut Vec<ChatTurn>,
    utterance: &[i16],
) -> Result<(), String> {
    if active.load(Ordering::Acquire) {
        hub.set_phase(Phase::Thinking);
    }
    let samples = resample_16k(utterance);
    let engines_bg = engines.clone();
    let text = tokio::task::spawn_blocking(move || {
        engines_bg
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .transcribe(&samples)
    })
    .await
    .map_err(|err| err.to_string())??;
    let text = text.trim().to_string();
    if text.is_empty() {
        return Ok(());
    }
    hub.info(format!("heard: {text}"));
    remember(app, hub, "user", &text);
    handle_text(
        app, hub, player, active, engines, system, family, remote, history, &text, true,
    )
    .await
}

async fn handle_text(
    app: &AppHandle,
    hub: &Hub,
    player: &Player,
    active: &AtomicBool,
    engines: &Arc<Mutex<Engines>>,
    system: &str,
    family: &str,
    remote: Option<&RemoteBrain>,
    history: &mut Vec<ChatTurn>,
    text: &str,
    already_recorded: bool,
) -> Result<(), String> {
    crate::consent::note_user_turn(text);
    if !already_recorded {
        remember(app, hub, "user", text);
    }
    if active.load(Ordering::Acquire) {
        hub.set_phase(Phase::Thinking);
    }
    let engines_bg = engines.clone();
    let app_bg = app.clone();
    let system = system.to_string();
    let family = family.to_string();
    let text = text.to_string();
    let remote_key = remote.map(|brain| (brain.key.clone(), brain.model.clone()));
    let mut history_owned = std::mem::take(history);
    let outcome = tokio::task::spawn_blocking(move || {
        let result = if let Some((key, model)) = remote_key {
            let spoken = answer_with_onemin(&app_bg, &system, &text, &key, &model);
            if let Ok(spoken) = &spoken {
                history_owned.push(ChatTurn { role: "user", text: text.clone() });
                history_owned.push(ChatTurn { role: "assistant", text: spoken.clone() });
            }
            spoken
        } else {
            turn::run_turns_with(
                &system,
                &mut history_owned,
                &text,
                &family,
                |prompt, grammar| {
                    let mut engine = engines_bg.lock().unwrap_or_else(|err| err.into_inner());
                    engine.complete(prompt, grammar)
                },
                |name, args| call_tool(&app_bg, name, args),
            )
        };
        (result, history_owned)
    })
    .await
    .map_err(|err| err.to_string())?;
    let (result, history_owned) = outcome;
    *history = history_owned;
    let spoken = result?;
    remember(app, hub, "assistant", &spoken);
    speak_line(app, hub, player, active, engines, &spoken).await;
    Ok(())
}

fn call_tool(app: &AppHandle, name: &str, args: &serde_json::Value) -> String {
    if let Some(output) = crate::local_memory::run_tool(app, name, args) {
        return output;
    }
    let hub = app.state::<Arc<Hub>>();
    if name == "web_search" {
        hub.emit("voice://busy", "Searching the web…");
    }
    let arguments = args.to_string();
    let output = actions::run(app, name, &arguments);
    if name == "web_search" {
        hub.emit("voice://busy", "");
    }
    output
}

fn remember(app: &AppHandle, hub: &Hub, role: &'static str, text: &str) {
    let text = text.trim();
    if text.is_empty() {
        return;
    }
    app.state::<crate::memory::Memory>().remember(role, text);
    hub.emit(
        "voice://transcript",
        Transcript {
            id: format!("local-{role}-{}", now_ms()),
            role,
            text,
            delta: None,
            is_final: true,
        },
    );
}

async fn speak_line(
    app: &AppHandle,
    hub: &Hub,
    player: &Player,
    active: &AtomicBool,
    engines: &Arc<Mutex<Engines>>,
    text: &str,
) {
    let text = text.trim();
    if text.is_empty() {
        return;
    }
    if active.load(Ordering::Acquire) {
        hub.set_phase(Phase::Speaking);
    }
    let settings = app.state::<crate::settings::SettingsStore>().get();
    let speech = settings.local_speech.clone();
    let engines_map = settings.speech_engines.clone();
    let models_map = settings.phonon_models.clone();
    let voices_map = settings.phonon_voices.clone();
    let dir = super::model_dir(app).ok();
    let planned = dir
        .as_ref()
        .map(|dir| super::speech::plan_in(dir, &speech, &engines_map, &models_map))
        .unwrap_or_else(|| {
            super::speech::plan(&speech, super::speech::EngineChoice::Auto, None, false, &[])
        });
    let voice_id = super::speech::voice_pref(&voices_map, &speech);
    for step in planned.steps {
        match step {
            super::speech::SpokenEngine::Phonon => {
                let (Some(dir), Some(model)) = (dir.clone(), planned.model_id) else {
                    continue;
                };
                let owned = text.to_string();
                let voice_id = voice_id.clone();
                let speech = speech.clone();
                let model = model.to_string();
                let synth = tokio::task::spawn_blocking(move || {
                    super::phonon::synthesize(&dir, &model, &speech, &voice_id, &owned)
                })
                .await;
                match synth {
                    Ok(Ok((pcm, rate))) => {
                        play(player, &pcm, rate).await;
                        return;
                    }
                    Ok(Err(err)) => {
                        hub.warn(format!("Phonon failed, trying the next voice: {err}"))
                    }
                    Err(err) => hub.warn(format!("Phonon failed, trying the next voice: {err}")),
                }
            }
            super::speech::SpokenEngine::Piper => {
                let engines_bg = engines.clone();
                let owned = text.to_string();
                let synth = tokio::task::spawn_blocking(move || {
                    engines_bg
                        .lock()
                        .unwrap_or_else(|err| err.into_inner())
                        .synthesize(&owned)
                })
                .await;
                match synth {
                    Ok(Ok((pcm, rate))) => {
                        play(player, &pcm, rate).await;
                        return;
                    }
                    Ok(Err(err)) => {
                        hub.warn(format!("Piper failed, trying the Android voice: {err}"))
                    }
                    Err(err) => hub.warn(format!("Piper failed, trying the Android voice: {err}")),
                }
            }
            super::speech::SpokenEngine::System => break,
        }
    }
    let app = app.clone();
    let line = text.to_string();
    let _ = tokio::task::spawn_blocking(move || app.state::<NativeBridge>().speak_fallback(&line))
        .await;
}

async fn play(player: &Player, samples: &[f32], rate: u32) {
    let out_rate = player.rate().max(16_000);
    let mut resampler = Resampler::new(rate.max(1), out_rate);
    let mut out = Vec::new();
    resampler.process(samples, &mut out);
    if out.is_empty() {
        return;
    }
    player.push(&out);
    let seconds = out.len() as f32 / out_rate as f32;
    let timeout = Duration::from_secs_f32(seconds + 3.0);
    let start = Instant::now();
    while player.is_active() && start.elapsed() < timeout {
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
}

fn resample_16k(pcm: &[i16]) -> Vec<f32> {
    let floats: Vec<f32> = pcm.iter().copied().map(i16_to_f32).collect();
    let mut resampler = Resampler::new(UPLINK_RATE, 16_000);
    let mut out = Vec::new();
    resampler.process(&floats, &mut out);
    out
}
