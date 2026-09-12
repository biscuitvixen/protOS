// Bring-up pattern: a side-coloured background, a 10 mm face-space grid,
// a white disc at (15, 0) mm and a yellow disc up and outward at
// (35, 15) mm. A correct render shows the yellow disc above and outward
// of the white one on both sides, which catches mirror and rotation
// mistakes at a glance.

fn disc(p: vec2<f32>, c: vec2<f32>, r: f32, px_mm: f32) -> f32 {
    return cov(length(p - c) - r, px_mm);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let p = in.face_mm;
    let px = in.px_mm;
    var col = select(vec3<f32>(0.60, 0.00, 0.50), vec3<f32>(0.00, 0.45, 0.60), in.side == 0u) * 0.25;
    // Grid lines one pixel wide on every 10 mm multiple.
    let g = abs(p - round(p / 10.0) * 10.0);
    col = mix(col, vec3<f32>(0.15), cov(min(g.x, g.y) - 0.5 * px, px));
    col = mix(col, vec3<f32>(1.0), disc(p, vec2<f32>(15.0, 0.0), 6.0, px));
    col = mix(col, vec3<f32>(1.0, 0.85, 0.0), disc(p, vec2<f32>(35.0, 15.0), 3.0, px));
    return vec4<f32>(col, 1.0);
}
