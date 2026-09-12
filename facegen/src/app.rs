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

use std::path::PathBuf;
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use tokio::sync::{broadcast, watch};

use crate::contract::{self, INPUT_COUNT, InputStore};
use crate::face::{Face, FrameState, fit_scale};
use crate::layout::atlas::AtlasRect;
use crate::layout::{Layout, PanelTransform, presets};
use crate::render::gpu::Gpu;
use crate::render::shader::{Assembled, FACE_SET};
use crate::render::{Renderer, Scene, shader};
use crate::rig::Rig;
use crate::sinks::Frame;

/// Render period; the LED refresh is independent of this.
pub const TICK: Duration = Duration::from_micros(16_667);

/// How often the render thread logs its frame statistics.
const STATS_PERIOD: Duration = Duration::from_secs(5);

/// Messages from the harness and the file watcher to the render thread.
#[derive(Debug)]
pub enum Control {
    /// Replace the layout. Already validated by the sender.
    SetLayout(Layout),
    /// Re-read the shader files and rebuild the pass.
    ReloadShaders,
    /// Re-read the face file and recompile the rig.
    ReloadFace,
    /// Show a different scene.
    SetScene(Scene),
}

/// Where editable files live. With no directory everything comes from
/// the copies embedded at build time and nothing reloads.
#[derive(Clone, Debug, Default)]
pub struct Assets {
    pub dir: Option<PathBuf>,
    pub face: String,
}

impl Assets {
    /// The assets directory holds `shaders/` and `faces/`; the crate
    /// directory is one, checked relative to the working directory so
    /// `cargo run` from the workspace root finds it.
    pub fn detect() -> Option<PathBuf> {
        ["facegen", "."]
            .into_iter()
            .map(PathBuf::from)
            .find(|d| d.join("shaders").is_dir() && d.join("faces").is_dir())
            .and_then(|d| d.canonicalize().ok())
    }

    pub fn shaders_dir(&self) -> Option<PathBuf> {
        self.dir.as_ref().map(|d| d.join("shaders"))
    }

    pub fn face_path(&self) -> Option<PathBuf> {
        self.dir
            .as_ref()
            .map(|d| d.join("faces").join(format!("{}.toml", self.face)))
    }

    pub fn load_face(&self) -> anyhow::Result<Face> {
        match self.face_path() {
            Some(path) => Face::load(&path),
            None => Ok(Face::default_face()),
        }
    }

    pub fn load_shaders(&self) -> anyhow::Result<Assembled> {
        Assembled::load(FACE_SET, self.shaders_dir().as_deref())
    }
}

/// A one-line report of a reload or a fault, for the page and the log.
#[derive(Clone, Debug, Serialize)]
pub struct Notice {
    pub ok: bool,
    pub message: String,
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
    pub scenes: Vec<&'static str>,
    pub scene: &'static str,
    pub layout: Layout,
    pub panels: Vec<PanelInfo>,
}

impl LayoutInfo {
    pub fn new(generation: u32, renderer: &Renderer, layout: &Layout) -> Self {
        let atlas = renderer.atlas();
        Self {
            generation,
            gpu: renderer.gpu().info.name.clone(),
            presets: presets::NAMES.to_vec(),
            atlas_width: atlas.width,
            atlas_height: atlas.height,
            scenes: Scene::NAMES.to_vec(),
            scene: renderer.scene().name(),
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
    pub notices: broadcast::Sender<Arc<Notice>>,
    pub control: Mutex<mpsc::Sender<Control>>,
}

impl Shared {
    /// Log a notice and hand it to every connected page.
    pub fn notify(&self, ok: bool, message: impl Into<String>) {
        let message = message.into();
        if ok {
            tracing::info!("{message}");
        } else {
            tracing::warn!("{message}");
        }
        let _ = self.notices.send(Arc::new(Notice { ok, message }));
    }
}

/// Build the renderer for `layout`, publish its description, and start
/// the render thread. Returns the shared state the web harness uses.
pub fn start(
    gpu: Gpu,
    layout: Layout,
    assets: Assets,
) -> anyhow::Result<(Arc<Shared>, thread::JoinHandle<()>)> {
    let face = assets.load_face()?;
    let shaders = assets.load_shaders()?;
    shaders.validate().map_err(anyhow::Error::msg)?;
    let renderer = Renderer::new(gpu, &layout, &shaders.source)?;
    let rig = Rig::new(&face)?;
    let info = LayoutInfo::new(1, &renderer, &layout);
    let (control_tx, control_rx) = mpsc::channel();
    let shared = Arc::new(Shared {
        inputs: Arc::new(Mutex::new(InputStore::new())),
        frames: watch::Sender::new(Arc::new(Frame::default())),
        layout: watch::Sender::new(Arc::new(info)),
        notices: broadcast::channel(16).0,
        control: Mutex::new(control_tx.clone()),
    });
    if let Some(dir) = &assets.dir {
        crate::watch::start(dir.clone(), control_tx)?;
    }
    let handle = thread::Builder::new().name("render".into()).spawn({
        let shared = Arc::clone(&shared);
        move || render_loop(renderer, rig, layout, face, assets, shared, control_rx)
    })?;
    Ok((shared, handle))
}

fn render_loop(
    mut renderer: Renderer,
    mut rig: Rig,
    mut layout: Layout,
    mut face: Face,
    assets: Assets,
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
                    let source = match assets.load_shaders() {
                        Ok(a) => a.source,
                        Err(_) => shader::face_source(),
                    };
                    let gpu = renderer.into_gpu();
                    // A failed rebuild leaves nothing to render with; the
                    // sender validated the layout, so this is a GPU fault.
                    renderer = match Renderer::new(gpu, &layout, &source) {
                        Ok(r) => r,
                        Err(e) => {
                            tracing::error!("renderer rebuild failed: {e:#}");
                            return;
                        }
                    };
                    generation += 1;
                    let info = LayoutInfo::new(generation, &renderer, &layout);
                    shared.layout.send_replace(Arc::new(info));
                    tracing::info!(generation, layout = %layout.name, "layout changed");
                }
                Ok(Control::ReloadShaders) => match reload_shaders(&assets, &mut renderer) {
                    Ok(()) => shared.notify(true, "shaders reloaded"),
                    Err(e) => shared.notify(false, format!("shader reload failed: {e:#}")),
                },
                Ok(Control::SetScene(scene)) => {
                    renderer.set_scene(scene);
                    let info = LayoutInfo::new(generation, &renderer, &layout);
                    shared.layout.send_replace(Arc::new(info));
                    tracing::info!(scene = scene.name(), "scene changed");
                }
                Ok(Control::ReloadFace) => match reload_face(&assets, &mut rig) {
                    Ok(f) => {
                        face = f;
                        state.face_scale = fit_scale(&layout, face.box_mm);
                        shared.notify(true, format!("face {:?} reloaded", face.name));
                    }
                    Err(e) => shared.notify(false, format!("face reload failed: {e:#}")),
                },
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

/// Read, validate and rebuild; the old pipeline survives any failure.
fn reload_shaders(assets: &Assets, renderer: &mut Renderer) -> anyhow::Result<()> {
    let assembled = assets.load_shaders()?;
    assembled.validate().map_err(anyhow::Error::msg)?;
    renderer.rebuild_pipeline(&assembled.source)
}

/// Read and recompile; the old face survives any failure.
fn reload_face(assets: &Assets, rig: &mut Rig) -> anyhow::Result<Face> {
    let face = assets.load_face()?;
    rig.replace_face(&face)?;
    Ok(face)
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
