//! Selection system.
//!
//! Provides selection management (single-click, window select),
//! GPU picking (entity ID framebuffer readback), and the
//! SelectionManager which maintains a HashSet<Entity> + `Selected`
//! marker component.

pub mod picking;
pub mod window_select;
