//! The spinning cube: a 3D scene where every panel is a window.
//!
//! Each side of the visor is its own world in face-space millimetres
//! with z toward the viewer. The cube sits at the centre of the side's
//! panels and the eye is a fixed distance in front of it. A panel's
//! projection maps the world onto the z = 0 plane from that eye, then
//! through the panel's inverse affine onto its electrical pixels, so a
//! cube spanning several panels lines up across the bezels exactly as
//! it would through real windows. Mirrored panels flip the winding, so
//! nothing is culled; the depth buffer sorts the faces.

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};

use super::uniforms::UniformBinding;
use crate::layout::atlas::AtlasRect;
use crate::layout::{Layout, PanelTransform, Side};

/// Eye distance in front of the face plane, mm.
const EYE_DISTANCE: f32 = 400.0;
/// Depth range in front of the eye, mm.
const NEAR: f32 = 100.0;
const FAR: f32 = 1000.0;
/// Uniform slot stride: the downlevel dynamic-offset alignment.
const SLOT: u64 = 256;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
struct CubeUniforms {
    mvp: [[f32; 4]; 4],
    model: [[f32; 4]; 4],
    light: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct Vertex {
    pos: [f32; 3],
    normal: [f32; 3],
    colour: [f32; 3],
}

/// One panel as a window: where it draws and how it maps to the world.
#[derive(Clone, Copy, Debug)]
pub struct Window {
    pub rect: AtlasRect,
    pub transform: PanelTransform,
    pub size_px: [u32; 2],
    pub side: Side,
}

pub struct CubePass {
    pipeline: wgpu::RenderPipeline,
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    index_count: u32,
    uniforms: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    depth: wgpu::TextureView,
    windows: Vec<Window>,
    /// Cube centre per side, from that side's panel bounds.
    centres: [Vec3; 2],
}

impl CubePass {
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        atlas: (u32, u32),
        shared: &UniformBinding,
        source: &str,
        windows: Vec<Window>,
    ) -> anyhow::Result<Self> {
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("cube"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        let layout1 = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("cube uniforms"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: wgpu::BufferSize::new(
                        std::mem::size_of::<CubeUniforms>() as u64
                    ),
                },
                count: None,
            }],
        });
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cube uniforms"),
            size: SLOT * windows.len().max(1) as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cube uniforms"),
            layout: &layout1,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &uniforms,
                    offset: 0,
                    size: wgpu::BufferSize::new(std::mem::size_of::<CubeUniforms>() as u64),
                }),
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("cube"),
            bind_group_layouts: &[Some(&shared.layout), Some(&layout1)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("cube"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_cube"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x3],
                })],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth24Plus,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_cube"),
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
            anyhow::bail!("cube shader or pipeline rejected: {error}");
        }
        use wgpu::util::DeviceExt;
        let (vertex_data, index_data) = cube_mesh();
        let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cube vertices"),
            contents: bytemuck::cast_slice(&vertex_data),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let indices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cube indices"),
            contents: bytemuck::cast_slice(&index_data),
            usage: wgpu::BufferUsages::INDEX,
        });
        let depth = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("depth"),
                size: wgpu::Extent3d {
                    width: atlas.0,
                    height: atlas.1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Depth24Plus,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            })
            .create_view(&Default::default());
        let centres = side_centres(&windows);
        Ok(Self {
            pipeline,
            vertices,
            indices,
            index_count: index_data.len() as u32,
            uniforms,
            bind_group,
            depth,
            windows,
            centres,
        })
    }

    /// The windows this pass was built for, from a layout and its atlas.
    pub fn windows(layout: &Layout, rects: &[AtlasRect]) -> Vec<Window> {
        layout
            .panels
            .iter()
            .zip(rects)
            .map(|(p, r)| Window {
                rect: *r,
                transform: p.transform(),
                size_px: p.size_px,
                side: p.side,
            })
            .collect()
    }

    /// Upload this frame's matrices and draw every panel.
    pub fn draw(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        shared: &UniformBinding,
        time_s: f32,
    ) {
        let mut slots = vec![0u8; SLOT as usize * self.windows.len()];
        let spin = Mat4::from_rotation_y(0.9 * time_s) * Mat4::from_rotation_x(0.6 * time_s);
        for (i, w) in self.windows.iter().enumerate() {
            let centre = self.centres[match w.side {
                Side::Left => 0,
                Side::Right => 1,
            }];
            let model = Mat4::from_translation(centre)
                * spin
                * Mat4::from_scale(Vec3::splat(cube_half_size(w)));
            let eye = centre + Vec3::Z * EYE_DISTANCE;
            let mvp = window_projection(&w.transform, w.size_px, eye) * model;
            let u = CubeUniforms {
                mvp: mvp.to_cols_array_2d(),
                model: model.to_cols_array_2d(),
                light: [-0.4, 0.8, 1.0, 0.0],
            };
            let start = i * SLOT as usize;
            slots[start..start + std::mem::size_of::<CubeUniforms>()]
                .copy_from_slice(bytemuck::bytes_of(&u));
        }
        queue.write_buffer(&self.uniforms, 0, &slots);
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("cube"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Discard,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &shared.bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertices.slice(..));
        pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint16);
        for (i, w) in self.windows.iter().enumerate() {
            let r = w.rect;
            pass.set_viewport(r.x as f32, r.y as f32, r.w as f32, r.h as f32, 0.0, 1.0);
            pass.set_scissor_rect(r.x, r.y, r.w, r.h);
            pass.set_bind_group(1, &self.bind_group, &[(i as u64 * SLOT) as u32]);
            pass.draw_indexed(0..self.index_count, 0, 0..1);
        }
    }
}

