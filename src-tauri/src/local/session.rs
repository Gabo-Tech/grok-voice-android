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
    memory_enough,
    mode::set_public_search,
    packs::{model_dir, model_paths_for, packs_ready_for},
    turn::{self, ChatTurn},
    vad::Vad,
    MIN_FREE_MB,
};

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
    if !packs_ready_for(&dir, &speech) {
        return Err(if speech == "de" {
            "German on-device speech is not downloaded. English Moonshine does not understand German. Download the German packs in Settings.".into()
        } else {
            super::web::OFFLINE_VOICE.into()
        });
    }
    let facts = app.state::<NativeBridge>().device_facts().unwrap_or(crate::native::DeviceFacts {
        avail_mb: 0,
        low_memory: false,
        online: false,
    });
    if !memory_enough(facts.avail_mb, facts.low_memory) {
        return Err("This phone is offline, and it is too low on memory to run the on-device voice.".into());
    }
    let paths = model_paths_for(&dir, &speech);
    let threads = std::thread::available_parallelism().map(|n| n.get().clamp(2, 4) as i32).unwrap_or(2);
    let mut engines = Engines::load(&paths, threads)?;
    let system = local_system(app);
    let mut history = Vec::new();
    let spoken = turn::run_turns(
        &system,
        &mut history,
        text,
        |prompt, grammar| engines.complete(prompt, grammar),
        |name, args| call_tool(app, name, args),
    )?;
    record_exchange(app, &history);
    let _ = app.state::<NativeBridge>().speak_fallback(&spoken);
    Ok(spoken)
}

pub async fn run_session(
    app: &AppHandle,
    mic: &mut mpsc::Receiver<Vec<i16>>,
    player: &Player,
    active: &AtomicBool,
    settings: &Settings,
    hub: &Hub,
) -> Result<String, String> {
    let _guard = SearchGuard;
    set_public_search(true);
    hub.update(|state| {
        state.ws_state = "on device".into();
        state.model = Some("smollm2-360m-instruct-q8_0".into());
        state.token_minted_at_ms = None;
        state.token_expires_at = None;
    });
    if active.load(Ordering::Acquire) {
        hub.set_phase(Phase::Connecting);
    }
    let facts = device_facts(app).await;
    if !memory_enough(facts.avail_mb, facts.low_memory) {
        let why = if facts.low_memory {
            "This phone is low on memory, so the on-device voice cannot load.".to_string()
        } else {
            format!("On-device voice needs about {MIN_FREE_MB} MB free, and this phone has {} MB free.", facts.avail_mb)
        };
        return Err(why);
    }
    let dir = model_dir(app)?;
    let speech = settings.local_speech.as_str();
    if !packs_ready_for(&dir, speech) {
        return Err(if speech == "de" {
            "German on-device speech is not downloaded. English Moonshine does not understand German. Download the German packs in Settings.".into()
        } else {
            "On-device voice packs are not downloaded yet. Download them in Settings, under Voice keys.".into()
        });
    }
    let paths = model_paths_for(&dir, speech);
    let threads = std::thread::available_parallelism().map(|n| n.get().clamp(2, 4) as i32).unwrap_or(2);
    hub.info("loading on-device speech, language model, and voice");
    let engines = tokio::task::spawn_blocking(move || Engines::load(&paths, threads))
        .await
        .map_err(|err| err.to_string())??;
    let engines = Arc::new(Mutex::new(engines));
    let system = local_system(app);
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
                    if let Err(err) = handle_audio(app, hub, player, active, &engines, &system, &mut history, &utterance).await {
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
                    let text = format!("Typed just now. Use this and do not answer it by itself. {note}");
                    if let Err(err) = handle_text(app, hub, player, active, &engines, &system, &mut history, &text, false).await {
                        hub.warn(format!("typed context skipped: {err}"));
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
        .map(|turn| crate::local_memory::ExchangeTurn { role: turn.role.to_string(), text: turn.text.clone() })
        .collect::<Vec<_>>();
    crate::local_memory::record_exchange(app, &turns);
}

async fn device_facts(app: &AppHandle) -> crate::native::DeviceFacts {
    let app = app.clone();
    match tokio::task::spawn_blocking(move || app.state::<NativeBridge>().device_facts()).await {
        Ok(Ok(facts)) => facts,
        _ => crate::native::DeviceFacts { avail_mb: 0, low_memory: false, online: true },
    }
}

async fn handle_audio(
    app: &AppHandle,
    hub: &Hub,
    player: &Player,
    active: &AtomicBool,
    engines: &Arc<Mutex<Engines>>,
    system: &str,
    history: &mut Vec<ChatTurn>,
    utterance: &[i16],
) -> Result<(), String> {
    if active.load(Ordering::Acquire) {
        hub.set_phase(Phase::Thinking);
    }
    let samples = resample_16k(utterance);
    let engines_bg = engines.clone();
    let text = tokio::task::spawn_blocking(move || {
        engines_bg.lock().unwrap_or_else(|err| err.into_inner()).transcribe(&samples)
    })
    .await
    .map_err(|err| err.to_string())??;
    let text = text.trim().to_string();
    if text.is_empty() {
        return Ok(());
    }
    hub.info(format!("heard: {text}"));
    remember(app, hub, "user", &text);
    handle_text(app, hub, player, active, engines, system, history, &text, true).await
}

async fn handle_text(
    app: &AppHandle,
    hub: &Hub,
    player: &Player,
    active: &AtomicBool,
    engines: &Arc<Mutex<Engines>>,
    system: &str,
    history: &mut Vec<ChatTurn>,
    text: &str,
    already_recorded: bool,
) -> Result<(), String> {
    if !already_recorded {
        remember(app, hub, "user", text);
    }
    if active.load(Ordering::Acquire) {
        hub.set_phase(Phase::Thinking);
    }
    let engines_bg = engines.clone();
    let app_bg = app.clone();
    let system = system.to_string();
    let text = text.to_string();
    let mut history_owned = std::mem::take(history);
    let outcome = tokio::task::spawn_blocking(move || {
        let result = turn::run_turns(
            &system,
            &mut history_owned,
            &text,
            |prompt, grammar| {
                let mut engine = engines_bg.lock().unwrap_or_else(|err| err.into_inner());
                engine.complete(prompt, grammar)
            },
            |name, args| call_tool(&app_bg, name, args),
        );
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
        Transcript { id: format!("local-{role}-{}", now_ms()), role, text, delta: None, is_final: true },
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
    let engines_bg = engines.clone();
    let owned = text.to_string();
    let synth = tokio::task::spawn_blocking(move || {
        engines_bg.lock().unwrap_or_else(|err| err.into_inner()).synthesize(&owned)
    })
    .await;
    match synth {
        Ok(Ok((pcm, rate))) => play(player, &pcm, rate).await,
        _ => {
            let app = app.clone();
            let line = text.to_string();
            let _ = tokio::task::spawn_blocking(move || app.state::<NativeBridge>().speak_fallback(&line)).await;
        }
    }
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
