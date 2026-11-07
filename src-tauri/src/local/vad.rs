//! Energy VAD for utterance mode. A pause of about 700 ms ends the turn,
//! matching the realtime session's silence window. This is not barge-in.

use std::collections::VecDeque;

pub struct Vad {
    speech: bool,
    speech_samples: usize,
    silence_samples: usize,
    buf: Vec<i16>,
    preroll: VecDeque<i16>,
    preroll_cap: usize,
    noise: f32,
    min_speech: usize,
    hangover: usize,
    max_samples: usize,
}

impl Vad {
    pub fn new(rate: u32) -> Self {
        let rate = rate.max(1);
        Self {
            speech: false,
            speech_samples: 0,
            silence_samples: 0,
            buf: Vec::new(),
            preroll: VecDeque::new(),
            // 400 ms of audio before the energy trigger, so the first syllable is kept.
            preroll_cap: rate as usize * 2 / 5,
            noise: 0.005,
            min_speech: rate as usize / 4,
            hangover: rate as usize * 7 / 10,
            max_samples: rate as usize * 20,
        }
    }

    /// Returns one finished utterance, including a short preroll.
    pub fn push(&mut self, frame: &[i16]) -> Option<Vec<i16>> {
        if frame.is_empty() {
            return None;
        }
        let rms = rms(frame);
        let threshold = (self.noise * 3.5).max(0.015);
        let voiced = rms > threshold;
        if !self.speech {
            if voiced {
                self.speech = true;
                self.buf.extend(self.preroll.drain(..));
                self.buf.extend_from_slice(frame);
                self.speech_samples = frame.len();
                self.silence_samples = 0;
            } else {
                self.noise = self.noise * 0.95 + rms * 0.05;
                self.preroll.extend(frame.iter().copied());
                while self.preroll.len() > self.preroll_cap {
                    self.preroll.pop_front();
                }
            }
            return None;
        }

        self.buf.extend_from_slice(frame);
        if voiced {
            self.speech_samples += frame.len();
            self.silence_samples = 0;
        } else {
            self.silence_samples += frame.len();
        }
        let done = self.silence_samples >= self.hangover || self.buf.len() >= self.max_samples;
        if !done {
            return None;
        }
        let uttered = std::mem::take(&mut self.buf);
        let long_enough = self.speech_samples >= self.min_speech;
        self.speech = false;
        self.speech_samples = 0;
        self.silence_samples = 0;
        long_enough.then_some(uttered)
    }
}

fn rms(frame: &[i16]) -> f32 {
    let mut acc = 0.0f32;
    for sample in frame {
        let x = *sample as f32 / 32768.0;
        acc += x * x;
    }
    (acc / frame.len() as f32).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(rate: u32, ms: u32, amp: i16) -> Vec<i16> {
        vec![amp; (rate as usize * ms as usize) / 1000]
    }

    #[test]
    fn silence_then_speech_then_pause_is_one_utterance() {
        let rate = 16_000;
        let mut vad = Vad::new(rate);
        for _ in 0..3 {
            assert!(vad.push(&frame(rate, 100, 0)).is_none());
        }
        let mut finished = None;
        for _ in 0..5 {
            finished = vad.push(&frame(rate, 100, 8000)).or(finished);
        }
        assert!(finished.is_none());
        for _ in 0..9 {
            if let Some(utt) = vad.push(&frame(rate, 100, 0)) {
                finished = Some(utt);
                break;
            }
        }
        let utt = finished.expect("utterance");
        assert!(utt.iter().any(|s| s.abs() > 1000));
        assert!(utt.len() > rate as usize / 2);
    }

    #[test]
    fn a_short_click_is_ignored() {
        let rate = 16_000;
        let mut vad = Vad::new(rate);
        assert!(vad.push(&frame(rate, 100, 0)).is_none());
        assert!(vad.push(&frame(rate, 80, 9000)).is_none());
        let mut saw = false;
        for _ in 0..10 {
            if vad.push(&frame(rate, 100, 0)).is_some() {
                saw = true;
            }
        }
        assert!(!saw);
    }
}
