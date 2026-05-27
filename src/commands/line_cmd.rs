//! LINE command implementation.
//!
//! Uses a **buffer-then-commit** design:
//! 1. Points are accumulated in `pending_points`.
//! 2. On `Confirm`, all buffered segments are spawned into the ECS.
//! 3. On `Cancel`, the buffer is dropped (nothing spawned → no cleanup needed).

use crate::commands::{Command, CommandInput, CommandResult, PreviewEntity};
use crate::ecs::components::{LineData, Renderable};
use crate::geometry::Point2D;
use crate::history::{AtomicOp, Transaction};
use crate::util::Color;
use hecs::World;

/// Interactive LINE command that accumulates points and spawns line segments
/// on confirmation.
pub struct LineCommand {
    /// Accumulated vertices (not yet in ECS).
    pending_points: Vec<Point2D>,
    /// Rubber-band line preview (deferred to v0.2.0+).
    preview_entity: Option<PreviewEntity>,
    /// Transaction populated on `Confirm`, consumed by caller via
    /// [`take_transaction`](LineCommand::take_transaction).
    pending_transaction: Option<Transaction>,
}

impl Default for LineCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl LineCommand {
    pub fn new() -> Self {
        Self {
            pending_points: Vec::new(),
            preview_entity: None,
            pending_transaction: None,
        }
    }

    /// Consume the pending transaction after command completion.
    ///
    /// Returns `None` if the command has not completed yet.
    pub fn take_transaction(&mut self) -> Option<Transaction> {
        self.pending_transaction.take()
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
                let mut tx = Transaction::new(format!(
                    "Line ({} segment{})",
                    self.pending_points.len() - 1,
                    if self.pending_points.len() == 2 { "" } else { "s" },
                ));

                for window in self.pending_points.windows(2) {
                    let entity = world.spawn((
                        LineData {
                            start: window[0],
                            end: window[1],
                            color: Color::WHITE,
                            width: 1.0,
                        },
                        Renderable,
                    ));
                    tx.push(AtomicOp::SpawnLine {
                        entity,
                        data: *world.get::<&LineData>(entity).unwrap(),
                    });
                }
                self.pending_transaction = Some(tx);
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
        self.pending_transaction = None;
    }

    fn take_transaction(&mut self) -> Option<Transaction> {
        self.pending_transaction.take()
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

    #[test]
    fn test_confirm_empty_buffer() {
        let mut cmd = LineCommand::new();
        let mut world = create_world();

        // Confirm with no points accumulated — should not spawn any entities
        let result = cmd.on_input(CommandInput::Confirm, &mut world);

        // Returns error because at least 2 points are needed
        assert!(matches!(result, CommandResult::Error(_)));

        // No entities should have been spawned
        let mut count = 0;
        for (_entity, (_line, _renderable)) in world.query::<(&LineData, &Renderable)>().iter() {
            count += 1;
        }
        assert_eq!(count, 0);

        // Command is still usable — prompt shows we're still at the first-point phase
        assert_eq!(cmd.prompt(), "Specify first point:");
    }

    #[test]
    fn test_on_cancel_direct() {
        // Call on_cancel directly (not via Cancel input) to verify buffer clearing.
        let mut cmd = LineCommand::new();
        let mut world = create_world();

        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(100.0, 100.0)), &mut world);

        // Direct call to on_cancel (as CommandState::process_pending_cancel does).
        cmd.on_cancel(&mut world);

        // Buffer should be empty — prompt back to first-point phase.
        assert_eq!(cmd.prompt(), "Specify first point:");

        // No entities should have been spawned.
        let mut count = 0;
        for (_entity, (_line, _renderable)) in world.query::<(&LineData, &Renderable)>().iter() {
            count += 1;
        }
        assert_eq!(count, 0);
    }

    #[test]
    fn test_on_cancel_direct_empty() {
        // Calling on_cancel on an empty command should be a no-op.
        let mut cmd = LineCommand::new();
        let mut world = create_world();
        cmd.on_cancel(&mut world);
        assert_eq!(cmd.prompt(), "Specify first point:");
    }

    #[test]
    fn test_steps_remaining_after_undo_then_readd() {
        let mut cmd = LineCommand::new();
        let mut world = create_world();

        assert_eq!(cmd.steps_remaining(), 2);

        // Add first point
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);
        assert_eq!(cmd.steps_remaining(), 1);

        // Add second point
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(100.0, 0.0)), &mut world);
        assert_eq!(cmd.steps_remaining(), 0);

        // Undo second point
        let _ = cmd.on_input(CommandInput::Text("U".to_string()), &mut world);
        assert_eq!(cmd.steps_remaining(), 1);

        // Add second point again
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(100.0, 100.0)), &mut world);
        assert_eq!(cmd.steps_remaining(), 0);
    }

    #[test]
    fn test_steps_remaining_after_undo_all_then_readd() {
        let mut cmd = LineCommand::new();
        let mut world = create_world();

        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(100.0, 0.0)), &mut world);

        // Undo both points
        let _ = cmd.on_input(CommandInput::Text("U".to_string()), &mut world);
        assert_eq!(cmd.steps_remaining(), 1);

        let _ = cmd.on_input(CommandInput::Text("U".to_string()), &mut world);
        assert_eq!(cmd.steps_remaining(), 2);

        // Re-add and confirm
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(50.0, 50.0)), &mut world);
        let result = cmd.on_input(CommandInput::Confirm, &mut world);
        assert!(matches!(result, CommandResult::Complete));
    }

    #[test]
    fn test_default_trait() {
        let cmd = LineCommand::default();
        assert_eq!(cmd.name(), "LINE");
        assert_eq!(cmd.prompt(), "Specify first point:");
        assert!(cmd.preview().is_empty());
        assert_eq!(cmd.steps_remaining(), 2);
    }

    #[test]
    fn test_distance_input_returns_error() {
        let mut cmd = LineCommand::new();
        let mut world = create_world();

        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);
        // Distance variant is reserved but should not panic.
        let result = cmd.on_input(CommandInput::Distance(50.0), &mut world);
        assert!(matches!(result, CommandResult::Error(_)));
    }

    #[test]
    fn test_angle_input_returns_error() {
        let mut cmd = LineCommand::new();
        let mut world = create_world();

        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);
        // Angle variant is reserved but should not panic.
        let result = cmd.on_input(CommandInput::Angle(90.0), &mut world);
        assert!(matches!(result, CommandResult::Error(_)));
    }

    #[test]
    fn test_multiple_undos_empty_buffer_no_panic() {
        let mut cmd = LineCommand::new();
        let mut world = create_world();

        // Undo on empty buffer should be safe.
        let result = cmd.on_input(CommandInput::Text("U".to_string()), &mut world);
        assert!(matches!(result, CommandResult::Continue));
        let result = cmd.on_input(CommandInput::Text("U".to_string()), &mut world);
        assert!(matches!(result, CommandResult::Continue));
        let result = cmd.on_input(CommandInput::Text("U".to_string()), &mut world);
        assert!(matches!(result, CommandResult::Continue));
    }

    #[test]
    fn test_text_close_noop() {
        // "C" text should be treated as invalid input (LINE ignores close).
        let mut cmd = LineCommand::new();
        let mut world = create_world();
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(100.0, 100.0)), &mut world);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(200.0, 200.0)), &mut world);

        let result = cmd.on_input(CommandInput::Text("C".to_string()), &mut world);
        // "C" is not "U" so it falls through to the error case.
        assert!(matches!(result, CommandResult::Error(_)));
    }
}
