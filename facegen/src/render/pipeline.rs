//! A 2D pass: the shared panel-quad vertex stage plus one fragment
//! shader, drawing every panel instance into the atlas.
//!
//! WGSL has no include, so the vertex stage and the fragment source are
//! concatenated into one module. Shader and pipeline creation run
//! inside a validation error scope so a broken shader surfaces as an
//! error rather than a panic; hot reload later relies on that.

use anyhow::anyhow;
use bytemuck::{Pod, Zeroable};

use super::panels::INSTANCE_LAYOUT;

pub const PANEL_QUAD_WGSL: &str = include_str!("../../shaders/panel_quad.wgsl");

/// Group 0, binding 0 of every 2D pass.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
pub struct Globals {
    /// Atlas width, height, 1/width, 1/height in pixels.
    pub atlas: [f32; 4],
}

pub struct PanelPipeline {
    pipeline: wgpu::RenderPipeline,
    globals: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

impl PanelPipeline {
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        fragment_wgsl: &str,
    ) -> anyhow::Result<Self> {
        let source = format!("{PANEL_QUAD_WGSL}\n{fragment_wgsl}");
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("panel pass"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("globals"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("globals"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals.as_entire_binding(),
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("panel pass"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("panel pass"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(INSTANCE_LAYOUT)],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_main"),
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
        if let Some(error) = pollster::block_on(scope.pop()) {
            return Err(anyhow!("shader or pipeline rejected: {error}"));
        }
        Ok(Self {
            pipeline,
            globals,
            bind_group,
        })
    }

    pub fn set_globals(&self, queue: &wgpu::Queue, globals: &Globals) {
        queue.write_buffer(&self.globals, 0, bytemuck::bytes_of(globals));
    }

    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>, instances: &wgpu::Buffer, count: u32) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_vertex_buffer(0, instances.slice(..));
        pass.draw(0..4, 0..count);
    }
}
