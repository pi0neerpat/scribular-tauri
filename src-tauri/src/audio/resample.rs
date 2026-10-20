//! Streaming conversion of any interleaved f32 format to 16 kHz mono, the
//! rate whisper takes. Downsampling averages each output sample's window of
//! input (a box filter, enough to keep speech-band aliasing out); upsampling
//! interpolates linearly. Electron decimated by an integer factor instead.

use super::Format;
use crate::stt::SAMPLE_RATE;

pub struct Resampler {
    channels: usize,
    /// Input samples per output sample.
    step: f64,
    /// Samples of a frame split across two calls.
    partial: Vec<f32>,
    /// Downmixed input not yet fully consumed.
    pending: Vec<f32>,
    /// Start of the next output sample, as an index into `pending`.
    pos: f64,
}

impl Resampler {
    pub fn new(format: Format) -> Self {
        Self {
            channels: usize::from(format.channels.max(1)),
            step: f64::from(format.sample_rate) / f64::from(SAMPLE_RATE),
            partial: Vec::new(),
            pending: Vec::new(),
            pos: 0.0,
        }
    }

    /// Appends the 16 kHz mono samples that `interleaved` completes to `out`.
    /// A trailing partial frame waits for the next call.
    pub fn process(&mut self, mut interleaved: &[f32], out: &mut Vec<f32>) {
        let n = self.channels as f32;
        if !self.partial.is_empty() {
            let take = (self.channels - self.partial.len()).min(interleaved.len());
            self.partial.extend_from_slice(&interleaved[..take]);
            interleaved = &interleaved[take..];
            if self.partial.len() < self.channels {
                return;
            }
            self.pending.push(self.partial.iter().sum::<f32>() / n);
            self.partial.clear();
        }
        let frames = interleaved.chunks_exact(self.channels);
        self.partial.extend_from_slice(frames.remainder());
        self.pending
            .extend(frames.map(|frame| frame.iter().sum::<f32>() / n));
        let len = self.pending.len() as f64;
        if self.step >= 1.0 {
            while self.pos + self.step <= len {
                out.push(box_average(&self.pending, self.pos, self.pos + self.step));
                self.pos += self.step;
            }
        } else {
            while self.pos.floor() + 1.0 < len {
                let i = self.pos.floor() as usize;
                let f = (self.pos - i as f64) as f32;
                out.push(self.pending[i] * (1.0 - f) + self.pending[i + 1] * f);
                self.pos += self.step;
            }
        }
        let consumed = (self.pos.floor() as usize).min(self.pending.len());
        self.pending.drain(..consumed);
        self.pos -= consumed as f64;
    }
}

/// Mean of the piecewise-constant signal `x` over `[a, b)`, `b <= x.len()`.
fn box_average(x: &[f32], a: f64, b: f64) -> f32 {
    let mut sum = 0.0;
    let mut i = a.floor() as usize;
    while (i as f64) < b {
        let overlap = b.min(i as f64 + 1.0) - a.max(i as f64);
        sum += f64::from(x[i]) * overlap;
        i += 1;
    }
    (sum / (b - a)) as f32
}

/// Full-scale f32 to i16, clipping out-of-range samples.
pub fn to_i16(sample: f32) -> i16 {
    (sample.clamp(-1.0, 1.0) * f32::from(i16::MAX)).round() as i16
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(rate: u32, channels: u16, seconds: f32, hz: f32, amp: f32) -> Vec<f32> {
        let frames = (rate as f32 * seconds) as usize;
        (0..frames)
            .flat_map(|i| {
                let v = amp * (2.0 * std::f32::consts::PI * hz * i as f32 / rate as f32).sin();
                std::iter::repeat_n(v, usize::from(channels))
            })
            .collect()
    }

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|s| s * s).sum::<f32>() / x.len() as f32).sqrt()
    }

    fn run(format: Format, input: &[f32], block: usize) -> Vec<f32> {
        let mut r = Resampler::new(format);
        let mut out = Vec::new();
        for chunk in input.chunks(block) {
            r.process(chunk, &mut out);
        }
        out
    }

    #[test]
    fn stereo_48k_to_16k_mono_keeps_count_and_level() {
        let format = Format {
            sample_rate: 48_000,
            channels: 2,
        };
        let input = sine(48_000, 2, 1.0, 440.0, 0.5);
        let out = run(format, &input, 1024);
        assert_eq!(out.len(), 16_000);
        let expected = 0.5 / 2f32.sqrt();
        let got = rms(&out);
        assert!((got - expected).abs() / expected < 0.02, "rms {got}");
    }

    #[test]
    fn block_boundaries_do_not_change_the_output() {
        let format = Format {
            sample_rate: 44_100,
            channels: 2,
        };
        let input = sine(44_100, 2, 0.5, 300.0, 0.8);
        let whole = run(format, &input, input.len());
        // Odd block sizes split frames across calls.
        let split = run(format, &input, 333);
        assert_eq!(whole.len(), split.len());
        for (a, b) in whole.iter().zip(&split) {
            assert!((a - b).abs() < 1e-5);
        }
        assert!((whole.len() as i64 - 8_000).abs() <= 1, "{}", whole.len());
    }

    #[test]
    fn upsamples_8k_by_interpolation() {
        let format = Format {
            sample_rate: 8_000,
            channels: 1,
        };
        let out = run(format, &sine(8_000, 1, 1.0, 200.0, 0.5), 500);
        assert!((out.len() as i64 - 16_000).abs() <= 2, "{}", out.len());
        let expected = 0.5 / 2f32.sqrt();
        assert!((rms(&out) - expected).abs() / expected < 0.02);
    }

    #[test]
    fn downmix_averages_channels() {
        let format = Format {
            sample_rate: 16_000,
            channels: 2,
        };
        let input: Vec<f32> = (0..100).flat_map(|_| [0.5, -0.5]).collect();
        let out = run(format, &input, 64);
        assert_eq!(out.len(), 100);
        assert!(out.iter().all(|s| s.abs() < 1e-6));
    }

    #[test]
    fn to_i16_clips() {
        assert_eq!(to_i16(2.0), i16::MAX);
        assert_eq!(to_i16(-2.0), -i16::MAX);
        assert_eq!(to_i16(0.0), 0);
    }
}
