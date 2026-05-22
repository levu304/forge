//! Camera control: pan (middle-drag), zoom (scroll wheel).

use crate::ecs::resources::CameraState;
use super::InputAction;

/// Applies a camera-level [`InputAction`] to a [`CameraState`].
///
/// Only [`InputAction::Pan`] and [`InputAction::Zoom`] are handled; all other
/// variants (e.g. [`InputAction::Click`], [`InputAction::Confirm`]) are silently
/// ignored, making this safe to call with any action without pre-filtering.
///
/// # Coordinate Conventions
///
/// The viewport uses a **screen-Y-down** coordinate system for input events
/// (`CursorMoved`, `MouseInput`) and a **world-Y-up** system for the camera.
/// Pan deltas therefore invert the Y component:
///
/// - `dx` is **negated**: screen-right → world-left (consistent with
///   right-handed world space where the camera looks down -Z).
/// - `dy` is **not negated** (screen-down is world-up), so a positive
///   `dy` moves the camera target upward.
///
/// # Zoom Behaviour
///
/// Zoom applies an exponential factor of `1.1^{delta}` to the current zoom
/// level. A positive `delta` zooms in, a negative `delta` zooms out. The
/// zoom level is clamped to `[0.0001, 100_000.0]` to prevent degenerate
/// states.
///
/// The camera target is adjusted toward the world-space `pivot` point so
/// that the pixel under the cursor stays fixed — this gives the expected
/// "zoom towards mouse pointer" feel.
///
/// # Pan Safety
///
/// Before dividing by `zoom` the value is floored to `0.0001` via
/// [`f64::max`]. This guards against division by zero or very small zoom
/// values that would produce extreme panning jumps.
///
/// # Side Effects
///
/// | Field | Mutated by |
/// |-------|------------|
/// | `camera.target` | `Pan` and `Zoom` |
/// | `camera.zoom`   | `Zoom` only |
pub fn apply_camera_action(camera: &mut CameraState, action: &InputAction) {
    match action {
        InputAction::Pan(dx, dy) => {
            // Defensive: prevent division by zero if zoom is uninitialized.
            let z = camera.zoom.max(0.0001);

            // Convert screen pixels to world units at current zoom
            let world_dx = -dx / z;
            let world_dy = dy / z; // screen Y is inverted
            camera.target.x += world_dx;
            camera.target.y += world_dy;
        }
        InputAction::Zoom(delta, pivot) => {
            let zoom_factor = 1.1f64.powf(*delta);
            let old_zoom = camera.zoom;
            camera.zoom = (camera.zoom * zoom_factor).clamp(0.0001, 100000.0);

            // Zoom towards mouse pointer (world pivot stays under cursor)
            let zoom_ratio = old_zoom / camera.zoom;
            camera.target.x = pivot.x + (camera.target.x - pivot.x) * zoom_ratio;
            camera.target.y = pivot.y + (camera.target.y - pivot.y) * zoom_ratio;
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use crate::ecs::resources::CameraState;
    use crate::geometry::Point2D;
    use crate::input::InputAction;
    use crate::util::Color;

    use super::apply_camera_action;

    #[test]
    fn test_pan_moves_target() {
        let mut camera = CameraState {
            target: Point2D::new(0.0, 0.0),
            zoom: 1.0,
            viewport_size: (1280, 720),
            clear_color: Color::BLACK,
        };
        apply_camera_action(&mut camera, &InputAction::Pan(100.0, 50.0));
        // Screen right → world left (dx is negated)
        assert_eq!(camera.target.x, -100.0);
        // Screen down → world up (Y inversion)
        assert_eq!(camera.target.y, 50.0);
    }

    #[test]
    fn test_zoom_zooms_toward_pivot() {
        let mut camera = CameraState {
            target: Point2D::new(10.0, 10.0),
            zoom: 1.0,
            viewport_size: (1280, 720),
            clear_color: Color::BLACK,
        };
        apply_camera_action(&mut camera, &InputAction::Zoom(1.0, Point2D::new(0.0, 0.0)));
        // zoom = 1.0 * 1.1^1
        assert!((camera.zoom - 1.1).abs() < 0.001);
        // target moves toward pivot: 10.0 / 1.1 ≈ 9.0909...
        let expected = 10.0 / 1.1;
        assert!((camera.target.x - expected).abs() < 0.001);
        assert!((camera.target.y - expected).abs() < 0.001);
    }

    #[test]
    fn test_zoom_clamps_minimum() {
        let mut camera = CameraState {
            target: Point2D::new(0.0, 0.0),
            zoom: 0.00005,
            viewport_size: (1280, 720),
            clear_color: Color::BLACK,
        };
        // Zoom out with a large negative delta
        apply_camera_action(&mut camera, &InputAction::Zoom(-10.0, Point2D::new(0.0, 0.0)));
        assert!(camera.zoom >= 0.0001);
    }

    #[test]
    fn test_pan_zero_zoom_does_not_panic() {
        let mut camera = CameraState {
            target: Point2D::new(0.0, 0.0),
            zoom: 0.0,
            viewport_size: (1280, 720),
            clear_color: Color::BLACK,
        };
        // Zoom=0 would cause division by zero without the max(0.0001) guard
        apply_camera_action(&mut camera, &InputAction::Pan(10.0, 10.0));
        // Guard ensures we don't panic; values may be extreme but stable
    }
}
