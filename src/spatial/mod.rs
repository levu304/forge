//! Spatial index (rstar R-tree wrapper).
//!
//! Provides nearest-neighbor queries for the snap engine and range queries
//! for window selection. Uses lazy rebuild with a `dirty` flag — callers
//! must call [`SpatialIndex::ensure_clean`] before querying after entity
//! modifications.
//!
//! # Architecture
//!
//! [`SpatialIndex`] wraps `rstar::RTree<SpatialEntry>`.  A companion
//! [`HashMap<Entity, BoundingBox2D>`] tracks bounds for incremental removal
//! (rstar 0.12 does not expose [`SelectionFunction`] implementations publicly,
//! so we reconstruct an entry with matching bounds and use [`RTree::remove`]
//! which requires `PartialEq` + correct envelope).
//!
//! The index supports three query types:
//!
//! | Query | Method | Use Case |
//! |-------|--------|----------|
//! | Nearest neighbour | [`nearest_neighbor`] | Snap engine candidate finding |
//! | Strict containment | [`enclosed_in`] | Enclosing window selection |
//! | Overlap | [`intersecting`] | Crossing window selection |
//!
//! [`nearest_neighbor`]: SpatialIndex::nearest_neighbor
//! [`enclosed_in`]: SpatialIndex::enclosed_in
//! [`intersecting`]: SpatialIndex::intersecting

use std::collections::HashMap;

use rstar::{AABB, Envelope, PointDistance, RTree, RTreeObject};

use crate::ecs::components::{ArcData, CircleData, LineData, PolylineData};
use crate::geometry::BoundingBox2D;
use crate::geometry::Point2D;

// ---------------------------------------------------------------------------
// Spatial entry
// ---------------------------------------------------------------------------

/// An entry in the spatial index, mapping an [`hecs::Entity`] to its
/// [`BoundingBox2D`].
///
/// `PartialEq` compares **by entity only** (by design, so that the
/// R-tree can identify entries to remove without caring about bounds).
#[derive(Debug, Clone)]
pub struct SpatialEntry {
    pub entity: hecs::Entity,
    pub bounds: BoundingBox2D,
}

impl PartialEq for SpatialEntry {
    fn eq(&self, other: &Self) -> bool {
        self.entity == other.entity
    }
}

impl Eq for SpatialEntry {}

impl RTreeObject for SpatialEntry {
    type Envelope = AABB<[f64; 2]>;

    fn envelope(&self) -> Self::Envelope {
        AABB::from_corners(
            [self.bounds.min.x, self.bounds.min.y],
            [self.bounds.max.x, self.bounds.max.y],
        )
    }
}

impl PointDistance for SpatialEntry {
    fn distance_2(&self, point: &[f64; 2]) -> f64 {
        self.envelope().distance_2(point)
    }

    fn contains_point(&self, point: &[f64; 2]) -> bool {
        self.envelope().contains_point(point)
    }
}

// ---------------------------------------------------------------------------
// Spatial index
// ---------------------------------------------------------------------------

/// Spatial index wrapping an `rstar::RTree` for efficient spatial queries.
///
/// Maintains a lazily-rebuilt index of entity bounding boxes.  After
/// entities are created, modified, or destroyed, call [`ensure_clean`] before
/// performing queries so the tree is rebuilt from the current ECS state.
///
/// The internal `bounds` map tracks bounding boxes for incremental removal.
///
/// [`ensure_clean`]: SpatialIndex::ensure_clean
pub struct SpatialIndex {
    pub tree: RTree<SpatialEntry>,
    /// Per-entity bounding boxes for incremental removal and bookkeeping.
    pub bounds: HashMap<hecs::Entity, BoundingBox2D>,
    pub dirty: bool,
}

impl SpatialIndex {
    /// Create a new empty spatial index.
    pub fn new() -> Self {
        Self {
            tree: RTree::new(),
            bounds: HashMap::new(),
            dirty: false,
        }
    }

