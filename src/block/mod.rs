//! Block definitions and instances.
//!
//! This module provides the block subsystem:
//!
//! * **Definition storage** — [`BlockTable`] holds named block definitions
//!   ([`BlockDef`]), each containing a list of entities ([`BlockEntity`]).
//! * **Instance component** — [`BlockRef`] is an ECS component that marks an
//!   entity as an instance of a block definition.
//! * **Manager** — [`BlockManager`] is a thin owning wrapper around
//!   [`BlockTable`] with delegated CRUD methods.

pub mod definition;
pub mod insert;

// ---------------------------------------------------------------------------
// Re-exports
// ---------------------------------------------------------------------------

pub use definition::{BlockDef, BlockEntity, BlockError, BlockId, BlockTable};
pub use insert::{BlockInsert, BlockRef};

/// Thin owning wrapper around [`BlockTable`].
///
/// Provides direct access to the underlying table via the `table` field and
/// delegates common CRUD operations.  This wrapper exists so that the block
/// subsystem can be swapped or extended without changing the public API
/// surface.
#[derive(Debug, Clone)]
pub struct BlockManager {
    /// The underlying block definition table.
    pub table: BlockTable,
}

impl BlockManager {
    /// Create a new `BlockManager` with an empty block table.
    pub fn new() -> Self {
        Self {
            table: BlockTable::new(),
        }
    }

    // ------------------------------------------------------------------
    // Delegated methods
    // ------------------------------------------------------------------

    /// Insert a block definition.  See [`BlockTable::insert`].
    pub fn insert(
        &mut self,
        def: BlockDef,
    ) -> Result<BlockId, BlockError> {
        self.table.insert(def)
    }

    /// Look up a block definition by ID.  See [`BlockTable::get`].
    pub fn get(&self, id: BlockId) -> Option<&BlockDef> {
        self.table.get(id)
    }

    /// Mutably borrow a block definition by ID.  See [`BlockTable::get_mut`].
    pub fn get_mut(&mut self, id: BlockId) -> Option<&mut BlockDef> {
        self.table.get_mut(id)
    }

    /// Remove a block definition by ID.  See [`BlockTable::delete`].
    pub fn delete(&mut self, id: BlockId) -> Result<(), BlockError> {
        self.table.delete(id)
    }

    /// Rename a block definition.  See [`BlockTable::rename`].
    pub fn rename(&mut self, id: BlockId, new_name: String) -> Result<(), BlockError> {
        self.table.rename(id, new_name)
    }

    /// Look up a block definition by name.  See [`BlockTable::get_by_name`].
    pub fn get_by_name(&self, name: &str) -> Option<&BlockDef> {
        self.table.get_by_name(name)
    }

    /// Check whether a block with the given name exists.
    pub fn contains_name(&self, name: &str) -> bool {
        self.table.contains_name(name)
    }

    /// Number of block definitions.
    pub fn len(&self) -> usize {
        self.table.len()
    }

    /// Returns `true` if the manager contains no block definitions.
    pub fn is_empty(&self) -> bool {
        self.table.is_empty()
    }
}

impl Default for BlockManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_def(name: &str) -> BlockDef {
        BlockDef {
            name: name.to_string(),
            base_point: crate::geometry::Point2D::new(0.0, 0.0),
            entities: vec![],
            bounds: crate::geometry::BoundingBox2D::empty(),
        }
    }

    #[test]
    fn test_block_manager_new_is_empty() {
        let mgr = BlockManager::new();
        assert!(mgr.is_empty());
        assert_eq!(mgr.len(), 0);
    }

    #[test]
    fn test_block_manager_insert_and_get() {
        let mut mgr = BlockManager::new();
        let id = mgr.insert(make_def("test")).unwrap();
        assert_eq!(mgr.len(), 1);

        let def = mgr.get(id);
        assert!(def.is_some());
        assert_eq!(def.unwrap().name, "test");
    }

    #[test]
    fn test_block_manager_delete() {
        let mut mgr = BlockManager::new();
        let id = mgr.insert(make_def("to-delete")).unwrap();
        assert_eq!(mgr.len(), 1);

        mgr.delete(id).unwrap();
        assert_eq!(mgr.len(), 0);
        assert!(mgr.get(id).is_none());
    }

    #[test]
    fn test_block_manager_get_by_name() {
        let mut mgr = BlockManager::new();
        mgr.insert(make_def("alpha")).unwrap();
        mgr.insert(make_def("beta")).unwrap();

        assert!(mgr.get_by_name("alpha").is_some());
        assert!(mgr.get_by_name("beta").is_some());
        assert!(mgr.get_by_name("gamma").is_none());
    }

    #[test]
    fn test_block_manager_default_is_empty() {
        let mgr = BlockManager::default();
        assert!(mgr.is_empty());
    }
}
