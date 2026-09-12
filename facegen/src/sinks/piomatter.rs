//! HUB75 output through Adafruit Piomatter on a Raspberry Pi 5.
//!
//! The atlas is already the driver's framebuffer: connectors stacked
//! as rows, chains along x, so the sink's map is the plain multilane
//! stacking and every frame is one copy. Piomatter's `show` blocks when
//! two frames are queued, which paces the render loop to the panel
//! refresh if it ever runs ahead.

use piomatter_sys::{Config, Pinout, Piomatter, multilane_map};

use super::{Frame, FrameSink};
use crate::layout::atlas::Atlas;
use crate::layout::{self, Layout};

pub struct PiomatterSink {
    driver: Piomatter,
}

impl PiomatterSink {
    pub fn new(layout: &Layout, atlas: &Atlas) -> anyhow::Result<Self> {
        let cfg = config(layout, atlas);
        let driver = Piomatter::new(&cfg)?;
        tracing::info!(
            width = cfg.width,
            height = cfg.height,
            lanes = cfg.n_lanes,
            "piomatter ready"
        );
        Ok(Self { driver })
    }
}

fn config(layout: &Layout, atlas: &Atlas) -> Config {
    let d = &layout.driver;
    let n_lanes = 2 * layout.connectors().len() as u32;
    Config {
        width: atlas.width,
        height: atlas.height,
        n_addr_lines: u32::from(d.n_addr_lines),
        n_planes: u32::from(d.n_planes),
        n_temporal_planes: u32::from(d.n_temporal_planes),
        n_lanes,
        pinout: match d.pinout {
            layout::Pinout::AdafruitMatrixBonnet => Pinout::AdafruitMatrixBonnet,
            layout::Pinout::AdafruitMatrixBonnetBgr => Pinout::AdafruitMatrixBonnetBgr,
            layout::Pinout::Active3 => Pinout::Active3,
            layout::Pinout::Active3Bgr => Pinout::Active3Bgr,
        },
        map: multilane_map(
            atlas.width,
            atlas.height,
            u32::from(d.n_addr_lines),
            n_lanes,
        ),
    }
}

impl FrameSink for PiomatterSink {
    fn submit(&mut self, frame: &Frame) -> anyhow::Result<()> {
        self.driver.show(&frame.bgra)?;
        Ok(())
    }

    fn relayout(&mut self, layout: &Layout, atlas: &Atlas) -> anyhow::Result<()> {
        // One driver instance may exist per Pi: release before rebuilding.
        let cfg = config(layout, atlas);
        let old = std::mem::replace(&mut self.driver, Piomatter::new(&cfg)?);
        drop(old);
        Ok(())
    }

    fn name(&self) -> &'static str {
        "piomatter"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::presets;

    #[test]
    fn the_two_panel_preset_maps_to_one_bonnet_chain_and_six_panels_to_two_active3_connectors() {
        let two = presets::load("two_64x32").unwrap();
        let cfg = config(&two, &Atlas::build(&two).unwrap());
        assert_eq!(
            (cfg.width, cfg.height, cfg.n_lanes, cfg.n_addr_lines),
            (128, 32, 2, 4),
            "two_64x32 driver geometry"
        );
        assert_eq!(cfg.map.len(), 128 * 32, "map covers the atlas");
        let six = presets::load("six_panel").unwrap();
        let cfg = config(&six, &Atlas::build(&six).unwrap());
        assert_eq!(
            (cfg.width, cfg.height, cfg.n_lanes, cfg.n_addr_lines),
            (96, 32, 4, 3),
            "six_panel driver geometry"
        );
        assert_eq!(
            cfg.pinout,
            Pinout::Active3,
            "six panels need the Active3 pinout"
        );
    }
}
