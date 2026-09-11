//! Golden-image tests of the test pattern on lavapipe.
//!
//! Output is byte-stable on one driver, not across drivers, so the
//! goldens are pinned to Mesa's software Vulkan (llvmpipe), which every
//! dev box and the Pi can run. Regenerate with
//! FACEGEN_UPDATE_GOLDENS=1 after an intentional change and commit
//! the PNGs.

use std::path::PathBuf;

use facegen::layout::presets;
use facegen::render::gpu::Gpu;
use facegen::render::{Renderer, TEST_PATTERN_WGSL};
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

#[test]
fn every_preset_renders_the_test_pattern_to_its_golden_on_lavapipe() {
    let update = std::env::var_os("FACEGEN_UPDATE_GOLDENS").is_some();
    let gpu = Gpu::new(Some("llvmpipe"))
        .expect("lavapipe (Mesa software Vulkan) is required for golden tests");
    let mut frame = Frame::default();
    let mut renderer = None;
    for name in presets::NAMES {
        let layout = presets::load(name).unwrap();
        // One device, one renderer per layout; the device is moved in once.
        let gpu = renderer
            .take()
            .map(|r: Renderer| r.into_gpu())
            .unwrap_or_else(|| Gpu::new(Some("llvmpipe")).unwrap());
        let mut r = Renderer::new(gpu, &layout, TEST_PATTERN_WGSL).expect("renderer builds");
        r.render(&mut frame).expect("frame renders");
        let path = golden_path(name);
        if update {
            write_png(&path, &frame).expect("golden written");
            renderer = Some(r);
            continue;
        }
        let (w, h, golden) = read_png_rgb(&path).unwrap_or_else(|e| {
            panic!("missing golden for {name} ({e}); run with FACEGEN_UPDATE_GOLDENS=1")
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
        renderer = Some(r);
    }
    drop(gpu);
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

#[test]
fn the_test_pattern_lands_where_the_layout_transform_says_on_both_sides() {
    // two_64x32: right panel is atlas x 0..64 with M = 3 I and O = (0, -48),
    // so face (x, y) mm sits at atlas (x/3, (y + 48)/3); the left panel is
    // atlas x 64..128 with M = diag(3, -3) and O = (0, 48), so face (x, y)
    // sits at atlas (64 + x/3, (48 - y)/3). The white disc is at (15, 0)
    // and the yellow one at (35, 15).
    let gpu = Gpu::new(Some("llvmpipe")).expect("lavapipe required");
    let layout = presets::load("two_64x32").unwrap();
    let mut r = Renderer::new(gpu, &layout, TEST_PATTERN_WGSL).unwrap();
    let mut frame = Frame::default();
    r.render(&mut frame).unwrap();
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