    /// Insert an entity with its bounding box into the index.
    ///
    /// This does **not** clear the dirty flag — callers that have marked
    /// the index dirty (e.g. after a batch of entity modifications) must
    /// use [`ensure_clean`] explicitly before the next query.
    ///
    /// [`ensure_clean`]: SpatialIndex::ensure_clean
    pub fn insert(&mut self, entity: hecs::Entity, bounds: BoundingBox2D) {
        self.bounds.insert(entity, bounds);
        self.tree.insert(SpatialEntry { entity, bounds });
    }

    /// Remove an entity from the index.
    ///
    /// Uses the tracked bounding box to construct a matching entry with the
    /// correct envelope so that [`RTree::remove`] can locate it (rstar 0.12
    /// requires `PartialEq` + matching envelope for removal).
    pub fn remove(&mut self, entity: hecs::Entity) {
        if let Some(&bounds) = self.bounds.get(&entity) {
            let dummy = SpatialEntry { entity, bounds };
            self.tree.remove(&dummy);
        }
        self.bounds.remove(&entity);
    }

    /// Find the nearest entity to a world-space point.
    ///
    /// Returns `None` if the index is empty.
    pub fn nearest_neighbor(&self, point: Point2D) -> Option<hecs::Entity> {
        self.tree
            .nearest_neighbor(&[point.x, point.y])
            .map(|entry| entry.entity)
    }

    /// Find all entities whose bounding box is **strictly inside** `rect`.
    ///
    /// Uses `rstar::RTree::locate_in_envelope` which returns entries fully
    /// contained within the query envelope.
    pub fn enclosed_in(&self, rect: &BoundingBox2D) -> Vec<hecs::Entity> {
        let envelope = AABB::from_corners(
            [rect.min.x, rect.min.y],
            [rect.max.x, rect.max.y],
        );
        self.tree
            .locate_in_envelope(&envelope)
            .map(|entry| entry.entity)
            .collect()
    }

    /// Find all entities whose bounding box **overlaps** `rect`.
    ///
    /// Uses `rstar::RTree::locate_in_envelope_intersecting` which returns
    /// entries that overlap the query envelope (including partial overlaps).
    pub fn intersecting(&self, rect: &BoundingBox2D) -> Vec<hecs::Entity> {
        let envelope = AABB::from_corners(
            [rect.min.x, rect.min.y],
            [rect.max.x, rect.max.y],
        );
        self.tree
            .locate_in_envelope_intersecting(&envelope)
            .map(|entry| entry.entity)
            .collect()
    }

    /// Rebuild the index from the current ECS world state.
    ///
    /// Iterates all entities with geometry components (`LineData`,
    /// `CircleData`, `ArcData`, `PolylineData`), computes their bounding
    /// boxes, and bulk-loads them into a fresh R-tree.  The companion
    /// bounds map is also repopulated.
    pub fn rebuild(&mut self, world: &hecs::World) {
        let mut entries: Vec<SpatialEntry> = Vec::new();
        let mut new_bounds: HashMap<hecs::Entity, BoundingBox2D> =
            HashMap::new();

        for (entity, (line,)) in world.query::<(&LineData,)>().iter() {
            let bb = line.bounding_box();
            new_bounds.insert(entity, bb);
            entries.push(SpatialEntry {
                entity,
                bounds: bb,
            });
        }

        for (entity, (circle,)) in world.query::<(&CircleData,)>().iter() {
            let bb = circle.bounding_box();
            new_bounds.insert(entity, bb);
            entries.push(SpatialEntry {
                entity,
                bounds: bb,
            });
        }

        for (entity, (arc,)) in world.query::<(&ArcData,)>().iter() {
            let bb = arc.bounding_box();
            new_bounds.insert(entity, bb);
            entries.push(SpatialEntry {
                entity,
                bounds: bb,
            });
        }

        for (entity, (polyline,)) in world.query::<(&PolylineData,)>().iter() {
            let bb = polyline.bounding_box();
            new_bounds.insert(entity, bb);
            entries.push(SpatialEntry {
                entity,
                bounds: bb,
            });
        }

        self.tree = RTree::bulk_load(entries);
        self.bounds = new_bounds;
        self.dirty = false;
    }

