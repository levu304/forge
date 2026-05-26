//! ROTATE command implementation.
//!
//! Rotates selected entities around a base point by a specified angle.
//! Two-step interaction:
//! 1. User picks a base point (center of rotation).
//! 2. User provides an angle (via text input in degrees, or a second
//!    point to derive the angle from).
//!
//! Builds a [`Transaction`] with `Set*` ops for the history system.

use super::{Command, CommandInput, CommandResult, PreviewEntity};
use crate::ecs::components::*;
use crate::geometry::Point2D;
use crate::history::{AtomicOp, Transaction};
use crate::selection::SelectionManager;
use hecs::World;
use std::f64::consts::PI;

/// Rotates selected entities around a base point by a specified angle.
///
/// Captures the selection at construction time. Two-step state machine:
/// - `base_point == None` → awaiting base point
/// - `base_point == Some(p)` → awaiting angle (text degrees or second point)
pub struct RotateCommand {
    /// Entities to rotate (captured at construction time).
    selected_entities: Vec<hecs::Entity>,
    /// Center of rotation.
    base_point: Option<Point2D>,
    /// Transaction populated on completion, consumed by caller via
    /// [`take_transaction`](RotateCommand::take_transaction).
    pending_transaction: Option<Transaction>,
}

/// Type-erased geometry value for holding old/new pairs without
/// borrowing the ECS world.
enum AtomicOpValue {
    Line(LineData),
    Circle(CircleData),
    Arc(ArcData),
    Polyline(PolylineData),
}

impl RotateCommand {
    /// Create a new ROTATE command, capturing the current selection.
    ///
    /// The command will be empty if nothing was selected. Callers should
    /// check [`is_empty`](RotateCommand::is_empty) before activating.
    pub fn new(selection: &SelectionManager) -> Self {
        Self {
            selected_entities: selection.selected.iter().copied().collect(),
            base_point: None,
            pending_transaction: None,
        }
    }

    /// Returns `true` if no entities were captured (empty selection).
    pub fn is_empty(&self) -> bool {
        self.selected_entities.is_empty()
    }

    /// Consume the pending transaction after command completion.
    ///
    /// Returns `None` if the command has not completed yet.
    pub fn take_transaction(&mut self) -> Option<Transaction> {
        self.pending_transaction.take()
    }

    /// Apply a rotation of `angle_deg` degrees around `center` to all
    /// captured entities.
    fn apply_rotation(&mut self, world: &mut World, center: Point2D, angle_deg: f64) -> Transaction {
        let count = self.selected_entities.len();
        let mut tx = Transaction::new(format!(
            "Rotate {} entit{}",
            count,
            if count == 1 { "y" } else { "ies" },
        ));

        let angle_rad = angle_deg * PI / 180.0;
        let cos_a = angle_rad.cos();
        let sin_a = angle_rad.sin();

        for &entity in &self.selected_entities {
            let result = Self::read_rotate_entity(world, entity, center, cos_a, sin_a, angle_deg);

            if let Some((old, new)) = result {
                // Write new data to world and record in transaction.
                match &new {
                    AtomicOpValue::Line(v) => {
                        if let Err(e) = world.insert_one(entity, *v) {
                            tracing::warn!("rotate: failed to update entity {:?}: {}", entity, e);
                        }
                    }
                    AtomicOpValue::Circle(v) => {
                        if let Err(e) = world.insert_one(entity, *v) {
                            tracing::warn!("rotate: failed to update entity {:?}: {}", entity, e);
                        }
                    }
                    AtomicOpValue::Arc(v) => {
                        if let Err(e) = world.insert_one(entity, *v) {
                            tracing::warn!("rotate: failed to update entity {:?}: {}", entity, e);
                        }
                    }
                    AtomicOpValue::Polyline(v) => {
                        if let Err(e) = world.insert_one(entity, v.clone()) {
                            tracing::warn!("rotate: failed to update entity {:?}: {}", entity, e);
                        }
                    }
                }

                match (old, new) {
                    (AtomicOpValue::Line(old), AtomicOpValue::Line(new)) => {
                        tx.push(AtomicOp::SetLineData { entity, old, new });
                    }
                    (AtomicOpValue::Circle(old), AtomicOpValue::Circle(new)) => {
                        tx.push(AtomicOp::SetCircleData { entity, old, new });
                    }
                    (AtomicOpValue::Arc(old), AtomicOpValue::Arc(new)) => {
                        tx.push(AtomicOp::SetArcData { entity, old, new });
                    }
                    (AtomicOpValue::Polyline(old), AtomicOpValue::Polyline(new)) => {
                        tx.push(AtomicOp::SetPolylineData { entity, old, new });
                    }
                    _ => {}
                }
            }
        }

        tx
    }

