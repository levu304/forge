//! Command line — text input panel above the status bar.
//!
//! egui `text_edit_singleline` for entering CAD commands.
//! Submits to [`CommandState`] on Enter.

use crate::commands::{Command, CommandState};

/// Maximum number of commands to retain in history.
/// Prevents unbounded memory growth over long CAD sessions.
const MAX_HISTORY: usize = 1000;

// ---------------------------------------------------------------------------
// Pure helper functions (unit-testable without egui)
// ---------------------------------------------------------------------------

/// Resolve the prompt string to display in the command line.
///
/// Returns the active command's prompt, or `"Type command:"` if no
/// command is active.
pub fn resolve_prompt(active: Option<&Box<dyn Command>>) -> String {
    active
        .map(|c| c.prompt())
        .unwrap_or_else(|| "Type command:".to_string())
}

/// Determine whether the command-line text should be submitted.
///
/// Submission occurs when the text field has focus **and** the Enter
/// key was pressed.
pub fn should_submit(has_focus: bool, enter_pressed: bool) -> bool {
    has_focus && enter_pressed
}

/// Determine whether the Escape key was pressed with focus.
///
/// When `true` the caller should clear the input buffer and, if a
/// command is active, set [`CommandState::cancel_requested`].
///
/// The `_active` parameter is accepted for interface completeness
/// but does not affect the condition — Escape is always processed
/// when focus + key match.
pub fn should_cancel(has_focus: bool, escape_pressed: bool, _active: bool) -> bool {
    has_focus && escape_pressed
}

/// Process a submitted command-line buffer.
///
/// 1. Trims leading/trailing whitespace.
/// 2. If the result is empty, returns `None`.
/// 3. Pushes the trimmed text into `history`.
/// 4. Truncates history to [`MAX_HISTORY`] entries.
/// 5. Returns `Some(text)` for dispatch.
pub fn handle_submit(buffer: &str, history: &mut Vec<String>) -> Option<String> {
    let text = buffer.trim().to_string();
    if text.is_empty() {
        return None;
    }
    history.push(text.clone());
    if history.len() > MAX_HISTORY {
        history.drain(..history.len() - MAX_HISTORY);
    }
    Some(text)
}

