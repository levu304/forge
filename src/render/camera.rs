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

    // Guard against zero-viewport: if either dimension is zero, the
    // orthographic projection would have left==right or bottom==top,
    // which nalgebra::Orthographic3 rejects with a panic. We fall back
    // to a 1x1 viewport in that case so the matrix remains valid.
    let vw = if viewport_width == 0 { 1 } else { viewport_width };
    let vh = if viewport_height == 0 { 1 } else { viewport_height };

    let half_w = (vw as f64) / (2.0 * z);
    let half_h = (vh as f64) / (2.0 * z);

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Point2D;

    /// Check that all entries in the 4×4 matrix are finite (not NaN, not infinite).
    fn assert_matrix_finite(m: &nalgebra::Matrix4<f32>) {
        for i in 0..4 {
            for j in 0..4 {
                assert!(
                    m[(i, j)].is_finite(),
                    "entry [{}][{}] = {} is not finite",
                    i,
                    j,
                    m[(i, j)]
                );
            }
        }
    }

    #[test]
    fn test_compute_view_proj_valid() {
        let m = compute_view_proj_matrix(Point2D::new(0.0, 0.0), 1.0, 1280, 720);
        assert_matrix_finite(&m);
        // Determinant must be non-zero (invertible projection).
        assert_ne!(m.determinant(), 0.0);
    }

    #[test]
    fn test_compute_view_proj_zoomed_in() {
        let m = compute_view_proj_matrix(Point2D::new(0.0, 0.0), 2.0, 1280, 720);
        assert_matrix_finite(&m);
        assert_ne!(m.determinant(), 0.0);
    }

    #[test]
    fn test_compute_view_proj_zoomed_out() {
        let m = compute_view_proj_matrix(Point2D::new(0.0, 0.0), 0.5, 1280, 720);
        assert_matrix_finite(&m);
        assert_ne!(m.determinant(), 0.0);
    }

    #[test]
    fn test_compute_view_proj_negative_target() {
        let m = compute_view_proj_matrix(Point2D::new(-500.0, -300.0), 1.0, 1280, 720);
        assert_matrix_finite(&m);
        assert_ne!(m.determinant(), 0.0);
    }

    #[test]
    fn test_compute_view_proj_zero_viewport() {
        // Zero viewport is clamped to 1x1 — matrix should be fully valid.
        let m = compute_view_proj_matrix(Point2D::new(0.0, 0.0), 1.0, 0, 0);
        assert_matrix_finite(&m);
        assert_ne!(m.determinant(), 0.0);
    }

    #[test]
    fn test_compute_view_proj_zero_zoom_clamped() {
        // zoom=0 is clamped to 0.0001, so this should produce the same
        // matrix as explicitly passing 0.0001.
        let m_zero = compute_view_proj_matrix(Point2D::new(10.0, 20.0), 0.0, 800, 600);
        let m_clamped = compute_view_proj_matrix(Point2D::new(10.0, 20.0), 0.0001, 800, 600);
        assert_eq!(m_zero, m_clamped);
    }

    #[test]
    fn test_compute_view_proj_negative_zoom_clamped() {
        // Negative zoom should also be clamped to 0.0001.
        let m_neg = compute_view_proj_matrix(Point2D::new(0.0, 0.0), -5.0, 100, 100);
        let m_clamped = compute_view_proj_matrix(Point2D::new(0.0, 0.0), 0.0001, 100, 100);
        assert_eq!(m_neg, m_clamped);
    }

    #[test]
    fn test_compute_view_proj_large_coordinates() {
        // Very large world coordinates should not cause numerical blowup.
        let m = compute_view_proj_matrix(Point2D::new(1e6, -1e6), 1.0, 1920, 1080);
        assert_matrix_finite(&m);
        assert_ne!(m.determinant(), 0.0);
    }

    #[test]
    fn test_orthographic_camera_delegates() {
        let cam = OrthographicCamera {
            target: Point2D::new(5.0, -3.0),
            zoom: 2.5,
            viewport_width: 1024.0,
            viewport_height: 768.0,
        };
        let m = cam.build_view_projection_matrix();
        assert_matrix_finite(&m);
        assert_ne!(m.determinant(), 0.0);

        // Must match the free function with the same parameters.
        let expected = compute_view_proj_matrix(
            Point2D::new(5.0, -3.0),
            2.5,
            1024,
            768,
        );
        assert_eq!(m, expected);
    }

    #[test]
    fn test_orthographic_camera_zero_viewport() {
        let cam = OrthographicCamera {
            target: Point2D::new(0.0, 0.0),
            zoom: 1.0,
            viewport_width: 0.0,
            viewport_height: 0.0,
        };
        let m = cam.build_view_projection_matrix();
        assert_matrix_finite(&m);
    }

    #[test]
    fn test_orthographic_camera_zero_zoom() {
        let cam = OrthographicCamera {
            target: Point2D::new(0.0, 0.0),
            zoom: 0.0,
            viewport_width: 1280.0,
            viewport_height: 720.0,
        };
        let m = cam.build_view_projection_matrix();
        assert_matrix_finite(&m);

        // Must match the explicitly-clamped free function.
        let expected = compute_view_proj_matrix(Point2D::new(0.0, 0.0), 0.0001, 1280, 720);
        assert_eq!(m, expected);
    }

    #[test]
    fn test_tiny_viewport_finite() {
        // Very small but non-zero viewport should still produce a valid matrix.
        let m = compute_view_proj_matrix(Point2D::new(0.0, 0.0), 1.0, 1, 1);
        assert_matrix_finite(&m);
    }
}
