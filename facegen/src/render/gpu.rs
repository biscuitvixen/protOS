//! Device bring-up.
//!
//! On native there is no window: the instance is created without a
//! display handle and the adapter is either the one named by the caller
//! (tests ask for lavapipe by name) or wgpu's environment-driven default
//! (WGPU_ADAPTER_NAME, WGPU_BACKEND). In the browser the caller makes
//! the instance and a canvas surface and passes both in. Limits are the
//! downlevel set resolved against the adapter, because the Pi 5's v3dv
//! driver caps textures at 4096 px and wgpu's defaults ask for more.

use anyhow::{Context, anyhow};

pub struct Gpu {
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub info: wgpu::AdapterInfo,
    pub limits: wgpu::Limits,
}

impl Gpu {
    /// `adapter_name` is a case-insensitive substring of the adapter to
    /// use, such as "llvmpipe"; `None` takes the environment default.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn new(adapter_name: Option<&str>) -> anyhow::Result<Self> {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        pollster::block_on(Self::from_instance(&instance, adapter_name, None))
    }

    /// Bring up a device on `instance`, optionally one that can present
    /// to `surface`.
    pub async fn from_instance(
        instance: &wgpu::Instance,
        adapter_name: Option<&str>,
        surface: Option<&wgpu::Surface<'_>>,
    ) -> anyhow::Result<Self> {
        let adapter = match adapter_name {
            Some(want) => {
                let adapters = instance.enumerate_adapters(wgpu::Backends::all()).await;
                let names: Vec<String> = adapters.iter().map(|a| a.get_info().name).collect();
                adapters
                    .into_iter()
                    .find(|a| {
                        a.get_info()
                            .name
                            .to_lowercase()
                            .contains(&want.to_lowercase())
                    })
                    .ok_or_else(|| anyhow!("no adapter matching {want:?}; available: {names:?}"))?
            }
            None => wgpu::util::initialize_adapter_from_env_or_default(instance, surface)
                .await
                .context("no usable GPU adapter")?,
        };
        let info = adapter.get_info();
        let limits = wgpu::Limits::downlevel_defaults().using_resolution(adapter.limits());
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("facegen"),
                required_features: wgpu::Features::empty(),
                required_limits: limits.clone(),
                ..Default::default()
            })
            .await
            .with_context(|| format!("device creation failed on {}", info.name))?;
        tracing::info!(adapter = %info.name, backend = ?info.backend, driver = %info.driver_info, "gpu ready");
        Ok(Self {
            adapter,
            device,
            queue,
            info,
            limits,
        })
    }
}
