//! Command line entry point. Subcommands arrive one milestone at a time;
//! `inputs` prints the contract table so producers can check addresses.

use std::io::{self, BufWriter, Write};

use clap::{Parser, Subcommand};
use facegen::contract;
use facegen::contract::InputStore;
use facegen::face::{Face, FrameState, fit_scale};
use facegen::features::mouth::MouthMode;
use facegen::layout::atlas::Atlas;
use facegen::layout::{Layout, presets};
use facegen::render::gpu::Gpu;
use facegen::render::{Renderer, shader};
use facegen::rig::Rig;
use facegen::sinks::Frame;
use facegen::sinks::png::write_png;
use std::net::SocketAddr;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "facegen",
    version,
    about = "GPU-rendered procedural protogen face"
)]
struct Cli {
    /// More log detail (-v debug, -vv trace). RUST_LOG overrides.
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Print every input the face accepts: index, OSC address, feature, side, range.
    Inputs,
    /// Show a layout: validation, atlas packing and each panel's face-space transform.
    Layout {
        /// A preset name (two_64x32, six_panel) or a path to a layout TOML file.
        layout: String,
    },
    /// Run the render loop with the OSC receiver and the browser harness.
    Serve {
        /// A preset name or a path to a layout TOML file.
        #[arg(long, default_value = "two_64x32")]
        layout: String,
        /// Address to listen on; 0.0.0.0 makes it reachable over Tailscale.
        #[arg(long, default_value = "0.0.0.0:8080")]
        bind: SocketAddr,
        /// UDP address for OSC input; Babble's default output port.
        #[arg(long, default_value = "127.0.0.1:8888")]
        osc: SocketAddr,
        /// Directory holding shaders/ and faces/ to watch for edits; found
        /// automatically when run from the workspace, otherwise embedded
        /// copies are used and nothing reloads.
        #[arg(long)]
        assets: Option<PathBuf>,
        /// Face to load from faces/<name>.toml.
        #[arg(long, default_value = "default")]
        face: String,
        /// Frame sink besides the browser: piomatter (Pi 5 panels) or null. Repeatable.
        #[arg(long)]
        sink: Vec<String>,
        /// Substring of the GPU adapter name to use, e.g. "llvmpipe".
        #[arg(long)]
        adapter: Option<String>,
    },
    /// Send canned blendshape curves over OSC, standing in for a tracker.
    Fake {
        /// Destination, normally facegen's OSC address.
        #[arg(long, default_value = "127.0.0.1:8888")]
        to: String,
        /// Messages per channel per second.
        #[arg(long, default_value_t = 30.0)]
        rate: f32,
    },
    /// Write an animated PNG of the face driven by the fake curves.
    Capture {
        /// A preset name or a path to a layout TOML file.
        #[arg(long, default_value = "two_64x32")]
        layout: String,
        /// Scene to draw: face or cube.
        #[arg(long, default_value = "face")]
        scene: String,
        #[arg(long, default_value_t = 6.0)]
        seconds: f32,
        #[arg(long, default_value_t = 20)]
        fps: u32,
        /// Visor-view scale in pixels per millimetre; 0 writes the raw atlas.
        #[arg(long, default_value_t = 2.0)]
        scale: f32,
        #[arg(long, default_value = "capture.png")]
        out: PathBuf,
        /// Substring of the GPU adapter name to use, e.g. "llvmpipe".
        #[arg(long)]
        adapter: Option<String>,
        /// Override the face's mouth mode: jaw or scope.
        #[arg(long)]
        mouth_mode: Option<MouthMode>,
    },
    /// Render one frame of the test pattern to a PNG.
    Render {
        /// A preset name or a path to a layout TOML file.
        #[arg(long, default_value = "two_64x32")]
        layout: String,
        /// Output PNG path.
        #[arg(long, default_value = "frame.png")]
        out: PathBuf,
        /// Substring of the GPU adapter name to use, e.g. "llvmpipe".
        #[arg(long)]
        adapter: Option<String>,
        /// Draw the bring-up test pattern instead of the face.
        #[arg(long)]
        test_pattern: bool,
        /// Set an input for the frame, e.g. --set jawOpen=1 (repeatable).
        #[arg(long = "set", value_name = "NAME=VALUE")]
        inputs: Vec<String>,
        /// Scene to draw: face or cube.
        #[arg(long, default_value = "face")]
        scene: String,
        /// Time in seconds for animated scenes.
        #[arg(long, default_value_t = 0.0)]
        time: f32,
        /// Override the face's mouth mode: jaw or scope.
        #[arg(long)]
        mouth_mode: Option<MouthMode>,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    init_logging(cli.verbose);
    let result = match cli.command {
        Command::Inputs => print_inputs(),
        Command::Layout { layout } => print_layout(&load_layout(&layout)?),
        Command::Serve {
            layout,
            bind,
            osc,
            assets,
            face,
            sink,
            adapter,
        } => {
            let assets = facegen::app::Assets {
                dir: assets.or_else(facegen::app::Assets::detect),
                face,
            };
            serve(
                load_layout(&layout)?,
                bind,
                osc,
                assets,
                &sink,
                adapter.as_deref(),
            )?;
            Ok(())
        }
        Command::Fake { to, rate } => {
            let sender = facegen::osc::Sender::new(to.as_str())?;
            println!("sending fake curves to {to} at {rate} Hz per channel");
            facegen::fake::run(&sender, rate)?;
            Ok(())
        }
        Command::Capture {
            layout,
            scene,
            seconds,
            fps,
            scale,
            out,
            adapter,
            mouth_mode,
        } => {
            let layout = load_layout(&layout)?;
            let scene = facegen::render::Scene::parse(&scene)
                .ok_or_else(|| anyhow::anyhow!("no scene named {scene:?}"))?;
            let gpu = Gpu::new(adapter.as_deref())?;
            let mut renderer = Renderer::new(gpu, &layout, &shader::face_source())?;
            let mut face = Face::default_face();
            if let Some(mode) = mouth_mode {
                face.mouth.mode = mode;
            }
            let opts = facegen::capture::Options {
                scene,
                seconds,
                fps,
                px_per_mm: scale,
            };
            facegen::capture::capture(&mut renderer, &layout, &face, &opts, &out)?;
            println!(
                "{} frames at {fps} fps -> {}",
                (seconds * fps as f32).round(),
                out.display()
            );
            Ok(())
        }
        Command::Render {
            layout,
            out,
            adapter,
            test_pattern,
            inputs,
            scene,
            time,
            mouth_mode,
        } => {
            let scene = facegen::render::Scene::parse(&scene)
                .ok_or_else(|| anyhow::anyhow!("no scene named {scene:?}"))?;
            let mut face = Face::default_face();
            if let Some(mode) = mouth_mode {
                face.mouth.mode = mode;
            }
            let opts = RenderOnce {
                test_pattern,
                scene,
                time,
                face,
            };
            render_once(
                &load_layout(&layout)?,
                &out,
                adapter.as_deref(),
                &inputs,
                &opts,
            )?;
            Ok(())
        }
    };
    // A reader closing the pipe early (`facegen inputs | head`) is a
    // normal way to stop, not an error.
    match result {
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        other => Ok(other?),
    }
}

/// Default to facegen's own info lines with the GPU stack quiet; -v adds
/// debug, -vv trace; RUST_LOG replaces the whole filter.
fn init_logging(verbose: u8) {
    use tracing_subscriber::EnvFilter;
    let level = match verbose {
        0 => "info",
        1 => "debug",
        _ => "trace",
    };
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(format!("warn,facegen={level}")));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(verbose > 0)
        .init();
}

