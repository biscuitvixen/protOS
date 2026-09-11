//! Command line entry point. Subcommands arrive one milestone at a time;
//! `inputs` prints the contract table so producers can check addresses.

use std::io::{self, BufWriter, Write};

use clap::{Parser, Subcommand};
use facegen::contract;
use facegen::layout::atlas::Atlas;
use facegen::layout::{Layout, presets};
use facegen::render::gpu::Gpu;
use facegen::render::{Renderer, TEST_PATTERN_WGSL};
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
    /// Run the render loop with the browser harness.
    Serve {
        /// A preset name or a path to a layout TOML file.
        #[arg(long, default_value = "two_64x32")]
        layout: String,
        /// Address to listen on; 0.0.0.0 makes it reachable over Tailscale.
        #[arg(long, default_value = "0.0.0.0:8080")]
        bind: SocketAddr,
        /// Substring of the GPU adapter name to use, e.g. "llvmpipe".
        #[arg(long)]
        adapter: Option<String>,
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
            adapter,
        } => {
            serve(load_layout(&layout)?, bind, adapter.as_deref())?;
            Ok(())
        }
        Command::Render {
            layout,
            out,
            adapter,
        } => {
            render_once(&load_layout(&layout)?, &out, adapter.as_deref())?;
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

fn serve(layout: Layout, bind: SocketAddr, adapter: Option<&str>) -> anyhow::Result<()> {
    let runtime = tokio::runtime::Runtime::new()?;
    let listener = runtime.block_on(facegen::web::bind(bind))?;
    let gpu = Gpu::new(adapter)?;
    let (shared, _render_thread) = facegen::app::start(gpu, layout)?;
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

fn render_once(
    layout: &Layout,
    out: &std::path::Path,
    adapter: Option<&str>,
) -> anyhow::Result<()> {
    let gpu = Gpu::new(adapter)?;
    let mut renderer = Renderer::new(gpu, layout, TEST_PATTERN_WGSL)?;
    let mut frame = Frame::default();
    renderer.render(&mut frame)?;
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
            "\n{:?}  {:?}  {}x{} px @ {} mm  rot {}  connector {} chain {}",
            panel.name,
            panel.side,
            panel.width(),
            panel.height(),
            panel.pitch_mm,
            u16::from(panel.rotation),
            panel.connector,
            panel.chain_index
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
