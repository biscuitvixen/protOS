//! Process wiring: the render thread, the state it shares with the web
//! harness, and the channels between them.
//!
//! The render thread owns the GPU and ticks at a fixed rate. Each frame
//! is published through a watch channel as a shared `Frame`, so a slow
//! consumer sees the newest frame and skips the rest instead of
//! queueing. Layout changes arrive on a control channel and rebuild the
//! renderer on the same device; the layout description the page needs
//! is published through a second watch channel with a generation
//! number that every frame carries.

use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use tokio::sync::watch;

use crate::contract::{self, INPUT_COUNT, InputStore};
use crate::face::{Face, FrameState, fit_scale};
use crate::layout::atlas::{Atlas, AtlasRect};
use crate::layout::{Layout, PanelTransform, presets};
use crate::render::gpu::Gpu;
use crate::render::{Renderer, shader};
use crate::rig::Rig;
use crate::sinks::Frame;

/// Render period; the LED refresh is independent of this.
pub const TICK: Duration = Duration::from_micros(16_667);

/// How often the render thread logs its frame statistics.
const STATS_PERIOD: Duration = Duration::from_secs(5);

/// Messages from the harness to the render thread.
#[derive(Debug)]
pub enum Control {
    /// Replace the layout. Already validated by the sender.
    SetLayout(Layout),
}

/// One panel as the page needs it: where it is in the atlas and how
/// its pixels map into face-space.
#[derive(Clone, Debug, Serialize)]
pub struct PanelInfo {
    pub atlas: AtlasRect,
    pub transform: PanelTransform,
}

/// Everything the page needs to draw frames; resent whenever the
/// layout changes.
#[derive(Clone, Debug, Serialize)]
pub struct LayoutInfo {
    pub generation: u32,
    pub gpu: String,
    pub presets: Vec<&'static str>,
    pub atlas_width: u32,
    pub atlas_height: u32,
    pub layout: Layout,
    pub panels: Vec<PanelInfo>,
}

impl LayoutInfo {
    pub fn new(generation: u32, gpu: &Gpu, layout: &Layout, atlas: &Atlas) -> Self {
        Self {
            generation,
            gpu: gpu.info.name.clone(),
            presets: presets::NAMES.to_vec(),
            atlas_width: atlas.width,
            atlas_height: atlas.height,
            layout: layout.clone(),
            panels: layout
                .panels
                .iter()
                .zip(&atlas.rects)
                .map(|(p, r)| PanelInfo {
                    atlas: *r,
                    transform: p.transform(),
                })
                .collect(),
        }
    }
}

/// The input vocabulary as the page renders its sliders.
#[derive(Clone, Debug, Serialize)]
pub struct InputInfo {
    pub name: &'static str,
    pub feature: String,
    pub side: String,
    pub signed: bool,
    pub value: f32,
}

pub fn inputs_info(store: &InputStore) -> Vec<InputInfo> {
    contract::all_ids()
        .map(|id| {
            let s = id.spec();
            InputInfo {
                name: s.name,
                feature: format!("{:?}", s.feature),
                side: format!("{:?}", s.side),
                signed: s.range == contract::Range::Signed,
                value: store.get(id),
            }
        })
        .collect()
}

/// State shared between the render thread and the web tasks.
pub struct Shared {
    pub inputs: Arc<Mutex<InputStore>>,
    pub frames: watch::Sender<Arc<Frame>>,
    pub layout: watch::Sender<Arc<LayoutInfo>>,
    pub control: Mutex<mpsc::Sender<Control>>,
}

