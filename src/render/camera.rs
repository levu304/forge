//! Orthographic camera for 2D viewport.
//!
//! Builds view-projection matrices from target position and zoom.
//! World Y+ is up; screen Y+ is down (Y-flip in projection).
//!
//! ## Design Note (v0.1.0)
//! `OrthographicCamera` duplicates `CameraState` from `ecs::resources`.
//! In v0.2.0+, consolidate into a single camera type. For v0.1.0 both are
//! kept separate — `CameraState` is the ECS resource (with coordinate
//! conversion helpers), while `OrthographicCamera` is the render-side
//! projection helper.

use crate::geometry::Point2D;

/// Compute the combined view-projection matrix from camera parameters.
///
/// This is the canonical implementation shared by `OrthographicCamera` and
/// `CameraState`. The bounds are centred on `target` so the projection
/// matrix encodes the camera position — no separate view translation
/// matrix is needed.
///
/// ## Defensive clamping
///
/// `zoom` is clamped to a minimum of `0.0001` to prevent division by zero
/// or numerical instability when zoom is uninitialised.
pub(crate) fn compute_view_proj_matrix(
    target: Point2D,
    zoom: f64,
    viewport_width: u32,
    viewport_height: u32,
) -> nalgebra::Matrix4<f32> {
    let z = zoom.max(0.0001);
    let half_w = (viewport_width as f64) / (2.0 * z);
    let half_h = (viewport_height as f64) / (2.0 * z);

    let left = (target.x - half_w) as f32;
    let right = (target.x + half_w) as f32;
    let bottom = (target.y - half_h) as f32;
    let top = (target.y + half_h) as f32;

    let proj = nalgebra::Orthographic3::new(left, right, bottom, top, -1.0, 1.0);
    *proj.as_matrix()
}

/// Orthographic camera for the 2D viewport.
///
/// Encapsulates the camera's world-space position, zoom level, and
/// viewport dimensions. Provides a single method to build the combined
/// view-projection matrix for GPU upload.
///
/// ## Camera translation
///
/// The projection bounds are computed **relative to `self.target`**:
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
pub struct OrthographicCamera {
    /// World-space centre of the view.
    pub target: Point2D,
    /// Zoom factor: world units per screen pixel at 1:1.
    pub zoom: f64,
    /// Viewport width in physical pixels.
    pub viewport_width: f32,
    /// Viewport height in physical pixels.
    pub viewport_height: f32,
}

impl OrthographicCamera {
    /// Compute the view-projection matrix for wgpu.
    ///
    /// Uses `nalgebra::Orthographic3` which is designed for 3D orthographic
    /// projection but perfectly suitable for 2D when z=0. It handles near/far
    /// clipping and coordinate conventions correctly compared to manual matrix
    /// construction.
    ///
    /// ## Defensive clamping
    ///
    /// `self.zoom` is clamped to a minimum of `0.0001` to prevent division
    /// by zero or numerical instability when zoom is uninitialised.
    pub fn build_view_projection_matrix(&self) -> nalgebra::Matrix4<f32> {
        compute_view_proj_matrix(
            self.target,
            self.zoom,
            self.viewport_width as u32,
            self.viewport_height as u32,
        )
    }
}
