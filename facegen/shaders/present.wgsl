// Present the atlas as the visor is seen from the front: one disc per
// LED, panels placed and rotated as mounted, the wearer's left side on
// the viewer's right, with the panel's gamma curve applied so a monitor
// shows the light the LEDs would emit. Every screen pixel asks each
// panel whether it covers it; up to eight panels.

struct PanelView {
    inv: vec4<f32>,     // inverse of M: u = a du + b dv, v = c du + d dv
    origin: vec4<f32>,  // O.x, O.y in mm, sigma (+1 left side, -1 right), unused
    rect: vec4<f32>,    // atlas x, y, w, h
};

struct PresentUniforms {
    view: vec4<f32>,    // screen x, y offset (px), px per mm, panel count
    flags: vec4<f32>,   // n_planes, apply panel gamma, led mask, encode output as sRGB
    panels: array<PanelView, 8>,
};

@group(0) @binding(0) var<uniform> P: PresentUniforms;
@group(0) @binding(1) var atlas: texture_2d<f32>;

fn srgb_encode(c: f32) -> f32 {
    return select(1.055 * pow(c, 1.0 / 2.4) - 0.055, 12.92 * c, c <= 0.0031308);
}

// Piomatter's 2.2 LUT on the stored byte, keeping the top n_planes bits.
fn panel_light(linear: f32, n_planes: f32) -> f32 {
    let byte = round(clamp(srgb_encode(linear), 0.0, 1.0) * 255.0);
    let lut = max(byte, round(1023.0 * pow(byte / 255.0, 2.2)));
    let step = pow(2.0, 10.0 - n_planes);
    return floor(lut / step) * step / 1023.0;
}

@vertex
fn vs_present(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4<f32> {
    // One triangle covering the screen: (-1,-1) (3,-1) (-1,3).
    let x = f32(i32(vi & 1u) * 4 - 1);
    let y = f32(i32(vi >> 1u) * 4 - 1);
    return vec4<f32>(x, y, 0.0, 1.0);
}

@fragment
fn fs_present(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let k = P.view.z;
    let vx = (pos.x - P.view.x) / k;
    let vy = (pos.y - P.view.y) / k;
    var col = vec3<f32>(0.003, 0.0035, 0.0045);
    let count = u32(P.view.w);
    for (var i = 0u; i < count; i = i + 1u) {
        let pv = P.panels[i];
        let du = pv.origin.z * vx - pv.origin.x;
        let dv = -vy - pv.origin.y;
        let u = pv.inv.x * du + pv.inv.y * dv;
        let v = pv.inv.z * du + pv.inv.w * dv;
        if (u >= 0.0 && v >= 0.0 && u < pv.rect.z && v < pv.rect.w) {
            let texel = textureLoad(atlas, vec2<i32>(i32(pv.rect.x + floor(u)), i32(pv.rect.y + floor(v))), 0).rgb;
            var c = texel;
            if (P.flags.y > 0.5) {
                c = vec3<f32>(panel_light(c.r, P.flags.x), panel_light(c.g, P.flags.x), panel_light(c.b, P.flags.x));
            }
            var cov = 1.0;
            if (P.flags.z > 0.5) {
                let d = length(fract(vec2<f32>(u, v)) - 0.5);
                cov = 1.0 - smoothstep(0.40, 0.46, d);
            }
            col = mix(col, c, cov);
        }
    }
    if (P.flags.w > 0.5) {
        col = vec3<f32>(srgb_encode(col.r), srgb_encode(col.g), srgb_encode(col.b));
    }
    return vec4<f32>(col, 1.0);
}
