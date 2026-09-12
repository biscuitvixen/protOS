//! Per-panel instance data for the shared vertex stage.
//!
//! One instance per panel carries its atlas rect and the affine from
//! its electrical pixels into face-space, straight from
//! `Panel::transform`. The struct is the byte layout the vertex buffer
//! uses, so it is plain-old-data with no padding.

use bytemuck::{Pod, Zeroable};

use crate::layout::atlas::Atlas;
use crate::layout::{Layout, Side};

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct PanelInstance {
    /// x, y, w, h in atlas pixels.
    pub atlas_rect: [f32; 4],
    /// Columns of M: col_u then col_v, mm per pixel step.
    pub m: [f32; 4],
    /// Origin x, y in mm; pitch in mm/px; unused.
    pub origin: [f32; 4],
    /// Side (0 left, 1 right) and the feature mask (`Panel::feature_mask`).
    pub side_flags: [u32; 2],
}

pub const INSTANCE_LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
    array_stride: std::mem::size_of::<PanelInstance>() as u64,
    step_mode: wgpu::VertexStepMode::Instance,
    attributes: &[
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x4,
            offset: 0,
            shader_location: 0,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x4,
            offset: 16,
            shader_location: 1,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x4,
            offset: 32,
            shader_location: 2,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Uint32x2,
            offset: 48,
            shader_location: 3,
        },
    ],
};

/// One instance per panel, in the layout's panel order (the same order
/// as the atlas rects).
pub fn instances(layout: &Layout, atlas: &Atlas) -> Vec<PanelInstance> {
    layout
        .panels
        .iter()
        .zip(&atlas.rects)
        .map(|(panel, rect)| {
            let t = panel.transform();
            PanelInstance {
                atlas_rect: [rect.x as f32, rect.y as f32, rect.w as f32, rect.h as f32],
                m: [t.col_u[0], t.col_u[1], t.col_v[0], t.col_v[1]],
                origin: [t.origin[0], t.origin[1], t.px_mm, 0.0],
                side_flags: [
                    match panel.side {
                        Side::Left => 0,
                        Side::Right => 1,
                    },
                    panel.feature_mask(),
                ],
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::presets;

    #[test]
    fn the_instance_layout_matches_the_struct_size() {
        assert_eq!(
            std::mem::size_of::<PanelInstance>(),
            56,
            "instance struct size"
        );
        assert_eq!(INSTANCE_LAYOUT.array_stride, 56, "vertex stride");
    }

    #[test]
    fn instances_carry_the_transform_and_the_atlas_rect() {
        let layout = presets::load("two_64x32").unwrap();
        let atlas = Atlas::build(&layout).unwrap();
        let inst = instances(&layout, &atlas);
        assert_eq!(inst.len(), 2, "one instance per panel");
        // Panel 0 is the right panel: chain slot 0, M = 3 I, side 1.
        assert_eq!(
            inst[0].atlas_rect,
            [0.0, 0.0, 64.0, 32.0],
            "right atlas rect"
        );
        assert_eq!(inst[0].m, [3.0, 0.0, 0.0, 3.0], "right M columns");
        assert_eq!(
            inst[0].origin,
            [0.0, -48.0, 3.0, 0.0],
            "right origin and pitch"
        );
        assert_eq!(inst[0].side_flags, [1, 7], "right side flag, every feature");
        assert_eq!(
            inst[1].atlas_rect,
            [64.0, 0.0, 64.0, 32.0],
            "left atlas rect"
        );
        assert_eq!(inst[1].side_flags, [0, 7], "left side flag, every feature");
    }

    #[test]
    fn six_panel_instances_carry_one_feature_each_in_chain_order() {
        let layout = presets::load("six_panel").unwrap();
        let atlas = Atlas::build(&layout).unwrap();
        let masks: Vec<u32> = instances(&layout, &atlas)
            .iter()
            .map(|i| i.side_flags[1])
            .collect();
        assert_eq!(masks, [1, 2, 4, 1, 2, 4], "eye, mouth, nose per connector");
    }
}
