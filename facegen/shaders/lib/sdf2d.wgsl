// Signed distance primitives in face-space mm: negative inside.
//
// The bent ellipse is the workhorse. Its exact distance needs a cubic
// solve per pixel; the first-order estimate d = f / |grad f| is exact
// on the curve and only degrades away from it, which is all a 1 px
// anti-aliasing band needs (Quilez, "distance to an implicit"). The
// parabolic warp makes the field a bound rather than a true distance;
// keep |bend| below about 0.6 so the edge stays within a pixel.

fn rotate(v: vec2<f32>, a: f32) -> vec2<f32> {
    let c = cos(a);
    let s = sin(a);
    return vec2<f32>(c * v.x - s * v.y, s * v.x + c * v.y);
}

// Ellipse centred on c with radii r, rotated by rot, tips bent up
// (bend > 0) or down (bend < 0) by a parabola across its width.
fn sd_bent_ellipse(p: vec2<f32>, c: vec2<f32>, r: vec2<f32>, rot: f32, bend: f32) -> f32 {
    var q = rotate(p - c, -rot);
    let t = q.x / r.x;
    q.y = q.y - bend * r.y * t * t;
    let k1 = length(q / r);
    let k2 = length(q / (r * r));
    return (k1 * k1 - 1.0) / (2.0 * max(k2, 1e-4));
}

fn sd_segment(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> f32 {
    let pa = p - a;
    let ba = b - a;
    let h = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0);
    return length(pa - ba * h);
}

fn sd_capsule(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>, radius: f32) -> f32 {
    return sd_segment(p, a, b) - radius;
}

// Quadratic smooth union; k is the blend width in mm.
fn smin(a: f32, b: f32, k: f32) -> f32 {
    let kk = max(k, 1e-4) * 4.0;
    let h = max(kk - abs(a - b), 0.0);
    return min(a, b) - h * h * 0.25 / kk;
}

// One-pixel anti-aliased coverage of a distance, given the pixel size
// in the same units as d.
fn cov(d: f32, px: f32) -> f32 {
    return clamp(0.5 - d / px, 0.0, 1.0);
}
