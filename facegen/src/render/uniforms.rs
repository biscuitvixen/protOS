//! Rust mirror of `shaders/lib/uniforms.wgsl`.
//!
//! Every field is a `[f32; 4]`, so the structs are plain-old-data with
//! no padding and the byte layout matches WGSL's uniform rules without
//! any alignment bookkeeping. The shader file is the documentation for
//! what each lane means; the size asserts below keep the two in step.

use bytemuck::{Pod, Zeroable};

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct EyeUniform {
    pub c_r: [f32; 4],
    pub shape: [f32; 4],
    pub gaze: [f32; 4],
    pub pupil: [f32; 4],
    pub colour: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct MouthUniform {
    pub c_w: [f32; 4],
    pub curve: [f32; 4],
    pub lips: [f32; 4],
    pub teeth: [f32; 4],
    pub tooth_row: [f32; 4],
    pub tongue: [f32; 4],
    pub colour: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct NoseUniform {
    pub c_r: [f32; 4],
    pub shape: [f32; 4],
    pub colour: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct CheekUniform {
    pub stripes: [f32; 4],
    pub colour: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct SideUniform {
    pub eye: EyeUniform,
    pub mouth: MouthUniform,
    pub nose: NoseUniform,
    pub cheek: CheekUniform,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct GlobalsUniform {
    pub time: [f32; 4],
    pub motion: [f32; 4],
    pub face: [f32; 4],
    pub bg: [f32; 4],
    pub atlas: [f32; 4],
    pub bands: [[f32; 4]; 8],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct FaceUniforms {
    pub g: GlobalsUniform,
    pub sides: [SideUniform; 2],
}

pub const LEFT: usize = 0;
pub const RIGHT: usize = 1;

const _: () = {
    assert!(std::mem::size_of::<EyeUniform>() == 80);
    assert!(std::mem::size_of::<MouthUniform>() == 112);
    assert!(std::mem::size_of::<NoseUniform>() == 48);
    assert!(std::mem::size_of::<CheekUniform>() == 32);
    assert!(std::mem::size_of::<SideUniform>() == 272);
    assert!(std::mem::size_of::<GlobalsUniform>() == 208);
    assert!(std::mem::size_of::<FaceUniforms>() == 752);
    assert!(std::mem::offset_of!(FaceUniforms, sides) == 208);
};

impl FaceUniforms {
    pub const SIZE: u64 = std::mem::size_of::<FaceUniforms>() as u64;
}

/// The uniform buffer and its bind group, bound at group 0 by every
/// pass so the face parameters, time and brightness are shared.
pub struct UniformBinding {
    pub buffer: wgpu::Buffer,
    pub layout: wgpu::BindGroupLayout,
    pub bind_group: wgpu::BindGroup,
}

impl UniformBinding {
    pub fn new(device: &wgpu::Device) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("face uniforms"),
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
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("face uniforms"),
            size: FaceUniforms::SIZE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("face uniforms"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        });
        Self {
            buffer,
            layout,
            bind_group,
        }
    }

    pub fn write(&self, queue: &wgpu::Queue, uniforms: &FaceUniforms) {
        queue.write_buffer(&self.buffer, 0, bytemuck::bytes_of(uniforms));
    }
}
