// Mouth: a band from the inner point outward along the jaw, its far
// corner lifted by a parabola symmetric about the centre line so a
// smile curls at the back of the snout. Opening splits it into an
// upper and a lower lip band around a gap; the lower band also takes
// the jaw's sideways shear.

fn mouth_band(p: vec2<f32>, x_in: f32, x_end: f32, cy: f32, corner_dy: f32, thickness: f32) -> f32 {
    let t = p.x / max(x_end, 1.0);
    let q = vec2<f32>(p.x, p.y - corner_dy * t * t);
    return sd_capsule(q, vec2<f32>(x_in, cy), vec2<f32>(x_end, cy), thickness);
}

fn mouth_sdf(p: vec2<f32>, m: Mouth) -> f32 {
    let x_in = m.c_w.x;
    let cy = m.c_w.y;
    let x_end = m.c_w.x + m.c_w.z;
    let thickness = m.c_w.w;
    let gap = m.curve.z;
    let upper = mouth_band(p, x_in, x_end, cy + 0.35 * gap + m.lips.x, m.curve.x, thickness * m.lips.z);
    let lower = mouth_band(p - vec2<f32>(m.curve.w, 0.0), x_in, x_end, cy - 0.65 * gap + m.lips.y, m.curve.x, thickness * m.lips.w);
    return min(upper, lower);
}
