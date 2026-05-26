//! Toolbar — narrow left side panel.
//!
//! Icon button placeholders for CAD primitives (LINE, CIRCLE, ARC, PLINE)
//! and modify commands (ERASE, MOVE, COPY, ROTATE, SCALE, MIRROR, OFFSET).

use crate::commands::CommandState;

/// Draw the toolbar panel on the left side of the viewport.
///
/// Buttons dispatch commands via `cmd_state.pending_dispatch`.
pub fn draw(ui: &mut egui::Ui, cmd_state: &mut CommandState) {
    egui::Panel::left("toolbar")
        .resizable(false)
        .default_size(40.0)
        .show_inside(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(8.0);

                // ── Draw commands ────────────────────────────────────────
                if ui.button("LINE").clicked() {
                    cmd_state.pending_dispatch = Some("LINE".into());
                }
                ui.add_space(4.0);
                if ui.button("CIRC").clicked() {
                    cmd_state.pending_dispatch = Some("CIRCLE".into());
                }
                ui.add_space(4.0);
                if ui.button("ARC").clicked() {
                    cmd_state.pending_dispatch = Some("ARC".into());
                }
                ui.add_space(4.0);
                if ui.button("PLNE").clicked() {
                    cmd_state.pending_dispatch = Some("PLINE".into());
                }

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(8.0);

                // ── Modify commands ──────────────────────────────────────
                if ui.button("ERASE").clicked() {
                    cmd_state.pending_dispatch = Some("ERASE".into());
                }
                ui.add_space(4.0);
                if ui.button("MOVE").clicked() {
                    cmd_state.pending_dispatch = Some("MOVE".into());
                }
                ui.add_space(4.0);
                if ui.button("COPY").clicked() {
                    cmd_state.pending_dispatch = Some("COPY".into());
                }
                ui.add_space(4.0);
                if ui.button("ROT").clicked() {
                    cmd_state.pending_dispatch = Some("ROTATE".into());
                }
                ui.add_space(4.0);
                if ui.button("SCALE").clicked() {
                    cmd_state.pending_dispatch = Some("SCALE".into());
                }
                ui.add_space(4.0);
                if ui.button("MIRR").clicked() {
                    cmd_state.pending_dispatch = Some("MIRROR".into());
                }
                ui.add_space(4.0);
                if ui.button("OFFS").clicked() {
                    cmd_state.pending_dispatch = Some("OFFSET".into());
                }
            });
        });
}
