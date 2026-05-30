//! ELLIPSE command implementation.
//!
//! Three-step state machine:
//! 1. Pick center point.
//! 2. Pick major axis endpoint (defines major radius + orientation).
//! 3. Pick a point to determine minor ratio (or type ratio directly).
//!
//! Spawns a closed [`PolylineData`] entity with 64 vertices approximating
//! the ellipse. Builds a [`Transaction`] for undo/redo.

use std::f64::consts::TAU;

use crate::commands::{Command, CommandInput, CommandResult, PreviewEntity};
use crate::ecs::components::{LayerRef, PolylineData, PropertySource, Renderable};
use crate::geometry::Point2D;
use crate::history::{AtomicOp, Transaction};
use crate::util::Color;
use hecs::World;

/// Number of segments for ellipse polyline approximation.
const ELLIPSE_SEGMENTS: usize = 64;

/// Interactive ELLIPSE command.
pub struct EllipseCommand {
    /// Center of the ellipse (set in step 1).
    center: Option<Point2D>,
    /// Endpoint of the major axis (set in step 2).
    major_end: Option<Point2D>,
    /// Minor-to-major ratio (set in step 3, default 0.5).
    minor_ratio: Option<f64>,
}

impl EllipseCommand {
    /// Create a new ELLIPSE command.
    pub fn new() -> Self {
        Self {
            center: None,
            major_end: None,
            minor_ratio: None,
        }
    }

    /// Compute ellipse vertices. The major axis direction is defined by
    /// the vector from `center` to `major_end`.
    fn compute_vertices(
        center: Point2D,
        major_end: Point2D,
        minor_ratio: f64,
        segments: usize,
    ) -> Vec<Point2D> {
        let major_radius = center.distance(major_end);
        let minor_radius = major_radius * minor_ratio;

        // Angle of the major axis from +X
        let dx = major_end.x - center.x;
        let dy = major_end.y - center.y;
        let angle = dy.atan2(dx);

        let cos_a = angle.cos();
        let sin_a = angle.sin();

        let n = segments as f64;
        (0..segments)
            .map(|i| {
                let theta = (i as f64) / n * TAU;
                let ex = major_radius * theta.cos();
                let ey = minor_radius * theta.sin();
                // Rotate by angle and translate to center
                Point2D::new(
                    center.x + ex * cos_a - ey * sin_a,
                    center.y + ex * sin_a + ey * cos_a,
                )
            })
            .collect()
    }
}

