//! RECTANGLE command implementation.
//!
//! Two-step state machine:
//! 1. Pick first corner.
//! 2. Pick opposite corner.
//!
//! Spawns 4 [`LineData`] entities forming the rectangle perimeter.
//! Each entity is spawned with [`Renderable`], [`LayerRef`] (default 0),
//! and [`PropertySource::ByLayer`] in a single bundle — matching the
//! pattern used by POLYGON / ELLIPSE / SPLINE. Builds a single
//! [`Transaction`] for full undo/redo via the history system.

use crate::commands::{Command, CommandInput, CommandResult, PreviewEntity};
use crate::ecs::components::{LayerRef, LineData, PropertySource, Renderable};
use crate::geometry::Point2D;
use crate::history::{AtomicOp, Transaction};
use crate::util::Color;
use hecs::World;

/// Interactive RECTANGLE command.
pub struct RectangleCommand {
    /// First corner of the rectangle (set in step 1).
    first_corner: Option<Point2D>,
}

impl RectangleCommand {
    /// Create a new RECTANGLE command with no corners set.
    pub fn new() -> Self {
        Self {
            first_corner: None,
        }
    }
}

impl Default for RectangleCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl Command for RectangleCommand {
    fn name(&self) -> &'static str {
        "RECTANGLE"
    }

    fn prompt(&self) -> String {
        if self.first_corner.is_some() {
            "Specify opposite corner:".to_string()
        } else {
            "Specify first corner:".to_string()
        }
    }

    fn steps_remaining(&self) -> usize {
        if self.first_corner.is_some() { 1 } else { 2 }
    }

    fn on_input(&mut self, input: CommandInput, world: &mut World) -> CommandResult {
        match input {
            CommandInput::Point(p) => {
                match self.first_corner {
                    None => {
                        // Step 1: store first corner
                        self.first_corner = Some(p);
                        CommandResult::Continue
                    }
                    Some(first) => {
                    // Step 2: compute rectangle and spawn entities
                    let second = p;

                    // Compute 4 corners in order: bottom-left, bottom-right,
                    // top-right, top-left (counter-clockwise).
                    let corners = [
                        Point2D::new(first.x.min(second.x), first.y.min(second.y)),
                        Point2D::new(first.x.max(second.x), first.y.min(second.y)),
                        Point2D::new(first.x.max(second.x), first.y.max(second.y)),
                        Point2D::new(first.x.min(second.x), first.y.max(second.y)),
                    ];

                    let mut tx = Transaction::new("Rectangle");

                    // Spawn 4 line segments forming the closed rectangle
                    for i in 0..4 {
                        let start = corners[i];
                        let end = corners[(i + 1) % 4];

                        let entity = world.spawn((
                            LineData {
                                start,
                                end,
                                color: Color::WHITE,
                                width: 1.0,
                            },
                            Renderable,
                            LayerRef(0),
                            PropertySource::ByLayer,
                        ));

                        tx.push(AtomicOp::SpawnLine {
                            entity,
                            data: *world.get::<&LineData>(entity).unwrap(),
                        });
                    }

                    CommandResult::CompleteWithTransaction(tx)
                    }
                }
            }
            CommandInput::Cancel => {
                self.on_cancel(world);
                CommandResult::Cancelled
            }
            _ => CommandResult::Error("Specify a point.".to_string()),
        }
    }

    fn on_cancel(&mut self, _world: &mut World) {
        self.first_corner = None;
    }

    fn preview(&self) -> Vec<PreviewEntity> {
        Vec::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn create_world() -> World {
        World::new()
    }

    #[test]
    fn test_name_and_prompt() {
        let cmd = RectangleCommand::new();
        assert_eq!(cmd.name(), "RECTANGLE");
        assert_eq!(cmd.prompt(), "Specify first corner:");
        assert_eq!(cmd.steps_remaining(), 2);
    }

    #[test]
    fn test_first_point_sets_prompt() {
        let mut cmd = RectangleCommand::new();
        let mut world = create_world();

        let result = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);
        assert!(matches!(result, CommandResult::Continue));
        assert_eq!(cmd.prompt(), "Specify opposite corner:");
        assert_eq!(cmd.steps_remaining(), 1);
    }

    #[test]
    fn test_second_point_completes_with_transaction() {
        let mut cmd = RectangleCommand::new();
        let mut world = create_world();

        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);
        let result = cmd.on_input(CommandInput::Point(Point2D::new(100.0, 100.0)), &mut world);

        // Should return CompleteWithTransaction
        let tx = match result {
            CommandResult::CompleteWithTransaction(tx) => tx,
            _ => panic!("Expected CompleteWithTransaction"),
        };

        // Transaction should have 4 ops: 4 SpawnLine (LayerRef + PropertySource
        // are included in the spawn bundle, no separate SetLayerRef/SetPropertySource)
        assert_eq!(tx.ops.len(), 4);
        assert_eq!(tx.label, "Rectangle");
    }

    #[test]
    fn test_spawns_four_entities() {
        let mut cmd = RectangleCommand::new();
        let mut world = create_world();

        let _ = cmd.on_input(CommandInput::Point(Point2D::new(10.0, 10.0)), &mut world);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(50.0, 50.0)), &mut world);

        let mut count = 0;
        for (_entity, _line) in world.query::<&LineData>().iter() {
            count += 1;
        }
        assert_eq!(count, 4);
    }

    #[test]
    fn test_correct_corners() {
        let mut cmd = RectangleCommand::new();
        let mut world = create_world();

        let _ = cmd.on_input(CommandInput::Point(Point2D::new(10.0, 20.0)), &mut world);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(40.0, 60.0)), &mut world);

        // Collect the 4 line segments
        let mut lines: Vec<LineData> = Vec::new();
        for (_entity, line) in world.query::<&LineData>().iter() {
            lines.push(*line);
        }

        // Each segment should connect two consecutive corners
        // Corners: (10,20), (40,20), (40,60), (10,60)
        let expected_pairs = [
            (Point2D::new(10.0, 20.0), Point2D::new(40.0, 20.0)),
            (Point2D::new(40.0, 20.0), Point2D::new(40.0, 60.0)),
            (Point2D::new(40.0, 60.0), Point2D::new(10.0, 60.0)),
            (Point2D::new(10.0, 60.0), Point2D::new(10.0, 20.0)),
        ];

        for (expected_start, expected_end) in &expected_pairs {
            let found = lines.iter().any(|l| {
                (l.start.x - expected_start.x).abs() < 1e-12
                    && (l.start.y - expected_start.y).abs() < 1e-12
                    && (l.end.x - expected_end.x).abs() < 1e-12
                    && (l.end.y - expected_end.y).abs() < 1e-12
            });
            assert!(found, "Expected segment ({:?} → {:?}) not found", expected_start, expected_end);
        }
    }

    #[test]
    fn test_cancel_clears_state() {
        let mut cmd = RectangleCommand::new();
        let mut world = create_world();

        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);
        let result = cmd.on_input(CommandInput::Cancel, &mut world);
        assert!(matches!(result, CommandResult::Cancelled));
        assert_eq!(cmd.prompt(), "Specify first corner:");
        assert_eq!(cmd.steps_remaining(), 2);
    }

    #[test]
    fn test_invalid_input_returns_error() {
        let mut cmd = RectangleCommand::new();
        let mut world = create_world();

        let result = cmd.on_input(CommandInput::Text("hello".to_string()), &mut world);
        assert!(matches!(result, CommandResult::Error(_)));
    }

    #[test]
    fn test_reversed_corners() {
        // User can pick corners in any order — rectangle should normalise
        let mut cmd = RectangleCommand::new();
        let mut world = create_world();

        let _ = cmd.on_input(CommandInput::Point(Point2D::new(100.0, 100.0)), &mut world);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);

        let mut lines: Vec<LineData> = Vec::new();
        for (_entity, line) in world.query::<&LineData>().iter() {
            lines.push(*line);
        }

        // Should still be normalised to (0,0)-(100,100)
        let has_min_corner = lines.iter().any(|l| {
            (l.start.x - 0.0).abs() < 1e-12 || (l.end.x - 0.0).abs() < 1e-12
        });
        let has_max_corner = lines.iter().any(|l| {
            (l.start.x - 100.0).abs() < 1e-12 || (l.end.x - 100.0).abs() < 1e-12
        });
        assert!(has_min_corner);
        assert!(has_max_corner);
    }

    #[test]
    fn test_entities_have_layer_and_property_source() {
        let mut cmd = RectangleCommand::new();
        let mut world = create_world();

        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(10.0, 10.0)), &mut world);

        // Check all entities have LayerRef and PropertySource
        let mut entity_count = 0;
        for (_entity, (_line, _layer, _ps)) in
            world.query::<(&LineData, &LayerRef, &PropertySource)>().iter()
        {
            entity_count += 1;
        }
        assert_eq!(entity_count, 4);
    }
}
