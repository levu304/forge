//! POLYGON command implementation.
//!
//! Two-step state machine:
//! 1. Pick center point.
//! 2. Pick radius (or type distance).
//!
//! Spawns a closed [`PolylineData`] entity with N vertices (default 6).
//! Builds a [`Transaction`] for undo/redo.

use std::f64::consts::TAU;

use crate::commands::{Command, CommandInput, CommandResult, PreviewEntity};
use crate::ecs::components::{LayerRef, PolylineData, PropertySource, Renderable};
use crate::geometry::Point2D;
use crate::history::{AtomicOp, Transaction};
use crate::util::Color;
use hecs::World;

/// Interactive POLYGON command.
pub struct PolygonCommand {
    /// Center of the polygon (set in step 1).
    center: Option<Point2D>,
    /// Number of sides (default 6, can be changed via set_sides).
    sides: u32,
}

impl PolygonCommand {
    /// Create a new POLYGON command.
    pub fn new() -> Self {
        Self {
            center: None,
            sides: 6,
        }
    }

    /// Override the default number of sides (used by parser when `sides`
    /// is provided as an inline argument).
    pub fn set_sides(&mut self, sides: u32) {
        self.sides = sides.max(3); // Minimum 3 sides for a valid polygon
    }

    /// Compute N vertices evenly spaced around the center at the given radius.
    fn compute_vertices(center: Point2D, radius: f64, sides: u32) -> Vec<Point2D> {
        let n = sides as f64;
        (0..sides)
            .map(|i| {
                let angle = (i as f64) / n * TAU;
                Point2D::new(
                    center.x + radius * angle.cos(),
                    center.y + radius * angle.sin(),
                )
            })
            .collect()
    }
}

