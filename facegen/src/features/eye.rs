//! Eye parameters: a bent ellipse with a lid, gaze offset and optional
//! pupil. See `shaders/features/eye.wgsl` for how each one is drawn.

use serde::{Deserialize, Serialize};

use super::Colour;
use crate::render::uniforms::EyeUniform;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pupil {
    pub enabled: bool,
    pub offset_mm: [f32; 2],
    /// Pupil radii as a fraction of the eye radii.
    pub scale: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EyeParams {
    pub centre_mm: [f32; 2],
    pub radii_mm: [f32; 2],
    pub rotation_deg: f32,
    /// Parabolic tip lift across the width; positive curls the tips up.
    pub bend: f32,
    /// 1 fully open, 0 closed.
    pub open: f32,
    /// 0 none, 1 lower lid fully raised (a squint from below).
    pub lower_close: f32,
    pub gaze_mm: [f32; 2],
    pub lid_tilt_deg: f32,
    /// Height multiplier, 1 = as authored.
    pub widen: f32,
    pub pupil: Pupil,
    pub colour: Colour,
}

impl EyeParams {
    pub fn pack(&self) -> EyeUniform {
        EyeUniform {
            c_r: [
                self.centre_mm[0],
                self.centre_mm[1],
                self.radii_mm[0],
                self.radii_mm[1],
            ],
            shape: [
                self.rotation_deg.to_radians(),
                self.bend,
                self.open,
                self.lower_close,
            ],
            gaze: [
                self.gaze_mm[0],
                self.gaze_mm[1],
                self.lid_tilt_deg.to_radians(),
                self.widen,
            ],
            pupil: [
                self.pupil.offset_mm[0],
                self.pupil.offset_mm[1],
                self.pupil.scale,
                if self.pupil.enabled { 1.0 } else { 0.0 },
            ],
            colour: self.colour.to_array(1.0),
        }
    }
}
