//! Blendshapes in, shader parameters out.
//!
//! Each tick the rig smooths the raw inputs, applies the voice fallback
//! (the mouth follows the louder of jawOpen and the voice envelope), and
//! walks the face's gain table to build one parameter set per side
//! from the authored base values. Left-suffixed shapes move the left
//! rig, Right-suffixed the right, unsuffixed both; a row may override
//! that. Everything the table can move is a plain parameter field, so
//! adding a morph is a TOML row, not code.

pub mod mapping;
pub mod smooth;

use crate::contract::{INPUT_COUNT, lookup_name};
use crate::face::{Face, FrameState};
use crate::render::uniforms::{FaceUniforms, LEFT, RIGHT, SideUniform};
use mapping::{MorphRow, SideParams};
use smooth::Smoother;

pub struct Rig {
    rows: Vec<MorphRow>,
    voice_gain: f32,
    smoother: Smoother,
    weights: [f32; INPUT_COUNT],
    sides: [SideParams; 2],
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

    pub fn sides(&self) -> &[SideParams; 2] {
        &self.sides
    }

    pub fn weights(&self) -> &[f32; INPUT_COUNT] {
        &self.weights
    }

    pub fn pack(&self, face: &Face, state: &FrameState) -> FaceUniforms {
        let mut u = face.globals(state);
        for (slot, side) in u.sides.iter_mut().zip(&self.sides) {
            *slot = SideUniform {
                eye: side.eye.pack(),
                mouth: side.mouth.pack(),
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
