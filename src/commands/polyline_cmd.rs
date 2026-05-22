//! PLINE command implementation (reserved — v0.2.0+).

use crate::commands::{Command, CommandInput, CommandResult, PreviewEntity};
use hecs::World;

/// Interactive PLINE (polyline) command.
///
/// Full implementation deferred to v0.2.0+.
#[derive(Default)]
pub struct PolylineCommand;

impl PolylineCommand {
    pub fn new() -> Self {
        Self
    }
}

impl Command for PolylineCommand {
    fn name(&self) -> &'static str {
        "PLINE"
    }

    fn prompt(&self) -> String {
        "Not implemented (v0.2.0+)".to_string()
    }

    fn steps_remaining(&self) -> usize {
        1
    }

    fn on_input(&mut self, _input: CommandInput, _world: &mut World) -> CommandResult {
        CommandResult::Error(
            "PLINE command not yet implemented (v0.2.0+)".to_string(),
        )
    }

    fn preview(&self) -> Vec<PreviewEntity> {
        vec![]
    }

    fn on_cancel(&mut self, _world: &mut World) {
        // No-op: nothing to clean up.
    }
}
