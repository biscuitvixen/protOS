//! A stand-in producer: canned curves on the bus so the face runs with
//! no tracker attached. Deterministic in time, so a run is repeatable,
//! and shaped to exercise what matters: talking bursts on the jaw and
//! the voice bands, slow asymmetric smiles, blinks, wandering gaze and
//! the odd sneer.

use std::f32::consts::TAU;

use crate::contract::{PROTOS_BAND_COUNT, band_id};

/// Whether the talking gate is open at `t`: a 0.15 Hz square wave.
fn talking(t: f32) -> bool {
    (TAU * 0.15 * t).sin() > 0.0
}

/// The syllable curve shared by the jaw and the bands: 2.3 Hz in [0, 1].
fn syllable(t: f32) -> f32 {
    0.5 + 0.5 * (TAU * 2.3 * t).sin()
}

/// Band energies at time `t`: two formant-like humps in band-index
/// space, the first wandering over roughly 300 to 650 Hz and the second
/// over 1.7 to 3.7 kHz, pulsing with the syllable curve while the
/// talking gate is open and silent otherwise.
pub fn bands(t: f32) -> [f32; PROTOS_BAND_COUNT] {
    let mut out = [0.0; PROTOS_BAND_COUNT];
    if !talking(t) {
        return out;
    }
    let level = syllable(t);
    let f1 = 8.0 + 3.0 * (TAU * 0.9 * t).sin();
    let f2 = 20.0 + 3.0 * (TAU * 0.6 * t + 2.0).sin();
    // A hump of the same width as the analyser gives a pure tone.
    let hump = |k: f32, centre: f32, height: f32| height * (-(k - centre).powi(2) / 4.5).exp();
    for (k, band) in out.iter_mut().enumerate() {
        let k = k as f32;
        let shaped = hump(k, f1, 1.0) + hump(k, f2, 0.7) + 0.04;
        *band = (level * shaped).clamp(0.0, 1.0);
    }
    out
}

/// Channel values at time `t` seconds, as (address, value) pairs.
pub fn curves(t: f32) -> Vec<(&'static str, f32)> {
    let jaw = if talking(t) { syllable(t) } else { 0.0 };
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
    let bands = bands(t);
    let level = bands.iter().cloned().fold(0.0, f32::max);
    let mut out = vec![
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
        ("/protos/voice/level", level),
    ];
    out.extend(
        bands
            .iter()
            .enumerate()
            .map(|(k, &value)| (band_id(k).spec().address, value)),
    );
    out
}

/// Send the curves to `sender` at `rate` Hz until the process ends.
#[cfg(not(target_arch = "wasm32"))]
pub fn run(sender: &crate::osc::Sender, rate: f32) -> anyhow::Result<()> {
    use crate::contract::BANDS_ADDRESS;
    use std::thread;
    use std::time::{Duration, Instant};
    let period = Duration::from_secs_f32(1.0 / rate);
    let started = Instant::now();
    let mut next = started;
    loop {
        let t = started.elapsed().as_secs_f32();
        // The bands go as one vector message, the way a real producer
        // sends them, so the receiver's vector path is what gets used.
        let first_band = band_id(0).spec().address;
        for (address, value) in curves(t) {
            if address == first_band {
                break;
            }
            sender.send(address, value)?;
        }
        sender.send_many(BANDS_ADDRESS, &bands(t))?;
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

    #[test]
    fn the_bands_light_up_while_talking_and_go_dark_between_bursts() {
        // The gate is open on (0, 3.33) s and closed on (3.33, 6.67) s.
        let loud = bands(1.0);
        assert!(
            loud.iter().any(|&b| b > 0.3),
            "some band should be lit while talking: {loud:?}"
        );
        let level = curves(1.0)
            .iter()
            .find(|(a, _)| *a == "/protos/voice/level")
            .map(|(_, v)| *v)
            .expect("the curves carry a voice level");
        assert!(level > 0.0, "the level should follow the bands");
        let quiet = bands(5.0);
        assert!(
            quiet.iter().all(|&b| b == 0.0),
            "every band should be dark between bursts: {quiet:?}"
        );
    }
}
