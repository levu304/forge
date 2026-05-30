//! Active grip-drag state machine.
//!
//! [`GripDragState`] tracks a single in-progress grip-drag operation:
//! which grip is being dragged, the world-space start/current positions,
//! and an undo snapshot captured at drag start.  Callers update positions
//! on each mouse move, apply geometry deltas every frame, and finalise the
//! undo transaction when the drag completes.

use crate::ecs::components::{ArcData, CircleData, LineData, PolylineData};
use crate::geometry::Point2D;
use crate::history::ops::AtomicOp;
use hecs::World;

/// Tracks an active grip-drag operation.
///
/// # Lifecycle
///
/// 1. Created via [`GripDragState::new`] when a grip is picked.
/// 2. [`update_position`](GripDragState::update_position) called on each
///    mouse move to track the current world-space cursor position.
/// 3. [`apply`](GripDragState::apply) called each frame to write the
///    geometry delta into the ECS world (the caller provides the current
///    handle list for entity look-up).
/// 4. [`capture_snapshot`](GripDragState::capture_snapshot) called once
///    at drag start (or first mutation) to record pre-drag state.
/// 5. [`finalize_transaction`](GripDragState::finalize_transaction) called
///    once on drag end to produce the final [`AtomicOp`] for undo history.
#[derive(Debug, Clone)]
pub struct GripDragState {
    /// Index into `GripSystem.handles` for the grip being dragged.
    pub handle_index: usize,
    /// Entity that owns the grip being dragged (cached for convenience).
    pub entity: hecs::Entity,
    /// World-space position where the drag started.
    pub start_position: Point2D,
    /// Current world-space position of the drag cursor.
    pub current_position: Point2D,
    /// Snapshot of undo state captured at drag start (old == new).
    /// Finalized by [`finalize_transaction`](GripDragState::finalize_transaction)
    /// which updates the `new` field to post-drag values.
    pub snapshot: Option<AtomicOp>,
}

impl GripDragState {
    /// Create a new drag state for the given handle and entity.
    pub fn new(handle_index: usize, entity: hecs::Entity, start_position: Point2D) -> Self {
        Self {
            handle_index,
            entity,
            start_position,
            current_position: start_position,
            snapshot: None,
        }
    }

    /// Apply the current drag delta to the gripped entity's geometry.
    ///
    /// Computes `delta = current_position - start_position` and updates
    /// the appropriate field(s) of the entity's geometry component.
    ///
    /// | Entity type | Grip type | Effect |
    /// |-------------|-----------|--------|
    /// | `LineData` | Endpoint (idx=0) | Shift `start` by delta |
    /// | `LineData` | Endpoint (idx=1) | Shift `end` by delta |
    /// | `LineData` | Midpoint | No-op (non-interactive in v0.3.0) |
    /// | `CircleData` | Center | Shift `center` by delta |
    /// | `CircleData` | Quadrant | No-op (non-interactive in v0.3.0) |
    /// | `ArcData` | Center | Shift `center` by delta |
    /// | `ArcData` | Endpoint / Midpoint | No-op (deferred in v0.3.0) |
    /// | `PolylineData` | Vertex | Shift vertex at `vertex_index` by delta |
    ///
    /// # Borrow discipline
    ///
    /// Each component check is done inside a block scope so that the
    /// `Ref` borrow guard from `world.get()` is dropped **before** the
    /// `world.insert_one()` write-back — satisfying the borrow checker.
    pub fn apply(&self, world: &mut World, handle: &super::handle::GripHandle) {
        let delta = self.current_position - self.start_position;
        let entity = self.entity;

        // ── Line ────────────────────────────────────────────────────
        if let Some(data) = {
            // Block scope ensures `result` (containing `Ref`) is dropped
            // before the write-back below.
            let result = world.get::<&LineData>(entity);
            match result {
                Ok(component) => {
                    let mut data = *component;
                    match handle.grip_type {
                        super::handle::GripType::Endpoint => {
                            if handle.vertex_index == 0 {
                                data.start = data.start + delta;
                            } else {
                                data.end = data.end + delta;
                            }
                            Some(data)
                        }
                        _ => None,
                    }
                }
                Err(_) => None,
            }
        } {
            world.insert_one(entity, data).ok();
            return;
        }

        // ── Circle ──────────────────────────────────────────────────
        if let Some(data) = {
            let result = world.get::<&CircleData>(entity);
            match result {
                Ok(component) => {
                    let mut data = *component;
                    match handle.grip_type {
                        super::handle::GripType::Center => {
                            data.center = data.center + delta;
                            Some(data)
                        }
                        _ => None,
                    }
                }
                Err(_) => None,
            }
        } {
            world.insert_one(entity, data).ok();
            return;
        }

        // ── Arc ─────────────────────────────────────────────────────
        if let Some(data) = {
            let result = world.get::<&ArcData>(entity);
            match result {
                Ok(component) => {
                    let mut data = *component;
                    match handle.grip_type {
                        super::handle::GripType::Center => {
                            data.center = data.center + delta;
                            Some(data)
                        }
                        _ => None,
                    }
                }
                Err(_) => None,
            }
        } {
            world.insert_one(entity, data).ok();
            return;
        }

        // ── Polyline ────────────────────────────────────────────────
        if let Some(data) = {
            let result = world.get::<&PolylineData>(entity);
            match result {
                Ok(component) => {
                    let data = PolylineData::clone(&*component);
                    // Out-of-bounds vertex_index (e.g. after undo shrinks
                    // the polyline without regenerate()) → skip silently.
                    if handle.vertex_index >= data.vertices.len() {
                        None
                    } else {
                        let mut data = data;
                        data.vertices[handle.vertex_index] =
                            data.vertices[handle.vertex_index] + delta;
                        Some(data)
                    }
                }
                Err(_) => None,
            }
        } {
            world.insert_one(entity, data).ok();
        }
    }

