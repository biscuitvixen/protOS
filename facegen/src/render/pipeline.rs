//! A 2D pass: the shared panel-quad vertex stage plus one fragment
//! shader, drawing every panel instance into the atlas.
//!
//! The pass takes an already assembled WGSL module (see `shader.rs`)
//! and binds the shared face uniform block at group 0. Shader and
//! pipeline creation run inside a validation error scope so a broken
//! shader surfaces as an error rather than a panic; hot reload relies
//! on that.

use super::panels::INSTANCE_LAYOUT;
use super::uniforms::UniformBinding;

pub struct PanelPipeline {
    pipeline: wgpu::RenderPipeline,
}

impl PanelPipeline {
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        shared: &UniformBinding,
        source: &str,
    ) -> anyhow::Result<Self> {
        #[cfg(not(target_arch = "wasm32"))]
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("panel pass"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("panel pass"),
            bind_group_layouts: &[Some(&shared.layout)],
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
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(error) = pollster::block_on(scope.pop()) {
            anyhow::bail!("shader or pipeline rejected: {error}");
        }
        Ok(Self { pipeline })
    }

    pub fn draw(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        shared: &UniformBinding,
        instances: &wgpu::Buffer,
        count: u32,
    ) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &shared.bind_group, &[]);
        pass.set_vertex_buffer(0, instances.slice(..));
        pass.draw(0..4, 0..count);
    }
}
