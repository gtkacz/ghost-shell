//! Synthetic telemetry for `ghostd run --fake`, shaped like the reference machine so the shell can
//! be developed and tested anywhere: two CCDs of 16 threads, a dead WD Green, an unreadable sensor
//! and an RTX 4080 SUPER. Every value follows a sine over the sample count, so runs repeat exactly.

mod feeds;
mod notifications;
mod sampled;

pub use feeds::feeds;
pub use sampled::sampler;

use std::f64::consts::TAU;

/// A sine between `low` and `high` that repeats every `period` samples, starting `phase` of a
/// period in.
#[derive(Clone, Copy)]
struct Wave {
    low: f32,
    high: f32,
    period: f64,
    phase: f64,
}

impl Wave {
    const fn new(low: f32, high: f32, period: f64) -> Self {
        Self {
            low,
            high,
            period,
            phase: 0.0,
        }
    }

    const fn shifted(self, phase: f64) -> Self {
        Self { phase, ..self }
    }

    fn at(self, sample: u64) -> f32 {
        let angle = TAU * (sample as f64 / self.period + self.phase);
        let unit = (angle.sin() + 1.0) / 2.0;
        self.low + (self.high - self.low) * unit as f32
    }
}
