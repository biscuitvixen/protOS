// The face pass: composites the features for this fragment's side in
// a fixed order, in linear light, one coverage clamp per feature,
// skipping features the panel's mask leaves out (the branch is uniform
// across a panel, so it costs nothing in divergence). The design-box
// scale maps face-space onto the face's authored size, and
// the pixel size is scaled with it so anti-aliasing stays one LED wide.

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let s = U.sides[in.side];
    let scale = max(U.g.face.x, 1e-3);
    let p = (in.face_mm - U.g.motion.xy) / scale;
    let px = in.px_mm / scale;
    var col = U.g.bg.rgb;
    if ((in.features & 1u) != 0u) {
        col = mix(col, s.eye.colour.rgb * s.eye.colour.w, cov(eye_sdf(p, s.eye, U.g.face.y), px));
    }
    if ((in.features & 2u) != 0u) {
        col = mix(col, s.mouth.colour.rgb * s.mouth.colour.w, cov(mouth_sdf(p, s.mouth), px));
    }
    if ((in.features & 4u) != 0u) {
        col = mix(col, s.nose.colour.rgb * s.nose.colour.w, cov(nose_sdf(p, s.nose), px));
    }
    return vec4<f32>(col * U.g.motion.w, 1.0);
}
