//! Blendshapes in, shader parameters out.
//!
//! Each tick the rig smooths the raw inputs, applies the voice fallback
//! (the mouth follows the louder of jawOpen and the voice envelope), and
//! walks the face's gain table to build one parameter set per side
//! from the authored base values. Left-suffixed shapes move the left
//! rig, Right-suffixed the right, unsuffixed both; a row may override
//! that. Everything the table can move is a plain parameter field, so
//! adding a morph is a TOML row, not code.
//!
//! The rig also owns the scope mouth's running state: a voice activity
//! follower, the smoothed spectral centroid and the carrier phase. The
//! phase advances here rather than in the shader so its rate can slide
//! between the idle and voiced speeds without the line jumping.

pub mod mapping;
pub mod smooth;

use std::f32::consts::TAU;

use crate::contract::{BANDS_FIRST, INPUT_COUNT, PROTOS_BAND_COUNT, lookup_name};
use crate::face::{Face, FrameState};
use crate::features::mouth::ScopeLive;
use crate::render::uniforms::{FaceUniforms, LEFT, RIGHT, SideUniform};
use mapping::{MorphRow, SideParams};
use smooth::Smoother;

/// Activity rises fast enough to catch a syllable onset and falls slowly
/// enough to bridge the gaps between syllables.
const ACTIVITY_ATTACK_S: f32 = 0.020;
const ACTIVITY_RELEASE_S: f32 = 0.250;
/// The centroid is a slow quantity; smoothing keeps the carrier
/// frequency from fluttering between hops.
const CENTROID_TAU_S: f32 = 0.080;
/// Below this band sum the centroid is noise and holds its last value.
const CENTROID_MIN_SUM: f32 = 0.02;

/// Running state of the scope mouth.
#[derive(Clone, Copy, Debug, Default)]
struct ScopeState {
    activity: f32,
    centroid: f32,
    phase: f32,
}

pub struct Rig {
    rows: Vec<MorphRow>,
    voice_gain: f32,
    smoother: Smoother,
    weights: [f32; INPUT_COUNT],
    sides: [SideParams; 2],
    scope: ScopeState,
}

/// One-pole step of `dt` seconds toward `target` with time constant `tau`.
fn ease(value: f32, target: f32, dt: f32, tau: f32) -> f32 {
    let alpha = 1.0 - (-dt / tau.max(1e-4)).exp();
    value + alpha * (target - value)
}

impl Rig {
    /// Compile the face's morph table; an unknown shape name is an error
    /// so a typo in a TOML row fails at load, not silently at runtime.
    pub fn new(face: &Face) -> anyhow::Result<Self> {
        let rows = face
            .morphs
            .iter()
            .map(|m| m.compile().map_err(anyhow::Error::msg))
            .collect::<anyhow::Result<Vec<_>>>()?;
        let base = SideParams {
            eye: face.eye.clone(),
            mouth: face.mouth.clone(),
            nose: face.nose.clone(),
        };
        Ok(Self {
            rows,
            voice_gain: face.voice_gain,
            smoother: Smoother::new(),
            weights: [0.0; INPUT_COUNT],
            sides: [base.clone(), base],
            scope: ScopeState::default(),
        })
    }

    /// Swap in a reloaded face: recompile its table and keep the
    /// smoothing state so the face does not jump.
    pub fn replace_face(&mut self, face: &Face) -> anyhow::Result<()> {
        let fresh = Rig::new(face)?;
        self.rows = fresh.rows;
        self.voice_gain = fresh.voice_gain;
        self.sides = fresh.sides;
        Ok(())
    }

    /// Smooth the raw inputs and rebuild both sides' parameters.
    pub fn update(&mut self, face: &Face, raw: &[f32; INPUT_COUNT], dt: f32) {
        self.weights = *self.smoother.update(raw, dt);
        let jaw = lookup_name("jawOpen")
            .expect("contract has jawOpen")
            .index();
        let voice = lookup_name("voiceLevel")
            .expect("contract has voiceLevel")
            .index();
        self.weights[jaw] = self.weights[jaw].max(self.voice_gain * self.weights[voice]);
        self.advance_scope(face, dt);
        for (i, side) in self.sides.iter_mut().enumerate() {
            side.eye = face.eye.clone();
            side.mouth = face.mouth.clone();
            side.nose = face.nose.clone();
            for row in &self.rows {
                let drives = if i == LEFT { row.left } else { row.right };
                if drives {
                    side.apply(
                        row.target,
                        row.gain * (self.weights[row.input.index()] + row.offset),
                    );
                }
            }
            side.clamp();
        }
    }

