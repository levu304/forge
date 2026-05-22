//! LINE command implementation.
//!
//! Uses a **buffer-then-commit** design:
//! 1. Points are accumulated in `pending_points`.
//! 2. On `Confirm`, all buffered segments are spawned into the ECS.
//! 3. On `Cancel`, the buffer is dropped (nothing spawned → no cleanup needed).

use crate::commands::{Command, CommandInput, CommandResult, PreviewEntity};
use crate::ecs::components::{LineData, Renderable};
use crate::geometry::Point2D;
use crate::util::Color;
use hecs::World;

/// Interactive LINE command that accumulates points and spawns line segments
/// on confirmation.
pub struct LineCommand {
    /// Accumulated vertices (not yet in ECS).
    pending_points: Vec<Point2D>,
    /// Rubber-band line preview (deferred to v0.2.0+).
    preview_entity: Option<PreviewEntity>,
}

impl LineCommand {
    pub fn new() -> Self {
        Self {
            pending_points: Vec::new(),
            preview_entity: None,
        }
    }
}

impl Command for LineCommand {
    fn name(&self) -> &'static str {
        "LINE"
    }

    fn prompt(&self) -> String {
        if self.pending_points.is_empty() {
            "Specify first point:".to_string()
        } else {
            "Specify next point or [Undo]:".to_string()
        }
    }

    fn steps_remaining(&self) -> usize {
        if self.pending_points.len() < 2 {
            2 - self.pending_points.len()
        } else {
            0
        }
    }

    fn on_input(&mut self, input: CommandInput, world: &mut World) -> CommandResult {
        match input {
            CommandInput::Point(p) => {
                self.pending_points.push(p);
                CommandResult::Continue
            }
            CommandInput::Text(ref t) if t.eq_ignore_ascii_case("U") => {
                self.pending_points.pop();
                CommandResult::Continue
            }
            CommandInput::Confirm if self.pending_points.len() >= 2 => {
                for window in self.pending_points.windows(2) {
                    world.spawn((
                        LineData {
                            start: window[0],
                            end: window[1],
                            color: Color::WHITE,
                            width: 1.0,
                        },
                        Renderable,
                    ));
                }
                CommandResult::Complete
            }
            CommandInput::Cancel => {
                self.on_cancel(world);
                CommandResult::Cancelled
            }
            _ => CommandResult::Error("Invalid input. Specify a point.".to_string()),
        }
    }

    fn preview(&self) -> Vec<PreviewEntity> {
        // Rubber-band preview deferred to v0.2.0+.
        vec![]
    }

    fn on_cancel(&mut self, _world: &mut World) {
        self.pending_points.clear();
        self.preview_entity = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_world() -> World {
        World::new()
    }

    #[test]
    fn test_name_and_prompt() {
        let cmd = LineCommand::new();
        assert_eq!(cmd.name(), "LINE");

        // First prompt when no points
        assert_eq!(cmd.prompt(), "Specify first point:");

        // Prompt after accumulating a point
        let mut cmd = LineCommand::new();
        let mut world = create_world();
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(10.0, 20.0)), &mut world);
        assert_eq!(cmd.prompt(), "Specify next point or [Undo]:");
    }

    #[test]
    fn test_accumulates_points() {
        let mut cmd = LineCommand::new();
        let mut world = create_world();
        assert_eq!(cmd.steps_remaining(), 2);

        let result = cmd.on_input(CommandInput::Point(Point2D::new(10.0, 20.0)), &mut world);
        assert!(matches!(result, CommandResult::Continue));
        assert_eq!(cmd.steps_remaining(), 1);

        let result = cmd.on_input(CommandInput::Point(Point2D::new(30.0, 40.0)), &mut world);
        assert!(matches!(result, CommandResult::Continue));
        assert_eq!(cmd.steps_remaining(), 0);
    }

    #[test]
    fn test_confirm_spawns_entities() {
        let mut cmd = LineCommand::new();
        let mut world = create_world();

        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(100.0, 100.0)), &mut world);

        let result = cmd.on_input(CommandInput::Confirm, &mut world);
        assert!(matches!(result, CommandResult::Complete));

        // Query for entities with both LineData and Renderable
        let mut count = 0;
        for (_entity, (_line, _renderable)) in world.query::<(&LineData, &Renderable)>().iter() {
            count += 1;
        }
        assert_eq!(count, 1);
    }

    #[test]
    fn test_cancel_clears_buffer() {
        let mut cmd = LineCommand::new();
        let mut world = create_world();

        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(100.0, 100.0)), &mut world);

        let result = cmd.on_input(CommandInput::Cancel, &mut world);
        assert!(matches!(result, CommandResult::Cancelled));

        // No entities should have been spawned
        let mut count = 0;
        for (_entity, (_line, _renderable)) in world.query::<(&LineData, &Renderable)>().iter() {
            count += 1;
        }
        assert_eq!(count, 0);

        // Name should still work after cancel
        assert_eq!(cmd.name(), "LINE");
    }

    #[test]
    fn test_undo_last_point() {
        let mut cmd = LineCommand::new();
        let mut world = create_world();

        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(100.0, 0.0)), &mut world);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(100.0, 100.0)), &mut world);

        // Undo last point
        let result = cmd.on_input(CommandInput::Text("U".to_string()), &mut world);
        assert!(matches!(result, CommandResult::Continue));

        // Confirm with 2 points should spawn 1 segment
        let result = cmd.on_input(CommandInput::Confirm, &mut world);
        assert!(matches!(result, CommandResult::Complete));

        let mut count = 0;
        for (_entity, (_line, _renderable)) in world.query::<(&LineData, &Renderable)>().iter() {
            count += 1;
        }
        assert_eq!(count, 1);
    }

    #[test]
    fn test_single_point_no_commit() {
        let mut cmd = LineCommand::new();
        let mut world = create_world();

        let _ = cmd.on_input(CommandInput::Point(Point2D::new(10.0, 20.0)), &mut world);

        let result = cmd.on_input(CommandInput::Confirm, &mut world);
        assert!(matches!(result, CommandResult::Error(_)));

        // No entities should have been spawned
        let mut count = 0;
        for (_entity, (_line, _renderable)) in world.query::<(&LineData, &Renderable)>().iter() {
            count += 1;
        }
        assert_eq!(count, 0);
    }

    #[test]
    fn test_multiple_segments() {
        let mut cmd = LineCommand::new();
        let mut world = create_world();

        // Feed 4 points = 3 segments
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(10.0, 0.0)), &mut world);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(10.0, 10.0)), &mut world);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 10.0)), &mut world);

        let result = cmd.on_input(CommandInput::Confirm, &mut world);
        assert!(matches!(result, CommandResult::Complete));

        let mut count = 0;
        for (_entity, (_line, _renderable)) in world.query::<(&LineData, &Renderable)>().iter() {
            count += 1;
        }
        assert_eq!(count, 3);
    }

    #[test]
    fn test_preview_empty() {
        let cmd = LineCommand::new();
        assert!(cmd.preview().is_empty());
    }

    #[test]
    fn test_invalid_text_input() {
        let mut cmd = LineCommand::new();
        let mut world = create_world();

        // Feed one point so we're in the "second point" phase
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);

        // Invalid text should give error
        let result = cmd.on_input(CommandInput::Text("X".to_string()), &mut world);
        assert!(matches!(result, CommandResult::Error(_)));
    }

    #[test]
    fn test_undo_empty_buffer() {
        let mut cmd = LineCommand::new();
        let mut world = create_world();

        // Undo on empty buffer — should not panic, just be a no-op
        let result = cmd.on_input(CommandInput::Text("U".to_string()), &mut world);
        assert!(matches!(result, CommandResult::Continue));
    }
}
