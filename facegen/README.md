# facegen

Procedural face generation for protOS. Face blendshapes come in over
OSC and LED panel frames go out, rendered on the GPU.

The face is not a set of bitmaps. Each feature (eye, mouth, nose) is a
signed distance field in a WGSL fragment shader, parameterised in
millimetres: an eye is a bent ellipse with a centre, radii, openness
and gaze; a mouth is, by default, a thin line along the lip that draws
the voice spectrum like an oscilloscope, bass at the snout tip and
treble toward the corner under a travelling sine, or in jaw mode the
region between two lip curves with an opening and a sawtooth of teeth.
Every input is a named float in [0, 1] (Project Babble's 45
blendshapes, the ARKit set, the protOS eye, voice level and 32 voice
band channels). A rig maps them through a gain table
in the face TOML onto those parameters, separately for each side so
expressions can be asymmetric. All panels are rendered into one atlas
laid out as the LED driver's framebuffer, with each panel carrying its
own affine into face-space and the list of features it draws, so panel
count, size, pitch, mounting and which panel shows the mouth are
layout data rather than code. A browser page shows the same frames
live.

## Layout

- `src/contract.rs` the input vocabulary and OSC addresses
- `src/layout.rs` panels, face-space transforms, presets, atlas packing
- `src/rig.rs` blendshapes to shape parameters
- `src/render/` wgpu device, atlas target, passes, uniform block
- `src/web/` the browser harness
- `src/osc.rs`, `src/fake.rs` the bus receiver and a stand-in producer
- `shaders/` WGSL, one file per feature; `faces/` face TOML;
  `layouts/` panel presets
- `demo/` the browser build (wasm-bindgen entry over the same library)

## Run

From the workspace root, so shader and face edits reload live:

```
cargo run -p facegen -- serve --bind 0.0.0.0:8081
cargo run -p facegen -- fake
```

Open the printed link. The page has sliders for every input, a preset
and scene selector, and an editable panel table. `serve` listens for
OSC on 127.0.0.1:8888, Babble's default output port. Other commands:

```
cargo run -p facegen -- inputs                 # the input table
cargo run -p facegen -- layout six_panel       # transforms and atlas
cargo run -p facegen -- render --mouth-mode jaw --set jawOpen=1 --out open.png
cargo run -p facegen -- render --set voiceLevel=1 --set voiceBand18=1 --out scope.png
cargo run -p facegen -- render --scene cube --time 2.1 --out cube.png
```

The voice bands are 32 log-spaced energies from 80 Hz to 8 kHz, defined
by the `protos-audio` crate and sent as one OSC message on
`/protos/voice/bands` (or one float per `/protos/voice/band/N`). A
producer that stops sending is treated as silent after a second.

`--adapter llvmpipe` forces Mesa's software Vulkan on any command.

## Test

```
cargo test
```

Unit tests cover the contract, layout maths, rig, OSC parsing, shader
validation and the watcher. The golden tests render each preset on
lavapipe and compare against `tests/golden/*.png`; after an intended
visual change regenerate them with `FACEGEN_UPDATE_GOLDENS=1 cargo
test` and commit the PNGs. Goldens are pinned to lavapipe because
output is byte-stable on one driver, not across drivers.

## Browser demo

`facegen/demo/` compiles the renderer, rig and shaders to WebAssembly
and draws through WebGPU, presenting the atlas as the visor view. To
try it locally:

```
rustup target add wasm32-unknown-unknown
cargo install wasm-pack
wasm-pack build facegen/demo --target web --release --out-dir ../../pages/pkg --out-name facegen_web
python3 -m http.server -d pages 8090
```

Then open `http://localhost:8090/demo.html` in a browser with WebGPU
(Chrome, Edge, Safari; Firefox on Linux needs a flag). The Pages
workflow builds the same package and publishes it with the captures.

## On the Pi 5

Raspberry Pi OS Trixie Lite, no desktop needed:

```
sudo apt install mesa-vulkan-drivers libvulkan1 vulkan-tools
sudo usermod -aG render,gpio $USER
echo 'SUBSYSTEM=="*-pio", GROUP="gpio", MODE="0660"' | sudo tee /etc/udev/rules.d/99-pio.rules
sudo udevadm control --reload-rules && sudo udevadm trigger
cargo build --release -p facegen --features piomatter
target/release/facegen serve --sink piomatter --bind 0.0.0.0:8081
```

The `piomatter` feature links Adafruit Piomatter (GPL-2.0-only) from
`../piomatter-sys`, so a binary built with it is GPL-2.0-only when
distributed. All panels on one Pi must share a scan depth; the layout
validator says so if they do not.
