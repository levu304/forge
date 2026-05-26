//! ERASE command implementation.
//!
//! Removes selected entities from the drawing. Captures entity data before
//! despawning and builds a [`Transaction`] with `Despawn*` ops for the
//! history system.
//!
//! # State Machine
//!
//! Single-step. On `Confirm`, the command despawns all entities that were
//! selected at construction time and stores the resulting transaction for
//! the caller to take via [`take_transaction`](EraseCommand::take_transaction).

use super::{Command, CommandInput, CommandResult, PreviewEntity};
use crate::ecs::components::*;
use crate::history::{AtomicOp, Transaction};
use crate::selection::SelectionManager;
use hecs::World;

/// Removes selected entities from the drawing.
///
/// Captures the selection at construction time. On `Confirm`, reads each
/// entity's component data, builds a `Despawn*` transaction, then despawns
/// the entities from the ECS world.
pub struct EraseCommand {
    /// Entities to erase (captured at construction time).
    selected_entities: Vec<hecs::Entity>,
    /// Transaction populated on `Confirm`, consumed by caller via
    /// [`take_transaction`](EraseCommand::take_transaction).
    pending_transaction: Option<Transaction>,
}

impl EraseCommand {
    /// Create a new ERASE command, capturing the current selection.
    ///
    /// The command will be empty if nothing was selected. Callers should
    /// check [`is_empty`](EraseCommand::is_empty) before activating.
    pub fn new(selection: &SelectionManager) -> Self {
        Self {
            selected_entities: selection.selected.iter().copied().collect(),
            pending_transaction: None,
        }
    }

    /// Returns `true` if no entities were captured (empty selection).
    pub fn is_empty(&self) -> bool {
        self.selected_entities.is_empty()
    }

    /// Consume the pending transaction after command completion.
    ///
    /// Returns `None` if the command has not completed yet or if
    /// it completed with an empty transaction.
    pub fn take_transaction(&mut self) -> Option<Transaction> {
        self.pending_transaction.take()
    }
}

