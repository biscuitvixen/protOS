// Shared vertex stage for every 2D pass: one full-panel quad per
// instance. The instance carries the panel's atlas rect and its affine
// into face-space, so the fragment stage receives face-space mm and
// never knows which panel or side it is drawing.

struct Globals {
    atlas: vec4<f32>,   // atlas width, height, 1/width, 1/height (px)
};
@group(0) @binding(0) var<uniform> G: Globals;

struct PanelInstance {
    @location(0) atlas_rect: vec4<f32>,  // x, y, w, h in atlas px
    @location(1) m: vec4<f32>,           // M columns: (col_u, col_v) in mm/px
    @location(2) origin: vec4<f32>,      // O.x, O.y in mm, px_mm, unused
    @location(3) side_flags: vec2<u32>,  // side (0 left, 1 right), flags
};

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) face_mm: vec2<f32>,
    @location(1) @interpolate(flat) px_mm: f32,
    @location(2) @interpolate(flat) side: u32,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, inst: PanelInstance) -> VsOut {
    // Triangle-strip corners (0,0) (1,0) (0,1) (1,1) of the unit quad.
    let c = vec2<f32>(f32(vi & 1u), f32(vi >> 1u));
    let a = inst.atlas_rect.xy + c * inst.atlas_rect.zw;
    var o: VsOut;
    o.pos = vec4<f32>(2.0 * a.x * G.atlas.z - 1.0, 1.0 - 2.0 * a.y * G.atlas.w, 0.0, 1.0);
    // Electrical (u, v) at this corner; interpolation to the fragment's
    // pixel centre supplies the +0.5 without per-fragment matrix work.
    let e = c * inst.atlas_rect.zw;
    o.face_mm = inst.origin.xy + inst.m.xy * e.x + inst.m.zw * e.y;
    o.px_mm = inst.origin.z;
    o.side = inst.side_flags.x;
    return o;
}
