//! Physical panel layout and the map from LED pixels into face-space.
//!
//! Face-space is defined per visor side in millimetres: x = 0 on the
//! centre line, +x outward toward that side's ear, +y up. Both sides
//! share the same coordinates; the wearer's right side is rendered
//! mirrored by construction of its transform, so one face definition
//! serves both halves. A panel is a window onto face-space described
//! by its centre, pitch, size and mounting rotation. Bezel gaps and
//! upside-down mounting fall out of the per-panel affine, so nothing
//! downstream (shaders, driver, web sim) needs a special case.
//!
//! The affine for a panel maps its electrical pixel grid (u along the
//! shift direction, v down the panel in its reference mounting) to
//! face-space: face_mm(u, v) = O + M (u, v), with
//! M = pitch * diag(sigma, -1) * R_rot, sigma = +1 for the wearer's
//! left side and -1 for the right, and R_rot the mounting rotation as
//! the viewer sees it, counter-clockwise, in a y-down screen frame.
//! O is derived from the panel centre so authors never type origins.
//!
//! Driver constraints are validated here because they shape what a
//! layout may contain: one Piomatter instance drives every connector
//! from shared clock and address lines, so all panels must have the
//! same scan depth (the same height), and every connector's chain must
//! have the same total width.

pub mod atlas;
pub mod presets;

use serde::{Deserialize, Serialize};

/// Which visor side a panel sits on (the wearer's own left or right).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Left,
    Right,
}

impl Side {
    /// Sign applied to the outward axis: +1 on the left, -1 on the right.
    pub fn sigma(self) -> f32 {
        match self {
            Side::Left => 1.0,
            Side::Right => -1.0,
        }
    }
}

/// Mounting rotation of a panel, counter-clockwise as the viewer sees
/// it, relative to the reference mounting where a plain image appears
/// upright and unmirrored.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "u16", into = "u16")]
pub enum Rotation {
    R0,
    R90,
    R180,
    R270,
}

impl Rotation {
    /// (cos, sin) of the rotation, exact for the four right angles.
    fn cos_sin(self) -> (f32, f32) {
        match self {
            Rotation::R0 => (1.0, 0.0),
            Rotation::R90 => (0.0, 1.0),
            Rotation::R180 => (-1.0, 0.0),
            Rotation::R270 => (0.0, -1.0),
        }
    }
}

impl TryFrom<u16> for Rotation {
    type Error = String;

    fn try_from(degrees: u16) -> Result<Self, Self::Error> {
        match degrees {
            0 => Ok(Rotation::R0),
            90 => Ok(Rotation::R90),
            180 => Ok(Rotation::R180),
            270 => Ok(Rotation::R270),
            other => Err(format!("rotation must be 0, 90, 180 or 270, got {other}")),
        }
    }
}

impl From<Rotation> for u16 {
    fn from(rotation: Rotation) -> u16 {
        match rotation {
            Rotation::R0 => 0,
            Rotation::R90 => 90,
            Rotation::R180 => 180,
            Rotation::R270 => 270,
        }
    }
}

/// Piomatter pinout, which fixes how many connectors exist and whether
/// the panels expect green and blue swapped (Adafruit's P2.5 panels).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Pinout {
    AdafruitMatrixBonnet,
    AdafruitMatrixBonnetBgr,
    Active3,
    Active3Bgr,
}

impl Pinout {
    pub fn connectors(self) -> u8 {
        match self {
            Pinout::AdafruitMatrixBonnet | Pinout::AdafruitMatrixBonnetBgr => 1,
            Pinout::Active3 | Pinout::Active3Bgr => 3,
        }
    }
}

/// Settings shared by every panel on the Pi.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Driver {
    pub pinout: Pinout,
    /// Row address lines: 3 for 1/8-scan (16-row) panels, 4 for 1/16
    /// (32-row), 5 for 1/32 (64-row). Sets the panel height as
    /// 2 * 2^n_addr_lines.
    pub n_addr_lines: u8,
    /// Bit planes shown, 1..=10. Fewer planes refresh faster.
    pub n_planes: u8,
    /// Low bit planes cycled across passes instead of shown every
    /// pass; 0 disables temporal dithering. Must be below n_planes.
    pub n_temporal_planes: u8,
}

