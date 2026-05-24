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
//! | `GridConfig` | Grid visibility, spacing, and colors |
//! | `InputState` | Mouse position, button states, keyboard modifiers |
//! | `SnapConfig` | Snap engine configuration (enabled, marker size, aperture) |
//! | `SelectionConfig` | Selection highlight configuration (alpha, enabled) |

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
    /// Background clear color.
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
        // Defensive clamp: prevent division by zero if zoom is uninitialised.
        // Consistent with view_proj_matrix().
        let z = self.zoom.max(0.0001);
        let half_w = self.viewport_size.0 as f64 / 2.0;
        let half_h = self.viewport_size.1 as f64 / 2.0;

        // Normalise screen coords to [-1, 1] with Y flip
        // (screen Y+ is down, world Y+ is up).
        let ndc_x = (screen.0 as f64 / half_w) - 1.0;
        let ndc_y = 1.0 - (screen.1 as f64 / half_h);

        // Inverse of ortho projection * view translation
        Point2D::new(
            self.target.x + ndc_x * half_w / z,
            self.target.y + ndc_y * half_h / z,
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
        crate::render::camera::compute_view_proj_matrix(
            self.target,
            self.zoom,
            self.viewport_size.0,
            self.viewport_size.1,
        )
    }
}

/// Grid visualisation settings.
///
/// Controls the background grid: visibility, major/minor line spacing,
/// and per-line-type colors. The grid helps users orient themselves
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

/// Snap engine configuration.
#[derive(Debug, Clone)]
pub struct SnapConfig {
    /// Master enable/disable switch for snapping.
    pub enabled: bool,
    /// Marker size in screen pixels.
    pub marker_size: f32,
    /// Cursor magnet radius in screen pixels.
    pub aperture_size: f32,
}

impl Default for SnapConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            marker_size: 10.0,
            aperture_size: 12.0,
        }
    }
}

/// Selection engine configuration.
#[derive(Debug, Clone)]
pub struct SelectionConfig {
    /// Whether to highlight selected entities.
    pub highlight_enabled: bool,
    /// Highlight tint alpha (0.0–1.0).
    pub highlight_alpha: f32,
}

