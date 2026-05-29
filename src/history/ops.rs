//! Atomic undo/redo operation types.
//!
//! Each [`AtomicOp`] variant captures enough data to reverse a single
//! entity-level change. Operations are collected into [`Transaction`]s
//! (see [`super::transaction`]) and replayed by [`super::History`].
//!
//! # Variants
//!
//! | Group | Variants | Undo | Redo |
//! |-------|----------|------|------|
//! | Spawn | `Spawn{Line,Circle,Arc,Polyline}` | Despawns entity | Re-spawns entity |
//! | Despawn | `Despawn{Line,Circle,Arc,Polyline}` | Re-spawns entity | Despawns entity |
//! | Set | `Set{Line,Circle,Arc,Polyline,Position}` | Restores `old` | Applies `new` |
//! | Property | `Set{LayerRef,PropertySource}` | Restores `old` | Applies `new` |
//!
//! [`Transaction`]: super::transaction::Transaction

use crate::ecs::components::{
    ArcData, CircleData, LayerRef, LineData, PolylineData, Position, PropertySource,
};

/// Typed atomic undo/redo operation.
///
/// Each variant stores exactly the data needed to reverse itself:
/// - **Spawn*** stores the entity handle and initial data.
/// - **Despawn*** stores the entity handle and the data it held at despawn time.
/// - **Set*** stores both the `old` (pre-change) and `new` (post-change) values.
#[derive(Debug, Clone)]
pub enum AtomicOp {
    /// A line entity was spawned.
    SpawnLine {
        entity: hecs::Entity,
        data: LineData,
    },
    /// A circle entity was spawned.
    SpawnCircle {
        entity: hecs::Entity,
        data: CircleData,
    },
    /// An arc entity was spawned.
    SpawnArc {
        entity: hecs::Entity,
        data: ArcData,
    },
    /// A polyline entity was spawned.
    SpawnPolyline {
        entity: hecs::Entity,
        data: PolylineData,
    },
    /// A line entity was despawned (data captured before removal).
    DespawnLine {
        entity: hecs::Entity,
        data: LineData,
    },
    /// A circle entity was despawned.
    DespawnCircle {
        entity: hecs::Entity,
        data: CircleData,
    },
    /// An arc entity was despawned.
    DespawnArc {
        entity: hecs::Entity,
        data: ArcData,
    },
    /// A polyline entity was despawned.
    DespawnPolyline {
        entity: hecs::Entity,
        data: PolylineData,
    },
    /// A line entity's data was modified.
    SetLineData {
        entity: hecs::Entity,
        old: LineData,
        new: LineData,
    },
    /// A circle entity's data was modified.
    SetCircleData {
        entity: hecs::Entity,
        old: CircleData,
        new: CircleData,
    },
    /// An arc entity's data was modified.
    SetArcData {
        entity: hecs::Entity,
        old: ArcData,
        new: ArcData,
    },
    /// A polyline entity's data was modified.
    SetPolylineData {
        entity: hecs::Entity,
        old: PolylineData,
        new: PolylineData,
    },
    /// An entity's world-space position was modified.
    SetPosition {
        entity: hecs::Entity,
        old: Position,
        new: Position,
    },

    // ------------------------------------------------------------------
    // Property system
    // ------------------------------------------------------------------

    /// An entity's layer reference was changed.
    ///
    /// `old` / `new` are `None` when the entity did not / should not have
    /// the [`LayerRef`] component at all.
    SetLayerRef {
        entity: hecs::Entity,
        old: Option<LayerRef>,
        new: Option<LayerRef>,
    },

    /// An entity's property source was changed.
    SetPropertySource {
        entity: hecs::Entity,
        old: PropertySource,
        new: PropertySource,
    },
}