    /// Follow the voice with the activity, centroid and carrier phase.
    /// A zero or invalid `dt` snaps the followers and leaves the phase
    /// alone, so a single-frame render is deterministic.
    fn advance_scope(&mut self, face: &Face, dt: f32) {
        let bands = &self.weights[BANDS_FIRST..BANDS_FIRST + PROTOS_BAND_COUNT];
        let voice = lookup_name("voiceLevel")
            .expect("contract has voiceLevel")
            .index();
        let loudest = bands.iter().cloned().fold(self.weights[voice], f32::max);
        let sum: f32 = bands.iter().sum();
        let centroid = if sum > CENTROID_MIN_SUM {
            let weighted: f32 = bands.iter().enumerate().map(|(k, b)| k as f32 * b).sum();
            Some(weighted / ((PROTOS_BAND_COUNT - 1) as f32 * sum))
        } else {
            None
        };
        if dt.is_nan() || dt <= 0.0 {
            self.scope.activity = loudest;
            if let Some(c) = centroid {
                self.scope.centroid = c;
            }
            return;
        }
        let tau = if loudest > self.scope.activity {
            ACTIVITY_ATTACK_S
        } else {
            ACTIVITY_RELEASE_S
        };
        self.scope.activity = ease(self.scope.activity, loudest, dt, tau);
        if let Some(c) = centroid {
            self.scope.centroid = ease(self.scope.centroid, c, dt, CENTROID_TAU_S);
        }
        let s = &face.mouth.scope;
        let rate = s.idle_speed_hz + (s.speed_hz - s.idle_speed_hz) * self.scope.activity;
        self.scope.phase = (self.scope.phase + TAU * rate * dt) % TAU;
    }

    pub fn sides(&self) -> &[SideParams; 2] {
        &self.sides
    }

    pub fn weights(&self) -> &[f32; INPUT_COUNT] {
        &self.weights
    }

