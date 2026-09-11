//! Where finished frames go. The renderer produces one atlas per frame
//! and every sink receives the same bytes: the LED driver, the web
//! harness, a PNG on disk, or nothing.

pub mod png;

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
}

/// Discards frames; useful for benchmarking the render path alone.
#[derive(Default)]
pub struct NullSink;

impl FrameSink for NullSink {
    fn submit(&mut self, _frame: &Frame) -> anyhow::Result<()> {
        Ok(())
    }
}
