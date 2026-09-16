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
use facegen::features::mouth::MouthMode;
use facegen::layout::{Layout, presets};
use facegen::render::gpu::Gpu;
use facegen::render::uniforms::FaceUniforms;
use facegen::render::{Renderer, Scene, shader};
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

fn render_scene(gpu: Gpu, layout: &Layout, scene: Scene, time_s: f32) -> (Gpu, Frame) {
    let mut renderer = Renderer::new(gpu, layout, &shader::face_source()).expect("renderer builds");
    renderer.set_scene(scene);
    let mut uniforms = face_uniforms(layout);
    uniforms.g.time[0] = time_s;
    let mut frame = Frame::default();
    renderer
        .render(&uniforms, &mut frame)
        .expect("frame renders");
    (renderer.into_gpu(), frame)
}

fn face_uniforms(layout: &Layout) -> FaceUniforms {
    face_uniforms_with(layout, &[])
}

/// Uniforms for the default face with some inputs set by name.
fn face_uniforms_with(layout: &Layout, inputs: &[(&str, f32)]) -> FaceUniforms {
    face_uniforms_for(&Face::default_face(), layout, inputs)
}

/// Uniforms for `face` with some inputs set by name, through the rig
/// with a zero dt so the scope phase stays at zero.
fn face_uniforms_for(face: &Face, layout: &Layout, inputs: &[(&str, f32)]) -> FaceUniforms {
    let mut rig = Rig::new(face).unwrap();
    let mut store = InputStore::new();
    let now = web_time::Instant::now();
    for (name, value) in inputs {
        let id = facegen::contract::lookup_name(name).expect("known input");
        store.set(id, *value, now);
    }
    rig.update(face, store.values(), 0.0);
    rig.pack(
        face,
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
        let (g, frame) = render_scene(g, &layout, Scene::Cube, 0.7);
        check_golden(&format!("cube_{name}"), &frame);
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

#[test]
fn a_voiced_band_displaces_the_scope_line_by_its_full_amplitude() {
    // Scope mode, phase 0, voiceBand18 = 1 alone: the centroid is
    // 18/31 so the carrier runs 3 + 3 * 18/31 = 4.74 cycles, and at
    // u = 18/31 its argument is 2.75 turns, sin = -1. The line at
    // x = 8 + 150 * 18/31 = 95.1 mm drops from the baseline
    // -28 + 16 * (95.1/158)^2 = -22.2 (idle carrier -1.5 on top: -23.7)
    // to -32.2. Right panel atlas (x/3, (y + 48)/3): idle row 8, voiced
    // row 5. Left panel (64 + x/3, (48 - y)/3): idle row 24, voiced 26.
    let layout = presets::load("two_64x32").unwrap();
    let mut face = Face::default_face();
    face.mouth.mode = MouthMode::Scope;
    let voiced = [("voiceLevel", 1.0), ("voiceBand18", 1.0)];
    let (gpu, frame) = render(
        lavapipe(),
        &layout,
        &shader::face_source(),
        &face_uniforms_for(&face, &layout, &voiced),
    );
    check_golden("face_scope_voiced_two_64x32", &frame);
    let mouth = |c: [u8; 3]| c[2] > 200 && c[1] > 150 && c[0] < 40;
    for (x, y, side) in [(31, 5, "right"), (95, 26, "left")] {
        assert!(
            mouth(px(&frame, x, y)),
            "{side} scope line should dip to atlas ({x},{y}): {:?}",
            px(&frame, x, y)
        );
    }
    for (x, y, side) in [(31, 8, "right"), (95, 24, "left")] {
        assert!(
            is_black(px(&frame, x, y)),
            "{side} baseline row should be empty while voiced at atlas ({x},{y}): {:?}",
            px(&frame, x, y)
        );
    }
    let (_, frame) = render(
        gpu,
        &layout,
        &shader::face_source(),
        &face_uniforms_for(&face, &layout, &[]),
    );
    for (x, y, side) in [(31, 8, "right"), (95, 24, "left")] {
        assert!(
            mouth(px(&frame, x, y)),
            "{side} idle scope line should sit on the baseline at atlas ({x},{y}): {:?}",
            px(&frame, x, y)
        );
    }
    for (x, y, side) in [(31, 5, "right"), (95, 26, "left")] {
        assert!(
            is_black(px(&frame, x, y)),
            "{side} idle line should not reach atlas ({x},{y}): {:?}",
            px(&frame, x, y)
        );
    }
}

#[test]
fn an_open_jaw_shows_a_lower_tooth_where_the_lip_between_teeth_is_dark() {
    // two_64x32 at scale 1. Lower tooth apexes sit at x = 30, 74 and
    // 118 mm. With jawOpen = 1 the lower lip at x = 74 is at
    // -28 - 3.5 - 0.65 * 22 * (1 - 0.75 * 74/158) + 16 * (74/158)^2 ~ -37.3,
    // so the tooth spans down to -49.3 and (74, -44) is inside it; at
    // x = 52, between teeth, the lip is at ~ -40.5 and (52, -44) is
    // background. Right panel atlas (x/3, (y + 48)/3), left (64 + x/3,
    // (48 - y)/3).
    let layout = presets::load("two_64x32").unwrap();
    let (_, frame) = render(
        lavapipe(),
        &layout,
        &shader::face_source(),
        &face_uniforms_with(&layout, &[("jawOpen", 1.0)]),
    );
    let mouth = |c: [u8; 3]| c[2] > 200 && c[1] > 150 && c[0] < 40;
    for (x, y, side) in [(24, 1, "right"), (88, 30, "left")] {
        assert!(
            mouth(px(&frame, x, y)),
            "{side} lower tooth missing at atlas ({x},{y}): {:?}",
            px(&frame, x, y)
        );
    }
    for (x, y, side) in [(17, 1, "right"), (81, 30, "left")] {
        assert!(
            is_black(px(&frame, x, y)),
            "{side} lip between teeth should be background at atlas ({x},{y}): {:?}",
            px(&frame, x, y)
        );
    }
}

#[test]
fn six_panel_windows_draw_only_their_own_feature() {
    // Left eye window is face x 74..170, y -8..40 at atlas (0,0); the
    // mouth's outer upper tooth reaches into it around face (140, -5),
    // which lands at atlas (22, 15) and must stay dark. The mouth
    // window (atlas x 32.., face origin (62, -2)) shows the closed
    // band at face (100, -21.6) -> atlas (44, 6); the nose window
    // (atlas x 64.., face origin (-18, 42)) shows the nose centre
    // (26, 20) -> atlas (78, 7). The fitted scale is a little under 1,
    // which moves every probe by under a pixel.
    let layout = presets::load("six_panel").unwrap();
    let (_, frame) = render(
        lavapipe(),
        &layout,
        &shader::face_source(),
        &face_uniforms(&layout),
    );
    let lit = |c: [u8; 3]| c[2] > 200 && c[1] > 150 && c[0] < 40;
    assert!(
        is_black(px(&frame, 22, 15)),
        "the eye window must not draw the mouth: {:?}",
        px(&frame, 22, 15)
    );
    assert!(
        lit(px(&frame, 44, 6)),
        "the mouth window should show the band: {:?}",
        px(&frame, 44, 6)
    );
    assert!(
        lit(px(&frame, 78, 7)),
        "the nose window should show the nose: {:?}",
        px(&frame, 78, 7)
    );
}

#[test]
fn the_cube_sits_at_each_panel_centre_and_leaves_the_corners_dark() {
    let layout = presets::load("two_64x32").unwrap();
    let (_, frame) = render_scene(lavapipe(), &layout, Scene::Cube, 0.7);
    let lit = |c: [u8; 3]| c.iter().any(|&v| v > 60);
    assert!(
        lit(px(&frame, 32, 16)),
        "right panel centre should show the cube: {:?}",
        px(&frame, 32, 16)
    );
    assert!(
        lit(px(&frame, 96, 16)),
        "left panel centre should show the cube: {:?}",
        px(&frame, 96, 16)
    );
    for (x, y) in [(0, 0), (63, 31), (64, 0), (127, 31)] {
        assert!(
            is_black(px(&frame, x, y)),
            "corner ({x},{y}) should be dark: {:?}",
            px(&frame, x, y)
        );
    }
}

#[test]
fn a_capture_writes_an_animated_png_with_one_frame_per_tick() {
    use facegen::capture::{Options, capture};
    let layout = presets::load("two_64x32").unwrap();
    let mut renderer = Renderer::new(lavapipe(), &layout, &shader::face_source()).unwrap();
    let out = std::env::temp_dir().join(format!("facegen-capture-{}.png", std::process::id()));
    let opts = Options {
        scene: Scene::Face,
        seconds: 0.5,
        fps: 10,
        px_per_mm: 1.0,
    };
    capture(&mut renderer, &layout, &Face::default_face(), &opts, &out).expect("capture writes");
    let mut reader = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(&out).unwrap()))
        .read_info()
        .expect("capture is a valid png");
    let control = reader
        .info()
        .animation_control
        .expect("capture is animated");
    assert_eq!(control.num_frames, 5, "0.5 s at 10 fps is five frames");
    let (w, h) = (reader.info().width, reader.info().height);
    assert!(
        w > 384 && h > 96,
        "the visor view at 1 px/mm spans the 384 mm visor plus padding: {w}x{h}"
    );
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    reader.next_frame(&mut buf).expect("first frame decodes");
    std::fs::remove_file(&out).unwrap();
}