    /// Capture the pre-drag state as an undo snapshot.
    ///
    /// Creates an [`AtomicOp`] where `old == new` (the `new` field will
    /// be updated by [`finalize_transaction`](GripDragState::finalize_transaction)
    /// when the drag completes).  Safe to call multiple times — only the
    /// first call populates the snapshot.
    pub fn capture_snapshot(&mut self, world: &World) {
        if self.snapshot.is_some() {
            return; // Already captured
        }

        let entity = self.entity;
        let snapshot = self.read_snapshot(world, entity);
        self.snapshot = snapshot;
    }

    /// Read the current geometry of `entity` and return the matching
    /// [`AtomicOp`] with `old == new`.
    fn read_snapshot(&self, world: &World, entity: hecs::Entity) -> Option<AtomicOp> {
        let result = world.get::<&LineData>(entity);
        if let Ok(component) = result {
            let data = *component;
            return Some(AtomicOp::SetLineData {
                entity,
                old: data,
                new: data,
            });
        }
        let result = world.get::<&CircleData>(entity);
        if let Ok(component) = result {
            let data = *component;
            return Some(AtomicOp::SetCircleData {
                entity,
                old: data,
                new: data,
            });
        }
        let result = world.get::<&ArcData>(entity);
        if let Ok(component) = result {
            let data = *component;
            return Some(AtomicOp::SetArcData {
                entity,
                old: data,
                new: data,
            });
        }
        let result = world.get::<&PolylineData>(entity);
        if let Ok(component) = result {
            let data = PolylineData::clone(&*component);
            return Some(AtomicOp::SetPolylineData {
                entity,
                old: data.clone(),
                new: data,
            });
        }
        None
    }

    /// Finalize the undo transaction with post-drag geometry.
    ///
    /// Reads the entity's current component state, updates the snapshot's
    /// `new` field, and returns the finalized [`AtomicOp`] ready for push
    /// to [`History`](crate::history::History).
    ///
    /// Returns `None` if no snapshot was captured or the entity no longer
    /// has the expected component.
    pub fn finalize_transaction(&mut self, world: &World) -> Option<AtomicOp> {
        // Borrow the snapshot first — don't move it out yet so we don't
        // lose the undo data if the world read fails (e.g. entity was
        // despawned mid-drag).  Cleared below only on success.
        let snapshot = self.snapshot.as_ref()?;
        let entity = self.entity;

        let finalized = match snapshot {
            AtomicOp::SetLineData {
                entity: e,
                old,
                ..
            } => {
                let result = world.get::<&LineData>(entity).ok()?;
                let new = *result;
                AtomicOp::SetLineData {
                    entity: *e,
                    old: *old,
                    new,
                }
            }
            AtomicOp::SetCircleData {
                entity: e,
                old,
                ..
            } => {
                let result = world.get::<&CircleData>(entity).ok()?;
                let new = *result;
                AtomicOp::SetCircleData {
                    entity: *e,
                    old: *old,
                    new,
                }
            }
            AtomicOp::SetArcData {
                entity: e,
                old,
                ..
            } => {
                let result = world.get::<&ArcData>(entity).ok()?;
                let new = *result;
                AtomicOp::SetArcData {
                    entity: *e,
                    old: *old,
                    new,
                }
            }
            AtomicOp::SetPolylineData {
                entity: e,
                old,
                ..
            } => {
                let result = world.get::<&PolylineData>(entity).ok()?;
                let new = PolylineData::clone(&*result);
                AtomicOp::SetPolylineData {
                    entity: *e,
                    old: old.clone(),
                    new,
                }
            }
            _ => {
                // AtomicOp is #[non_exhaustive]; new variants can appear
                // without triggering a compile error.  Surface misuse in
                // debug builds so we catch stale snapshots early.
                debug_assert!(false, "finalize_transaction: unexpected snapshot variant {snapshot:?}");
                return None;
            }
        };

        // Only clear the snapshot after a successful world read.
        self.snapshot = None;
        Some(finalized)
    }

