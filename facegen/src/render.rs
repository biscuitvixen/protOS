//! GPU rendering of the panel atlas.
//!
//! A `Renderer` owns the device, the atlas target sized from a layout,
//! the per-panel instance buffer and one 2D pass. `render` draws a
//! frame and reads it back synchronously: at these sizes the GPU round
//! trip is the fixed cost, and overlapping it with the next frame is a
//! later optimisation to be measured on the Pi, not assumed.

pub mod gpu;
pub mod panels;
pub mod pipeline;
pub mod target;

use wgpu::util::DeviceExt;

use crate::layout::Layout;
use crate::layout::atlas::Atlas;
use crate::sinks::Frame;
use gpu::Gpu;
use pipeline::{Globals, PanelPipeline};
use target::RenderTarget;

pub const TEST_PATTERN_WGSL: &str = include_str!("../shaders/test_pattern.wgsl");

pub struct Renderer {
    gpu: Gpu,
    atlas: Atlas,
    target: RenderTarget,
    pipeline: PanelPipeline,
    instances: wgpu::Buffer,
    instance_count: u32,
    seq: u64,
}

impl Renderer {
    /// Build everything for `layout`, drawing with `fragment_wgsl`.
    pub fn new(gpu: Gpu, layout: &Layout, fragment_wgsl: &str) -> anyhow::Result<Self> {
        let atlas = Atlas::build(layout)?;
        let target = RenderTarget::new(&gpu.device, atlas.width, atlas.height);
        let pipeline = PanelPipeline::new(&gpu.device, target::FORMAT, fragment_wgsl)?;
        pipeline.set_globals(
            &gpu.queue,
            &Globals {
                atlas: [
                    atlas.width as f32,
                    atlas.height as f32,
                    1.0 / atlas.width as f32,
                    1.0 / atlas.height as f32,
                ],
            },
        );
        let instance_data = panels::instances(layout, &atlas);
        let instances = gpu
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("panel instances"),
                contents: bytemuck::cast_slice(&instance_data),
                usage: wgpu::BufferUsages::VERTEX,
            });
        Ok(Self {
            gpu,
            atlas,
            target,
            pipeline,
            instances,
            instance_count: instance_data.len() as u32,
            seq: 0,
        })
    }

    pub fn atlas(&self) -> &Atlas {
        &self.atlas
    }

    pub fn gpu(&self) -> &Gpu {
        &self.gpu
    }

    /// Give the device back so another layout can be built on it.
    pub fn into_gpu(self) -> Gpu {
        self.gpu
    }

    /// Draw one frame into the atlas and read it back into `frame`.
    pub fn render(&mut self, frame: &mut Frame) -> anyhow::Result<()> {
        let mut encoder = self
            .gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("panels"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.target.view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            self.pipeline
                .draw(&mut pass, &self.instances, self.instance_count);
        }
        self.target.copy_to_staging(&mut encoder);
        let submission = self.gpu.queue.submit([encoder.finish()]);
        self.target.read_back(&self.gpu.device, submission, frame)?;
        self.seq += 1;
        frame.seq = self.seq;
        Ok(())
    }
}
