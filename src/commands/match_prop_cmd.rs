//! MATCHPROP command implementation.
//!
//! 2-step toolbar command:
//!   1. User picks a source entity (click on geometry).
//!   2. User confirms → source's [`PropertySource`] and [`LayerRef`] are
//!      applied to all previously-selected target entities.
//!
//! The source entity is found via a simple point-in-geometry hit-test
//! using the ECS `World` directly.

use hecs::{Entity, World};
use tracing;

use super::{Command, CommandInput, CommandResult, PreviewEntity};
use crate::ecs::components::{
    ArcData, CircleData, LayerRef, LineData, PolylineData, PropertySource,
};
use crate::history::{AtomicOp, Transaction};
use crate::selection::SelectionManager;

/// Pick tolerance in drawing units.
const PICK_TOLERANCE: f64 = 5.0;

// ─── Hit‑test helper ──────────────────────────────────────────────────────────

/// Find the closest `Renderable` entity whose geometry is within
/// `PICK_TOLERANCE` of `point`.
///
/// Checks [`LineData`], [`CircleData`], [`ArcData`], and [`PolylineData`]
/// components in that order.  Returns the entity handle and the distance, or
/// `None` if nothing is close enough.
fn hit_test(world: &World, point: crate::geometry::Point2D) -> Option<Entity> {
    let mut best: Option<(Entity, f64)> = None;

    // Lines
    for (entity, line) in world.query::<&LineData>().iter() {
        let d = point_to_line_segment_distance(point, line.start, line.end);
        if d < PICK_TOLERANCE && best.is_none_or(|(_, bd)| d < bd) {
            best = Some((entity, d));
        }
    }

    // Circles
    for (entity, circle) in world.query::<&CircleData>().iter() {
        let dx = point.x - circle.center.x;
        let dy = point.y - circle.center.y;
        let d = (dx * dx + dy * dy).sqrt() - circle.radius;
        let d = d.abs();
        if d < PICK_TOLERANCE && best.is_none_or(|(_, bd)| d < bd) {
            best = Some((entity, d));
        }
    }

    // Arcs
    for (entity, arc) in world.query::<&ArcData>().iter() {
        let dx = point.x - arc.center.x;
        let dy = point.y - arc.center.y;
        let center_dist = (dx * dx + dy * dy).sqrt();
        let radius_dist = (center_dist - arc.radius).abs();
        if radius_dist < PICK_TOLERANCE {
            // Also check angular range
            let angle = (point.y - arc.center.y).atan2(point.x - arc.center.x);
            let in_range = angle_in_range(angle, arc.start_angle, arc.end_angle);
            if in_range && best.is_none_or(|(_, bd)| radius_dist < bd) {
                best = Some((entity, radius_dist));
            }
        }
    }

    // Polylines
    for (entity, poly) in world.query::<&PolylineData>().iter() {
        for window in poly.vertices.windows(2) {
            let d = point_to_line_segment_distance(point, window[0], window[1]);
            if d < PICK_TOLERANCE && best.is_none_or(|(_, bd)| d < bd) {
                best = Some((entity, d));
            }
        }
        if poly.closed && poly.vertices.len() >= 2 {
            let d = point_to_line_segment_distance(
                point,
                *poly.vertices.last().unwrap(),
                poly.vertices[0],
            );
            if d < PICK_TOLERANCE && best.is_none_or(|(_, bd)| d < bd) {
                best = Some((entity, d));
            }
        }
    }

    best.map(|(e, _)| e)
}

/// Minimum distance from `p` to the line segment `a`–`b`.
///
/// When `a == b` (zero-length segment, e.g. duplicate polyline vertex),
/// returns the distance from `p` to `a` directly instead of computing
/// a NaN projection.
fn point_to_line_segment_distance(
    p: crate::geometry::Point2D,
    a: crate::geometry::Point2D,
    b: crate::geometry::Point2D,
) -> f64 {
    let ab = b - a;
    let dot_ab_ab = ab.x * ab.x + ab.y * ab.y;

    // Zero-length segment — return distance from p to a.
    if dot_ab_ab == 0.0 {
        let dx = p.x - a.x;
        let dy = p.y - a.y;
        return (dx * dx + dy * dy).sqrt();
    }

    let ap = p - a;
    let dot_ap_ab = ap.x * ab.x + ap.y * ab.y;
    let t = (dot_ap_ab / dot_ab_ab).clamp(0.0, 1.0);
    let closest = a + ab * t;
    let dx = p.x - closest.x;
    let dy = p.y - closest.y;
    (dx * dx + dy * dy).sqrt()
}

