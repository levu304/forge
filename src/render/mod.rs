//! wgpu render pipeline.
//!
//! Manages GPU state: surface, device, queue, shaders, and render passes.
//! Pipeline order per frame: clear → grid → entities → UI overlay.

pub mod camera;
pub mod grid;
pub mod entity_renderer;
pub mod shaders;
pub mod pipeline;   // reserved — v0.2.0+ cleanup
