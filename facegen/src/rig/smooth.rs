//! Per-channel smoothing of the raw inputs.
//!
//! A one-pole low-pass per input, with the time constant chosen by the
//! input's source: tracker outputs are already filtered upstream and
//! must stay responsive, eye channels carry saccades that must not be
//! blurred, the voice envelope is smoothed a little harder. The filter
//! is expressed in seconds so the result does not depend on frame rate.

use crate::contract::{INPUT_COUNT, INPUTS, Source};

/// Time constants in seconds, by source.
fn tau(source: Source) -> f32 {
    match source {
        Source::Babble | Source::ArKit => 0.040,
        Source::ProtosEye => 0.012,
        Source::ProtosVoice => 0.030,
        // The analyser already applies attack and release; this only
        // bridges its 100 Hz readouts to the frame rate.
        Source::ProtosBands => 0.020,
    }
}

pub struct Smoother {
    y: [f32; INPUT_COUNT],
    primed: bool,
}

impl Default for Smoother {
    fn default() -> Self {
        Self::new()
    }
}

impl Smoother {
    pub fn new() -> Self {
        Self {
            y: [0.0; INPUT_COUNT],
            primed: false,
        }
    }

    /// Advance by `dt` seconds toward `raw`. The first call snaps to
    /// the input so the face never eases in from all zeros.
    pub fn update(&mut self, raw: &[f32; INPUT_COUNT], dt: f32) -> &[f32; INPUT_COUNT] {
        if !self.primed || dt.is_nan() || dt <= 0.0 {
            self.y = *raw;
            self.primed = true;
            return &self.y;
        }
        for ((y, x), spec) in self.y.iter_mut().zip(raw).zip(INPUTS.iter()) {
            let alpha = 1.0 - (-dt / tau(spec.source)).exp();
            *y += alpha * (x - *y);
        }
        &self.y
    }

    pub fn values(&self) -> &[f32; INPUT_COUNT] {
        &self.y
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::lookup_name;

    #[test]
    fn the_first_update_snaps_and_later_updates_converge_with_the_time_constant() {
        let mut s = Smoother::new();
        let jaw = lookup_name("jawOpen").unwrap().index();
        let mut raw = [0.0; INPUT_COUNT];
        raw[jaw] = 1.0;
        assert_eq!(
            s.update(&raw, 1.0 / 60.0)[jaw],
            1.0,
            "first update should snap"
        );
        raw[jaw] = 0.0;
        let after_one_tau = {
            let mut v = 0.0;
            for _ in 0..40 {
                v = s.update(&raw, 0.001)[jaw];
            }
            v
        };
        assert!(
            (after_one_tau - (-1.0f32).exp()).abs() < 0.01,
            "after one tau (40 ms) a step should decay to 1/e, got {after_one_tau}"
        );
    }
}
