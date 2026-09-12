//! Facial features as authored parameters.
//!
//! Each feature module holds the parameter struct a face TOML fills in
//! (millimetres, degrees, sRGB colours) and packs it into the uniform
//! slot its shader reads. The rig will later modify copies of these
//! parameters per side before packing; the structs are the vocabulary
//! its gain table targets.

pub mod eye;
pub mod mouth;
pub mod nose;

use serde::{Deserialize, Serialize};

/// A colour authored as sRGB hex ("#00d0ff") and held as linear light,
/// which is what the shader blends in.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Colour {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

impl Colour {
    pub fn to_array(self, w: f32) -> [f32; 4] {
        [self.r, self.g, self.b, w]
    }
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

impl TryFrom<String> for Colour {
    type Error = String;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        let hex = text
            .strip_prefix('#')
            .ok_or_else(|| format!("colour {text:?} must be #rrggbb"))?;
        if hex.len() != 6 {
            return Err(format!("colour {text:?} must be #rrggbb"));
        }
        let channel = |i: usize| {
            u8::from_str_radix(&hex[i..i + 2], 16)
                .map(|v| srgb_to_linear(f32::from(v) / 255.0))
                .map_err(|_| format!("colour {text:?} is not hex"))
        };
        Ok(Self {
            r: channel(0)?,
            g: channel(2)?,
            b: channel(4)?,
        })
    }
}

impl From<Colour> for String {
    fn from(c: Colour) -> String {
        let byte = |v: f32| (linear_to_srgb(v).clamp(0.0, 1.0) * 255.0).round() as u8;
        format!("#{:02x}{:02x}{:02x}", byte(c.r), byte(c.g), byte(c.b))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_round_trip_through_hex_and_land_in_linear_light() {
        let c = Colour::try_from("#00d0ff".to_string()).unwrap();
        assert_eq!(c.r, 0.0, "black channel stays zero");
        assert!((c.b - 1.0).abs() < 1e-6, "full channel is 1.0 linear");
        assert!(
            (c.g - 0.6308).abs() < 1e-3,
            "0xd0 sRGB should decode to ~0.63 linear, got {}",
            c.g
        );
        assert_eq!(String::from(c), "#00d0ff", "hex round trip");
        assert!(
            Colour::try_from("00d0ff".to_string()).is_err(),
            "missing # must fail"
        );
        assert!(
            Colour::try_from("#zzzzzz".to_string()).is_err(),
            "non-hex must fail"
        );
    }
}
