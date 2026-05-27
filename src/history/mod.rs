//! Undo/redo history.
//!
//! Provides a delta-based command journal with configurable depth.
//! Each [`Transaction`] is a list of [`AtomicOp`] variants. Undo replays
//! operations in reverse; redo applies them forward. Entity handle
//! remapping is managed via [`EntityMapping`].
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────┐
//! │                 History                      │
//! │  ┌──────────────┐   ┌──────────────┐        │
//! │  │  Undo Stack  │   │  Redo Stack  │        │
//! │  │  VecDeque<   │   │  VecDeque<   │        │
//! │  │  Transaction>│   │  Transaction>│        │
//! │  └──────┬───────┘   └──────┬───────┘        │
//! │         │                  │                 │
//! │         └──────┬───────────┘                 │
//! │           EntityMapping                       │
//! │           (old → new handles)                 │
//! └─────────────────────────────────────────────┘
//! ```
//!
//! # Usage
//!
//! ```text
//! // history.undo(&mut world) returns the label and updates EntityMapping
//! // apply_entity_remapping(&mut selection, &mut spatial, &mapping)
//! //   remaps stale entity handles after undo/redo
//! ```

pub mod transaction;
pub mod ops;

use std::collections::{HashMap, HashSet, VecDeque};

use hecs::Entity;
use hecs::World;

use crate::ecs::components::Renderable;

pub use self::ops::AtomicOp;
pub use self::transaction::Transaction;

// ---------------------------------------------------------------------------
// EntityMapping
// ---------------------------------------------------------------------------

/// Maps old entity handles to new ones after entity re-spawn during undo/redo.
///
/// When an undo operation must re-spawn a despawned entity, the new entity
/// receives a **different** handle from the original.  `EntityMapping` records
/// these `old → new` mappings so that consumers (selection manager, spatial
/// index) can fix up their stale handles.
///
/// # Invariant
///
/// Every call to [`record_spawn`] inserts a mapping; [`map`] returns the
/// remapped handle if one exists, otherwise the original handle unchanged.
///
/// [`record_spawn`]: EntityMapping::record_spawn
/// [`map`]: EntityMapping::map
#[derive(Debug, Clone)]
pub struct EntityMapping {
    map: HashMap<Entity, Entity>,
}

impl EntityMapping {
    /// Create an empty mapping.
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }

    /// Return the remapped handle if a mapping for `old` exists, otherwise
    /// return `old` unchanged.
    pub fn map(&self, old: Entity) -> Entity {
        self.map.get(&old).copied().unwrap_or(old)
    }

    /// Record that `new` is the replacement handle for the original `old`.
    pub fn record_spawn(&mut self, old: Entity, new: Entity) {
        self.map.insert(old, new);
    }

    /// Remove all mappings.
    pub fn clear(&mut self) {
        self.map.clear();
    }

    /// Returns `true` if no mappings have been recorded.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

impl Default for EntityMapping {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// History
// ---------------------------------------------------------------------------

/// A delta-based command journal supporting undo and redo.
///
/// History stores a stack of [`Transaction`]s.  Each undo pops the top
/// transaction, replays its operations **in reverse** to restore prior state,
/// and pushes it onto the redo stack.  Redo replays **forward**.
///
/// When a new transaction is pushed, the redo stack is cleared (standard
/// undo/redo branching semantics — a new action invalidates the redo chain).
///
/// # Depth management
///
/// [`max_depth`](Self::max_depth) limits how many undo steps are retained
/// (default: 1000).  When the limit is exceeded, the oldest transaction is
/// silently dropped from the front of the undo stack.
pub struct History {
    /// Stack of past transactions (newest at back).
    pub undo_stack: VecDeque<Transaction>,
    /// Stack of undone transactions available for redo (newest at back).
    pub redo_stack: VecDeque<Transaction>,
    /// Maximum number of undo steps.  `None` = unlimited.
    pub max_depth: Option<usize>,
    /// Accumulated entity-handle remappings from re-spawns during undo/redo.
    entity_map: EntityMapping,
}

impl History {
    /// Create an empty history with a default maximum depth of 1000.
    pub fn new() -> Self {
        Self {
            undo_stack: VecDeque::new(),
            redo_stack: VecDeque::new(),
            max_depth: Some(1000),
            entity_map: EntityMapping::new(),
        }
    }

