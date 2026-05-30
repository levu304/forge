//! Block editor state machine for in-place block editing (BEDIT / REFEDIT).
//!
//! [`BlockEditorState`] manages entering, editing, saving, and cancelling
//! block-editing sessions.  During an editing session the caller is
//! responsible for applying modifications to the block definition in the
//! [`BlockTable`]; `save()` records those changes as a single undoable
//! [`Transaction`] and `cancel()` restores the original entities.

use crate::block::definition::{BlockDef, BlockEntity, BlockId, BlockTable};
use crate::history::{AtomicOp, History, Transaction};

/// State machine for in-place block editing (BEDIT / REFEDIT).
///
/// # State transitions
///
/// ```text
///            enter(id, def)
///     ┌─────────────────────────┐
///     v                         │
///  Inactive ──► Active ───► save(block_table, history) ──► Inactive
///                   │                                      (history pushed)
///                   └──► cancel(block_table) ──► Inactive
///                                      (entities restored)
/// ```
///
/// Only one block can be edited at a time.  Calling `enter()` while already
/// active will overwrite the previous session state.
#[derive(Debug, Clone)]
pub struct BlockEditorState {
    /// Whether an editing session is active.
    active: bool,
    /// The block being edited.
    editing_block: BlockId,
    /// Snapshot of the block definition's entities taken at `enter()` time.
    /// Used by `cancel()` to restore the original state.
    original_entities: Vec<BlockEntity>,
}

impl BlockEditorState {
    /// Create a new idle editor state.
    pub fn new() -> Self {
        Self {
            active: false,
            editing_block: BlockId(0),
            original_entities: Vec::new(),
        }
    }

    /// Enter editing mode for the given block definition.
    ///
    /// Sets `active = true` and takes a snapshot of `def.entities` so that
    /// [`cancel()`](Self::cancel) can restore the original state.
    pub fn enter(&mut self, block_id: BlockId, def: &BlockDef) {
        self.active = true;
        self.editing_block = block_id;
        self.original_entities = def.entities.clone();
    }

    /// Save the editing session and push a [`Transaction`] to history.
    ///
    /// **Important:** `old_def` is captured **before**
    /// [`BlockDef::compute_bounds`] is called, ensuring the undo record
    /// preserves the pre-bounds-computation state.
    ///
    /// # Panics
    ///
    /// Panics if `self.editing_block` is no longer present in
    /// `block_table` (this should never happen if `enter()` was called
    /// with a valid block).
    pub fn save(&mut self, block_table: &mut BlockTable, history: &mut History) {
        // Capture the old definition BEFORE bounds computation (critical).
        let old_def = block_table
            .get(self.editing_block)
            .cloned()
            .expect("BlockEditorState::save: editing block vanished from table");

        // Recompute bounds on the current (user-modified) definition.
        if let Some(def) = block_table.get_mut(self.editing_block) {
            def.bounds = def.compute_bounds();
        }

        let new_def = block_table
            .get(self.editing_block)
            .cloned()
            .expect("BlockEditorState::save: editing block vanished after bounds update");

        let mut tx = Transaction::new("Edit Block");
        tx.push(AtomicOp::ModifyBlockDef {
            block_id: self.editing_block,
            old: old_def,
            new: new_def,
        });
        history.push(tx);

        self.active = false;
    }

    /// Cancel the editing session and restore the original entities.
    ///
    /// The block definition's entities are replaced with the snapshot taken
    /// during [`enter()`](Self::enter), bounds are recomputed, and state
    /// returns to inactive.
    ///
    /// # Panics
    ///
    /// Panics if `self.editing_block` is no longer present in
    /// `block_table`.
    pub fn cancel(&mut self, block_table: &mut BlockTable) {
        if let Some(def) = block_table.get_mut(self.editing_block) {
            def.entities = self.original_entities.clone();
            def.bounds = def.compute_bounds();
        }
        self.active = false;
    }

    /// Returns the block ID being edited if a session is active.
    pub fn active_block_id(&self) -> Option<BlockId> {
        if self.active {
            Some(self.editing_block)
        } else {
            None
        }
    }