impl Driver {
    /// Rows every panel must have for this scan depth.
    pub fn panel_height(&self) -> u32 {
        2u32 << self.n_addr_lines
    }
}

/// One physical LED panel.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Panel {
    pub name: String,
    pub side: Side,
    /// Width and height in pixels.
    pub size_px: [u32; 2],
    /// LED pitch in mm; the panel's physical size is size_px * pitch.
    pub pitch_mm: f32,
    /// Centre of the panel in face-space mm.
    pub centre_mm: [f32; 2],
    pub rotation: Rotation,
    /// Connector on the driver board: 0 on a Bonnet, 0..3 on an Active3.
    pub connector: u8,
    /// Position along that connector's chain, 0 nearest the Pi.
    pub chain_index: u8,
}

impl Panel {
    pub fn width(&self) -> u32 {
        self.size_px[0]
    }

    pub fn height(&self) -> u32 {
        self.size_px[1]
    }

    /// The affine from electrical pixels into face-space mm.
    pub fn transform(&self) -> PanelTransform {
        let p = self.pitch_mm;
        let sigma = self.side.sigma();
        let (c, s) = self.rotation.cos_sin();
        // M = p * diag(sigma, -1) * [[c, s], [-s, c]], stored by column.
        let col_u = [p * sigma * c, p * s];
        let col_v = [p * sigma * s, -p * c];
        let (w, h) = (self.width() as f32, self.height() as f32);
        let origin = [
            self.centre_mm[0] - (col_u[0] * w + col_v[0] * h) * 0.5,
            self.centre_mm[1] - (col_u[1] * w + col_v[1] * h) * 0.5,
        ];
        PanelTransform {
            col_u,
            col_v,
            origin,
            px_mm: p,
        }
    }
}

/// Affine from a panel's electrical pixel grid into face-space mm.
///
/// `col_u` and `col_v` are the columns of M: the face-space
/// displacement for one step along u and along v. Each column has
/// length `px_mm`, so the panel's physical footprint is
/// |col_u| * width by |col_v| * height.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PanelTransform {
    pub col_u: [f32; 2],
    pub col_v: [f32; 2],
    pub origin: [f32; 2],
    pub px_mm: f32,
}

impl PanelTransform {
    /// Face-space position of electrical coordinate (u, v); pixel
    /// centres sit at half-integers.
    pub fn face_mm(&self, u: f32, v: f32) -> [f32; 2] {
        [
            self.origin[0] + self.col_u[0] * u + self.col_v[0] * v,
            self.origin[1] + self.col_u[1] * u + self.col_v[1] * v,
        ]
    }

    pub fn pixel_centre_mm(&self, u: u32, v: u32) -> [f32; 2] {
        self.face_mm(u as f32 + 0.5, v as f32 + 0.5)
    }

    /// Axis-aligned bounds of the panel's LED grid in face-space,
    /// as (min, max), from its four corners.
    pub fn bounds_mm(&self, size_px: [u32; 2]) -> ([f32; 2], [f32; 2]) {
        let (w, h) = (size_px[0] as f32, size_px[1] as f32);
        let corners = [
            self.face_mm(0.0, 0.0),
            self.face_mm(w, 0.0),
            self.face_mm(0.0, h),
            self.face_mm(w, h),
        ];
        let mut min = corners[0];
        let mut max = corners[0];
        for c in &corners[1..] {
            min = [min[0].min(c[0]), min[1].min(c[1])];
            max = [max[0].max(c[0]), max[1].max(c[1])];
        }
        (min, max)
    }
}