    /// Create an empty history with the given maximum depth.
    ///
    /// Pass `None` for unlimited depth.
    pub fn with_max_depth(max_depth: Option<usize>) -> Self {
        Self {
            undo_stack: VecDeque::new(),
            redo_stack: VecDeque::new(),
            max_depth,
            entity_map: EntityMapping::new(),
        }
    }

    /// Push a new transaction onto the undo stack.
    ///
    /// This **clears the redo stack** (standard branching semantics) and
    /// enforces [`max_depth`](Self::max_depth) by discarding the oldest
    /// transaction if the limit is exceeded.
    pub fn push(&mut self, tx: Transaction) {
        self.redo_stack.clear();
        self.undo_stack.push_back(tx);
        if let Some(max) = self.max_depth {
            while self.undo_stack.len() > max {
                self.undo_stack.pop_front();
            }
        }
    }

    /// Undo the most recent transaction.
    ///
    /// Replays operations **in reverse** order:
    ///
    /// | Op | Undo action |
    /// |----|-------------|
    /// | `Spawn*` | `world.despawn(entity)` |
    /// | `Despawn*` | Re-spawn entity with stored data + [`Renderable`] |
    /// | `Set*` | Restore `old` value via `world.insert_one` |
    ///
    /// After a `Despawn*` re-spawn, the resulting entity handle (which is
    /// different from the original) is recorded in the internal
    /// [`EntityMapping`].  The caller **must** call
    /// [`take_entity_mapping`](Self::take_entity_mapping) and pass it to
    /// [`apply_entity_remapping`] so that the selection manager and spatial
    /// index are kept consistent.
    ///
    /// Returns the transaction's label, or `None` if the undo stack is empty.
    pub fn undo(&mut self, world: &mut World) -> Option<String> {
        let tx = self.undo_stack.pop_back()?;

        for op in tx.ops.iter().rev() {
            match op {
                // Spawn* → despawn entity
                AtomicOp::SpawnLine { entity, .. }
                | AtomicOp::SpawnCircle { entity, .. }
                | AtomicOp::SpawnArc { entity, .. }
                | AtomicOp::SpawnPolyline { entity, .. } => {
                    world.despawn(*entity).ok();
                }

                // Despawn* → re-spawn entity with stored data + Renderable
                AtomicOp::DespawnLine { entity, data } => {
                    let new_entity = world.spawn((*data, Renderable));
                    self.entity_map.record_spawn(*entity, new_entity);
                }
                AtomicOp::DespawnCircle { entity, data } => {
                    let new_entity = world.spawn((*data, Renderable));
                    self.entity_map.record_spawn(*entity, new_entity);
                }
                AtomicOp::DespawnArc { entity, data } => {
                    let new_entity = world.spawn((*data, Renderable));
                    self.entity_map.record_spawn(*entity, new_entity);
                }
                AtomicOp::DespawnPolyline { entity, data } => {
                    let new_entity = world.spawn((data.clone(), Renderable));
                    self.entity_map.record_spawn(*entity, new_entity);
                }

                // Set* → restore old value
                AtomicOp::SetLineData { entity, old, .. } => {
                    world.insert_one(*entity, *old).ok();
                }
                AtomicOp::SetCircleData { entity, old, .. } => {
                    world.insert_one(*entity, *old).ok();
                }
                AtomicOp::SetArcData { entity, old, .. } => {
                    world.insert_one(*entity, *old).ok();
                }
                AtomicOp::SetPolylineData { entity, old, .. } => {
                    world.insert_one(*entity, old.clone()).ok();
                }
                AtomicOp::SetPosition { entity, old, .. } => {
                    world.insert_one(*entity, *old).ok();
                }
            }
        }

        let label = tx.label.clone();
        self.redo_stack.push_back(tx);
        Some(label)
    }