    /// Rebuild the index if it is marked dirty.
    ///
    /// Call this before any query sequence when entities may have been
    /// modified since the last rebuild.
    pub fn ensure_clean(&mut self, world: &hecs::World) {
        if self.dirty {
            self.rebuild(world);
        }
    }
}

impl Default for SpatialIndex {
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
    use crate::ecs::components::{ArcData, CircleData, LineData, PolylineData, Renderable};
    use crate::util::Color;

    // ------------------------------------------------------------------
    // Helpers
    // ------------------------------------------------------------------

    /// Spawn a line entity in the given world.
    fn make_line_entity(
        world: &mut hecs::World,
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
    ) -> hecs::Entity {
        world.spawn((
            LineData {
                start: Point2D::new(x1, y1),
                end: Point2D::new(x2, y2),
                color: Color::WHITE,
                width: 1.0,
            },
            Renderable,
        ))
    }

    /// Spawn a circle entity in the given world.
    fn make_circle_entity(
        world: &mut hecs::World,
        cx: f64,
        cy: f64,
        r: f64,
    ) -> hecs::Entity {
        world.spawn((
            CircleData {
                center: Point2D::new(cx, cy),
                radius: r,
                color: Color::WHITE,
                width: 1.0,
            },
            Renderable,
        ))
    }

    /// Sort entities by their opaque bits for deterministic ordering.
    fn sort_entities(entities: &mut [hecs::Entity]) {
        entities.sort_by_key(|e| e.to_bits());
    }

    // ------------------------------------------------------------------
    // insert + nearest_neighbor
    // ------------------------------------------------------------------
    #[test]
    fn test_insert_and_nearest_neighbor() {
        let mut index = SpatialIndex::new();
        let mut world = hecs::World::new();

        let e1 = make_line_entity(&mut world, 0.0, 0.0, 10.0, 10.0);
        let e2 = make_circle_entity(&mut world, 100.0, 100.0, 5.0);

        index.rebuild(&world);

        // Query near e1 (centre of line bounding box)
        assert_eq!(
            index.nearest_neighbor(Point2D::new(5.0, 5.0)),
            Some(e1)
        );

        // Query near e2
        assert_eq!(
            index.nearest_neighbor(Point2D::new(100.0, 100.0)),
            Some(e2)
        );
    }

    // ------------------------------------------------------------------
    // remove
    // ------------------------------------------------------------------
    #[test]
    fn test_remove() {
        let mut index = SpatialIndex::new();
        let mut world = hecs::World::new();

        let e1 = make_line_entity(&mut world, 0.0, 0.0, 10.0, 10.0);
        index.rebuild(&world);

        // Entity should be findable before removal
        assert!(index.nearest_neighbor(Point2D::new(5.0, 5.0)).is_some());

        index.remove(e1);

        // After removal the index should not contain the entity
        assert!(index.nearest_neighbor(Point2D::new(5.0, 5.0)).is_none());
        assert!(!index.bounds.contains_key(&e1));
    }

    // ------------------------------------------------------------------
    // remove non-existent entity (should not panic)
    // ------------------------------------------------------------------
    #[test]
    fn test_remove_nonexistent() {
        let mut index = SpatialIndex::new();

        // from_bits packs (generation << 32) | id; both must be non-zero.
        let dummy =
            hecs::Entity::from_bits(1u64 << 32 | 1).unwrap();
        // Should not panic
        index.remove(dummy);
    }

