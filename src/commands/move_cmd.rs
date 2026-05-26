//! MOVE command implementation.
//!
//! Displaces selected entities by a vector. Two-step interaction:
//! 1. User picks a base point (or types relative displacement).
//! 2. User picks a second point, or presses Enter to use the base point
//!    as a relative displacement from origin.
//!
//! Builds a [`Transaction`] with `Set*` ops capturing both old and new
//! geometry data for the history system.

use super::{Command, CommandInput, CommandResult, PreviewEntity};
use crate::ecs::components::*;
use crate::geometry::Point2D;
use crate::history::{AtomicOp, Transaction};
use crate::selection::SelectionManager;
use hecs::World;

/// Displaces selected entities by a vector.
///
/// Captures the selection at construction time. Two-step state machine:
/// - `base_point == None` → awaiting first point
/// - `base_point == Some(p)` → awaiting second point (or Enter for relative)
pub struct MoveCommand {
    /// Entities to move (captured at construction time).
    selected_entities: Vec<hecs::Entity>,
    /// First point picked by the user (base point).
    base_point: Option<Point2D>,
    /// Transaction populated on completion, consumed by caller via
    /// [`take_transaction`](MoveCommand::take_transaction).
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

impl MoveCommand {
    /// Create a new MOVE command, capturing the current selection.
    ///
    /// The command will be empty if nothing was selected. Callers should
    /// check [`is_empty`](MoveCommand::is_empty) before activating.
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

    /// Apply a displacement vector to all captured entities.
    ///
    /// Reads each entity's current geometry data, applies the displacement,
    /// writes the new data back to the ECS world, and records both `old` and
    /// `new` values in a `Transaction`.
    fn apply_displacement(&mut self, world: &mut World, displacement: Point2D) -> Transaction {
        let count = self.selected_entities.len();
        let mut tx = Transaction::new(format!(
            "Move {} entit{}",
            count,
            if count == 1 { "y" } else { "ies" },
        ));

        for &entity in &self.selected_entities {
            // Read data first (may borrow `world` immutably), then mutate.
            // Using a separate scope to drop the Ref before the mutable borrow.
            let result = Self::read_entity_data(world, entity, displacement);

            if let Some((old, new)) = result {
                // Write new data to world and record in transaction.
                // Polylines need clone because they don't implement Copy.
                match &new {
                    AtomicOpValue::Line(v) => {
                        if let Err(e) = world.insert_one(entity, *v) {
                            tracing::warn!("move: failed to update entity {:?}: {}", entity, e);
                        }
                    }
                    AtomicOpValue::Circle(v) => {
                        if let Err(e) = world.insert_one(entity, *v) {
                            tracing::warn!("move: failed to update entity {:?}: {}", entity, e);
                        }
                    }
                    AtomicOpValue::Arc(v) => {
                        if let Err(e) = world.insert_one(entity, *v) {
                            tracing::warn!("move: failed to update entity {:?}: {}", entity, e);
                        }
                    }
                    AtomicOpValue::Polyline(v) => {
                        if let Err(e) = world.insert_one(entity, v.clone()) {
                            tracing::warn!("move: failed to update entity {:?}: {}", entity, e);
                        }
                    }
                }

                // Build the appropriate AtomicOp variant.
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
                    _ => {
                        // Should never happen — old and new always match on type.
                    }
                }
            }
            // Entities without a recognised geometry type are silently skipped.
        }

        tx
    }

    /// Helper to read an entity's geometry, apply displacement, and return
    /// (old, new) as type-erased values. Returns `None` if the entity has
    /// no recognised geometry component.
    fn read_entity_data(
        world: &World,
        entity: hecs::Entity,
        displacement: Point2D,
    ) -> Option<(AtomicOpValue, AtomicOpValue)> {
        if let Ok(data) = world.get::<&LineData>(entity) {
            let old = *data;
            let new = LineData {
                start: old.start + displacement,
                end: old.end + displacement,
                ..old
            };
            Some((AtomicOpValue::Line(old), AtomicOpValue::Line(new)))
        } else if let Ok(data) = world.get::<&CircleData>(entity) {
            let old = *data;
            let new = CircleData {
                center: old.center + displacement,
                ..old
            };
            Some((AtomicOpValue::Circle(old), AtomicOpValue::Circle(new)))
        } else if let Ok(data) = world.get::<&ArcData>(entity) {
            let old = *data;
            let new = ArcData {
                center: old.center + displacement,
                ..old
            };
            Some((AtomicOpValue::Arc(old), AtomicOpValue::Arc(new)))
        } else if let Ok(data) = world.get::<&PolylineData>(entity) {
            let old: PolylineData = (&*data).clone();
            let new = PolylineData {
                vertices: old.vertices.iter().map(|v| *v + displacement).collect(),
                ..old.clone()
            };
            Some((AtomicOpValue::Polyline(old), AtomicOpValue::Polyline(new)))
        } else {
            None
        }
    }
}

