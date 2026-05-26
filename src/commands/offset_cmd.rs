//! OFFSET command implementation.
//!
//! Creates parallel copies of selected entities at a specified distance.
//! Two-step interaction:
//! 1. User specifies the offset distance (via text input).
//! 2. User picks a side (click to determine which side of each entity
//!    the offset lies on).
//!
//! For v0.2.0, supports:
//! - **Lines**: parallel offset at perpendicular distance
//! - **Circles**: concentric offset (radius ± distance)
//!
//! Arcs and polylines are unsupported in v0.2.0. If all selected
//! entities are unsupported, the command returns an error.
//!
//! Builds a [`Transaction`] with `Spawn*` ops for the history system.

use super::{Command, CommandInput, CommandResult, PreviewEntity};
use crate::ecs::components::*;
use crate::geometry::Point2D;
use crate::history::{AtomicOp, Transaction};
use crate::selection::SelectionManager;
use hecs::World;

/// Creates parallel copies of selected entities at a specified distance.
///
/// Captures the selection at construction time. Two-step state machine:
/// - `distance == None` → awaiting distance value
/// - `distance == Some(d)` → awaiting side-pick click
pub struct OffsetCommand {
    /// Entities to offset (captured at construction time).
    selected_entities: Vec<hecs::Entity>,
    /// Offset distance (positive magnitude).
    distance: Option<f64>,
    /// Transaction populated on completion, consumed by caller via
    /// [`take_transaction`](OffsetCommand::take_transaction).
    pending_transaction: Option<Transaction>,
    /// Entities spawned during execution, for the caller to select.
    spawned_entities: Vec<hecs::Entity>,
}

