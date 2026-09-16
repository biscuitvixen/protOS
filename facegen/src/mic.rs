//! Built-in microphone analysis, for running facegen with no other
//! voice producer on the bus.
//!
//! One capture thread owns the cpal stream and pushes every buffer
//! through the shared band analyser; each completed hop writes the
//! bands and the level into the input store, the same rows the OSC
//! receiver fills, so the rig cannot tell the two apart. The device's
//! own sample rate and format are used as they come: there is no
//! resampling and no gain control, so a quiet device gives quiet bands
//! and the analyser's floor is the knob. Only the first channel is
//! analysed. A device whose native format is neither f32 nor i16 is
//! rejected rather than converted, which no common capture device
//! triggers.

use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;

use anyhow::{Context, anyhow};
use cpal::SampleFormat;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use protos_audio::{BandAnalyser, Config};

use crate::contract::{InputStore, lookup_name};

/// Open the default input device, or the first whose name contains
/// `device`, and analyse it until the process ends. Returns once the
/// stream is running, or with the error that stopped it opening.
pub fn start(
    inputs: Arc<Mutex<InputStore>>,
    device: Option<String>,
) -> anyhow::Result<thread::JoinHandle<()>> {
    // The stream is not Send, so the thread that opens it keeps it;
    // the open result comes back over a channel so a bad device name
    // fails the caller instead of logging from a thread.
    let (ready, opened) = mpsc::channel();
    let handle = thread::Builder::new().name("mic".into()).spawn(move || {
        let stream = match open(inputs, device.as_deref()) {
            Ok(stream) => {
                let _ = ready.send(Ok(()));
                stream
            }
            Err(e) => {
                let _ = ready.send(Err(e));
                return;
            }
        };
        if let Err(e) = stream.play() {
            tracing::error!("mic stream failed to start: {e}");
            return;
        }
        loop {
            thread::park();
        }
    })?;
    opened
        .recv()
        .context("the mic thread ended before reporting")??;
    Ok(handle)
}

fn open(inputs: Arc<Mutex<InputStore>>, device: Option<&str>) -> anyhow::Result<cpal::Stream> {
    let host = cpal::default_host();
    let device = match device {
        Some(wanted) => host
            .input_devices()
            .context("listing input devices")?
            .find(|d| device_name(d).contains(wanted))
            .ok_or_else(|| anyhow!("no input device whose name contains {wanted:?}"))?,
        None => host
            .default_input_device()
            .ok_or_else(|| anyhow!("no default input device"))?,
    };
    let name = device_name(&device);
    let supported = device
        .default_input_config()
        .with_context(|| format!("querying the input format of {name}"))?;
    let sample_rate = supported.sample_rate() as f32;
    let channels = usize::from(supported.channels());
    let format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    tracing::info!(device = %name, sample_rate, channels, ?format, "mic capture");
    let analyser = BandAnalyser::new(Config {
        sample_rate,
        ..Config::default()
    });
    let mut sink = Sink {
        inputs,
        analyser,
        channels,
        mono: Vec::new(),
    };
    let err = |e| tracing::error!("mic stream error: {e}");
    let stream = match format {
        SampleFormat::F32 => device.build_input_stream(
            config,
            move |data: &[f32], _| sink.push(data.iter().copied()),
            err,
            None,
        ),
        SampleFormat::I16 => device.build_input_stream(
            config,
            move |data: &[i16], _| sink.push(data.iter().map(|&s| f32::from(s) / 32768.0)),
            err,
            None,
        ),
        other => return Err(anyhow!("{name} captures {other:?}, which is not supported")),
    }
    .with_context(|| format!("opening the input stream on {name}"))?;
    Ok(stream)
}

fn device_name(device: &cpal::Device) -> String {
    device
        .description()
        .map(|d| d.name().to_string())
        .unwrap_or_else(|_| "unnamed".into())
}

/// Where the capture callback delivers samples.
struct Sink {
    inputs: Arc<Mutex<InputStore>>,
    analyser: BandAnalyser,
    channels: usize,
    /// Reused per callback so the audio thread never allocates in
    /// steady state.
    mono: Vec<f32>,
}

impl Sink {
    fn push(&mut self, interleaved: impl Iterator<Item = f32>) {
        self.mono.clear();
        self.mono.extend(interleaved.step_by(self.channels.max(1)));
        if !self.analyser.push(&self.mono) {
            return;
        }
        let now = Instant::now();
        let level = lookup_name("voiceLevel").expect("contract has voiceLevel");
        let mut store = self.inputs.lock().expect("input store lock");
        store.set_bands(&self.analyser.bands(), now);
        store.set(level, self.analyser.level(), now);
    }
}
