//! Turns audio into the spectrum's frames: 32 log-spaced bands of a Hann-windowed FFT of the mono
//! mix, 30 times a second of audio.

use std::f32::consts::TAU;
use std::ops::Range;
use std::sync::Arc;

use realfft::num_complex::Complex;
use realfft::{RealFftPlanner, RealToComplex};

pub const FFT_SIZE: usize = 2048;
/// The mockup's 32 columns.
pub const BANDS: usize = 32;
pub const FRAMES_PER_S: u32 = 30;
pub const LOW_HZ: f32 = 50.0;
pub const HIGH_HZ: f32 = 16_000.0;
/// The level a band reads 0 at; 0 dBFS reads 255.
pub const FLOOR_DB: f32 = -60.0;
/// The least share of its previous value a band keeps from one frame to the next.
pub const FALL: f32 = 0.8;

pub type Bands = [u8; BANDS];

/// The band `hz` falls in; frequencies below [`LOW_HZ`] or from [`HIGH_HZ`] up count as the
/// lowest or highest band.
pub fn band_of(hz: f32) -> usize {
    let position = (hz / LOW_HZ).ln() / (HIGH_HZ / LOW_HZ).ln();
    // A negative or NaN position saturates to 0.
    ((position * BANDS as f32).floor() as usize).min(BANDS - 1)
}

/// The geometric middle of `band`.
fn centre_hz(band: usize) -> f32 {
    let position = (band as f32 + 0.5) / BANDS as f32;
    LOW_HZ * (HIGH_HZ / LOW_HZ).powf(position)
}

pub struct Analyzer {
    fft: Arc<dyn RealToComplex<f32>>,
    window: Vec<f32>,
    /// The latest [`FFT_SIZE`] mono samples, a ring whose oldest sample is at `next`.
    history: Vec<f32>,
    next: usize,
    frames_per_output: usize,
    frames_since_output: usize,
    /// The FFT bins each band takes its peak from.
    band_bins: Vec<Range<usize>>,
    /// Turns a bin's squared magnitude into the squared amplitude of a sine centred on it.
    power_scale: f32,
    input: Vec<f32>,
    spectrum: Vec<Complex<f32>>,
    scratch: Vec<Complex<f32>>,
    bands: Bands,
}

impl Analyzer {
    /// For audio sampled at `rate` Hz.
    pub fn new(rate: u32) -> Self {
        let fft = RealFftPlanner::new().plan_fft_forward(FFT_SIZE);
        let window: Vec<f32> = (0..FFT_SIZE)
            .map(|n| 0.5 - 0.5 * (TAU * n as f32 / FFT_SIZE as f32).cos())
            .collect();
        // A sine of amplitude A centred on a bin gives it a magnitude of A / 2 times the window's sum.
        let amplitude_scale = 2.0 / window.iter().sum::<f32>();
        Self {
            input: fft.make_input_vec(),
            spectrum: fft.make_output_vec(),
            scratch: fft.make_scratch_vec(),
            fft,
            window,
            history: vec![0.0; FFT_SIZE],
            next: 0,
            frames_per_output: (rate / FRAMES_PER_S) as usize,
            frames_since_output: 0,
            band_bins: band_bins(rate),
            power_scale: amplitude_scale * amplitude_scale,
            bands: [0; BANDS],
        }
    }

    /// Takes `interleaved` audio of `channels` channels, which must be at least 1, and appends a
    /// frame to `out` each time another `rate / 30` frames have arrived.
    pub fn push(&mut self, interleaved: &[f32], channels: usize, out: &mut Vec<Bands>) {
        for frame in interleaved.chunks_exact(channels) {
            self.history[self.next] = frame.iter().sum::<f32>() / channels as f32;
            self.next = (self.next + 1) % FFT_SIZE;
            self.frames_since_output += 1;
            if self.frames_since_output == self.frames_per_output {
                self.frames_since_output = 0;
                out.push(self.analyze());
            }
        }
    }

    fn analyze(&mut self) -> Bands {
        let (newer, older) = self.history.split_at(self.next);
        let in_order = older.iter().chain(newer);
        for ((input, &sample), &weight) in self.input.iter_mut().zip(in_order).zip(&self.window) {
            *input = sample * weight;
        }
        self.fft
            .process_with_scratch(&mut self.input, &mut self.spectrum, &mut self.scratch)
            .expect("the buffers come from the plan, so their lengths match it");
        for (band, bins) in self.band_bins.iter().enumerate() {
            let peak_power = self.spectrum[bins.clone()]
                .iter()
                .map(Complex::norm_sqr)
                .fold(0.0, f32::max);
            let fallen = (f32::from(self.bands[band]) * FALL) as u8;
            self.bands[band] = level(peak_power * self.power_scale).max(fallen);
        }
        self.bands
    }
}

/// A squared amplitude in full-scale units as a band's value.
fn level(power: f32) -> u8 {
    let db = 10.0 * power.log10();
    let unit = ((db - FLOOR_DB) / -FLOOR_DB).clamp(0.0, 1.0);
    (unit * f32::from(u8::MAX)).round() as u8
}

