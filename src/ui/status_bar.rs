//! Status bar — bottom edge of the viewport.
//!
//! Displays cursor coordinates, zoom level, and version info.

use crate::ecs::resources::{CameraState, InputState};

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
                ui.label(format!("X: {:.4}", input.mouse_world.x));
                ui.label(format!("Y: {:.4}", input.mouse_world.y));
                ui.separator();
                ui.label(format!("Zoom: {:.2}%", camera.zoom * 100.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label("Forge v0.1.0");
                });
            });
        });
}