    // ------------------------------------------------------------------
    // enclosed_in
    // ------------------------------------------------------------------
    #[test]
    fn test_enclosed_in() {
        let mut index = SpatialIndex::new();
        let mut world = hecs::World::new();

        // Entity bbox: (0,0) to (10,10)
        let e_inside = make_line_entity(&mut world, 0.0, 0.0, 10.0, 10.0);
        // Entity bbox: (20,20) to (30,30) — well outside
        let _e_outside = make_circle_entity(&mut world, 25.0, 25.0, 5.0);
        // Entity bbox: (5,5) to (15,15) — partially overlaps
        let _e_overlap = make_line_entity(&mut world, 5.0, 5.0, 15.0, 15.0);

        index.rebuild(&world);

        // Query rect (0,0) to (10,10):
        // e_inside is strictly inside; e_outside is outside; e_overlap extends to 15
        let rect = BoundingBox2D {
            min: Point2D::new(0.0, 0.0),
            max: Point2D::new(10.0, 10.0),
        };
        let mut results = index.enclosed_in(&rect);
        sort_entities(&mut results);

        assert_eq!(
            results, vec![e_inside],
            "only e_inside is strictly enclosed"
        );
    }

    // ------------------------------------------------------------------
    // intersecting
    // ------------------------------------------------------------------
    #[test]
    fn test_intersecting() {
        let mut index = SpatialIndex::new();
        let mut world = hecs::World::new();

        // Entity bbox: (0,0) to (10,10)
        let e_inside = make_line_entity(&mut world, 0.0, 0.0, 10.0, 10.0);
        // Entity bbox: (20,20) to (30,30) — outside
        let e_outside = make_circle_entity(&mut world, 25.0, 25.0, 5.0);
        // Entity bbox: (8,8) to (18,18) — partially overlaps (8-10)
        let e_overlap = make_line_entity(&mut world, 8.0, 8.0, 18.0, 18.0);

        index.rebuild(&world);

        let rect = BoundingBox2D {
            min: Point2D::new(0.0, 0.0),
            max: Point2D::new(10.0, 10.0),
        };
        let mut results = index.intersecting(&rect);
        sort_entities(&mut results);

        assert_eq!(
            results,
            vec![e_inside, e_overlap],
            "enclosed + overlapping entities should be found by intersecting"
        );

        // e_outside must NOT appear
        assert!(
            !results.contains(&e_outside),
            "outside entity should not intersect"
        );
    }

    // ------------------------------------------------------------------
    // enclosed_in vs intersecting: partial overlap
    // ------------------------------------------------------------------
    #[test]
    fn test_enclosed_vs_intersecting_partial_overlap() {
        let mut index = SpatialIndex::new();
        let mut world = hecs::World::new();

        // Entity bbox extending from (5,5) to (15,15)
        let e = make_line_entity(&mut world, 5.0, 5.0, 15.0, 15.0);
        index.rebuild(&world);

        // Query rect (0,0) to (10,10):
        // Entity bbox (5,5)-(15,15) overlaps but is NOT strictly inside
        let rect = BoundingBox2D {
            min: Point2D::new(0.0, 0.0),
            max: Point2D::new(10.0, 10.0),
        };

        let enclosed = index.enclosed_in(&rect);
        let intersecting = index.intersecting(&rect);

        assert!(
            enclosed.is_empty(),
            "entity extends to x=15, should not be strictly inside"
        );
        assert!(
            intersecting.contains(&e),
            "entity overlaps [0,10]x[0,10], should be found by intersecting"
        );
    }

    // ------------------------------------------------------------------
    // empty index
    // ------------------------------------------------------------------
    #[test]
    fn test_empty_index_returns_none() {
        let index = SpatialIndex::new();

        assert!(index.nearest_neighbor(Point2D::new(0.0, 0.0)).is_none());
        assert!(index
            .enclosed_in(&BoundingBox2D {
                min: Point2D::new(0.0, 0.0),
                max: Point2D::new(10.0, 10.0),
            })
            .is_empty());
        assert!(index
            .intersecting(&BoundingBox2D {
                min: Point2D::new(0.0, 0.0),
                max: Point2D::new(10.0, 10.0),
            })
            .is_empty());
        assert!(index.bounds.is_empty());
    }

