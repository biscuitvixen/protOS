//! A stand-in producer: canned curves on the bus so the face runs with
//! no tracker attached. Deterministic in time, so a run is repeatable,
//! and shaped to exercise what matters: talking bursts on the jaw,
//! slow asymmetric smiles, blinks, wandering gaze and the odd sneer.

use std::f32::consts::TAU;
use std::thread;
use std::time::{Duration, Instant};

use crate::osc::Sender;

/// Channel values at time `t` seconds, as (address, value) pairs.
pub fn curves(t: f32) -> Vec<(&'static str, f32)> {
    let talking = (TAU * 0.15 * t).sin() > 0.0;
    let jaw = if talking {
        0.5 + 0.5 * (TAU * 2.3 * t).sin()
    } else {
        0.0
    };
    let smile_l = 0.5 + 0.5 * (TAU * 0.07 * t).sin();
    let smile_r = 0.5 + 0.5 * (TAU * 0.07 * t + 1.1).sin();
    // A 150 ms blink every 3.7 s, shaped as a raised cosine.
    let phase = t % 3.7;
    let blink = if phase < 0.15 {
        0.5 - 0.5 * (TAU * phase / 0.15).cos()
    } else {
        0.0
    };
    let gaze_x = 0.6 * (TAU * 0.11 * t).sin();
    let gaze_y = 0.4 * (TAU * 0.13 * t + 1.0).sin();
    let sneer = ((TAU * 0.05 * t).sin() - 0.8).max(0.0) * 5.0;
    vec![
        ("/jawOpen", jaw),
        ("/mouthSmileLeft", smile_l.max(0.0)),
        ("/mouthSmileRight", smile_r.max(0.0)),
        ("/noseSneerLeft", sneer.min(1.0)),
        ("/protos/eye/left/lid", 1.0 - blink),
        ("/protos/eye/right/lid", 1.0 - blink),
        ("/protos/eye/left/x", gaze_x),
        ("/protos/eye/right/x", gaze_x),
        ("/protos/eye/left/y", gaze_y),
        ("/protos/eye/right/y", gaze_y),
    ]
}

/// Send the curves to `sender` at `rate` Hz until the process ends.
pub fn run(sender: &Sender, rate: f32) -> anyhow::Result<()> {
    let period = Duration::from_secs_f32(1.0 / rate);
    let started = Instant::now();
    let mut next = started;
    loop {
        let t = started.elapsed().as_secs_f32();
        for (address, value) in curves(t) {
            sender.send(address, value)?;
        }
        next += period;
        if let Some(wait) = next.checked_duration_since(Instant::now()) {
            thread::sleep(wait);
        } else {
            next = Instant::now();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::lookup_address;

    #[test]
    fn every_fake_channel_is_in_the_contract_and_within_range() {
        for t in [0.0, 0.37, 1.9, 4.2, 11.0] {
            for (address, value) in curves(t) {
                let id = lookup_address(address)
                    .unwrap_or_else(|| panic!("{address} is not a contract address"));
                let clamped = id.spec().range.clamp(value);
                assert_eq!(
                    clamped, value,
                    "{address} at t={t} is out of range: {value}"
                );
            }
        }
    }
}