impl OffsetCommand {
    /// Create a new OFFSET command, capturing the current selection.
    ///
    /// The command will be empty if nothing was selected. Callers should
    /// check [`is_empty`](OffsetCommand::is_empty) before activating.
    pub fn new(selection: &SelectionManager) -> Self {
        Self {
            selected_entities: selection.selected.iter().copied().collect(),
            distance: None,
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

    /// Apply offset to all selected entities.
    ///
    /// `dist` is the offset magnitude; `side_point` determines which side
    /// each entity is offset toward:
    /// - For lines: the perpendicular direction is toward `side_point`.
    /// - For circles: `side_point` outside the circle → outer offset (+dist),
    ///   inside → inner offset (−dist).
    ///
    /// Uses a two-phase approach to avoid borrow conflicts:
    /// 1. Read all entity data (immutable borrow of `world`), compute new data.
    /// 2. Spawn new entities (mutable borrow of `world`).
    ///
    /// Returns `None` when no entities could be offset (all selected entities
    /// are unsupported types like arcs or polylines in v0.2.0).
    fn apply_offset(&mut self, world: &mut World, dist: f64, side_point: Point2D) -> Option<Transaction> {
        // Phase 1: read and compute (immutable borrow).
        let new_entities: Vec<AtomicOpValue> = self
            .selected_entities
            .iter()
            .filter_map(|&entity| {
                Self::compute_offset_data(world, entity, dist, side_point)
            })
            .collect();

        if new_entities.is_empty() {
            return None;
        }

        let count = new_entities.len();
        let mut tx = Transaction::new(format!(
            "Offset {} entit{}",
            count,
            if count == 1 { "y" } else { "ies" },
        ));

        // Phase 2: spawn (mutable borrow).
        for new_data in new_entities {
            let new_entity = match &new_data {
                AtomicOpValue::Line(v) => world.spawn((*v, Renderable)),
                AtomicOpValue::Circle(v) => world.spawn((*v, Renderable)),
                AtomicOpValue::Arc(v) => world.spawn((*v, Renderable)),
                AtomicOpValue::Polyline(v) => world.spawn((v.clone(), Renderable)),
            };
            self.spawned_entities.push(new_entity);

            match new_data {
                AtomicOpValue::Line(v) => {
                    tx.push(AtomicOp::SpawnLine {
                        entity: new_entity,
                        data: v,
                    });
                }
                AtomicOpValue::Circle(v) => {
                    tx.push(AtomicOp::SpawnCircle {
                        entity: new_entity,
                        data: v,
                    });
                }
                AtomicOpValue::Arc(v) => {
                    tx.push(AtomicOp::SpawnArc {
                        entity: new_entity,
                        data: v,
                    });
                }
                AtomicOpValue::Polyline(v) => {
                    tx.push(AtomicOp::SpawnPolyline {
                        entity: new_entity,
                        data: v,
                    });
                }
            }
        }

        Some(tx)
    }

    /// Read-only helper: compute the offset data for a single entity.
    ///
    /// Takes `&World` (immutable) so callers can hold the result without
    /// borrow conflicts. Returns `None` for unsupported entity types
    /// (arcs, polylines in v0.2.0).
    fn compute_offset_data(
        world: &World,
        entity: hecs::Entity,
        dist: f64,
        side_point: Point2D,
    ) -> Option<AtomicOpValue> {
        // Try LineData first.
        if let Ok(data) = world.get::<&LineData>(entity) {
            let line = *data;

            // Compute direction and perpendicular.
            let dir = (line.end - line.start).normalized();
            // If the line is zero-length, skip.
            if dir.x.abs() < 1e-15 && dir.y.abs() < 1e-15 {
                return None;
            }

            // Determine which side the click point is on using cross product.
            let dx = line.end.x - line.start.x;
            let dy = line.end.y - line.start.y;
            let side_cross = dx * (side_point.y - line.start.y)
                - dy * (side_point.x - line.start.x);
            let sign = if side_cross >= 0.0 { 1.0 } else { -1.0 };

            // Perpendicular vector (rotate direction 90° CCW).
            let perp = Point2D::new(-dir.y * sign, dir.x * sign);

            let new_start = line.start + perp * dist;
            let new_end = line.end + perp * dist;

            let new_data = LineData {
                start: new_start,
                end: new_end,
                ..line
            };

            return Some(AtomicOpValue::Line(new_data));
        }

        // Try CircleData next.
        if let Ok(data) = world.get::<&CircleData>(entity) {
            let circle = *data;

            // Determine inner vs outer offset.
            let click_dist = side_point.distance(circle.center);
            let sign = if click_dist >= circle.radius { 1.0 } else { -1.0 };

            let new_radius = circle.radius + dist * sign;

            // Skip if the new radius would be non-positive.
            if new_radius <= 0.0 {
                return None;
            }

            let new_data = CircleData {
                radius: new_radius,
                ..circle
            };

            return Some(AtomicOpValue::Circle(new_data));
        }

        // ArcData and PolylineData are unsupported in v0.2.0 — skip.
        None
    }
}

/// Type-erased geometry value for spawned entities.
enum AtomicOpValue {
    Line(LineData),
    Circle(CircleData),
    #[allow(dead_code)]
    Arc(ArcData),
    #[allow(dead_code)]
    Polyline(PolylineData),
}

impl Command for OffsetCommand {
    fn name(&self) -> &'static str {
        "OFFSET"
    }

    fn prompt(&self) -> String {
        if self.distance.is_none() {
            "Specify offset distance:".to_string()
        } else {
            "Specify side to offset (click on one side):".to_string()
        }
    }

    fn steps_remaining(&self) -> usize {
        if self.distance.is_none() { 2 } else { 1 }
    }

    fn on_input(&mut self, input: CommandInput, world: &mut World) -> CommandResult {
        if self.selected_entities.is_empty() {
            return CommandResult::Error(
                "No entities selected. Select objects before running OFFSET.".to_string(),
            );
        }

        match input {
            CommandInput::Text(s) => {
                if self.distance.is_some() {
                    return CommandResult::Error(
                        "Distance already set. Click to pick side.".to_string(),
                    );
                }

                let dist: f64 = match s.parse() {
                    Ok(v) if v > 0.0 => v,
                    Ok(_) => {
                        return CommandResult::Error(
                            "Offset distance must be positive.".to_string(),
                        );
                    }
                    Err(_) => {
                        return CommandResult::Error(
                            format!("Invalid distance: '{s}'. Enter a positive number."),
                        );
                    }
                };

                self.distance = Some(dist);
                CommandResult::Continue
            }
            CommandInput::Point(p) => {
                let dist = match self.distance {
                    Some(d) => d,
                    None => {
                        return CommandResult::Error(
                            "Specify an offset distance first.".to_string(),
                        );
                    }
                };

                match self.apply_offset(world, dist, p) {
                    Some(tx) => {
                        self.pending_transaction = Some(tx);
                        CommandResult::Complete
                    }
                    None => CommandResult::Error(
                        "No supported entities for OFFSET. Only lines and circles are supported in v0.2.0.".to_string(),
                    ),
                }
            }
            CommandInput::Confirm => {
                if self.distance.is_none() {
                    CommandResult::Error(
                        "Specify an offset distance first.".to_string(),
                    )
                } else {
                    CommandResult::Error(
                        "Click to pick the offset side.".to_string(),
                    )
                }
            }
            CommandInput::Cancel => {
                self.on_cancel(world);
                CommandResult::Cancelled
            }
            _ => CommandResult::Error("Specify a distance or point.".to_string()),
        }
    }

    fn on_cancel(&mut self, _world: &mut World) {
        self.distance = None;
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

    fn count_entities(world: &World) -> usize {
        world.query::<&Renderable>().iter().count()
    }

    // -- construction ------------------------------------------------------

    #[test]
    fn new_captures_selection() {
        let mut world = World::new();
        let e = make_line(&mut world, Point2D::new(0.0, 0.0), Point2D::new(10.0, 0.0));

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let cmd = OffsetCommand::new(&sel);
        assert_eq!(cmd.selected_entities.len(), 1);
        assert!(!cmd.is_empty());
    }

    #[test]
    fn new_with_empty_selection_is_empty() {
        let sel = SelectionManager::new();
        let cmd = OffsetCommand::new(&sel);
        assert!(cmd.is_empty());
    }

    #[test]
    fn name_and_prompt() {
        let sel = SelectionManager::new();
        let cmd = OffsetCommand::new(&sel);
        assert_eq!(cmd.name(), "OFFSET");
        assert_eq!(cmd.prompt(), "Specify offset distance:");
        assert_eq!(cmd.steps_remaining(), 2);
    }

    // -- line offset -------------------------------------------------------

    #[test]
    fn offset_line_at_distance() {
        let mut world = World::new();
        // Horizontal line from (0,0) to (10,0)
        let e = make_line(&mut world, Point2D::new(0.0, 0.0), Point2D::new(10.0, 0.0));
        let original_count = count_entities(&world);

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let mut cmd = OffsetCommand::new(&sel);

        // Step 1: distance = 10
        let r1 = cmd.on_input(CommandInput::Text("10".to_string()), &mut world);
        assert!(matches!(r1, CommandResult::Continue));

        // Step 2: click above the line (positive y) → offset upward
        let r2 = cmd.on_input(CommandInput::Point(Point2D::new(5.0, 1.0)), &mut world);
        assert!(matches!(r2, CommandResult::Complete));

        // A new entity should have been spawned
        assert_eq!(count_entities(&world), original_count + 1);

        // Original entity unchanged
        let orig = world.get::<&LineData>(e).unwrap();
        assert_eq!(orig.start, Point2D::new(0.0, 0.0));
        assert_eq!(orig.end, Point2D::new(10.0, 0.0));

        // Offset line should be parallel at y=10
        let spawned = cmd.take_spawned_entities();
        assert_eq!(spawned.len(), 1);
        let offset = world.get::<&LineData>(spawned[0]).unwrap();
        assert!((offset.start.x - 0.0).abs() < 1e-10, "offset start x ≈ 0, got {}", offset.start.x);
        assert!((offset.start.y - 10.0).abs() < 1e-10, "offset start y ≈ 10, got {}", offset.start.y);
        assert!((offset.end.x - 10.0).abs() < 1e-10, "offset end x ≈ 10, got {}", offset.end.x);
        assert!((offset.end.y - 10.0).abs() < 1e-10, "offset end y ≈ 10, got {}", offset.end.y);
        assert_eq!(offset.color, Color::WHITE, "color preserved");
        assert_eq!(offset.width, 1.0, "width preserved");
    }

    #[test]
    fn offset_line_below() {
        let mut world = World::new();
        // Horizontal line from (0,0) to (10,0)
        let e = make_line(&mut world, Point2D::new(0.0, 0.0), Point2D::new(10.0, 0.0));

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let mut cmd = OffsetCommand::new(&sel);
        let _ = cmd.on_input(CommandInput::Text("5".to_string()), &mut world);
        // Click below the line → offset downward
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(5.0, -1.0)), &mut world);

        let spawned = cmd.take_spawned_entities();
        assert_eq!(spawned.len(), 1);
        let offset = world.get::<&LineData>(spawned[0]).unwrap();
        assert!((offset.start.y - (-5.0)).abs() < 1e-10, "offset start y ≈ -5, got {}", offset.start.y);
        assert!((offset.end.y - (-5.0)).abs() < 1e-10, "offset end y ≈ -5, got {}", offset.end.y);
    }

