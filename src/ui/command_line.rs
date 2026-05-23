//! Command line — text input panel above the status bar.
//!
//! egui `text_edit_singleline` for entering CAD commands.
//! Submits to [`CommandState`] on Enter.

use crate::commands::CommandState;

/// Draw the command line panel immediately above the status bar.
///
/// Shows:
/// * A prompt string from the active command (or "Type command:").
/// * A red error message if the last command produced an error.
/// * A single-line text input bound to [`CommandState::buffer`].
///
/// On Enter the buffer is stored in [`CommandState::pending_dispatch`]
/// and cleared so the event-loop handler can dispatch it.
pub fn draw(ui: &mut egui::Ui, cmd_state: &mut CommandState) {
    egui::Panel::bottom("command_line")
        .exact_size(32.0)
        .show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                let prompt = cmd_state
                    .active
                    .as_ref()
                    .map(|c| c.prompt())
                    .unwrap_or_else(|| "Type command:".to_string());
                ui.label(prompt);

                // Show last command error in red if present
                if let Some(ref err) = cmd_state.last_error {
                    ui.colored_label(egui::Color32::RED, err);
                }

                let response = ui.text_edit_singleline(&mut cmd_state.buffer);

                // Detect Enter keypress.
                // text_edit_singleline retains focus on Enter, so lost_focus()
                // alone won't trigger. Check key_pressed independently too.
                let submit_enter = response.lost_focus()
                    && ui.input(|i| i.key_pressed(egui::Key::Enter));
                let submit_focused = response.has_focus()
                    && ui.input(|i| i.key_pressed(egui::Key::Enter));

                if submit_enter || submit_focused {
                    let text = cmd_state.buffer.trim().to_string();
                    if !text.is_empty() {
                        cmd_state.history.push(text.clone());
                        cmd_state.pending_dispatch = Some(text);
                        cmd_state.buffer.clear();
                    }
                }

                if response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    cmd_state.buffer.clear();
                }
            });
        });
}