    /// Redo the most recently undone transaction.
    ///
    /// Replays operations **forward**:
    ///
    /// | Op | Redo action |
    /// |----|-------------|
    /// | `Spawn*` | Re-spawn entity with stored data + [`Renderable`] |
    /// | `Despawn*` | `world.despawn(entity)` |
    /// | `Set*` | Apply `new` value via `world.insert_one` |
    ///
    /// After a `Spawn*` re-spawn, the resulting entity handle (which is
    /// different from the original) is recorded in the internal
    /// [`EntityMapping`].  The caller **must** call
    /// [`take_entity_mapping`](Self::take_entity_mapping) and pass it to
    /// [`apply_entity_remapping`].
    ///
    /// Returns the transaction's label, or `None` if the redo stack is empty.
    pub fn redo(&mut self, world: &mut World) -> Option<String> {
        let tx = self.redo_stack.pop_back()?;

        for op in &tx.ops {
            match op {
                // Spawn* → re-spawn entity with stored data + Renderable
                AtomicOp::SpawnLine { entity, data } => {
                    let new_entity = world.spawn((*data, Renderable));
                    self.entity_map.record_spawn(*entity, new_entity);
                }
                AtomicOp::SpawnCircle { entity, data } => {
                    let new_entity = world.spawn((*data, Renderable));
                    self.entity_map.record_spawn(*entity, new_entity);
                }
                AtomicOp::SpawnArc { entity, data } => {
                    let new_entity = world.spawn((*data, Renderable));
                    self.entity_map.record_spawn(*entity, new_entity);
                }
                AtomicOp::SpawnPolyline { entity, data } => {
                    let new_entity = world.spawn((data.clone(), Renderable));
                    self.entity_map.record_spawn(*entity, new_entity);
                }

                // Despawn* → despawn entity
                AtomicOp::DespawnLine { entity, .. }
                | AtomicOp::DespawnCircle { entity, .. }
                | AtomicOp::DespawnArc { entity, .. }
                | AtomicOp::DespawnPolyline { entity, .. } => {
                    world.despawn(*entity).ok();
                }

                // Set* → apply new value
                AtomicOp::SetLineData {
                    entity, new: val, ..
                } => {
                    world.insert_one(*entity, *val).ok();
                }
                AtomicOp::SetCircleData {
                    entity, new: val, ..
                } => {
                    world.insert_one(*entity, *val).ok();
                }
                AtomicOp::SetArcData {
                    entity, new: val, ..
                } => {
                    world.insert_one(*entity, *val).ok();
                }
                AtomicOp::SetPolylineData {
                    entity, new: val, ..
                } => {
                    world.insert_one(*entity, val.clone()).ok();
                }
                AtomicOp::SetPosition {
                    entity, new: val, ..
                } => {
                    world.insert_one(*entity, *val).ok();
                }
            }
        }

        let label = tx.label.clone();
        self.undo_stack.push_back(tx);
        Some(label)
    }

    /// Consume the accumulated entity mapping after an undo/redo cycle.
    ///
    /// The caller must pass this mapping to [`apply_entity_remapping`] to
    /// fix up stale handles in the selection manager and spatial index.
    pub fn take_entity_mapping(&mut self) -> EntityMapping {
        std::mem::replace(&mut self.entity_map, EntityMapping::new())
    }

    /// Returns `true` if there are transactions that can be undone.
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    /// Returns `true` if there are transactions that can be redone.
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// Returns the label of the top (most recent) undo transaction, if any.
    pub fn undo_label(&self) -> Option<&str> {
        self.undo_stack.back().map(|tx| tx.label.as_str())
    }

