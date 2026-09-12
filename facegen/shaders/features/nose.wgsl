// Nose: a small bent ellipse near the snout tip. A sneer lifts it and
// tips it outward a little.

fn nose_sdf(p: vec2<f32>, n: Nose) -> f32 {
    let lift = n.shape.z;
    let c = n.c_r.xy + vec2<f32>(0.0, 0.5 * lift);
    return sd_bent_ellipse(p, c, n.c_r.zw, n.shape.x + 0.02 * lift, n.shape.y);
}
