//! Selection system.
//!
//! Provides selection management (single-click, window select),
//! GPU picking (entity ID framebuffer readback), and the
//! SelectionManager which maintains a `HashSet<Entity>` + [`Selected`]
//! marker component.

pub mod picking;
pub mod window_select;

use crate::ecs::components::Selected;
use hecs::World;
use std::collections::HashSet;

/// Manages the current selection set.
///
/// Selection is represented by a `HashSet<Entity>` for O(1) membership tests,
/// and a [`Selected`] marker component on entities in the ECS world. Every
/// `select()` inserts the component; every `deselect()` removes it.
///
/// # Invariant
///
/// `selected.len() == query::<&Selected>().count()` — the `HashSet` and the
/// ECS marker component must always be in sync.
///
/// # Example
///
/// ```ignore
/// use hecs::World;
/// use forge::selection::SelectionManager;
///
/// let mut world = World::new();
/// let mut sel = SelectionManager::new();
/// let entity = world.spawn(());
///
/// sel.select(&mut world, entity);
/// assert_eq!(sel.count(), 1);
/// assert!(sel.is_selected(entity));
/// ```
pub struct SelectionManager {
    /// The set of currently selected entities (O(1) membership test).
    pub selected: HashSet<hecs::Entity>,
    /// The most-recently-selected entity, used by the property panel.
    ///
    /// Updated on every `select()` call. When the primary entity is
    /// deselected, falls back to the first remaining selected entity
    /// (in arbitrary iteration order), or `None` if the selection is empty.
    pub primary: Option<hecs::Entity>,
    /// Selection mode controlling how new selections interact with the
    /// current set.
    ///
    /// Only [`SelectionMode::Replace`] is active in v0.2.0. `Add` and
    /// `Remove` are reserved for future Ctrl+click modifiers (v0.3.0+).
    pub mode: SelectionMode,
}

/// Controls how new entity selections interact with the current selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SelectionMode {
    /// Each new selection replaces the entire current selection.
    #[default]
    Replace,
    /// *Future*: Ctrl+click adds to the selection (inactive in v0.2.0).
    Add,
    /// *Future*: Ctrl+click removes from the selection (inactive in v0.2.0).
    Remove,
}

impl SelectionManager {
    /// Creates an empty `SelectionManager`.
    pub fn new() -> Self {
        Self {
            selected: HashSet::new(),
            primary: None,
            mode: SelectionMode::Replace,
        }
    }

    /// Adds `entity` to the selection.
    ///
    /// Inserts the [`Selected`] marker component into the ECS world and
    /// updates [`primary`](Self::primary) to this entity. If the entity
    /// is already selected this is a no-op.
    pub fn select(&mut self, world: &mut World, entity: hecs::Entity) {
        if self.selected.insert(entity) {
            // `insert_one` silently succeeds or returns an error if the
            // entity has been despawned — we ignore that because the
            // HashSet entry is already added.
            world.insert_one(entity, Selected).ok();
            self.primary = Some(entity);
        }
    }

    /// Removes `entity` from the selection.
    ///
    /// Removes the [`Selected`] marker component from the ECS world.
    /// If `entity` is not currently selected this is a no-op. When the
    /// deselectecd entity was the [`primary`](Self::primary), falls back
    /// to the first remaining selected entity (arbitrary iteration order),
    /// or `None` if the set becomes empty.
    pub fn deselect(&mut self, world: &mut World, entity: hecs::Entity) {
        if self.selected.remove(&entity) {
            // Entity may have been despawned externally — use `.ok()` to
            // silently absorb errors.
            world.remove_one::<Selected>(entity).ok();
            if self.primary == Some(entity) {
                self.primary = self.selected.iter().next().copied();
            }
        }
    }

    /// Clears the entire selection.
    ///
    /// Removes the [`Selected`] component from every currently selected
    /// entity in the ECS world, empties the `HashSet`, and resets
    /// [`primary`](Self::primary) to `None`.
    pub fn clear(&mut self, world: &mut World) {
        for &entity in &self.selected {
            world.remove_one::<Selected>(entity).ok();
        }
        self.selected.clear();
        self.primary = None;
    }

