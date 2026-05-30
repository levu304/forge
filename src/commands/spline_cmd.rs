//! SPLINE command implementation.
//!
//! Variable-step state machine:
//! 1. User clicks fit points (accumulated).
//! 2. User presses Confirm (Enter) to finalise.
//!
//! Uses Catmull-Rom spline with tension=0.5 (centripetal) and converts
//! each segment to a [`CubicBezier`] for evaluation. The resulting polyline
//! uses 64 segments per Bezier segment.
//!
//! Boundary conditions: first segment uses p0=p1, last segment uses p3=p_{n-1}
//! (i.e., the first and last control points are duplicated).

use crate::commands::{Command, CommandInput, CommandResult, PreviewEntity};
use crate::ecs::components::{LayerRef, PolylineData, PropertySource, Renderable};
use crate::geometry::{CubicBezier, Point2D};
use crate::history::{AtomicOp, Transaction};
use crate::util::Color;
use hecs::World;

/// Segments per Bezier curve for polyline approximation.
const SPLINE_SEGMENTS_PER_BEZIER: usize = 64;

/// Minimum number of fit points needed for a valid spline.
/// With 2 points we can create a straight-line spline (degenerate).
/// With 3+ we get proper curvature.
const MIN_SPLINE_POINTS: usize = 2;

/// Interactive SPLINE command.
pub struct SplineCommand {
    /// Accumulated fit points (not yet in ECS).
    pending_points: Vec<Point2D>,
}

impl SplineCommand {
    /// Create a new SPLINE command.
    pub fn new() -> Self {
        Self {
            pending_points: Vec::new(),
        }
    }

    /// Convert a Catmull-Rom segment (4 control points) to a CubicBezier.
    ///
    /// Uses tension=0.5 (centripetal Catmull-Rom → uniform when points
    /// are evenly spaced).
    ///
    /// Given control points P0, P1, P2, P3, the cubic Bezier control points are:
    ///   B0 = P1
    ///   B1 = P1 + (P2 - P0) / 6
    ///   B2 = P2 - (P3 - P1) / 6
    ///   B3 = P2
    fn catmull_rom_to_bezier(p0: Point2D, p1: Point2D, p2: Point2D, p3: Point2D) -> CubicBezier {
        CubicBezier {
            p0: p1,
            p1: Point2D::new(
                p1.x + (p2.x - p0.x) / 6.0,
                p1.y + (p2.y - p0.y) / 6.0,
            ),
            p2: Point2D::new(
                p2.x - (p3.x - p1.x) / 6.0,
                p2.y - (p3.y - p1.y) / 6.0,
            ),
            p3: p2,
        }
    }

    /// Build the full spline polyline from accumulated fit points.
    fn build_spline(&self) -> Vec<Point2D> {
        let n = self.pending_points.len();
        if n < 2 {
            return Vec::new();
        }

        let mut all_vertices = Vec::new();

        // For each segment between consecutive fit points, create a
        // Catmull-Rom → Bezier segment and evaluate it.
        for i in 0..(n - 1) {
            // Boundary: first segment duplicates p0, last duplicates p_n-1
            let p0 = if i == 0 {
                self.pending_points[0]
            } else {
                self.pending_points[i - 1]
            };
            let p1 = self.pending_points[i];
            let p2 = self.pending_points[i + 1];
            let p3 = if i + 2 < n {
                self.pending_points[i + 2]
            } else {
                self.pending_points[n - 1]
            };

            let bezier = Self::catmull_rom_to_bezier(p0, p1, p2, p3);

            // Get vertices for this segment
            let segment_vertices = bezier.to_polyline(SPLINE_SEGMENTS_PER_BEZIER);

            if all_vertices.is_empty() {
                all_vertices = segment_vertices;
            } else {
                // Skip the first vertex of this segment (it duplicates
                // the last vertex of the previous segment)
                all_vertices.extend_from_slice(&segment_vertices[1..]);
            }
        }

        all_vertices
    }
}

