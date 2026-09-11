//! Packing of panels into the single render target.
//!
//! The atlas is the LED driver's electrical framebuffer: connectors
//! stack as rows, and a connector's chain runs along x in chain order,
//! so the readback bytes go to the driver in slot order with no remap.
//! Rotation and mirroring never appear here; they live in each panel's
//! face-space transform.

use serde::Serialize;

use super::{Layout, LayoutError};

/// Where a panel's pixels sit in the atlas, in atlas pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct AtlasRect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// Atlas size and one rect per panel, in the layout's panel order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Atlas {
    pub width: u32,
    pub height: u32,
    pub rects: Vec<AtlasRect>,
}

impl Atlas {
    /// Pack a validated layout. Validation runs here too so a caller
    /// cannot pack something the driver would reject.
    pub fn build(layout: &Layout) -> Result<Self, LayoutError> {
        layout.validate()?;
        let row_height = layout.driver.panel_height();
        let connectors = layout.connectors();
        let width = layout.chain_width(connectors[0]);
        let rects = layout
            .panels
            .iter()
            .map(|p| {
                let row = connectors
                    .iter()
                    .position(|&c| c == p.connector)
                    .expect("connector listed") as u32;
                let x = layout
                    .panels
                    .iter()
                    .filter(|q| q.connector == p.connector && q.chain_index < p.chain_index)
                    .map(|q| q.width())
                    .sum();
                AtlasRect {
                    x,
                    y: row * row_height,
                    w: p.width(),
                    h: p.height(),
                }
            })
            .collect();
        Ok(Atlas {
            width,
            height: row_height * connectors.len() as u32,
            rects,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::presets;

    #[test]
    fn two_panels_on_one_chain_sit_side_by_side_in_chain_order() {
        let layout = presets::load("two_64x32").unwrap();
        let atlas = Atlas::build(&layout).unwrap();
        assert_eq!((atlas.width, atlas.height), (128, 32), "atlas size");
        // Panel 0 is the right panel at chain index 0, panel 1 the left at 1.
        assert_eq!(
            atlas.rects[0],
            AtlasRect {
                x: 0,
                y: 0,
                w: 64,
                h: 32
            },
            "chain 0 rect"
        );
        assert_eq!(
            atlas.rects[1],
            AtlasRect {
                x: 64,
                y: 0,
                w: 64,
                h: 32
            },
            "chain 1 rect"
        );
    }

    #[test]
    fn six_panels_on_two_connectors_stack_connectors_as_rows() {
        let layout = presets::load("six_panel").unwrap();
        let atlas = Atlas::build(&layout).unwrap();
        assert_eq!((atlas.width, atlas.height), (96, 32), "atlas size");
        let expect = |i: usize, x, y| {
            assert_eq!(
                atlas.rects[i],
                AtlasRect { x, y, w: 32, h: 16 },
                "rect for {}",
                layout.panels[i].name
            );
        };
        expect(0, 0, 0);
        expect(1, 32, 0);
        expect(2, 64, 0);
        expect(3, 0, 16);
        expect(4, 32, 16);
        expect(5, 64, 16);
    }

    #[test]
    fn packing_an_invalid_layout_fails_instead_of_producing_a_wrong_atlas() {
        let mut layout = presets::load("two_64x32").unwrap();
        layout.panels.clear();
        assert_eq!(
            Atlas::build(&layout),
            Err(LayoutError::NoPanels),
            "empty layout must not pack"
        );
    }
}
