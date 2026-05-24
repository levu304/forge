//! OFFSET command (stub).
//!
//! Creates parallel copies of selected entities at a specified distance.

use super::{Command, CommandInput, CommandResult};
use hecs::World;

/// Creates parallel copies of selected entities at a specified distance.
pub struct OffsetCommand;

impl Command for OffsetCommand {
    fn name(&self) -> &'static str {
        "OFFSET"
    }

    fn prompt(&self) -> String {
        "Specify offset distance:".to_string()
    }

    fn steps_remaining(&self) -> usize {
        2
    }

    fn on_input(&mut self, _input: CommandInput, _world: &mut World) -> CommandResult {
        todo!()
    }

    fn on_cancel(&mut self, _world: &mut World) {}

    fn preview(&self) -> Vec<super::PreviewEntity> {
        Vec::new()
    }
}
