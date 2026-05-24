//! COPY command (stub).
//!
//! Creates displaced copies of selected entities. Identical to MOVE
//! but spawns new entities instead of relocating originals.

use super::{Command, CommandInput, CommandResult};
use hecs::World;

/// Creates displaced copies of selected entities.
pub struct CopyCommand;

impl Command for CopyCommand {
    fn name(&self) -> &'static str {
        "COPY"
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
