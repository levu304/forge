//! ECS singleton resources.
//!
//! These types have at-most-one instance in the hecs `World` and are accessed
//! via `world.get::<T>()` / `world.insert(T)`. They represent global state
//! shared across the application: camera settings, grid configuration, and
//! current input modality.
//!
//! # Resources
//!
//! | Resource | Description |
//! |----------|-------------|
//! | `CameraState` | Orthographic camera position, zoom, viewport size |
//! | `GridConfig` | Grid visibility, spacing, and colours |
//! | `InputState` | Mouse position, button states, keyboard modifiers |

use crate::geometry::Point2D;
use crate::util::Color;

/// Orthographic camera state.
///
/// Controls the viewport's world-space centre (`target`), zoom level,
/// and physical pixel dimensions. Used for screen↔world coordinate
/// conversion and GPU view-projection matrix computation.
#[derive(Debug, Clone)]
pub struct CameraState {
    /// World-space centre of the view (the point at the screen centre).
    pub target: Point2D,
    /// Zoom factor: world units per screen pixel at 1:1.
    /// Larger values = more zoomed in.
    pub zoom: f64,
    /// Viewport dimensions in physical (non-scaled) pixels.
    pub viewport_size: (u32, u32),
    /// Background clear colour.
    pub clear_color: Color,
}

impl CameraState {
    /// Convert a screen-space pixel position (top-left origin) to
    /// world-space coordinates.
    ///
    /// `screen` is in physical (non-scaled) pixels. Uses the current
    /// zoom and target to invert the view-projection transform.
    ///
    /// Returns `Point2D::default()` (the origin) if the viewport has
    /// zero width or height.
    pub fn screen_to_world(&self, screen: (f32, f32)) -> Point2D {
        if self.viewport_size.0 == 0 || self.viewport_size.1 == 0 {
            return Point2D::default();
        }
        let half_w = self.viewport_size.0 as f64 / 2.0;
        let half_h = self.viewport_size.1 as f64 / 2.0;

        // Normalise screen coords to [-1, 1] with Y flip
        // (screen Y+ is down, world Y+ is up).
        let ndc_x = (screen.0 as f64 / half_w) - 1.0;
        let ndc_y = 1.0 - (screen.1 as f64 / half_h);

        // Inverse of ortho projection * view translation
        Point2D::new(
            self.target.x + ndc_x * half_w / self.zoom,
            self.target.y + ndc_y * half_h / self.zoom,
        )
    }

    /// Convert a world-space coordinate to screen-space pixels
    /// (top-left origin).
    ///
    /// Returns `(0.0, 0.0)` if the viewport has zero width or height.
    pub fn world_to_screen(&self, world: Point2D) -> (f32, f32) {
        if self.viewport_size.0 == 0 || self.viewport_size.1 == 0 {
            return (0.0, 0.0);
        }
        let half_w = self.viewport_size.0 as f64 / 2.0;
        let half_h = self.viewport_size.1 as f64 / 2.0;

        let dx = world.x - self.target.x;
        let dy = world.y - self.target.y;

        let sx = half_w + dx * self.zoom;
        let sy = half_h - dy * self.zoom; // flip Y

        (sx as f32, sy as f32)
    }

    /// Compute the combined view-projection matrix for GPU upload.
    ///
    /// Returns a `nalgebra::Matrix4<f32>` (column-major, matching WGSL's
    /// default matrix layout).
    ///
    /// ## Why `nalgebra::Orthographic3`
    ///
    /// This type is designed for 3D orthographic projection but is perfectly
    /// suitable for 2D when z=0. It handles near/far clipping and coordinate
    /// conventions correctly compared to manual matrix construction.
    ///
    /// ## Camera translation
    ///
    /// The orthographic bounds are computed **relative to `self.target`**:
    ///
    /// ```text
    /// left   = target.x - half_w / zoom
    /// right  = target.x + half_w / zoom
    /// top    = target.y + half_h / zoom
    /// bottom = target.y - half_h / zoom
    /// ```
    ///
    /// Because the bounds are centred on `self.target`, the projection
    /// matrix **already encodes the camera position**. No separate view
    /// translation matrix is needed — applying one would double-translate
    /// and break pan behaviour.
    ///
    /// ## Defensive clamping
    ///
    /// `self.zoom` is clamped to a minimum of `0.0001` to prevent division
    /// by zero or numerical instability when the zoom is uninitialised.
    pub fn view_proj_matrix(&self) -> nalgebra::Matrix4<f32> {
        let z = self.zoom.max(0.0001);
        let half_w = (self.viewport_size.0 as f64) / (2.0 * z);
        let half_h = (self.viewport_size.1 as f64) / (2.0 * z);

        let left = (self.target.x - half_w) as f32;
        let right = (self.target.x + half_w) as f32;
        let bottom = (self.target.y - half_h) as f32;
        let top = (self.target.y + half_h) as f32;

        let proj = nalgebra::Orthographic3::new(left, right, bottom, top, -1.0, 1.0);
        *proj.as_matrix()
    }
}

/// Grid visualisation settings.
///
/// Controls the background grid: visibility, major/minor line spacing,
/// and per-line-type colours. The grid helps users orient themselves
/// in the CAD viewport.
#[derive(Debug, Clone)]
pub struct GridConfig {
    /// Whether the grid is drawn at all.
    pub visible: bool,
    /// Spacing between major grid lines in world units (e.g. 100.0).
    pub major_spacing: f64,
    /// Spacing between minor grid lines in world units (e.g. 10.0).
    pub minor_spacing: f64,
    /// Colour of major grid lines.
    pub major_color: Color,
    /// Colour of minor grid lines.
    pub minor_color: Color,
    /// Colour of the X and Y axis lines.
    pub axis_color: Color,
}

impl Default for GridConfig {
    fn default() -> Self {
        Self {
            visible: true,
            major_spacing: 100.0,
            minor_spacing: 10.0,
            major_color: Color::GRAY_MEDIUM,
            minor_color: Color::GRAY_DARK,
            axis_color: Color::GRAY_LIGHT,
        }
    }
}

/// Snapshot of current input modality.
///
/// Updated each frame by the input mapper. Provides the UI and command
/// system with the latest mouse position, button states, and keyboard
/// modifier state.
#[derive(Debug, Clone, Default)]
pub struct InputState {
    /// Mouse cursor position in screen pixels (top-left origin).
    pub mouse_screen: (f32, f32),
    /// Mouse cursor position in world-space coordinates.
    pub mouse_world: Point2D,
    /// Is the left mouse button held down?
    pub left_down: bool,
    /// Is the middle mouse button held down? (used for pan)
    pub middle_down: bool,
    /// Is the right mouse button held down? (reserved for context menu)
    pub right_down: bool,
    /// Is Shift held?
    pub shift: bool,
    /// Is Ctrl (or Cmd on macOS) held?
    pub ctrl: bool,
    /// Is Alt held?
    pub alt: bool,
}
