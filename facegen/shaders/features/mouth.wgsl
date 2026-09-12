// Mouth: the filled region between an upper and a lower lip curve,
// running from the inner point outward along the jaw. Both curves
// share a parabolic corner lift symmetric about the centre line, so a
// smile curls at the back of the snout. Opening pushes the lips apart
// by a gap that tapers toward the corner, so the open mouth reads as a
// wedge widest at the snout tip; the lower lip carries a triangle wave
// for teeth. Closed, it collapses to a band of the authored thickness.

fn tri_wave(u: f32) -> f32 {
    return 1.0 - 2.0 * abs(fract(u) - 0.5);
}

fn mouth_sdf(p: vec2<f32>, m: Mouth) -> f32 {
    let x_in = m.c_w.x;
    let cy = m.c_w.y;
    let x_end = m.c_w.x + m.c_w.z;
    let half = m.c_w.w;
    let t = clamp(p.x / max(x_end, 1.0), 0.0, 1.0);
    let y = p.y - m.curve.x * t * t;
    let gap = m.curve.z * (1.0 - m.teeth.z * t);
    let upper = cy + half * m.lips.z + 0.35 * gap + m.lips.x;
    var lower = cy - half * m.lips.w - 0.65 * gap + m.lips.y;
    if (m.teeth.x > 0.0) {
        // Jaw shear slides the tooth pattern sideways with the lower lip.
        lower = lower - m.teeth.x * tri_wave((p.x - m.curve.w) / max(m.teeth.y, 0.5));
    }
    let dy = max(lower - y, y - upper);
    let dx = max(x_in - p.x, p.x - x_end);
    return max(dx, dy);
}