/// Cube half-size: a third of the side's smaller extent, so the cube
/// fits with room to spin.
fn cube_half_size(w: &Window) -> f32 {
    let (min, max) = w.transform.bounds_mm(w.size_px);
    0.33 * (max[0] - min[0]).min(max[1] - min[1]).max(1.0)
}

/// Centre of each side's panel bounds, z = 0.
fn side_centres(windows: &[Window]) -> [Vec3; 2] {
    let mut out = [Vec3::ZERO; 2];
    for (i, side) in [Side::Left, Side::Right].into_iter().enumerate() {
        let mut min = [f32::INFINITY; 2];
        let mut max = [f32::NEG_INFINITY; 2];
        for w in windows.iter().filter(|w| w.side == side) {
            let (lo, hi) = w.transform.bounds_mm(w.size_px);
            min = [min[0].min(lo[0]), min[1].min(lo[1])];
            max = [max[0].max(hi[0]), max[1].max(hi[1])];
        }
        if min[0].is_finite() {
            out[i] = Vec3::new((min[0] + max[0]) * 0.5, (min[1] + max[1]) * 0.5, 0.0);
        }
    }
    out
}

/// World (face-space mm, z toward the viewer) to clip space for one
/// panel: project onto z = 0 from `eye`, map the plane through the
/// panel's inverse affine to electrical pixels, then to NDC with y up.
/// Depth is linear in the distance from the eye plane.
pub fn window_projection(t: &PanelTransform, size_px: [u32; 2], eye: Vec3) -> Mat4 {
    let a_depth = FAR / (FAR - NEAR);
    let b_depth = -NEAR * FAR / (FAR - NEAR);
    // Rows act on [x, y, z, 1]: x' = ez x - ex z, y' = ez y - ey z,
    // z_clip = -A z + (A ez + B), w = ez - z.
    let project = Mat4::from_cols_array_2d(&[
        [eye.z, 0.0, -eye.x, 0.0],
        [0.0, eye.z, -eye.y, 0.0],
        [0.0, 0.0, -a_depth, a_depth * eye.z + b_depth],
        [0.0, 0.0, -1.0, eye.z],
    ])
    .transpose();
    let (cu, cv, o) = (t.col_u, t.col_v, t.origin);
    let det = cu[0] * cv[1] - cv[0] * cu[1];
    let (a, b, c, d) = (cv[1] / det, -cv[0] / det, -cu[1] / det, cu[0] / det);
    let (w, h) = (size_px[0] as f32, size_px[1] as f32);
    let su = 2.0 / w;
    let sv = 2.0 / h;
    // Rows act on [x', y', z_clip, w]: u_h = a x' + b y' - (a Ox + b Oy) w,
    // ndc_x = su u_h - w; ndc_y = w - sv v_h.
    let to_ndc = Mat4::from_cols_array_2d(&[
        [su * a, su * b, 0.0, -su * (a * o[0] + b * o[1]) - 1.0],
        [-sv * c, -sv * d, 0.0, sv * (c * o[0] + d * o[1]) + 1.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ])
    .transpose();
    to_ndc * project
}

/// A cube face: outward normal, the two in-plane axes, linear colour.
type Face = ([f32; 3], [f32; 3], [f32; 3], [f32; 3]);

/// 24 vertices (four per face, flat normals) and 36 indices.
fn cube_mesh() -> (Vec<Vertex>, Vec<u16>) {
    let faces: [Face; 6] = [
        (
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.7, 1.0],
        ),
        (
            [0.0, 0.0, -1.0],
            [-1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [1.0, 0.1, 0.5],
        ),
        (
            [1.0, 0.0, 0.0],
            [0.0, 0.0, -1.0],
            [0.0, 1.0, 0.0],
            [1.0, 0.6, 0.0],
        ),
        (
            [-1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.0, 1.0, 0.0],
            [0.2, 1.0, 0.3],
        ),
        (
            [0.0, 1.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 0.0, -1.0],
            [1.0, 0.9, 0.1],
        ),
        (
            [0.0, -1.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.4, 0.2, 1.0],
        ),
    ];
    let mut vertices = Vec::with_capacity(24);
    let mut indices = Vec::with_capacity(36);
    for (n, t, b, colour) in faces {
        let (n, t, b) = (Vec3::from(n), Vec3::from(t), Vec3::from(b));
        let base = vertices.len() as u16;
        for (s, u) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            let pos = n + t * s + b * u;
            vertices.push(Vertex {
                pos: pos.to_array(),
                normal: n.to_array(),
                colour,
            });
        }
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    (vertices, indices)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::presets;
    use glam::Vec4;

    fn ndc(m: Mat4, p: Vec3) -> Vec3 {
        let c = m * Vec4::new(p.x, p.y, p.z, 1.0);
        Vec3::new(c.x / c.w, c.y / c.w, c.z / c.w)
    }

    #[test]
    fn a_point_on_the_face_plane_lands_on_its_own_pixel_for_every_panel_orientation() {
        let layout = presets::load("two_64x32").unwrap();
        for panel in &layout.panels {
            let t = panel.transform();
            let eye = Vec3::new(96.0, 0.0, EYE_DISTANCE);
            let m = window_projection(&t, panel.size_px, eye);
            // Electrical pixel (10, 5) centre, on the plane, must map to its own NDC.
            let face = t.pixel_centre_mm(10, 5);
            let n = ndc(m, Vec3::new(face[0], face[1], 0.0));
            let expect_x = 2.0 * 10.5 / 64.0 - 1.0;
            let expect_y = 1.0 - 2.0 * 5.5 / 32.0;
            assert!(
                (n.x - expect_x).abs() < 1e-4,
                "{}: x {} vs {expect_x}",
                panel.name,
                n.x
            );
            assert!(
                (n.y - expect_y).abs() < 1e-4,
                "{}: y {} vs {expect_y}",
                panel.name,
                n.y
            );
            assert!(
                n.z > 0.0 && n.z < 1.0,
                "{}: depth {} outside [0,1]",
                panel.name,
                n.z
            );
        }
    }

    #[test]
    fn a_point_toward_the_eye_projects_away_from_the_eye_axis() {
        let layout = presets::load("two_64x32").unwrap();
        let t = layout.panels[1].transform();
        let eye = Vec3::new(96.0, 0.0, EYE_DISTANCE);
        let m = window_projection(&t, [64, 32], eye);
        let on_plane = ndc(m, Vec3::new(126.0, 0.0, 0.0));
        let raised = ndc(m, Vec3::new(126.0, 0.0, 100.0));
        assert!(
            raised.x > on_plane.x,
            "a point nearer the eye should spread outward from the axis"
        );
        assert!(
            raised.z < on_plane.z,
            "a nearer point should have smaller depth"
        );
    }

    #[test]
    fn the_mesh_has_six_faces_of_two_triangles() {
        let (v, i) = cube_mesh();
        assert_eq!(v.len(), 24, "vertices");
        assert_eq!(i.len(), 36, "indices");
        assert!(
            i.iter().all(|&k| (k as usize) < v.len()),
            "indices in range"
        );
    }
}