/// Build the renderer for `layout`, publish its description, and start
/// the render thread. Returns the shared state the web harness uses.
pub fn start(
    gpu: Gpu,
    layout: Layout,
    face: Face,
) -> anyhow::Result<(Arc<Shared>, thread::JoinHandle<()>)> {
    let renderer = Renderer::new(gpu, &layout, &shader::face_source())?;
    let rig = Rig::new(&face)?;
    let info = LayoutInfo::new(1, renderer.gpu(), &layout, renderer.atlas());
    let (control_tx, control_rx) = mpsc::channel();
    let shared = Arc::new(Shared {
        inputs: Arc::new(Mutex::new(InputStore::new())),
        frames: watch::Sender::new(Arc::new(Frame::default())),
        layout: watch::Sender::new(Arc::new(info)),
        control: Mutex::new(control_tx),
    });
    let handle = thread::Builder::new().name("render".into()).spawn({
        let shared = Arc::clone(&shared);
        move || render_loop(renderer, rig, layout, face, shared, control_rx)
    })?;
    Ok((shared, handle))
}

fn render_loop(
    mut renderer: Renderer,
    mut rig: Rig,
    mut layout: Layout,
    face: Face,
    shared: Arc<Shared>,
    control: mpsc::Receiver<Control>,
) {
    let mut generation = shared.layout.borrow().generation;
    let mut state = FrameState {
        face_scale: fit_scale(&layout, face.box_mm),
        ..Default::default()
    };
    let started = Instant::now();
    let mut next = started;
    let mut stats = Stats::new();
    loop {
        loop {
            match control.try_recv() {
                Ok(Control::SetLayout(new_layout)) => {
                    layout = new_layout;
                    state.face_scale = fit_scale(&layout, face.box_mm);
                    let gpu = renderer.into_gpu();
                    // A failed rebuild leaves nothing to render with; the
                    // sender validated the layout, so this is a GPU fault.
                    renderer = match Renderer::new(gpu, &layout, &shader::face_source()) {
                        Ok(r) => r,
                        Err(e) => {
                            tracing::error!("renderer rebuild failed: {e:#}");
                            return;
                        }
                    };
                    generation += 1;
                    let info =
                        LayoutInfo::new(generation, renderer.gpu(), &layout, renderer.atlas());
                    shared.layout.send_replace(Arc::new(info));
                    tracing::info!(generation, layout = %layout.name, "layout changed");
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => return,
            }
        }
        let now = Instant::now();
        let t = now.duration_since(started).as_secs_f32();
        state.dt_s = t - state.time_s;
        state.time_s = t;
        state.frame = state.frame.wrapping_add(1);
        let raw: [f32; INPUT_COUNT] = *shared.inputs.lock().expect("input store lock").values();
        rig.update(&face, &raw, state.dt_s);
        let uniforms = rig.pack(&face, &state);
        let mut frame = Frame::default();
        if let Err(e) = renderer.render(&uniforms, &mut frame) {
            tracing::error!("render failed: {e:#}");
            return;
        }
        stats.record(now.elapsed());
        shared.frames.send_replace(Arc::new(frame));
        stats.maybe_log(shared.frames.receiver_count());
        next += TICK;
        let now = Instant::now();
        if next > now {
            thread::sleep(next - now);
        } else {
            next = now;
        }
    }
}

/// Frame timing, logged every `STATS_PERIOD`.
struct Stats {
    since: Instant,
    frames: u32,
    total: Duration,
    worst: Duration,
}

impl Stats {
    fn new() -> Self {
        Self {
            since: Instant::now(),
            frames: 0,
            total: Duration::ZERO,
            worst: Duration::ZERO,
        }
    }

    fn record(&mut self, render: Duration) {
        self.frames += 1;
        self.total += render;
        self.worst = self.worst.max(render);
    }

    fn maybe_log(&mut self, clients: usize) {
        let elapsed = self.since.elapsed();
        if elapsed < STATS_PERIOD || self.frames == 0 {
            return;
        }
        tracing::info!(
            fps = format_args!("{:.1}", self.frames as f64 / elapsed.as_secs_f64()),
            render_mean_ms =
                format_args!("{:.2}", self.total.as_secs_f64() * 1e3 / self.frames as f64),
            render_max_ms = format_args!("{:.2}", self.worst.as_secs_f64() * 1e3),
            clients,
            "render"
        );
        *self = Self::new();
    }
}
