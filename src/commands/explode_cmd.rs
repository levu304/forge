//! EXPLODE command implementation.
//!
//! Single-step toolbar command. Captures selected entities at construction
//! time along with their block definition data (extracted from BlockTable).
//! On Confirm, each block insert entity is decomposed into its constituent
//! primitives by directly spawning the transformed geometry.
//!
//! This approach avoids needing BlockTable access inside on_input() —
//! all data is extracted during construction where BlockTable is available.

use super::{Command, CommandInput, CommandResult, PreviewEntity};
use crate::block::definition::BlockEntity;
use crate::block::insert::BlockRef;
use crate::ecs::components::{
    ArcData, CircleData, LayerRef, LineData, PolylineData, Renderable,
};
use crate::geometry::Point2D;
use crate::history::{AtomicOp, Transaction};
use crate::selection::SelectionManager;
use crate::util::Color;
use hecs::World;

/// A captured block instance ready for explosion.
struct BlockInstance {
    /// Entity handle of the insert entity (will be despawned).
    entity: hecs::Entity,
    /// The block definition's entities (geometry + style).
    def_entities: Vec<BlockEntity>,
    /// The transform to apply to each entity's geometry.
    transform: crate::geometry::Transform2D,
}

/// EXPLODE command — decomposes BlockRef entities into primitives.
pub struct ExplodeCommand {
    /// Captured block instances to explode.
    instances: Vec<BlockInstance>,
}

impl ExplodeCommand {
    /// Create a new EXPLODE command, capturing selection + block data.
    ///
    /// Reads each selected entity's [`BlockRef`] component and looks up the
    /// corresponding [`BlockDef`] from `block_table`.  Entities that are not
    /// block inserts or whose definition is missing are silently skipped.
    pub fn new(
        selection: &SelectionManager,
        world: &World,
        block_table: &crate::block::definition::BlockTable,
    ) -> Self {
        let mut instances = Vec::new();

        for &entity in &selection.selected {
            // Check if entity has a BlockRef component
            if let Ok(block_ref) = world.get::<&BlockRef>(entity) {
                // Look up the block definition
                if let Some(def) = block_table.get(block_ref.definition) {
                    instances.push(BlockInstance {
                        entity,
                        def_entities: def.entities.clone(),
                        transform: block_ref.transform,
                    });
                }
            }
        }

        Self { instances }
    }

    /// Returns `true` if no block instances were captured.
    pub fn is_empty(&self) -> bool {
        self.instances.is_empty()
    }
}