    // ------------------------------------------------------------------
    // rebuild from ECS world
    // ------------------------------------------------------------------
    #[test]
    fn test_rebuild_from_world() {
        let mut index = SpatialIndex::new();
        let mut world = hecs::World::new();

        let e1 = make_line_entity(&mut world, 0.0, 0.0, 20.0, 20.0);
        let e2 = make_circle_entity(&mut world, 50.0, 50.0, 10.0);

        index.rebuild(&world);

        // Both entities should be findable
        let mut results = index.enclosed_in(&BoundingBox2D {
            min: Point2D::new(0.0, 0.0),
            max: Point2D::new(100.0, 100.0),
        });
        sort_entities(&mut results);
        assert_eq!(results, vec![e1, e2]);

        // Query near e1
        assert_eq!(index.nearest_neighbor(Point2D::new(10.0, 10.0)), Some(e1));

        // Bounds map should be populated
        assert_eq!(index.bounds.len(), 2);
    }

    // ------------------------------------------------------------------
    // rebuild with PolylineData
    // ------------------------------------------------------------------
    #[test]
    fn test_rebuild_with_polyline() {
        let mut index = SpatialIndex::new();
        let mut world = hecs::World::new();

        let e1 = world.spawn((
            PolylineData {
                vertices: vec![
                    Point2D::new(-5.0, -10.0),
                    Point2D::new(15.0, -10.0),
                    Point2D::new(15.0, 20.0),
                    Point2D::new(-5.0, 20.0),
                ],
                closed: true,
                color: Color::WHITE,
                width: 1.0,
            },
            Renderable,
        ));
        let e2 = make_line_entity(&mut world, 0.0, 0.0, 1.0, 1.0);

        index.rebuild(&world);

        // e1 has bbox (-5,-10) to (15,20)
        assert_eq!(
            index.nearest_neighbor(Point2D::new(0.0, 0.0)),
            Some(e2),
            "e2 at (0,0) is closer than e1's nearest point at (-5,-10)"
        );

        // Large rect should contain both
        let big_rect = BoundingBox2D {
            min: Point2D::new(-100.0, -100.0),
            max: Point2D::new(100.0, 100.0),
        };
        let mut results = index.enclosed_in(&big_rect);
        sort_entities(&mut results);
        assert_eq!(results, vec![e1, e2]);
    }

    // ------------------------------------------------------------------
    // rebuild with all four geometry types
    // ------------------------------------------------------------------
    #[test]
    fn test_rebuild_all_geometry_types() {
        let mut index = SpatialIndex::new();
        let mut world = hecs::World::new();

        let _e1 = make_line_entity(&mut world, 0.0, 0.0, 10.0, 10.0);
        let _e2 = make_circle_entity(&mut world, 50.0, 50.0, 5.0);
        let _e3 = world.spawn((
            ArcData {
                center: Point2D::new(100.0, 100.0),
                radius: 8.0,
                start_angle: 0.0,
                end_angle: 90.0,
                color: Color::WHITE,
                width: 1.0,
            },
            Renderable,
        ));
        let _e4 = world.spawn((
            PolylineData {
                vertices: vec![Point2D::new(-10.0, -10.0), Point2D::new(10.0, 10.0)],
                closed: false,
                color: Color::WHITE,
                width: 1.0,
            },
            Renderable,
        ));

        index.rebuild(&world);

        let all_entities = index.enclosed_in(&BoundingBox2D {
            min: Point2D::new(-100.0, -100.0),
            max: Point2D::new(200.0, 200.0),
        });
        assert_eq!(all_entities.len(), 4, "all 4 entities should be found");
        assert_eq!(index.bounds.len(), 4, "bounds map should match");
    }

    // ------------------------------------------------------------------
    // dirty flag
    // ------------------------------------------------------------------
    #[test]
    fn test_dirty_flag_after_rebuild() {
        let mut index = SpatialIndex::new();
        let mut world = hecs::World::new();

        // Initially not dirty
        assert!(!index.dirty);

        let _e = make_line_entity(&mut world, 0.0, 0.0, 1.0, 1.0);
        index.rebuild(&world);
        assert!(!index.dirty, "after rebuild, dirty should be false");
    }

