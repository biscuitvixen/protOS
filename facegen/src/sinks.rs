//! Where finished frames go. The renderer produces one atlas per frame
//! and every sink receives the same bytes: the LED driver, the web
//! harness, a PNG on disk, or nothing.

#[cfg(feature = "piomatter")]
pub mod piomatter;
pub mod png;

use crate::layout::Layout;
use crate::layout::atlas::Atlas;

/// One rendered atlas. Pixels are BGRA, 8 bits each, row-major with no
/// padding, in sRGB encoding as the render target stores them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Frame {
    pub seq: u64,
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}

impl Frame {
    pub const BYTES_PER_PIXEL: usize = 4;

    pub fn new(width: u32, height: u32) -> Self {
        Self {
            seq: 0,
            width,
            height,
            bgra: vec![0; width as usize * height as usize * Self::BYTES_PER_PIXEL],
        }
    }

    /// RGB triples, for consumers that do not want the alpha byte.
    pub fn to_rgb(&self) -> Vec<u8> {
        self.bgra
            .chunks_exact(Self::BYTES_PER_PIXEL)
            .flat_map(|px| [px[2], px[1], px[0]])
            .collect()
    }
}

pub trait FrameSink {
    fn submit(&mut self, frame: &Frame) -> anyhow::Result<()>;

    /// The atlas changed shape; sinks that depend on it rebuild here.
    fn relayout(&mut self, _layout: &Layout, _atlas: &Atlas) -> anyhow::Result<()> {
        Ok(())
    }

    fn name(&self) -> &'static str;
}

/// Discards frames; useful for benchmarking the render path alone.
#[derive(Default)]
pub struct NullSink;

impl FrameSink for NullSink {
    fn submit(&mut self, _frame: &Frame) -> anyhow::Result<()> {
        Ok(())
    }

    fn name(&self) -> &'static str {
        "null"
    }
}

/// Build a sink by name. `piomatter` needs the cargo feature of the
/// same name and a Raspberry Pi 5.
pub fn by_name(
    name: &str,
    layout: &Layout,
    atlas: &Atlas,
) -> anyhow::Result<Box<dyn FrameSink + Send>> {
    match name {
        "null" => Ok(Box::new(NullSink)),
        #[cfg(feature = "piomatter")]
        "piomatter" => Ok(Box::new(piomatter::PiomatterSink::new(layout, atlas)?)),
        #[cfg(not(feature = "piomatter"))]
        "piomatter" => {
            let _ = (layout, atlas);
            anyhow::bail!(
                "this build has no panel driver; build with --features piomatter on the Pi"
            )
        }
        other => anyhow::bail!("unknown sink {other:?}; known: null, piomatter"),
    }
}
