//! SCALE command implementation.
//!
//! Scales selected entities around a base point by a scale factor.
//! Two-step interaction:
//! 1. User picks a base point (center of scaling).
//! 2. User enters a numeric scale factor (e.g., `2` for 2x, `0.5` for half).
//!
//! Builds a [`Transaction`] with `Set*` ops for the history system.
//!
//! Note: v0.2.0 only supports text-input factor. A proper reference-length
//! workflow (pick reference distance, then new distance) is deferred.

use super::{Command, CommandInput, CommandResult, PreviewEntity};
use crate::ecs::components::*;
use crate::geometry::Point2D;
use crate::history::{AtomicOp, Transaction};
use crate::selection::SelectionManager;
use hecs::World;

/// Scales selected entities around a base point by a scale factor.
///
/// Captures the selection at construction time. Two-step state machine:
/// - `base_point == None` → awaiting base point
/// - `base_point == Some(p)` → awaiting scale factor (text input only)
pub struct ScaleCommand {
    /// Entities to scale (captured at construction time).
    selected_entities: Vec<hecs::Entity>,
    /// Center of scaling.
    base_point: Option<Point2D>,
    /// Transaction populated on completion, consumed by caller via
    /// [`take_transaction`](ScaleCommand::take_transaction).
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

impl ScaleCommand {
    /// Create a new SCALE command, capturing the current selection.
    ///
    /// The command will be empty if nothing was selected. Callers should
    /// check [`is_empty`](ScaleCommand::is_empty) before activating.
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

