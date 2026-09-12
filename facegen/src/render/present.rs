//! The present pass: draws the atlas texture as the visor view onto any
//! colour target, a browser canvas or a readable texture. It is the GPU
//! counterpart of `preview::compose` and shares its placement maths.

use bytemuck::{Pod, Zeroable};

use crate::layout::Layout;
use crate::layout::atlas::Atlas;
use crate::preview::visor_bounds;

pub const MAX_PANELS: usize = 8;

/// Padding around the visor in screen pixels.
const PAD: f32 = 12.0;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
struct PanelView {
    inv: [f32; 4],
    origin: [f32; 4],
    rect: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
struct PresentUniforms {
    view: [f32; 4],
    flags: [f32; 4],
    panels: [PanelView; MAX_PANELS],
}

#[derive(Clone, Copy, Debug)]
pub struct PresentOptions {
    /// Pixels per millimetre; 0 fits the visor to the target.
    pub px_per_mm: f32,
    pub apply_gamma: bool,
    pub led_mask: bool,
    /// True when the target format is not an sRGB one, so the shader
    /// encodes the output itself.
    pub srgb_encode: bool,
}

pub struct PresentPass {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    uniforms: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

impl PresentPass {
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        atlas: &wgpu::TextureView,
    ) -> anyhow::Result<Self> {
        #[cfg(not(target_arch = "wasm32"))]
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("present"),
            source: wgpu::ShaderSource::Wgsl(super::shader::PRESENT.embedded.into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("present"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("present uniforms"),
            size: std::mem::size_of::<PresentUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = Self::bind(device, &layout, &uniforms, atlas);
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("present"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("present"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_present"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_present"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(error) = pollster::block_on(scope.pop()) {
            anyhow::bail!("present shader or pipeline rejected: {error}");
        }
        Ok(Self {
            pipeline,
            layout,
            uniforms,
            bind_group,
        })
    }

    fn bind(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        uniforms: &wgpu::Buffer,
        atlas: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("present"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniforms.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(atlas),
                },
            ],
        })
    }

    /// Point the pass at a new atlas texture after a layout change.
    pub fn rebind(&mut self, device: &wgpu::Device, atlas: &wgpu::TextureView) {
        self.bind_group = Self::bind(device, &self.layout, &self.uniforms, atlas);
    }

    /// Draw the visor view of `layout` into `target` of `size` pixels.
    // Queue, encoder, target, size, layout, atlas, options: each is a
    // distinct thing the caller has in hand; bundling them would only
    // move the list into a struct literal.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        size: (u32, u32),
        layout: &Layout,
        atlas: &Atlas,
        opts: &PresentOptions,
    ) {
        let (min, max) = visor_bounds(layout);
        let extent = [max[0] - min[0], max[1] - min[1]];
        let (w, h) = (size.0 as f32, size.1 as f32);
        let k = if opts.px_per_mm > 0.0 {
            opts.px_per_mm
        } else {
            ((w - 2.0 * PAD) / extent[0])
                .min((h - 2.0 * PAD) / extent[1])
                .max(0.01)
        };
        let x0 = PAD - k * min[0] + ((w - 2.0 * PAD) - k * extent[0]) * 0.5;
        let y0 = PAD - k * min[1] + ((h - 2.0 * PAD) - k * extent[1]) * 0.5;
        let mut u = PresentUniforms {
            view: [x0, y0, k, layout.panels.len().min(MAX_PANELS) as f32],
            flags: [
                f32::from(layout.driver.n_planes),
                if opts.apply_gamma { 1.0 } else { 0.0 },
                if opts.led_mask { 1.0 } else { 0.0 },
                if opts.srgb_encode { 1.0 } else { 0.0 },
            ],
            panels: Default::default(),
        };
        for ((panel, rect), slot) in layout
            .panels
            .iter()
            .zip(&atlas.rects)
            .zip(u.panels.iter_mut())
        {
            let t = panel.transform();
            *slot = PanelView {
                inv: t.inverse(),
                origin: [t.origin[0], t.origin[1], panel.side.sigma(), 0.0],
                rect: [rect.x as f32, rect.y as f32, rect.w as f32, rect.h as f32],
            };
        }
        queue.write_buffer(&self.uniforms, 0, bytemuck::bytes_of(&u));
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("present"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
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
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}
