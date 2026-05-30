//! Block definition types: BlockId, BlockEntity, BlockDef, BlockTable.
//!
//! A block definition is a named collection of entities (blueprint) stored
//! in a `BlockTable`.  Instances in the ECS world reference a definition
//! via `BlockRef` (see [`super::insert::BlockRef`]).
//!
//! Block definitions store entities in a `Vec<BlockEntity>` — not in a
//! separate `hecs::World` — to avoid nested ECS complexity.

use std::collections::HashMap;

use crate::ecs::components::{LayerRef, PropertySource};
use crate::geometry::{BoundingBox2D, Point2D};

// ---------------------------------------------------------------------------
// BlockId
// ---------------------------------------------------------------------------

/// Unique identifier for a block definition.
///
/// IDs are assigned sequentially by [`BlockTable::insert`] starting from 0.
/// They are stable for the lifetime of the block definition within the table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockId(pub u32);

// ---------------------------------------------------------------------------
// BlockEntity
// ---------------------------------------------------------------------------

/// A single entity stored inside a block definition.
///
/// This mirrors the ECS geometry primitives (Line, Circle, Arc, Polyline)
/// but stores data inline rather than as separate components.  Each variant
/// carries a [`PropertySource`] and [`LayerRef`] per-entity — they are
/// **not** inherited from the block definition.
///
/// # Splines
///
/// Spline curves are converted to a `Polyline` with 64 segments before
/// being stored in a block definition.  There is no `Spline` variant here.
#[derive(Debug, Clone)]
pub enum BlockEntity {
    /// A line segment from `start` to `end`.
    Line(Point2D, Point2D, PropertySource, LayerRef),
    /// A circle defined by `center` and `radius`.
    Circle(Point2D, f64, PropertySource, LayerRef),
    /// An arc defined by `center`, `radius`, `start_angle`, `end_angle`.
    ///
    /// Angles are in degrees, 0° = +X axis, increasing counter-clockwise.
    /// Uses a conservative full-circle bounding box (same as Circle).
    Arc(Point2D, f64, f64, f64, PropertySource, LayerRef),
    /// A polyline (or closed polygon) defined by an ordered vertex list.
    Polyline(Vec<Point2D>, bool, PropertySource, LayerRef),
}