impl Default for PolygonCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl Command for PolygonCommand {
    fn name(&self) -> &'static str {
        "POLYGON"
    }

    fn prompt(&self) -> String {
        if self.center.is_some() {
            "Specify radius:".to_string()
        } else {
            "Specify center:".to_string()
        }
    }

    fn steps_remaining(&self) -> usize {
        if self.center.is_some() { 1 } else { 2 }
    }

    fn on_input(&mut self, input: CommandInput, world: &mut World) -> CommandResult {
        match input {
            CommandInput::Point(p) => {
                match self.center {
                    None => {
                        // Step 1: store center
                        self.center = Some(p);
                        CommandResult::Continue
                    }
                    Some(center) => {
                    // Step 2: use point as radius (distance from center)
                    let radius = center.distance(p);
                    let vertices = Self::compute_vertices(center, radius, self.sides);

                    let entity = world.spawn((
                        PolylineData {
                            vertices,
                            closed: true,
                            color: Color::WHITE,
                            width: 1.0,
                        },
                        Renderable,
                        LayerRef(0),
                        PropertySource::ByLayer,
                    ));

                    let mut tx = Transaction::new(format!(
                        "Polygon ({} sides)",
                        self.sides,
                    ));

                    tx.push(AtomicOp::SpawnPolyline {
                        entity,
                        data: world.get::<&PolylineData>(entity).as_deref().unwrap().clone(),
                    });

                    CommandResult::CompleteWithTransaction(tx)
                    }
                }
            }
            CommandInput::Distance(d) => {
                if let Some(center) = self.center {
                    let vertices = Self::compute_vertices(center, d, self.sides);

                    let entity = world.spawn((
                        PolylineData {
                            vertices,
                            closed: true,
                            color: Color::WHITE,
                            width: 1.0,
                        },
                        Renderable,
                        LayerRef(0),
                        PropertySource::ByLayer,
                    ));

                    let mut tx = Transaction::new(format!(
                        "Polygon ({} sides)",
                        self.sides,
                    ));

                    tx.push(AtomicOp::SpawnPolyline {
                        entity,
                        data: world.get::<&PolylineData>(entity).as_deref().unwrap().clone(),
                    });

                    CommandResult::CompleteWithTransaction(tx)
                } else {
                    CommandResult::Error("Specify center point first.".to_string())
                }
            }
            CommandInput::Cancel => {
                self.on_cancel(world);
                CommandResult::Cancelled
            }
            _ => CommandResult::Error("Specify a point or distance.".to_string()),
        }
    }

    fn on_cancel(&mut self, _world: &mut World) {
        self.center = None;
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
        let cmd = PolygonCommand::new();
        assert_eq!(cmd.name(), "POLYGON");
        assert_eq!(cmd.prompt(), "Specify center:");
        assert_eq!(cmd.steps_remaining(), 2);
    }

    #[test]
    fn test_default_sides_is_six() {
        let cmd = PolygonCommand::new();
        assert_eq!(cmd.sides, 6);
    }

    #[test]
    fn test_set_sides() {
        let mut cmd = PolygonCommand::new();
        cmd.set_sides(8);
        assert_eq!(cmd.sides, 8);
    }

    #[test]
    fn test_set_sides_minimum_three() {
        let mut cmd = PolygonCommand::new();
        cmd.set_sides(1);
        assert_eq!(cmd.sides, 3);
    }

    #[test]
    fn test_first_point_sets_center() {
        let mut cmd = PolygonCommand::new();
        let mut world = create_world();

        let result = cmd.on_input(CommandInput::Point(Point2D::new(50.0, 50.0)), &mut world);
        assert!(matches!(result, CommandResult::Continue));
        assert_eq!(cmd.prompt(), "Specify radius:");
        assert_eq!(cmd.steps_remaining(), 1);
    }

    #[test]
    fn test_second_point_spawns_entity() {
        let mut cmd = PolygonCommand::new();
        let mut world = create_world();

        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);
        let result = cmd.on_input(CommandInput::Point(Point2D::new(10.0, 0.0)), &mut world);

        let tx = match result {
            CommandResult::CompleteWithTransaction(tx) => tx,
            _ => panic!("Expected CompleteWithTransaction"),
        };

        assert_eq!(tx.ops.len(), 1);
    }

    #[test]
    fn test_vertex_count_default() {
        let mut cmd = PolygonCommand::new();
        let mut world = create_world();

        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(10.0, 0.0)), &mut world);

        for (_entity, poly) in world.query::<&PolylineData>().iter() {
            assert_eq!(poly.vertices.len(), 6);
            assert!(poly.closed);
        }
    }

    #[test]
    fn test_vertex_count_custom_sides() {
        let mut cmd = PolygonCommand::new();
        cmd.set_sides(8);
        let mut world = create_world();

        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(10.0, 0.0)), &mut world);

        for (_entity, poly) in world.query::<&PolylineData>().iter() {
            assert_eq!(poly.vertices.len(), 8);
        }
    }

    #[test]
    fn test_distance_input() {
        let mut cmd = PolygonCommand::new();
        let mut world = create_world();

        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);
        let result = cmd.on_input(CommandInput::Distance(25.0), &mut world);

        assert!(matches!(result, CommandResult::CompleteWithTransaction(_)));

        for (_entity, poly) in world.query::<&PolylineData>().iter() {
            assert_eq!(poly.vertices.len(), 6);
            // Check first vertex is at (25, 0)
            assert!((poly.vertices[0].x - 25.0).abs() < 1e-12);
            assert!((poly.vertices[0].y - 0.0).abs() < 1e-12);
        }
    }

    #[test]
    fn test_distance_before_center_returns_error() {
        let mut cmd = PolygonCommand::new();
        let mut world = create_world();

        let result = cmd.on_input(CommandInput::Distance(10.0), &mut world);
        assert!(matches!(result, CommandResult::Error(_)));
    }

    #[test]
    fn test_cancel_clears_state() {
        let mut cmd = PolygonCommand::new();
        let mut world = create_world();

        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);
        let result = cmd.on_input(CommandInput::Cancel, &mut world);
        assert!(matches!(result, CommandResult::Cancelled));
        assert_eq!(cmd.prompt(), "Specify center:");
    }

    #[test]
    fn test_invalid_input_returns_error() {
        let mut cmd = PolygonCommand::new();
        let mut world = create_world();

        let result = cmd.on_input(CommandInput::Text("hello".to_string()), &mut world);
        assert!(matches!(result, CommandResult::Error(_)));
    }

    #[test]
    fn test_vertices_form_regular_polygon() {
        let center = Point2D::new(0.0, 0.0);
        let vertices = PolygonCommand::compute_vertices(center, 10.0, 6);

        // All vertices should be at distance ~10 from center
        for v in &vertices {
            let dist = center.distance(*v);
            assert!((dist - 10.0).abs() < 1e-12, "Vertex distance {}, expected 10", dist);
        }

        // Each vertex should be ~TAU/6 apart from the next
        for i in 0..6 {
            let next = (i + 1) % 6;
            let dx = vertices[next].x - vertices[i].x;
            let dy = vertices[next].y - vertices[i].y;
            let edge_len = (dx * dx + dy * dy).sqrt();
            // Edge length for regular hexagon of radius 10 = 10
            assert!((edge_len - 10.0).abs() < 1e-12, "Edge {} length {}, expected 10", i, edge_len);
        }
    }
}