    // -- circle offset -----------------------------------------------------

    #[test]
    fn offset_circle_outer() {
        let mut world = World::new();
        let e = make_circle(&mut world, Point2D::new(0.0, 0.0), 5.0);
        let original_count = count_entities(&world);

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let mut cmd = OffsetCommand::new(&sel);
        let _ = cmd.on_input(CommandInput::Text("3".to_string()), &mut world);
        // Click outside the circle → outer offset (radius increases)
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(10.0, 0.0)), &mut world);

        assert_eq!(count_entities(&world), original_count + 1);

        let spawned = cmd.take_spawned_entities();
        assert_eq!(spawned.len(), 1);
        let offset = world.get::<&CircleData>(spawned[0]).unwrap();
        assert_eq!(offset.center, Point2D::new(0.0, 0.0), "center unchanged");
        assert_eq!(offset.radius, 8.0, "radius increased by 3");
    }

    #[test]
    fn offset_circle_inner() {
        let mut world = World::new();
        let e = make_circle(&mut world, Point2D::new(0.0, 0.0), 5.0);

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let mut cmd = OffsetCommand::new(&sel);
        let _ = cmd.on_input(CommandInput::Text("2".to_string()), &mut world);
        // Click inside the circle → inner offset (radius decreases)
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);

        let spawned = cmd.take_spawned_entities();
        assert_eq!(spawned.len(), 1);
        let offset = world.get::<&CircleData>(spawned[0]).unwrap();
        assert_eq!(offset.radius, 3.0, "radius decreased by 2");
    }

    #[test]
    fn offset_circle_inner_skips_if_radius_non_positive() {
        let mut world = World::new();
        let e = make_circle(&mut world, Point2D::new(0.0, 0.0), 5.0);

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let mut cmd = OffsetCommand::new(&sel);
        let _ = cmd.on_input(CommandInput::Text("10".to_string()), &mut world);
        // Click inside → would make radius -5, which is skipped
        let result = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);

        assert!(matches!(result, CommandResult::Error(_)), "should error when no entities can be offset");
        assert!(cmd.take_transaction().is_none(), "no transaction created");
    }

    // -- mixed selection ---------------------------------------------------

    #[test]
    fn offset_mixed_line_and_circle() {
        let mut world = World::new();
        let line_e = make_line(&mut world, Point2D::new(0.0, 0.0), Point2D::new(10.0, 0.0));
        let circle_e = make_circle(&mut world, Point2D::new(0.0, 0.0), 5.0);

        let mut sel = SelectionManager::new();
        sel.select(&mut world, line_e);
        sel.select(&mut world, circle_e);

        let mut cmd = OffsetCommand::new(&sel);
        let _ = cmd.on_input(CommandInput::Text("2".to_string()), &mut world);
        // Click above the line and outside the circle
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(5.0, 10.0)), &mut world);

        let spawned = cmd.take_spawned_entities();
        assert_eq!(spawned.len(), 2, "both entities should produce offsets");
    }

    // -- empty selection ---------------------------------------------------

    #[test]
    fn offset_with_empty_selection_returns_error() {
        let mut world = World::new();
        let sel = SelectionManager::new();

        let mut cmd = OffsetCommand::new(&sel);
        let result = cmd.on_input(CommandInput::Text("10".to_string()), &mut world);
        assert!(matches!(result, CommandResult::Error(_)));
    }

    // -- unsupported entity types ------------------------------------------

    #[test]
    fn offset_unsupported_entities_returns_error() {
        let mut world = World::new();

        // Arc (unsupported in v0.2.0)
        let e = world.spawn((
            ArcData {
                center: Point2D::new(0.0, 0.0),
                radius: 5.0,
                start_angle: 0.0,
                end_angle: 90.0,
                color: Color::WHITE,
                width: 1.0,
            },
            Renderable,
        ));

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let mut cmd = OffsetCommand::new(&sel);
        let _ = cmd.on_input(CommandInput::Text("5".to_string()), &mut world);

        let result = cmd.on_input(CommandInput::Point(Point2D::new(1.0, 0.0)), &mut world);
        assert!(matches!(result, CommandResult::Error(_)), "should error when all entities unsupported");
        assert!(cmd.take_transaction().is_none(), "no transaction created");
        assert!(cmd.take_spawned_entities().is_empty(), "no entities spawned");
    }

    // -- invalid text ------------------------------------------------------

    #[test]
    fn offset_invalid_text_returns_error() {
        let mut world = World::new();
        let e = make_line(&mut world, Point2D::new(0.0, 0.0), Point2D::new(10.0, 0.0));

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let mut cmd = OffsetCommand::new(&sel);
        let result = cmd.on_input(CommandInput::Text("not_a_number".to_string()), &mut world);
        assert!(matches!(result, CommandResult::Error(_)));
    }

    #[test]
    fn offset_non_positive_distance_returns_error() {
        let mut world = World::new();
        let e = make_line(&mut world, Point2D::new(0.0, 0.0), Point2D::new(10.0, 0.0));

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let mut cmd = OffsetCommand::new(&sel);
        let result = cmd.on_input(CommandInput::Text("0".to_string()), &mut world);
        assert!(matches!(result, CommandResult::Error(_)));

        let result2 = cmd.on_input(CommandInput::Text("-5".to_string()), &mut world);
        assert!(matches!(result2, CommandResult::Error(_)));
    }

    // -- click before distance ---------------------------------------------

    #[test]
    fn offset_click_before_distance_returns_error() {
        let mut world = World::new();
        let e = make_line(&mut world, Point2D::new(0.0, 0.0), Point2D::new(10.0, 0.0));

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let mut cmd = OffsetCommand::new(&sel);
        let result = cmd.on_input(CommandInput::Point(Point2D::new(5.0, 1.0)), &mut world);
        assert!(matches!(result, CommandResult::Error(_)));
    }

    // -- cancel ------------------------------------------------------------

    #[test]
    fn offset_cancel_clears_state() {
        let mut world = World::new();
        let e = make_line(&mut world, Point2D::new(0.0, 0.0), Point2D::new(10.0, 0.0));

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let mut cmd = OffsetCommand::new(&sel);
        let _ = cmd.on_input(CommandInput::Text("5".to_string()), &mut world);

        let result = cmd.on_input(CommandInput::Cancel, &mut world);
        assert!(matches!(result, CommandResult::Cancelled));
        assert!(cmd.distance.is_none());
        assert!(cmd.take_transaction().is_none());
        assert!(cmd.take_spawned_entities().is_empty());

        // Original entity unchanged
        let line = world.get::<&LineData>(e).unwrap();
        assert_eq!(line.end, Point2D::new(10.0, 0.0));
    }
}
