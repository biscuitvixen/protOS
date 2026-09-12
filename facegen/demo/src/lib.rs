//! facegen in the browser: the renderer, the rig and the shaders as
//! they are, on WebGPU, presented to a canvas through the present pass.
//! No OSC, no readback; inputs come from JavaScript and frames go to
//! the screen. The exported functions are the whole API the page uses.

use facegen::contract::{self, INPUT_COUNT, InputStore};
use facegen::face::{Face, FrameState, fit_scale};
use facegen::fake;
use facegen::layout::atlas::Atlas;
use facegen::layout::{Layout, presets};
use facegen::render::gpu::Gpu;
use facegen::render::present::{PresentOptions, PresentPass};
use facegen::render::{Renderer, Scene, shader};
use facegen::rig::Rig;
use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;

fn js_err(e: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&e.to_string())
}

#[wasm_bindgen]
pub struct Demo {
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    renderer: Renderer,
    present: PresentPass,
    layout: Layout,
    atlas: Atlas,
    face: Face,
    rig: Rig,
    store: InputStore,
    state: FrameState,
    last_ms: Option<f64>,
    fake: bool,
    options: PresentOptions,
}

/// Bring up WebGPU on `canvas` and build the demo for a layout preset.
#[wasm_bindgen]
pub async fn start(canvas: HtmlCanvasElement, preset: &str) -> Result<Demo, JsValue> {
    console_error_panic_hook::set_once();
    let layout = presets::load(preset).ok_or_else(|| js_err(format!("no preset {preset:?}")))?;
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let surface = instance
        .create_surface(wgpu::SurfaceTarget::Canvas(canvas.clone()))
        .map_err(js_err)?;
    let gpu = Gpu::from_instance(&instance, None, Some(&surface))
        .await
        .map_err(js_err)?;
    let caps = surface.get_capabilities(&gpu.adapter);
    let format = caps
        .formats
        .first()
        .copied()
        .ok_or_else(|| js_err("surface has no formats"))?;
    let mut config = surface
        .get_default_config(&gpu.adapter, canvas.width().max(1), canvas.height().max(1))
        .ok_or_else(|| js_err("surface is not supported by the adapter"))?;
    config.format = format;
    surface.configure(&gpu.device, &config);
    let renderer = Renderer::new(gpu, &layout, &shader::face_source()).map_err(js_err)?;
    let present =
        PresentPass::new(&renderer.gpu().device, format, renderer.atlas_view()).map_err(js_err)?;
    let atlas = renderer.atlas().clone();
    let face = Face::default_face();
    let rig = Rig::new(&face).map_err(js_err)?;
    let state = FrameState {
        face_scale: fit_scale(&layout, face.box_mm),
        ..Default::default()
    };
    Ok(Demo {
        surface,
        config,
        renderer,
        present,
        layout,
        atlas,
        face,
        rig,
        store: InputStore::new(),
        state,
        last_ms: None,
        fake: false,
        options: PresentOptions {
            px_per_mm: 0.0,
            apply_gamma: true,
            led_mask: true,
            srgb_encode: !format.is_srgb(),
        },
    })
}

#[wasm_bindgen]
impl Demo {
    /// Render one frame; call from requestAnimationFrame with its
    /// timestamp in milliseconds.
    pub fn frame(&mut self, time_ms: f64) -> Result<(), JsValue> {
        let t = (time_ms / 1000.0) as f32;
        let dt = self
            .last_ms
            .map_or(0.0, |last| ((time_ms - last) / 1000.0) as f32);
        self.last_ms = Some(time_ms);
        self.state.dt_s = dt;
        self.state.time_s = t;
        self.state.frame = self.state.frame.wrapping_add(1);
        if self.fake {
            let now = web_time::Instant::now();
            for (address, value) in fake::curves(t) {
                self.store.set_by_address(address, value, now);
            }
        }
        let raw: [f32; INPUT_COUNT] = *self.store.values();
        self.rig.update(&self.face, &raw, dt);
        let uniforms = self.rig.pack(&self.face, &self.state);
        use wgpu::CurrentSurfaceTexture as Current;
        let frame = match self.surface.get_current_texture() {
            Current::Success(f) | Current::Suboptimal(f) => f,
            Current::Timeout | Current::Occluded => return Ok(()),
            Current::Outdated | Current::Lost => {
                self.surface
                    .configure(&self.renderer.gpu().device, &self.config);
                return Ok(());
            }
            Current::Validation => return Err(js_err("surface validation error")),
        };
        let view = frame.texture.create_view(&Default::default());
        let mut encoder = self
            .renderer
            .gpu()
            .device
            .create_command_encoder(&Default::default());
        self.renderer.draw_atlas(&uniforms, &mut encoder);
        self.present.draw(
            &self.renderer.gpu().queue,
            &mut encoder,
            &view,
            (self.config.width, self.config.height),
            &self.layout,
            &self.atlas,
            &self.options,
        );
        self.renderer.gpu().queue.submit([encoder.finish()]);
        self.renderer.gpu().queue.present(frame);
        Ok(())
    }

    /// Match the canvas's pixel size after a resize.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.config.width = width.max(1);
        self.config.height = height.max(1);
        self.surface
            .configure(&self.renderer.gpu().device, &self.config);
    }

    pub fn set_input(&mut self, name: &str, value: f32) -> bool {
        match contract::lookup_name(name) {
            Some(id) => {
                self.store.set(id, value, web_time::Instant::now());
                true
            }
            None => false,
        }
    }

    pub fn reset_inputs(&mut self) {
        self.store.reset();
    }

    /// Drive the inputs from the built-in curves instead of the sliders.
    pub fn set_fake(&mut self, on: bool) {
        self.fake = on;
        if !on {
            self.store.reset();
        }
    }

    pub fn set_options(&mut self, gamma: bool, led_mask: bool) {
        self.options.apply_gamma = gamma;
        self.options.led_mask = led_mask;
    }

    pub fn set_scene(&mut self, name: &str) -> bool {
        match Scene::parse(name) {
            Some(scene) => {
                self.renderer.set_scene(scene);
                true
            }
            None => false,
        }
    }

    pub fn set_layout(&mut self, preset: &str) -> Result<(), JsValue> {
        let layout =
            presets::load(preset).ok_or_else(|| js_err(format!("no preset {preset:?}")))?;
        self.renderer
            .rebuild(&layout, &shader::face_source())
            .map_err(js_err)?;
        self.present
            .rebind(&self.renderer.gpu().device, self.renderer.atlas_view());
        self.atlas = self.renderer.atlas().clone();
        self.state.face_scale = fit_scale(&layout, self.face.box_mm);
        self.layout = layout;
        Ok(())
    }

    /// The input vocabulary with current values, for building sliders.
    pub fn inputs_json(&self) -> String {
        let list: Vec<serde_json::Value> = contract::all_ids()
            .map(|id| {
                let s = id.spec();
                serde_json::json!({
                    "name": s.name,
                    "feature": format!("{:?}", s.feature),
                    "side": format!("{:?}", s.side),
                    "signed": s.range == contract::Range::Signed,
                    "value": self.store.get(id),
                })
            })
            .collect();
        serde_json::to_string(&list).expect("serialises")
    }
}

#[wasm_bindgen]
pub fn presets_json() -> String {
    serde_json::to_string(&presets::NAMES).expect("serialises")
}

#[wasm_bindgen]
pub fn scenes_json() -> String {
    serde_json::to_string(&Scene::NAMES).expect("serialises")
}
