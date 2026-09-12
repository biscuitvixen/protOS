//! PNG output for bring-up and golden-image tests.

use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};

use anyhow::Context;

use super::{Frame, FrameSink};

/// Writes every submitted frame to the same path, overwriting.
pub struct PngSink {
    pub path: PathBuf,
}

impl FrameSink for PngSink {
    fn submit(&mut self, frame: &Frame) -> anyhow::Result<()> {
        write_png(&self.path, frame)
    }

    fn name(&self) -> &'static str {
        "png"
    }
}

pub fn write_png(path: &Path, frame: &Frame) -> anyhow::Result<()> {
    let file = File::create(path).with_context(|| format!("creating {}", path.display()))?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), frame.width, frame.height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(&frame.to_rgb())?;
    writer.finish()?;
    Ok(())
}

/// Read a PNG written by [`write_png`] back into RGB bytes.
pub fn read_png_rgb(path: &Path) -> anyhow::Result<(u32, u32, Vec<u8>)> {
    let file = File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let mut reader = png::Decoder::new(BufReader::new(file)).read_info()?;
    let mut buf = vec![0; reader.output_buffer_size().context("png too large")?];
    let info = reader.next_frame(&mut buf)?;
    anyhow::ensure!(
        info.color_type == png::ColorType::Rgb && info.bit_depth == png::BitDepth::Eight,
        "{} is not 8-bit RGB",
        path.display()
    );
    buf.truncate(info.buffer_size());
    Ok((info.width, info.height, buf))
}
