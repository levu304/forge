//! Toolbar — narrow left side panel.
//!
//! Draw and modify command buttons: LINE, CIRCLE, ARC, PLINE,
//! and modify commands (ERASE, MOVE, COPY, ROTATE, SCALE,
//! MIRROR, OFFSET).

use crate::commands::{CommandState, PendingModifyCommand};

const BUTTON_SPACING: f32 = 4.0;

/// Draw the toolbar panel on the left side of the viewport.
///
/// Clicking a button sets [`CommandState::pending_dispatch`] (for draw
/// commands) or [`CommandState::pending_modify_command`] (for modify
/// commands), each consumed by the event-loop handler to start the command.
pub fn draw(ui: &mut egui::Ui, cmd_state: &mut CommandState) {
    egui::Panel::left("toolbar")
        .resizable(false)
        .default_size(48.0)
        .show_inside(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(8.0);

                // ── Draw commands ────────────────────────────────────────
                for &name in &["LINE", "CIRCLE", "ARC", "PLINE"] {
                    if ui.button(name).clicked() {
                        cmd_state.pending_dispatch = Some(name.into());
                    }
                    ui.add_space(BUTTON_SPACING);
                }

                ui.separator();
                ui.add_space(BUTTON_SPACING);

                // ── Modify commands ──────────────────────────────────────
                // NOTE: These bypass the text parser entirely; they set
                // `pending_modify_command` which is consumed by the event
                // loop to construct the concrete command with access to
                // `SelectionManager` (forge-51x).
                const MODIFY_COMMANDS: &[(&str, PendingModifyCommand)] = &[
                    ("ERASE", PendingModifyCommand::Erase),
                    ("MOVE", PendingModifyCommand::Move),
                    ("COPY", PendingModifyCommand::Copy),
                    ("ROTATE", PendingModifyCommand::Rotate),
                    ("SCALE", PendingModifyCommand::Scale),
                    ("MIRROR", PendingModifyCommand::Mirror),
                    ("OFFSET", PendingModifyCommand::Offset),
                ];
                for &(label, cmd) in MODIFY_COMMANDS {
                    if ui.button(label).clicked() {
                        cmd_state.pending_modify_command = Some(cmd);
                    }
                    ui.add_space(BUTTON_SPACING);
                }
            });
        });
}
