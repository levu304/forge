//! MIRROR command (stub).
//!
//! Reflects selected entities across a mirror line defined by two points.

use super::{Command, CommandInput, CommandResult};
use hecs::World;

/// Reflects selected entities across a mirror line.
pub struct MirrorCommand;

impl Command for MirrorCommand {
    fn name(&self) -> &'static str {
        "MIRROR"
    }

    fn prompt(&self) -> String {
        "Specify first point of mirror line:".to_string()
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
