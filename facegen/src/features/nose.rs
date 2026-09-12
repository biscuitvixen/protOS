//! Nose parameters: a small bent ellipse near the snout tip. See
//! `shaders/features/nose.wgsl`.

use serde::{Deserialize, Serialize};

use super::Colour;
use crate::render::uniforms::NoseUniform;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NoseParams {
    pub centre_mm: [f32; 2],
    pub radii_mm: [f32; 2],
    pub rotation_deg: f32,
    pub bend: f32,
    /// Sneer: raises the nose and tips it outward.
    pub nostril_lift_mm: f32,
    pub colour: Colour,
}

impl NoseParams {
    pub fn pack(&self) -> NoseUniform {
        NoseUniform {
            c_r: [
                self.centre_mm[0],
                self.centre_mm[1],
                self.radii_mm[0],
                self.radii_mm[1],
            ],
            shape: [
                self.rotation_deg.to_radians(),
                self.bend,
                self.nostril_lift_mm,
                0.0,
            ],
            colour: self.colour.to_array(1.0),
        }
    }
}
