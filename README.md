# Φ protOS

> [!IMPORTANT]
> In development

A full-stack, face tracked, protogen head software package.
Aimed at running on a Raspberry Pi 5, this operating system
consists of four independent aspects, working together to
bring yourself to life! Allowing you to control your face
with... your face.

Those being:
- A procedurally generated face, taking in standard facial blendshapes to controls.
- A face tracker.
- An eye tracker.
- And a voice modulator.

## What is a protogen?

A protogen is an anthropomorhic cyborg. Relevant to this project: it
has a visor with a digital display for a face.

## facegen

The face generator, [`facegen/`](facegen/): blendshapes in over OSC,
LED panel frames out. Built and running end to end; see its
[README](facegen/README.md) for how to run and test it.

- **The face is rendered on the GPU.** The Pi 5's VideoCore VII is
  driven through Vulkan (via [wgpu](https://wgpu.rs)), and the eye,
  mouth and nose are signed distance fields written as WGSL fragment
  shaders. Faces are shader code plus a TOML file of millimetre
  parameters; editing either reloads live. The same shaders run
  unmodified in a browser through WebGPU.
- **Every LED panel is a window onto one face-space.** All panels are
  rendered into a single atlas laid out as the LED driver's
  framebuffer, each carrying its own affine transform, so panel
  count, size, pitch and mounting are layout data. Two 64x32 panels
  and a six-panel visor are shipped presets.
- **Blendshapes drive shape parameters through a rig.** Project
  Babble's 45 shapes, the ARKit set and the protOS eye and voice
  channels are one vocabulary; a gain table in the face TOML maps
  them onto per-side parameters, so expressions can be asymmetric.
- **The mouth listens.** By default it is a scope: a thin line along
  the lip carrying the voice spectrum, bass at the snout tip and
  treble toward the corner, under a travelling sine that slows to a
  gentle wobble in silence. The 32 bands come over OSC from whatever
  is analysing the microphone; the band definition lives in
  [`protos-audio/`](protos-audio/) so every producer agrees. A jaw
  mode with lips and teeth is a TOML switch away.
- **A browser harness shows the panels live**, with a slider for every
  input, layout editing and a scene switch. A spinning cube scene
  turns each panel into a window onto a small 3D world, which is the
  start of display pieces beyond the face.
- **Panels are driven by Adafruit Piomatter** through
  [`piomatter-sys/`](piomatter-sys/), a small binding over its C++
  core, behind a cargo feature for the Pi.

See it running, rendered from the built-in curves:

- [Captures](https://biscuitvixen.github.io/protOS/) of the face and
  the cube on both presets.
- [Live demo](https://biscuitvixen.github.io/protOS/demo.html): the
  renderer compiled to WebAssembly, rendering with WebGPU in the
  browser, with every input on a slider.

## License

MIT, see [LICENSE](LICENSE). The one exception is `piomatter-sys/`,
a binding to Adafruit Piomatter, which is GPL-2.0-only; a facegen
binary built with its `piomatter` feature is GPL-2.0-only when
distributed. The Project Babble tracking model is fetched at setup
time under its own non-commercial licence and is never committed.