    /// Update the current drag position (called on each mouse move).
    pub fn update_position(&mut self, world_pos: Point2D) {
        self.current_position = world_pos;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::components::*;
    use crate::geometry::Point2D;
    use crate::grip::handle::{GripHandle, GripType};
    use crate::util::Color;
    use hecs::World;

    // ── helpers ─────────────────────────────────────────────────────

    fn make_line(world: &mut World) -> hecs::Entity {
        world.spawn((
            LineData {
                start: Point2D::new(0.0, 0.0),
                end: Point2D::new(10.0, 10.0),
                color: Color::WHITE,
                width: 1.0,
            },
            Selected,
        ))
    }

    fn make_circle(world: &mut World) -> hecs::Entity {
        world.spawn((
            CircleData {
                center: Point2D::new(5.0, 5.0),
                radius: 3.0,
                color: Color::WHITE,
                width: 1.0,
            },
            Selected,
        ))
    }

    fn make_arc(world: &mut World) -> hecs::Entity {
        world.spawn((
            ArcData {
                center: Point2D::new(0.0, 0.0),
                radius: 5.0,
                start_angle: 0.0,
                end_angle: 90.0,
                color: Color::WHITE,
                width: 1.0,
            },
            Selected,
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
            Selected,
        ))
    }

    // ── construction & position ─────────────────────────────────────

    #[test]
    fn test_new_drag_state() {
        let entity = hecs::Entity::from_bits(1u64 << 32 | 1).unwrap();
        let pos = Point2D::new(10.0, 20.0);
        let drag = GripDragState::new(0, entity, pos);

        assert_eq!(drag.handle_index, 0);
        assert_eq!(drag.entity, entity);
        assert_eq!(drag.start_position, pos);
        assert_eq!(drag.current_position, pos);
        assert!(drag.snapshot.is_none());
    }

    #[test]
    fn test_update_position() {
        let entity = hecs::Entity::from_bits(1u64 << 32 | 1).unwrap();
        let mut drag = GripDragState::new(0, entity, Point2D::new(0.0, 0.0));
        drag.update_position(Point2D::new(5.0, 10.0));

        assert_eq!(drag.current_position, Point2D::new(5.0, 10.0));
        // start_position unchanged
        assert_eq!(drag.start_position, Point2D::new(0.0, 0.0));
    }

    // ── apply: line endpoint ────────────────────────────────────────

    #[test]
    fn test_apply_line_start_endpoint() {
        let mut world = World::new();
        let entity = make_line(&mut world);
        let pos = Point2D::new(0.0, 0.0);
        let handle = GripHandle::new(GripType::Endpoint, pos, entity, 0);

        let mut drag = GripDragState::new(0, entity, Point2D::new(0.0, 0.0));
        drag.update_position(Point2D::new(5.0, 0.0));
        drag.apply(&mut world, &handle);

        let line = world.get::<&LineData>(entity).unwrap();
        assert_eq!(line.start, Point2D::new(5.0, 0.0), "start moves");
        assert_eq!(line.end, Point2D::new(10.0, 10.0), "end unchanged");
    }

    #[test]
    fn test_apply_line_end_endpoint() {
        let mut world = World::new();
        let entity = make_line(&mut world);
        let pos = Point2D::new(10.0, 10.0);
        let handle = GripHandle::new(GripType::Endpoint, pos, entity, 1);

        let mut drag = GripDragState::new(0, entity, Point2D::new(0.0, 0.0));
        drag.update_position(Point2D::new(3.0, 4.0));
        drag.apply(&mut world, &handle);

        let line = world.get::<&LineData>(entity).unwrap();
        assert_eq!(line.start, Point2D::new(0.0, 0.0), "start unchanged");
        assert_eq!(
            line.end,
            Point2D::new(13.0, 14.0),
            "end moved by delta (3,4)"
        );
    }

    #[test]
    fn test_apply_line_midpoint_noop() {
        let mut world = World::new();
        let entity = make_line(&mut world);
        let pos = Point2D::new(5.0, 5.0);
        let handle = GripHandle::new(GripType::Midpoint, pos, entity, 0);

        let mut drag = GripDragState::new(0, entity, Point2D::new(0.0, 0.0));
        drag.update_position(Point2D::new(10.0, 10.0));
        drag.apply(&mut world, &handle);

        let line = world.get::<&LineData>(entity).unwrap();
        assert_eq!(line.start, Point2D::new(0.0, 0.0), "start unchanged");
        assert_eq!(line.end, Point2D::new(10.0, 10.0), "end unchanged");
    }

    // ── apply: circle center ────────────────────────────────────────

    #[test]
    fn test_apply_circle_center() {
        let mut world = World::new();
        let entity = make_circle(&mut world);
        let pos = Point2D::new(5.0, 5.0);
        let handle = GripHandle::new(GripType::Center, pos, entity, 0);

        let mut drag = GripDragState::new(0, entity, Point2D::new(0.0, 0.0));
        drag.update_position(Point2D::new(-2.0, 3.0));
        drag.apply(&mut world, &handle);

        let circle = world.get::<&CircleData>(entity).unwrap();
        assert_eq!(
            circle.center,
            Point2D::new(3.0, 8.0),
            "center shifted by delta (-2,3)"
        );
    }

    #[test]
    fn test_apply_circle_quadrant_noop() {
        let mut world = World::new();
        let entity = make_circle(&mut world);
        let pos = Point2D::new(8.0, 5.0); // 0° quadrant
        let handle = GripHandle::new(GripType::Quadrant, pos, entity, 0);

        let mut drag = GripDragState::new(0, entity, Point2D::new(0.0, 0.0));
        drag.update_position(Point2D::new(10.0, 10.0));
        drag.apply(&mut world, &handle);

        let circle = world.get::<&CircleData>(entity).unwrap();
        assert_eq!(circle.center, Point2D::new(5.0, 5.0), "center unchanged");
        assert_eq!(circle.radius, 3.0, "radius unchanged");
    }

    // ── apply: arc center / endpoint / midpoint ─────────────────────

    #[test]
    fn test_apply_arc_center() {
        let mut world = World::new();
        let entity = make_arc(&mut world);
        let pos = Point2D::new(0.0, 0.0);
        let handle = GripHandle::new(GripType::Center, pos, entity, 0);

        let mut drag = GripDragState::new(0, entity, Point2D::new(0.0, 0.0));
        drag.update_position(Point2D::new(2.0, -3.0));
        drag.apply(&mut world, &handle);

        let arc = world.get::<&ArcData>(entity).unwrap();
        assert_eq!(
            arc.center,
            Point2D::new(2.0, -3.0),
            "center shifted by delta"
        );
    }

    #[test]
    fn test_apply_arc_endpoint_noop() {
        // v0.3.0: dragging arc endpoints is deferred — must be no-op.
        let mut world = World::new();
        let entity = make_arc(&mut world);
        // Arc: center=(0,0), radius=5, start=0° → end point at (5,  0)
        let pos = Point2D::new(5.0, 0.0);
        let handle = GripHandle::new(GripType::Endpoint, pos, entity, 0);

        let mut drag = GripDragState::new(0, entity, Point2D::new(0.0, 0.0));
        drag.update_position(Point2D::new(10.0, 10.0));
        drag.apply(&mut world, &handle);

        let arc = world.get::<&ArcData>(entity).unwrap();
        assert_eq!(arc.center, Point2D::new(0.0, 0.0), "center unchanged");
        assert_eq!(arc.radius, 5.0, "radius unchanged");
        assert_eq!(arc.start_angle, 0.0, "start_angle unchanged");
        assert_eq!(arc.end_angle, 90.0, "end_angle unchanged");
    }

    #[test]
    fn test_apply_arc_midpoint_noop() {
        // v0.3.0: dragging arc midpoints is deferred — must be no-op.
        let mut world = World::new();
        let entity = make_arc(&mut world);
        let pos = Point2D::new(5.0 * std::f64::consts::FRAC_1_SQRT_2, 5.0 * std::f64::consts::FRAC_1_SQRT_2);
        let handle = GripHandle::new(GripType::Midpoint, pos, entity, 0);

        let mut drag = GripDragState::new(0, entity, Point2D::new(0.0, 0.0));
        drag.update_position(Point2D::new(10.0, 10.0));
        drag.apply(&mut world, &handle);

        let arc = world.get::<&ArcData>(entity).unwrap();
        assert_eq!(arc.center, Point2D::new(0.0, 0.0), "center unchanged");
        assert_eq!(arc.radius, 5.0, "radius unchanged");
        assert_eq!(arc.start_angle, 0.0, "start_angle unchanged");
        assert_eq!(arc.end_angle, 90.0, "end_angle unchanged");
    }

    // ── apply: polyline vertex ──────────────────────────────────────

    #[test]
    fn test_apply_polyline_vertex() {
        let mut world = World::new();
        let entity = make_polyline(&mut world);
        let pos = Point2D::new(5.0, 5.0);
        let handle = GripHandle::new(GripType::Vertex, pos, entity, 1);

        let mut drag = GripDragState::new(0, entity, Point2D::new(0.0, 0.0));
        drag.update_position(Point2D::new(-5.0, 5.0));
        drag.apply(&mut world, &handle);

        let poly = world.get::<&PolylineData>(entity).unwrap();
        assert_eq!(poly.vertices[0], Point2D::new(0.0, 0.0), "v0 unchanged");
        assert_eq!(
            poly.vertices[1],
            Point2D::new(0.0, 10.0),
            "v1 shifted by delta (-5,5)"
        );
        assert_eq!(poly.vertices[2], Point2D::new(10.0, 0.0), "v2 unchanged");
    }

    #[test]
    fn test_apply_polyline_vertex_oob_noop() {
        // vertex_index=99 on a 3-vertex polyline → skip without panic
        // and without modifying any vertex.
        let mut world = World::new();
        let entity = make_polyline(&mut world);
        let pos = Point2D::new(0.0, 0.0);
        let handle = GripHandle::new(GripType::Vertex, pos, entity, 99);

        let mut drag = GripDragState::new(0, entity, Point2D::new(0.0, 0.0));
        drag.update_position(Point2D::new(100.0, 100.0));
        drag.apply(&mut world, &handle);

        let poly = world.get::<&PolylineData>(entity).unwrap();
        assert_eq!(poly.vertices.len(), 3, "vertices unchanged");
        assert_eq!(poly.vertices[0], Point2D::new(0.0, 0.0), "v0 unchanged");
        assert_eq!(poly.vertices[1], Point2D::new(5.0, 5.0), "v1 unchanged");
        assert_eq!(poly.vertices[2], Point2D::new(10.0, 0.0), "v2 unchanged");
    }

    // ── snapshot / finalize ─────────────────────────────────────────

    #[test]
    fn test_capture_snapshot_line_old_equals_new() {
        let mut world = World::new();
        let entity = make_line(&mut world);
        let mut drag = GripDragState::new(0, entity, Point2D::new(0.0, 0.0));

        drag.capture_snapshot(&world);
        let snap = drag.snapshot.as_ref().unwrap();

        match snap {
            AtomicOp::SetLineData {
                entity: e,
                old,
                new,
            } => {
                assert_eq!(*e, entity);
                assert_eq!(old.start, new.start, "old == new initially");
                assert_eq!(old.end, new.end, "old == new initially");
            }
            other => panic!("expected SetLineData, got {other:?}"),
        }
    }

    #[test]
    fn test_capture_snapshot_idempotent() {
        let mut world = World::new();
        let entity = make_line(&mut world);
        let mut drag = GripDragState::new(0, entity, Point2D::new(0.0, 0.0));

        drag.capture_snapshot(&world);

        // Apply a change, then capture again — should be no-op (already captured)
        let handle = GripHandle::new(GripType::Endpoint, Point2D::new(0.0, 0.0), entity, 0);
        let mut drag2 = drag.clone();
        drag2.update_position(Point2D::new(99.0, 99.0));
        drag2.apply(&mut world, &handle);
        drag2.capture_snapshot(&world);

        // Snapshot should still reflect the ORIGINAL (pre-drag) state
        match &drag2.snapshot {
            Some(AtomicOp::SetLineData { old, .. }) => {
                assert_eq!(old.start.x, 0.0, "snapshot captured pre-drag state");
            }
            _ => panic!("expected SetLineData snapshot"),
        }
    }

    #[test]
    fn test_finalize_transaction_updates_new() {
        let mut world = World::new();
        let entity = make_line(&mut world);
        let mut drag = GripDragState::new(0, entity, Point2D::new(0.0, 0.0));

        drag.capture_snapshot(&world);

        // Apply a drag
        let handle = GripHandle::new(GripType::Endpoint, Point2D::new(0.0, 0.0), entity, 0);
        drag.update_position(Point2D::new(7.0, 8.0));
        drag.apply(&mut world, &handle);

        // Finalize
        let finalized = drag.finalize_transaction(&world).unwrap();

        match finalized {
            AtomicOp::SetLineData { old, new, .. } => {
                assert_eq!(old.start, Point2D::new(0.0, 0.0), "old is pre-drag");
                assert_eq!(new.start, Point2D::new(7.0, 8.0), "new is post-drag");
            }
            _ => panic!("expected SetLineData"),
        }
    }

    #[test]
    fn test_finalize_transaction_returns_none_without_snapshot() {
        let world = World::new();
        let entity = hecs::Entity::from_bits(1u64 << 32 | 1).unwrap();
        let mut drag = GripDragState::new(0, entity, Point2D::new(0.0, 0.0));

        assert!(drag.finalize_transaction(&world).is_none());
    }

    #[test]
    fn test_finalize_transaction_after_despawn_returns_none() {
        // If the entity is despawned between capture_snapshot() and
        // finalize_transaction(), the snapshot must NOT be consumed/lost
        // — return None but keep the snapshot for potential recovery.
        let mut world = World::new();
        let entity = make_line(&mut world);
        let mut drag = GripDragState::new(0, entity, Point2D::new(0.0, 0.0));

        drag.capture_snapshot(&world);
        assert!(drag.snapshot.is_some(), "snapshot captured");

        // Despawn the entity
        world.despawn(entity).unwrap();

        // finalize_transaction returns None because entity is gone
        assert!(
            drag.finalize_transaction(&world).is_none(),
            "should return None when entity is despawned"
        );

        // Snapshot should NOT be consumed — undo data preserved
        assert!(
            drag.snapshot.is_some(),
            "snapshot must survive failed finalize"
        );
    }

    #[test]
    fn test_capture_snapshot_circle() {
        let mut world = World::new();
        let entity = make_circle(&mut world);
        let mut drag = GripDragState::new(0, entity, Point2D::new(0.0, 0.0));

        drag.capture_snapshot(&world);
        let snap = drag.snapshot.as_ref().unwrap();

        match snap {
            AtomicOp::SetCircleData { old, new, .. } => {
                assert_eq!(old.center.x, 5.0);
                assert_eq!(old.center, new.center);
            }
            other => panic!("expected SetCircleData, got {other:?}"),
        }
    }

    #[test]
    fn test_finalize_transaction_polyline() {
        let mut world = World::new();
        let entity = make_polyline(&mut world);
        let mut drag = GripDragState::new(0, entity, Point2D::new(0.0, 0.0));

        drag.capture_snapshot(&world);

        let handle = GripHandle::new(GripType::Vertex, Point2D::new(5.0, 5.0), entity, 1);
        drag.update_position(Point2D::new(5.0, -5.0));
        drag.apply(&mut world, &handle);

        let finalized = drag.finalize_transaction(&world).unwrap();

        match finalized {
            AtomicOp::SetPolylineData { old, new, .. } => {
                assert_eq!(old.vertices[1], Point2D::new(5.0, 5.0), "old is pre-drag");
                assert_eq!(
                    new.vertices[1],
                    Point2D::new(10.0, 0.0),
                    "new is post-drag"
                );
            }
            other => panic!("expected SetPolylineData, got {other:?}"),
        }
    }

    #[test]
    fn test_finalize_transaction_arc() {
        let mut world = World::new();
        let entity = make_arc(&mut world);
        let mut drag = GripDragState::new(0, entity, Point2D::new(0.0, 0.0));

        drag.capture_snapshot(&world);

        let handle = GripHandle::new(GripType::Center, Point2D::new(0.0, 0.0), entity, 0);
        drag.update_position(Point2D::new(10.0, 20.0));
        drag.apply(&mut world, &handle);

        let finalized = drag.finalize_transaction(&world).unwrap();

        match finalized {
            AtomicOp::SetArcData { old, new, .. } => {
                assert_eq!(old.center, Point2D::new(0.0, 0.0));
                assert_eq!(new.center, Point2D::new(10.0, 20.0));
            }
            other => panic!("expected SetArcData, got {other:?}"),
        }
    }
}
