//! Animated captures for documentation: the fake producer's curves
//! through the rig, frame by frame, written as an animated PNG that
//! any browser plays.

use std::fs::File;
use std::io::BufWriter;
use std::path::Path;
use std::time::Instant;

use anyhow::Context;

use crate::contract::InputStore;
use crate::face::{Face, FrameState, fit_scale};
use crate::fake;
use crate::layout::Layout;
use crate::render::{Renderer, Scene};
use crate::rig::Rig;
use crate::sinks::Frame;
use crate::{preview, sinks};

pub struct Options {
    pub scene: Scene,
    pub seconds: f32,
    pub fps: u32,
    /// Pixels per millimetre of the visor view; 0 writes the raw atlas.
    pub px_per_mm: f32,
}

/// Render `seconds * fps` frames from t = 0 and write them to `out`.
pub fn capture(
    renderer: &mut Renderer,
    layout: &Layout,
    face: &Face,
    opts: &Options,
    out: &Path,
) -> anyhow::Result<()> {
    let frames = (opts.seconds * opts.fps as f32).round().max(1.0) as u32;
    let mut rig = Rig::new(face)?;
    let mut store = InputStore::new();
    let mut frame = Frame::default();
    let mut state = FrameState {
        face_scale: fit_scale(layout, face.box_mm),
        ..Default::default()
    };
    renderer.set_scene(opts.scene);
    let mut encoder: Option<png::Writer<BufWriter<File>>> = None;
    let started = Instant::now();
    for n in 0..frames {
        let t = n as f32 / opts.fps as f32;
        for (address, value) in fake::curves(t) {
            store.set_by_address(address, value, started);
        }
        state.dt_s = if n == 0 { 0.0 } else { 1.0 / opts.fps as f32 };
        state.time_s = t;
        state.frame = n;
        rig.update(face, store.values(), state.dt_s);
        renderer.render(&rig.pack(face, &state), &mut frame)?;
        let (width, height, rgb) = if opts.px_per_mm > 0.0 {
            let image = preview::compose(layout, renderer.atlas(), &frame, opts.px_per_mm, true);
            (image.width, image.height, image.rgb)
        } else {
            (frame.width, frame.height, frame.to_rgb())
        };
        let writer = match encoder.as_mut() {
            Some(w) => w,
            None => {
                let file =
                    File::create(out).with_context(|| format!("creating {}", out.display()))?;
                let mut enc = png::Encoder::new(BufWriter::new(file), width, height);
                enc.set_color(png::ColorType::Rgb);
                enc.set_depth(png::BitDepth::Eight);
                enc.set_animated(frames, 0)?;
                enc.set_frame_delay(1, opts.fps as u16)?;
                encoder.insert(enc.write_header()?)
            }
        };
        writer.write_image_data(&rgb)?;
    }
    if let Some(w) = encoder {
        w.finish()?;
    }
    let _ = sinks::NullSink;
    Ok(())
}
