//! Mouth parameters: the filled region between two lip curves from the
//! inner point outward, with a lifted corner, a tapered opening and a
//! sawtooth teeth on both lips. See
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
    /// Height of each tooth beyond its lip line; 0 for smooth lips.
    #[serde(default)]
    pub tooth_height_mm: f32,
    /// Width of a tooth at its base. Keep it at or below the pitch so
    /// neighbouring teeth do not overlap.
    #[serde(default = "default_tooth_base")]
    pub tooth_base_mm: f32,
    /// Centre of the first lower tooth, outward from the inner end; the
    /// upper row sits half a pitch further out.
    #[serde(default)]
    pub tooth_offset_mm: f32,
    /// Spacing between tooth centres.
    #[serde(default = "default_tooth_pitch")]
    pub tooth_pitch_mm: f32,
    /// Number of teeth in each row; 0 for none.
    #[serde(default)]
    pub tooth_count: u32,
    /// How much the opening narrows toward the corner: 0 keeps the gap
    /// even, 1 closes it fully at the corner (a triangle).
    #[serde(default)]
    pub open_taper: f32,
    pub colour: Colour,
}

fn default_tooth_base() -> f32 {
    8.0
}

fn default_tooth_pitch() -> f32 {
    20.0
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
            teeth: [
                self.tooth_height_mm,
                self.tooth_base_mm,
                self.open_taper.clamp(0.0, 1.0),
                0.0,
            ],
            tooth_row: [
                self.tooth_offset_mm,
                self.tooth_pitch_mm,
                self.tooth_count as f32,
                0.0,
            ],
            tongue: [0.0; 4],
            colour: self.colour.to_array(1.0),
        }
    }
}
