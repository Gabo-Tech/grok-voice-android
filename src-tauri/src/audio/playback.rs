//! Lock-light speaker queue shared between the realtime session (producer) and the cpal
//! output callback (consumer). Samples are mono f32 at the output device rate.

use std::{
    collections::VecDeque,
    sync::{
        LazyLock, Mutex,
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

static EPOCH: LazyLock<Instant> = LazyLock::new(Instant::now);
static TALK_OUTPUT_BUSY: AtomicBool = AtomicBool::new(false);
static NOTICE_SPEAKING: AtomicBool = AtomicBool::new(false);

/// True while a Talk session still has samples in the speaker queue.
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub fn talk_output_busy() -> bool {
    TALK_OUTPUT_BUSY.load(Ordering::Relaxed)
}

/// Spoken alerts set this so Talk does not hear its own reminder.
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub fn set_notice_speaking(on: bool) {
    NOTICE_SPEAKING.store(on, Ordering::Relaxed);
}

pub fn notice_speaking() -> bool {
    NOTICE_SPEAKING.load(Ordering::Relaxed)
}

/// Keep the echo guard engaged briefly after the last sample leaves the speaker so the
/// room's reverb tail doesn't trigger server VAD.
const TAIL: Duration = Duration::from_millis(350);

pub struct Player {
    queue: Mutex<VecDeque<f32>>,
    rate: AtomicU32,
    last_output_ms: AtomicU64,
    level_bits: AtomicU32,
}

impl Player {
    pub fn new() -> Self {
        Self {
            queue: Mutex::new(VecDeque::with_capacity(48_000 * 4)),
            rate: AtomicU32::new(48_000),
            last_output_ms: AtomicU64::new(0),
            level_bits: AtomicU32::new(0),
        }
    }

    pub fn set_rate(&self, rate: u32) {
        self.rate.store(rate, Ordering::Relaxed);
    }

    pub fn rate(&self) -> u32 {
        self.rate.load(Ordering::Relaxed)
    }

    pub fn push(&self, samples: &[f32]) {
        let mut q = self.queue.lock().unwrap();
        // Cap at ~60 s so a stalled output device can't grow memory without bound.
        let cap = self.rate() as usize * 60;
        if q.len() + samples.len() > cap {
            let drop = (q.len() + samples.len()).saturating_sub(cap).min(q.len());
            q.drain(..drop);
        }
        q.extend(samples.iter().copied());
        if !samples.is_empty() {
            TALK_OUTPUT_BUSY.store(true, Ordering::Relaxed);
        }
    }

    pub fn clear(&self) {
        self.queue.lock().unwrap().clear();
        self.level_bits.store(0, Ordering::Relaxed);
        TALK_OUTPUT_BUSY.store(false, Ordering::Relaxed);
    }

    pub fn queued(&self) -> usize {
        self.queue.lock().map(|q| q.len()).unwrap_or(0)
    }

    /// True while audio is queued or was audible within the reverb tail window.
    pub fn is_active(&self) -> bool {
        if self.queued() > 0 {
            return true;
        }
        let last = self.last_output_ms.load(Ordering::Relaxed);
        last != 0 && (EPOCH.elapsed().as_millis() as u64).saturating_sub(last) < TAIL.as_millis() as u64
    }

    pub fn level(&self) -> f32 {
        f32::from_bits(self.level_bits.load(Ordering::Relaxed))
    }

    /// Called from the realtime audio callback. Never blocks: if the producer holds the
    /// lock this callback renders silence for one buffer.
    pub fn render(&self, out: &mut [f32], channels: usize) {
        let frames = out.len() / channels.max(1);
        let mut sum = 0.0f32;
        let mut wrote = 0usize;
        if let Ok(mut q) = self.queue.try_lock() {
            for f in 0..frames {
                let s = q.pop_front();
                let v = s.unwrap_or(0.0);
                if s.is_some() {
                    wrote += 1;
                    sum += v * v;
                }
                for c in 0..channels {
                    out[f * channels + c] = v;
                }
            }
        } else {
            out.fill(0.0);
        }
        if wrote > 0 {
            self.last_output_ms.store(EPOCH.elapsed().as_millis() as u64, Ordering::Relaxed);
            let rms = (sum / wrote as f32).sqrt();
            self.level_bits.store(rms.to_bits(), Ordering::Relaxed);
        } else {
            self.level_bits.store(0, Ordering::Relaxed);
        }
        TALK_OUTPUT_BUSY.store(self.is_active(), Ordering::Relaxed);
    }
}
