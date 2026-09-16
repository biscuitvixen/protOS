// The per-frame uniform block shared by every pass. Every field is a
// vec4<f32> so the layout is identical in WGSL and in the Rust mirror
// (render/uniforms.rs) with no padding rules to get wrong. Units are
// millimetres in face-space, radians for angles, [0, 1] for weights.

struct Eye {
    c_r: vec4<f32>,     // cx, cy, rx, ry
    shape: vec4<f32>,   // rotation, bend, open, lower_close
    gaze: vec4<f32>,    // gaze_dx, gaze_dy, lid_tilt, widen
    pupil: vec4<f32>,   // dx, dy, scale, enabled
    colour: vec4<f32>,  // linear rgb, intensity
};

struct Mouth {
    c_w: vec4<f32>,     // inner x, y, width, thickness
    curve: vec4<f32>,   // corner_dy, mode (0 jaw, 1 scope), gap, lower_dx
    lips: vec4<f32>,    // upper_dy, lower_dy, upper_thickness_scale, lower_thickness_scale
    teeth: vec4<f32>,   // tooth height, tooth base width, open taper, voice activity
    tooth_row: vec4<f32>, // first tooth offset from the inner end, pitch, count, spectral centroid
    scope: vec4<f32>,   // idle amplitude, voiced amplitude, carrier cycles, carrier phase
    colour: vec4<f32>,
};

struct Nose {
    c_r: vec4<f32>,     // cx, cy, rx, ry
    shape: vec4<f32>,   // rotation, bend, nostril_lift, bridge
    colour: vec4<f32>,
};

struct Cheek {
    stripes: vec4<f32>, // opacity, scale, dx, dy
    colour: vec4<f32>,
};

struct Side {
    eye: Eye,
    mouth: Mouth,
    nose: Nose,
    cheek: Cheek,
};

struct Globals {
    time: vec4<f32>,    // t, dt, frame, scene
    motion: vec4<f32>,  // drift_dx, drift_dy, breathe, brightness
    face: vec4<f32>,    // face_scale, close_mode (0 squash, 1 cut), unused, unused
    bg: vec4<f32>,      // background linear rgb, unused
    atlas: vec4<f32>,   // atlas width, height, 1/width, 1/height (px)
    bands: array<vec4<f32>, 8>,  // 32 audio bands
};

struct FaceUniforms {
    g: Globals,
    sides: array<Side, 2>,  // 0 = wearer's left, 1 = wearer's right
};

@group(0) @binding(0) var<uniform> U: FaceUniforms;
