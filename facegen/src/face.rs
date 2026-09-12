//! A face: authored feature parameters plus the globals that shape the
//! whole render, loaded from TOML and packed into the uniform block.
//!
//! Faces are authored in millimetres for a design box; the layout's fit
//! scale maps the box onto whatever panels are present so one face
//! serves P3, P2.5 and multi-panel visors alike. Both sides receive the
//! same parameters here; per-side differences come from the rig.

use serde::{Deserialize, Serialize};

use crate::features::Colour;
use crate::features::eye::EyeParams;
use crate::features::mouth::MouthParams;
use crate::features::nose::NoseParams;
use crate::layout::Layout;
use crate::render::uniforms::{FaceUniforms, GlobalsUniform};
use crate::rig::mapping::Morph;

pub const DEFAULT_TOML: &str = include_str!("../faces/default.toml");

/// How an eye closes: squash keeps the lower edge fixed and shrinks the
/// height; cut slides a lid down across the open shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CloseMode {
    Squash,
    Cut,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Face {
    pub name: String,
    /// Width and height the face was authored for, per side.
    pub box_mm: [f32; 2],
    pub background: Colour,
    pub close_mode: CloseMode,
    pub brightness: f32,
    /// Multiplier on the voice level before it competes with jawOpen
    /// for the mouth opening.
    #[serde(default = "default_voice_gain")]
    pub voice_gain: f32,
    pub eye: EyeParams,
    pub mouth: MouthParams,
    pub nose: NoseParams,
    /// The gain table; see `rig::mapping`.
    #[serde(default, rename = "morph")]
    pub morphs: Vec<Morph>,
}

fn default_voice_gain() -> f32 {
    1.0
}

/// Per-frame values that are not part of the authored face.
#[derive(Clone, Copy, Debug, Default)]
pub struct FrameState {
    pub time_s: f32,
    pub dt_s: f32,
    pub frame: u32,
    /// Design-box fit for the current layout, from [`fit_scale`].
    pub face_scale: f32,
}

impl Face {
    pub fn from_toml(text: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(text)
    }

    pub fn default_face() -> Self {
        Self::from_toml(DEFAULT_TOML).expect("embedded default face parses")
    }

    /// The globals half of the uniform block; the rig fills the sides.
    pub fn globals(&self, state: &FrameState) -> FaceUniforms {
        FaceUniforms {
            g: GlobalsUniform {
                time: [state.time_s, state.dt_s, state.frame as f32, 0.0],
                motion: [0.0, 0.0, 1.0, self.brightness],
                face: [
                    state.face_scale,
                    match self.close_mode {
                        CloseMode::Squash => 0.0,
                        CloseMode::Cut => 1.0,
                    },
                    0.0,
                    0.0,
                ],
                bg: self.background.to_array(0.0),
                atlas: [0.0; 4],
                bands: [[0.0; 4]; 8],
            },
            sides: Default::default(),
        }
    }
}

/// Uniform scale that fits the face's design box into the panels of
/// one side: the smaller of the width and height ratios, so a face
/// authored for 192 x 96 mm renders on 160 x 80 mm panels at 0.833.
pub fn fit_scale(layout: &Layout, box_mm: [f32; 2]) -> f32 {
    let mut min = [f32::INFINITY; 2];
    let mut max = [f32::NEG_INFINITY; 2];
    for panel in &layout.panels {
        let (lo, hi) = panel.transform().bounds_mm(panel.size_px);
        min = [min[0].min(lo[0]), min[1].min(lo[1])];
        max = [max[0].max(hi[0]), max[1].max(hi[1])];
    }
    let extent = [max[0] - min[0], max[1] - min[1]];
    if !(extent[0] > 0.0 && extent[1] > 0.0) {
        return 1.0;
    }
    (extent[0] / box_mm[0]).min(extent[1] / box_mm[1])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::presets;

    #[test]
    fn the_default_face_parses_with_its_morph_table_and_globals() {
        let face = Face::default_face();
        assert_eq!(face.name, "default", "name");
        assert!(
            face.morphs.len() > 20,
            "the default face should ship a gain table"
        );
        let u = face.globals(&FrameState {
            face_scale: 1.0,
            ..Default::default()
        });
        assert_eq!(u.g.face[1], 0.0, "squash close mode packs as 0");
        assert_eq!(u.g.motion[3], face.brightness, "brightness lane");
    }

    #[test]
    fn the_fit_scale_is_one_for_the_panel_the_face_was_authored_for() {
        let face = Face::default_face();
        let two = presets::load("two_64x32").unwrap();
        assert!(
            (fit_scale(&two, face.box_mm) - 1.0).abs() < 1e-6,
            "P3 64x32 is the design box"
        );
        let mut p25 = two.clone();
        for p in &mut p25.panels {
            p.pitch_mm = 2.5;
        }
        assert!(
            (fit_scale(&p25, face.box_mm) - 160.0 / 192.0).abs() < 1e-6,
            "P2.5 fits at 0.833"
        );
    }

    #[test]
    fn a_face_survives_a_toml_round_trip() {
        let face = Face::default_face();
        let back = Face::from_toml(&toml::to_string_pretty(&face).unwrap()).unwrap();
        assert_eq!(back, face, "round trip changed the face");
    }
}