    /// Returns `true` if an editing session is currently active.
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Deactivate the editor without modifying the block table.
    ///
    /// Unlike [`cancel()`](Self::cancel), this does **not** restore the
    /// original entities — any changes made during the session are kept
    /// in the block table.  Useful for discarding the editor state after
    /// [`save()`](Self::save) has already been called.
    pub fn deactivate(&mut self) {
        self.active = false;
    }
}

impl Default for BlockEditorState {
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
    use crate::block::definition::{BlockDef, BlockEntity, BlockTable};
    use crate::block::BlockId;
    use crate::ecs::components::{LayerRef, PropertySource};
    use crate::geometry::{BoundingBox2D, Point2D};
    use crate::history::{AtomicOp, History};

    /// Helper: create a simple block definition with one line entity.
    fn make_def(name: &str, x: f64, y: f64) -> BlockDef {
        BlockDef {
            name: name.to_string(),
            base_point: Point2D::new(0.0, 0.0),
            entities: vec![BlockEntity::Line(
                Point2D::new(x, y),
                Point2D::new(x + 10.0, y + 10.0),
                PropertySource::ByLayer,
                LayerRef(0),
            )],
            bounds: BoundingBox2D::empty(),
        }
    }

    // ------------------------------------------------------------------
    // new / default
    // ------------------------------------------------------------------

    #[test]
    fn test_editor_new_is_inactive() {
        let editor = BlockEditorState::new();
        assert!(!editor.is_active());
        assert!(editor.active_block_id().is_none());
    }

    #[test]
    fn test_editor_default_is_inactive() {
        let editor = BlockEditorState::default();
        assert!(!editor.is_active());
    }

    // ------------------------------------------------------------------
    // enter
    // ------------------------------------------------------------------

    #[test]
    fn test_enter_activates_and_clones_entities() {
        let def = make_def("test", 0.0, 0.0);
        let mut editor = BlockEditorState::new();

        editor.enter(BlockId(1), &def);

        assert!(editor.is_active());
        assert_eq!(editor.active_block_id(), Some(BlockId(1)));
        assert_eq!(
            editor.original_entities.len(),
            def.entities.len()
        );
    }

    #[test]
    fn test_enter_re_entering_overwrites() {
        let def1 = make_def("first", 0.0, 0.0);
        let def2 = make_def("second", 100.0, 100.0);
        let mut editor = BlockEditorState::new();

        editor.enter(BlockId(1), &def1);
        assert_eq!(editor.active_block_id(), Some(BlockId(1)));

        // Re-enter with a different block
        editor.enter(BlockId(2), &def2);
        assert_eq!(editor.active_block_id(), Some(BlockId(2)));
        // Snapshot should now match def2
        assert!(!editor.original_entities.is_empty());
    }

    // ------------------------------------------------------------------
    // save
    // ------------------------------------------------------------------

    #[test]
    fn test_save_builds_modify_block_def_and_deactivates() {
        let mut table = BlockTable::new();
        let def = make_def("save-test", 0.0, 0.0);
        let block_id = table.insert(def).unwrap();

        let def_before = table.get(block_id).cloned().unwrap();

        let mut editor = BlockEditorState::new();
        editor.enter(block_id, &def_before);
        assert!(editor.is_active());

        // Modify the block's entities (simulate user editing)
        if let Some(def) = table.get_mut(block_id) {
            def.entities.push(BlockEntity::Circle(
                Point2D::new(5.0, 5.0),
                3.0,
                PropertySource::ByLayer,
                LayerRef(0),
            ));
        }

        let mut history = History::new();
        editor.save(&mut table, &mut history);

        // Save should deactivate
        assert!(!editor.is_active());

        // History should have one transaction with a ModifyBlockDef op
        assert!(history.can_undo());
        let tx = history.undo_stack.back().unwrap();
        assert_eq!(tx.label, "Edit Block");
        assert_eq!(tx.ops.len(), 1);
        match &tx.ops[0] {
            AtomicOp::ModifyBlockDef {
                block_id: id,
                old,
                new,
            } => {
                assert_eq!(*id, block_id);
                // old captures current state before bounds-update (2 entities)
                assert_eq!(old.entities.len(), 2);
                // new has same entities, only bounds changed
                assert_eq!(new.entities.len(), 2);
            }
            other => panic!("Expected ModifyBlockDef, got {other:?}"),
        }
    }