    /// Replaces the entire selection with a single entity.
    ///
    /// Equivalent to [`clear()`](Self::clear) followed by
    /// [`select(entity)`](Self::select).
    pub fn replace(&mut self, world: &mut World, entity: hecs::Entity) {
        self.clear(world);
        self.select(world, entity);
    }

    /// Returns `true` if the selection is empty.
    pub fn is_empty(&self) -> bool {
        self.selected.is_empty()
    }

    /// Returns the number of selected entities.
    pub fn count(&self) -> usize {
        self.selected.len()
    }

    /// Returns `true` if `entity` is currently selected.
    pub fn is_selected(&self, entity: hecs::Entity) -> bool {
        self.selected.contains(&entity)
    }

    /// Handles the result of a GPU picking operation.
    ///
    /// * `Some(entity)` — replaces the selection with the picked entity.
    /// * `None` — clears the selection (the cursor did not hit any entity).
    pub fn handle_picking_result(&mut self, world: &mut World, entity: Option<hecs::Entity>) {
        match entity {
            Some(e) => self.replace(world, e),
            None => self.clear(world),
        }
    }
}

impl Default for SelectionManager {
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
    use hecs::World;

    // -- helpers -----------------------------------------------------------

    /// Returns the number of entities in `world` that carry the [`Selected`]
    /// marker component.
    fn selected_component_count(world: &World) -> usize {
        world.query::<&Selected>().iter().count()
    }

    /// Returns `true` if `entity` currently has the [`Selected`] component.
    fn has_selected_component(world: &World, entity: hecs::Entity) -> bool {
        world
            .query::<&Selected>()
            .iter()
            .any(|(e, _)| e == entity)
    }

    // -- select() ----------------------------------------------------------

    #[test]
    fn select_adds_entity_to_set_and_inserts_component() {
        let mut world = World::new();
        let mut mgr = SelectionManager::new();
        let entity = world.spawn(());

        mgr.select(&mut world, entity);

        assert!(mgr.selected.contains(&entity));
        assert!(has_selected_component(&world, entity));
        assert_eq!(mgr.primary, Some(entity));
        assert_eq!(mgr.count(), 1);
        assert_eq!(selected_component_count(&world), 1);
    }

    #[test]
    fn select_idempotent_same_entity_twice() {
        let mut world = World::new();
        let mut mgr = SelectionManager::new();
        let entity = world.spawn(());

        mgr.select(&mut world, entity);
        mgr.select(&mut world, entity); // second call — no-op

        assert_eq!(mgr.count(), 1);
        assert!(has_selected_component(&world, entity));
        assert_eq!(mgr.primary, Some(entity));
        assert_eq!(selected_component_count(&world), 1);
    }

    // -- deselect() --------------------------------------------------------

    #[test]
    fn deselect_removes_from_set_and_removes_component() {
        let mut world = World::new();
        let mut mgr = SelectionManager::new();
        let entity = world.spawn(());

        mgr.select(&mut world, entity);
        mgr.deselect(&mut world, entity);

        assert!(!mgr.selected.contains(&entity));
        assert!(!has_selected_component(&world, entity));
        assert!(mgr.is_empty());
        assert_eq!(selected_component_count(&world), 0);
    }

    #[test]
    fn deselect_non_selected_entity_is_noop() {
        let mut world = World::new();
        let mut mgr = SelectionManager::new();
        let entity = world.spawn(());

        // Should not panic or change state.
        mgr.deselect(&mut world, entity);

        assert!(mgr.is_empty());
        assert_eq!(mgr.primary, None);
    }

    // -- clear() -----------------------------------------------------------

    #[test]
    fn clear_removes_all_components_and_empties_set() {
        let mut world = World::new();
        let mut mgr = SelectionManager::new();
        let e1 = world.spawn(());
        let e2 = world.spawn(());

        mgr.select(&mut world, e1);
        mgr.select(&mut world, e2);
        assert_eq!(mgr.count(), 2);

        mgr.clear(&mut world);

        assert!(mgr.is_empty());
        assert_eq!(mgr.primary, None);
        assert_eq!(selected_component_count(&world), 0);
    }

    // -- replace() ---------------------------------------------------------