impl Command for MoveCommand {
    fn name(&self) -> &'static str {
        "MOVE"
    }

    fn prompt(&self) -> String {
        if self.base_point.is_none() {
            "Specify base point or displacement:".to_string()
        } else {
            "Specify second point or <use first point as displacement>:".to_string()
        }
    }

    fn steps_remaining(&self) -> usize {
        if self.base_point.is_none() { 2 } else { 1 }
    }

    fn on_input(&mut self, input: CommandInput, world: &mut World) -> CommandResult {
        match input {
            CommandInput::Point(p) => {
                if self.selected_entities.is_empty() {
                    return CommandResult::Error(
                        "No entities selected. Select objects before running MOVE."
                            .to_string(),
                    );
                }

                if self.base_point.is_none() {
                    // Step 1: store base point.
                    self.base_point = Some(p);
                    CommandResult::Continue
                } else {
                    // Step 2: compute displacement and apply.
                    let base = self.base_point.unwrap();
                    let displacement = p - base;

                    self.pending_transaction =
                        Some(self.apply_displacement(world, displacement));
                    CommandResult::Complete
                }
            }
            CommandInput::Confirm => {
                if self.selected_entities.is_empty() {
                    return CommandResult::Error(
                        "No entities selected. Select objects before running MOVE."
                            .to_string(),
                    );
                }

                if let Some(base) = self.base_point {
                    // Use base point as relative displacement from origin.
                    let displacement = base;
                    self.pending_transaction =
                        Some(self.apply_displacement(world, displacement));
                    CommandResult::Complete
                } else {
                    CommandResult::Error(
                        "Specify a base point first.".to_string(),
                    )
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

    fn make_line(world: &mut World) -> hecs::Entity {
        world.spawn((
            LineData {
                start: Point2D::new(0.0, 0.0),
                end: Point2D::new(10.0, 10.0),
                color: Color::WHITE,
                width: 1.0,
            },
            Renderable,
        ))
    }

    fn make_circle(world: &mut World, center: Point2D, radius: f64) -> hecs::Entity {
        world.spawn((
            CircleData {
                center,
                radius,
                color: Color::WHITE,
                width: 1.0,
            },
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
        let e = make_line(&mut world);

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let cmd = MoveCommand::new(&sel);
        assert_eq!(cmd.selected_entities.len(), 1);
        assert!(!cmd.is_empty());
    }

    #[test]
    fn new_with_empty_selection_is_empty() {
        let sel = SelectionManager::new();
        let cmd = MoveCommand::new(&sel);
        assert!(cmd.is_empty());
        assert!(cmd.base_point.is_none());
    }

    #[test]
    fn name_and_prompt() {
        let sel = SelectionManager::new();
        let cmd = MoveCommand::new(&sel);
        assert_eq!(cmd.name(), "MOVE");
        assert_eq!(cmd.prompt(), "Specify base point or displacement:");
        assert_eq!(cmd.steps_remaining(), 2);
    }

    #[test]
    fn prompt_after_base_point() {
        let mut world = World::new();
        let e = make_line(&mut world);

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let mut cmd = MoveCommand::new(&sel);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(5.0, 5.0)), &mut world);

        assert_eq!(cmd.prompt(), "Specify second point or <use first point as displacement>:");
        assert_eq!(cmd.steps_remaining(), 1);
    }

    // -- two-point move ----------------------------------------------------

    #[test]
    fn move_command_two_points_displaces_line() {
        let mut world = World::new();
        let e = make_line(&mut world);

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let mut cmd = MoveCommand::new(&sel);

        // Step 1: base point
        let r1 = cmd.on_input(CommandInput::Point(Point2D::new(1.0, 1.0)), &mut world);
        assert!(matches!(r1, CommandResult::Continue));

        // Step 2: second point → displacement = (2,2) - (1,1) = (1,1)
        let r2 = cmd.on_input(CommandInput::Point(Point2D::new(2.0, 2.0)), &mut world);
        assert!(matches!(r2, CommandResult::Complete));

        // Verify geometry was displaced
        let line = world.get::<&LineData>(e).unwrap();
        assert_eq!(line.start, Point2D::new(1.0, 1.0), "start should be displaced by (1,1)");
        assert_eq!(line.end, Point2D::new(11.0, 11.0), "end should be displaced by (1,1)");

        // Verify color and width preserved
        assert_eq!(line.color, Color::WHITE);
        assert_eq!(line.width, 1.0);
    }

    #[test]
    fn move_transaction_contains_old_and_new_data() {
        let mut world = World::new();
        let e = make_line(&mut world);

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let old_line = *world.get::<&LineData>(e).unwrap();

        let mut cmd = MoveCommand::new(&sel);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(5.0, 10.0)), &mut world);

        let tx = cmd.take_transaction().unwrap();
        assert_eq!(tx.ops.len(), 1);

        match &tx.ops[0] {
            AtomicOp::SetLineData {
                entity,
                old,
                new,
            } => {
                assert_eq!(*entity, e);
                // Compare individual fields since LineData doesn't implement PartialEq
                assert_eq!(old.start, old_line.start, "old start preserved");
                assert_eq!(old.end, old_line.end, "old end preserved");
                assert_eq!(old.color, old_line.color, "old color preserved");
                assert_eq!(
                    new.start,
                    Point2D::new(5.0, 10.0),
                    "new start = old start + displacement (5,10)"
                );
                assert_eq!(
                    new.end,
                    Point2D::new(15.0, 20.0),
                    "new end = old end + displacement (5,10)"
                );
            }
            other => panic!("expected SetLineData, got {other:?}"),
        }
    }

    // -- confirm as relative displacement ----------------------------------

    #[test]
    fn move_command_confirm_uses_base_as_displacement() {
        let mut world = World::new();
        let e = make_line(&mut world);

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let mut cmd = MoveCommand::new(&sel);

        // Step 1: base point = (3, 4)
        let r1 = cmd.on_input(CommandInput::Point(Point2D::new(3.0, 4.0)), &mut world);
        assert!(matches!(r1, CommandResult::Continue));

        // Step 2: Confirm → use base as relative displacement
        let r2 = cmd.on_input(CommandInput::Confirm, &mut world);
        assert!(matches!(r2, CommandResult::Complete));

        // Verify geometry: displaced by (3, 4)
        let line = world.get::<&LineData>(e).unwrap();
        assert_eq!(line.start, Point2D::new(3.0, 4.0));
        assert_eq!(line.end, Point2D::new(13.0, 14.0));
    }

    // -- empty selection ---------------------------------------------------

    #[test]
    fn move_command_with_empty_selection_returns_error() {
        let mut world = World::new();
        let sel = SelectionManager::new();

        let mut cmd = MoveCommand::new(&sel);

        // Any point input should error
        let result = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);
        assert!(matches!(result, CommandResult::Error(_)));

        // Confirm should also error
        let result2 = cmd.on_input(CommandInput::Confirm, &mut world);
        assert!(matches!(result2, CommandResult::Error(_)));
    }

    // -- all entity types --------------------------------------------------

    #[test]
    fn move_displaces_line() {
        let mut world = World::new();
        let e = make_line(&mut world);

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let mut cmd = MoveCommand::new(&sel);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(10.0, 0.0)), &mut world);

        let line = world.get::<&LineData>(e).unwrap();
        assert_eq!(line.start, Point2D::new(10.0, 0.0));
        assert_eq!(line.end, Point2D::new(20.0, 10.0));
    }

    #[test]
    fn move_displaces_circle() {
        let mut world = World::new();
        let e = make_circle(&mut world, Point2D::new(5.0, 5.0), 3.0);

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let mut cmd = MoveCommand::new(&sel);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(2.0, 3.0)), &mut world);

        let circle = world.get::<&CircleData>(e).unwrap();
        assert_eq!(circle.center, Point2D::new(7.0, 8.0), "center displaced by (2,3)");
        assert_eq!(circle.radius, 3.0, "radius unchanged");
        assert_eq!(circle.color, Color::WHITE, "color preserved");
    }

    #[test]
    fn move_displaces_arc() {
        let mut world = World::new();
        let e = make_arc(&mut world, Point2D::new(3.0, 4.0));

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let mut cmd = MoveCommand::new(&sel);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(1.0, -1.0)), &mut world);

        let arc = world.get::<&ArcData>(e).unwrap();
        assert_eq!(arc.center, Point2D::new(4.0, 3.0), "center displaced by (1,-1)");
        assert_eq!(arc.radius, 5.0, "radius unchanged");
        assert_eq!(arc.start_angle, 0.0, "angles unchanged");
        assert_eq!(arc.end_angle, 90.0, "angles unchanged");
    }

    #[test]
    fn move_displaces_polyline() {
        let mut world = World::new();
        let e = make_polyline(&mut world);

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let mut cmd = MoveCommand::new(&sel);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(-5.0, 5.0)), &mut world);

        let poly = world.get::<&PolylineData>(e).unwrap();
        assert_eq!(
            poly.vertices,
            vec![
                Point2D::new(-5.0, 5.0),
                Point2D::new(0.0, 10.0),
                Point2D::new(5.0, 5.0),
            ],
            "all vertices displaced by (-5,5)"
        );
        assert!(!poly.closed, "closed flag unchanged");
    }

    #[test]
    fn move_displaces_all_entity_types() {
        let mut world = World::new();
        let line_e = make_line(&mut world);
        let circle_e = make_circle(&mut world, Point2D::new(1.0, 1.0), 2.0);
        let arc_e = make_arc(&mut world, Point2D::new(2.0, 2.0));
        let poly_e = make_polyline(&mut world);

        let mut sel = SelectionManager::new();
        sel.select(&mut world, line_e);
        sel.select(&mut world, circle_e);
        sel.select(&mut world, arc_e);
        sel.select(&mut world, poly_e);

        let mut cmd = MoveCommand::new(&sel);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(100.0, 200.0)), &mut world);

        // Verify all types displaced
        let line = world.get::<&LineData>(line_e).unwrap();
        assert_eq!(line.start, Point2D::new(100.0, 200.0));

        let circle = world.get::<&CircleData>(circle_e).unwrap();
        assert_eq!(circle.center, Point2D::new(101.0, 201.0));

        let arc = world.get::<&ArcData>(arc_e).unwrap();
        assert_eq!(arc.center, Point2D::new(102.0, 202.0));

        let poly = world.get::<&PolylineData>(poly_e).unwrap();
        assert!(poly.vertices[0].x > 99.0, "polyline vertices displaced");

        // Transaction should contain all 4 ops
        let tx = cmd.take_transaction().unwrap();
        assert_eq!(tx.ops.len(), 4);
    }

    // -- cancel ------------------------------------------------------------

    #[test]
    fn move_cancel_clears_state() {
        let mut world = World::new();
        let e = make_line(&mut world);

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let mut cmd = MoveCommand::new(&sel);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(5.0, 5.0)), &mut world);

        let result = cmd.on_input(CommandInput::Cancel, &mut world);
        assert!(matches!(result, CommandResult::Cancelled));
        assert!(cmd.base_point.is_none(), "base point should be cleared on cancel");
        assert!(cmd.take_transaction().is_none(), "no transaction after cancel");

        // Entities should not have moved
        let line = world.get::<&LineData>(e).unwrap();
        assert_eq!(line.start, Point2D::new(0.0, 0.0), "entity position unchanged after cancel");
    }

    // -- re-use after cancel -----------------------------------------------

    #[test]
    fn move_reusable_after_cancel() {
        let mut world = World::new();
        let e = make_line(&mut world);

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let mut cmd = MoveCommand::new(&sel);

        // Cancel before any input
        let _ = cmd.on_input(CommandInput::Cancel, &mut world);
        assert!(cmd.base_point.is_none());
        assert_eq!(cmd.steps_remaining(), 2);

        // Can be reused after cancel
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(1.0, 1.0)), &mut world);
        let r = cmd.on_input(CommandInput::Point(Point2D::new(3.0, 4.0)), &mut world);
        assert!(matches!(r, CommandResult::Complete));

        let line = world.get::<&LineData>(e).unwrap();
        assert_eq!(line.start, Point2D::new(2.0, 3.0), "displaced by (2,3)");
    }

    // -- take_transaction --------------------------------------------------

    #[test]
    fn move_take_transaction_consumes() {
        let mut world = World::new();
        let e = make_line(&mut world);

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let mut cmd = MoveCommand::new(&sel);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(1.0, 1.0)), &mut world);

        assert!(cmd.take_transaction().is_some(), "first take returns transaction");
        assert!(cmd.take_transaction().is_none(), "second take returns None (consumed)");
    }
}
