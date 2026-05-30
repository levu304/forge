//! Block explode operation.
//!
//! [`explode_block_insert`] decomposes a [`BlockRef`] insert entity into its
//! constituent geometric primitives (lines, circles, arcs, polylines) by
//! spawning independent ECS entities for each [`BlockEntity`] in the block
//! definition.  The block definition itself is **not** removed from the
//! [`BlockTable`].

use hecs::{Entity, World};

use crate::block::definition::{BlockEntity, BlockTable, BlockError};
use crate::block::insert::BlockRef;
use crate::ecs::components::{
    ArcData, CircleData, LayerRef, LineData, PolylineData, Renderable,
};
use crate::geometry::Point2D;
use crate::history::{AtomicOp, History, Transaction};
use crate::util::Color;

/// Decompose a block-instance entity into its constituent primitives.
///
/// For each [`BlockEntity`] in the block definition referenced by
/// `block_ref`, a new ECS entity is spawned with the transformed geometry
/// plus a [`Renderable`] marker, the **insert entity's** [`LayerRef`], and
/// the original [`PropertySource`] from the [`BlockEntity`].
///
/// The `insert_entity` is despawned after extraction.  The block definition
/// remains in the [`BlockTable`] — it is **not** deleted.
///
/// # Errors
///
/// Returns [`BlockError::BlockNotFound`] if `block_ref.definition` does not
/// exist in `block_table`.
///
/// # Geometry transformation
///
/// * `Line` / `Polyline` vertices — each point is transformed via
///   [`Transform2D::apply_to_point`](crate::geometry::Transform2D::apply_to_point).
/// * `Circle` / `Arc` radius — scaled by `transform.scale_x`. Non-uniform
///   scale handling (scale_x != scale_y) for circle/arc radius is deferred
///   to v0.3.1 — a `debug_assert!` enforces uniform scale for now.
/// * `Circle` / `Arc` center — transformed via `apply_to_point`.
pub fn explode_block_insert(
    world: &mut World,
    block_table: &BlockTable,
    insert_entity: Entity,
    block_ref: &BlockRef,
    history: &mut History,
) -> Result<(), BlockError> {
    // ── 1. Look up the block definition ──────────────────────────────────
    let def = block_table
        .get(block_ref.definition)
        .ok_or(BlockError::BlockNotFound)?;

    // ── 2. Read the insert entity's LayerRef ─────────────────────────────
    let layer_ref = world
        .get::<&LayerRef>(insert_entity)
        .map(|ref_component| *ref_component)
        .unwrap_or(LayerRef(0));

    let transform = block_ref.transform;
    debug_assert!(
        (transform.scale_x - transform.scale_y).abs() < f64::EPSILON,
        "EXPLODE circle/arc radius scaling does not support non-uniform block transforms (scale_x={}, scale_y={}). Non-uniform scale handling deferred to v0.3.1.",
        transform.scale_x, transform.scale_y,
    );
    let scale_x = transform.scale_x;

    // ── 3. Spawn entities for each BlockEntity ──────────────────────────
    let mut tx = Transaction::new("Explode Block");

    for block_entity in &def.entities {
        match block_entity {
            BlockEntity::Line(p1, p2, ps, _lr) => {
                let start = transform.apply_to_point(*p1);
                let end = transform.apply_to_point(*p2);
                let entity = world.spawn((
                    LineData {
                        start,
                        end,
                        color: Color::WHITE,
                        width: 1.0,
                    },
                    Renderable,
                    layer_ref,
                    *ps,
                ));
                tx.push(AtomicOp::SpawnLine {
                    entity,
                    data: LineData {
                        start,
                        end,
                        color: Color::WHITE,
                        width: 1.0,
                    },
                });
            }
            BlockEntity::Circle(center, radius, ps, _lr) => {
                let center = transform.apply_to_point(*center);
                let radius = radius * scale_x;
                let entity = world.spawn((
                    CircleData {
                        center,
                        radius,
                        color: Color::WHITE,
                        width: 1.0,
                    },
                    Renderable,
                    layer_ref,
                    *ps,
                ));
                tx.push(AtomicOp::SpawnCircle {
                    entity,
                    data: CircleData {
                        center,
                        radius,
                        color: Color::WHITE,
                        width: 1.0,
                    },
                });
            }
            BlockEntity::Arc(center, radius, start_angle, end_angle, ps, _lr) => {
                let center = transform.apply_to_point(*center);
                let radius = radius * scale_x;
                let start_angle = transform.apply_to_angle(*start_angle);
                let end_angle = transform.apply_to_angle(*end_angle);
                let entity = world.spawn((
                    ArcData {
                        center,
                        radius,
                        start_angle,
                        end_angle,
                        color: Color::WHITE,
                        width: 1.0,
                    },
                    Renderable,
                    layer_ref,
                    *ps,
                ));
                tx.push(AtomicOp::SpawnArc {
                    entity,
                    data: ArcData {
                        center,
                        radius,
                        start_angle,
                        end_angle,
                        color: Color::WHITE,
                        width: 1.0,
                    },
                });
            }
            BlockEntity::Polyline(vertices, closed, ps, _lr) => {
                let vertices: Vec<Point2D> = vertices
                    .iter()
                    .map(|v| transform.apply_to_point(*v))
                    .collect();
                let entity = world.spawn((
                    PolylineData {
                        vertices: vertices.clone(),
                        closed: *closed,
                        color: Color::WHITE,
                        width: 1.0,
                    },
                    Renderable,
                    layer_ref,
                    *ps,
                ));
                tx.push(AtomicOp::SpawnPolyline {
                    entity,
                    data: PolylineData {
                        vertices,
                        closed: *closed,
                        color: Color::WHITE,
                        width: 1.0,
                    },
                });
            }
        }
    }

    // ── 4. Despawn the insert entity ─────────────────────────────────────
    // Capture the block_ref's storage to include in the transaction
    world.despawn(insert_entity).ok();
    // Note: we don't record a Despawn* AtomicOp here since the insert entity
    // carries a BlockRef component (not a geometry primitive). The transaction
    // records the spawns of the exploded pieces; the despawn is implicit in
    // the reverse direction.
    //
    // However, for proper undo/redo the despawn needs to be tracked too.
    // We skip it here because the current AtomicOp variants only support
    // LineData/CircleData/etc. — the insert entity's components (BlockRef,
    // Position, LayerRef) don't match any existing Despawn* variant.
    //
    // TODO: Add a generic DespawnEntity variant in Step 7+ for non-geometry
    // entities so that explode undo/redo is fully symmetric.

    // ── 5. Push to history ──────────────────────────────────────────────
    if !tx.is_empty() {
        history.push(tx);
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::definition::{BlockDef, BlockEntity, BlockId, BlockTable};
    use crate::ecs::components::PropertySource;
    use crate::geometry::{BoundingBox2D, Point2D, Transform2D};

    /// Helper: create a block definition with one line entity.
    fn make_line_def() -> BlockDef {
        BlockDef {
            name: "line-block".to_string(),
            base_point: Point2D::new(0.0, 0.0),
            entities: vec![BlockEntity::Line(
                Point2D::new(0.0, 0.0),
                Point2D::new(10.0, 10.0),
                PropertySource::ByLayer,
                LayerRef(1),
            )],
            bounds: BoundingBox2D::empty(),
        }
    }

    /// Helper: create a block definition with one circle entity.
    fn make_circle_def() -> BlockDef {
        BlockDef {
            name: "circle-block".to_string(),
            base_point: Point2D::new(0.0, 0.0),
            entities: vec![BlockEntity::Circle(
                Point2D::new(5.0, 5.0),
                3.0,
                PropertySource::ByBlock,
                LayerRef(0),
            )],
            bounds: BoundingBox2D::empty(),
        }
    }

    /// Helper: create a block definition with one arc entity.
    fn make_arc_def() -> BlockDef {
        BlockDef {
            name: "arc-block".to_string(),
            base_point: Point2D::new(0.0, 0.0),
            entities: vec![BlockEntity::Arc(
                Point2D::new(0.0, 0.0),
                5.0,
                0.0,
                90.0,
                PropertySource::Explicit,
                LayerRef(0),
            )],
            bounds: BoundingBox2D::empty(),
        }
    }

    /// Helper: create a block definition with one polyline entity.
    fn make_polyline_def() -> BlockDef {
        BlockDef {
            name: "polyline-block".to_string(),
            base_point: Point2D::new(0.0, 0.0),
            entities: vec![BlockEntity::Polyline(
                vec![
                    Point2D::new(0.0, 0.0),
                    Point2D::new(10.0, 0.0),
                    Point2D::new(5.0, 10.0),
                ],
                true,
                PropertySource::ByLayer,
                LayerRef(0),
            )],
            bounds: BoundingBox2D::empty(),
        }
    }

    /// Spawn an insert entity with BlockRef + LayerRef in a world, returning
    /// the entity handle and inserting the block def into the table.
    fn setup_insert(
        world: &mut World,
        table: &mut BlockTable,
        def: BlockDef,
        layer_id: u32,
        transform: Transform2D,
    ) -> (Entity, BlockRef) {
        let block_id = table.insert(def).unwrap();
        let block_ref = BlockRef {
            definition: block_id,
            transform,
        };
        let entity = world.spawn((block_ref, Renderable, LayerRef(layer_id)));
        (entity, block_ref)
    }

    // ------------------------------------------------------------------
    // explode_block_insert — line transformation
    // ------------------------------------------------------------------

    #[test]
    fn test_explode_line_transforms_geometry() {
        let mut world = World::new();
        let mut table = BlockTable::new();
        let mut history = History::new();

        let transform = Transform2D::new(Point2D::new(10.0, 20.0), 0.0, 2.0, 2.0);
        let (entity, block_ref) =
            setup_insert(&mut world, &mut table, make_line_def(), 5, transform);

        // Confirm the insert entity exists before explode
        assert!(world.get::<&BlockRef>(entity).is_ok());

        explode_block_insert(&mut world, &table, entity, &block_ref, &mut history).unwrap();

        // Insert entity should be despawned
        assert!(world.get::<&BlockRef>(entity).is_err());

        // There should be one spawned line entity
        let mut line_entities: Vec<Entity> = Vec::new();
        for (e, _line) in &mut world.query::<&LineData>() {
            line_entities.push(e);
        }
        assert_eq!(line_entities.len(), 1);

        let line = world.get::<&LineData>(line_entities[0]).unwrap();
        // Original: (0,0) → (10,10). Scale 2×: (0,0) → (20,20). Translate (10,20): (10,20) → (30,40)
        assert_eq!(line.start.x, 10.0);
        assert_eq!(line.start.y, 20.0);
        assert_eq!(line.end.x, 30.0);
        assert_eq!(line.end.y, 40.0);
    }

    // ------------------------------------------------------------------
    // Exploded entities inherit LayerRef from insert entity
    // ------------------------------------------------------------------

    #[test]
    fn test_explode_inherits_layer_from_insert() {
        let mut world = World::new();
        let mut table = BlockTable::new();
        let mut history = History::new();

        // Block def entities have LayerRef(1), but insert entity has LayerRef(42)
        let (entity, block_ref) = setup_insert(
            &mut world,
            &mut table,
            make_line_def(),
            42, // layer_id
            Transform2D::IDENTITY,
        );

        explode_block_insert(&mut world, &table, entity, &block_ref, &mut history).unwrap();

        for (_e, lr) in &mut world.query::<&LayerRef>() {
            assert_eq!(lr.0, 42, "exploded entity should inherit insert's LayerRef");
        }
    }

    // ------------------------------------------------------------------
    // Exploded entities carry PropertySource from the BlockEntity
    // ------------------------------------------------------------------

    #[test]
    fn test_explode_preserves_property_source() {
        let mut world = World::new();
        let mut table = BlockTable::new();
        let mut history = History::new();

        let def = BlockDef {
            name: "multi-ps".to_string(),
            base_point: Point2D::new(0.0, 0.0),
            entities: vec![
                BlockEntity::Line(
                    Point2D::new(0.0, 0.0),
                    Point2D::new(1.0, 1.0),
                    PropertySource::ByLayer,
                    LayerRef(0),
                ),
                BlockEntity::Circle(
                    Point2D::new(5.0, 5.0),
                    2.0,
                    PropertySource::ByBlock,
                    LayerRef(1),
                ),
            ],
            bounds: BoundingBox2D::empty(),
        };
        let (entity, block_ref) = setup_insert(
            &mut world,
            &mut table,
            def,
            7,
            Transform2D::IDENTITY,
        );

        explode_block_insert(&mut world, &table, entity, &block_ref, &mut history).unwrap();

        let mut ps_values: Vec<PropertySource> = Vec::new();
        for (_e, ps) in &mut world.query::<&PropertySource>() {
            ps_values.push(*ps);
        }
        ps_values.sort_by_key(|ps| match ps {
            PropertySource::ByLayer => 0,
            PropertySource::ByBlock => 1,
            PropertySource::Explicit => 2,
        });

        assert_eq!(ps_values.len(), 2);
        assert_eq!(ps_values[0], PropertySource::ByLayer);
        assert_eq!(ps_values[1], PropertySource::ByBlock);
    }

    // ------------------------------------------------------------------
    // Circle radius is scaled by transform.scale_x
    // ------------------------------------------------------------------

    #[test]
    fn test_explode_circle_radius_scaled() {
        let mut world = World::new();
        let mut table = BlockTable::new();
        let mut history = History::new();

        // Uniform scale 3× — non-uniform scale handling deferred to v0.3.1
        let transform = Transform2D::new(Point2D::new(0.0, 0.0), 0.0, 3.0, 3.0);
        let (entity, block_ref) = setup_insert(
            &mut world,
            &mut table,
            make_circle_def(),
            0,
            transform,
        );

        explode_block_insert(&mut world, &table, entity, &block_ref, &mut history).unwrap();

        for (_e, circle) in &mut world.query::<&CircleData>() {
            // radius was 3.0, scale 3× → 9.0
            assert!((circle.radius - 9.0).abs() < 1e-12, "expected radius 9.0, got {}", circle.radius);
            // center was (5,5), scale (3×, 3×) → (15,15), translate (0,0) → (15,15)
            assert_eq!(circle.center.x, 15.0);
            assert_eq!(circle.center.y, 15.0);
        }
    }

    // ------------------------------------------------------------------
    // Arc radius is scaled by transform.scale_x
    // ------------------------------------------------------------------

    #[test]
    fn test_explode_arc_radius_scaled() {
        let mut world = World::new();
        let mut table = BlockTable::new();
        let mut history = History::new();

        let transform = Transform2D::new(Point2D::new(5.0, 5.0), 0.0, 2.0, 2.0);
        let (entity, block_ref) = setup_insert(
            &mut world,
            &mut table,
            make_arc_def(),
            0,
            transform,
        );

        explode_block_insert(&mut world, &table, entity, &block_ref, &mut history).unwrap();

        for (_e, arc) in &mut world.query::<&ArcData>() {
            // radius was 5.0, scale_x is 2.0 → 10.0
            assert!((arc.radius - 10.0).abs() < 1e-12, "expected radius 10.0, got {}", arc.radius);
            // center was (0,0), scale 2× → (0,0), translate (5,5) → (5,5)
            assert_eq!(arc.center.x, 5.0);
            assert_eq!(arc.center.y, 5.0);
        }
    }

    // ------------------------------------------------------------------
    // Arc angles are rotated by transform.rotation
    // ------------------------------------------------------------------

    #[test]
    fn test_explode_arc_angles_rotated_by_transform() {
        let mut world = World::new();
        let mut table = BlockTable::new();
        let mut history = History::new();

        // Block with arc 0°→90°, insert with 90° rotation (π/2)
        let transform = Transform2D::new(
            Point2D::new(0.0, 0.0),
            std::f64::consts::FRAC_PI_2,
            1.0,
            1.0,
        );
        let (entity, block_ref) = setup_insert(
            &mut world,
            &mut table,
            make_arc_def(),
            0,
            transform,
        );

        explode_block_insert(&mut world, &table, entity, &block_ref, &mut history).unwrap();

        for (_e, arc) in &mut world.query::<&ArcData>() {
            // 0° + 90° → 90°
            assert!(
                (arc.start_angle - 90.0).abs() < 1e-12,
                "expected start_angle 90°, got {}",
                arc.start_angle,
            );
            // 90° + 90° → 180°
            assert!(
                (arc.end_angle - 180.0).abs() < 1e-12,
                "expected end_angle 180°, got {}",
                arc.end_angle,
            );
        }
    }

    #[test]
    fn test_explode_arc_angles_wrap_at_360() {
        let mut world = World::new();
        let mut table = BlockTable::new();
        let mut history = History::new();

        // Arc 300°→350°, rotate by 90° → should wrap: 30°→80°
        let def = BlockDef {
            name: "wrap-arc".to_string(),
            base_point: Point2D::new(0.0, 0.0),
            entities: vec![BlockEntity::Arc(
                Point2D::new(0.0, 0.0),
                5.0,
                300.0,
                350.0,
                PropertySource::Explicit,
                LayerRef(0),
            )],
            bounds: BoundingBox2D::empty(),
        };

        let transform = Transform2D::new(
            Point2D::new(0.0, 0.0),
            std::f64::consts::FRAC_PI_2,
            1.0,
            1.0,
        );
        let mut table = BlockTable::new();
        let block_id = table.insert(def).unwrap();
        let block_ref = BlockRef { definition: block_id, transform };
        let entity = world.spawn((block_ref, Renderable, LayerRef(0)));

        explode_block_insert(&mut world, &table, entity, &block_ref, &mut history).unwrap();

        for (_e, arc) in &mut world.query::<&ArcData>() {
            // 300° + 90° = 390°, rem_euclid 360 → 30°
            assert!(
                (arc.start_angle - 30.0).abs() < 1e-12,
                "expected start_angle 30°, got {}",
                arc.start_angle,
            );
            // 350° + 90° = 440°, rem_euclid 360 → 80°
            assert!(
                (arc.end_angle - 80.0).abs() < 1e-12,
                "expected end_angle 80°, got {}",
                arc.end_angle,
            );
        }
    }

    // ------------------------------------------------------------------
    // Polyline vertices are transformed
    // ------------------------------------------------------------------

    #[test]
    fn test_explode_polyline_vertices_transformed() {
        let mut world = World::new();
        let mut table = BlockTable::new();
        let mut history = History::new();

        let transform = Transform2D::new(Point2D::new(1.0, 2.0), 0.0, 2.0, 2.0);
        let (entity, block_ref) = setup_insert(
            &mut world,
            &mut table,
            make_polyline_def(),
            0,
            transform,
        );

        explode_block_insert(&mut world, &table, entity, &block_ref, &mut history).unwrap();

        for (_e, poly) in &mut world.query::<&PolylineData>() {
            assert_eq!(poly.vertices.len(), 3);
            // (0,0) → scale 2× → (0,0) → translate (1,2) → (1,2)
            assert_eq!(poly.vertices[0].x, 1.0);
            assert_eq!(poly.vertices[0].y, 2.0);
            // (10,0) → scale 2× → (20,0) → translate (1,2) → (21,2)
            assert_eq!(poly.vertices[1].x, 21.0);
            assert_eq!(poly.vertices[1].y, 2.0);
            // closed flag preserved
            assert!(poly.closed);
        }
    }

    // ------------------------------------------------------------------
    // Insert entity is despawned
    // ------------------------------------------------------------------

    #[test]
    fn test_explode_despawns_insert_entity() {
        let mut world = World::new();
        let mut table = BlockTable::new();
        let mut history = History::new();

        let (entity, block_ref) = setup_insert(
            &mut world,
            &mut table,
            make_line_def(),
            0,
            Transform2D::IDENTITY,
        );

        assert!(world.get::<&BlockRef>(entity).is_ok());
        explode_block_insert(&mut world, &table, entity, &block_ref, &mut history).unwrap();
        assert!(world.get::<&BlockRef>(entity).is_err());
    }

    // ------------------------------------------------------------------
    // Error variant: BlockNotFound
    // ------------------------------------------------------------------

    #[test]
    fn test_explode_block_not_found() {
        let mut world = World::new();
        let table = BlockTable::new();
        let mut history = History::new();

        let block_ref = BlockRef {
            definition: BlockId(999),
            transform: Transform2D::IDENTITY,
        };
        let entity = world.spawn((block_ref, Renderable, LayerRef(0)));

        let result = explode_block_insert(&mut world, &table, entity, &block_ref, &mut history);
        assert_eq!(result, Err(BlockError::BlockNotFound));
    }

    // ------------------------------------------------------------------
    // BlockDef stays in table after explode
    // ------------------------------------------------------------------

    #[test]
    fn test_explode_does_not_delete_block_def() {
        let mut world = World::new();
        let mut table = BlockTable::new();
        let mut history = History::new();

        let block_id = {
            let def = make_line_def();
            let (entity, block_ref) = setup_insert(
                &mut world,
                &mut table,
                def,
                0,
                Transform2D::IDENTITY,
            );
            let id = block_ref.definition;
            explode_block_insert(&mut world, &table, entity, &block_ref, &mut history).unwrap();
            id
        };

        // Block definition should still exist
        assert!(table.get(block_id).is_some());
    }

    // ------------------------------------------------------------------
    // History transaction is pushed
    // ------------------------------------------------------------------

    #[test]
    fn test_explode_pushes_history() {
        let mut world = World::new();
        let mut table = BlockTable::new();
        let mut history = History::new();

        let (entity, block_ref) = setup_insert(
            &mut world,
            &mut table,
            make_line_def(),
            0,
            Transform2D::IDENTITY,
        );

        assert!(!history.can_undo());
        explode_block_insert(&mut world, &table, entity, &block_ref, &mut history).unwrap();
        assert!(history.can_undo());
        assert_eq!(history.undo_label(), Some("Explode Block"));
    }
}
