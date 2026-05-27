//! Toolbar — narrow left side panel.
//!
//! Draw and modify command buttons: LINE, CIRCLE, ARC, PLINE,
//! and modify commands (ERASE, MOVE, COPY, ROTATE, SCALE,
//! MIRROR, OFFSET).

use crate::commands::{CommandState, PendingModifyCommand};

/// Draw the toolbar panel on the left side of the viewport.
///
/// Clicking a button sets [`CommandState::pending_dispatch`] which
/// is consumed by the event-loop handler to start the command.
pub fn draw(ui: &mut egui::Ui, cmd_state: &mut CommandState) {
    egui::Panel::left("toolbar")
        .resizable(false)
        .default_size(48.0)
        .show_inside(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(8.0);

                // ── Draw commands ────────────────────────────────────────
                if ui.button("LINE").clicked() {
                    cmd_state.pending_dispatch = Some("LINE".into());
                }
                ui.add_space(4.0);

                if ui.button("CIRCLE").clicked() {
                    cmd_state.pending_dispatch = Some("CIRCLE".into());
                }
                ui.add_space(4.0);

                if ui.button("ARC").clicked() {
                    cmd_state.pending_dispatch = Some("ARC".into());
                }
                ui.add_space(4.0);

                if ui.button("PLINE").clicked() {
                    cmd_state.pending_dispatch = Some("PLINE".into());
                }

                ui.separator();
                ui.add_space(4.0);

                // ── Modify commands ──────────────────────────────────────
                // NOTE: These bypass the text parser entirely; they set
                // `pending_modify_command` which is consumed by the event
                // loop to construct the concrete command with access to
                // `SelectionManager`.  (forge-51x: use same enum).
                if ui.button("ERASE").clicked() {
                    cmd_state.pending_modify_command = Some(PendingModifyCommand::Erase);
                }
                ui.add_space(4.0);

                if ui.button("MOVE").clicked() {
                    cmd_state.pending_modify_command = Some(PendingModifyCommand::Move);
                }
                ui.add_space(4.0);

                if ui.button("COPY").clicked() {
                    cmd_state.pending_modify_command = Some(PendingModifyCommand::Copy);
                }
                ui.add_space(4.0);

                if ui.button("ROTATE").clicked() {
                    cmd_state.pending_modify_command = Some(PendingModifyCommand::Rotate);
                }
                ui.add_space(4.0);

                if ui.button("SCALE").clicked() {
                    cmd_state.pending_modify_command = Some(PendingModifyCommand::Scale);
                }
                ui.add_space(4.0);

                if ui.button("MIRROR").clicked() {
                    cmd_state.pending_modify_command = Some(PendingModifyCommand::Mirror);
                }
                ui.add_space(4.0);

                if ui.button("OFFSET").clicked() {
                    cmd_state.pending_modify_command = Some(PendingModifyCommand::Offset);
                }
            });
        });
}
