//! The audio engine: a dedicated OS thread that owns the cpal input/output streams
//! (they are `!Send` on Android/AAudio), runs wake-word detection on the 16 kHz mic
//! signal, and forwards uplink PCM to the realtime session task. Nothing here touches
//! the WebView thread.

use std::{
    collections::VecDeque,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, Sender, SyncSender, TryRecvError},
    },
    thread,
    time::{Duration, Instant},
};

use cpal::{
    FromSample, SampleFormat, SizedSample, Stream, StreamConfig,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use rustpotter::Rustpotter;
use serde::Serialize;
use tauri::{AppHandle, Manager};
use tokio::sync::mpsc::error::TrySendError;

use super::{
    playback::Player,
    resample::{Resampler, f32_to_i16},
};
use crate::{
    diagnostics::{Hub, Phase},
    native::NativeBridge,
    realtime,
    settings::{Settings, SettingsStore},
    wakeword::WakewordFiles,
};

/// Wake-word sample rate. The realtime uplink is resampled to 24 kHz.
pub const WAKE_RATE: u32 = 16_000;
pub const UPLINK_RATE: u32 = 24_000;
/// 100 ms uplink frames.
const UPLINK_CHUNK: usize = (UPLINK_RATE / 10) as usize;
const PREROLL: usize = WAKE_RATE as usize;
const LEVEL_INTERVAL: Duration = Duration::from_millis(66);

pub type RecordReply = Sender<Result<Vec<f32>, String>>;

pub enum Ctl {
    SetWakeEnabled(bool),
    StartSession,
    EndSession,
    Record { secs: f32, reply: RecordReply },
    ReloadWakeword,
    ReloadSettings,
}

#[derive(Clone)]
pub struct Engine {
    tx: Sender<Ctl>,
}

static CONTROL: OnceLock<Sender<Ctl>> = OnceLock::new();
static KEEP_ALIVE: AtomicBool = AtomicBool::new(false);

/// Used by the JNI entry point when the user taps "Stop" on the foreground notification.
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub fn control() -> Option<&'static Sender<Ctl>> {
    CONTROL.get()
}

/// True while the wake listener should outlive the activity (app swiped away or screen off).
pub fn keep_alive() -> bool {
    KEEP_ALIVE.load(Ordering::Acquire)
}

impl Engine {
    pub fn spawn(app: AppHandle) -> Self {
        let (tx, rx) = mpsc::channel();
        let _ = CONTROL.set(tx.clone());
        thread::Builder::new()
            .name("grok-audio".into())
            .spawn(move || EngineThread::new(app, rx).run())
            .expect("spawn audio engine thread");
        Self { tx }
    }

    pub fn send(&self, c: Ctl) {
        let _ = self.tx.send(c);
    }
}

#[derive(Clone, Serialize)]
struct Level {
    mic: f32,
    out: f32,
}

struct Input {
    _stream: Stream,
    rx: Receiver<Vec<f32>>,
    resampler: Resampler,
}

struct SessionLink {
    tx: tokio::sync::mpsc::Sender<Vec<i16>>,
    pending: Vec<i16>,
    active: Arc<AtomicBool>,
}

struct Recording {
    buf: Vec<f32>,
    target: usize,
    reply: RecordReply,
}

struct EngineThread {
    app: AppHandle,
    hub: Arc<Hub>,
    player: Arc<Player>,
    rx: Receiver<Ctl>,
    settings: Settings,
    wake_enabled: bool,
    service_running: bool,
    service_warned: bool,
    mic_warned: bool,
    mic_retry_at: Option<Instant>,
    input: Option<Input>,
    input_failed: Arc<AtomicBool>,
    output: Option<Stream>,
    output_failed: Arc<AtomicBool>,
    detector: Option<Rustpotter>,
    wake_buf: Vec<f32>,
    preroll: VecDeque<f32>,
    uplink: Resampler,
    uplink_gain: f32,
    session: Option<SessionLink>,
    recording: Option<Recording>,
    last_level: Instant,
    last_chunk_at: Instant,
    scratch: Vec<f32>,
}

