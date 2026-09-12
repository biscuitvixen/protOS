//! Shader source assembly. WGSL has no include, so a pass is the
//! concatenation of the uniform block, the shared vertex stage, any
//! libraries and the pass's own fragment file, in that order. The
//! sources are embedded for now; hot reload later swaps the embedded
//! text for file reads without changing the assembly.

pub const UNIFORMS: &str = include_str!("../../shaders/lib/uniforms.wgsl");
pub const PANEL_QUAD: &str = include_str!("../../shaders/panel_quad.wgsl");
pub const SDF2D: &str = include_str!("../../shaders/lib/sdf2d.wgsl");
pub const EYE: &str = include_str!("../../shaders/features/eye.wgsl");
pub const MOUTH: &str = include_str!("../../shaders/features/mouth.wgsl");
pub const NOSE: &str = include_str!("../../shaders/features/nose.wgsl");
pub const FACE: &str = include_str!("../../shaders/face.wgsl");
pub const TEST_PATTERN: &str = include_str!("../../shaders/test_pattern.wgsl");

pub fn face_source() -> String {
    [UNIFORMS, PANEL_QUAD, SDF2D, EYE, MOUTH, NOSE, FACE].join("\n")
}

pub fn test_pattern_source() -> String {
    [UNIFORMS, PANEL_QUAD, SDF2D, TEST_PATTERN].join("\n")
}