impl Command for ExplodeCommand {
    fn name(&self) -> &'static str {
        "EXPLODE"
    }

    fn prompt(&self) -> String {
        "Select objects to explode:".to_string()
    }

    fn steps_remaining(&self) -> usize {
        1
    }

    fn on_input(&mut self, input: CommandInput, world: &mut World) -> CommandResult {
        match input {
            CommandInput::Confirm => {
                if self.instances.is_empty() {
                    return CommandResult::Error(
                        "No block references found in selection. Select blocks before EXPLODE."
                            .to_string(),
                    );
                }

                let mut tx = Transaction::new("Explode Block");

                for instance in &self.instances {
                    let layer_ref = world
                        .get::<&LayerRef>(instance.entity)
                        .as_deref()
                        .copied()
                        .unwrap_or(LayerRef(0));

                    let transform = instance.transform;
                    debug_assert!(
                        (transform.scale_x - transform.scale_y).abs() < f64::EPSILON,
                        "EXPLODE circle/arc radius scaling does not support non-uniform block transforms (scale_x={}, scale_y={}). Non-uniform scale handling deferred to v0.3.1.",
                        transform.scale_x, transform.scale_y,
                    );
                    let scale_x = transform.scale_x;

                    for block_entity in &instance.def_entities {
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
                                    data: *world.get::<&LineData>(entity).unwrap(),
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
                                    data: *world.get::<&CircleData>(entity).unwrap(),
                                });
                            }
                            BlockEntity::Arc(
                                center,
                                radius,
                                start_angle,
                                end_angle,
                                ps,
                                _lr,
                            ) => {
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
                                    data: *world.get::<&ArcData>(entity).unwrap(),
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
                                    data: world.get::<&PolylineData>(entity).as_deref().unwrap().clone(),
                                });
                            }
                        }
                    }

                    // Despawn the insert entity
                    world.despawn(instance.entity).ok();
                }

                CommandResult::CompleteWithTransaction(tx)
            }
            CommandInput::Cancel => {
                self.on_cancel(world);
                CommandResult::Cancelled
            }
            _ => CommandResult::Error(
                "Press Enter to explode selected blocks, or Esc to cancel.".to_string(),
            ),
        }
    }

    fn on_cancel(&mut self, _world: &mut World) {
        // No cleanup needed — nothing has been spawned yet.
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
    use crate::block::definition::{BlockDef, BlockTable};
    use crate::ecs::components::PropertySource;
    use crate::geometry::{BoundingBox2D, Transform2D};

    fn create_world() -> World {
        World::new()
    }

    /// Helper: create a block table with a line block and spawn an insert entity.
    fn setup_line_insert(
        world: &mut World,
        table: &mut BlockTable,
        layer_id: u32,
        transform: Transform2D,
    ) -> hecs::Entity {
        let def = BlockDef {
            name: "test-line-block".to_string(),
            base_point: Point2D::new(0.0, 0.0),
            entities: vec![BlockEntity::Line(
                Point2D::new(0.0, 0.0),
                Point2D::new(10.0, 10.0),
                PropertySource::ByLayer,
                LayerRef(0),
            )],
            bounds: BoundingBox2D::empty(),
        };
        let block_id = table.insert(def).unwrap();
        let block_ref = BlockRef {
            definition: block_id,
            transform,
        };
        world.spawn((block_ref, Renderable, LayerRef(layer_id)))
    }

    #[test]
    fn test_name_and_prompt() {
        let world = create_world();
        let table = BlockTable::new();
        let sel = SelectionManager::new();
        let cmd = ExplodeCommand::new(&sel, &world, &table);
        assert_eq!(cmd.name(), "EXPLODE");
        assert_eq!(cmd.prompt(), "Select objects to explode:");
        assert_eq!(cmd.steps_remaining(), 1);
    }

    #[test]
    fn test_new_captures_no_instances_with_empty_selection() {
        let world = create_world();
        let table = BlockTable::new();
        let sel = SelectionManager::new();
        let cmd = ExplodeCommand::new(&sel, &world, &table);
        assert!(cmd.is_empty());
    }

    #[test]
    fn test_new_captures_block_instance_from_selection() {
        let mut world = create_world();
        let mut table = BlockTable::new();
        let _e1 = setup_line_insert(&mut world, &mut table, 0, Transform2D::IDENTITY);

        let mut sel = SelectionManager::new();
        // Need to iterate to find entity with BlockRef
        let entity = world
            .query::<&BlockRef>()
            .iter()
            .next()
            .map(|(e, _)| e)
            .unwrap();
        sel.select(&mut world, entity);

        let cmd = ExplodeCommand::new(&sel, &world, &table);
        assert!(!cmd.is_empty());
        assert_eq!(cmd.instances.len(), 1);
    }

    #[test]
    fn test_confirm_explodes_block() {
        let mut world = create_world();
        let mut table = BlockTable::new();
        let entity = setup_line_insert(&mut world, &mut table, 0, Transform2D::IDENTITY);

        let mut sel = SelectionManager::new();
        sel.select(&mut world, entity);

        let mut cmd = ExplodeCommand::new(&sel, &world, &table);

        // Confirm should explode
        let result = cmd.on_input(CommandInput::Confirm, &mut world);
        match result {
            CommandResult::CompleteWithTransaction(tx) => {
                // Transaction should have at least 1 SpawnLine op
                assert!(!tx.is_empty());
                assert_eq!(tx.label, "Explode Block");
            }
            _ => panic!("Expected CompleteWithTransaction, got {:?}", result),
        }

        // Insert entity should be despawned
        assert!(world.get::<&BlockRef>(entity).is_err());

        // Should have spawned a line entity
        let line_count = world.query::<&LineData>().iter().count();
        assert_eq!(line_count, 1);
    }

    #[test]
    fn test_confirm_with_empty_instances_returns_error() {
        let mut world = create_world();
        let table = BlockTable::new();
        let sel = SelectionManager::new();
        let mut cmd = ExplodeCommand::new(&sel, &world, &table);

        let result = cmd.on_input(CommandInput::Confirm, &mut world);
        assert!(matches!(result, CommandResult::Error(_)));
    }

    #[test]
    fn test_cancel_does_nothing() {
        let mut world = create_world();
        let table = BlockTable::new();
        let sel = SelectionManager::new();
        let mut cmd = ExplodeCommand::new(&sel, &world, &table);

        let result = cmd.on_input(CommandInput::Cancel, &mut world);
        assert!(matches!(result, CommandResult::Cancelled));
    }

    #[test]
    fn test_entity_without_block_ref_is_skipped() {
        let mut world = create_world();
        let table = BlockTable::new();
        let e = world.spawn(());

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let cmd = ExplodeCommand::new(&sel, &world, &table);
        assert!(cmd.is_empty());
    }
}
