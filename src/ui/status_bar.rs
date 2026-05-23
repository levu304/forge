//! Status bar — bottom edge of the viewport.
//!
//! Displays cursor coordinates, zoom level, and version info.

use crate::ecs::resources::{CameraState, InputState};

// ---------------------------------------------------------------------------
// Pure helper functions (unit-testable without egui)
// ---------------------------------------------------------------------------

/// Format a coordinate value to 4 decimal places.
///
/// Example: `3.14159265` → `"3.1416"`.
pub fn format_coord(value: f64) -> String {
    format!("{:.4}", value)
}

/// Format a zoom factor as a percentage with 2 decimal places.
///
/// Example: `1.0` → `"100.00%"`.
pub fn format_zoom(zoom: f64) -> String {
    format!("{:.2}%", zoom * 100.0)
}

// ---------------------------------------------------------------------------
// UI draw function
// ---------------------------------------------------------------------------

/// Draw the status bar panel at the bottom of the screen.
///
/// Shows:
/// * Mouse world-space X and Y coordinates (4 decimal places).
/// * Current zoom level as a percentage.
/// * "Forge v0.1.0" right-aligned.
pub fn draw(ui: &mut egui::Ui, camera: &CameraState, input: &InputState) {
    egui::Panel::bottom("status_bar")
        .exact_size(28.0)
        .show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(format!("X: {}", format_coord(input.mouse_world.x)));
                ui.label(format!("Y: {}", format_coord(input.mouse_world.y)));
                ui.separator();
                ui.label(format!("Zoom: {}", format_zoom(camera.zoom)));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label("Forge v0.1.0");
                });
            });
        });
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- format_coord -- //

    #[test]
    fn test_format_coord_positive() {
        assert_eq!(format_coord(3.14159265), "3.1416");
    }

    #[test]
    fn test_format_coord_negative() {
        assert_eq!(format_coord(-42.0), "-42.0000");
    }

    #[test]
    fn test_format_coord_zero() {
        assert_eq!(format_coord(0.0), "0.0000");
    }

    #[test]
    fn test_format_coord_rounding() {
        // Fifth decimal is 9 → rounds up
        assert_eq!(format_coord(1.2345678), "1.2346");
    }

    #[test]
    fn test_format_coord_large_value() {
        assert_eq!(format_coord(123456.789), "123456.7890");
    }

    // -- format_zoom -- //

    #[test]
    fn test_format_zoom_100_percent() {
        assert_eq!(format_zoom(1.0), "100.00%");
    }

    #[test]
    fn test_format_zoom_zero() {
        assert_eq!(format_zoom(0.0), "0.00%");
    }

    #[test]
    fn test_format_zoom_large() {
        assert_eq!(format_zoom(100.0), "10000.00%");
    }

    #[test]
    fn test_format_zoom_half() {
        assert_eq!(format_zoom(0.5), "50.00%");
    }

    #[test]
    fn test_format_zoom_tiny() {
        assert_eq!(format_zoom(0.001), "0.10%");
    }
}
