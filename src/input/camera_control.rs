//! Camera control: pan (middle-drag), zoom (scroll wheel).

use crate::ecs::resources::CameraState;
use super::InputAction;

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