/// Check whether `angle` lies between `start` and `end` (radians, may wrap).
fn angle_in_range(angle: f64, start: f64, end: f64) -> bool {
    let mut a = angle;
    let mut s = start;
    let mut e = end;
    // Normalise to [0, 2π)
    let two_pi = 2.0 * std::f64::consts::PI;
    while a < 0.0 { a += two_pi; }
    while a >= two_pi { a -= two_pi; }
    while s < 0.0 { s += two_pi; }
    while s >= two_pi { s -= two_pi; }
    while e < 0.0 { e += two_pi; }
    while e >= two_pi { e -= two_pi; }

    if s <= e {
        s <= a && a <= e
    } else {
        // Wraps around 0
        a >= s || a <= e
    }
}

// ─── MatchPropCommand ─────────────────────────────────────────────────────────

/// MATCHPROP command — copy visual properties from a source entity to targets.
pub struct MatchPropCommand {
    /// Target entities (selected before the command was invoked).
    targets: Vec<Entity>,
    /// Source entity (set in step 1 via point‑and‑click).
    source: Option<Entity>,
    /// Current step (0 = pick source, 1 = confirm).
    step: u8,
    /// Human‑readable error for the current step.
    last_error: Option<String>,
}

impl MatchPropCommand {
    /// Create a new MATCHPROP command, capturing the current selection as targets.
    pub fn new(selection: &SelectionManager) -> Self {
        Self {
            targets: selection.selected.iter().copied().collect(),
            source: None,
            step: 0,
            last_error: None,
        }
    }
}

