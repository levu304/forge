//! Picking pass renderer.
//!
//! Offscreen ID-encoded framebuffer for GPU pixel-perfect picking.
//! Renders entity instance indices as flat u32 colours; the host CPU
//! reads back the pixel under the cursor on the next frame.
//!
//! The primary implementation lives in [`crate::selection::picking::PickingPass`].
//! This module re-exports it for convenience when accessed from the render layer.
//!
//! # Usage
//!
//! ```ignore
//! use forge::render::picking_renderer::PickingPass;
//! ```
//!
//! The `PickingPass` is wired into `RenderState` as `picking_pass: Option<PickingPass>`
//! and is integrated into the render loop in `ForgeApp::render()`.

pub use crate::selection::picking::PickingPass;