fn serve(
    layout: Layout,
    bind: SocketAddr,
    osc: SocketAddr,
    assets: facegen::app::Assets,
    sinks: &[String],
    adapter: Option<&str>,
) -> anyhow::Result<()> {
    let runtime = tokio::runtime::Runtime::new()?;
    let listener = runtime.block_on(facegen::web::bind(bind))?;
    match &assets.dir {
        Some(dir) => println!("assets: {} (edits reload live)", dir.display()),
        None => println!("assets: embedded (no live reload; pass --assets)"),
    }
    let gpu = Gpu::new(adapter)?;
    let (shared, _render_thread) = facegen::app::start(gpu, layout, assets, sinks)?;
    let _osc_thread = facegen::osc::start_receiver(osc, std::sync::Arc::clone(&shared.inputs))?;
    let port = listener.local_addr()?.port();
    println!("facegen harness:");
    println!("  http://localhost:{port}/");
    if bind.ip().is_unspecified() {
        let host = std::fs::read_to_string("/etc/hostname")
            .map(|h| h.trim().to_string())
            .unwrap_or_default();
        if !host.is_empty() {
            println!("  http://{host}:{port}/");
        }
    }
    runtime.block_on(facegen::web::serve(listener, shared))
}

/// What one `render` frame shows, beyond the layout and the inputs.
struct RenderOnce {
    test_pattern: bool,
    scene: facegen::render::Scene,
    time: f32,
    face: Face,
}

