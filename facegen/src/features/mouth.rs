//! Mouth parameters. Two modes share one baseline (inner point, width,
//! corner lift): `jaw` is the filled region between two lip curves with
//! a tapered opening and sawtooth teeth; `scope` is a thin line on the
//! baseline displaced by a travelling sine under a spectrum envelope.
//! See `shaders/features/mouth.wgsl`.

use serde::{Deserialize, Serialize};

use super::Colour;
use crate::render::uniforms::MouthUniform;

/// How the mouth answers the voice: the jaw opens and closes, the
/// scope draws the spectrum along the lip.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MouthMode {
    #[default]
    Jaw,
    Scope,
}

impl std::str::FromStr for MouthMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "jaw" => Ok(Self::Jaw),
            "scope" => Ok(Self::Scope),
            other => Err(format!("no mouth mode named {other:?}; jaw or scope")),
        }
    }
}

/// Scope mode tuning. The line's displacement is the band envelope
/// scaled by `amplitude_mm` while voiced, crossfading to a flat
/// `idle_mm` in silence, under a sine of `cycles` periods across the
/// mouth. The spectral centroid adds up to `centroid_cycles` more so
/// treble ripples finer. Keep the total below about 8 on a P3 panel or
/// the carrier aliases against the LED pitch.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ScopeParams {
    pub idle_mm: f32,
    pub amplitude_mm: f32,
    pub cycles: f32,
    pub centroid_cycles: f32,
    /// Carrier phase rate while voiced and while idle.
    pub speed_hz: f32,
    pub idle_speed_hz: f32,
}

impl Default for ScopeParams {
    fn default() -> Self {
        Self {
            idle_mm: 1.5,
            amplitude_mm: 10.0,
            cycles: 3.0,
            centroid_cycles: 3.0,
            speed_hz: 2.0,
            idle_speed_hz: 0.35,
        }
    }
}

/// Per-frame scope signals the rig derives from the inputs; not
/// authored, so not part of the face TOML.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ScopeLive {
    /// 0 idle to 1 voiced; the shader crossfades amplitudes on it.
    pub activity: f32,
    /// Spectral centroid, 0 bass to 1 treble.
    pub centroid: f32,
    /// Carrier phase in radians.
    pub phase: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MouthParams {
    #[serde(default)]
    pub mode: MouthMode,
    #[serde(default)]
    pub scope: ScopeParams,
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
    pub fn pack(&self, live: &ScopeLive) -> MouthUniform {
        let mode = match self.mode {
            MouthMode::Jaw => 0.0,
            MouthMode::Scope => 1.0,
        };
        let centroid = live.centroid.clamp(0.0, 1.0);
        MouthUniform {
            c_w: [
                self.inner_mm[0],
                self.inner_mm[1],
                self.width_mm,
                self.thickness_mm,
            ],
            curve: [
                self.corner_dy_mm,
                mode,
                self.open.clamp(0.0, 1.0) * self.max_open_mm,
                self.lower_dx_mm,
            ],
            lips: [self.upper_lip_dy_mm, self.lower_lip_dy_mm, 1.0, 1.0],
            teeth: [
                self.tooth_height_mm,
                self.tooth_base_mm,
                self.open_taper.clamp(0.0, 1.0),
                live.activity.clamp(0.0, 1.0),
            ],
            tooth_row: [
                self.tooth_offset_mm,
                self.tooth_pitch_mm,
                self.tooth_count as f32,
                centroid,
            ],
            scope: [
                self.scope.idle_mm,
                self.scope.amplitude_mm,
                self.scope.cycles + self.scope.centroid_cycles * centroid,
                live.phase,
            ],
            colour: self.colour.to_array(1.0),
        }
    }
}
