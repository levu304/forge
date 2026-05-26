//! WGSL shader modules.
//!
//! Shaders are loaded at runtime from the files in this directory.
//!
//! | Shader             | Used by                        | Purpose                                      |
//! |--------------------|--------------------------------|----------------------------------------------|
//! | `shared.wgsl`      | All renderers                  | Camera uniform bind group (view-proj mat4x4) |
//! | `grid.wgsl`        | `GridRenderer`                 | Grid lines (major/minor/axes)                |
//! | `entity.wgsl`      | `EntityRenderer`               | Geometry entities (with selection tint)      |
//! | `window_select.wgsl` | `WindowSelectRenderer`       | Translucent selection rectangle              |
//! | `highlight.wgsl`   | `SelectionRenderer` (v0.2.0+) | Bounding-box overlay (placeholder)           |
//! | `picking.wgsl`     | `PickingPass`                  | Entity ID → u32 framebuffer                  |
//! | `snap_marker.wgsl` | `SnapMarkerRenderer`           | Yellow screen-space snap markers             |