impl Default for EllipseCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl Command for EllipseCommand {
    fn name(&self) -> &'static str {
        "ELLIPSE"
    }

    fn prompt(&self) -> String {
        match (self.center, self.major_end) {
            (None, _) => "Specify center:".to_string(),
            (Some(_), None) => "Specify major axis endpoint:".to_string(),
            (Some(_), Some(_)) => "Specify minor ratio or point:".to_string(),
        }
    }

    fn steps_remaining(&self) -> usize {
        match (self.center, self.major_end) {
            (None, _) => 3,
            (Some(_), None) => 2,
            (Some(_), Some(_)) => 1,
        }
    }

    fn on_input(&mut self, input: CommandInput, world: &mut World) -> CommandResult {
        match input {
            CommandInput::Point(p) => {
                match (self.center, self.major_end) {
                    (None, _) => {
                        // Step 1: store center
                        self.center = Some(p);
                        CommandResult::Continue
                    }
                    (Some(_), None) => {
                        // Step 2: store major axis endpoint
                        self.major_end = Some(p);
                        CommandResult::Continue
                    }
                    (Some(center), Some(major_end)) => {
                    // Step 3: compute minor ratio from point distance
                    let major_radius = center.distance(major_end);
                    let point_distance = center.distance(p);
                    let ratio = if major_radius > 0.0 {
                        (point_distance / major_radius).clamp(0.01, 100.0)
                    } else {
                        0.5
                    };

                    let vertices = Self::compute_vertices(center, major_end, ratio, ELLIPSE_SEGMENTS);

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

                    let mut tx = Transaction::new("Ellipse".to_string());
                    tx.push(AtomicOp::SpawnPolyline {
                        entity,
                        data: world.get::<&PolylineData>(entity).as_deref().unwrap().clone(),
                    });

                    CommandResult::CompleteWithTransaction(tx)
                    }
                }
            }
            CommandInput::Distance(d) => {
                // Can be used for minor ratio in step 3
                if let (Some(center), Some(major_end)) = (self.center, self.major_end) {
                    let ratio = d.clamp(0.01, 100.0);

                    let vertices = Self::compute_vertices(center, major_end, ratio, ELLIPSE_SEGMENTS);

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

                    let mut tx = Transaction::new("Ellipse".to_string());
                    tx.push(AtomicOp::SpawnPolyline {
                        entity,
                        data: world.get::<&PolylineData>(entity).as_deref().unwrap().clone(),
                    });

                    CommandResult::CompleteWithTransaction(tx)
                } else {
                    CommandResult::Error("Set center and major axis first.".to_string())
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
        self.major_end = None;
        self.minor_ratio = None;
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
        let cmd = EllipseCommand::new();
        assert_eq!(cmd.name(), "ELLIPSE");
        assert_eq!(cmd.prompt(), "Specify center:");
        assert_eq!(cmd.steps_remaining(), 3);
    }

    #[test]
    fn test_first_point_sets_center() {
        let mut cmd = EllipseCommand::new();
        let mut world = create_world();

        let result = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);
        assert!(matches!(result, CommandResult::Continue));
        assert_eq!(cmd.prompt(), "Specify major axis endpoint:");
        assert_eq!(cmd.steps_remaining(), 2);
    }

    #[test]
    fn test_second_point_sets_major_end() {
        let mut cmd = EllipseCommand::new();
        let mut world = create_world();

        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);
        let result = cmd.on_input(CommandInput::Point(Point2D::new(50.0, 0.0)), &mut world);
        assert!(matches!(result, CommandResult::Continue));
        assert_eq!(cmd.prompt(), "Specify minor ratio or point:");
        assert_eq!(cmd.steps_remaining(), 1);
    }

    #[test]
    fn test_third_point_completes() {
        let mut cmd = EllipseCommand::new();
        let mut world = create_world();

        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(50.0, 0.0)), &mut world);
        let result = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 25.0)), &mut world);

        assert!(matches!(result, CommandResult::CompleteWithTransaction(_)));
    }

    #[test]
    fn test_vertex_count() {
        let mut cmd = EllipseCommand::new();
        let mut world = create_world();

        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(50.0, 0.0)), &mut world);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 25.0)), &mut world);

        for (_entity, poly) in world.query::<&PolylineData>().iter() {
            assert_eq!(poly.vertices.len(), ELLIPSE_SEGMENTS);
            assert!(poly.closed);
        }
    }

    #[test]
    fn test_distance_input_completes() {
        let mut cmd = EllipseCommand::new();
        let mut world = create_world();

        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(50.0, 0.0)), &mut world);
        let result = cmd.on_input(CommandInput::Distance(0.5), &mut world);

        assert!(matches!(result, CommandResult::CompleteWithTransaction(_)));

        for (_entity, poly) in world.query::<&PolylineData>().iter() {
            assert_eq!(poly.vertices.len(), ELLIPSE_SEGMENTS);
        }
    }

    #[test]
    fn test_distance_before_center_returns_error() {
        let mut cmd = EllipseCommand::new();
        let mut world = create_world();

        let result = cmd.on_input(CommandInput::Distance(0.5), &mut world);
        assert!(matches!(result, CommandResult::Error(_)));
    }

    #[test]
    fn test_cancel_clears_state() {
        let mut cmd = EllipseCommand::new();
        let mut world = create_world();

        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(50.0, 0.0)), &mut world);
        let result = cmd.on_input(CommandInput::Cancel, &mut world);
        assert!(matches!(result, CommandResult::Cancelled));
        assert_eq!(cmd.prompt(), "Specify center:");
    }

    #[test]
    fn test_invalid_input_returns_error() {
        let mut cmd = EllipseCommand::new();
        let mut world = create_world();

        let result = cmd.on_input(CommandInput::Text("hello".to_string()), &mut world);
        assert!(matches!(result, CommandResult::Error(_)));
    }

    #[test]
    fn test_circle_when_ratio_is_one() {
        let mut cmd = EllipseCommand::new();
        let mut world = create_world();

        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(50.0, 0.0)), &mut world);
        let _ = cmd.on_input(CommandInput::Distance(1.0), &mut world);

        // With ratio 1.0, all vertices should be at distance ~50 from center
        for (_entity, poly) in world.query::<&PolylineData>().iter() {
            for v in &poly.vertices {
                let dist = Point2D::new(0.0, 0.0).distance(*v);
                assert!((dist - 50.0).abs() < 1.0, "Vertex distance {}, expected ~50", dist);
            }
        }
    }
}
