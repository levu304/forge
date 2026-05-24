//! MOVE command (stub).
//!
//! Displaces selected entities by a vector. Two-step interaction:
//! base point + second point (or relative displacement).

use super::{Command, CommandInput, CommandResult};
use hecs::World;

/// Displaces selected entities by a vector.
pub struct MoveCommand;

impl Command for MoveCommand {
    fn name(&self) -> &'static str {
        "MOVE"
    }

    fn prompt(&self) -> String {
        "Specify base point or displacement:".to_string()
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
