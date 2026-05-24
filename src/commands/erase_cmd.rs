//! ERASE command (stub).
//!
//! Removes selected entities from the drawing. Builds a `Transaction`
//! with `Despawn*` ops for the history system.

use super::{Command, CommandInput, CommandResult};
use hecs::World;

/// Removes selected entities from the drawing.
pub struct EraseCommand;

impl Command for EraseCommand {
    fn name(&self) -> &'static str {
        "ERASE"
    }

    fn prompt(&self) -> String {
        "Select objects to erase:".to_string()
    }

    fn steps_remaining(&self) -> usize {
        1
    }

    fn on_input(&mut self, _input: CommandInput, _world: &mut World) -> CommandResult {
        todo!()
    }

    fn on_cancel(&mut self, _world: &mut World) {}

    fn preview(&self) -> Vec<super::PreviewEntity> {
        Vec::new()
    }
}
