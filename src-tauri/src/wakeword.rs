//! "Hey Grok" wake word: on-device enrollment (3 spoken samples → rustpotter reference
//! file) and detector construction. Fully offline, no third-party access key.

use std::{collections::HashMap, fs, path::PathBuf};

use rustpotter::{
    Rustpotter, RustpotterConfig, WakewordRef, WakewordRefBuildFromBuffers, WakewordSave,
};
use serde::Serialize;

use crate::{
    audio::engine::WAKE_RATE,
    error::{Result, msg},
};

pub const SAMPLE_COUNT: usize = 3;
pub const WAKEWORD_NAME: &str = "hey_grok";
const MFCC_SIZE: u16 = 16;
const FRAME: usize = (WAKE_RATE / 100) as usize; // 10 ms

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WakewordStatus {
    pub trained: bool,
    pub samples: Vec<bool>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SampleInfo {
    pub index: usize,
    pub duration_ms: u64,
    pub peak: f32,
}

pub struct WakewordFiles {
    dir: PathBuf,
}

impl WakewordFiles {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    fn model_path(&self) -> PathBuf {
        self.dir.join(format!("{WAKEWORD_NAME}.rpw"))
    }

    fn sample_path(&self, i: usize) -> PathBuf {
        self.dir.join("wake_samples").join(format!("sample_{i}.wav"))
    }

    pub fn status(&self) -> WakewordStatus {
        WakewordStatus {
            trained: self.model_path().exists(),
            samples: (0..SAMPLE_COUNT).map(|i| self.sample_path(i).exists()).collect(),
        }
    }

    pub fn save_sample(&self, index: usize, pcm16k: &[f32]) -> Result<SampleInfo> {
        if index >= SAMPLE_COUNT {
            return Err(msg(format!("sample index must be < {SAMPLE_COUNT}")));
        }
        let (trimmed, peak) = trim_silence(pcm16k)?;
        let path = self.sample_path(index);
        fs::create_dir_all(path.parent().unwrap())?;
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: WAKE_RATE,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut w = hound::WavWriter::create(&path, spec).map_err(|e| msg(e.to_string()))?;
        for &s in trimmed {
            w.write_sample(crate::audio::resample::f32_to_i16(s)).map_err(|e| msg(e.to_string()))?;
        }
        w.finalize().map_err(|e| msg(e.to_string()))?;
        Ok(SampleInfo {
            index,
            duration_ms: (trimmed.len() as u64 * 1000) / WAKE_RATE as u64,
            peak,
        })
    }

    pub fn train(&self) -> Result<()> {
        let mut samples = HashMap::new();
        for i in 0..SAMPLE_COUNT {
            let p = self.sample_path(i);
            let bytes = fs::read(&p)
                .map_err(|_| msg(format!("record sample {} first", i + 1)))?;
            samples.insert(format!("sample_{i}.wav"), bytes);
        }
        let wakeword = WakewordRef::new_from_sample_buffers(
            WAKEWORD_NAME.into(),
            None,
            None,
            samples,
            MFCC_SIZE,
        )
        .map_err(msg)?;
        wakeword
            .save_to_file(self.model_path().to_string_lossy().as_ref())
            .map_err(msg)?;
        Ok(())
    }

    pub fn delete(&self) -> Result<()> {
        let _ = fs::remove_file(self.model_path());
        for i in 0..SAMPLE_COUNT {
            let _ = fs::remove_file(self.sample_path(i));
        }
        Ok(())
    }

    pub fn build_detector(&self, threshold: f32) -> Result<Option<Rustpotter>> {
        let path = self.model_path();
        if !path.exists() {
            return Ok(None);
        }
        let mut cfg = RustpotterConfig::default();
        cfg.fmt.sample_rate = WAKE_RATE as usize;
        cfg.fmt.channels = 1;
        cfg.detector.threshold = threshold;
        cfg.detector.avg_threshold = (threshold * 0.4).max(0.1);
        cfg.filters.gain_normalizer.enabled = true;
        let mut detector = Rustpotter::new(&cfg).map_err(msg)?;
        detector
            .add_wakeword_from_file(WAKEWORD_NAME, path.to_string_lossy().as_ref())
            .map_err(msg)?;
        Ok(Some(detector))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Lcg(u64);
    impl Lcg {
        fn noise(&mut self) -> f32 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((self.0 >> 33) as f32 / (1u64 << 31) as f32) - 1.0
        }
    }

    /// Two-syllable formant-ish tone pattern standing in for "hey grok".
    fn phrase(pitch: f32, stretch: f32, rng: &mut Lcg) -> Vec<f32> {
        let mut out = vec![0.0; 3200];
        let sr = WAKE_RATE as f32;
        let segs = [(0.18, 420.0, 1900.0), (0.06, 0.0, 0.0), (0.28, 260.0, 1100.0)];
        let mut phase = (0.0f32, 0.0f32);
        for (dur, f1, f2) in segs {
            let n = (dur * stretch * sr) as usize;
            for i in 0..n {
                let env = (std::f32::consts::PI * i as f32 / n as f32).sin();
                let sweep = 1.0 + 0.25 * (i as f32 / n as f32);
                phase.0 += 2.0 * std::f32::consts::PI * f1 * pitch * sweep / sr;
                phase.1 += 2.0 * std::f32::consts::PI * f2 * pitch / sr;
                let v = if f1 == 0.0 { 0.0 } else { 0.35 * phase.0.sin() + 0.2 * phase.1.sin() };
                out.push(env * v + 0.003 * rng.noise());
            }
        }
        out.extend(std::iter::repeat_n(0.0, 3200));
        out
    }

    fn count_detections(det: &mut Rustpotter, audio: &[f32]) -> usize {
        let n = det.get_samples_per_frame();
        audio
            .chunks_exact(n)
            .filter(|f| det.process_samples(f.to_vec()).is_some())
            .count()
    }

    #[test]
    fn enroll_and_detect_synthetic_phrase() {
        let dir = std::env::temp_dir().join(format!("gv-wake-{}", std::process::id()));
        let files = WakewordFiles::new(dir.clone());
        let mut rng = Lcg(7);
        for (i, (p, s)) in [(1.0, 1.0), (1.04, 0.95), (0.97, 1.06)].into_iter().enumerate() {
            files.save_sample(i, &phrase(p, s, &mut rng)).unwrap();
        }
        files.train().unwrap();
        assert!(files.status().trained);

        let mut det = files.build_detector(0.5).unwrap().unwrap();
        let noise: Vec<f32> = (0..WAKE_RATE * 3).map(|_| 0.01 * rng.noise()).collect();
        assert_eq!(count_detections(&mut det, &noise), 0, "false wake on noise");

        let mut stream = noise.clone();
        stream.extend(phrase(1.02, 1.02, &mut rng));
        stream.extend(noise.iter().take(WAKE_RATE as usize));
        det.reset();
        assert!(count_detections(&mut det, &stream) >= 1, "phrase not detected");

        files.delete().unwrap();
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn rejects_silence() {
        assert!(trim_silence(&vec![0.0; 32_000]).is_err());
    }
}

/// Crop leading/trailing silence (keeping 150 ms of padding) so the reference
/// templates align on the spoken phrase rather than room noise.
fn trim_silence(pcm: &[f32]) -> Result<(&[f32], f32)> {
    let rms: Vec<f32> = pcm
        .chunks(FRAME)
        .map(|c| (c.iter().map(|s| s * s).sum::<f32>() / c.len() as f32).sqrt())
        .collect();
    let peak_rms = rms.iter().copied().fold(0.0f32, f32::max);
    let peak = pcm.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    if peak_rms < 0.01 {
        return Err(msg("That was too quiet — hold the phone closer and say “Hey Grok”."));
    }
    let threshold = (peak_rms * 0.15).max(0.005);
    let first = rms.iter().position(|&r| r > threshold).unwrap_or(0);
    let last = rms.iter().rposition(|&r| r > threshold).unwrap_or(rms.len() - 1);
    let pad = 15;
    let start = first.saturating_sub(pad) * FRAME;
    let end = ((last + 1 + pad) * FRAME).min(pcm.len());
    let speech_ms = ((last + 1 - first) * 10) as u64;
    if speech_ms < 250 {
        return Err(msg("Too short — say the full phrase “Hey Grok”."));
    }
    if speech_ms > 2_000 {
        return Err(msg("Too long — say only “Hey Grok”, in a quiet spot."));
    }
    Ok((&pcm[start..end], peak))
}
