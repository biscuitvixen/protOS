// The spinning cube display piece. Group 0 is the face uniform block
// (time, brightness); group 1 carries this panel's matrices. Lit by a
// fixed directional light in world space.

struct CubeUniforms {
    mvp: mat4x4<f32>,
    model: mat4x4<f32>,
    light: vec4<f32>,   // direction in world space, unused
};
@group(1) @binding(0) var<uniform> C: CubeUniforms;

struct CubeVsIn {
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) colour: vec3<f32>,
};

struct CubeVsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) colour: vec3<f32>,
    @location(1) normal: vec3<f32>,
};

@vertex
fn vs_cube(in: CubeVsIn) -> CubeVsOut {
    var o: CubeVsOut;
    o.pos = C.mvp * vec4<f32>(in.pos, 1.0);
    o.normal = (C.model * vec4<f32>(in.normal, 0.0)).xyz;
    o.colour = in.colour;
    return o;
}

@fragment
fn fs_cube(in: CubeVsOut) -> @location(0) vec4<f32> {
    let n = normalize(in.normal);
    let l = normalize(C.light.xyz);
    let shade = 0.25 + 0.75 * max(dot(n, l), 0.0);
    return vec4<f32>(in.colour * shade * U.g.motion.w, 1.0);
}
