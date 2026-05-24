//! ROTATE command (stub).
//!
//! Rotates selected entities around a base point by a specified angle.

use super::{Command, CommandInput, CommandResult};
use hecs::World;

/// Rotates selected entities around a base point.
pub struct RotateCommand;

impl Command for RotateCommand {
    fn name(&self) -> &'static str {
        "ROTATE"
    }

    fn prompt(&self) -> String {
        "Specify base point:".to_string()
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