    /// Apply a uniform scale of `factor` around `center` to all captured
    /// entities.
    fn apply_scale(&mut self, world: &mut World, center: Point2D, factor: f64) -> Transaction {
        let count = self.selected_entities.len();
        let mut tx = Transaction::new(format!(
            "Scale {} entit{}",
            count,
            if count == 1 { "y" } else { "ies" },
        ));

        for &entity in &self.selected_entities {
            let result = Self::read_scale_entity(world, entity, center, factor);

            if let Some((old, new)) = result {
                // Write new data to world and record in transaction.
                match &new {
                    AtomicOpValue::Line(v) => {
                        if let Err(e) = world.insert_one(entity, *v) {
                            tracing::warn!("scale: failed to update entity {:?}: {}", entity, e);
                        }
                    }
                    AtomicOpValue::Circle(v) => {
                        if let Err(e) = world.insert_one(entity, *v) {
                            tracing::warn!("scale: failed to update entity {:?}: {}", entity, e);
                        }
                    }
                    AtomicOpValue::Arc(v) => {
                        if let Err(e) = world.insert_one(entity, *v) {
                            tracing::warn!("scale: failed to update entity {:?}: {}", entity, e);
                        }
                    }
                    AtomicOpValue::Polyline(v) => {
                        if let Err(e) = world.insert_one(entity, v.clone()) {
                            tracing::warn!("scale: failed to update entity {:?}: {}", entity, e);
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

    /// Scale a single point around `center` by `factor`.
    fn scale_point(p: Point2D, center: Point2D, factor: f64) -> Point2D {
        Point2D::new(
            center.x + (p.x - center.x) * factor,
            center.y + (p.y - center.y) * factor,
        )
    }

    /// Helper to read an entity's geometry, apply scale, and return
    /// (old, new) as type-erased values.
    fn read_scale_entity(
        world: &World,
        entity: hecs::Entity,
        center: Point2D,
        factor: f64,
    ) -> Option<(AtomicOpValue, AtomicOpValue)> {
        if let Ok(data) = world.get::<&LineData>(entity) {
            let old = *data;
            let new = LineData {
                start: Self::scale_point(old.start, center, factor),
                end: Self::scale_point(old.end, center, factor),
                ..old
            };
            Some((AtomicOpValue::Line(old), AtomicOpValue::Line(new)))
        } else if let Ok(data) = world.get::<&CircleData>(entity) {
            let old = *data;
            let new = CircleData {
                center: Self::scale_point(old.center, center, factor),
                radius: old.radius * factor.abs(),
                ..old
            };
            Some((AtomicOpValue::Circle(old), AtomicOpValue::Circle(new)))
        } else if let Ok(data) = world.get::<&ArcData>(entity) {
            let old = *data;
            let new = ArcData {
                center: Self::scale_point(old.center, center, factor),
                radius: old.radius * factor.abs(),
                ..old
            };
            Some((AtomicOpValue::Arc(old), AtomicOpValue::Arc(new)))
        } else if let Ok(data) = world.get::<&PolylineData>(entity) {
            let old: PolylineData = PolylineData::clone(&*data);
            let new = PolylineData {
                vertices: old.vertices.iter()
                    .map(|v| Self::scale_point(*v, center, factor))
                    .collect(),
                ..old.clone()
            };
            Some((AtomicOpValue::Polyline(old), AtomicOpValue::Polyline(new)))
        } else {
            None
        }
    }
}

impl Command for ScaleCommand {
    fn name(&self) -> &'static str {
        "SCALE"
    }

    fn prompt(&self) -> String {
        if self.base_point.is_none() {
            "Specify base point:".to_string()
        } else {
            "Specify scale factor (e.g., 2 for 2x, 0.5 for half):".to_string()
        }
    }

    fn steps_remaining(&self) -> usize {
        if self.base_point.is_none() { 2 } else { 1 }
    }

    fn on_input(&mut self, input: CommandInput, world: &mut World) -> CommandResult {
        if self.selected_entities.is_empty() {
            return CommandResult::Error(
                "No entities selected. Select objects before running SCALE.".to_string(),
            );
        }

        match input {
            CommandInput::Point(p) => {
                if self.base_point.is_none() {
                    // Step 1: store base point.
                    self.base_point = Some(p);
                    CommandResult::Continue
                } else {
                    // Step 2: point input is supported only via text factor.
                    CommandResult::Error(
                        "Enter a numeric scale factor (e.g., 2 for 2x).".to_string(),
                    )
                }
            }
            CommandInput::Text(s) => {
                if self.base_point.is_none() {
                    return CommandResult::Error(
                        "Specify a base point first.".to_string(),
                    );
                }

                let factor: f64 = match s.parse() {
                    Ok(v) if v != 0.0 => v,
                    Ok(_) => {
                        return CommandResult::Error(
                            "Scale factor must be non-zero.".to_string(),
                        );
                    }
                    Err(_) => {
                        return CommandResult::Error(
                            format!("Invalid scale factor: '{s}'. Enter a number (e.g., 2 for 2x)."),
                        );
                    }
                };

                let base = self.base_point.unwrap();
                self.pending_transaction =
                    Some(self.apply_scale(world, base, factor));
                CommandResult::Complete
            }
            CommandInput::Confirm => {
                CommandResult::Error(
                    "Specify a scale factor or second point.".to_string(),
                )
            }
            CommandInput::Cancel => {
                self.on_cancel(world);
                CommandResult::Cancelled
            }
            _ => CommandResult::Error("Specify a factor or point.".to_string()),
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

    #[allow(dead_code)]
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

        let cmd = ScaleCommand::new(&sel);
        assert_eq!(cmd.selected_entities.len(), 1);
        assert!(!cmd.is_empty());
    }

    #[test]
    fn new_with_empty_selection_is_empty() {
        let sel = SelectionManager::new();
        let cmd = ScaleCommand::new(&sel);
        assert!(cmd.is_empty());
    }

    #[test]
    fn name_and_prompt() {
        let sel = SelectionManager::new();
        let cmd = ScaleCommand::new(&sel);
        assert_eq!(cmd.name(), "SCALE");
        assert_eq!(cmd.prompt(), "Specify base point:");
        assert_eq!(cmd.steps_remaining(), 2);
    }

    // -- scale via text factor ---------------------------------------------

    #[test]
    fn scale_2x_line_around_origin() {
        let mut world = World::new();
        let e = make_line(&mut world, Point2D::new(0.0, 0.0), Point2D::new(10.0, 0.0));

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let mut cmd = ScaleCommand::new(&sel);

        // Step 1: base point at origin
        let r1 = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);
        assert!(matches!(r1, CommandResult::Continue));

        // Step 2: factor = 2
        let r2 = cmd.on_input(CommandInput::Text("2".to_string()), &mut world);
        assert!(matches!(r2, CommandResult::Complete));

        let line = world.get::<&LineData>(e).unwrap();
        assert_eq!(line.start, Point2D::new(0.0, 0.0), "start unchanged (at base)");
        assert_eq!(line.end, Point2D::new(20.0, 0.0), "end scaled 2x");
    }

    #[test]
    fn scale_half_point_around_origin() {
        let mut world = World::new();
        let e = make_line(&mut world, Point2D::new(0.0, 0.0), Point2D::new(10.0, 0.0));

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let mut cmd = ScaleCommand::new(&sel);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);
        let _ = cmd.on_input(CommandInput::Text("0.5".to_string()), &mut world);

        let line = world.get::<&LineData>(e).unwrap();
        assert_eq!(line.start, Point2D::new(0.0, 0.0));
        assert_eq!(line.end, Point2D::new(5.0, 0.0), "end scaled 0.5x");
    }

    #[test]
    fn scale_around_non_origin_point() {
        let mut world = World::new();
        let e = make_line(&mut world, Point2D::new(10.0, 10.0), Point2D::new(20.0, 10.0));

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let mut cmd = ScaleCommand::new(&sel);
        // Base point = (10, 10) — line start is at base
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(10.0, 10.0)), &mut world);
        let _ = cmd.on_input(CommandInput::Text("2".to_string()), &mut world);

        let line = world.get::<&LineData>(e).unwrap();
        assert_eq!(line.start, Point2D::new(10.0, 10.0), "start at base (unchanged)");
        assert_eq!(line.end, Point2D::new(30.0, 10.0), "end scaled 2x around base");
    }

    // -- circle scaling ----------------------------------------------------

    #[test]
    fn scale_circle_center_and_radius_scale() {
        let mut world = World::new();
        let e = make_circle(&mut world, Point2D::new(5.0, 0.0), 3.0);

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let mut cmd = ScaleCommand::new(&sel);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);
        let _ = cmd.on_input(CommandInput::Text("2".to_string()), &mut world);

        let circle = world.get::<&CircleData>(e).unwrap();
        assert_eq!(circle.center, Point2D::new(10.0, 0.0), "center scaled 2x");
        assert_eq!(circle.radius, 6.0, "radius scaled 2x");
    }

    // -- polyline scaling --------------------------------------------------

    #[test]
    fn scale_polyline_all_vertices_scale() {
        let mut world = World::new();
        let e = make_polyline(&mut world);

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let mut cmd = ScaleCommand::new(&sel);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);
        let _ = cmd.on_input(CommandInput::Text("2".to_string()), &mut world);

        let poly = world.get::<&PolylineData>(e).unwrap();
        assert_eq!(poly.vertices[0], Point2D::new(0.0, 0.0), "origin vertex unchanged");
        assert_eq!(poly.vertices[1], Point2D::new(10.0, 10.0), "mid vertex scaled 2x");
        assert_eq!(poly.vertices[2], Point2D::new(20.0, 0.0), "end vertex scaled 2x");
    }

