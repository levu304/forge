//! Layer system — core types, table storage, ECS integration, and manager.
//!
//! # Overview
//!
//! | Module / Type     | Role                                          |
//! |-------------------|-----------------------------------------------|
//! | [`layer`]         | [`Layer`], [`LayerId`], [`Linetype`]          |
//! | [`table`]         | [`LayerTable`] — CRUD by ID and name          |
//! | [`error`]         | [`LayerError`] — fallible operation errors    |
//! | [`LayerRef`]      | ECS component linking entities to a layer     |
//! | [`ActiveLayer`]   | ECS resource holding the current layer ID     |
//! | [`LayerManager`]  | Facade over [`LayerTable`] + ECS delete logic |

pub mod error;
pub mod layer;
pub mod table;

use hecs::World;

pub use error::LayerError;
pub use layer::{Layer, LayerId, Linetype};
pub use table::LayerTable;

/// ECS component that associates an entity with a layer.
///
/// Attach this to any drawable entity to assign it to the given layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayerRef(pub LayerId);

/// ECS resource component storing the currently active layer.
///
/// Insert this into the world on the resource entity during setup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActiveLayer(pub LayerId);

impl Default for ActiveLayer {
    fn default() -> Self {
        Self(LayerId::DEFAULT)
    }
}

/// Count the number of entities in the world whose [`LayerRef`] matches `id`.
pub fn count_layer_refs(world: &World, id: LayerId) -> usize {
    world
        .query::<&LayerRef>()
        .iter()
        .filter(|(_, r)| r.0 == id)
        .count()
}

/// High-level facade over [`LayerTable`] with ECS-aware layer management.
///
/// `LayerManager` wraps a `LayerTable` and provides helpers that coordinate
/// with the ECS world: counting [`LayerRef`] references before deletion, and
/// updating the [`ActiveLayer`] resource.
#[derive(Debug, Clone)]
pub struct LayerManager {
    table: LayerTable,
}

impl LayerManager {
    /// Create a new manager wrapping the given table.
    pub fn new(table: LayerTable) -> Self {
        Self { table }
    }

    /// Immutably borrow the inner [`LayerTable`].
    pub fn table(&self) -> &LayerTable {
        &self.table
    }

    /// Mutably borrow the inner [`LayerTable`].
    pub fn table_mut(&mut self) -> &mut LayerTable {
        &mut self.table
    }

    /// Delete a layer after counting entities that reference it in `world`.
    ///
    /// Queries `world` for every entity holding a [`LayerRef`] pointing to
    /// `id`, passes the count to [`LayerTable::delete`], and updates the
    /// `ActiveLayer` component on `resource_entity` if it pointed to the
    /// deleted layer.
    ///
    /// The caller must provide the entity that hosts the [`ActiveLayer`]
    /// resource component (typically the resource entity spawned during
    /// application setup).
    pub fn delete_layer(
        &mut self,
        world: &mut World,
        resource_entity: hecs::Entity,
        id: LayerId,
    ) -> Result<(), LayerError> {
        // Capture whether the deleted layer is the active one *before*
        // calling table::delete, which resets the table's own cursor.
        let was_active = world
            .get::<&ActiveLayer>(resource_entity)
            .map(|active| active.0 == id)
            .unwrap_or(false);

        let count = count_layer_refs(world, id);
        self.table.delete(id, count)?;

        // If the deleted layer was active in the world, reset the resource.
        if was_active {
            world
                .insert_one(resource_entity, ActiveLayer(LayerId::DEFAULT))
                .map_err(|_| LayerError::NotFound { id: id.0 })?;
        }

        Ok(())
    }

    /// Set the active layer, updating both the inner table and the
    /// [`ActiveLayer`] component on the given resource entity.
    ///
    /// The caller must provide the entity that hosts the [`ActiveLayer`]
    /// resource.
    pub fn set_active_layer(
        &mut self,
        world: &mut World,
        resource_entity: hecs::Entity,
        id: LayerId,
    ) -> Result<(), LayerError> {
        self.table.set_active(id)?;
        world
            .insert_one(resource_entity, ActiveLayer(id))
            .map_err(|_| LayerError::NotFound { id: id.0 })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: create a world with a resource entity hosting ActiveLayer.
    fn make_world() -> (World, hecs::Entity, LayerManager) {
        let mut world = World::new();
        let res = world.spawn((ActiveLayer::default(),));
        let table = LayerTable::new();
        let mgr = LayerManager::new(table);
        (world, res, mgr)
    }

    #[test]
    fn layer_manager_new_and_accessors() {
        let table = LayerTable::new();
        let mgr = LayerManager::new(table);
        assert_eq!(mgr.table().len(), 1);
        assert!(mgr.table().contains(LayerId::DEFAULT));
    }

    #[test]
    fn layer_manager_table_mut() {
        let table = LayerTable::new();
        let mut mgr = LayerManager::new(table);
        let id = mgr.table_mut().insert("walls").unwrap();
        assert_eq!(id.0, 1);
    }

    #[test]
    fn delete_layer_counts_references() {
        let (mut world, res, mut mgr) = make_world();

        // Insert a non-default layer.
        let id = mgr.table_mut().insert("walls").unwrap();
        world
            .insert_one(res, ActiveLayer(id))
            .expect("active layer set");

        // Spawn entities referencing this layer.
        let e1 = world.spawn((LayerRef(id),));
        let e2 = world.spawn((LayerRef(id),));
        let e3 = world.spawn((LayerRef(id),));
        let _other = world.spawn((LayerRef(LayerId::DEFAULT),));

        // Should fail because 3 entities reference it.
        let err = mgr
            .delete_layer(&mut world, res, id)
            .expect_err("expected HasReferences");
        assert_eq!(err, LayerError::HasReferences { count: 3 });

        // Despawn all entities referencing the target layer.
        world.despawn(e1).unwrap();
        world.despawn(e2).unwrap();
        world.despawn(e3).unwrap();

        // Should succeed now with 0 references.
        mgr.delete_layer(&mut world, res, id).unwrap();
    }

    #[test]
    fn set_active_layer_updates_resource() {
        let (mut world, res, mut mgr) = make_world();

        // Insert a non-default layer.
        let id = mgr.table_mut().insert("walls").unwrap();

        mgr.set_active_layer(&mut world, res, id).unwrap();

        let active = world.get::<&ActiveLayer>(res).unwrap();
        assert_eq!(active.0, id);

        // Table also reflects the change.
        assert_eq!(mgr.table().active(), id);
    }

    #[test]
    fn set_active_layer_invalid_id() {
        let (mut world, res, mut mgr) = make_world();
        let err = mgr
            .set_active_layer(&mut world, res, LayerId(999))
            .unwrap_err();
        assert_eq!(err, LayerError::NotFound { id: 999 });
    }

    #[test]
    fn count_layer_refs_works() {
        let mut world = World::new();
        world.spawn(());
        let id = LayerId(5);
        world.spawn((LayerRef(id),));
        world.spawn((LayerRef(id),));
        world.spawn((LayerRef(LayerId::DEFAULT),));
        assert_eq!(count_layer_refs(&world, id), 2);
        assert_eq!(count_layer_refs(&world, LayerId::DEFAULT), 1);
        assert_eq!(count_layer_refs(&world, LayerId(99)), 0);
    }
}
