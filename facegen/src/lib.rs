//! Procedural protogen face renderer for protOS.
//!
//! Named float inputs (face blendshapes, eye gaze, voice level) come in
//! over OSC, a rig turns them into per-feature shader parameters, and
//! the GPU renders every LED panel into one atlas that is read back and
//! pushed to the panels. Each module is one area of that pipeline.

pub mod app;
pub mod contract;
pub mod face;
pub mod features;
pub mod layout;
pub mod render;
pub mod rig;
pub mod sinks;
pub mod web;