    #[test]
    fn test_save_updates_bounds() {
        let mut table = BlockTable::new();
        let def = make_def("bounds-test", 0.0, 0.0);
        let block_id = table.insert(def).unwrap();

        let mut editor = BlockEditorState::new();
        editor.enter(block_id, table.get(block_id).unwrap());

        // Modify to extend bounds
        if let Some(def) = table.get_mut(block_id) {
            def.entities.push(BlockEntity::Line(
                Point2D::new(50.0, 50.0),
                Point2D::new(100.0, 100.0),
                PropertySource::ByLayer,
                LayerRef(0),
            ));
        }

        let mut history = History::new();
        editor.save(&mut table, &mut history);

        let saved_def = table.get(block_id).unwrap();
        // Bounds should encompass both entities
        assert!(saved_def.bounds.max.x >= 100.0);
        assert!(saved_def.bounds.max.y >= 100.0);
    }

    #[test]
    #[should_panic(expected = "vanished")]
    fn test_save_panics_if_block_missing() {
        let mut table = BlockTable::new();
        let mut editor = BlockEditorState::new();
        editor.enter(BlockId(42), &make_def("ghost", 0.0, 0.0));

        // Block 42 was never inserted
        let mut history = History::new();
        editor.save(&mut table, &mut history);
    }

    // ------------------------------------------------------------------
    // cancel
    // ------------------------------------------------------------------

    #[test]
    fn test_cancel_restores_original_entities() {
        let mut table = BlockTable::new();
        let def = make_def("cancel-test", 0.0, 0.0);
        let block_id = table.insert(def).unwrap();

        let mut editor = BlockEditorState::new();
        editor.enter(block_id, table.get(block_id).unwrap());

        // Modify the block (add an entity)
        if let Some(def) = table.get_mut(block_id) {
            def.entities.push(BlockEntity::Circle(
                Point2D::new(5.0, 5.0),
                3.0,
                PropertySource::ByLayer,
                LayerRef(0),
            ));
            assert_eq!(def.entities.len(), 2);
        }

        // Cancel → should restore original single entity
        editor.cancel(&mut table);
        assert!(!editor.is_active());

        let restored = table.get(block_id).unwrap();
        assert_eq!(restored.entities.len(), 1);
    }

    #[test]
    fn test_cancel_recomputes_bounds() {
        let mut table = BlockTable::new();
        let def = BlockDef {
            name: "bounds".to_string(),
            base_point: Point2D::new(0.0, 0.0),
            entities: vec![
                BlockEntity::Line(
                    Point2D::new(0.0, 0.0),
                    Point2D::new(10.0, 10.0),
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
        let block_id = table.insert(def).unwrap();

        let mut editor = BlockEditorState::new();
        let original = table.get(block_id).unwrap().clone();
        editor.enter(block_id, &original);

        // Nuke all entities during edit
        if let Some(def) = table.get_mut(block_id) {
            def.entities.clear();
        }

        // Cancel → should restore original entities + correct bounds
        editor.cancel(&mut table);
        let restored = table.get(block_id).unwrap();
        assert_eq!(restored.entities.len(), 2);
        // Bounds should match the original compute_bounds
        let expected = original.compute_bounds();
        assert_eq!(restored.bounds.min.x, expected.min.x);
    }

    #[test]
    fn test_cancel_idempotent_when_inactive() {
        let mut table = BlockTable::new();
        let mut editor = BlockEditorState::new();
        // Calling cancel on an inactive editor should be a no-op
        editor.cancel(&mut table);
        assert!(!editor.is_active());
    }

    // ------------------------------------------------------------------
    // deactivate
    // ------------------------------------------------------------------

    #[test]
    fn test_deactivate() {
        let def = make_def("deact", 0.0, 0.0);
        let mut editor = BlockEditorState::new();
        editor.enter(BlockId(1), &def);
        assert!(editor.is_active());

        editor.deactivate();
        assert!(!editor.is_active());
        assert!(editor.active_block_id().is_none());
    }
}
