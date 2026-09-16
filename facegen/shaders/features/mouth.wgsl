// Mouth, in one of two modes on a shared baseline that runs from the
// inner point outward along the jaw with a parabolic corner lift
// symmetric about the centre line, so a smile curls at the back of
// the snout.
//
// Jaw: the filled region between an upper and a lower lip curve.
// Opening pushes the lips apart by a gap that tapers toward the
// corner, so the open mouth reads as a wedge widest at the snout tip.
// Both lips carry a row of triangular teeth pointing outward, the
// upper row half a pitch out of phase with the lower, so a closed
// mouth is an interlocking zigzag band and an open one is a solid
// wedge with sawtooth edges.
//
// Scope: a line of the closed thickness on the baseline, displaced by
// a travelling sine whose amplitude follows the voice spectrum along
// the mouth, low bands at the inner end and high bands at the corner.
// In silence the amplitude crossfades to a small constant so the line
// keeps a slow wobble.

const TAU: f32 = 6.283185307179586;

fn band(k: i32) -> f32 {
    let i = clamp(k, 0, 31);
    return U.g.bands[i / 4][i % 4];
}

// Spectrum envelope at u in [0, 1] along the mouth, linear between
// neighbouring bands.
fn band_envelope(u: f32) -> f32 {
    let x = clamp(u, 0.0, 1.0) * 31.0;
    let k = i32(floor(x));
    return mix(band(k), band(k + 1), x - f32(k));
}

// Distance to the scope line. The curve is y = f(x); the estimate
// |y - f| / sqrt(1 + f'^2) is the distance to the tangent line, exact
// on the curve and off by a curvature term away from it, which a
// one-pixel anti-aliasing band never sees. f' carries the carrier and
// the corner lift but not the envelope: its piecewise-linear slope
// would put a seam in the line width at every band boundary.
fn scope_sdf(p: vec2<f32>, m: Mouth) -> f32 {
    let x_in = m.c_w.x;
    let width = max(m.c_w.z, 1.0);
    let x_end = x_in + width;
    let half = m.c_w.w;
    let t = clamp(p.x / max(x_end, 1.0), 0.0, 1.0);
    let u = clamp((p.x - x_in) / width, 0.0, 1.0);
    let voice = clamp(m.teeth.w, 0.0, 1.0);
    let amp = mix(m.scope.x, m.scope.y * band_envelope(u), voice);
    let arg = m.scope.z * TAU * u + m.scope.w;
    let f = m.c_w.y + m.curve.x * t * t + amp * sin(arg);
    let dfdx = 2.0 * m.curve.x * t / max(x_end, 1.0) + amp * m.scope.z * TAU / width * cos(arg);
    let dy = abs(p.y - f) / sqrt(1.0 + dfdx * dfdx) - half;
    let dx = max(x_in - p.x, p.x - x_end);
    return max(dx, dy);
}

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
    // The mode is uniform per side, so this branch never diverges.
    if (m.curve.y >= 0.5) {
        return scope_sdf(p, m);
    }
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
