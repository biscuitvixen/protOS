//! The atlas render target and its readback path.
//!
//! The atlas is one BGRA texture laid out as the LED driver's
//! framebuffer. Rendering goes into it, a copy moves it into a
//! mappable staging buffer, and the CPU reads that buffer after the
//! submission completes. Copies require rows padded to 256 bytes, so
//! the staging layout may be wider than the image; the padding is
//! stripped on the way out.

use std::sync::mpsc;
use std::time::Duration;

use anyhow::{Context, anyhow};

use crate::sinks::Frame;

/// sRGB-encoded BGRA: the shader works in linear light, the store
/// encodes, and the bytes match Piomatter's rgb888 word layout.
pub const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8UnormSrgb;

/// How long to wait for the GPU before treating a frame as lost.
const READBACK_TIMEOUT: Duration = Duration::from_secs(5);

pub struct RenderTarget {
    pub width: u32,
    pub height: u32,
    texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    staging: wgpu::Buffer,
    padded_bytes_per_row: u32,
}

impl RenderTarget {
    pub fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("atlas"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let padded_bytes_per_row = padded_row(width);
        let staging = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("atlas staging"),
            size: u64::from(padded_bytes_per_row) * u64::from(height),
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            width,
            height,
            texture,
            view,
            staging,
            padded_bytes_per_row,
        }
    }

    /// Record the atlas-to-staging copy after the render pass.
    pub fn copy_to_staging(&self, encoder: &mut wgpu::CommandEncoder) {
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &self.staging,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(self.padded_bytes_per_row),
                    rows_per_image: Some(self.height),
                },
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );
    }

    /// Wait for `submission` and copy the staging buffer into `frame`,
    /// dropping the row padding.
    pub fn read_back(
        &self,
        device: &wgpu::Device,
        submission: wgpu::SubmissionIndex,
        frame: &mut Frame,
    ) -> anyhow::Result<()> {
        let slice = self.staging.slice(..);
        let (tx, rx) = mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(submission),
                timeout: Some(READBACK_TIMEOUT),
            })
            .context("waiting for the GPU")?;
        rx.recv()
            .map_err(|_| anyhow!("map callback never ran"))?
            .context("mapping the staging buffer")?;
        {
            let mapped = slice
                .get_mapped_range()
                .context("reading the mapped staging buffer")?;
            let row_bytes = self.width as usize * Frame::BYTES_PER_PIXEL;
            frame.width = self.width;
            frame.height = self.height;
            frame.bgra.resize(row_bytes * self.height as usize, 0);
            for (dst, src) in frame
                .bgra
                .chunks_exact_mut(row_bytes)
                .zip(mapped.chunks_exact(self.padded_bytes_per_row as usize))
            {
                dst.copy_from_slice(&src[..row_bytes]);
            }
        }
        self.staging.unmap();
        Ok(())
    }
}

/// Bytes per staging row, rounded up to the copy alignment.
pub fn padded_row(width: u32) -> u32 {
    let unpadded = width * Frame::BYTES_PER_PIXEL as u32;
    unpadded.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn staging_rows_round_up_to_the_copy_alignment() {
        assert_eq!(padded_row(64), 256, "64 px is exactly one aligned row");
        assert_eq!(padded_row(96), 512, "96 px needs padding to 512");
        assert_eq!(padded_row(128), 512, "128 px is exactly two aligned rows");
    }
}