// ---------------------------------------------------------------------------
// UI draw function
// ---------------------------------------------------------------------------

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
                let prompt = resolve_prompt(cmd_state.active.as_ref());
                ui.label(prompt);

                // Show last command error in red if present
                if let Some(ref err) = cmd_state.last_error {
                    ui.colored_label(egui::Color32::RED, err);
                }

                let response = ui.text_edit_singleline(&mut cmd_state.buffer);

                // Detect Enter keypress.
                // text_edit_singleline retains focus on Enter, so lost_focus()
                // is never true simultaneously with Enter. Only has_focus()
                // detects the keypress correctly.
                if should_submit(
                    response.has_focus(),
                    ui.input(|i| i.key_pressed(egui::Key::Enter)),
                ) {
                    if let Some(text) = handle_submit(&cmd_state.buffer, &mut cmd_state.history)
                    {
                        cmd_state.pending_dispatch = Some(text);
                        cmd_state.buffer.clear();
                    } else if cmd_state.active.is_some() {
                        // Buffer is empty but a command is active — send an
                        // empty dispatch signal so dispatch_command_text()
                        // can forward it as CommandInput::Confirm.
                        cmd_state.pending_dispatch = Some(String::new());
                        cmd_state.buffer.clear();
                    }
                }

                if should_cancel(
                    response.has_focus(),
                    ui.input(|i| i.key_pressed(egui::Key::Escape)),
                    cmd_state.active.is_some(),
                ) {
                    cmd_state.buffer.clear();
                    if cmd_state.active.is_some() {
                        cmd_state.cancel_requested = true;
                    }
                }
            });
        });
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{Command, CommandInput, CommandResult, PreviewEntity};
    use hecs::World;

    /// Minimal command stub for testing prompt resolution.
    struct TestCommand;
    impl Command for TestCommand {
        fn name(&self) -> &'static str {
            "TEST"
        }
        fn prompt(&self) -> String {
            "Enter test value:".to_string()
        }
        fn steps_remaining(&self) -> usize {
            1
        }
        fn on_input(
            &mut self,
            _input: CommandInput,
            _world: &mut World,
        ) -> CommandResult {
            CommandResult::Complete
        }
        fn on_cancel(&mut self, _world: &mut World) {}
        fn preview(&self) -> Vec<PreviewEntity> {
            vec![]
        }
    }

    // -- resolve_prompt -- //

    #[test]
    fn test_resolve_prompt_default() {
        assert_eq!(resolve_prompt(None), "Type command:");
    }

    #[test]
    fn test_resolve_prompt_active() {
        let cmd: Box<dyn Command> = Box::new(TestCommand);
        assert_eq!(resolve_prompt(Some(&cmd)), "Enter test value:");
    }

    // -- should_submit -- //

    #[test]
    fn test_should_submit_both_true() {
        assert!(should_submit(true, true));
    }

    #[test]
    fn test_should_submit_no_focus() {
        assert!(!should_submit(false, true));
    }

    #[test]
    fn test_should_submit_no_enter() {
        assert!(!should_submit(true, false));
    }

    #[test]
    fn test_should_submit_neither() {
        assert!(!should_submit(false, false));
    }

    // -- should_cancel -- //

    #[test]
    fn test_should_cancel_both_true() {
        assert!(should_cancel(true, true, true));
    }

    #[test]
    fn test_should_cancel_no_focus() {
        assert!(!should_cancel(false, true, true));
    }

    #[test]
    fn test_should_cancel_no_escape() {
        assert!(!should_cancel(true, false, true));
    }

    #[test]
    fn test_should_cancel_active_irrelevant() {
        // The `_active` parameter does not affect the condition.
        assert!(should_cancel(true, true, false));
        assert!(should_cancel(true, true, true));
    }

    // -- handle_submit -- //

    #[test]
    fn test_handle_submit_empty_string() {
        let mut history = Vec::new();
        assert_eq!(handle_submit("", &mut history), None);
        assert!(history.is_empty());
    }

    #[test]
    fn test_handle_submit_whitespace_only() {
        let mut history = Vec::new();
        assert_eq!(handle_submit("   ", &mut history), None);
        assert!(history.is_empty());
    }

    #[test]
    fn test_handle_submit_trims_whitespace() {
        let mut history = Vec::new();
        let result = handle_submit("  LINE 0,0 100,100  ", &mut history);
        assert_eq!(result, Some("LINE 0,0 100,100".to_string()));
        assert_eq!(history.len(), 1);
        assert_eq!(history[0], "LINE 0,0 100,100");
    }

    #[test]
    fn test_handle_submit_preserves_internal_spaces() {
        let mut history = Vec::new();
        let result = handle_submit("LINE 0,0 100,100", &mut history);
        assert_eq!(result, Some("LINE 0,0 100,100".to_string()));
    }

    #[test]
    fn test_handle_submit_history_order() {
        let mut history = Vec::new();
        handle_submit("first", &mut history);
        handle_submit("second", &mut history);
        handle_submit("third", &mut history);
        assert_eq!(history, vec!["first", "second", "third"]);
    }

    #[test]
    fn test_handle_submit_history_truncation() {
        // Push MAX_HISTORY + 1 entries and verify the oldest is dropped.
        let mut history = Vec::new();
        for i in 0..=MAX_HISTORY {
            handle_submit(&format!("cmd{}", i), &mut history);
        }
        assert_eq!(history.len(), MAX_HISTORY);
        assert_eq!(history[0], "cmd1"); // "cmd0" was dropped
        assert_eq!(history[MAX_HISTORY - 1], format!("cmd{}", MAX_HISTORY));
    }

    #[test]
    fn test_handle_submit_single_entry_no_truncation() {
        let mut history = Vec::new();
        handle_submit("only", &mut history);
        assert_eq!(history.len(), 1);
        assert_eq!(history[0], "only");
    }
}
