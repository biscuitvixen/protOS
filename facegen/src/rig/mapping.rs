//! The gain table: which input moves which feature parameter, by how
//! much, on which side. Rows are authored in the face TOML as
//! `[[morph]]` tables and compiled once into dense input indices.

use serde::{Deserialize, Serialize};

use crate::contract::{self, InputId, Side as InputSide};
use crate::features::eye::EyeParams;
use crate::features::mouth::MouthParams;
use crate::features::nose::NoseParams;
use crate::layout::Side;

/// A feature parameter a morph row can move. Names are the TOML
/// spelling; units follow the parameter (mm, degrees, or a weight).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Target {
    #[serde(rename = "eye.open")]
    EyeOpen,
    #[serde(rename = "eye.lower_close")]
    EyeLowerClose,
    #[serde(rename = "eye.widen")]
    EyeWiden,
    #[serde(rename = "eye.gaze_x")]
    EyeGazeX,
    #[serde(rename = "eye.gaze_y")]
    EyeGazeY,
    #[serde(rename = "eye.lid_tilt")]
    EyeLidTilt,
    #[serde(rename = "eye.bend")]
    EyeBend,
    #[serde(rename = "mouth.open")]
    MouthOpen,
    #[serde(rename = "mouth.width")]
    MouthWidth,
    #[serde(rename = "mouth.thickness")]
    MouthThickness,
    #[serde(rename = "mouth.corner_dy")]
    MouthCornerDy,
    #[serde(rename = "mouth.inner_x")]
    MouthInnerX,
    #[serde(rename = "mouth.lower_dx")]
    MouthLowerDx,
    #[serde(rename = "mouth.upper_lip_dy")]
    MouthUpperLipDy,
    #[serde(rename = "mouth.lower_lip_dy")]
    MouthLowerLipDy,
    #[serde(rename = "nose.lift")]
    NoseLift,
    #[serde(rename = "nose.rotation")]
    NoseRotation,
}

/// One authored row: `value += gain * (weight + offset)` on the target.
/// `side` overrides the side implied by the input's name.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Morph {
    pub shape: String,
    pub target: Target,
    pub gain: f32,
    #[serde(default)]
    pub offset: f32,
    #[serde(default)]
    pub side: Option<Side>,
}

/// A row resolved to an input index and the sides it drives.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MorphRow {
    pub input: InputId,
    pub target: Target,
    pub gain: f32,
    pub offset: f32,
    pub left: bool,
    pub right: bool,
}

impl Morph {
    pub fn compile(&self) -> Result<MorphRow, String> {
        let input = contract::lookup_name(&self.shape)
            .ok_or_else(|| format!("morph row names unknown shape {:?}", self.shape))?;
        let (left, right) = match self.side {
            Some(Side::Left) => (true, false),
            Some(Side::Right) => (false, true),
            None => match input.spec().side {
                InputSide::Left => (true, false),
                InputSide::Right => (false, true),
                InputSide::Both => (true, true),
            },
        };
        Ok(MorphRow {
            input,
            target: self.target,
            gain: self.gain,
            offset: self.offset,
            left,
            right,
        })
    }
}

/// The three features of one side, as the rig moves them.
#[derive(Clone, Debug, PartialEq)]
pub struct SideParams {
    pub eye: EyeParams,
    pub mouth: MouthParams,
    pub nose: NoseParams,
}

impl SideParams {
    pub fn apply(&mut self, target: Target, delta: f32) {
        match target {
            Target::EyeOpen => self.eye.open += delta,
            Target::EyeLowerClose => self.eye.lower_close += delta,
            Target::EyeWiden => self.eye.widen += delta,
            Target::EyeGazeX => self.eye.gaze_mm[0] += delta,
            Target::EyeGazeY => self.eye.gaze_mm[1] += delta,
            Target::EyeLidTilt => self.eye.lid_tilt_deg += delta,
            Target::EyeBend => self.eye.bend += delta,
            Target::MouthOpen => self.mouth.open += delta,
            Target::MouthWidth => self.mouth.width_mm += delta,
            Target::MouthThickness => self.mouth.thickness_mm += delta,
            Target::MouthCornerDy => self.mouth.corner_dy_mm += delta,
            Target::MouthInnerX => self.mouth.inner_mm[0] += delta,
            Target::MouthLowerDx => self.mouth.lower_dx_mm += delta,
            Target::MouthUpperLipDy => self.mouth.upper_lip_dy_mm += delta,
            Target::MouthLowerLipDy => self.mouth.lower_lip_dy_mm += delta,
            Target::NoseLift => self.nose.nostril_lift_mm += delta,
            Target::NoseRotation => self.nose.rotation_deg += delta,
        }
    }

    /// Keep every parameter inside the range its shader expects.
    pub fn clamp(&mut self) {
        self.eye.open = self.eye.open.clamp(0.0, 1.0);
        self.eye.lower_close = self.eye.lower_close.clamp(0.0, 1.0);
        self.eye.widen = self.eye.widen.max(0.05);
        self.eye.bend = self.eye.bend.clamp(-0.6, 0.6);
        self.mouth.open = self.mouth.open.clamp(0.0, 1.0);
        self.mouth.width_mm = self.mouth.width_mm.max(1.0);
        self.mouth.thickness_mm = self.mouth.thickness_mm.max(0.3);
    }
}
