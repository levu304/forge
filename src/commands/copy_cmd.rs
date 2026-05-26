//! COPY command implementation.
//!
//! Creates displaced copies of selected entities. Two-step interaction:
//! 1. User picks a base point (or types relative displacement).
//! 2. User picks a second point, or presses Enter to use the base point
//!    as a relative displacement from origin.
//!
//! Builds a [`Transaction`] with `Spawn*` ops for the history system.
//! Newly spawned entities are tracked so the caller can update the
//! selection.

use super::{Command, CommandInput, CommandResult, PreviewEntity};
use crate::ecs::components::*;
use crate::geometry::Point2D;
use crate::history::{AtomicOp, Transaction};
use crate::selection::SelectionManager;
use hecs::World;

/// Creates displaced copies of selected entities.
///
/// Captures the selection at construction time. Two-step state machine:
/// - `base_point == None` → awaiting first point
/// - `base_point == Some(p)` → awaiting second point (or Enter for relative)
///
/// After completion, call [`take_spawned_entities`](CopyCommand::take_spawned_entities)
/// to retrieve the handles of newly created entities for selection.
pub struct CopyCommand {
    /// Entities to copy (captured at construction time).
    selected_entities: Vec<hecs::Entity>,
    /// First point picked by the user (base point).
    base_point: Option<Point2D>,
    /// Transaction populated on completion, consumed by caller via
    /// [`take_transaction`](CopyCommand::take_transaction).
    pending_transaction: Option<Transaction>,
    /// Entities spawned during execution, for the caller to select.
    spawned_entities: Vec<hecs::Entity>,
}

/// Type-erased geometry value for holding entity data without
/// borrowing the ECS world.
enum AtomicOpValue {
    Line(LineData),
    Circle(CircleData),
    Arc(ArcData),
    Polyline(PolylineData),
}

impl CopyCommand {
    /// Create a new COPY command, capturing the current selection.
    ///
    /// The command will be empty if nothing was selected. Callers should
    /// check [`is_empty`](CopyCommand::is_empty) before activating.
    pub fn new(selection: &SelectionManager) -> Self {
        Self {
            selected_entities: selection.selected.iter().copied().collect(),
            base_point: None,
            pending_transaction: None,
            spawned_entities: Vec::new(),
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

    /// Consume the list of newly spawned entity handles.
    ///
    /// Callers should select these entities after the command completes.
    pub fn take_spawned_entities(&mut self) -> Vec<hecs::Entity> {
        std::mem::take(&mut self.spawned_entities)
    }

    /// Apply a displacement vector to create copies of captured entities.
    ///
    /// Reads each entity's current geometry data, spawns a new entity at
    /// the displaced position, and records the spawn in a `Transaction`.
    fn apply_displacement(&mut self, world: &mut World, displacement: Point2D) -> Transaction {
        let count = self.selected_entities.len();
        let mut tx = Transaction::new(format!(
            "Copy {} entit{}",
            count,
            if count == 1 { "y" } else { "ies" },
        ));

        for &entity in &self.selected_entities {
            let result = Self::read_entity_data(world, entity, displacement);

            if let Some(value) = result {
                // Spawn new entity with displaced data.
                let new_entity = match &value {
                    AtomicOpValue::Line(v) => {
                        let e = world.spawn((*v, Renderable));
                        tx.push(AtomicOp::SpawnLine {
                            entity: e,
                            data: *v,
                        });
                        e
                    }
                    AtomicOpValue::Circle(v) => {
                        let e = world.spawn((*v, Renderable));
                        tx.push(AtomicOp::SpawnCircle {
                            entity: e,
                            data: *v,
                        });
                        e
                    }
                    AtomicOpValue::Arc(v) => {
                        let e = world.spawn((*v, Renderable));
                        tx.push(AtomicOp::SpawnArc {
                            entity: e,
                            data: *v,
                        });
                        e
                    }
                    AtomicOpValue::Polyline(v) => {
                        let e = world.spawn((v.clone(), Renderable));
                        tx.push(AtomicOp::SpawnPolyline {
                            entity: e,
                            data: v.clone(),
                        });
                        e
                    }
                };
                self.spawned_entities.push(new_entity);
            }
            // Entities without a recognised geometry type are silently skipped.
        }

        tx
    }

    /// Helper to read an entity's geometry, apply displacement, and return
    /// the displaced data as a type-erased value.
    ///
    /// Returns `None` if the entity has no recognised geometry component.
    fn read_entity_data(
        world: &World,
        entity: hecs::Entity,
        displacement: Point2D,
    ) -> Option<AtomicOpValue> {
        if let Ok(data) = world.get::<&LineData>(entity) {
            let old = *data;
            let new = LineData {
                start: old.start + displacement,
                end: old.end + displacement,
                ..old
            };
            Some(AtomicOpValue::Line(new))
        } else if let Ok(data) = world.get::<&CircleData>(entity) {
            let old = *data;
            let new = CircleData {
                center: old.center + displacement,
                ..old
            };
            Some(AtomicOpValue::Circle(new))
        } else if let Ok(data) = world.get::<&ArcData>(entity) {
            let old = *data;
            let new = ArcData {
                center: old.center + displacement,
                ..old
            };
            Some(AtomicOpValue::Arc(new))
        } else if let Ok(data) = world.get::<&PolylineData>(entity) {
            let old: PolylineData = (&*data).clone();
            let new = PolylineData {
                vertices: old.vertices.iter().map(|v| *v + displacement).collect(),
                ..old.clone()
            };
            Some(AtomicOpValue::Polyline(new))
        } else {
            None
        }
    }
}

impl Command for CopyCommand {
    fn name(&self) -> &'static str {
        "COPY"
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
                        "No entities selected. Select objects before running COPY."
                            .to_string(),
                    );
                }