    #[test]
    fn replace_clears_existing_and_selects_new() {
        let mut world = World::new();
        let mut mgr = SelectionManager::new();
        let e1 = world.spawn(());
        let e2 = world.spawn(());

        mgr.select(&mut world, e1);
        mgr.replace(&mut world, e2);

        assert!(!mgr.selected.contains(&e1));
        assert!(!has_selected_component(&world, e1));
        assert!(mgr.selected.contains(&e2));
        assert!(has_selected_component(&world, e2));
        assert_eq!(mgr.count(), 1);
        assert_eq!(mgr.primary, Some(e2));
    }

    // -- is_empty / count / is_selected -----------------------------------

    #[test]
    fn empty_after_construction() {
        let mgr = SelectionManager::new();
        assert!(mgr.is_empty());
        assert_eq!(mgr.count(), 0);
    }

    #[test]
    fn is_selected_returns_true_for_selected_entity() {
        let mut world = World::new();
        let mut mgr = SelectionManager::new();
        let entity = world.spawn(());

        assert!(!mgr.is_selected(entity));
        mgr.select(&mut world, entity);
        assert!(mgr.is_selected(entity));
        mgr.deselect(&mut world, entity);
        assert!(!mgr.is_selected(entity));
    }

    // -- primary field ----------------------------------------------------

    #[test]
    fn primary_updates_to_latest_selected_entity() {
        let mut world = World::new();
        let mut mgr = SelectionManager::new();
        assert_eq!(mgr.primary, None);

        let e1 = world.spawn(());
        mgr.select(&mut world, e1);
        assert_eq!(mgr.primary, Some(e1));

        let e2 = world.spawn(());
        mgr.select(&mut world, e2);
        assert_eq!(mgr.primary, Some(e2));
    }

    #[test]
    fn primary_falls_back_when_primary_deselected() {
        let mut world = World::new();
        let mut mgr = SelectionManager::new();
        let e1 = world.spawn(());
        let e2 = world.spawn(());

        mgr.select(&mut world, e1);
        mgr.select(&mut world, e2); // primary → e2
        mgr.deselect(&mut world, e2);

        assert_eq!(mgr.primary, Some(e1));
    }

    #[test]
    fn primary_becomes_none_when_last_entity_deselected() {
        let mut world = World::new();
        let mut mgr = SelectionManager::new();
        let entity = world.spawn(());

        mgr.select(&mut world, entity);
        mgr.deselect(&mut world, entity);

        assert_eq!(mgr.primary, None);
    }

    // -- handle_picking_result() -------------------------------------------

    #[test]
    fn handle_picking_result_some_replaces_selection() {
        let mut world = World::new();
        let mut mgr = SelectionManager::new();
        let e1 = world.spawn(());
        let e2 = world.spawn(());

        mgr.select(&mut world, e1);
        mgr.handle_picking_result(&mut world, Some(e2));

        assert!(!mgr.selected.contains(&e1));
        assert!(mgr.selected.contains(&e2));
        assert_eq!(mgr.count(), 1);
        assert_eq!(mgr.primary, Some(e2));
    }

    #[test]
    fn handle_picking_result_none_clears_all() {
        let mut world = World::new();
        let mut mgr = SelectionManager::new();
        let e1 = world.spawn(());
        let e2 = world.spawn(());

        mgr.select(&mut world, e1);
        mgr.select(&mut world, e2);
        mgr.handle_picking_result(&mut world, None);

        assert!(mgr.is_empty());
        assert_eq!(mgr.primary, None);
        assert_eq!(selected_component_count(&world), 0);
    }

    // -- invariant ---------------------------------------------------------

    #[test]
    fn selected_set_and_component_count_match() {
        let mut world = World::new();
        let mut mgr = SelectionManager::new();
        let entities: Vec<_> = (0..5).map(|_| world.spawn(())).collect();

        for (i, &e) in entities.iter().enumerate() {
            mgr.select(&mut world, e);
            assert_eq!(
                mgr.count(),
                selected_component_count(&world),
                "after selecting {} entity(ies)",
                i + 1,
            );
        }

        for (i, &e) in entities.iter().enumerate() {
            mgr.deselect(&mut world, e);
            assert_eq!(
                mgr.count(),
                selected_component_count(&world),
                "after deselecting {} entity(ies)",
                i + 1,
            );
        }
    }
}