impl EngineThread {
    fn new(app: AppHandle, rx: Receiver<Ctl>) -> Self {
        let hub = app.state::<Arc<Hub>>().inner().clone();
        let player = app.state::<Arc<Player>>().inner().clone();
        let settings = app.state::<SettingsStore>().get();
        Self {
            app,
            hub,
            player,
            rx,
            wake_enabled: settings.listen_armed,
            settings,
            service_running: false,
            service_warned: false,
            mic_warned: false,
            mic_retry_at: None,
            input: None,
            input_failed: Arc::new(AtomicBool::new(false)),
            output: None,
            output_failed: Arc::new(AtomicBool::new(false)),
            detector: None,
            wake_buf: Vec::with_capacity(4096),
            preroll: VecDeque::with_capacity(PREROLL),
            uplink: Resampler::new(WAKE_RATE, UPLINK_RATE),
            uplink_gain: 1.0,
            session: None,
            recording: None,
            last_level: Instant::now(),
            last_chunk_at: Instant::now(),
            scratch: Vec::with_capacity(4096),
        }
    }

    fn run(mut self) {
        self.reload_wakeword();
        loop {
            loop {
                match self.rx.try_recv() {
                    Ok(c) => self.handle(c),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => return,
                }
            }

            if self.input_failed.swap(false, Ordering::Relaxed) {
                self.hub.warn("microphone stream interrupted, reopening");
                self.input = None;
            }
            // Screen-off and leaving the app can silence capture without an error.
            // Wait long enough that a stream which is still starting is not torn down.
            let needs_mic = self.wake_enabled || self.session.is_some() || self.recording.is_some();
            if needs_mic && self.input.is_some() && self.last_chunk_at.elapsed() >= Duration::from_secs(6) {
                self.hub.warn("microphone silent, reopening");
                self.input = None;
                self.wake_buf.clear();
            }
            if self.output_failed.swap(false, Ordering::Relaxed) {
                self.hub.warn("speaker stream interrupted, reopening");
                self.output = None;
            }
            if self.session.as_ref().is_some_and(|s| s.tx.is_closed()) {
                self.finish_session();
            }
            self.reconcile();

            let Some(input) = self.input.as_ref() else {
                let needs = self.wake_enabled || self.session.is_some() || self.recording.is_some();
                let recv = if needs {
                    self.rx.recv_timeout(Duration::from_millis(400))
                } else {
                    self.rx.recv_timeout(Duration::from_secs(3600))
                };
                match recv {
                    Ok(c) => self.handle(c),
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => return,
                }
                continue;
            };
            match input.rx.recv_timeout(Duration::from_millis(40)) {
                Ok(chunk) => {
                    self.on_chunk(&chunk);
                    while let Some(chunk) = self.input.as_ref().and_then(|i| i.rx.try_recv().ok()) {
                        self.on_chunk(&chunk);
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => self.input_failed.store(true, Ordering::Relaxed),
            }
        }
    }

    fn handle(&mut self, c: Ctl) {
        match c {
            Ctl::SetWakeEnabled(on) => {
                self.wake_enabled = on;
                self.settings.listen_armed = on;
                let store = self.app.state::<SettingsStore>();
                if let Ok(saved) = store.save(self.settings.clone()) {
                    self.settings = saved;
                }
                self.hub.update(|s| s.wake_enabled = on);
                self.hub.emit("voice://wake_enabled", on);
                if on && self.detector.is_none() {
                    self.hub.warn("wake word not trained yet — tap the ring to talk, or enroll “Hey Grok” in the Wake tab");
                }
                if on {
                    self.hub.info("listening for “Hey Grok”");
                }
            }
            Ctl::StartSession => self.start_session("manual"),
            Ctl::EndSession => self.finish_session(),
            Ctl::Record { secs, reply } => {
                if self.session.is_some() {
                    let _ = reply.send(Err("end the voice session before recording".into()));
                } else if self.recording.is_some() {
                    let _ = reply.send(Err("already recording".into()));
                } else {
                    let target = (secs.clamp(0.5, 5.0) * WAKE_RATE as f32) as usize;
                    self.recording = Some(Recording { buf: Vec::with_capacity(target), target, reply });
                    self.hub.info(format!("recording wake sample ({secs:.1}s)"));
                }
            }
            Ctl::ReloadWakeword => self.reload_wakeword(),
            Ctl::ReloadSettings => {
                self.settings = self.app.state::<SettingsStore>().get();
                self.reload_wakeword();
            }
        }
        self.update_idle_phase();
    }

    fn reload_wakeword(&mut self) {
        let files = self.app.state::<WakewordFiles>();
        self.detector = match files.build_detector(self.settings.wake_threshold) {
            Ok(d) => d,
            Err(e) => {
                self.hub.error(format!("wake word load failed: {e}"));
                None
            }
        };
        let trained = self.detector.is_some();
        if let Some(d) = &self.detector {
            self.hub.info(format!(
                "wake word ready (threshold {:.2}, {} samples/frame)",
                self.settings.wake_threshold,
                d.get_samples_per_frame()
            ));
        }
        self.wake_buf.clear();
        self.hub.update(|s| s.wakeword_trained = trained);
    }

    fn reconcile(&mut self) {
        let need_in = self.wake_enabled || self.session.is_some() || self.recording.is_some();
        let need_out = self.session.is_some();

        // Android 14+: a microphone foreground service must be running before (or while)
        // we capture, otherwise capture is silenced once the activity leaves the foreground.
        if need_in && !self.service_running {
            let bridge = self.app.state::<NativeBridge>();
            match bridge.start_service("Hey Grok is listening", "Say “Hey Grok”, or tap to open") {
                Ok(()) => {
                    self.service_running = true;
                    self.service_warned = false;
                    // startForeground runs on the main thread after this returns. Give it
                    // a moment so Android still allows the microphone once we leave the app.
                    thread::sleep(Duration::from_millis(400));
                }
                Err(e) if !self.service_warned => {
                    self.service_warned = true;
                    self.hub.warn(format!("foreground service not started: {e}"));
                }
                Err(_) => {}
            }
        }

        let retry_due = self.mic_retry_at.is_none_or(|t| Instant::now() >= t);
        if need_in && self.input.is_none() && retry_due {
            match self.open_input() {
                Ok(input) => {
                    self.input = Some(input);
                    self.mic_warned = false;
                    self.mic_retry_at = None;
                    self.last_chunk_at = Instant::now();
                }
                Err(e) => {
                    if !self.mic_warned {
                        self.mic_warned = true;
                        self.hub.warn(format!("microphone unavailable, will keep trying: {e}"));
                    }
                    self.mic_retry_at = Some(Instant::now() + Duration::from_secs(1));
                    if let Some(r) = self.recording.take() {
                        let _ = r.reply.send(Err(e));
                    }
                }
            }
        } else if !need_in && self.input.is_some() {
            self.input = None;
            self.hub.update(|s| s.mic_rate = None);
        }

        if need_out && self.output.is_none() {
            match self.open_output() {
                Ok(s) => self.output = Some(s),
                Err(e) => {
                    self.hub.error(format!("speaker unavailable: {e}"));
                    self.finish_session();
                }
            }
        } else if !need_out && self.output.is_some() {
            self.output = None;
            self.hub.update(|s| s.speaker_rate = None);
        }

        let need_in = self.wake_enabled || self.session.is_some() || self.recording.is_some();
        if !need_in && self.service_running {
            let _ = self.app.state::<NativeBridge>().stop_service();
            self.service_running = false;
            self.service_warned = false;
            self.mic_retry_at = None;
        }
        KEEP_ALIVE.store(self.service_running, Ordering::Release);
        self.update_idle_phase();
    }

    fn update_idle_phase(&self) {
        if self.session.is_some() {
            return;
        }
        let phase = if self.recording.is_some() {
            Phase::Recording
        } else if self.wake_enabled {
            Phase::Wake
        } else {
            Phase::Idle
        };
        self.hub.set_phase(phase);
    }

    fn on_chunk(&mut self, chunk: &[f32]) {
        let Some(input) = self.input.as_mut() else { return };
        self.last_chunk_at = Instant::now();
        let mut pcm = std::mem::take(&mut self.scratch);
        pcm.clear();
        input.resampler.process(chunk, &mut pcm);

        if self.last_level.elapsed() >= LEVEL_INTERVAL {
            self.last_level = Instant::now();
            let mic = if pcm.is_empty() {
                0.0
            } else {
                (pcm.iter().map(|s| s * s).sum::<f32>() / pcm.len() as f32).sqrt()
            };
            self.hub.emit(
                "voice://level",
                Level { mic: (mic * 8.0).min(1.0), out: (self.player.level() * 5.0).min(1.0) },
            );
        }

        if self.session.is_none() && self.recording.is_none() {
            for &s in &pcm {
                if self.preroll.len() == PREROLL {
                    self.preroll.pop_front();
                }
                self.preroll.push_back(s);
            }
        }

        if let Some(rec) = self.recording.as_mut() {
            rec.buf.extend_from_slice(&pcm);
            if rec.buf.len() >= rec.target {
                let rec = self.recording.take().unwrap();
                let _ = rec.reply.send(Ok(rec.buf));
                self.update_idle_phase();
            }
        } else if let Some(link) = self.session.as_mut() {
            let mute = self.settings.echo_guard && self.player.is_active();
            let mut up = Vec::new();
            self.uplink.process(&pcm, &mut up);
            let gain = self.uplink_gain;
            link.pending.extend(up.iter().map(|&s| if mute { 0 } else { f32_to_i16((s * gain).clamp(-1.0, 1.0)) }));
            let mut closed = false;
            while link.pending.len() >= UPLINK_CHUNK {
                let frame: Vec<i16> = link.pending.drain(..UPLINK_CHUNK).collect();
                match link.tx.try_send(frame) {
                    Ok(()) => {}
                    Err(TrySendError::Full(_)) => self.hub.update(|s| s.frames_dropped += 1),
                    Err(TrySendError::Closed(_)) => {
                        closed = true;
                        break;
                    }
                }
            }
            if closed {
                self.finish_session();
            }
        } else if self.wake_enabled {
            if let Some(det) = self.detector.as_mut() {
                self.wake_buf.extend_from_slice(&pcm);
                let n = det.get_samples_per_frame();
                let mut fired = None;
                while self.wake_buf.len() >= n {
                    let frame: Vec<f32> = self.wake_buf.drain(..n).collect();
                    if let Some(d) = det.process_samples(frame) {
                        fired = Some(d);
                        break;
                    }
                }
                if let Some(d) = fired {
                    let score = d.score;
                    self.hub.update(|s| s.last_wake_score = Some(score));
                    self.hub.info(format!("wake word detected (score {score:.3}, avg {:.3})", d.avg_score));
                    self.hub.emit("voice://wake", score);
                    self.start_session("wake word");
                }
            }
        }
        self.scratch = pcm;
    }

    fn start_session(&mut self, reason: &str) {
        if self.session.is_some() {
            return;
        }
        if self.recording.is_some() {
            self.hub.warn("finish recording the wake sample first");
            return;
        }
        let (tx, rx) = tokio::sync::mpsc::channel(96);
        let active = Arc::new(AtomicBool::new(true));
        self.uplink = Resampler::new(WAKE_RATE, UPLINK_RATE);
        let lead: Vec<f32> = self.preroll.drain(..).collect();
        let peak = lead.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        // Phone float audio is often far below full scale. Server VAD ignores it,
        // while the wake-word model still matches. Lift speech toward half scale.
        self.uplink_gain = if peak < 0.0008 { 1.0 } else { (0.45 / peak).clamp(1.0, 16.0) };
        self.hub.info(format!("uplink gain {:.1} (mic peak {:.4})", self.uplink_gain, peak));
        let mut up = Vec::new();
        self.uplink.process(&lead, &mut up);
        let gain = self.uplink_gain;
        let mut pending = Vec::with_capacity(UPLINK_CHUNK * 2 + up.len());
        pending.extend(up.iter().map(|&s| f32_to_i16((s * gain).clamp(-1.0, 1.0))));
        self.session = Some(SessionLink {
            tx,
            pending,
            active: active.clone(),
        });
        self.wake_buf.clear();
        if let Some(d) = self.detector.as_mut() {
            d.reset();
        }
        self.player.clear();
        self.hub.update(|s| s.sessions += 1);
        self.hub.info(format!("voice session starting ({reason})"));
        self.hub.set_phase(Phase::Connecting);
        let wake = reason == "wake word";
        tauri::async_runtime::spawn(realtime::run_session(
            self.app.clone(),
            rx,
            self.player.clone(),
            active,
            self.settings.clone(),
            wake,
        ));
    }

    fn finish_session(&mut self) {
        if let Some(link) = self.session.take() {
            link.active.store(false, Ordering::Release);
            self.player.clear();
            self.wake_buf.clear();
            if let Some(d) = self.detector.as_mut() {
                d.reset();
            }
            self.hub.info("voice session closed");
        }
        self.update_idle_phase();
    }

    fn open_input(&mut self) -> Result<Input, String> {
        let host = cpal::default_host();
        let device = host.default_input_device().ok_or("no microphone found")?;
        let supported = device.default_input_config().map_err(|e| e.to_string())?;
        let channels = supported.channels() as usize;
        let rate = supported.sample_rate().0;
        let config: StreamConfig = supported.config();
        let (tx, rx) = mpsc::sync_channel::<Vec<f32>>(256);
        let failed = self.input_failed.clone();
        let hub = self.hub.clone();
        let on_err = move |e: cpal::StreamError| {
            hub.warn(format!("mic stream error: {e}"));
            failed.store(true, Ordering::Relaxed);
        };
        let stream = match supported.sample_format() {
            SampleFormat::F32 => build_input::<f32>(&device, &config, channels, tx, on_err),
            SampleFormat::I16 => build_input::<i16>(&device, &config, channels, tx, on_err),
            SampleFormat::I32 => build_input::<i32>(&device, &config, channels, tx, on_err),
            SampleFormat::U16 => build_input::<u16>(&device, &config, channels, tx, on_err),
            f => return Err(format!("unsupported mic sample format {f:?}")),
        }?;
        stream.play().map_err(|e| e.to_string())?;
        self.hub.update(|s| s.mic_rate = Some(rate));
        self.hub.info(format!("mic open: {rate} Hz × {channels}ch {:?}", supported.sample_format()));
        Ok(Input { _stream: stream, rx, resampler: Resampler::new(rate, WAKE_RATE) })
    }

    fn open_output(&mut self) -> Result<Stream, String> {
        let host = cpal::default_host();
        let device = host.default_output_device().ok_or("no speaker found")?;
        let supported = device.default_output_config().map_err(|e| e.to_string())?;
        let channels = supported.channels() as usize;
        let rate = supported.sample_rate().0;
        let config: StreamConfig = supported.config();
        self.player.set_rate(rate);
        let failed = self.output_failed.clone();
        let hub = self.hub.clone();
        let on_err = move |e: cpal::StreamError| {
            hub.warn(format!("speaker stream error: {e}"));
            failed.store(true, Ordering::Relaxed);
        };
        let p = self.player.clone();
        let stream = match supported.sample_format() {
            SampleFormat::F32 => build_output::<f32>(&device, &config, channels, p, on_err),
            SampleFormat::I16 => build_output::<i16>(&device, &config, channels, p, on_err),
            SampleFormat::I32 => build_output::<i32>(&device, &config, channels, p, on_err),
            SampleFormat::U16 => build_output::<u16>(&device, &config, channels, p, on_err),
            f => return Err(format!("unsupported speaker sample format {f:?}")),
        }?;
        stream.play().map_err(|e| e.to_string())?;
        self.hub.update(|s| s.speaker_rate = Some(rate));
        self.hub.info(format!("speaker open: {rate} Hz × {channels}ch {:?}", supported.sample_format()));
        Ok(stream)
    }
}

fn build_input<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    channels: usize,
    tx: SyncSender<Vec<f32>>,
    on_err: impl FnMut(cpal::StreamError) + Send + 'static,
) -> Result<Stream, String>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let channels = channels.max(1);
    device
        .build_input_stream(
            config,
            move |data: &[T], _: &cpal::InputCallbackInfo| {
                let mono: Vec<f32> = data
                    .chunks(channels)
                    .map(|frame| frame.iter().map(|&s| s.to_sample::<f32>()).sum::<f32>() / channels as f32)
                    .collect();
                // Never block the realtime callback; drop audio if the engine fell behind.
                let _ = tx.try_send(mono);
            },
            on_err,
            None,
        )
        .map_err(|e| e.to_string())
}

fn build_output<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    channels: usize,
    player: Arc<Player>,
    on_err: impl FnMut(cpal::StreamError) + Send + 'static,
) -> Result<Stream, String>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = channels.max(1);
    let mut scratch: Vec<f32> = Vec::with_capacity(8192);
    device
        .build_output_stream(
            config,
            move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                scratch.resize(data.len(), 0.0);
                player.render(&mut scratch, channels);
                for (d, s) in data.iter_mut().zip(scratch.iter()) {
                    *d = T::from_sample(*s);
                }
            },
            on_err,
            None,
        )
        .map_err(|e| e.to_string())
}