    /// Rotate a single point around `center` by `(cos_a, sin_a)`.
    fn rotate_point(p: Point2D, center: Point2D, cos_a: f64, sin_a: f64) -> Point2D {
        let dx = p.x - center.x;
        let dy = p.y - center.y;
        Point2D::new(
            center.x + dx * cos_a - dy * sin_a,
            center.y + dx * sin_a + dy * cos_a,
        )
    }

    /// Helper to read an entity's geometry, apply rotation, and return
    /// (old, new) as type-erased values.
    ///
    /// `angle_deg` is the rotation angle in degrees, used for arc angle offsets.
    fn read_rotate_entity(
        world: &World,
        entity: hecs::Entity,
        center: Point2D,
        cos_a: f64,
        sin_a: f64,
        angle_deg: f64,
    ) -> Option<(AtomicOpValue, AtomicOpValue)> {
        if let Ok(data) = world.get::<&LineData>(entity) {
            let old = *data;
            let new = LineData {
                start: Self::rotate_point(old.start, center, cos_a, sin_a),
                end: Self::rotate_point(old.end, center, cos_a, sin_a),
                ..old
            };
            Some((AtomicOpValue::Line(old), AtomicOpValue::Line(new)))
        } else if let Ok(data) = world.get::<&CircleData>(entity) {
            let old = *data;
            let new = CircleData {
                center: Self::rotate_point(old.center, center, cos_a, sin_a),
                ..old
            };
            Some((AtomicOpValue::Circle(old), AtomicOpValue::Circle(new)))
        } else if let Ok(data) = world.get::<&ArcData>(entity) {
            let old = *data;
            let new = ArcData {
                center: Self::rotate_point(old.center, center, cos_a, sin_a),
                start_angle: old.start_angle + angle_deg,
                end_angle: old.end_angle + angle_deg,
                ..old
            };
            Some((AtomicOpValue::Arc(old), AtomicOpValue::Arc(new)))
        } else if let Ok(data) = world.get::<&PolylineData>(entity) {
            let old: PolylineData = (&*data).clone();
            let new = PolylineData {
                vertices: old.vertices.iter()
                    .map(|v| Self::rotate_point(*v, center, cos_a, sin_a))
                    .collect(),
                ..old.clone()
            };
            Some((AtomicOpValue::Polyline(old), AtomicOpValue::Polyline(new)))
        } else {
            None
        }
    }
}

