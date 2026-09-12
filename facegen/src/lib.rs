//! Procedural protogen face renderer for protOS.
//!
//! Named float inputs (face blendshapes, eye gaze, voice level) come in
//! over OSC, a rig turns them into per-feature shader parameters, and
//! the GPU renders every LED panel into one atlas that is read back and
//! pushed to the panels. Each module is one area of that pipeline.

#[cfg(not(target_arch = "wasm32"))]
pub mod app;
#[cfg(not(target_arch = "wasm32"))]
pub mod capture;
pub mod contract;
pub mod face;
pub mod fake;
pub mod features;
pub mod layout;
#[cfg(not(target_arch = "wasm32"))]
pub mod osc;
pub mod preview;
pub mod render;
pub mod rig;
pub mod sinks;
#[cfg(not(target_arch = "wasm32"))]
pub mod watch;
#[cfg(not(target_arch = "wasm32"))]
pub mod web;
