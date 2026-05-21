//! Input abstraction.
//!
//! Maps winit window events to application-level actions.
//! Handles mouse, keyboard, and camera control events.

pub mod camera_control;
pub mod command_input; // reserved — all keyboard handling is in input/mod.rs for v0.1.0