    pub fn pack(&self, face: &Face, state: &FrameState) -> FaceUniforms {
        let mut u = face.globals(state);
        for k in 0..PROTOS_BAND_COUNT {
            u.g.bands[k / 4][k % 4] = self.weights[BANDS_FIRST + k];
        }
        let live = ScopeLive {
            activity: self.scope.activity,
            centroid: self.scope.centroid,
            phase: self.scope.phase,
        };
        for (slot, side) in u.sides.iter_mut().zip(&self.sides) {
            *slot = SideUniform {
                eye: side.eye.pack(),
                mouth: side.mouth.pack(&live),
                nose: side.nose.pack(),
                cheek: Default::default(),
            };
        }
        let _ = RIGHT;
        u
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::InputStore;

    fn weights_with(pairs: &[(&str, f32)]) -> [f32; INPUT_COUNT] {
        let mut store = InputStore::new();
        for (name, value) in pairs {
            store.set(
                lookup_name(name).unwrap(),
                *value,
                std::time::Instant::now(),
            );
        }
        *store.values()
    }

    #[test]
    fn a_resting_input_store_reproduces_the_authored_face_on_both_sides() {
        let face = Face::default_face();
        let mut rig = Rig::new(&face).unwrap();
        rig.update(&face, &InputStore::new().values().clone(), 1.0 / 60.0);
        for side in rig.sides() {
            assert_eq!(side.eye, face.eye, "eye should be untouched at rest");
            assert_eq!(side.mouth, face.mouth, "mouth should be untouched at rest");
            assert_eq!(side.nose, face.nose, "nose should be untouched at rest");
        }
    }

    #[test]
    fn jaw_open_opens_the_mouth_on_both_sides_and_the_voice_level_is_a_fallback() {
        let face = Face::default_face();
        let mut rig = Rig::new(&face).unwrap();
        rig.update(&face, &weights_with(&[("jawOpen", 1.0)]), 1.0 / 60.0);
        assert_eq!(rig.sides()[LEFT].mouth.open, 1.0, "left mouth open");
        assert_eq!(rig.sides()[RIGHT].mouth.open, 1.0, "right mouth open");
        let mut rig = Rig::new(&face).unwrap();
        rig.update(&face, &weights_with(&[("voiceLevel", 0.5)]), 1.0 / 60.0);
        assert!(
            (rig.sides()[LEFT].mouth.open - 0.5 * face.voice_gain).abs() < 1e-6,
            "voice level should drive the mouth when the jaw is quiet"
        );
    }

    #[test]
    fn a_left_smile_moves_only_the_left_corner() {
        let face = Face::default_face();
        let mut rig = Rig::new(&face).unwrap();
        rig.update(&face, &weights_with(&[("mouthSmileLeft", 1.0)]), 1.0 / 60.0);
        assert!(
            rig.sides()[LEFT].mouth.corner_dy_mm > face.mouth.corner_dy_mm,
            "left corner should lift"
        );
        assert_eq!(
            rig.sides()[RIGHT].mouth.corner_dy_mm,
            face.mouth.corner_dy_mm,
            "right corner should not move"
        );
    }

    #[test]
    fn a_blink_closes_the_eye_and_a_missing_lid_signal_leaves_it_open() {
        let face = Face::default_face();
        let mut rig = Rig::new(&face).unwrap();
        rig.update(&face, &weights_with(&[("eyeBlinkRight", 1.0)]), 1.0 / 60.0);
        assert_eq!(rig.sides()[RIGHT].eye.open, 0.0, "right eye should close");
        assert_eq!(rig.sides()[LEFT].eye.open, 1.0, "left eye should stay open");
        let mut rig = Rig::new(&face).unwrap();
        rig.update(&face, &weights_with(&[("eyeLeftLid", 0.25)]), 1.0 / 60.0);
        assert!(
            (rig.sides()[LEFT].eye.open - 0.25).abs() < 1e-6,
            "lid openness should set the eye openness"
        );
    }

    #[test]
    fn gaze_to_the_wearers_right_moves_both_eyes_the_same_way_on_the_face() {
        // +x on the bus is the wearer's right. On the right rig that is
        // outward (+x face); on the left rig it is inward (-x face).
        let face = Face::default_face();
        let mut rig = Rig::new(&face).unwrap();
        rig.update(
            &face,
            &weights_with(&[("eyeLeftX", 1.0), ("eyeRightX", 1.0)]),
            1.0 / 60.0,
        );
        assert!(
            rig.sides()[RIGHT].eye.gaze_mm[0] > 0.0,
            "right eye should look outward"
        );
        assert!(
            rig.sides()[LEFT].eye.gaze_mm[0] < 0.0,
            "left eye should look inward"
        );
    }

    #[test]
    fn bands_land_in_the_globals_and_the_mode_packs_into_its_lane() {
        use crate::features::mouth::MouthMode;
        let mut face = Face::default_face();
        face.mouth.mode = MouthMode::Jaw;
        let mut rig = Rig::new(&face).unwrap();
        rig.update(
            &face,
            &weights_with(&[("voiceBand17", 0.75), ("voiceBand31", 0.25)]),
            0.0,
        );
        let u = rig.pack(&face, &FrameState::default());
        assert_eq!(u.g.bands[4][1], 0.75, "band 17 is lane 4.y");
        assert_eq!(u.g.bands[7][3], 0.25, "band 31 is lane 7.w");
        assert_eq!(u.g.bands[0][0], 0.0, "band 0 is silent");
        assert_eq!(u.sides[LEFT].mouth.curve[1], 0.0, "jaw packs as 0");
        assert_eq!(
            u.sides[LEFT].mouth.teeth[3], 0.75,
            "activity snaps to the loudest band on a zero dt"
        );
        face.mouth.mode = MouthMode::Scope;
        let mut rig = Rig::new(&face).unwrap();
        rig.update(&face, &InputStore::new().values().clone(), 0.0);
        let u = rig.pack(&face, &FrameState::default());
        assert_eq!(u.sides[RIGHT].mouth.curve[1], 1.0, "scope packs as 1");
        assert_eq!(
            u.sides[RIGHT].mouth.scope,
            [1.5, 10.0, 3.0, 0.0],
            "scope lane carries idle, amplitude, cycles and a zero phase"
        );
    }

    #[test]
    fn the_carrier_phase_holds_on_a_zero_dt_and_advances_otherwise() {
        let face = Face::default_face();
        let mut rig = Rig::new(&face).unwrap();
        let rest = *InputStore::new().values();
        rig.update(&face, &rest, 0.0);
        assert_eq!(rig.scope.phase, 0.0, "no time has passed");
        rig.update(&face, &rest, 1.0 / 60.0);
        let idle_step = rig.scope.phase;
        assert!(
            (idle_step - TAU * 0.35 / 60.0).abs() < 1e-5,
            "idle phase advances at the idle rate, got {idle_step}"
        );
        // The input smoother's 30 ms and the 20 ms attack stack.
        for _ in 0..4 {
            rig.update(&face, &weights_with(&[("voiceLevel", 1.0)]), 1.0 / 60.0);
        }
        assert!(
            rig.scope.activity > 0.5,
            "activity should rise within four frames of full voice, got {}",
            rig.scope.activity
        );
        for _ in 0..6 {
            rig.update(&face, &rest, 1.0 / 60.0);
        }
        assert!(
            rig.scope.activity > 0.3,
            "activity should still bridge a 100 ms gap, got {}",
            rig.scope.activity
        );
        for _ in 0..120 {
            rig.update(&face, &rest, 1.0 / 60.0);
        }
        assert!(
            rig.scope.activity < 0.01,
            "activity should release within two seconds, got {}",
            rig.scope.activity
        );
    }

    #[test]
    fn a_morph_row_naming_an_unknown_shape_fails_at_load() {
        let mut face = Face::default_face();
        face.morphs.push(mapping::Morph {
            shape: "jawOpenn".into(),
            target: mapping::Target::MouthOpen,
            gain: 1.0,
            offset: 0.0,
            side: None,
        });
        assert!(
            Rig::new(&face).is_err(),
            "typo in a shape name must be rejected"
        );
    }
}