/// A complete panel arrangement plus the driver settings it needs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Layout {
    pub name: String,
    pub driver: Driver,
    #[serde(rename = "panel")]
    pub panels: Vec<Panel>,
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum LayoutError {
    #[error("layout has no panels")]
    NoPanels,
    #[error("panel name {0:?} is used more than once")]
    DuplicateName(String),
    #[error("panel {name:?} has size {size:?}; both dimensions must be positive")]
    BadSize { name: String, size: [u32; 2] },
    #[error("panel {name:?} has pitch {pitch_mm}; it must be positive")]
    BadPitch { name: String, pitch_mm: f32 },
    #[error(
        "panel {name:?} is {height} rows tall but {n_addr_lines} address lines give {expected}-row panels; every panel on one Pi shares the address lines and must have the same scan depth"
    )]
    ScanDepthMismatch {
        name: String,
        height: u32,
        n_addr_lines: u8,
        expected: u32,
    },
    #[error("panel {name:?} uses connector {connector} but the {pinout:?} pinout has {available}")]
    NoSuchConnector {
        name: String,
        connector: u8,
        pinout: Pinout,
        available: u8,
    },
    #[error(
        "connector {connector} chain indices must be 0..n with no gaps or repeats, got {indices:?}"
    )]
    BadChainOrder { connector: u8, indices: Vec<u8> },
    #[error(
        "connector {connector} chain is {width} pixels across but connector {reference} is {reference_width}; all connectors share one clock and must match"
    )]
    UnequalChainWidth {
        connector: u8,
        width: u32,
        reference: u8,
        reference_width: u32,
    },
    #[error("n_addr_lines must be 1..=5, got {0}")]
    BadAddrLines(u8),
    #[error("n_planes must be 1..=10, got {0}")]
    BadPlanes(u8),
    #[error("n_temporal_planes ({temporal}) must be below n_planes ({planes})")]
    BadTemporalPlanes { temporal: u8, planes: u8 },
}

impl Layout {
    pub fn from_toml(text: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(text)
    }

    pub fn to_toml(&self) -> Result<String, toml::ser::Error> {
        toml::to_string_pretty(self)
    }

    /// Check the driver constraints. A layout that passes can be
    /// packed into an atlas and driven by Piomatter.
    pub fn validate(&self) -> Result<(), LayoutError> {
        let d = &self.driver;
        if !(1..=5).contains(&d.n_addr_lines) {
            return Err(LayoutError::BadAddrLines(d.n_addr_lines));
        }
        if !(1..=10).contains(&d.n_planes) {
            return Err(LayoutError::BadPlanes(d.n_planes));
        }
        if d.n_temporal_planes >= d.n_planes {
            return Err(LayoutError::BadTemporalPlanes {
                temporal: d.n_temporal_planes,
                planes: d.n_planes,
            });
        }
        if self.panels.is_empty() {
            return Err(LayoutError::NoPanels);
        }
        let expected_height = d.panel_height();
        for (i, p) in self.panels.iter().enumerate() {
            if self.panels[..i].iter().any(|q| q.name == p.name) {
                return Err(LayoutError::DuplicateName(p.name.clone()));
            }
            if p.width() == 0 || p.height() == 0 {
                return Err(LayoutError::BadSize {
                    name: p.name.clone(),
                    size: p.size_px,
                });
            }
            if p.pitch_mm.is_nan() || p.pitch_mm <= 0.0 {
                return Err(LayoutError::BadPitch {
                    name: p.name.clone(),
                    pitch_mm: p.pitch_mm,
                });
            }
            if p.height() != expected_height {
                return Err(LayoutError::ScanDepthMismatch {
                    name: p.name.clone(),
                    height: p.height(),
                    n_addr_lines: d.n_addr_lines,
                    expected: expected_height,
                });
            }
            if p.connector >= d.pinout.connectors() {
                return Err(LayoutError::NoSuchConnector {
                    name: p.name.clone(),
                    connector: p.connector,
                    pinout: d.pinout,
                    available: d.pinout.connectors(),
                });
            }
        }
        let mut reference: Option<(u8, u32)> = None;
        for connector in self.connectors() {
            let mut indices: Vec<u8> = self
                .panels
                .iter()
                .filter(|p| p.connector == connector)
                .map(|p| p.chain_index)
                .collect();
            indices.sort_unstable();
            if indices
                .iter()
                .enumerate()
                .any(|(i, &c)| usize::from(c) != i)
            {
                return Err(LayoutError::BadChainOrder { connector, indices });
            }
            let width = self.chain_width(connector);
            match reference {
                None => reference = Some((connector, width)),
                Some((r, rw)) if rw != width => {
                    return Err(LayoutError::UnequalChainWidth {
                        connector,
                        width,
                        reference: r,
                        reference_width: rw,
                    });
                }
                Some(_) => {}
            }
        }
        Ok(())
    }

