//! Toolbar — narrow left side panel.
//!
//! Icon button placeholders for CAD primitives (LINE, CIRCLE, ARC).
//! Full implementation deferred to v0.2.0+.

use crate::commands::CommandState;

/// Draw the toolbar panel on the left side of the viewport.
///
/// v0.1.0 stub — displays unicode icon placeholders only.
/// Command shortcuts will be added in v0.2.0+.
pub fn draw(ui: &mut egui::Ui, _cmd_state: &mut CommandState) {
    egui::Panel::left("toolbar")
        .resizable(false)
        .default_size(40.0)
        .show_inside(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(8.0);
                ui.label("🔲");
                ui.add_space(4.0);
                ui.label("🔵");
                ui.add_space(4.0);
                ui.label("📐");
            });
        });
}
