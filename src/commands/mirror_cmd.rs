//! MIRROR command implementation.
//!
//! Reflects selected entities across a mirror line defined by two points.
//! Two-step interaction:
//! 1. User picks the first point of the mirror line.
//! 2. User picks the second point of the mirror line.
//!
//! Builds a [`Transaction`] with `Set*` ops for the history system.

use super::{Command, CommandInput, CommandResult, PreviewEntity};
use crate::ecs::components::*;
use crate::geometry::Point2D;
use crate::history::{AtomicOp, Transaction};
use crate::selection::SelectionManager;
use hecs::World;

/// Reflects selected entities across a mirror line.
///
/// Captures the selection at construction time. Two-step state machine:
/// - `point1 == None` → awaiting first point of mirror line
/// - `point1 == Some(p1)` → awaiting second point of mirror line
pub struct MirrorCommand {
    /// Entities to mirror (captured at construction time).
    selected_entities: Vec<hecs::Entity>,
    /// First point of the mirror line.
    point1: Option<Point2D>,
    /// Transaction populated on completion, consumed by caller via
    /// [`take_transaction`](MirrorCommand::take_transaction).
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

impl MirrorCommand {
    /// Create a new MIRROR command, capturing the current selection.
    ///
    /// The command will be empty if nothing was selected. Callers should
    /// check [`is_empty`](MirrorCommand::is_empty) before activating.
    pub fn new(selection: &SelectionManager) -> Self {
        Self {
            selected_entities: selection.selected.iter().copied().collect(),
            point1: None,
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

    /// Apply a reflection across the line through `p1` and `p2` to all
    /// captured entities.
    fn apply_mirror(&mut self, world: &mut World, p1: Point2D, p2: Point2D) -> Transaction {
        let count = self.selected_entities.len();
        let mut tx = Transaction::new(format!(
            "Mirror {} entit{}",
            count,
            if count == 1 { "y" } else { "ies" },
        ));

        // Compute line coefficients: a·x + b·y + c = 0
        let dx = p2.x - p1.x;
        let dy = p2.y - p1.y;
        let a = -dy;
        let b = dx;
        let c = -(a * p1.x + b * p1.y);
        let denom = a * a + b * b;

        for &entity in &self.selected_entities {
            let result = Self::read_mirror_entity(world, entity, a, b, c, denom);

            if let Some((old, new)) = result {
                // Write new data to world and record in transaction.
                match &new {
                    AtomicOpValue::Line(v) => {
                        if let Err(e) = world.insert_one(entity, *v) {
                            tracing::warn!("mirror: failed to update entity {:?}: {}", entity, e);
                        }
                    }
                    AtomicOpValue::Circle(v) => {
                        if let Err(e) = world.insert_one(entity, *v) {
                            tracing::warn!("mirror: failed to update entity {:?}: {}", entity, e);
                        }
                    }
                    AtomicOpValue::Arc(v) => {
                        if let Err(e) = world.insert_one(entity, *v) {
                            tracing::warn!("mirror: failed to update entity {:?}: {}", entity, e);
                        }
                    }
                    AtomicOpValue::Polyline(v) => {
                        if let Err(e) = world.insert_one(entity, v.clone()) {
                            tracing::warn!("mirror: failed to update entity {:?}: {}", entity, e);
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

    /// Reflect a single point across the line a·x + b·y + c = 0.
    fn reflect_point(p: Point2D, a: f64, b: f64, c: f64, denom: f64) -> Point2D {
        if denom < 1e-30 {
            // Degenerate line (p1 == p2) — return point unchanged.
            return p;
        }
        let d = (a * p.x + b * p.y + c) / denom;
        Point2D::new(
            p.x - 2.0 * a * d,
            p.y - 2.0 * b * d,
        )
    }

    /// Helper to read an entity's geometry, apply reflection, and return
    /// (old, new) as type-erased values.
    fn read_mirror_entity(
        world: &World,
        entity: hecs::Entity,
        a: f64,
        b: f64,
        c: f64,
        denom: f64,
    ) -> Option<(AtomicOpValue, AtomicOpValue)> {
        if let Ok(data) = world.get::<&LineData>(entity) {
            let old = *data;
            let new = LineData {
                start: Self::reflect_point(old.start, a, b, c, denom),
                end: Self::reflect_point(old.end, a, b, c, denom),
                ..old
            };
            Some((AtomicOpValue::Line(old), AtomicOpValue::Line(new)))
        } else if let Ok(data) = world.get::<&CircleData>(entity) {
            let old = *data;
            let new = CircleData {
                center: Self::reflect_point(old.center, a, b, c, denom),
                ..old
            };
            Some((AtomicOpValue::Circle(old), AtomicOpValue::Circle(new)))
        } else if let Ok(data) = world.get::<&ArcData>(entity) {
            let old = *data;
            let new = ArcData {
                center: Self::reflect_point(old.center, a, b, c, denom),
                ..old
            };
            Some((AtomicOpValue::Arc(old), AtomicOpValue::Arc(new)))
        } else if let Ok(data) = world.get::<&PolylineData>(entity) {
            let old: PolylineData = (&*data).clone();
            let new = PolylineData {
                vertices: old.vertices.iter()
                    .map(|v| Self::reflect_point(*v, a, b, c, denom))
                    .collect(),
                ..old.clone()
            };
            Some((AtomicOpValue::Polyline(old), AtomicOpValue::Polyline(new)))
        } else {
            None
        }
    }
}

impl Command for MirrorCommand {
    fn name(&self) -> &'static str {
        "MIRROR"
    }

    fn prompt(&self) -> String {
        if self.point1.is_none() {
            "Specify first point of mirror line:".to_string()
        } else {
            "Specify second point of mirror line:".to_string()
        }
    }

    fn steps_remaining(&self) -> usize {
        if self.point1.is_none() { 2 } else { 1 }
    }

    fn on_input(&mut self, input: CommandInput, world: &mut World) -> CommandResult {
        if self.selected_entities.is_empty() {
            return CommandResult::Error(
                "No entities selected. Select objects before running MIRROR.".to_string(),
            );
        }

        match input {
            CommandInput::Point(p) => {
                if self.point1.is_none() {
                    // Step 1: store first point.
                    self.point1 = Some(p);
                    CommandResult::Continue
                } else {
                    // Step 2: second point — mirror across line through both points.
                    let p1 = self.point1.unwrap();

                    self.pending_transaction =
                        Some(self.apply_mirror(world, p1, p));
                    CommandResult::Complete
                }
            }
            CommandInput::Confirm => {
                CommandResult::Error(
                    "Specify two points to define the mirror line.".to_string(),
                )
            }
            CommandInput::Cancel => {
                self.on_cancel(world);
                CommandResult::Cancelled
            }
            _ => CommandResult::Error("Specify a point.".to_string()),
        }
    }

    fn on_cancel(&mut self, _world: &mut World) {
        self.point1 = None;
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

        let cmd = MirrorCommand::new(&sel);
        assert_eq!(cmd.selected_entities.len(), 1);
        assert!(!cmd.is_empty());
    }

    #[test]
    fn new_with_empty_selection_is_empty() {
        let sel = SelectionManager::new();
        let cmd = MirrorCommand::new(&sel);
        assert!(cmd.is_empty());
    }

    #[test]
    fn name_and_prompt() {
        let sel = SelectionManager::new();
        let cmd = MirrorCommand::new(&sel);
        assert_eq!(cmd.name(), "MIRROR");
        assert_eq!(cmd.prompt(), "Specify first point of mirror line:");
        assert_eq!(cmd.steps_remaining(), 2);
    }

    #[test]
    fn prompt_after_first_point() {
        let mut world = World::new();
        let e = make_line(&mut world, Point2D::new(0.0, 0.0), Point2D::new(10.0, 0.0));

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let mut cmd = MirrorCommand::new(&sel);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);

        assert_eq!(cmd.prompt(), "Specify second point of mirror line:");
        assert_eq!(cmd.steps_remaining(), 1);
    }

    // -- reflect across y-axis ---------------------------------------------

    #[test]
    fn mirror_point_across_y_axis() {
        let mut world = World::new();
        // Point (1, 0) — create a zero-length line at (1,0)
        let e = make_line(&mut world, Point2D::new(1.0, 0.0), Point2D::new(1.0, 0.0));

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let mut cmd = MirrorCommand::new(&sel);

        // Step 1: first point of mirror line at origin
        let r1 = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);
        assert!(matches!(r1, CommandResult::Continue));

        // Step 2: second point at (0, 1) → y-axis
        let r2 = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 1.0)), &mut world);
        assert!(matches!(r2, CommandResult::Complete));

        let line = world.get::<&LineData>(e).unwrap();
        assert!((line.start.x - (-1.0)).abs() < 1e-10, "start x ≈ -1, got {}", line.start.x);
        assert!((line.start.y - 0.0).abs() < 1e-10, "start y ≈ 0");
    }

    // -- reflect across x-axis ---------------------------------------------

    #[test]
    fn mirror_point_across_x_axis() {
        let mut world = World::new();
        let e = make_line(&mut world, Point2D::new(0.0, 1.0), Point2D::new(0.0, 1.0));

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let mut cmd = MirrorCommand::new(&sel);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(1.0, 0.0)), &mut world);

        let line = world.get::<&LineData>(e).unwrap();
        assert!((line.start.x - 0.0).abs() < 1e-10, "start x ≈ 0");
        assert!((line.start.y - (-1.0)).abs() < 1e-10, "start y ≈ -1, got {}", line.start.y);
    }