impl Command for RotateCommand {
    fn name(&self) -> &'static str {
        "ROTATE"
    }

    fn prompt(&self) -> String {
        if self.base_point.is_none() {
            "Specify base point:".to_string()
        } else {
            "Specify rotation angle or second point:".to_string()
        }
    }

    fn steps_remaining(&self) -> usize {
        if self.base_point.is_none() { 2 } else { 1 }
    }

    fn on_input(&mut self, input: CommandInput, world: &mut World) -> CommandResult {
        if self.selected_entities.is_empty() {
            return CommandResult::Error(
                "No entities selected. Select objects before running ROTATE.".to_string(),
            );
        }

        match input {
            CommandInput::Point(p) => {
                if self.base_point.is_none() {
                    // Step 1: store base point.
                    self.base_point = Some(p);
                    CommandResult::Continue
                } else {
                    // Step 2: compute angle from base point to second point.
                    let base = self.base_point.unwrap();
                    let dx = p.x - base.x;
                    let dy = p.y - base.y;
                    let angle_deg = dy.atan2(dx) * 180.0 / PI;

                    self.pending_transaction =
                        Some(self.apply_rotation(world, base, angle_deg));
                    CommandResult::Complete
                }
            }
            CommandInput::Text(s) => {
                if self.base_point.is_none() {
                    return CommandResult::Error(
                        "Specify a base point first.".to_string(),
                    );
                }

                // Parse angle from text (degrees).
                let angle_deg: f64 = match s.parse() {
                    Ok(v) => v,
                    Err(_) => {
                        return CommandResult::Error(
                            format!("Invalid angle: '{s}'. Enter a number in degrees."),
                        );
                    }
                };

                let base = self.base_point.unwrap();
                self.pending_transaction =
                    Some(self.apply_rotation(world, base, angle_deg));
                CommandResult::Complete
            }
            CommandInput::Confirm => {
                CommandResult::Error(
                    "Specify a rotation angle or second point.".to_string(),
                )
            }
            CommandInput::Cancel => {
                self.on_cancel(world);
                CommandResult::Cancelled
            }
            _ => CommandResult::Error("Specify an angle or point.".to_string()),
        }
    }

    fn on_cancel(&mut self, _world: &mut World) {
        self.base_point = None;
        self.pending_transaction = None;
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
    use crate::selection::SelectionManager;
    use crate::util::Color;
    use hecs::World;

    // -- helpers -----------------------------------------------------------

    fn make_line(world: &mut World, start: Point2D, end: Point2D) -> hecs::Entity {
        world.spawn((
            LineData { start, end, color: Color::WHITE, width: 1.0 },
            Renderable,
        ))
    }

    fn make_circle(world: &mut World, center: Point2D, radius: f64) -> hecs::Entity {
        world.spawn((
            CircleData { center, radius, color: Color::WHITE, width: 1.0 },
            Renderable,
        ))
    }

    fn make_arc(world: &mut World, center: Point2D) -> hecs::Entity {
        world.spawn((
            ArcData {
                center,
                radius: 5.0,
                start_angle: 0.0,
                end_angle: 90.0,
                color: Color::WHITE,
                width: 1.0,
            },
            Renderable,
        ))
    }

    fn make_polyline(world: &mut World) -> hecs::Entity {
        world.spawn((
            PolylineData {
                vertices: vec![
                    Point2D::new(0.0, 0.0),
                    Point2D::new(5.0, 5.0),
                    Point2D::new(10.0, 0.0),
                ],
                closed: false,
                color: Color::WHITE,
                width: 1.0,
            },
            Renderable,
        ))
    }

    // -- construction ------------------------------------------------------

    #[test]
    fn new_captures_selection() {
        let mut world = World::new();
        let e = make_line(&mut world, Point2D::new(0.0, 0.0), Point2D::new(10.0, 0.0));

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let cmd = RotateCommand::new(&sel);
        assert_eq!(cmd.selected_entities.len(), 1);
        assert!(!cmd.is_empty());
    }

    #[test]
    fn new_with_empty_selection_is_empty() {
        let sel = SelectionManager::new();
        let cmd = RotateCommand::new(&sel);
        assert!(cmd.is_empty());
    }

    #[test]
    fn name_and_prompt() {
        let sel = SelectionManager::new();
        let cmd = RotateCommand::new(&sel);
        assert_eq!(cmd.name(), "ROTATE");
        assert_eq!(cmd.prompt(), "Specify base point:");
        assert_eq!(cmd.steps_remaining(), 2);
    }

    // -- rotation via text angle -------------------------------------------

    #[test]
    fn rotate_90_degrees_line_around_origin() {
        let mut world = World::new();
        let e = make_line(&mut world, Point2D::new(0.0, 0.0), Point2D::new(10.0, 0.0));

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let mut cmd = RotateCommand::new(&sel);

        // Step 1: base point at origin
        let r1 = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);
        assert!(matches!(r1, CommandResult::Continue));

        // Step 2: angle = 90 degrees
        let r2 = cmd.on_input(CommandInput::Text("90".to_string()), &mut world);
        assert!(matches!(r2, CommandResult::Complete));

        let line = world.get::<&LineData>(e).unwrap();
        assert!((line.start.x - 0.0).abs() < 1e-10, "start x (0)");
        assert!((line.start.y - 0.0).abs() < 1e-10, "start y (0)");
        assert!((line.end.x - 0.0).abs() < 1e-10, "end x ≈ 0");
        assert!((line.end.y - 10.0).abs() < 1e-10, "end y ≈ 10 (90° rotation)");
    }

    #[test]
    fn rotate_45_degrees_point_around_origin() {
        let mut world = World::new();
        let e = make_line(&mut world, Point2D::new(0.0, 0.0), Point2D::new(1.0, 0.0));

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let mut cmd = RotateCommand::new(&sel);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);
        let _ = cmd.on_input(CommandInput::Text("45".to_string()), &mut world);

        let line = world.get::<&LineData>(e).unwrap();
        let expected = 2.0_f64.sqrt() / 2.0; // cos(45°) = sin(45°) ≈ 0.7071
        assert!((line.end.x - expected).abs() < 1e-10, "end x ≈ 0.7071, got {}", line.end.x);
        assert!((line.end.y - expected).abs() < 1e-10, "end y ≈ 0.7071, got {}", line.end.y);
    }

    #[test]
    fn rotate_preserves_color_and_width() {
        let mut world = World::new();
        let e = make_line(&mut world, Point2D::new(0.0, 0.0), Point2D::new(10.0, 0.0));

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let mut cmd = RotateCommand::new(&sel);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);
        let _ = cmd.on_input(CommandInput::Text("90".to_string()), &mut world);

        let line = world.get::<&LineData>(e).unwrap();
        assert_eq!(line.color, Color::WHITE);
        assert_eq!(line.width, 1.0);
    }

    // -- rotation via second point -----------------------------------------

    #[test]
    fn rotate_via_second_point() {
        let mut world = World::new();
        let e = make_line(&mut world, Point2D::new(0.0, 0.0), Point2D::new(10.0, 0.0));

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let mut cmd = RotateCommand::new(&sel);

        // Step 1: base point at origin
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);

        // Step 2: second point at (0, 1) → angle = 90° from base
        let r2 = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 1.0)), &mut world);
        assert!(matches!(r2, CommandResult::Complete));

        let line = world.get::<&LineData>(e).unwrap();
        assert!((line.end.x - 0.0).abs() < 1e-10, "end x ≈ 0");
        assert!((line.end.y - 10.0).abs() < 1e-10, "end y ≈ 10");
    }

    // -- circle rotation ---------------------------------------------------

    #[test]
    fn rotate_circle_center_rotates_radius_unchanged() {
        let mut world = World::new();
        let e = make_circle(&mut world, Point2D::new(5.0, 0.0), 3.0);

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let mut cmd = RotateCommand::new(&sel);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);
        let _ = cmd.on_input(CommandInput::Text("90".to_string()), &mut world);

        let circle = world.get::<&CircleData>(e).unwrap();
        assert!((circle.center.x - 0.0).abs() < 1e-10, "center x rotated to ~0");
        assert!((circle.center.y - 5.0).abs() < 1e-10, "center y rotated to ~5");
        assert_eq!(circle.radius, 3.0, "radius unchanged");
    }

    // -- arc rotation ------------------------------------------------------

    #[test]
    fn rotate_arc_center_and_angles_rotate() {
        let mut world = World::new();
        let e = make_arc(&mut world, Point2D::new(5.0, 0.0));

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let mut cmd = RotateCommand::new(&sel);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);
        let _ = cmd.on_input(CommandInput::Text("90".to_string()), &mut world);

        let arc = world.get::<&ArcData>(e).unwrap();
        assert!((arc.center.x - 0.0).abs() < 1e-10, "center x rotated to ~0");
        assert!((arc.center.y - 5.0).abs() < 1e-10, "center y rotated to ~5");
        assert!((arc.start_angle - 90.0).abs() < 1e-10, "start_angle rotated by 90°, got {}", arc.start_angle);
        assert!((arc.end_angle - 180.0).abs() < 1e-10, "end_angle rotated by 90°, got {}", arc.end_angle);
    }

    // -- polyline rotation -------------------------------------------------

    #[test]
    fn rotate_polyline_all_vertices_rotate() {
        let mut world = World::new();
        let e = make_polyline(&mut world);

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let mut cmd = RotateCommand::new(&sel);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);
        let _ = cmd.on_input(CommandInput::Text("90".to_string()), &mut world);

        let poly = world.get::<&PolylineData>(e).unwrap();
        // (0,0) stays at (0,0); (5,5) → (-5,5); (10,0) → (0,10)
        assert!((poly.vertices[0].x - 0.0).abs() < 1e-10);
        assert!((poly.vertices[0].y - 0.0).abs() < 1e-10);
        assert!((poly.vertices[1].x - (-5.0)).abs() < 1e-10, "v1 x ≈ -5, got {}", poly.vertices[1].x);
        assert!((poly.vertices[1].y - 5.0).abs() < 1e-10, "v1 y ≈ 5, got {}", poly.vertices[1].y);
        assert!((poly.vertices[2].x - 0.0).abs() < 1e-10, "v2 x ≈ 0, got {}", poly.vertices[2].x);
        assert!((poly.vertices[2].y - 10.0).abs() < 1e-10, "v2 y ≈ 10, got {}", poly.vertices[2].y);
    }

    // -- empty selection ---------------------------------------------------

    #[test]
    fn rotate_with_empty_selection_returns_error() {
        let mut world = World::new();
        let sel = SelectionManager::new();

        let mut cmd = RotateCommand::new(&sel);
        let result = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);
        assert!(matches!(result, CommandResult::Error(_)));
    }

    // -- invalid text ------------------------------------------------------

    #[test]
    fn rotate_invalid_text_returns_error() {
        let mut world = World::new();
        let e = make_line(&mut world, Point2D::new(0.0, 0.0), Point2D::new(10.0, 0.0));

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let mut cmd = RotateCommand::new(&sel);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);

        let result = cmd.on_input(CommandInput::Text("not_a_number".to_string()), &mut world);
        assert!(matches!(result, CommandResult::Error(_)));
    }

    // -- cancel ------------------------------------------------------------

    #[test]
    fn rotate_cancel_clears_state() {
        let mut world = World::new();
        let e = make_line(&mut world, Point2D::new(0.0, 0.0), Point2D::new(10.0, 0.0));

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let mut cmd = RotateCommand::new(&sel);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(5.0, 5.0)), &mut world);

        let result = cmd.on_input(CommandInput::Cancel, &mut world);
        assert!(matches!(result, CommandResult::Cancelled));
        assert!(cmd.base_point.is_none());
        assert!(cmd.take_transaction().is_none());

        // Entity unchanged
        let line = world.get::<&LineData>(e).unwrap();
        assert_eq!(line.end, Point2D::new(10.0, 0.0));
    }

    // -- transaction -------------------------------------------------------

    #[test]
    fn rotate_transaction_contains_old_and_new_data() {
        let mut world = World::new();
        let e = make_line(&mut world, Point2D::new(0.0, 0.0), Point2D::new(10.0, 0.0));

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let old_line = *world.get::<&LineData>(e).unwrap();

        let mut cmd = RotateCommand::new(&sel);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);
        let _ = cmd.on_input(CommandInput::Text("90".to_string()), &mut world);

        let tx = cmd.take_transaction().unwrap();
        assert_eq!(tx.ops.len(), 1);

        match &tx.ops[0] {
            AtomicOp::SetLineData { entity, old, new } => {
                assert_eq!(*entity, e);
                assert_eq!(old.start, old_line.start);
                assert_eq!(old.end, old_line.end);
                assert!((new.end.x - 0.0).abs() < 1e-10, "new end x ≈ 0");
                assert!((new.end.y - 10.0).abs() < 1e-10, "new end y ≈ 10");
            }
            other => panic!("expected SetLineData, got {other:?}"),
        }
    }
}
