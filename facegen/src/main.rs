//! Command line entry point. Subcommands arrive one milestone at a time;
//! `inputs` prints the contract table so producers can check addresses.

use std::io::{self, BufWriter, Write};

use clap::{Parser, Subcommand};
use facegen::contract;

#[derive(Parser)]
#[command(
    name = "facegen",
    version,
    about = "GPU-rendered procedural protogen face"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Print every input the face accepts: index, OSC address, feature, side, range.
    Inputs,
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Inputs => print_inputs(),
    };
    // A reader closing the pipe early (`facegen inputs | head`) is a
    // normal way to stop, not an error.
    match result {
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        other => Ok(other?),
    }
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
