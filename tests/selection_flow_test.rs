//! Integration test: selection flow.
//!
//! Spawns entities, simulates selection via SelectionManager, runs ERASE
//! command, verifies undo restores them.

use forge::commands::erase_cmd::EraseCommand;
use forge::commands::{Command, CommandInput, CommandResult};
use forge::ecs::components::{LineData, Renderable, Selected};
use forge::geometry::Point2D;
use forge::history::History;
use forge::selection::SelectionManager;
use forge::util::Color;
use hecs::World;

fn make_line(world: &mut World) -> hecs::Entity {
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

/// Returns the count of entities with a `Renderable` component.
fn count_entities(world: &World) -> usize {
    world.query::<&Renderable>().iter().count()
}

#[test]
fn selection_flow_select_erase_undo() {
    let mut world = World::new();
    let mut sel = SelectionManager::new();
    let mut history = History::new();

    // Spawn two entities.
    let e1 = make_line(&mut world);
    let e2 = make_line(&mut world);
    assert_eq!(count_entities(&world), 2);

    // Select e1.
    sel.select(&mut world, e1);
    assert!(sel.is_selected(e1));
    assert!(world.get::<&Selected>(e1).is_ok());
    assert_eq!(sel.count(), 1);

    // Run ERASE command.
    let mut cmd = EraseCommand::new(&sel);
    let result = cmd.on_input(CommandInput::Confirm, &mut world);
    assert!(matches!(result, CommandResult::Complete));
    let tx = cmd.take_transaction().expect("transaction should exist");
    history.push(tx);

    // e1 should be despawned, e2 should still exist.
    assert!(world.get::<&LineData>(e1).is_err(), "e1 should be erased");
    assert!(world.get::<&LineData>(e2).is_ok(), "e2 should still exist");
    assert_eq!(count_entities(&world), 1);

    // Undo — e1 should be restored.
    let label = history.undo(&mut world);
    assert!(label.is_some());
    let mapping = history.take_entity_mapping();
    let remapped_e1 = mapping.map(e1);
    assert_ne!(remapped_e1, e1, "re-spawned entity gets a new handle");
    assert!(
        world.get::<&LineData>(remapped_e1).is_ok(),
        "e1 should be restored after undo"
    );
    assert_eq!(count_entities(&world), 2);
}