impl Command for MatchPropCommand {
    fn name(&self) -> &'static str {
        "MATCHPROP"
    }

    fn prompt(&self) -> String {
        if let Some(ref msg) = self.last_error {
            return msg.clone();
        }
        match self.step {
            0 => "Select source entity (click on geometry):".to_string(),
            1 => {
                let count = self.targets.len()
                    - usize::from(self.targets.iter().any(|e| Some(*e) == self.source));
                format!(
                    "Press Enter to match properties to {} entity(ies), or Esc to cancel.",
                    count
                )
            }
            _ => unreachable!(),
        }
    }

    fn steps_remaining(&self) -> usize {
        match self.step {
            0 => 2,
            1 => 1,
            _ => 0,
        }
    }

    fn on_input(&mut self, input: CommandInput, world: &mut World) -> CommandResult {
        self.last_error = None;

        match self.step {
            0 => match input {
                CommandInput::Point(pt) => {
                    // Hit‑test to find the source entity
                    match hit_test(world, pt) {
                        Some(entity) => {
                            tracing::debug!("MATCHPROP source entity = {:?}", entity);
                            self.source = Some(entity);
                            self.step = 1;
                            CommandResult::Continue
                        }
                        None => {
                            self.last_error = Some(
                                "No entity found at that point. Try again or press Esc to cancel."
                                    .to_string(),
                            );
                            CommandResult::Error(
                                "No entity found at that point.".to_string(),
                            )
                        }
                    }
                }
                CommandInput::Cancel => {
                    self.on_cancel(world);
                    CommandResult::Cancelled
                }
                _ => {
                    // All other input — repeat the prompt
                    CommandResult::Continue
                }
            },
            1 => match input {
                CommandInput::Confirm => {
                    let source = match self.source {
                        Some(e) => e,
                        None => {
                            return CommandResult::Error(
                                "No source entity selected.".to_string(),
                            );
                        }
                    };

                    // Read source properties (fall back to ByLayer if absent).
                    let src_source = world
                        .get::<&PropertySource>(source)
                        .as_deref()
                        .copied()
                        .unwrap_or(PropertySource::ByLayer);
                    let src_layer = world.get::<&LayerRef>(source).ok().as_deref().copied();

                    let mut tx = Transaction::new("Match Properties");

                    for &target in &self.targets {
                        if target == source {
                            continue;
                        }

                        // --- PropertySource ---
                        let old_source = world
                            .get::<&PropertySource>(target)
                            .as_deref()
                            .copied()
                            .unwrap_or(PropertySource::ByLayer);
                        let _ = world.insert_one(target, src_source);
                        tx.push(AtomicOp::SetPropertySource {
                            entity: target,
                            old: old_source,
                            new: src_source,
                        });

                        // --- LayerRef ---
                        let old_layer = world.get::<&LayerRef>(target).ok().as_deref().copied();
                        match src_layer {
                            Some(lr) => {
                                let _ = world.insert_one(target, lr);
                                tx.push(AtomicOp::SetLayerRef {
                                    entity: target,
                                    old: old_layer,
                                    new: Some(lr),
                                });
                            }
                            None => {
                                let _ = world.remove_one::<LayerRef>(target);
                                tx.push(AtomicOp::SetLayerRef {
                                    entity: target,
                                    old: old_layer,
                                    new: None,
                                });
                            }
                        }
                    }

                    CommandResult::CompleteWithTransaction(tx)
                }
                CommandInput::Cancel => {
                    self.on_cancel(world);
                    CommandResult::Cancelled
                }
                _ => CommandResult::Continue,
            },
            _ => CommandResult::CompleteWithTransaction(Transaction::new("Match Properties")),
        }
    }

    fn on_cancel(&mut self, _world: &mut World) {
        // Nothing to clean up — no entities spawned.
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
    use crate::ecs::components::Renderable;
    use crate::geometry::Point2D;

    fn create_world() -> World {
        World::new()
    }

    fn spawn_line(world: &mut World, x1: f64, y1: f64, x2: f64, y2: f64) -> Entity {
        world.spawn((
            LineData {
                start: Point2D::new(x1, y1),
                end: Point2D::new(x2, y2),
                color: crate::util::Color::WHITE,
                width: 1.0,
            },
            Renderable,
            PropertySource::ByLayer,
            LayerRef(0),
        ))
    }

    fn spawn_circle(world: &mut World, cx: f64, cy: f64, r: f64) -> Entity {
        world.spawn((
            CircleData {
                center: Point2D::new(cx, cy),
                radius: r,
                color: crate::util::Color::WHITE,
                width: 1.0,
            },
            Renderable,
            PropertySource::ByLayer,
            LayerRef(0),
        ))
    }

    #[test]
    fn test_name_and_prompt() {
        let sel = SelectionManager::new();
        let cmd = MatchPropCommand::new(&sel);
        assert_eq!(cmd.name(), "MATCHPROP");
        assert_eq!(cmd.prompt(), "Select source entity (click on geometry):");
        assert_eq!(cmd.steps_remaining(), 2);
    }

    #[test]
    fn test_step0_point_pick_finds_line() {
        let mut world = create_world();
        let _entity = spawn_line(&mut world, 0.0, 0.0, 100.0, 0.0);

        let sel = SelectionManager::new();
        let mut cmd = MatchPropCommand::new(&sel);

        // Click near the line
        let result = cmd.on_input(CommandInput::Point(Point2D::new(50.0, 2.0)), &mut world);
        assert!(matches!(result, CommandResult::Continue));
        assert_eq!(cmd.step, 1);
        assert!(cmd.source.is_some());
    }

    #[test]
    fn test_step0_point_pick_misses_returns_error() {
        let mut world = create_world();
        let _entity = spawn_line(&mut world, 0.0, 0.0, 100.0, 0.0);

        let sel = SelectionManager::new();
        let mut cmd = MatchPropCommand::new(&sel);

        // Click far away
        let result = cmd.on_input(CommandInput::Point(Point2D::new(500.0, 500.0)), &mut world);
        assert!(matches!(result, CommandResult::Error(_)));
        assert_eq!(cmd.step, 0);
    }

    #[test]
    fn test_confirm_applies_properties_to_targets() {
        let mut world = create_world();

        // Source entity with explicit properties
        let source = world.spawn((
            Renderable,
            PropertySource::Explicit,
            LayerRef(5),
        ));

        // Target entities (selected)
        let target_a = world.spawn((
            Renderable,
            PropertySource::ByLayer,
            LayerRef(0),
        ));
        let target_b = world.spawn((
            Renderable,
            PropertySource::ByBlock,
            LayerRef(1),
        ));

        let mut sel = SelectionManager::new();
        sel.select(&mut world, target_a);
        sel.select(&mut world, target_b);

        let mut cmd = MatchPropCommand::new(&sel);
        cmd.source = Some(source);
        cmd.step = 1;

        let result = cmd.on_input(CommandInput::Confirm, &mut world);
        match result {
            CommandResult::CompleteWithTransaction(tx) => {
                assert_eq!(tx.label, "Match Properties");
                // Should have 4 ops (2 per target: SetPropertySource + SetLayerRef)
                assert_eq!(tx.ops.len(), 4);
            }
            _ => panic!("Expected CompleteWithTransaction, got {:?}", result),
        }

        // Verify properties applied
        assert_eq!(
            *world.get::<&PropertySource>(target_a).unwrap(),
            PropertySource::Explicit
        );
        assert_eq!(
            *world.get::<&PropertySource>(target_b).unwrap(),
            PropertySource::Explicit
        );
        assert_eq!(world.get::<&LayerRef>(target_a).unwrap().0, 5);
        assert_eq!(world.get::<&LayerRef>(target_b).unwrap().0, 5);
    }

    #[test]
    fn test_confirm_skips_source_if_in_targets() {
        let mut world = create_world();
        let source = world.spawn((Renderable, PropertySource::Explicit, LayerRef(5)));
        let target = world.spawn((Renderable, PropertySource::ByLayer, LayerRef(0)));

        let mut sel = SelectionManager::new();
        sel.select(&mut world, source);
        sel.select(&mut world, target);

        let mut cmd = MatchPropCommand::new(&sel);
        cmd.source = Some(source);
        cmd.step = 1;

        let result = cmd.on_input(CommandInput::Confirm, &mut world);
        match result {
            CommandResult::CompleteWithTransaction(tx) => {
                // Only 2 ops (for the real target only)
                assert_eq!(tx.ops.len(), 2);
            }
            _ => panic!("Expected CompleteWithTransaction, got {:?}", result),
        }
    }

    #[test]
    fn test_cancel_at_step0() {
        let mut world = create_world();
        let sel = SelectionManager::new();
        let mut cmd = MatchPropCommand::new(&sel);

        let result = cmd.on_input(CommandInput::Cancel, &mut world);
        assert!(matches!(result, CommandResult::Cancelled));
    }

    #[test]
    fn test_cancel_at_step1() {
        let mut world = create_world();
        let sel = SelectionManager::new();
        let mut cmd = MatchPropCommand::new(&sel);
        cmd.source = Some(world.spawn((Renderable, PropertySource::ByLayer, LayerRef(0))));
        cmd.step = 1;

        let result = cmd.on_input(CommandInput::Cancel, &mut world);
        assert!(matches!(result, CommandResult::Cancelled));
    }

    #[test]
    fn test_confirm_without_source_returns_error() {
        let mut world = create_world();
        let sel = SelectionManager::new();
        let mut cmd = MatchPropCommand::new(&sel);
        cmd.step = 1; // Skip to step 1 without setting source

        let result = cmd.on_input(CommandInput::Confirm, &mut world);
        assert!(matches!(result, CommandResult::Error(_)));
    }

    #[test]
    fn test_hit_test_finds_circle() {
        let mut world = create_world();
        let entity = spawn_circle(&mut world, 50.0, 50.0, 20.0);

        // Click on the circle edge
        let hit = hit_test(&world, Point2D::new(70.0, 50.0));
        assert!(hit.is_some());
        assert_eq!(hit.unwrap(), entity);
    }

    #[test]
    fn test_hit_test_within_tolerance() {
        let mut world = create_world();
        let entity = spawn_line(&mut world, 0.0, 0.0, 100.0, 0.0);

        // Just within PICK_TOLERANCE (5 units)
        let hit = hit_test(&world, Point2D::new(50.0, 4.9));
        assert!(hit.is_some());
        assert_eq!(hit.unwrap(), entity);

        // Just outside PICK_TOLERANCE
        let hit = hit_test(&world, Point2D::new(50.0, 5.1));
        assert!(hit.is_none());
    }

    #[test]
    fn test_empty_world_hit_test() {
        let world = create_world();
        let hit = hit_test(&world, Point2D::new(50.0, 50.0));
        assert!(hit.is_none());
    }
}