impl Default for SplineCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl Command for SplineCommand {
    fn name(&self) -> &'static str {
        "SPLINE"
    }

    fn prompt(&self) -> String {
        if self.pending_points.is_empty() {
            "Specify first fit point:".to_string()
        } else {
            "Specify next fit point or press Enter to confirm:".to_string()
        }
    }

    fn steps_remaining(&self) -> usize {
        if self.pending_points.len() < MIN_SPLINE_POINTS {
            MIN_SPLINE_POINTS - self.pending_points.len()
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
            CommandInput::Confirm => {
                if self.pending_points.len() < MIN_SPLINE_POINTS {
                    return CommandResult::Error(format!(
                        "Need at least {} points for a spline (got {}).",
                        MIN_SPLINE_POINTS,
                        self.pending_points.len(),
                    ));
                }

                let vertices = self.build_spline();
                if vertices.is_empty() {
                    return CommandResult::Error("Failed to build spline (no vertices).".to_string());
                }

                let entity = world.spawn((
                    PolylineData {
                        vertices,
                        closed: false,
                        color: Color::WHITE,
                        width: 1.0,
                    },
                    Renderable,
                    LayerRef(0),
                    PropertySource::ByLayer,
                ));

                let mut tx = Transaction::new(format!(
                    "Spline ({} point{})",
                    self.pending_points.len(),
                    if self.pending_points.len() == 1 { "" } else { "s" },
                ));

                tx.push(AtomicOp::SpawnPolyline {
                    entity,
                    data: world.get::<&PolylineData>(entity).as_deref().unwrap().clone(),
                });

                CommandResult::CompleteWithTransaction(tx)
            }
            CommandInput::Cancel => {
                self.on_cancel(world);
                CommandResult::Cancelled
            }
            _ => CommandResult::Error("Specify a point or press Enter.".to_string()),
        }
    }

    fn on_cancel(&mut self, _world: &mut World) {
        self.pending_points.clear();
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
        let cmd = SplineCommand::new();
        assert_eq!(cmd.name(), "SPLINE");
        assert_eq!(cmd.prompt(), "Specify first fit point:");
        assert_eq!(cmd.steps_remaining(), 2);
    }

    #[test]
    fn test_accumulates_points() {
        let mut cmd = SplineCommand::new();
        let mut world = create_world();

        let result = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);
        assert!(matches!(result, CommandResult::Continue));
        assert_eq!(cmd.pending_points.len(), 1);
    }

    #[test]
    fn test_confirm_with_fewer_than_min_points_returns_error() {
        let mut cmd = SplineCommand::new();
        let mut world = create_world();

        let result = cmd.on_input(CommandInput::Confirm, &mut world);
        assert!(matches!(result, CommandResult::Error(_)));

        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);
        let result = cmd.on_input(CommandInput::Confirm, &mut world);
        // With 1 point, still < 2
        assert!(matches!(result, CommandResult::Error(_)));
    }

    #[test]
    fn test_confirm_with_two_points_spawns_entity() {
        let mut cmd = SplineCommand::new();
        let mut world = create_world();

        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(100.0, 100.0)), &mut world);
        let result = cmd.on_input(CommandInput::Confirm, &mut world);

        assert!(matches!(result, CommandResult::CompleteWithTransaction(_)));

        // Should have spawned one polyline entity
        let mut count = 0;
        for (_entity, _poly) in world.query::<&PolylineData>().iter() {
            count += 1;
        }
        assert_eq!(count, 1);
    }

    #[test]
    fn test_confirm_with_four_points() {
        let mut cmd = SplineCommand::new();
        let mut world = create_world();

        let points = vec![
            Point2D::new(0.0, 0.0),
            Point2D::new(50.0, 100.0),
            Point2D::new(100.0, 0.0),
            Point2D::new(150.0, 100.0),
        ];

        for p in points {
            let _ = cmd.on_input(CommandInput::Point(p), &mut world);
        }
        let result = cmd.on_input(CommandInput::Confirm, &mut world);
        assert!(matches!(result, CommandResult::CompleteWithTransaction(_)));

        for (_entity, poly) in world.query::<&PolylineData>().iter() {
            // With 4 points → 3 segments × 64 = 192 vertices (minus overlap)
            // Actually: first segment 65 vertices, then 63 each for next 2 = 191
            assert!(poly.vertices.len() > 100, "Expected >100 vertices, got {}", poly.vertices.len());
            assert!(!poly.closed);
        }
    }

    #[test]
    fn test_cancel_clears_buffer() {
        let mut cmd = SplineCommand::new();
        let mut world = create_world();

        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(100.0, 100.0)), &mut world);

        let result = cmd.on_input(CommandInput::Cancel, &mut world);
        assert!(matches!(result, CommandResult::Cancelled));
        assert!(cmd.pending_points.is_empty());
    }

    #[test]
    fn test_invalid_input_returns_error() {
        let mut cmd = SplineCommand::new();
        let mut world = create_world();

        let result = cmd.on_input(CommandInput::Text("hello".to_string()), &mut world);
        assert!(matches!(result, CommandResult::Error(_)));
    }

    #[test]
    fn test_build_spline_returns_vertices() {
        let mut cmd = SplineCommand::new();
        cmd.pending_points = vec![
            Point2D::new(0.0, 0.0),
            Point2D::new(50.0, 100.0),
            Point2D::new(100.0, 0.0),
            Point2D::new(150.0, 100.0),
        ];

        let vertices = cmd.build_spline();
        assert!(!vertices.is_empty());

        // First vertex should be at the first fit point
        assert!((vertices[0].x - 0.0).abs() < 1e-12);
        assert!((vertices[0].y - 0.0).abs() < 1e-12);

        // Last vertex should be at the last fit point
        let last = vertices.last().unwrap();
        assert!((last.x - 150.0).abs() < 1e-12);
        assert!((last.y - 100.0).abs() < 1e-12);
    }

    #[test]
    fn test_catmull_rom_to_bezier_straight_line() {
        // With collinear evenly-spaced points, the Bezier should be a straight line
        let bezier = SplineCommand::catmull_rom_to_bezier(
            Point2D::new(0.0, 0.0),
            Point2D::new(10.0, 0.0),
            Point2D::new(20.0, 0.0),
            Point2D::new(30.0, 0.0),
        );

        // All control points should be on y=0
        assert!((bezier.p0.y - 0.0).abs() < 1e-12);
        assert!((bezier.p1.y - 0.0).abs() < 1e-12);
        assert!((bezier.p2.y - 0.0).abs() < 1e-12);
        assert!((bezier.p3.y - 0.0).abs() < 1e-12);

        // Midpoint should be at (15, 0)
        let mid = bezier.evaluate(0.5);
        assert!((mid.x - 15.0).abs() < 1e-12);
        assert!((mid.y - 0.0).abs() < 1e-12);
    }

    #[test]
    fn test_empty_spline_returns_empty() {
        let cmd = SplineCommand::new();
        let vertices = cmd.build_spline();
        assert!(vertices.is_empty());
    }

    #[test]
    fn test_single_point_spline_returns_empty() {
        let mut cmd = SplineCommand::new();
        cmd.pending_points.push(Point2D::new(0.0, 0.0));
        let vertices = cmd.build_spline();
        assert!(vertices.is_empty());
    }
}