#[test]
fn the_present_pass_draws_the_visor_view_with_the_wearers_left_on_the_viewers_right() {
    use facegen::render::present::{PresentOptions, PresentPass};
    use facegen::render::target::RenderTarget;
    let layout = presets::load("two_64x32").unwrap();
    let mut renderer = Renderer::new(lavapipe(), &layout, &shader::face_source()).unwrap();
    let uniforms = face_uniforms(&layout);
    let (w, h) = (400, 120);
    let target = RenderTarget::with_format(
        &renderer.gpu().device,
        w,
        h,
        wgpu::TextureFormat::Rgba8Unorm,
    );
    let present = PresentPass::new(
        &renderer.gpu().device,
        wgpu::TextureFormat::Rgba8Unorm,
        renderer.atlas_view(),
    )
    .unwrap();
    let mut encoder = renderer
        .gpu()
        .device
        .create_command_encoder(&Default::default());
    renderer.draw_atlas(&uniforms, &mut encoder);
    let atlas = renderer.atlas().clone();
    present.draw(
        &renderer.gpu().queue,
        &mut encoder,
        &target.view,
        (w, h),
        &layout,
        &atlas,
        &PresentOptions {
            px_per_mm: 0.0,
            apply_gamma: true,
            led_mask: true,
            srgb_encode: true,
        },
    );
    target.copy_to_staging(&mut encoder);
    let submission = renderer.gpu().queue.submit([encoder.finish()]);
    let mut out = Frame::default();
    target
        .read_back(&renderer.gpu().device, submission, &mut out)
        .unwrap();
    // Rgba8 target: bytes are R, G, B, A.
    let rgba = |x: u32, y: u32| {
        let i = (y * w + x) as usize * 4;
        [out.bgra[i], out.bgra[i + 1], out.bgra[i + 2]]
    };
    let eye = |c: [u8; 3]| c[2] > 150 && c[1] > 100 && c[0] < 60;
    // Wearer's left eye at face (122, 16) mm sits right of centre and above the middle.
    let mut left_eye_hits = 0;
    let mut right_eye_hits = 0;
    for y in 0..h {
        for x in 0..w {
            if eye(rgba(x, y)) {
                if x > w / 2 && y < h / 2 {
                    left_eye_hits += 1;
                }
                if x < w / 2 && y < h / 2 {
                    right_eye_hits += 1;
                }
            }
        }
    }
    assert!(
        left_eye_hits > 50,
        "the wearer's left eye should be lit on the viewer's right: {left_eye_hits}"
    );
    assert!(
        right_eye_hits > 50,
        "the wearer's right eye should be lit on the viewer's left: {right_eye_hits}"
    );
    let corner = rgba(1, 1);
    assert!(
        corner.iter().all(|&v| v < 40),
        "the padding should be dark: {corner:?}"
    );
}