/// The bins whose centres fall in each band. The lowest bands are narrower than a bin, so one that
/// holds no centre takes the bin nearest its middle instead of staying dark.
fn band_bins(rate: u32) -> Vec<Range<usize>> {
    let bin_hz = rate as f32 / FFT_SIZE as f32;
    let last_bin = FFT_SIZE / 2;
    let band_of_bin = |bin: usize| {
        let hz = bin as f32 * bin_hz;
        (LOW_HZ..HIGH_HZ).contains(&hz).then(|| band_of(hz))
    };
    (0..BANDS)
        .map(|band| {
            let mut centred = (1..=last_bin).filter(|&bin| band_of_bin(bin) == Some(band));
            match (centred.next(), centred.next_back()) {
                (Some(first), last) => first..last.unwrap_or(first) + 1,
                (None, _) => {
                    let nearest = ((centre_hz(band) / bin_hz).round() as usize).min(last_bin);
                    nearest..nearest + 1
                }
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::f64::consts::TAU;

    use super::*;

    const RATE: u32 = 48_000;
    const ONE_SECOND: usize = 48_000;
    const CHUNK_FRAMES: usize = 1024;
    const MONO: usize = 1;
    const STEREO: usize = 2;
    const TONE_HZ: f32 = 1000.0;
    const FULL_SCALE: f32 = 1.0;
    const BRIGHT: u8 = 200;

    /// `frames` samples of a sine at `hz` that peaks at `amplitude`.
    fn sine(hz: f32, amplitude: f32, frames: usize) -> Vec<f32> {
        (0..frames)
            .map(|n| {
                let phase = TAU * f64::from(hz) * n as f64 / f64::from(RATE);
                amplitude * phase.sin() as f32
            })
            .collect()
    }

    fn interleave(left: &[f32], right: &[f32]) -> Vec<f32> {
        left.iter()
            .zip(right)
            .flat_map(|(&left, &right)| [left, right])
            .collect()
    }

    /// Every frame `analyzer` emits for `interleaved`, pushed the way PipeWire hands it over.
    fn analyze(analyzer: &mut Analyzer, interleaved: &[f32], channels: usize) -> Vec<[u8; 32]> {
        let mut out = Vec::new();
        for chunk in interleaved.chunks(CHUNK_FRAMES * channels) {
            analyzer.push(chunk, channels, &mut out);
        }
        out
    }

    fn loudest_band(bands: &[u8; 32]) -> usize {
        (0..BANDS).max_by_key(|&band| bands[band]).unwrap()
    }

    #[test]
    fn silence_is_dark() {
        let mut out = Vec::new();

        Analyzer::new(RATE).push(&vec![0.0; ONE_SECOND * STEREO], STEREO, &mut out);

        assert_eq!(out.len(), 30);
        assert!(out.iter().flatten().all(|&band| band == 0), "{out:?}");
    }

    #[test]
    fn thirty_frames_per_second() {
        let tone = sine(TONE_HZ, FULL_SCALE, ONE_SECOND);

        let out = analyze(&mut Analyzer::new(RATE), &interleave(&tone, &tone), STEREO);

        assert_eq!(out.len(), 30);
    }

    #[test]
    fn a_1khz_tone_peaks_in_its_band() {
        let out = analyze(
            &mut Analyzer::new(RATE),
            &sine(TONE_HZ, FULL_SCALE, ONE_SECOND),
            MONO,
        );

        let last = out.last().unwrap();
        assert_eq!(loudest_band(last), band_of(TONE_HZ), "{last:?}");
        assert!(last[band_of(TONE_HZ)] >= BRIGHT, "{last:?}");
    }

    #[test]
    fn stereo_mixes_to_mono() {
        let left = sine(TONE_HZ, FULL_SCALE, ONE_SECOND);
        let silent = vec![0.0; ONE_SECOND];

        let stereo = analyze(
            &mut Analyzer::new(RATE),
            &interleave(&left, &silent),
            STEREO,
        );
        let mono = analyze(
            &mut Analyzer::new(RATE),
            &sine(TONE_HZ, FULL_SCALE / 2.0, ONE_SECOND),
            MONO,
        );

        assert_eq!(stereo.len(), mono.len());
        for (stereo, mono) in stereo.iter().zip(&mono) {
            for (stereo, mono) in stereo.iter().zip(mono) {
                assert!(stereo.abs_diff(*mono) <= 1, "{stereo} vs {mono}");
            }
        }
    }

    #[test]
    fn bands_fall_gradually() {
        let mut analyzer = Analyzer::new(RATE);
        let lit = analyze(&mut analyzer, &sine(TONE_HZ, FULL_SCALE, ONE_SECOND), MONO);
        let fading = analyze(&mut analyzer, &vec![0.0; ONE_SECOND], MONO);

        let frames: Vec<&[u8; 32]> = lit.last().into_iter().chain(&fading).collect();
        for pair in frames.windows(2) {
            for (&before, &after) in pair[0].iter().zip(pair[1]) {
                let floor = (f32::from(before) * FALL) as u8;
                assert!(after >= floor, "{before} fell to {after}");
            }
        }
        assert!(
            fading.last().unwrap().iter().all(|&band| band == 0),
            "{:?}",
            fading.last()
        );
    }
}