impl Default for SelectionConfig {
    fn default() -> Self {
        Self {
            highlight_enabled: true,
            highlight_alpha: 0.3,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to build a CameraState with common defaults.
    fn make_camera(
        target_x: f64,
        target_y: f64,
        zoom: f64,
        width: u32,
        height: u32,
    ) -> CameraState {
        CameraState {
            target: Point2D::new(target_x, target_y),
            zoom,
            viewport_size: (width, height),
            clear_color: Color::default(),
        }
    }

    // ------------------------------------------------------------------
    // 1. screen_to_world: screen centre → camera target
    // ------------------------------------------------------------------
    #[test]
    fn test_screen_to_world_center_is_target() {
        // Given a camera pointing at (100, 200) with zoom=1, 800×600 viewport
        let cam = make_camera(100.0, 200.0, 1.0, 800, 600);
        // Screen centre = (400, 300)  (half of 800×600)
        let world = cam.screen_to_world((400.0, 300.0));
        assert!(
            (world.x - 100.0).abs() < 0.01,
            "expected world.x ≈ 100, got {}",
            world.x,
        );
        assert!(
            (world.y - 200.0).abs() < 0.01,
            "expected world.y ≈ 200, got {}",
            world.y,
        );
    }

    // ------------------------------------------------------------------
    // 2. world_to_screen: camera target → screen centre
    // ------------------------------------------------------------------
    #[test]
    fn test_world_to_screen_center() {
        let cam = make_camera(100.0, 200.0, 1.0, 800, 600);
        let screen = cam.world_to_screen(Point2D::new(100.0, 200.0));
        assert!(
            (screen.0 - 400.0).abs() < 0.01,
            "expected screen.x ≈ 400, got {}",
            screen.0,
        );
        assert!(
            (screen.1 - 300.0).abs() < 0.01,
            "expected screen.y ≈ 300, got {}",
            screen.1,
        );
    }

    // ------------------------------------------------------------------
    // 3. Round-trip: world_to_screen(screen_to_world(p)) ≈ p
    // ------------------------------------------------------------------
    #[test]
    fn test_world_to_screen_round_trip() {
        // Config A: zoom=1.0, target=(50, 50)
        let cam_a = make_camera(50.0, 50.0, 1.0, 800, 600);
        // Config B: zoom=2.0, target=(50, 50)
        let cam_b = make_camera(50.0, 50.0, 2.0, 800, 600);
        // Config C: zoom=0.5, target=(-100, 200)
        let cam_c = make_camera(-100.0, 200.0, 0.5, 800, 600);

        for (cam, label) in &[
            (&cam_a, "zoom=1, target=(50,50)"),
            (&cam_b, "zoom=2, target=(50,50)"),
            (&cam_c, "zoom=0.5, target=(-100,200)"),
        ] {
            for world_pt in &[
                Point2D::new(0.0, 0.0),
                Point2D::new(100.0, -50.0),
                Point2D::new(-200.0, 300.0),
            ] {
                let screen = cam.world_to_screen(*world_pt);
                let round = cam.screen_to_world(screen);
                assert!(
                    (round.x - world_pt.x).abs() < 0.01,
                    "[{label}] round-trip x mismatch: input={}, got={}",
                    world_pt.x,
                    round.x,
                );
                assert!(
                    (round.y - world_pt.y).abs() < 0.01,
                    "[{label}] round-trip y mismatch: input={}, got={}",
                    world_pt.y,
                    round.y,
                );
            }
        }
    }

    // ------------------------------------------------------------------
    // 4. screen_to_world with zero viewport → default Point2D
    // ------------------------------------------------------------------
    #[test]
    fn test_screen_to_world_zero_viewport() {
        // Fully zero
        let cam = make_camera(0.0, 0.0, 1.0, 0, 0);
        let p = cam.screen_to_world((400.0, 300.0));
        assert_eq!(p, Point2D::default());

        // Width zero only
        let cam = make_camera(0.0, 0.0, 1.0, 0, 720);
        let p = cam.screen_to_world((400.0, 300.0));
        assert_eq!(p, Point2D::default());

        // Height zero only
        let cam = make_camera(0.0, 0.0, 1.0, 1280, 0);
        let p = cam.screen_to_world((400.0, 300.0));
        assert_eq!(p, Point2D::default());
    }

    // ------------------------------------------------------------------
    // 5. world_to_screen with zero viewport → (0.0, 0.0)
    // ------------------------------------------------------------------
    #[test]
    fn test_world_to_screen_zero_viewport() {
        let cam = make_camera(0.0, 0.0, 1.0, 0, 0);
        let s = cam.world_to_screen(Point2D::new(100.0, 200.0));
        assert_eq!(s, (0.0, 0.0));

        let cam = make_camera(0.0, 0.0, 1.0, 0, 720);
        let s = cam.world_to_screen(Point2D::new(100.0, 200.0));
        assert_eq!(s, (0.0, 0.0));

        let cam = make_camera(0.0, 0.0, 1.0, 1280, 0);
        let s = cam.world_to_screen(Point2D::new(100.0, 200.0));
        assert_eq!(s, (0.0, 0.0));
    }

    // ------------------------------------------------------------------
    // 6. Zoom halves the world distance from target
    // ------------------------------------------------------------------
    #[test]
    fn test_screen_to_world_with_zoom() {
        // target=(0,0), zoom=2.0, viewport=800×600
        // A screen point 100px right and 100px down from centre
        // should map to world (100/2, -100/2) = (50, -50)
        // because zoom=2 means 2 world units fit in 1 pixel, so
        // each pixel offset is half a world unit.
        let cam = make_camera(0.0, 0.0, 2.0, 800, 600);
        // Screen centre is (400, 300).  Screen (500, 400) is (+100, +100) from centre.
        // world.x = target.x + ndc_x * half_w / z
        //          = 0 + ((500/400) - 1) * 400 / 2
        //          = (1.25 - 1) * 200
        //          = 0.25 * 200 = 50
        // world.y = target.y + ndc_y * half_h / z
        //          = 0 + (1 - (400/300)) * 300 / 2
        //          = (1 - 1.333...) * 150
        //          = -0.333... * 150 = -50
        let world = cam.screen_to_world((500.0, 400.0));
        assert!(
            (world.x - 50.0).abs() < 0.01,
            "expected world.x ≈ 50, got {}",
            world.x,
        );
        assert!(
            (world.y - (-50.0)).abs() < 0.01,
            "expected world.y ≈ -50, got {}",
            world.y,
        );
    }

    // ------------------------------------------------------------------
    // 7. Negative camera target still maps centre to target
    // ------------------------------------------------------------------
    #[test]
    fn test_screen_to_world_negative_target() {
        let cam = make_camera(-100.0, -200.0, 1.0, 800, 600);
        let world = cam.screen_to_world((400.0, 300.0));
        assert!(
            (world.x - (-100.0)).abs() < 0.01,
            "expected world.x ≈ -100, got {}",
            world.x,
        );
        assert!(
            (world.y - (-200.0)).abs() < 0.01,
            "expected world.y ≈ -200, got {}",
            world.y,
        );
    }

    // ------------------------------------------------------------------
    // 8. Zoom clamping: zoom=0 behaves like zoom=0.0001
    // ------------------------------------------------------------------
    #[test]
    fn test_screen_to_world_zoom_clamping() {
        let cam_zero = make_camera(50.0, 50.0, 0.0, 800, 600);
        let cam_eps = make_camera(50.0, 50.0, 0.0001, 800, 600);

        let p_zero = cam_zero.screen_to_world((400.0, 300.0));
        let p_eps = cam_eps.screen_to_world((400.0, 300.0));

        assert!(
            (p_zero.x - p_eps.x).abs() < f64::EPSILON,
            "zoom=0 x ({}) ≠ zoom=0.0001 x ({})",
            p_zero.x,
            p_eps.x,
        );
        assert!(
            (p_zero.y - p_eps.y).abs() < f64::EPSILON,
            "zoom=0 y ({}) ≠ zoom=0.0001 y ({})",
            p_zero.y,
            p_eps.y,
        );

        // Also verify a non-centre point to exercise the zoom path
        let p_off_zero = cam_zero.screen_to_world((500.0, 400.0));
        let p_off_eps = cam_eps.screen_to_world((500.0, 400.0));
        assert!(
            (p_off_zero.x - p_off_eps.x).abs() < 0.001,
            "zoom=0 offset x mismatch",
        );
        assert!(
            (p_off_zero.y - p_off_eps.y).abs() < 0.001,
            "zoom=0 offset y mismatch",
        );
    }

    // ------------------------------------------------------------------
    // 9. Y-axis flip: world Y+ maps to screen Y-
    // ------------------------------------------------------------------
    #[test]
    fn test_world_to_screen_y_flip() {
        let cam = make_camera(0.0, 0.0, 1.0, 800, 600);

        // A point above the target in world-space should appear at a
        // *lower* screen-y coordinate (screen origin is top-left).
        let above = cam.world_to_screen(Point2D::new(0.0, 100.0));
        let target_screen = cam.world_to_screen(Point2D::new(0.0, 0.0));

        assert!(
            above.1 < target_screen.1,
            "world Y+ should give smaller screen-y: above.1={}, target.1={}",
            above.1,
            target_screen.1,
        );

        // A point below the target in world-space should appear at a
        // *higher* screen-y coordinate.
        let below = cam.world_to_screen(Point2D::new(0.0, -100.0));
        assert!(
            below.1 > target_screen.1,
            "world Y- should give larger screen-y: below.1={}, target.1={}",
            below.1,
            target_screen.1,
        );
    }
}