impl Command for EraseCommand {
    fn name(&self) -> &'static str {
        "ERASE"
    }

    fn prompt(&self) -> String {
        "Select objects to erase:".to_string()
    }

    fn steps_remaining(&self) -> usize {
        1
    }

    fn on_input(&mut self, input: CommandInput, world: &mut World) -> CommandResult {
        match input {
            CommandInput::Confirm => {
                if self.selected_entities.is_empty() {
                    return CommandResult::Error(
                        "No entities selected. Select objects before running ERASE.".to_string(),
                    );
                }

                let mut tx = Transaction::new(format!(
                    "Erase {} entit{}",
                    self.selected_entities.len(),
                    if self.selected_entities.len() == 1 {
                        "y"
                    } else {
                        "ies"
                    },
                ));

                // Read entity data BEFORE despawning, then build Despawn ops.
                for &entity in &self.selected_entities {
                    // Check each component type in order.
                    // `world.get` returns Err if entity was already despawned or
                    // has a different type — we silently skip those.
                    if let Ok(data) = world.get::<&LineData>(entity) {
                        tx.push(AtomicOp::DespawnLine {
                            entity,
                            data: *data,
                        });
                    } else if let Ok(data) = world.get::<&CircleData>(entity) {
                        tx.push(AtomicOp::DespawnCircle {
                            entity,
                            data: *data,
                        });
                    } else if let Ok(data) = world.get::<&ArcData>(entity) {
                        tx.push(AtomicOp::DespawnArc {
                            entity,
                            data: *data,
                        });
                    } else if let Ok(data) = world.get::<&PolylineData>(entity) {
                        tx.push(AtomicOp::DespawnPolyline {
                            entity,
                            data: (&*data).clone(),
                        });
                    }
                    // Skip entities with no matching geometry type.
                }

                // Now despawn all entities from the ECS world.
                for &entity in &self.selected_entities {
                    world.despawn(entity).ok();
                }

                self.pending_transaction = Some(tx);
                CommandResult::Complete
            }
            CommandInput::Cancel => {
                self.on_cancel(world);
                CommandResult::Cancelled
            }
            _ => CommandResult::Error(
                "Press Enter to erase selected objects, or Esc to cancel.".to_string(),
            ),
        }
    }

    fn on_cancel(&mut self, _world: &mut World) {
        self.pending_transaction = None;
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
    use crate::geometry::Point2D;
    use crate::selection::SelectionManager;
    use crate::util::Color;
    use hecs::World;

    // -- helpers -----------------------------------------------------------

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

    fn make_circle(world: &mut World) -> hecs::Entity {
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

    fn make_arc(world: &mut World) -> hecs::Entity {
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

    fn make_polyline(world: &mut World) -> hecs::Entity {
        world.spawn((
            PolylineData {
                vertices: vec![
                    Point2D::new(0.0, 0.0),
                    Point2D::new(5.0, 5.0),
                    Point2D::new(10.0, 0.0),
                ],
                closed: false,
                color: Color::WHITE,
                width: 1.0,
            },
            Renderable,
        ))
    }

    fn count_entities(world: &World) -> usize {
        world
            .query::<&Renderable>()
            .iter()
            .count()
    }

    // -- construction ------------------------------------------------------

    #[test]
    fn new_captures_selection() {
        let mut world = World::new();
        let e1 = make_line(&mut world);
        let e2 = make_line(&mut world);

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e1);
        sel.select(&mut world, e2);

        let cmd = EraseCommand::new(&sel);
        assert_eq!(cmd.selected_entities.len(), 2);
        assert!(!cmd.is_empty());
    }

    #[test]
    fn new_with_empty_selection_is_empty() {
        let sel = SelectionManager::new();
        let cmd = EraseCommand::new(&sel);
        assert!(cmd.is_empty());
        assert_eq!(cmd.selected_entities.len(), 0);
    }

    #[test]
    fn name_and_prompt() {
        let sel = SelectionManager::new();
        let cmd = EraseCommand::new(&sel);
        assert_eq!(cmd.name(), "ERASE");
        assert_eq!(cmd.prompt(), "Select objects to erase:");
        assert_eq!(cmd.steps_remaining(), 1);
    }

    // -- confirm with selection --------------------------------------------

    #[test]
    fn erase_confirmed_with_selection_builds_transaction() {
        let mut world = World::new();
        let e1 = make_line(&mut world);
        let e2 = make_circle(&mut world);

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e1);
        sel.select(&mut world, e2);

        let mut cmd = EraseCommand::new(&sel);
        let result = cmd.on_input(CommandInput::Confirm, &mut world);

        assert!(matches!(result, CommandResult::Complete));

        let tx = cmd.take_transaction();
        assert!(tx.is_some());
        let tx = tx.unwrap();
        assert_eq!(tx.ops.len(), 2, "should have 2 Despawn ops");

        // Verify the transaction contains correct Despawn variants
        let mut line_found = false;
        let mut circle_found = false;
        for op in &tx.ops {
            match op {
                AtomicOp::DespawnLine { entity, .. } => {
                    assert_eq!(*entity, e1);
                    line_found = true;
                }
                AtomicOp::DespawnCircle { entity, .. } => {
                    assert_eq!(*entity, e2);
                    circle_found = true;
                }
                _ => panic!("unexpected op variant: {op:?}"),
            }
        }
        assert!(line_found, "should contain DespawnLine for e1");
        assert!(circle_found, "should contain DespawnCircle for e2");
    }

    #[test]
    fn erase_confirmed_with_selection_despawns_entities() {
        let mut world = World::new();
        let e1 = make_line(&mut world);
        let e2 = make_circle(&mut world);

        assert_eq!(count_entities(&world), 2);

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e1);
        sel.select(&mut world, e2);

        let mut cmd = EraseCommand::new(&sel);
        let _ = cmd.on_input(CommandInput::Confirm, &mut world);

        assert_eq!(count_entities(&world), 0, "all entities should be despawned");
        assert!(world.get::<&LineData>(e1).is_err(), "e1 should no longer exist");
        assert!(world.get::<&CircleData>(e2).is_err(), "e2 should no longer exist");
    }

    #[test]
    fn erase_handles_mixed_selection() {
        let mut world = World::new();
        let line = make_line(&mut world);
        let circle = make_circle(&mut world);
        let arc = make_arc(&mut world);
        let poly = make_polyline(&mut world);

        let mut sel = SelectionManager::new();
        sel.select(&mut world, line);
        sel.select(&mut world, circle);
        sel.select(&mut world, arc);
        sel.select(&mut world, poly);

        let mut cmd = EraseCommand::new(&sel);
        let result = cmd.on_input(CommandInput::Confirm, &mut world);

        assert!(matches!(result, CommandResult::Complete));
        assert_eq!(count_entities(&world), 0);

        let tx = cmd.take_transaction().unwrap();
        assert_eq!(tx.ops.len(), 4);

        // Check we got all 4 Despawn types
        let mut variants = std::collections::HashSet::new();
        for op in &tx.ops {
            match op {
                AtomicOp::DespawnLine { .. } => { variants.insert("DespawnLine"); }
                AtomicOp::DespawnCircle { .. } => { variants.insert("DespawnCircle"); }
                AtomicOp::DespawnArc { .. } => { variants.insert("DespawnArc"); }
                AtomicOp::DespawnPolyline { .. } => { variants.insert("DespawnPolyline"); }
                _ => {}
            }
        }
        assert_eq!(variants.len(), 4, "should have one of each Despawn type");
    }

    // -- empty selection ---------------------------------------------------

    #[test]
    fn erase_with_empty_selection_returns_error() {
        let mut world = World::new();
        let sel = SelectionManager::new();

        let mut cmd = EraseCommand::new(&sel);
        let result = cmd.on_input(CommandInput::Confirm, &mut world);

        assert!(matches!(result, CommandResult::Error(_)));
        assert!(cmd.take_transaction().is_none(),
                "no transaction should be created on error");
    }

    // -- cancel ------------------------------------------------------------

    #[test]
    fn erase_cancel_does_nothing() {
        let mut world = World::new();
        let e = make_line(&mut world);

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let mut cmd = EraseCommand::new(&sel);
        let result = cmd.on_input(CommandInput::Cancel, &mut world);

        assert!(matches!(result, CommandResult::Cancelled));
        assert!(count_entities(&world) > 0, "entities should still exist after cancel");
        assert!(cmd.take_transaction().is_none(),
                "no transaction after cancel");
    }

    #[test]
    fn erase_take_transaction_consumes() {
        let mut world = World::new();
        let e = make_line(&mut world);

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let mut cmd = EraseCommand::new(&sel);
        let _ = cmd.on_input(CommandInput::Confirm, &mut world);

        assert!(cmd.take_transaction().is_some(), "first take returns transaction");
        assert!(cmd.take_transaction().is_none(), "second take returns None (consumed)");
    }

    // -- invalid input -----------------------------------------------------

    #[test]
    fn erase_text_input_returns_error() {
        let mut world = World::new();
        let e = make_line(&mut world);

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e);

        let mut cmd = EraseCommand::new(&sel);
        let result = cmd.on_input(CommandInput::Text("hello".to_string()), &mut world);
        assert!(matches!(result, CommandResult::Error(_)));
    }

    #[test]
    fn erase_skip_already_despawned_entity() {
        let mut world = World::new();
        let e1 = make_line(&mut world);
        let e2 = make_line(&mut world);

        // Despawn e1 before erase runs
        world.despawn(e1).ok();

        let mut sel = SelectionManager::new();
        sel.select(&mut world, e1); // stale handle — but SelectionManager won't insert it
        sel.select(&mut world, e2);

        let mut cmd = EraseCommand::new(&sel);
        // Note: e1 was not inserted because select checks world.insert_one first.
        // So only e2 is in selected_entities.
        assert_eq!(cmd.selected_entities.len(), 1,
                   "despawned entity should not be in captured selection");

        let result = cmd.on_input(CommandInput::Confirm, &mut world);
        assert!(matches!(result, CommandResult::Complete));
        assert_eq!(count_entities(&world), 0);
    }
}
