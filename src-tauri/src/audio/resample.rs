//! Streaming mono resampler: linear interpolation, preceded by a 4th-order Butterworth
//! low-pass when downsampling so 48 kHz device audio doesn't alias into the 16 kHz band.

#[derive(Clone)]
struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl Biquad {
    fn lowpass(sample_rate: f32, cutoff: f32, q: f32) -> Self {
        let w0 = 2.0 * std::f32::consts::PI * cutoff / sample_rate;
        let (sin, cos) = w0.sin_cos();
        let alpha = sin / (2.0 * q);
        let a0 = 1.0 + alpha;
        Self {
            b0: ((1.0 - cos) / 2.0) / a0,
            b1: (1.0 - cos) / a0,
            b2: ((1.0 - cos) / 2.0) / a0,
            a1: (-2.0 * cos) / a0,
            a2: (1.0 - alpha) / a0,
            z1: 0.0,
            z2: 0.0,
        }
    }

    #[inline]
    fn run(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }
}

pub struct Resampler {
    step: f64,
    pos: f64,
    last: f32,
    passthrough: bool,
    filters: Vec<Biquad>,
}

impl Resampler {
    pub fn new(from_hz: u32, to_hz: u32) -> Self {
        let filters = if to_hz < from_hz {
            let cutoff = to_hz as f32 * 0.45;
            vec![
                Biquad::lowpass(from_hz as f32, cutoff, 0.541_196_1),
                Biquad::lowpass(from_hz as f32, cutoff, 1.306_563),
            ]
        } else {
            Vec::new()
        };
        Self {
            step: from_hz as f64 / to_hz as f64,
            pos: 0.0,
            last: 0.0,
            passthrough: from_hz == to_hz,
            filters,
        }
    }

    pub fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        if input.is_empty() {
            return;
        }
        if self.passthrough {
            out.extend_from_slice(input);
            return;
        }
        let filtered: Vec<f32>;
        let src: &[f32] = if self.filters.is_empty() {
            input
        } else {
            filtered = input
                .iter()
                .map(|&x| self.filters.iter_mut().fold(x, |acc, f| f.run(acc)))
                .collect();
            &filtered
        };

        // Virtual buffer: index 0 = previous chunk's last sample, index k = src[k - 1].
        let n = src.len() as f64;
        out.reserve((n / self.step) as usize + 1);
        while self.pos < n {
            let i = self.pos as usize;
            let frac = (self.pos - i as f64) as f32;
            let a = if i == 0 { self.last } else { src[i - 1] };
            let b = src[i];
            out.push(a + (b - a) * frac);
            self.pos += self.step;
        }
        self.pos -= n;
        self.last = src[src.len() - 1];
    }
}

pub fn f32_to_i16(x: f32) -> i16 {
    (x.clamp(-1.0, 1.0) * i16::MAX as f32) as i16
}

pub fn i16_to_f32(x: i16) -> f32 {
    x as f32 / 32768.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ratio_is_preserved_across_chunks() {
        let mut r = Resampler::new(48_000, 16_000);
        let mut out = Vec::new();
        for _ in 0..100 {
            r.process(&[0.1; 480], &mut out);
        }
        assert!((out.len() as i64 - 16_000).abs() <= 1, "got {}", out.len());
    }

    #[test]
    fn upsample_ratio() {
        let mut r = Resampler::new(24_000, 48_000);
        let mut out = Vec::new();
        for _ in 0..10 {
            r.process(&[0.0; 2400], &mut out);
        }
        assert!((out.len() as i64 - 48_000).abs() <= 2, "got {}", out.len());
    }
}
