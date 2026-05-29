//! MATCHPROP command — copy visual properties from one entity to others.
//!
//! The [`match_properties`] function reads the source entity's
//! [`PropertySource`] and [`LayerRef`], then applies them to every
//! target entity, recording undo ops for each.

use hecs::{Entity, World};

use crate::ecs::components::{LayerRef, PropertySource};
use crate::history::{AtomicOp, History, Transaction};

/// Copy visual properties from `source` to every entity in `targets`.
///
/// For each target entity, `match_properties`:
///
/// 1. Captures the target's current [`PropertySource`] (or `ByLayer` if
///    absent) and [`LayerRef`] (or `None` if absent).
/// 2. Inserts the source's [`PropertySource`] and [`LayerRef`] into the
///    target.
/// 3. Builds a [`Transaction`] containing [`SetPropertySource`] and
///    [`SetLayerRef`] ops for every target.
/// 4. Pushes the transaction onto `history`.
///
/// [`SetPropertySource`]: AtomicOp::SetPropertySource
/// [`SetLayerRef`]: AtomicOp::SetLayerRef
///
/// # Panics
///
/// Panics if `source` has been despawned (the caller must ensure the
/// source entity is alive before calling).
pub fn match_properties(
    world: &mut World,
    source: Entity,
    targets: &[Entity],
    history: &mut History,
) {
    // Read source properties (panic if source is dead).
    let src_source = world
        .get::<&PropertySource>(source)
        .map(|r| *r)
        .expect("match_properties: source entity has no PropertySource component");
    let src_layer = world.get::<&LayerRef>(source).ok().map(|r| *r);

    let mut tx = Transaction::new("Match Properties");

    for &target in targets {
        // Skip the source entity if it happens to be in the target list.
        if target == source {
            continue;
        }

        // --- PropertySource ---
        let old_source = world
            .get::<&PropertySource>(target)
            .map(|r| *r)
            .unwrap_or(PropertySource::ByLayer);
        let _ = world.insert_one(target, src_source);
        tx.push(AtomicOp::SetPropertySource {
            entity: target,
            old: old_source,
            new: src_source,
        });

        // --- LayerRef ---
        let old_layer = world.get::<&LayerRef>(target).ok().map(|r| *r);
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

    if !tx.is_empty() {
        history.push(tx);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use hecs::World;

    use super::*;
    use crate::history::History;

    /// Spawn an entity with the given source and layer, returning its handle.
    fn spawn_entity(world: &mut World, source: PropertySource, layer: u32) -> Entity {
        world.spawn((source, LayerRef(layer)))
    }

    #[test]
    fn match_properties_builds_correct_transaction() {
        let mut world = World::new();
        let mut history = History::new();

        let source = spawn_entity(&mut world, PropertySource::Explicit, 5);
        let target_a = spawn_entity(&mut world, PropertySource::ByLayer, 0);
        let target_b = spawn_entity(&mut world, PropertySource::ByBlock, 1);

        match_properties(&mut world, source, &[target_a, target_b], &mut history);

        // Verify the transaction was pushed
        assert!(history.can_undo());
        assert_eq!(history.undo_label(), Some("Match Properties"));

        // Verify entities now have source's properties
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

        // Transaction should have 4 ops (2 per target: SetPropertySource + SetLayerRef)
        let tx = history.undo_stack.back().unwrap();
        assert_eq!(tx.ops.len(), 4);
    }

    #[test]
    fn match_properties_skips_source_if_in_targets() {
        let mut world = World::new();
        let mut history = History::new();

        let source = spawn_entity(&mut world, PropertySource::Explicit, 5);
        let target = spawn_entity(&mut world, PropertySource::ByLayer, 0);

        // Include source in the target list
        match_properties(&mut world, source, &[source, target], &mut history);

        let tx = history.undo_stack.back().unwrap();
        // Only 2 ops (for the real target only)
        assert_eq!(tx.ops.len(), 2);
    }

    #[test]
    fn match_properties_source_source_copied_to_target() {
        let mut world = World::new();
        let mut history = History::new();

        let source = spawn_entity(&mut world, PropertySource::Explicit, 3);
        let target = spawn_entity(&mut world, PropertySource::ByLayer, 0);

        match_properties(&mut world, source, &[target], &mut history);

        // Target should have source's PropertySource
        assert_eq!(
            *world.get::<&PropertySource>(target).unwrap(),
            PropertySource::Explicit
        );
    }

    #[test]
    fn match_properties_layer_ref_copied_to_target() {
        let mut world = World::new();
        let mut history = History::new();

        let source = spawn_entity(&mut world, PropertySource::ByLayer, 7);
        let target = spawn_entity(&mut world, PropertySource::ByLayer, 0);

        match_properties(&mut world, source, &[target], &mut history);

        // Target should have source's LayerRef
        assert_eq!(world.get::<&LayerRef>(target).unwrap().0, 7);
    }

    #[test]
    fn match_properties_history_receives_transaction() {
        let mut world = World::new();
        let mut history = History::new();

        let source = spawn_entity(&mut world, PropertySource::Explicit, 5);
        let target = spawn_entity(&mut world, PropertySource::ByLayer, 0);

        match_properties(&mut world, source, &[target], &mut history);

        assert!(history.can_undo());
        assert_eq!(history.undo_label(), Some("Match Properties"));

        // Undo should restore target's original properties
        history.undo(&mut world);
        assert_eq!(
            *world.get::<&PropertySource>(target).unwrap(),
            PropertySource::ByLayer
        );
        assert_eq!(world.get::<&LayerRef>(target).unwrap().0, 0);
    }
}