impl BlockEntity {
    /// Compute the axis-aligned bounding box of this entity.
    ///
    /// # Variant behaviour
    ///
    /// | Variant | Method |
    /// |---------|--------|
    /// | `Line` | `BoundingBox2D::from_points(&[start, end])` |
    /// | `Circle` | Conservative: `center ± radius` |
    /// | `Arc` | Conservative full-circle bound (same as Circle) |
    /// | `Polyline` | `BoundingBox2D::from_points(vertices)` |
    pub fn bounding_box(&self) -> BoundingBox2D {
        match self {
            BlockEntity::Line(start, end, _, _) => {
                BoundingBox2D::from_points(&[*start, *end])
            }
            BlockEntity::Circle(center, radius, _, _) => BoundingBox2D {
                min: Point2D::new(center.x - radius, center.y - radius),
                max: Point2D::new(center.x + radius, center.y + radius),
            },
            BlockEntity::Arc(center, radius, _, _, _, _) => BoundingBox2D {
                min: Point2D::new(center.x - radius, center.y - radius),
                max: Point2D::new(center.x + radius, center.y + radius),
            },
            BlockEntity::Polyline(vertices, _, _, _) => {
                BoundingBox2D::from_points(vertices)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// BlockDef
// ---------------------------------------------------------------------------

/// A block definition (blueprint).
///
/// Stored in [`BlockTable`], not in the main ECS `World`.  Each definition
/// holds a list of entities, a base point, and a pre-computed bounding box.
#[derive(Debug, Clone)]
pub struct BlockDef {
    /// Human-readable block name (must be unique within the table).
    pub name: String,
    /// Base / insertion point in local coordinates.
    pub base_point: Point2D,
    /// Local entity storage for the block's geometry.
    ///
    /// Each entry carries per-entity style information
    /// ([`PropertySource`], [`LayerRef`]).
    pub entities: Vec<BlockEntity>,
    /// Pre-computed axis-aligned bounding box covering all entities.
    ///
    /// Is `BoundingBox2D::empty()` when the entity list is empty.
    pub bounds: BoundingBox2D,
}

impl BlockDef {
    /// Compute the union of all entity bounding boxes.
    ///
    /// Returns `BoundingBox2D::empty()` when the entity list is empty.
    ///
    /// This is called internally by [`BlockTable::insert`] and does not
    /// need to be invoked manually under normal use.
    pub fn compute_bounds(&self) -> BoundingBox2D {
        self.entities
            .iter()
            .fold(BoundingBox2D::empty(), |acc, entity| {
                acc.union(&entity.bounding_box())
            })
    }
}

// ---------------------------------------------------------------------------
// BlockError
// ---------------------------------------------------------------------------

/// Errors that can occur during block table operations.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum BlockError {
    /// A block with the given name already exists in the table.
    #[error("Block name already exists")]
    NameExists,
    /// No block with the given ID was found in the table.
    #[error("Block not found")]
    BlockNotFound,
}

// ---------------------------------------------------------------------------
// BlockTable
// ---------------------------------------------------------------------------

/// Block definition table.
///
/// Owned by [`super::BlockManager`].  Provides CRUD operations on block
/// definitions: insert, get, get_mut, delete, rename, and lookup by name.
///
/// IDs are auto-assigned sequentially from 0.
#[derive(Debug, Clone)]
pub struct BlockTable {
    /// Primary storage: block ID → definition.
    definitions: HashMap<BlockId, BlockDef>,
    /// Reverse lookup: name → block ID (enforces name uniqueness).
    name_to_id: HashMap<String, BlockId>,
    /// Next auto-incrementing ID to assign.
    next_id: u32,
}

impl BlockTable {
    /// Create an empty block table.
    pub fn new() -> Self {
        Self {
            definitions: HashMap::new(),
            name_to_id: HashMap::new(),
            next_id: 0,
        }
    }

    /// Insert a block definition, auto-assigning a new [`BlockId`].
    ///
    /// Returns an error if a block with the same name already exists.
    /// The definition's `bounds` field is automatically recomputed from
    /// its entities on insertion.
    pub fn insert(&mut self, mut def: BlockDef) -> Result<BlockId, BlockError> {
        if self.name_to_id.contains_key(&def.name) {
            return Err(BlockError::NameExists);
        }
        let id = BlockId(self.next_id);
        self.next_id += 1;

        // Auto-compute bounds for the caller.
        def.bounds = def.compute_bounds();

        self.name_to_id.insert(def.name.clone(), id);
        self.definitions.insert(id, def);
        Ok(id)
    }

    /// Look up a block definition by ID.
    pub fn get(&self, id: BlockId) -> Option<&BlockDef> {
        self.definitions.get(&id)
    }

    /// Look up a block definition by name.
    pub fn get_by_name(&self, name: &str) -> Option<&BlockDef> {
        self.name_to_id
            .get(name)
            .and_then(|id| self.definitions.get(id))
    }

    /// Mutably borrow a block definition by ID.
    pub fn get_mut(&mut self, id: BlockId) -> Option<&mut BlockDef> {
        self.definitions.get_mut(&id)
    }

    /// Rename a block definition.
    ///
    /// Returns `Ok(())` if the rename succeeds.  Returns
    /// [`BlockError::BlockNotFound`] if `id` does not exist.
    /// Returns [`BlockError::NameExists`] if `new_name` is already
    /// taken by a *different* block.  Renaming to the current name
    /// is a no-op and returns `Ok`.
    pub fn rename(&mut self, id: BlockId, new_name: String) -> Result<(), BlockError> {
        // Scope the shared borrow so we can mutably borrow later.
        let old_name = {
            let def = self.definitions.get(&id).ok_or(BlockError::BlockNotFound)?;

            // No-op if the name hasn't changed.
            if def.name == new_name {
                return Ok(());
            }

            // Reject if the new name belongs to another block.
            if self.name_to_id.contains_key(&new_name) {
                return Err(BlockError::NameExists);
            }

            def.name.clone()
        };

        let def = self.definitions.get_mut(&id).unwrap();
        def.name = new_name.clone();

        self.name_to_id.remove(&old_name);
        self.name_to_id.insert(new_name, id);
        Ok(())
    }

    /// Number of block definitions in the table.
    pub fn len(&self) -> usize {
        self.definitions.len()
    }

    /// Returns `true` if the table contains no definitions.
    pub fn is_empty(&self) -> bool {
        self.definitions.is_empty()
    }

    /// Check whether a block with the given name exists.
    pub fn contains_name(&self, name: &str) -> bool {
        self.name_to_id.contains_key(name)
    }

    /// Remove a block definition by ID.
    ///
    /// Returns [`BlockError::BlockNotFound`] if the ID does not exist.
    pub fn delete(&mut self, id: BlockId) -> Result<(), BlockError> {
        let def = self.definitions.remove(&id).ok_or(BlockError::BlockNotFound)?;
        self.name_to_id.remove(&def.name);
        Ok(())
    }
}

impl Default for BlockTable {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Point2D;

    // ------------------------------------------------------------------
    // BlockId
    // ------------------------------------------------------------------

    #[test]
    fn test_block_id_creation_and_equality() {
        let a = BlockId(0);
        let b = BlockId(0);
        let c = BlockId(1);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn test_block_id_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(BlockId(0));
        set.insert(BlockId(1));
        set.insert(BlockId(0)); // duplicate
        assert_eq!(set.len(), 2);
    }

    // ------------------------------------------------------------------
    // BlockEntity::bounding_box
    // ------------------------------------------------------------------

    #[test]
    fn test_block_entity_line_bounding_box() {
        let start = Point2D::new(1.0, 2.0);
        let end = Point2D::new(5.0, 6.0);
        let entity = BlockEntity::Line(start, end, PropertySource::ByLayer, LayerRef(0));
        let bbox = entity.bounding_box();
        assert_eq!(bbox.min.x, 1.0);
        assert_eq!(bbox.min.y, 2.0);
        assert_eq!(bbox.max.x, 5.0);
        assert_eq!(bbox.max.y, 6.0);
    }

    #[test]
    fn test_block_entity_line_reversed() {
        let start = Point2D::new(5.0, 6.0);
        let end = Point2D::new(1.0, 2.0);
        let entity = BlockEntity::Line(start, end, PropertySource::ByLayer, LayerRef(0));
        let bbox = entity.bounding_box();
        assert_eq!(bbox.min.x, 1.0);
        assert_eq!(bbox.min.y, 2.0);
        assert_eq!(bbox.max.x, 5.0);
        assert_eq!(bbox.max.y, 6.0);
    }

    #[test]
    fn test_block_entity_circle_bounding_box() {
        let center = Point2D::new(0.0, 0.0);
        let entity = BlockEntity::Circle(center, 5.0, PropertySource::ByLayer, LayerRef(0));
        let bbox = entity.bounding_box();
        assert_eq!(bbox.min.x, -5.0);
        assert_eq!(bbox.min.y, -5.0);
        assert_eq!(bbox.max.x, 5.0);
        assert_eq!(bbox.max.y, 5.0);
    }

    #[test]
    fn test_block_entity_arc_bounding_box() {
        // Arc uses the same conservative full-circle bound as Circle.
        let center = Point2D::new(10.0, 20.0);
        let entity = BlockEntity::Arc(
            center,
            3.0,
            0.0,
            90.0,
            PropertySource::ByLayer,
            LayerRef(0),
        );
        let bbox = entity.bounding_box();
        assert_eq!(bbox.min.x, 7.0);
        assert_eq!(bbox.min.y, 17.0);
        assert_eq!(bbox.max.x, 13.0);
        assert_eq!(bbox.max.y, 23.0);
    }

    #[test]
    fn test_block_entity_polyline_bounding_box() {
        let vertices = vec![
            Point2D::new(0.0, 0.0),
            Point2D::new(10.0, 0.0),
            Point2D::new(10.0, 5.0),
            Point2D::new(0.0, 5.0),
        ];
        let entity = BlockEntity::Polyline(vertices, true, PropertySource::ByLayer, LayerRef(0));
        let bbox = entity.bounding_box();
        assert_eq!(bbox.min.x, 0.0);
        assert_eq!(bbox.min.y, 0.0);
        assert_eq!(bbox.max.x, 10.0);
        assert_eq!(bbox.max.y, 5.0);
    }

    #[test]
    fn test_block_entity_polyline_single_vertex() {
        let vertices = vec![Point2D::new(7.0, 8.0)];
        let entity = BlockEntity::Polyline(vertices, false, PropertySource::ByLayer, LayerRef(0));
        let bbox = entity.bounding_box();
        assert_eq!(bbox.min.x, 7.0);
        assert_eq!(bbox.min.y, 8.0);
        assert_eq!(bbox.max.x, 7.0);
        assert_eq!(bbox.max.y, 8.0);
    }

    #[test]
    fn test_block_entity_polyline_empty_vertices() {
        // Empty vertex list should produce an empty bounding box.
        let entity = BlockEntity::Polyline(vec![], false, PropertySource::ByLayer, LayerRef(0));
        let bbox = entity.bounding_box();
        assert!(bbox.is_empty());
    }

    // ------------------------------------------------------------------
    // BlockDef
    // ------------------------------------------------------------------

    #[test]
    fn test_block_def_compute_bounds_multiple_entities() {
        let def = BlockDef {
            name: "test".to_string(),
            base_point: Point2D::new(0.0, 0.0),
            entities: vec![
                BlockEntity::Line(
                    Point2D::new(0.0, 0.0),
                    Point2D::new(10.0, 5.0),
                    PropertySource::ByLayer,
                    LayerRef(0),
                ),
                BlockEntity::Circle(
                    Point2D::new(5.0, 5.0),
                    3.0,
                    PropertySource::ByLayer,
                    LayerRef(0),
                ),
            ],
            bounds: BoundingBox2D::empty(),
        };
        let bbox = def.compute_bounds();
        // Line spans x:[0,10] y:[0,5]; Circle spans x:[2,8] y:[2,8]
        // Union: x:[0,10], y:[0,8]
        assert_eq!(bbox.min.x, 0.0);
        assert_eq!(bbox.min.y, 0.0);
        assert_eq!(bbox.max.x, 10.0);
        assert_eq!(bbox.max.y, 8.0);
    }

    #[test]
    fn test_block_def_compute_bounds_empty_entities() {
        let def = BlockDef {
            name: "empty".to_string(),
            base_point: Point2D::new(0.0, 0.0),
            entities: vec![],
            bounds: BoundingBox2D::empty(),
        };
        let bbox = def.compute_bounds();
        assert!(bbox.is_empty());
    }

    #[test]
    fn test_block_def_compute_bounds_single_entity() {
        let def = BlockDef {
            name: "single".to_string(),
            base_point: Point2D::new(0.0, 0.0),
            entities: vec![BlockEntity::Circle(
                Point2D::new(0.0, 0.0),
                5.0,
                PropertySource::ByLayer,
                LayerRef(0),
            )],
            bounds: BoundingBox2D::empty(),
        };
        let bbox = def.compute_bounds();
        assert_eq!(bbox.min.x, -5.0);
        assert_eq!(bbox.max.x, 5.0);
    }

    // ------------------------------------------------------------------
    // BlockTable
    // ------------------------------------------------------------------

    #[test]
    fn test_block_table_new_is_empty() {
        let table = BlockTable::new();
        assert_eq!(table.len(), 0);
        assert!(table.is_empty());
    }

    #[test]
    fn test_block_table_insert_sequential_ids() {
        let mut table = BlockTable::new();

        let id0 = table.insert(BlockDef {
            name: "A".to_string(),
            base_point: Point2D::new(0.0, 0.0),
            entities: vec![],
            bounds: BoundingBox2D::empty(),
        });
        assert!(id0.is_ok());
        assert_eq!(id0.unwrap(), BlockId(0));

        let id1 = table.insert(BlockDef {
            name: "B".to_string(),
            base_point: Point2D::new(0.0, 0.0),
            entities: vec![],
            bounds: BoundingBox2D::empty(),
        });
        assert!(id1.is_ok());
        assert_eq!(id1.unwrap(), BlockId(1));

        assert_eq!(table.len(), 2);
    }

    #[test]
    fn test_block_table_insert_duplicate_name() {
        let mut table = BlockTable::new();

        assert!(table
            .insert(BlockDef {
                name: "dup".to_string(),
                base_point: Point2D::new(0.0, 0.0),
                entities: vec![],
                bounds: BoundingBox2D::empty(),
            })
            .is_ok());

        let result = table.insert(BlockDef {
            name: "dup".to_string(),
            base_point: Point2D::new(0.0, 0.0),
            entities: vec![],
            bounds: BoundingBox2D::empty(),
        });
        assert_eq!(result, Err(BlockError::NameExists));
        assert_eq!(table.len(), 1);
    }

    #[test]
    fn test_block_table_insert_auto_computes_bounds() {
        let mut table = BlockTable::new();

        let id = table
            .insert(BlockDef {
                name: "auto-bounds".to_string(),
                base_point: Point2D::new(0.0, 0.0),
                entities: vec![BlockEntity::Line(
                    Point2D::new(2.0, 3.0),
                    Point2D::new(8.0, 7.0),
                    PropertySource::ByLayer,
                    LayerRef(0),
                )],
                bounds: BoundingBox2D::empty(),
            })
            .unwrap();

        let def = table.get(id).unwrap();
        assert_eq!(def.bounds.min.x, 2.0);
        assert_eq!(def.bounds.max.x, 8.0);
        assert_eq!(def.bounds.min.y, 3.0);
        assert_eq!(def.bounds.max.y, 7.0);
    }

    #[test]
    fn test_block_table_get_existing() {
        let mut table = BlockTable::new();
        let id = table
            .insert(BlockDef {
                name: "get-me".to_string(),
                base_point: Point2D::new(0.0, 0.0),
                entities: vec![],
                bounds: BoundingBox2D::empty(),
            })
            .unwrap();

        let def = table.get(id);
        assert!(def.is_some());
        assert_eq!(def.unwrap().name, "get-me");
    }

    #[test]
    fn test_block_table_get_missing() {
        let table = BlockTable::new();
        assert!(table.get(BlockId(42)).is_none());
    }

    #[test]
    fn test_block_table_get_by_name() {
        let mut table = BlockTable::new();
        table
            .insert(BlockDef {
                name: "find-me".to_string(),
                base_point: Point2D::new(0.0, 0.0),
                entities: vec![],
                bounds: BoundingBox2D::empty(),
            })
            .unwrap();

        let def = table.get_by_name("find-me");
        assert!(def.is_some());
        assert_eq!(def.unwrap().name, "find-me");
    }

    #[test]
    fn test_block_table_get_by_name_missing() {
        let table = BlockTable::new();
        assert!(table.get_by_name("nope").is_none());
    }

    #[test]
    fn test_block_table_get_mut_allows_mutation() {
        let mut table = BlockTable::new();
        let id = table
            .insert(BlockDef {
                name: "mutable".to_string(),
                base_point: Point2D::new(0.0, 0.0),
                entities: vec![],
                bounds: BoundingBox2D::empty(),
            })
            .unwrap();

        let def = table.get_mut(id).unwrap();
        def.base_point = Point2D::new(100.0, 200.0);

        let def = table.get(id).unwrap();
        assert_eq!(def.base_point.x, 100.0);
        assert_eq!(def.base_point.y, 200.0);
    }

    #[test]
    fn test_block_table_rename_valid() {
        let mut table = BlockTable::new();
        let id = table
            .insert(BlockDef {
                name: "old-name".to_string(),
                base_point: Point2D::new(0.0, 0.0),
                entities: vec![],
                bounds: BoundingBox2D::empty(),
            })
            .unwrap();

        assert!(table.rename(id, "new-name".to_string()).is_ok());
        assert_eq!(table.get(id).unwrap().name, "new-name");
        assert!(table.get_by_name("new-name").is_some());
        assert!(table.get_by_name("old-name").is_none());
    }

    #[test]
    fn test_block_table_rename_duplicate() {
        let mut table = BlockTable::new();
        table
            .insert(BlockDef {
                name: "first".to_string(),
                base_point: Point2D::new(0.0, 0.0),
                entities: vec![],
                bounds: BoundingBox2D::empty(),
            })
            .unwrap();
        let id2 = table
            .insert(BlockDef {
                name: "second".to_string(),
                base_point: Point2D::new(0.0, 0.0),
                entities: vec![],
                bounds: BoundingBox2D::empty(),
            })
            .unwrap();

        assert_eq!(
            table.rename(id2, "first".to_string()),
            Err(BlockError::NameExists)
        );
    }

    #[test]
    fn test_block_table_rename_same_name_is_no_op() {
        let mut table = BlockTable::new();
        let id = table
            .insert(BlockDef {
                name: "unchanged".to_string(),
                base_point: Point2D::new(0.0, 0.0),
                entities: vec![],
                bounds: BoundingBox2D::empty(),
            })
            .unwrap();

        assert!(table.rename(id, "unchanged".to_string()).is_ok());
        assert_eq!(table.len(), 1);
    }

    #[test]
    fn test_block_table_rename_nonexistent() {
        let mut table = BlockTable::new();
        assert_eq!(
            table.rename(BlockId(999), "anything".to_string()),
            Err(BlockError::BlockNotFound)
        );
    }

    #[test]
    fn test_block_table_delete_valid() {
        let mut table = BlockTable::new();
        let id = table
            .insert(BlockDef {
                name: "delete-me".to_string(),
                base_point: Point2D::new(0.0, 0.0),
                entities: vec![],
                bounds: BoundingBox2D::empty(),
            })
            .unwrap();

        assert!(table.delete(id).is_ok());
        assert_eq!(table.len(), 0);
        assert!(table.get(id).is_none());
        assert!(table.get_by_name("delete-me").is_none());
    }

    #[test]
    fn test_block_table_delete_nonexistent() {
        let mut table = BlockTable::new();
        assert_eq!(
            table.delete(BlockId(42)),
            Err(BlockError::BlockNotFound)
        );
    }

    #[test]
    fn test_block_table_contains_name() {
        let mut table = BlockTable::new();
        table
            .insert(BlockDef {
                name: "exists".to_string(),
                base_point: Point2D::new(0.0, 0.0),
                entities: vec![],
                bounds: BoundingBox2D::empty(),
            })
            .unwrap();

        assert!(table.contains_name("exists"));
        assert!(!table.contains_name("missing"));
    }

    #[test]
    fn test_block_table_default_is_empty() {
        let table = BlockTable::default();
        assert!(table.is_empty());
    }
}