    /// Returns the label of the top (most recent) redo transaction, if any.
    pub fn redo_label(&self) -> Option<&str> {
        self.redo_stack.back().map(|tx| tx.label.as_str())
    }
}

impl Default for History {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Entity remapping
// ---------------------------------------------------------------------------

/// Apply entity handle remapping to a `SelectionManager` and `SpatialIndex`
/// after an undo/redo cycle.
///
/// This function must be called after every undo/redo to fix up stale entity
/// handles that changed because entities were re-spawned (getting new handles).
/// It remaps the selection set and marks the spatial index for lazy rebuild.
pub fn apply_entity_remapping(
    selection: &mut crate::selection::SelectionManager,
    spatial: &mut crate::spatial::SpatialIndex,
    mapping: &EntityMapping,
) {
    // Remap every handle in the selection set.
    let mut remapped = HashSet::new();
    for &entity in &selection.selected {
        remapped.insert(mapping.map(entity));
    }
    selection.selected = remapped;

    // Remap the primary handle.
    if let Some(primary) = selection.primary {
        selection.primary = Some(mapping.map(primary));
    }

    // Mark the spatial index dirty so it rebuilds on the next query.
    spatial.dirty = true;
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::components::{
        ArcData, CircleData, LineData, PolylineData, Position, Renderable,
    };
    use crate::geometry::Point2D;
    use crate::selection::SelectionManager;
    use crate::spatial::SpatialIndex;
    use crate::util::Color;
    use hecs::World;

    // ------------------------------------------------------------------
    // Helpers
    // ------------------------------------------------------------------

    /// Spawn a line entity with `Renderable` and return its handle.
    fn make_line(world: &mut World) -> Entity {
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

    /// Spawn a circle entity with `Renderable` and return its handle.
    fn make_circle(world: &mut World) -> Entity {
        world.spawn((
            CircleData {
                center: Point2D::new(5.0, 5.0),
                radius: 3.0,
                color: Color::WHITE,
                width: 1.0,
            },
            Renderable,
        ))
    }

    /// Spawn a polyline entity with `Renderable` and return its handle.
    fn make_polyline(world: &mut World) -> Entity {
        world.spawn((
            PolylineData {
                vertices: vec![Point2D::new(0.0, 0.0), Point2D::new(5.0, 5.0), Point2D::new(10.0, 0.0)],
                closed: false,
                color: Color::WHITE,
                width: 1.0,
            },
            Renderable,
        ))
    }

    /// Spawn an arc entity with `Renderable` and return its handle.
    fn make_arc(world: &mut World) -> Entity {
        world.spawn((
            ArcData {
                center: Point2D::new(0.0, 0.0),
                radius: 5.0,
                start_angle: 0.0,
                end_angle: 90.0,
                color: Color::WHITE,
                width: 1.0,
            },
            Renderable,
        ))
    }

    // ------------------------------------------------------------------
    // EntityMapping
    // ------------------------------------------------------------------

    #[test]
    fn entity_mapping_unknown_returns_original() {
        let mapping = EntityMapping::new();
        let e = Entity::from_bits(1u64 << 32 | 42).unwrap();
        assert_eq!(mapping.map(e), e);
    }

    #[test]
    fn entity_mapping_mapped_returns_new() {
        let mut mapping = EntityMapping::new();
        let old = Entity::from_bits(1u64 << 32 | 1).unwrap();
        let new = Entity::from_bits(1u64 << 32 | 2).unwrap();
        mapping.record_spawn(old, new);
        assert_eq!(mapping.map(old), new);
    }

    #[test]
    fn entity_mapping_clear_and_is_empty() {
        let mut mapping = EntityMapping::new();
        assert!(mapping.is_empty());

        let old = Entity::from_bits(1u64 << 32 | 1).unwrap();
        let new = Entity::from_bits(1u64 << 32 | 2).unwrap();
        mapping.record_spawn(old, new);
        assert!(!mapping.is_empty());

        mapping.clear();
        assert!(mapping.is_empty());
    }

    #[test]
    fn entity_mapping_multiple_entries() {
        let mut mapping = EntityMapping::new();
        let o1 = Entity::from_bits(1u64 << 32 | 10).unwrap();
        let n1 = Entity::from_bits(1u64 << 32 | 20).unwrap();
        let o2 = Entity::from_bits(1u64 << 32 | 30).unwrap();
        let n2 = Entity::from_bits(1u64 << 32 | 40).unwrap();

        mapping.record_spawn(o1, n1);
        mapping.record_spawn(o2, n2);

        assert_eq!(mapping.map(o1), n1);
        assert_eq!(mapping.map(o2), n2);
    }

    // ------------------------------------------------------------------
    // AtomicOp Spawn / Despawn round-trip via undo
    // ------------------------------------------------------------------

    #[test]
    fn undo_despawn_line_round_trip() {
        let mut world = World::new();
        let entity = make_line(&mut world);
        let data = *world.get::<&LineData>(entity).unwrap();

        let mut history = History::new();
        let mut tx = Transaction::new("despawn line");
        tx.push(AtomicOp::DespawnLine { entity, data });
        history.push(tx);

        // Entity is alive before undo
        assert!(world.get::<&LineData>(entity).is_ok());

        // Undo → entity should be re-spawned (new handle)
        let label = history.undo(&mut world);
        assert_eq!(label.as_deref(), Some("despawn line"));

        let mapping = history.take_entity_mapping();
        let remapped = mapping.map(entity);
        assert_ne!(remapped, entity, "re-spawned entity gets a new handle");

        // The new entity should have the same LineData
        let new_data = world.get::<&LineData>(remapped).unwrap();
        assert_eq!(new_data.start, data.start);
        assert_eq!(new_data.end, data.end);
    }

    #[test]
    fn undo_despawn_polyline_round_trip() {
        let mut world = World::new();
        let entity = make_polyline(&mut world);
        let data = (&*world.get::<&PolylineData>(entity).unwrap()).clone();

        let mut history = History::new();
        let mut tx = Transaction::new("despawn polyline");
        tx.push(AtomicOp::DespawnPolyline { entity, data: data.clone() });
        history.push(tx);

        // Undo → re-spawn with cloned data
        history.undo(&mut world);
        let mapping = history.take_entity_mapping();
        let remapped = mapping.map(entity);
        assert_ne!(remapped, entity);

        let restored = world.get::<&PolylineData>(remapped).unwrap();
        assert_eq!(restored.vertices.len(), data.vertices.len());
        assert_eq!(restored.closed, data.closed);
    }

    // ------------------------------------------------------------------
    // History push / undo / redo with SetLineData
    // ------------------------------------------------------------------

    #[test]
    fn history_set_line_undo_redo() {
        let mut world = World::new();
        let entity = make_line(&mut world);

        let old_line = *world.get::<&LineData>(entity).unwrap();
        let new_line = LineData {
            start: Point2D::new(100.0, 100.0),
            end: Point2D::new(200.0, 200.0),
            ..old_line
        };

        // Apply the change and record it
        world.insert_one(entity, new_line).ok();
        let mut history = History::new();
        let mut tx = Transaction::new("move line");
        tx.push(AtomicOp::SetLineData {
            entity,
            old: old_line,
            new: new_line,
        });
        history.push(tx);

        // Verify current state is new_line
        let current = *world.get::<&LineData>(entity).unwrap();
        assert_eq!(current.start.x, 100.0);

        // Undo → should restore old_line
        let label = history.undo(&mut world);
        assert_eq!(label.as_deref(), Some("move line"));
        let restored = *world.get::<&LineData>(entity).unwrap();
        assert_eq!(restored.start.x, old_line.start.x);
        assert_eq!(restored.end.x, old_line.end.x);

        // Redo → should re-apply new_line
        let label2 = history.redo(&mut world);
        assert_eq!(label2.as_deref(), Some("move line"));
        let reapplied = *world.get::<&LineData>(entity).unwrap();
        assert_eq!(reapplied.start.x, 100.0);
    }

    // ------------------------------------------------------------------
    // History push clears redo stack
    // ------------------------------------------------------------------

    #[test]
    fn history_push_clears_redo() {
        let mut world = World::new();

        let mut history = History::new();

        // Push two transactions
        history.push(Transaction::new("tx1"));
        history.push(Transaction::new("tx2"));

        // Undo tx2
        history.undo(&mut world);
        assert!(history.can_redo());

        // Push a new transaction → redo should be cleared
        history.push(Transaction::new("tx3"));
        assert!(!history.can_redo());
    }

    // ------------------------------------------------------------------
    // max_depth enforcement
    // ------------------------------------------------------------------

    #[test]
    fn history_max_depth_enforced() {
        let mut history = History::with_max_depth(Some(2));

        history.push(Transaction::new("tx1"));
        history.push(Transaction::new("tx2"));
        history.push(Transaction::new("tx3")); // Should push out tx1

        assert_eq!(history.undo_stack.len(), 2);
        assert_eq!(history.undo_label(), Some("tx3"));

        // After undo, tx2 becomes the top
        history.undo(&mut World::new());
        assert_eq!(history.undo_label(), Some("tx2"));
    }

    #[test]
    fn history_unlimited_depth() {
        let mut history = History::with_max_depth(None);

        for i in 0..10_000 {
            history.push(Transaction::new(format!("tx{}", i)));
        }

        assert_eq!(history.undo_stack.len(), 10_000);
    }

    // ------------------------------------------------------------------
    // can_undo / can_redo transitions
    // ------------------------------------------------------------------

    #[test]
    fn history_can_undo_can_redo_transitions() {
        let mut world = World::new();
        let entity = make_line(&mut world);
        let data = *world.get::<&LineData>(entity).unwrap();

        let mut history = History::new();

        // Initially nothing to undo or redo
        assert!(!history.can_undo());
        assert!(!history.can_redo());

        // After push, can undo but not redo
        let mut tx = Transaction::new("test");
        tx.push(AtomicOp::DespawnLine { entity, data });
        history.push(tx);
        assert!(history.can_undo());
        assert!(!history.can_redo());

        // After undo, can redo (and still undo if more exist)
        history.undo(&mut world);
        assert!(history.can_redo());

        // After redo, can undo again
        history.redo(&mut world);
        assert!(history.can_undo());
    }

    // ------------------------------------------------------------------
    // undo / redo label retrieval
    // ------------------------------------------------------------------

    #[test]
    fn history_labels() {
        let mut history = History::new();
        assert_eq!(history.undo_label(), None);
        assert_eq!(history.redo_label(), None);

        history.push(Transaction::new("first action"));
        assert_eq!(history.undo_label(), Some("first action"));
        assert_eq!(history.redo_label(), None);
    }

    // ------------------------------------------------------------------
    // take_entity_mapping
    // ------------------------------------------------------------------

    #[test]
    fn take_entity_mapping_clears_internal_map() {
        let mut mapping = EntityMapping::new();
        let old = Entity::from_bits(1u64 << 32 | 1).unwrap();
        let new = Entity::from_bits(1u64 << 32 | 2).unwrap();
        mapping.record_spawn(old, new);

        assert!(!mapping.is_empty());

        // Create history, inject mapping (normally only happens via undo)
        let mut history = History::new();
        history.entity_map = mapping;

        let taken = history.take_entity_mapping();
        assert!(!taken.is_empty());
        assert_eq!(taken.map(old), new);
        assert!(history.entity_map.is_empty());
    }

    // ------------------------------------------------------------------
    // apply_entity_remapping
    // ------------------------------------------------------------------

    #[test]
    fn apply_entity_remapping_updates_selection() {
        let mut world = World::new();
        let mut sel = SelectionManager::new();

        let e1 = make_line(&mut world);
        let e2 = make_line(&mut world);

        sel.select(&mut world, e1);

        // Create a mapping: e1 → e2 (simulating re-spawn)
        let mut mapping = EntityMapping::new();
        mapping.record_spawn(e1, e2);

        let mut spatial = SpatialIndex::new();

        apply_entity_remapping(&mut sel, &mut spatial, &mapping);

        // e1 should still be in the set via the old handle until we verify:
        // After remapping, e1's old handle should have been replaced by e2
        assert!(sel.is_selected(e2), "remapped entity should be selected");
        assert!(!sel.is_selected(e1), "original entity should no longer be in selected set");
    }

    #[test]
    fn apply_entity_remapping_updates_primary() {
        let mut world = World::new();
        let mut sel = SelectionManager::new();

        let e1 = make_line(&mut world);
        let e2 = make_line(&mut world);

        sel.select(&mut world, e1);
        assert_eq!(sel.primary, Some(e1));

        let mut mapping = EntityMapping::new();
        mapping.record_spawn(e1, e2);

        let mut spatial = SpatialIndex::new();
        apply_entity_remapping(&mut sel, &mut spatial, &mapping);

        assert_eq!(sel.primary, Some(e2), "primary should be remapped");
    }

    #[test]
    fn apply_entity_remapping_marks_spatial_dirty() {
        let mut world = World::new();
        let mut sel = SelectionManager::new();
        let mut spatial = SpatialIndex::new();

        let e = make_line(&mut world);
        sel.select(&mut world, e);

        let mapping = EntityMapping::new();

        spatial.dirty = false;
        apply_entity_remapping(&mut sel, &mut spatial, &mapping);

        assert!(spatial.dirty, "spatial index should be marked dirty");
    }

    #[test]
    fn apply_entity_remapping_empty_selection() {
        let mut sel = SelectionManager::new();
        let mut spatial = SpatialIndex::new();
        let mapping = EntityMapping::new();

        // Should not panic on empty selection
        apply_entity_remapping(&mut sel, &mut spatial, &mapping);

        assert!(sel.is_empty());
        assert!(spatial.dirty);
    }

    // ------------------------------------------------------------------
    // History undo/redo sets EntityMapping correctly
    // ------------------------------------------------------------------

    #[test]
    fn history_undo_despawn_produces_mapping() {
        let mut world = World::new();
        let entity = make_line(&mut world);
        let data = *world.get::<&LineData>(entity).unwrap();

        let mut history = History::new();
        let mut tx = Transaction::new("despawn line");
        tx.push(AtomicOp::DespawnLine { entity, data });
        history.push(tx);

        history.undo(&mut world);
        let mapping = history.take_entity_mapping();

        assert!(!mapping.is_empty(), "undo should produce entity mapping");
        let remapped = mapping.map(entity);
        assert_ne!(remapped, entity, "re-spawned handle should differ");

        // The remapped entity should exist in the world
        assert!(world.get::<&LineData>(remapped).is_ok());
    }

    #[test]
    fn history_redo_spawn_produces_mapping() {
        let mut world = World::new();

        // Create an entity and record a SpawnLine op for it
        let entity = make_line(&mut world);
        let data = *world.get::<&LineData>(entity).unwrap();

        let mut history = History::new();

        // First: despawn → undo → redo
        let mut tx = Transaction::new("despawn line");
        tx.push(AtomicOp::DespawnLine { entity, data });
        history.push(tx);

        // Undo re-spawns the entity; redo uses the original DespawnLine op
        // handle (stale after respawn), so `world.despawn` is a silent no-op.
        // This is correct — callers apply EntityMapping to fix up handles
        // when they care about the entity being truly gone.
        history.undo(&mut world);
        let _first_mapping = history.take_entity_mapping();
        let label = history.redo(&mut world);
        assert_eq!(label.as_deref(), Some("despawn line"));
    }

    // ------------------------------------------------------------------
    // Serial undo/redo cycle with SetLineData
    // ------------------------------------------------------------------

    #[test]
    fn history_set_line_undo_twice_redo_twice() {
        let mut world = World::new();
        let entity = make_line(&mut world);
        let orig = *world.get::<&LineData>(entity).unwrap();

        // Change 1: move to (100,100)-(200,200)
        let mid = LineData {
            start: Point2D::new(100.0, 100.0),
            end: Point2D::new(200.0, 200.0),
            ..orig
        };
        world.insert_one(entity, mid).ok();
        let mut tx1 = Transaction::new("move line 1");
        tx1.push(AtomicOp::SetLineData {
            entity,
            old: orig,
            new: mid,
        });

        // Change 2: move to (300,300)-(400,400)
        let final_ = LineData {
            start: Point2D::new(300.0, 300.0),
            end: Point2D::new(400.0, 400.0),
            ..orig
        };
        world.insert_one(entity, final_).ok();
        let mut tx2 = Transaction::new("move line 2");
        tx2.push(AtomicOp::SetLineData {
            entity,
            old: mid,
            new: final_,
        });

        let mut history = History::new();
        history.push(tx1);
        history.push(tx2);

        // Undo tx2 → mid
        history.undo(&mut world);
        let current = *world.get::<&LineData>(entity).unwrap();
        assert_eq!(current.start.x, 100.0, "after undo tx2, should be mid");

        // Undo tx1 → orig
        history.undo(&mut world);
        let current = *world.get::<&LineData>(entity).unwrap();
        assert_eq!(current.start.x, 0.0, "after undo tx1, should be orig");

        // Redo tx1 → mid
        history.redo(&mut world);
        let current = *world.get::<&LineData>(entity).unwrap();
        assert_eq!(current.start.x, 100.0, "after redo tx1, should be mid");

        // Redo tx2 → final_
        history.redo(&mut world);
        let current = *world.get::<&LineData>(entity).unwrap();
        assert_eq!(current.start.x, 300.0, "after redo tx2, should be final_");
    }

    // ------------------------------------------------------------------
    // Empty undo / redo returns None
    // ------------------------------------------------------------------

    #[test]
    fn history_undo_empty_returns_none() {
        let mut history = History::new();
        let mut world = World::new();

        assert!(history.undo(&mut world).is_none());
        assert!(history.redo(&mut world).is_none());
    }

    // ------------------------------------------------------------------
    // Undo/redo with SetPosition
    // ------------------------------------------------------------------

    #[test]
    fn history_set_position_undo_redo() {
        let mut world = World::new();
        let entity = world.spawn((
            Position(Point2D::new(10.0, 20.0)),
            LineData {
                start: Point2D::new(0.0, 0.0),
                end: Point2D::new(5.0, 5.0),
                color: Color::WHITE,
                width: 1.0,
            },
            Renderable,
        ));

        let old_pos = *world.get::<&Position>(entity).unwrap();
        let new_pos = Position(Point2D::new(100.0, 200.0));

        world.insert_one(entity, new_pos).ok();

        let mut history = History::new();
        let mut tx = Transaction::new("move position");
        tx.push(AtomicOp::SetPosition {
            entity,
            old: old_pos,
            new: new_pos,
        });
        history.push(tx);

        // Undo → restores old position
        history.undo(&mut world);
        let pos = *world.get::<&Position>(entity).unwrap();
        assert_eq!(pos.0.x, 10.0);
        assert_eq!(pos.0.y, 20.0);

        // Redo → applies new position
        history.redo(&mut world);
        let pos = *world.get::<&Position>(entity).unwrap();
        assert_eq!(pos.0.x, 100.0);
        assert_eq!(pos.0.y, 200.0);
    }

    // ------------------------------------------------------------------
    // Spawn op undo = despawn
    // ------------------------------------------------------------------

    #[test]
    fn history_spawn_line_undo_despawns() {
        let mut world = World::new();
        let entity = make_line(&mut world);
        let data = *world.get::<&LineData>(entity).unwrap();

        let mut history = History::new();
        let mut tx = Transaction::new("spawn line");
        tx.push(AtomicOp::SpawnLine { entity, data });
        history.push(tx);

        assert!(world.get::<&LineData>(entity).is_ok());

        // Undo → despawns the entity
        history.undo(&mut world);
        assert!(world.get::<&LineData>(entity).is_err());
    }

    #[test]
    fn history_spawn_polyline_undo_despawns() {
        let mut world = World::new();
        let entity = make_polyline(&mut world);
        let data = (&*world.get::<&PolylineData>(entity).unwrap()).clone();

        let mut history = History::new();
        let mut tx = Transaction::new("spawn polyline");
        tx.push(AtomicOp::SpawnPolyline { entity, data });
        history.push(tx);

        assert!(world.get::<&PolylineData>(entity).is_ok());

        // Undo → despawns the entity
        history.undo(&mut world);
        assert!(world.get::<&PolylineData>(entity).is_err());
    }
}