    #[test]
    fn test_dirty_flag_after_insert() {
        let mut index = SpatialIndex::new();

        let dummy = hecs::Entity::from_bits(1u64 << 32 | 42).unwrap();
        let bw = BoundingBox2D {
            min: Point2D::new(0.0, 0.0),
            max: Point2D::new(10.0, 10.0),
        };
        index.insert(dummy, bw);
        assert!(!index.dirty, "after insert, dirty should be false");
    }

    // ------------------------------------------------------------------
    // ensure_clean triggers rebuild when dirty
    // ------------------------------------------------------------------
    #[test]
    fn test_ensure_clean_rebuilds_when_dirty() {
        let mut index = SpatialIndex::new();
        let mut world = hecs::World::new();

        let _e = make_circle_entity(&mut world, 0.0, 0.0, 5.0);
        index.rebuild(&world);

        // Mark dirty manually to simulate out-of-sync state
        index.dirty = true;

        // After the world gains another entity, ensure_clean should
        // re-read from world
        let e2 = make_line_entity(&mut world, 100.0, 100.0, 200.0, 200.0);
        index.ensure_clean(&world);

        assert!(!index.dirty, "ensure_clean cleared the dirty flag");

        // Because rebuild re-reads from world, e2 should now be in the tree
        let near_e2 = index.nearest_neighbor(Point2D::new(150.0, 150.0));
        assert_eq!(near_e2, Some(e2));
    }

    // ------------------------------------------------------------------
    // PartialEq compares entity only
    // ------------------------------------------------------------------
    #[test]
    fn test_spatial_entry_partial_eq_entity_only() {
        let e1 = hecs::Entity::from_bits(1u64 << 32 | 1).unwrap();
        let e2 = hecs::Entity::from_bits(1u64 << 32 | 2).unwrap();

        let a = SpatialEntry {
            entity: e1,
            bounds: BoundingBox2D {
                min: Point2D::new(0.0, 0.0),
                max: Point2D::new(10.0, 10.0),
            },
        };
        let b = SpatialEntry {
            entity: e1, // same entity, different bounds
            bounds: BoundingBox2D {
                min: Point2D::new(20.0, 20.0),
                max: Point2D::new(30.0, 30.0),
            },
        };
        assert_eq!(a, b, "SpatialEntry equality should be entity-only");

        let c = SpatialEntry {
            entity: e2, // different entity
            bounds: a.bounds,
        };
        assert_ne!(a, c, "different entity => not equal");
    }

    // ------------------------------------------------------------------
    // multiple entities — nearest returns correct one from set
    // ------------------------------------------------------------------
    #[test]
    fn test_nearest_neighbor_among_multiple() {
        let mut index = SpatialIndex::new();
        let mut world = hecs::World::new();

        // bbox (0,0)-(2,2)
        let e_near = make_line_entity(&mut world, 0.0, 0.0, 2.0, 2.0);
        // bbox (99,99)-(101,101)
        let e_far = make_circle_entity(&mut world, 100.0, 100.0, 1.0);

        index.rebuild(&world);

        // Query point (1,1) — e_near is closer
        assert_eq!(
            index.nearest_neighbor(Point2D::new(1.0, 1.0)),
            Some(e_near)
        );

        // Query point (150,150) — e_far is closer
        assert_eq!(
            index.nearest_neighbor(Point2D::new(150.0, 150.0)),
            Some(e_far)
        );
    }

    // ------------------------------------------------------------------
    // insert updates bounds map
    // ------------------------------------------------------------------
    #[test]
    fn test_insert_populates_bounds() {
        let mut index = SpatialIndex::new();

        let e = hecs::Entity::from_bits(1u64 << 32 | 99).unwrap();
        let bb = BoundingBox2D {
            min: Point2D::new(-5.0, -5.0),
            max: Point2D::new(5.0, 5.0),
        };
        index.insert(e, bb);

        assert!(index.bounds.contains_key(&e), "bounds map should contain the entity");
        assert_eq!(index.nearest_neighbor(Point2D::new(0.0, 0.0)), Some(e));
    }
}