                if self.base_point.is_none() {
                    // Step 1: store base point.
                    self.base_point = Some(p);
                    CommandResult::Continue
                } else {
                    // Step 2: compute displacement and spawn copies.
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
                        "No entities selected. Select objects before running COPY."
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
        self.spawned_entities.clear();
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

    fn count_entities(world: &World) -> usize {
        world.query::<&Renderable>().iter().count()
    }

    // -- construction ------------------------------------------------------

    #[test]
    fn new_captures_selection() {
        let mut world = World::new();
        let e = make_line(&mut world);

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let cmd = CopyCommand::new(&sel);
        assert_eq!(cmd.selected_entities.len(), 1);
        assert!(!cmd.is_empty());
    }

    #[test]
    fn new_with_empty_selection_is_empty() {
        let sel = SelectionManager::new();
        let cmd = CopyCommand::new(&sel);
        assert!(cmd.is_empty());
        assert!(cmd.base_point.is_none());
    }

    #[test]
    fn name_and_prompt() {
        let sel = SelectionManager::new();
        let cmd = CopyCommand::new(&sel);
        assert_eq!(cmd.name(), "COPY");
        assert_eq!(cmd.prompt(), "Specify base point or displacement:");
        assert_eq!(cmd.steps_remaining(), 2);
    }

    #[test]
    fn prompt_after_base_point() {
        let mut world = World::new();
        let e = make_line(&mut world);

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let mut cmd = CopyCommand::new(&sel);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(5.0, 5.0)), &mut world);

