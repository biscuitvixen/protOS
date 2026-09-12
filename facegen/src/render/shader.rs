//! Shader source assembly and validation.
//!
//! WGSL has no include, so a pass is the concatenation of the uniform
//! block, the shared vertex stage, any libraries and the pass's own
//! fragment file, in that order. Every file is embedded so the binary
//! runs with nothing on disk; when an assets directory is present the
//! same files are read from it instead, which is what hot reload edits.
//! Sources are validated with naga before they reach the device, so a
//! broken save produces a message naming the file and line rather than
//! a driver error.

use std::path::Path;

use anyhow::Context;

/// One shader file: its path under `shaders/` and the embedded copy.
#[derive(Clone, Copy, Debug)]
pub struct SourceFile {
    pub path: &'static str,
    pub embedded: &'static str,
}

macro_rules! file {
    ($path:literal) => {
        SourceFile {
            path: $path,
            embedded: include_str!(concat!("../../shaders/", $path)),
        }
    };
}

pub const UNIFORMS: SourceFile = file!("lib/uniforms.wgsl");
pub const PANEL_QUAD: SourceFile = file!("panel_quad.wgsl");
pub const SDF2D: SourceFile = file!("lib/sdf2d.wgsl");
pub const EYE: SourceFile = file!("features/eye.wgsl");
pub const MOUTH: SourceFile = file!("features/mouth.wgsl");
pub const NOSE: SourceFile = file!("features/nose.wgsl");
pub const FACE: SourceFile = file!("face.wgsl");
pub const TEST_PATTERN: SourceFile = file!("test_pattern.wgsl");
pub const CUBE: SourceFile = file!("cube.wgsl");
pub const PRESENT: SourceFile = file!("present.wgsl");

/// The files of the face pass, in concatenation order.
pub const FACE_SET: &[SourceFile] = &[UNIFORMS, PANEL_QUAD, SDF2D, EYE, MOUTH, NOSE, FACE];
pub const TEST_PATTERN_SET: &[SourceFile] = &[UNIFORMS, PANEL_QUAD, SDF2D, TEST_PATTERN];
pub const CUBE_SET: &[SourceFile] = &[UNIFORMS, CUBE];
pub const PRESENT_SET: &[SourceFile] = &[PRESENT];

/// An assembled module plus the table needed to map a line in it back
/// to the file it came from.
pub struct Assembled {
    pub source: String,
    /// (path, first line number of that file in `source`, 1-based).
    starts: Vec<(&'static str, usize)>,
}

impl Assembled {
    /// Concatenate `set`, reading each file from `dir` when given and
    /// from the embedded copy otherwise.
    pub fn load(set: &[SourceFile], dir: Option<&Path>) -> anyhow::Result<Self> {
        let mut source = String::new();
        let mut starts = Vec::with_capacity(set.len());
        let mut line = 1;
        for file in set {
            let text = match dir {
                Some(dir) => {
                    let path = dir.join(file.path);
                    std::fs::read_to_string(&path)
                        .with_context(|| format!("reading {}", path.display()))?
                }
                None => file.embedded.to_string(),
            };
            starts.push((file.path, line));
            line += text.lines().count() + 1;
            source.push_str(&text);
            source.push('\n');
        }
        Ok(Self { source, starts })
    }

    /// Parse and validate with naga. The error names the file and the
    /// line within it.
    pub fn validate(&self) -> Result<(), String> {
        let module = naga::front::wgsl::parse_str(&self.source).map_err(|e| {
            let location = e.location(&self.source).map(|l| l.line_number as usize);
            self.describe(location, e.message())
        })?;
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .map(|_| ())
        .map_err(|e| {
            let location = e.location(&self.source).map(|l| l.line_number as usize);
            self.describe(location, &e.to_string())
        })
    }

    /// "file:line: message" for a line in the concatenated source.
    fn describe(&self, line: Option<usize>, message: &str) -> String {
        match line.and_then(|l| self.file_of(l)) {
            Some((path, local)) => format!("{path}:{local}: {message}"),
            None => message.to_string(),
        }
    }

    fn file_of(&self, line: usize) -> Option<(&'static str, usize)> {
        self.starts
            .iter()
            .rev()
            .find(|(_, start)| *start <= line)
            .map(|(path, start)| (*path, line - start + 1))
    }
}

/// The embedded face pass, for tests and the CLI.
pub fn face_source() -> String {
    Assembled::load(FACE_SET, None)
        .expect("embedded sources load")
        .source
}

pub fn cube_source() -> String {
    Assembled::load(CUBE_SET, None)
        .expect("embedded sources load")
        .source
}

pub fn test_pattern_source() -> String {
    Assembled::load(TEST_PATTERN_SET, None)
        .expect("embedded sources load")
        .source
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_embedded_face_and_test_pattern_validate() {
        for set in [FACE_SET, TEST_PATTERN_SET, CUBE_SET, PRESENT_SET] {
            let a = Assembled::load(set, None).unwrap();
            assert_eq!(a.validate(), Ok(()), "embedded shaders must validate");
        }
    }

    #[test]
    fn a_broken_shader_is_reported_against_its_own_file_and_line() {
        let dir = std::env::temp_dir().join(format!("facegen-shader-test-{}", std::process::id()));
        for file in FACE_SET {
            let path = dir.join(file.path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, file.embedded).unwrap();
        }
        // Break line 3 of the nose shader.
        let nose = dir.join(NOSE.path);
        let mut lines: Vec<String> = std::fs::read_to_string(&nose)
            .unwrap()
            .lines()
            .map(String::from)
            .collect();
        lines.insert(2, "let oops = ;".into());
        std::fs::write(&nose, lines.join("\n")).unwrap();
        let a = Assembled::load(FACE_SET, Some(&dir)).unwrap();
        let err = a
            .validate()
            .expect_err("broken shader must fail validation");
        assert!(
            err.starts_with("features/nose.wgsl:3:"),
            "error should name the nose file and line 3, got {err}"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_missing_file_in_the_assets_directory_is_an_error_not_a_silent_fallback() {
        let dir =
            std::env::temp_dir().join(format!("facegen-shader-missing-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(
            Assembled::load(FACE_SET, Some(&dir)).is_err(),
            "an empty assets directory must fail to load"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
