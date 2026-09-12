//! Mouth parameters: a band from the inner point outward with a lifted
//! corner, split into lips by an opening. See
//! `shaders/features/mouth.wgsl`.

use serde::{Deserialize, Serialize};

use super::Colour;
use crate::render::uniforms::MouthUniform;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MouthParams {
    /// Inner end of the band, nearest the centre line.
    pub inner_mm: [f32; 2],
    /// Length of the band outward from the inner end.
    pub width_mm: f32,
    /// Band half-height when closed.
    pub thickness_mm: f32,
    /// Lift of the outer corner; positive smiles.
    pub corner_dy_mm: f32,
    /// 0 closed, 1 opened to `max_open_mm`.
    pub open: f32,
    pub max_open_mm: f32,
    /// Sideways shear of the lower lip (jaw left/right).
    pub lower_dx_mm: f32,
    pub upper_lip_dy_mm: f32,
    pub lower_lip_dy_mm: f32,
    pub colour: Colour,
}

impl MouthParams {
    pub fn pack(&self) -> MouthUniform {
        MouthUniform {
            c_w: [
                self.inner_mm[0],
                self.inner_mm[1],
                self.width_mm,
                self.thickness_mm,
            ],
            curve: [
                self.corner_dy_mm,
                0.0,
                self.open.clamp(0.0, 1.0) * self.max_open_mm,
                self.lower_dx_mm,
            ],
            lips: [self.upper_lip_dy_mm, self.lower_lip_dy_mm, 1.0, 1.0],
            teeth: [0.0; 4],
            tongue: [0.0; 4],
            colour: self.colour.to_array(1.0),
        }
    }
}