    // -- reflect line across arbitrary axis --------------------------------

    #[test]
    fn mirror_line_across_diagonal() {
        let mut world = World::new();
        // Horizontal line from (0,0) to (2,0)
        let e = make_line(&mut world, Point2D::new(0.0, 0.0), Point2D::new(2.0, 0.0));

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let mut cmd = MirrorCommand::new(&sel);
        // Mirror line: y = x (diagonal through origin)
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(1.0, 1.0)), &mut world);

        let line = world.get::<&LineData>(e).unwrap();
        // (0,0) on mirror line → unchanged; (2,0) → (0,2)
        assert!((line.start.x - 0.0).abs() < 1e-10, "start x ≈ 0");
        assert!((line.start.y - 0.0).abs() < 1e-10, "start y ≈ 0");
        assert!((line.end.x - 0.0).abs() < 1e-10, "end x ≈ 0, got {}", line.end.x);
        assert!((line.end.y - 2.0).abs() < 1e-10, "end y ≈ 2, got {}", line.end.y);
    }

    // -- reflect circle ----------------------------------------------------

    #[test]
    fn mirror_circle_center_reflects() {
        let mut world = World::new();
        let e = make_circle(&mut world, Point2D::new(3.0, 0.0), 2.0);

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let mut cmd = MirrorCommand::new(&sel);
        // Mirror across y-axis
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 1.0)), &mut world);

        let circle = world.get::<&CircleData>(e).unwrap();
        assert!((circle.center.x - (-3.0)).abs() < 1e-10, "center x ≈ -3, got {}", circle.center.x);
        assert!((circle.center.y - 0.0).abs() < 1e-10, "center y ≈ 0");
        assert_eq!(circle.radius, 2.0, "radius unchanged");
    }

    // -- reflect polyline --------------------------------------------------

    #[test]
    fn mirror_polyline_all_vertices_reflect() {
        let mut world = World::new();
        let e = make_polyline(&mut world);

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let mut cmd = MirrorCommand::new(&sel);
        // Mirror across x-axis: p1=(0,0), p2=(1,0)
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(1.0, 0.0)), &mut world);

        let poly = world.get::<&PolylineData>(e).unwrap();
        // (0,0) on axis → (0,0); (5,5) → (5,-5); (10,0) → (10,0)
        assert!((poly.vertices[0].x - 0.0).abs() < 1e-10);
        assert!((poly.vertices[0].y - 0.0).abs() < 1e-10);
        assert!((poly.vertices[1].x - 5.0).abs() < 1e-10);
        assert!((poly.vertices[1].y - (-5.0)).abs() < 1e-10, "v1 y ≈ -5, got {}", poly.vertices[1].y);
        assert!((poly.vertices[2].x - 10.0).abs() < 1e-10);
        assert!((poly.vertices[2].y - 0.0).abs() < 1e-10);
    }

    // -- empty selection ---------------------------------------------------

    #[test]
    fn mirror_with_empty_selection_returns_error() {
        let mut world = World::new();
        let sel = SelectionManager::new();

        let mut cmd = MirrorCommand::new(&sel);
        let result = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);
        assert!(matches!(result, CommandResult::Error(_)));
    }

    // -- cancel ------------------------------------------------------------

    #[test]
    fn mirror_cancel_clears_state() {
        let mut world = World::new();
        let e = make_line(&mut world, Point2D::new(1.0, 0.0), Point2D::new(1.0, 0.0));

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let mut cmd = MirrorCommand::new(&sel);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(5.0, 5.0)), &mut world);

        let result = cmd.on_input(CommandInput::Cancel, &mut world);
        assert!(matches!(result, CommandResult::Cancelled));
        assert!(cmd.point1.is_none());
        assert!(cmd.take_transaction().is_none());

        let line = world.get::<&LineData>(e).unwrap();
        assert_eq!(line.start.x, 1.0, "entity unchanged after cancel");
    }

    // -- transaction -------------------------------------------------------

    #[test]
    fn mirror_transaction_contains_old_and_new() {
        let mut world = World::new();
        let e = make_line(&mut world, Point2D::new(1.0, 0.0), Point2D::new(1.0, 0.0));

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let old_line = *world.get::<&LineData>(e).unwrap();

        let mut cmd = MirrorCommand::new(&sel);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 1.0)), &mut world);

        let tx = cmd.take_transaction().unwrap();
        assert_eq!(tx.ops.len(), 1);

        match &tx.ops[0] {
            AtomicOp::SetLineData { entity, old, new } => {
                assert_eq!(*entity, e);
                assert_eq!(old.start, old_line.start);
                assert!((new.start.x - (-1.0)).abs() < 1e-10, "new x ≈ -1");
            }
            other => panic!("expected SetLineData, got {other:?}"),
        }
    }
}