        assert_eq!(cmd.prompt(), "Specify second point or <use first point as displacement>:");
        assert_eq!(cmd.steps_remaining(), 1);
    }

    // -- two-point copy ----------------------------------------------------

    #[test]
    fn copy_two_points_spawns_new_entity() {
        let mut world = World::new();
        let e = make_line(&mut world);
        let original_count = count_entities(&world);

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let mut cmd = CopyCommand::new(&sel);

        // Step 1: base point
        let r1 = cmd.on_input(CommandInput::Point(Point2D::new(1.0, 1.0)), &mut world);
        assert!(matches!(r1, CommandResult::Continue));

        // Step 2: second point → displacement = (2,2) - (1,1) = (1,1)
        let r2 = cmd.on_input(CommandInput::Point(Point2D::new(2.0, 2.0)), &mut world);
        assert!(matches!(r2, CommandResult::Complete));

        // A new entity should have been spawned
        assert_eq!(count_entities(&world), original_count + 1);

        // Original entity should be unchanged
        let line = world.get::<&LineData>(e).unwrap();
        assert_eq!(line.start, Point2D::new(0.0, 0.0), "original unchanged");
        assert_eq!(line.end, Point2D::new(10.0, 10.0), "original unchanged");

        // Spawned entities list should contain the new entity
        let spawned = cmd.take_spawned_entities();
        assert_eq!(spawned.len(), 1);

        let new_data = world.get::<&LineData>(spawned[0]).unwrap();
        assert_eq!(new_data.start, Point2D::new(1.0, 1.0), "copy start displaced by (1,1)");
        assert_eq!(new_data.end, Point2D::new(11.0, 11.0), "copy end displaced by (1,1)");
        assert_eq!(new_data.color, Color::WHITE, "color preserved");
        assert_eq!(new_data.width, 1.0, "width preserved");
    }

    #[test]
    fn copy_transaction_contains_spawn_ops() {
        let mut world = World::new();
        let e = make_line(&mut world);

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let mut cmd = CopyCommand::new(&sel);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(5.0, 10.0)), &mut world);

        let tx = cmd.take_transaction().unwrap();
        assert_eq!(tx.ops.len(), 1);

        match &tx.ops[0] {
            AtomicOp::SpawnLine { entity: _, data } => {
                assert_eq!(data.start, Point2D::new(5.0, 10.0), "spawned line start correct");
                assert_eq!(data.end, Point2D::new(15.0, 20.0), "spawned line end correct");
            }
            other => panic!("expected SpawnLine, got {other:?}"),
        }
    }

    // -- confirm as relative displacement ----------------------------------

    #[test]
    fn copy_confirm_uses_base_as_displacement() {
        let mut world = World::new();
        let e = make_line(&mut world);
        let original_count = count_entities(&world);

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let mut cmd = CopyCommand::new(&sel);

        // Step 1: base point = (3, 4)
        let r1 = cmd.on_input(CommandInput::Point(Point2D::new(3.0, 4.0)), &mut world);
        assert!(matches!(r1, CommandResult::Continue));

        // Step 2: Confirm → use base as relative displacement
        let r2 = cmd.on_input(CommandInput::Confirm, &mut world);
        assert!(matches!(r2, CommandResult::Complete));

        // A new entity should have been spawned
        assert_eq!(count_entities(&world), original_count + 1);

        // Original entity unchanged
        let line = world.get::<&LineData>(e).unwrap();
        assert_eq!(line.start, Point2D::new(0.0, 0.0));

        // Spawned copy displaced by (3, 4)
        let spawned = cmd.take_spawned_entities();
        assert_eq!(spawned.len(), 1);
        let new_data = world.get::<&LineData>(spawned[0]).unwrap();
        assert_eq!(new_data.start, Point2D::new(3.0, 4.0));
        assert_eq!(new_data.end, Point2D::new(13.0, 14.0));
    }

    // -- empty selection ---------------------------------------------------

    #[test]
    fn copy_with_empty_selection_returns_error() {
        let mut world = World::new();
        let sel = SelectionManager::new();

        let mut cmd = CopyCommand::new(&sel);

        let result = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);
        assert!(matches!(result, CommandResult::Error(_)));

        let result2 = cmd.on_input(CommandInput::Confirm, &mut world);
        assert!(matches!(result2, CommandResult::Error(_)));
    }

    // -- all entity types --------------------------------------------------

    #[test]
    fn copy_line_creates_copy() {
        let mut world = World::new();
        let e = make_line(&mut world);

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let mut cmd = CopyCommand::new(&sel);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(10.0, 0.0)), &mut world);

        let spawned = cmd.take_spawned_entities();
        assert_eq!(spawned.len(), 1);
        let line = world.get::<&LineData>(spawned[0]).unwrap();
        assert_eq!(line.start, Point2D::new(10.0, 0.0));
        assert_eq!(line.end, Point2D::new(20.0, 10.0));
    }

    #[test]
    fn copy_circle_creates_copy() {
        let mut world = World::new();
        let e = make_circle(&mut world, Point2D::new(5.0, 5.0), 3.0);

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let mut cmd = CopyCommand::new(&sel);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(2.0, 3.0)), &mut world);

        let spawned = cmd.take_spawned_entities();
        assert_eq!(spawned.len(), 1);
        let circle = world.get::<&CircleData>(spawned[0]).unwrap();
        assert_eq!(circle.center, Point2D::new(7.0, 8.0), "center displaced by (2,3)");
        assert_eq!(circle.radius, 3.0, "radius unchanged");
        assert_eq!(circle.color, Color::WHITE, "color preserved");
    }

    #[test]
    fn copy_arc_creates_copy() {
        let mut world = World::new();
        let e = make_arc(&mut world, Point2D::new(3.0, 4.0));

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let mut cmd = CopyCommand::new(&sel);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(1.0, -1.0)), &mut world);

        let spawned = cmd.take_spawned_entities();
        assert_eq!(spawned.len(), 1);
        let arc = world.get::<&ArcData>(spawned[0]).unwrap();
        assert_eq!(arc.center, Point2D::new(4.0, 3.0), "center displaced by (1,-1)");
        assert_eq!(arc.radius, 5.0, "radius unchanged");
        assert_eq!(arc.start_angle, 0.0, "angles unchanged");
        assert_eq!(arc.end_angle, 90.0, "angles unchanged");
    }

    #[test]
    fn copy_polyline_creates_copy() {
        let mut world = World::new();
        let e = make_polyline(&mut world);

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let mut cmd = CopyCommand::new(&sel);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(-5.0, 5.0)), &mut world);

        let spawned = cmd.take_spawned_entities();
        assert_eq!(spawned.len(), 1);
        let poly = world.get::<&PolylineData>(spawned[0]).unwrap();
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
    fn copy_handles_all_entity_types() {
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

        let mut cmd = CopyCommand::new(&sel);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(100.0, 200.0)), &mut world);

        // All 4 copies should exist.
        let spawned = cmd.take_spawned_entities();
        assert_eq!(spawned.len(), 4, "one copy per selected entity");

        // Verify each type exists among spawned entities (order is
        // non-deterministic because SelectionManager uses a HashSet).
        let mut line_found = false;
        let mut circle_found = false;
        let mut arc_found = false;
        let mut poly_found = false;

        for &e in &spawned {
            if let Ok(line) = world.get::<&LineData>(e) {
                assert_eq!(line.start, Point2D::new(100.0, 200.0));
                line_found = true;
            } else if let Ok(circle) = world.get::<&CircleData>(e) {
                assert_eq!(circle.center, Point2D::new(101.0, 201.0));
                circle_found = true;
            } else if let Ok(arc) = world.get::<&ArcData>(e) {
                assert_eq!(arc.center, Point2D::new(102.0, 202.0));
                arc_found = true;
            } else if let Ok(poly) = world.get::<&PolylineData>(e) {
                assert!(poly.vertices[0].x > 99.0, "polyline vertices displaced");
                poly_found = true;
            } else {
                panic!("spawned entity {:?} has no recognised geometry", e);
            }
        }

        assert!(line_found, "line copy not found");
        assert!(circle_found, "circle copy not found");
        assert!(arc_found, "arc copy not found");
        assert!(poly_found, "polyline copy not found");

        // Transaction should contain all 4 Spawn ops
        let tx = cmd.take_transaction().unwrap();
        assert_eq!(tx.ops.len(), 4);
    }

    // -- cancel ------------------------------------------------------------

    #[test]
    fn copy_cancel_clears_state() {
        let mut world = World::new();
        let e = make_line(&mut world);

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let mut cmd = CopyCommand::new(&sel);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(5.0, 5.0)), &mut world);

        let result = cmd.on_input(CommandInput::Cancel, &mut world);
        assert!(matches!(result, CommandResult::Cancelled));
        assert!(cmd.base_point.is_none(), "base point should be cleared on cancel");
        assert!(cmd.take_transaction().is_none(), "no transaction after cancel");
        assert!(cmd.take_spawned_entities().is_empty(), "no spawned entities after cancel");

        // Original entity should still be there
        let line = world.get::<&LineData>(e).unwrap();
        assert_eq!(line.start, Point2D::new(0.0, 0.0), "entity unchanged after cancel");
    }

    // -- re-use after cancel -----------------------------------------------

    #[test]
    fn copy_reusable_after_cancel() {
        let mut world = World::new();
        let e = make_line(&mut world);

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let mut cmd = CopyCommand::new(&sel);

        // Cancel before any input
        let _ = cmd.on_input(CommandInput::Cancel, &mut world);
        assert!(cmd.base_point.is_none());
        assert_eq!(cmd.steps_remaining(), 2);

        // Can be reused after cancel
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(1.0, 1.0)), &mut world);
        let r = cmd.on_input(CommandInput::Point(Point2D::new(3.0, 4.0)), &mut world);
        assert!(matches!(r, CommandResult::Complete));

        let spawned = cmd.take_spawned_entities();
        assert_eq!(spawned.len(), 1);
        let new_data = world.get::<&LineData>(spawned[0]).unwrap();
        assert_eq!(new_data.start, Point2D::new(2.0, 3.0), "displaced by (2,3)");
    }

    // -- take_transaction --------------------------------------------------

    #[test]
    fn copy_take_transaction_consumes() {
        let mut world = World::new();
        let e = make_line(&mut world);

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let mut cmd = CopyCommand::new(&sel);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(1.0, 1.0)), &mut world);

        assert!(cmd.take_transaction().is_some(), "first take returns transaction");
        assert!(cmd.take_transaction().is_none(), "second take returns None (consumed)");
    }

    // -- take_spawned_entities ---------------------------------------------

    #[test]
    fn copy_take_spawned_consumes() {
        let mut world = World::new();
        let e = make_line(&mut world);

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let mut cmd = CopyCommand::new(&sel);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(1.0, 1.0)), &mut world);

        assert_eq!(cmd.take_spawned_entities().len(), 1, "first take returns spawned entities");
        assert!(cmd.take_spawned_entities().is_empty(), "second take returns empty");
    }
}
