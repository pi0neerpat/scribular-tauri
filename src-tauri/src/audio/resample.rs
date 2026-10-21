//! Streaming conversion of any interleaved f32 format to 16 kHz mono, the
//! rate whisper takes. Downsampling uses a Kaiser-windowed-sinc low-pass
//! filter to suppress aliasing; upsampling interpolates linearly.
//! Electron decimated by an integer factor instead.

use super::Format;
use crate::stt::SAMPLE_RATE;

pub struct Resampler {
    channels: usize,
    /// Input samples per output sample.
    step: f64,
    method: Method,
    /// Samples of a frame split across two calls.
    partial: Vec<f32>,
    /// Downmixed input not yet fully consumed.
    pending: Vec<f32>,
    /// Centre of the next output sample, as an index into `pending`.
    pos: f64,
}

enum Method {
    Copy,
    Interpolate,
    Decimate(LowPass),
}

impl Resampler {
    pub fn new(format: Format) -> Self {
        let step = f64::from(format.sample_rate) / f64::from(SAMPLE_RATE);
        let method = if step > 1.0 {
            Method::Decimate(LowPass::new(step))
        } else if step == 1.0 {
            Method::Copy
        } else {
            Method::Interpolate
        };
        let padding = match &method {
            Method::Decimate(filter) => filter.half_in.ceil() as usize,
            _ => 0,
        };
        Self {
            channels: usize::from(format.channels.max(1)),
            step,
            method,
            partial: Vec::new(),
            pending: vec![0.0; padding],
            pos: padding as f64,
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
        let consumed = match &self.method {
            Method::Copy => {
                out.append(&mut self.pending);
                return;
            }
            Method::Interpolate => {
                while self.pos.floor() + 1.0 < len {
                    let i = self.pos.floor() as usize;
                    let f = (self.pos - i as f64) as f32;
                    out.push(self.pending[i] * (1.0 - f) + self.pending[i + 1] * f);
                    self.pos += self.step;
                }
                self.pos.floor() as usize
            }
            Method::Decimate(filter) => {
                while (self.pos + filter.half_in).floor() < len {
                    out.push(filter.apply(&self.pending, self.pos));
                    self.pos += self.step;
                }
                (self.pos - filter.half_in).floor() as usize
            }
        };
        let consumed = consumed.min(self.pending.len());
        self.pending.drain(..consumed);
        self.pos -= consumed as f64;
    }
}

/// Kaiser-windowed-sinc low-pass for decimating by `step`. The half kernel is
/// tabulated once; `apply` interpolates it at each tap's fractional offset.
struct LowPass {
    table: Vec<f32>,
    /// Taps on each side of a centre, in input samples.
    half_in: f64,
}

impl LowPass {
    /// Cycles per output sample: 7.2 kHz at 16 kHz.
    const CUTOFF: f64 = 0.45;
    /// Kernel reach on each side, in output samples.
    const HALF_WIDTH: f64 = 20.0;
    /// Kaiser window shape, about 70 dB of stop-band rejection.
    const BETA: f64 = 7.0;
    /// Table entries per input sample.
    const RESOLUTION: f64 = 256.0;

    fn new(step: f64) -> Self {
        let half_in = Self::HALF_WIDTH * step;
        let fc = Self::CUTOFF / step;
        let denominator = bessel_i0(Self::BETA);
        let table = (0..=(half_in * Self::RESOLUTION).ceil() as usize)
            .map(|i| {
                let d = (i as f64 / Self::RESOLUTION).min(half_in);
                let arg = 2.0 * fc * d * std::f64::consts::PI;
                let sinc = if arg == 0.0 { 1.0 } else { arg.sin() / arg };
                let u = d / half_in;
                let window = bessel_i0(Self::BETA * (1.0 - u * u).sqrt()) / denominator;
                (2.0 * fc * sinc * window) as f32
            })
            .collect();
        Self { table, half_in }
    }

    fn apply(&self, x: &[f32], centre: f64) -> f32 {
        let start = (centre - self.half_in).ceil() as usize;
        let end = (centre + self.half_in).floor() as usize;
        let mut sum = 0.0;
        let mut weights = 0.0;
        for (offset, &sample) in x[start..=end].iter().enumerate() {
            let index = ((start + offset) as f64 - centre).abs() * Self::RESOLUTION;
            let i = index.floor() as usize;
            let fraction = index - i as f64;
            let a = f64::from(self.table[i]);
            let b = f64::from(self.table[(i + 1).min(self.table.len() - 1)]);
            let weight = a + (b - a) * fraction;
            sum += f64::from(sample) * weight;
            weights += weight;
        }
        (sum / weights) as f32
    }
}

fn bessel_i0(x: f64) -> f64 {
    let mut sum = 1.0;
    let mut term = 1.0;
    let mut k = 1.0;
    loop {
        term *= (x / (2.0 * k)).powi(2);
        sum += term;
        if term < sum * 1e-15 {
            return sum;
        }
        k += 1.0;
    }
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
                let v = amp
                    * (2.0 * std::f64::consts::PI * hz as f64 * i as f64 / rate as f64).sin()
                        as f32;
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
        assert!((16_000 - 21..=16_000).contains(&out.len()), "{}", out.len());
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
        assert!(
            (8_000 - 21..=8_000).contains(&whole.len()),
            "{}",
            whole.len()
        );
    }

    #[test]
    fn downsampling_rejects_stopband_tones() {
        for (rate, tones) in [
            (
                48_000,
                &[9_000.0, 12_000.0, 16_000.0, 20_000.0, 23_000.0][..],
            ),
            (44_100, &[12_000.0, 10_000.0][..]),
        ] {
            for &hz in tones {
                let input = sine(rate, 1, 1.0, hz, 0.5);
                let out = run(
                    Format {
                        sample_rate: rate,
                        channels: 1,
                    },
                    &input,
                    1024,
                );
                let ratio = rms(&out[100..out.len() - 100]) / rms(&input);
                assert!(ratio < 1e-3, "{hz} Hz at {rate}: rms ratio {ratio}");
            }
        }
    }

    #[test]
    fn downsampling_preserves_passband_tones() {
        for rate in [48_000, 44_100] {
            for hz in [440.0, 1_000.0, 5_000.0] {
                let input = sine(rate, 1, 1.0, hz, 0.5);
                let out = run(
                    Format {
                        sample_rate: rate,
                        channels: 1,
                    },
                    &input,
                    1024,
                );
                let gain = rms(&out[100..out.len() - 100]) / rms(&input);
                assert!((gain - 1.0).abs() < 0.01, "{hz} Hz at {rate}: gain {gain}");
            }
        }
    }

    #[test]
    fn downsampling_preserves_dc() {
        let out = run(
            Format {
                sample_rate: 48_000,
                channels: 1,
            },
            &vec![0.5; 48_000],
            1024,
        );
        assert!(out[50..].iter().all(|sample| (sample - 0.5).abs() < 1e-4));
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
