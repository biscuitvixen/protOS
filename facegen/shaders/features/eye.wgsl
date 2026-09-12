// Eye: a bent ellipse closed either by squashing its height with the
// lower edge anchored (the classic protogen blob) or by a tilted lid
// cut from the top, chosen per face. A lower lid rises for squints and
// an optional pupil is subtracted.

fn eye_sdf(p: vec2<f32>, e: Eye, close_mode: f32) -> f32 {
    let open = clamp(e.shape.z, 0.0, 1.0);
    var c = e.c_r.xy + e.gaze.xy;
    var r = e.c_r.zw * vec2<f32>(1.0, max(e.gaze.w, 0.05));
    if (close_mode < 0.5) {
        let ry = max(r.y * open, 0.6);
        c.y = c.y - (r.y - ry);
        r.y = ry;
    }
    var d = sd_bent_ellipse(p, c, r, e.shape.x, e.shape.y);
    if (close_mode >= 0.5) {
        let q = rotate(p - c, -e.shape.x - e.gaze.z);
        d = max(d, q.y - r.y * (2.0 * open - 1.0));
    }
    if (e.shape.w > 0.0) {
        let q = rotate(p - c, -e.shape.x);
        d = max(d, (2.0 * r.y * e.shape.w - r.y) - q.y);
    }
    if (e.pupil.w > 0.5) {
        d = max(d, -sd_bent_ellipse(p, c + e.pupil.xy, r * e.pupil.z, e.shape.x, 0.0));
    }
    return d;
}