fn render_once(
    layout: &Layout,
    out: &std::path::Path,
    adapter: Option<&str>,
    inputs: &[String],
    opts: &RenderOnce,
) -> anyhow::Result<()> {
    let mut store = InputStore::new();
    for spec in inputs {
        let (name, value) = spec
            .split_once('=')
            .ok_or_else(|| anyhow::anyhow!("--set expects NAME=VALUE, got {spec:?}"))?;
        let id = facegen::contract::lookup_name(name)
            .ok_or_else(|| anyhow::anyhow!("unknown input {name:?}"))?;
        store.set(id, value.parse()?, std::time::Instant::now());
    }
    let gpu = Gpu::new(adapter)?;
    let source = if opts.test_pattern {
        shader::test_pattern_source()
    } else {
        shader::face_source()
    };
    let mut renderer = Renderer::new(gpu, layout, &source)?;
    renderer.set_scene(opts.scene);
    let face = &opts.face;
    let mut rig = Rig::new(face)?;
    rig.update(face, store.values(), 0.0);
    let uniforms = rig.pack(
        face,
        &FrameState {
            time_s: opts.time,
            face_scale: fit_scale(layout, face.box_mm),
            ..Default::default()
        },
    );
    let mut frame = Frame::default();
    renderer.render(&uniforms, &mut frame)?;
    write_png(out, &frame)?;
    println!(
        "{} x {} atlas on {} -> {}",
        frame.width,
        frame.height,
        renderer.gpu().info.name,
        out.display()
    );
    Ok(())
}

/// A preset name first, then a file path.
fn load_layout(spec: &str) -> anyhow::Result<Layout> {
    if let Some(layout) = presets::load(spec) {
        return Ok(layout);
    }
    let text = std::fs::read_to_string(spec).map_err(|e| {
        anyhow::anyhow!(
            "{spec} is not a preset ({}) and could not be read: {e}",
            presets::NAMES.join(", ")
        )
    })?;
    Ok(Layout::from_toml(&text)?)
}

fn print_layout(layout: &Layout) -> io::Result<()> {
    let mut out = BufWriter::new(io::stdout().lock());
    writeln!(
        out,
        "layout {:?}: {:?}, {} address lines, {} planes",
        layout.name, layout.driver.pinout, layout.driver.n_addr_lines, layout.driver.n_planes
    )?;
    let atlas = match Atlas::build(layout) {
        Ok(atlas) => atlas,
        Err(e) => {
            writeln!(out, "invalid: {e}")?;
            return out.flush();
        }
    };
    writeln!(out, "atlas {} x {} px", atlas.width, atlas.height)?;
    for (panel, rect) in layout.panels.iter().zip(&atlas.rects) {
        let t = panel.transform();
        let (min, max) = t.bounds_mm(panel.size_px);
        writeln!(
            out,
            "\n{:?}  {:?}  {}x{} px @ {} mm  rot {}  connector {} chain {}  draws {:?}",
            panel.name,
            panel.side,
            panel.width(),
            panel.height(),
            panel.pitch_mm,
            u16::from(panel.rotation),
            panel.connector,
            panel.chain_index,
            panel.features
        )?;
        writeln!(
            out,
            "  atlas   x {:>3} y {:>3} w {:>3} h {:>3}",
            rect.x, rect.y, rect.w, rect.h
        )?;
        writeln!(
            out,
            "  col_u   [{:>7.2} {:>7.2}] mm   col_v [{:>7.2} {:>7.2}] mm",
            t.col_u[0], t.col_u[1], t.col_v[0], t.col_v[1]
        )?;
        writeln!(
            out,
            "  origin  [{:>7.2} {:>7.2}] mm   bounds x [{:.1}, {:.1}]  y [{:.1}, {:.1}]",
            t.origin[0], t.origin[1], min[0], max[0], min[1], max[1]
        )?;
        let c0 = t.pixel_centre_mm(0, 0);
        let c1 = t.pixel_centre_mm(panel.width() - 1, panel.height() - 1);
        writeln!(
            out,
            "  pixel (0,0) -> [{:.1} {:.1}] mm   pixel ({},{}) -> [{:.1} {:.1}] mm",
            c0[0],
            c0[1],
            panel.width() - 1,
            panel.height() - 1,
            c1[0],
            c1[1]
        )?;
    }
    out.flush()
}

fn print_inputs() -> io::Result<()> {
    let mut out = BufWriter::new(io::stdout().lock());
    writeln!(
        out,
        "{:>3}  {:<28} {:<8} {:<6} {:<7} source",
        "idx", "address", "feature", "side", "range"
    )?;
    for id in contract::all_ids() {
        let s = id.spec();
        writeln!(
            out,
            "{:>3}  {:<28} {:<8} {:<6} {:<7} {:?}",
            id.index(),
            s.address,
            format!("{:?}", s.feature),
            format!("{:?}", s.side),
            format!("{:?}", s.range),
            s.source
        )?;
    }
    out.flush()
}