    // -- empty selection ---------------------------------------------------

    #[test]
    fn scale_with_empty_selection_returns_error() {
        let mut world = World::new();
        let sel = SelectionManager::new();

        let mut cmd = ScaleCommand::new(&sel);
        let result = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);
        assert!(matches!(result, CommandResult::Error(_)));
    }

    // -- invalid text ------------------------------------------------------

    #[test]
    fn scale_invalid_text_returns_error() {
        let mut world = World::new();
        let e = make_line(&mut world, Point2D::new(0.0, 0.0), Point2D::new(10.0, 0.0));

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let mut cmd = ScaleCommand::new(&sel);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);

        let result = cmd.on_input(CommandInput::Text("not_a_number".to_string()), &mut world);
        assert!(matches!(result, CommandResult::Error(_)));
    }

    #[test]
    fn scale_negative_factor_mirrors_line() {
        let mut world = World::new();
        let e = make_line(&mut world, Point2D::new(0.0, 0.0), Point2D::new(10.0, 0.0));

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let mut cmd = ScaleCommand::new(&sel);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);

        // factor = -1 → mirror across origin
        let r = cmd.on_input(CommandInput::Text("-1".to_string()), &mut world);
        assert!(matches!(r, CommandResult::Complete), "negative factor should succeed");

        let line = world.get::<&LineData>(e).unwrap();
        assert_eq!(line.start, Point2D::new(0.0, 0.0), "start at origin (unchanged)");
        assert_eq!(line.end, Point2D::new(-10.0, 0.0), "end mirrored across origin");
    }

    #[test]
    fn scale_negative_factor_keeps_circle_radius_positive() {
        let mut world = World::new();
        let e = make_circle(&mut world, Point2D::new(5.0, 0.0), 3.0);

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let mut cmd = ScaleCommand::new(&sel);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);

        let r = cmd.on_input(CommandInput::Text("-2".to_string()), &mut world);
        assert!(matches!(r, CommandResult::Complete));

        let circle = world.get::<&CircleData>(e).unwrap();
        assert_eq!(circle.center, Point2D::new(-10.0, 0.0), "center mirrored");
        assert_eq!(circle.radius, 6.0, "radius uses abs(factor)");
    }

    #[test]
    fn scale_zero_factor_returns_error() {
        let mut world = World::new();
        let e = make_line(&mut world, Point2D::new(0.0, 0.0), Point2D::new(10.0, 0.0));

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let mut cmd = ScaleCommand::new(&sel);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);

        let result = cmd.on_input(CommandInput::Text("0".to_string()), &mut world);
        assert!(matches!(result, CommandResult::Error(_)));
    }

    // -- point-to-point returns error (forge-ki0) -------------------------

    #[test]
    fn scale_point_after_base_returns_error() {
        let mut world = World::new();
        let e = make_line(&mut world, Point2D::new(0.0, 0.0), Point2D::new(10.0, 0.0));

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let mut cmd = ScaleCommand::new(&sel);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);

        let result = cmd.on_input(CommandInput::Point(Point2D::new(5.0, 0.0)), &mut world);
        assert!(matches!(result, CommandResult::Error(_)));
        assert!(cmd.take_transaction().is_none(), "no transaction on point click");
    }

    #[test]
    fn scale_prompt_after_base_mentions_text_only() {
        let mut world = World::new();
        let e = make_line(&mut world, Point2D::new(0.0, 0.0), Point2D::new(10.0, 0.0));

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let mut cmd = ScaleCommand::new(&sel);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(0.0, 0.0)), &mut world);

        assert!(cmd.prompt().contains("2 for 2x"), "prompt should mention text factor");
    }

    // -- cancel ------------------------------------------------------------

    #[test]
    fn scale_cancel_clears_state() {
        let mut world = World::new();
        let e = make_line(&mut world, Point2D::new(0.0, 0.0), Point2D::new(10.0, 0.0));

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let mut cmd = ScaleCommand::new(&sel);
        let _ = cmd.on_input(CommandInput::Point(Point2D::new(5.0, 5.0)), &mut world);

        let result = cmd.on_input(CommandInput::Cancel, &mut world);
        assert!(matches!(result, CommandResult::Cancelled));
        assert!(cmd.base_point.is_none());
        assert!(cmd.take_transaction().is_none());

        let line = world.get::<&LineData>(e).unwrap();
        assert_eq!(line.end, Point2D::new(10.0, 0.0), "entity unchanged after cancel");
    }
}
