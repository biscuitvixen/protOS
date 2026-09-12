// Mouth: the filled region between an upper and a lower lip curve,
// running from the inner point outward along the jaw. Both curves
// share a parabolic corner lift symmetric about the centre line, so a
// smile curls at the back of the snout. Opening pushes the lips apart
// by a gap that tapers toward the corner, so the open mouth reads as a
// wedge widest at the snout tip. Both lips carry a row of triangular
// teeth pointing outward, the upper row half a pitch out of phase with
// the lower, so a closed mouth is an interlocking zigzag band and an
// open one is a solid wedge with sawtooth edges.

// Distance to a row of teeth standing on the line y = base, pointing
// in direction dir (+1 up, -1 down), tooth i centred at x0 + i * pitch.
// Nearest tooth by limited repetition; exact while the base width is
// no wider than the pitch.
fn tooth_row_sdf(p: vec2<f32>, x0: f32, base: f32, dir: f32, m: Mouth) -> f32 {
    let pitch = max(m.tooth_row.y, 0.5);
    let i = clamp(round((p.x - x0) / pitch), 0.0, m.tooth_row.z - 1.0);
    let q = vec2<f32>(p.x - x0 - i * pitch, dir * (base - p.y) + m.teeth.x);
    return sd_isosceles(q, vec2<f32>(0.5 * m.teeth.y, m.teeth.x));
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
    let lower = cy - half * m.lips.w - 0.65 * gap + m.lips.y;
    let dy = max(lower - y, y - upper);
    let dx = max(x_in - p.x, p.x - x_end);
    var d = max(dx, dy);
    if (m.tooth_row.z >= 1.0 && m.teeth.x > 0.0) {
        let q = vec2<f32>(p.x, y);
        // The lower row shears sideways with the jaw; the upper row
        // stays with the snout.
        let x_lower = m.tooth_row.x + m.curve.w;
        let x_upper = m.tooth_row.x + 0.5 * m.tooth_row.y;
        var teeth = min(
            tooth_row_sdf(q, x_upper, upper, 1.0, m),
            tooth_row_sdf(q, x_lower, lower, -1.0, m),
        );
        d = min(d, max(teeth, dx));
    }
    return d;
}
