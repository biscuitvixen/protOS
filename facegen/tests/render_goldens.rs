//! Golden-image tests on lavapipe.
//!
//! Output is byte-stable on one driver, not across drivers, so the
//! goldens are pinned to Mesa's software Vulkan (llvmpipe), which every
//! dev box and the Pi can run. Regenerate with
//! FACEGEN_UPDATE_GOLDENS=1 after an intentional change and commit
//! the PNGs.

use std::path::PathBuf;

use facegen::contract::InputStore;
use facegen::face::{Face, FrameState, fit_scale};
use facegen::layout::{Layout, presets};
use facegen::render::gpu::Gpu;
use facegen::render::uniforms::FaceUniforms;
use facegen::render::{Renderer, shader};
use facegen::rig::Rig;
use facegen::sinks::Frame;
use facegen::sinks::png::{read_png_rgb, write_png};

/// Per-channel tolerance; lavapipe is deterministic, so this only
/// absorbs a future LLVM changing a rounding somewhere.
const TOLERANCE: u8 = 2;

fn golden_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(format!("{name}.png"))
}

fn lavapipe() -> Gpu {
    Gpu::new(Some("llvmpipe"))
        .expect("lavapipe (Mesa software Vulkan) is required for golden tests")
}

fn render(gpu: Gpu, layout: &Layout, source: &str, uniforms: &FaceUniforms) -> (Gpu, Frame) {
    let mut renderer = Renderer::new(gpu, layout, source).expect("renderer builds");
    let mut frame = Frame::default();
    renderer
        .render(uniforms, &mut frame)
        .expect("frame renders");
    (renderer.into_gpu(), frame)
}

fn face_uniforms(layout: &Layout) -> FaceUniforms {
    let face = Face::default_face();
    let mut rig = Rig::new(&face).unwrap();
    rig.update(&face, InputStore::new().values(), 0.0);
    rig.pack(
        &face,
        &FrameState {
            face_scale: fit_scale(layout, face.box_mm),
            ..Default::default()
        },
    )
}

fn check_golden(name: &str, frame: &Frame) {
    let path = golden_path(name);
    if std::env::var_os("FACEGEN_UPDATE_GOLDENS").is_some() {
        write_png(&path, frame).expect("golden written");
        return;
    }
    let (w, h, golden) = read_png_rgb(&path).unwrap_or_else(|e| {
        panic!("missing golden {name} ({e}); run with FACEGEN_UPDATE_GOLDENS=1")
    });
    assert_eq!(
        (w, h),
        (frame.width, frame.height),
        "{name}: atlas size changed"
    );
    let rgb = frame.to_rgb();
    let worst = rgb
        .iter()
        .zip(&golden)
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .unwrap_or(0);
    assert!(
        worst <= TOLERANCE,
        "{name}: rendered frame differs from the golden by up to {worst} per channel"
    );
}

#[test]
fn every_preset_renders_the_test_pattern_and_the_default_face_to_their_goldens() {
    let mut gpu = lavapipe();
    for name in presets::NAMES {
        let layout = presets::load(name).unwrap();
        let (g, frame) = render(
            gpu,
            &layout,
            &shader::test_pattern_source(),
            &FaceUniforms::default(),
        );
        check_golden(&format!("pattern_{name}"), &frame);
        let (g, frame) = render(g, &layout, &shader::face_source(), &face_uniforms(&layout));
        check_golden(&format!("face_{name}"), &frame);
        gpu = g;
    }
}

/// RGB of one atlas pixel.
fn px(frame: &Frame, x: u32, y: u32) -> [u8; 3] {
    let i = (y * frame.width + x) as usize * Frame::BYTES_PER_PIXEL;
    [frame.bgra[i + 2], frame.bgra[i + 1], frame.bgra[i]]
}

fn is_white(c: [u8; 3]) -> bool {
    c.iter().all(|&v| v > 200)
}

fn is_yellow(c: [u8; 3]) -> bool {
    c[0] > 200 && c[1] > 150 && c[2] < 80
}

fn is_black(c: [u8; 3]) -> bool {
    c.iter().all(|&v| v < 8)
}

#[test]
fn the_test_pattern_lands_where_the_layout_transform_says_on_both_sides() {
    // two_64x32: right panel is atlas x 0..64 with M = 3 I and O = (0, -48),
    // so face (x, y) mm sits at atlas (x/3, (y + 48)/3); the left panel is
    // atlas x 64..128 with M = diag(3, -3) and O = (0, 48), so face (x, y)
    // sits at atlas (64 + x/3, (48 - y)/3). The white disc is at (15, 0)
    // and the yellow one at (35, 15).
    let layout = presets::load("two_64x32").unwrap();
    let (_, frame) = render(
        lavapipe(),
        &layout,
        &shader::test_pattern_source(),
        &FaceUniforms::default(),
    );
    assert!(
        is_white(px(&frame, 5, 16)),
        "white disc missing on the right panel: {:?}",
        px(&frame, 5, 16)
    );
    assert!(
        is_white(px(&frame, 69, 16)),
        "white disc missing on the left panel: {:?}",
        px(&frame, 69, 16)
    );
    assert!(
        is_yellow(px(&frame, 11, 21)),
        "yellow disc should be below centre in atlas rows on the upside-down right panel: {:?}",
        px(&frame, 11, 21)
    );
    assert!(
        !is_yellow(px(&frame, 11, 11)),
        "yellow disc appeared on the wrong row of the right panel: rotation is not applied"
    );
    assert!(
        is_yellow(px(&frame, 75, 11)),
        "yellow disc should be above centre on the upright left panel: {:?}",
        px(&frame, 75, 11)
    );
    assert!(
        !is_yellow(px(&frame, 75, 21)),
        "yellow disc appeared on the wrong row of the left panel"
    );
}

#[test]
fn the_default_face_puts_a_lit_eye_and_an_empty_corner_where_the_toml_says() {
    // Eye centre (122, 16) mm: right panel atlas (40, 21), left panel
    // (104, 10). Face (0, 0) mm is between the features on both panels.
    let layout = presets::load("two_64x32").unwrap();
    let (_, frame) = render(
        lavapipe(),
        &layout,
        &shader::face_source(),
        &face_uniforms(&layout),
    );
    let eye_colour = |c: [u8; 3]| c[2] > 200 && c[1] > 150 && c[0] < 40;
    assert!(
        eye_colour(px(&frame, 40, 21)),
        "right eye centre not lit in eye colour: {:?}",
        px(&frame, 40, 21)
    );
    assert!(
        eye_colour(px(&frame, 104, 10)),
        "left eye centre not lit in eye colour: {:?}",
        px(&frame, 104, 10)
    );
    assert!(
        is_black(px(&frame, 0, 16)),
        "right panel origin should be background: {:?}",
        px(&frame, 0, 16)
    );
    assert!(
        is_black(px(&frame, 64, 16)),
        "left panel origin should be background: {:?}",
        px(&frame, 64, 16)
    );
    // Mouth at (80, -28 + 16 * (80/158)^2 ~ -24) mm: right panel row (48 - 24)/3 = 8, left row (48 + 24)/3 = 24.
    assert!(
        eye_colour(px(&frame, 26, 8)),
        "right mouth band missing: {:?}",
        px(&frame, 26, 8)
    );
    assert!(
        eye_colour(px(&frame, 90, 24)),
        "left mouth band missing: {:?}",
        px(&frame, 90, 24)
    );
}