    /// Connectors in use, ascending.
    pub fn connectors(&self) -> Vec<u8> {
        let mut c: Vec<u8> = self.panels.iter().map(|p| p.connector).collect();
        c.sort_unstable();
        c.dedup();
        c
    }

    /// Total pixels across one connector's chain.
    pub fn chain_width(&self, connector: u8) -> u32 {
        self.panels
            .iter()
            .filter(|p| p.connector == connector)
            .map(Panel::width)
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn panel(side: Side, rotation: Rotation) -> Panel {
        Panel {
            name: "p".into(),
            side,
            size_px: [64, 32],
            pitch_mm: 3.0,
            centre_mm: [96.0, 0.0],
            rotation,
            connector: 0,
            chain_index: 0,
        }
    }

    fn assert_close(actual: [f32; 2], expected: [f32; 2], what: &str) {
        for i in 0..2 {
            assert!(
                (actual[i] - expected[i]).abs() < 1e-4,
                "{what}: got {actual:?}, expected {expected:?}"
            );
        }
    }

    #[test]
    fn a_left_panel_mounted_upright_maps_u_outward_and_v_downward() {
        let t = panel(Side::Left, Rotation::R0).transform();
        assert_close(t.col_u, [3.0, 0.0], "col_u");
        assert_close(t.col_v, [0.0, -3.0], "col_v");
        assert_close(t.origin, [0.0, 48.0], "origin");
        assert_close(t.pixel_centre_mm(0, 0), [1.5, 46.5], "inner top pixel");
        assert_close(
            t.pixel_centre_mm(63, 31),
            [190.5, -46.5],
            "outer bottom pixel",
        );
    }

    #[test]
    fn a_right_panel_mounted_upside_down_has_the_identity_scaled_by_pitch() {
        // Mirror and 180-degree mounting cancel: M = 3 * I.
        let t = panel(Side::Right, Rotation::R180).transform();
        assert_close(t.col_u, [3.0, 0.0], "col_u");
        assert_close(t.col_v, [0.0, 3.0], "col_v");
        assert_close(t.origin, [0.0, -48.0], "origin");
        assert_close(t.pixel_centre_mm(0, 0), [1.5, -46.5], "inner bottom pixel");
        assert_close(t.pixel_centre_mm(63, 31), [190.5, 46.5], "outer top pixel");
    }

    #[test]
    fn a_right_panel_mounted_upright_is_the_mirror_of_the_left_one() {
        let t = panel(Side::Right, Rotation::R0).transform();
        assert_close(t.col_u, [-3.0, 0.0], "col_u");
        assert_close(t.col_v, [0.0, -3.0], "col_v");
        assert_close(t.origin, [192.0, 48.0], "origin");
        assert_close(t.pixel_centre_mm(0, 0), [190.5, 46.5], "outer top pixel");
    }

    #[test]
    fn a_portrait_mounting_swaps_the_footprint_axes() {
        let t = panel(Side::Left, Rotation::R90).transform();
        assert_close(t.col_u, [0.0, 3.0], "col_u points up");
        assert_close(t.col_v, [3.0, 0.0], "col_v points outward");
        let (min, max) = t.bounds_mm([64, 32]);
        assert_close(min, [48.0, -96.0], "portrait min");
        assert_close(max, [144.0, 96.0], "portrait max");
    }

    #[test]
    fn every_transform_keeps_the_panel_centred_on_its_centre() {
        for side in [Side::Left, Side::Right] {
            for rotation in [Rotation::R0, Rotation::R90, Rotation::R180, Rotation::R270] {
                let t = panel(side, rotation).transform();
                assert_close(t.face_mm(32.0, 16.0), [96.0, 0.0], "centre drifted");
                let (min, max) = t.bounds_mm([64, 32]);
                let extent = [max[0] - min[0], max[1] - min[1]];
                let expected = match rotation {
                    Rotation::R0 | Rotation::R180 => [192.0, 96.0],
                    Rotation::R90 | Rotation::R270 => [96.0, 192.0],
                };
                assert_close(extent, expected, "footprint");
            }
        }
    }

    #[test]
    fn rotation_only_accepts_the_four_right_angles() {
        assert_eq!(
            Rotation::try_from(270),
            Ok(Rotation::R270),
            "270 should parse"
        );
        assert!(Rotation::try_from(45).is_err(), "45 should be rejected");
        assert_eq!(u16::from(Rotation::R90), 90, "round trip");
    }

    fn valid_layout() -> Layout {
        Layout::from_toml(presets::TWO_64X32).expect("preset parses")
    }

    #[test]
    fn the_validator_accepts_the_presets() {
        for name in presets::NAMES {
            let layout = presets::load(name).expect("preset exists");
            assert_eq!(layout.validate(), Ok(()), "preset {name} failed validation");
        }
    }

    #[test]
    fn the_validator_rejects_a_panel_with_the_wrong_scan_depth() {
        let mut layout = valid_layout();
        layout.panels[0].size_px = [32, 16];
        assert!(
            matches!(
                layout.validate(),
                Err(LayoutError::ScanDepthMismatch {
                    height: 16,
                    expected: 32,
                    ..
                })
            ),
            "a 16-row panel on a 4-address-line driver must be rejected"
        );
    }

    #[test]
    fn the_validator_rejects_a_connector_the_pinout_does_not_have() {
        let mut layout = valid_layout();
        layout.panels[1].connector = 1;
        assert!(
            matches!(
                layout.validate(),
                Err(LayoutError::NoSuchConnector {
                    connector: 1,
                    available: 1,
                    ..
                })
            ),
            "a Bonnet has one connector"
        );
    }

    #[test]
    fn the_validator_rejects_gaps_and_repeats_in_a_chain() {
        let mut layout = valid_layout();
        layout.panels[1].chain_index = 2;
        assert!(
            matches!(layout.validate(), Err(LayoutError::BadChainOrder { .. })),
            "chain index 2 with no index 1 must be rejected"
        );
        layout.panels[1].chain_index = 0;
        assert!(
            matches!(layout.validate(), Err(LayoutError::BadChainOrder { .. })),
            "two panels at chain index 0 must be rejected"
        );
    }

    #[test]
    fn the_validator_rejects_connectors_with_different_chain_widths() {
        let mut layout = presets::load("six_panel").unwrap();
        layout.panels.pop();
        assert!(
            matches!(
                layout.validate(),
                Err(LayoutError::UnequalChainWidth { .. })
            ),
            "a two-panel chain beside a three-panel chain must be rejected"
        );
    }

    #[test]
    fn the_validator_rejects_bad_driver_settings() {
        let mut layout = valid_layout();
        layout.driver.n_temporal_planes = 8;
        assert!(
            matches!(
                layout.validate(),
                Err(LayoutError::BadTemporalPlanes { .. })
            ),
            "temporal planes must be below planes"
        );
        layout.driver.n_planes = 11;
        assert!(
            matches!(layout.validate(), Err(LayoutError::BadPlanes(11))),
            "11 planes must be rejected"
        );
    }

    #[test]
    fn a_layout_survives_a_toml_round_trip() {
        let layout = valid_layout();
        let text = layout.to_toml().expect("serialises");
        let back = Layout::from_toml(&text).expect("re-parses");
        assert_eq!(back, layout, "round trip changed the layout");
    }
}
