//! Layer storage table — [`LayerTable`] manages all layers by ID and name.

use std::collections::HashMap;

use tracing::info;

use super::error::LayerError;
use super::types::{Layer, LayerId};

/// A table of all layers in the drawing, indexed by ID.
///
/// Provides CRUD operations, a name → ID lookup index, and an active-layer
/// cursor. The default layer (ID 0, name `"0"`) is always present and cannot
/// be deleted or renamed.
#[derive(Debug, Clone)]
pub struct LayerTable {
    layers: HashMap<LayerId, Layer>,
    name_map: HashMap<String, LayerId>,
    next_id: u32,
    active_layer: LayerId,
}

impl LayerTable {
    /// Create a new table containing only the default layer (ID 0, name `"0"`).
    pub fn new() -> Self {
        let default_id = LayerId::DEFAULT;
        let default_name = String::from("0");

        let mut layers = HashMap::new();
        let mut name_map = HashMap::new();

        let default = Layer::new(default_id, &default_name);

        layers.insert(default_id, default);
        name_map.insert(default_name, default_id);

        info!("created layer table with default layer");

        Self {
            layers,
            name_map,
            next_id: 1,
            active_layer: default_id,
        }
    }

    /// Insert a new layer with the given name.
    ///
    /// Returns an error if the name is already taken. The new layer receives
    /// an auto-incremented ID and default visual properties (solid linetype,
    /// white colour, 0.25 linewidth, visible, unlocked, thawed).
    pub fn insert(&mut self, name: &str) -> Result<LayerId, LayerError> {
        if self.name_map.contains_key(name) {
            return Err(LayerError::DuplicateName {
                name: name.to_owned(),
            });
        }

        let id = LayerId(self.next_id);
        self.next_id += 1;

        let layer = Layer::new(id, name);

        self.name_map.insert(name.to_owned(), id);
        self.layers.insert(id, layer);

        info!(id = id.0, name = %name, "inserted layer");

        Ok(id)
    }

    /// Delete a layer by ID.
    ///
    /// The default layer (ID 0) cannot be deleted. If `number_of_references`
    /// is greater than zero the deletion is rejected with
    /// [`LayerError::HasReferences`].
    pub(crate) fn delete(
        &mut self,
        id: LayerId,
        number_of_references: usize,
    ) -> Result<(), LayerError> {
        if id == LayerId::DEFAULT {
            return Err(LayerError::CannotDeleteDefault);
        }

        if !self.layers.contains_key(&id) {
            return Err(LayerError::NotFound { id: id.0 });
        }

        if number_of_references > 0 {
            return Err(LayerError::HasReferences {
                count: number_of_references,
            });
        }

        let removed = self
            .layers
            .remove(&id)
            .expect("layer existence verified above");
        self.name_map.remove(removed.name());

        // If the active layer was deleted, reset to default.
        if self.active_layer == id {
            self.active_layer = LayerId::DEFAULT;
        }

        info!(id = id.0, name = %removed.name, "deleted layer");

        Ok(())
    }

    /// Rename an existing layer.
    ///
    /// The default layer (ID 0) cannot be renamed. Returns an error if the
    /// new name is already in use by a different layer.
    pub fn rename(&mut self, id: LayerId, new_name: &str) -> Result<(), LayerError> {
        if id == LayerId::DEFAULT {
            return Err(LayerError::CannotRenameDefault);
        }

        let layer = self
            .layers
            .get_mut(&id)
            .ok_or(LayerError::NotFound { id: id.0 })?;

        if new_name != layer.name() && self.name_map.contains_key(new_name) {
            return Err(LayerError::DuplicateName {
                name: new_name.to_owned(),
            });
        }

        self.name_map.remove(layer.name());
        self.name_map.insert(new_name.to_owned(), id);
        layer.set_name(new_name.to_owned());

        info!(id = id.0, name = %new_name, "renamed layer");

        Ok(())
    }

    /// Borrow a layer by ID.
    pub fn get(&self, id: LayerId) -> Option<&Layer> {
        self.layers.get(&id)
    }

    /// Look up a layer by name.
    pub fn get_by_name(&self, name: &str) -> Option<&Layer> {
        let id = self.name_map.get(name)?;
        self.layers.get(id)
    }

    /// Mutably borrow a layer by ID.
    pub fn get_mut(&mut self, id: LayerId) -> Option<&mut Layer> {
        self.layers.get_mut(&id)
    }

    /// Set the active (current) layer.
    ///
    /// Returns [`LayerError::NotFound`] if the ID does not exist.
    pub fn set_active(&mut self, id: LayerId) -> Result<(), LayerError> {
        if !self.layers.contains_key(&id) {
            return Err(LayerError::NotFound { id: id.0 });
        }
        self.active_layer = id;
        Ok(())
    }

    /// Return the ID of the currently active layer.
    pub fn active(&self) -> LayerId {
        self.active_layer
    }

    /// Iterate over all layers sorted by ascending ID.
    pub fn iter(&self) -> impl Iterator<Item = &Layer> {
        let mut ids: Vec<&LayerId> = self.layers.keys().collect();
        ids.sort_by_key(|a| a.0);
        ids.into_iter().map(|id| &self.layers[id])
    }

    /// Number of layers in the table (always ≥ 1).
    pub fn len(&self) -> usize {
        self.layers.len()
    }

    /// Returns `true` if the table is empty (should not happen in normal use).
    pub fn is_empty(&self) -> bool {
        self.layers.is_empty()
    }

    /// Returns `true` if the given layer ID exists in the table.
    pub fn contains(&self, id: LayerId) -> bool {
        self.layers.contains_key(&id)
    }

    /// Returns `true` if a layer with the given name exists.
    pub fn contains_name(&self, name: &str) -> bool {
        self.name_map.contains_key(name)
    }

    /// Collect all layer names into a vector.
    pub fn all_names(&self) -> Vec<String> {
        self.name_map.keys().cloned().collect()
    }

    /// Returns `true` if the given ID is the default layer (ID 0).
    pub fn is_default(&self, id: LayerId) -> bool {
        id == LayerId::DEFAULT
    }
}

impl Default for LayerTable {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layer::Linetype;
    use crate::util::Color;

    #[test]
    fn new_creates_default_layer() {
        let table = LayerTable::new();
        assert_eq!(table.len(), 1);
        let def = table.get(LayerId::DEFAULT).unwrap();
        assert_eq!(def.name(), "0");
        assert_eq!(def.color, Color::WHITE);
        assert_eq!(def.linetype, Linetype::Solid);
    }

    #[test]
    fn insert_rejects_duplicate_names() {
        let mut table = LayerTable::new();
        table.insert("walls").unwrap();
        let err = table.insert("walls").unwrap_err();
        assert_eq!(
            err,
            LayerError::DuplicateName {
                name: "walls".to_owned()
            }
        );
    }

    #[test]
    fn insert_assigns_sequential_ids() {
        let mut table = LayerTable::new();
        let a = table.insert("A").unwrap();
        let b = table.insert("B").unwrap();
        let c = table.insert("C").unwrap();
        assert_eq!(a.0, 1);
        assert_eq!(b.0, 2);
        assert_eq!(c.0, 3);
    }

    #[test]
    fn delete_refuses_default_layer() {
        let mut table = LayerTable::new();
        let err = table.delete(LayerId::DEFAULT, 0).unwrap_err();
        assert_eq!(err, LayerError::CannotDeleteDefault);
    }

    #[test]
    fn delete_with_references_refuses() {
        let mut table = LayerTable::new();
        let id = table.insert("walls").unwrap();
        let err = table.delete(id, 3).unwrap_err();
        assert_eq!(
            err,
            LayerError::HasReferences { count: 3 }
        );
    }

    #[test]
    fn delete_with_zero_references_succeeds() {
        let mut table = LayerTable::new();
        let id = table.insert("walls").unwrap();
        table.delete(id, 0).unwrap();
        assert!(!table.contains(id));
        assert!(!table.contains_name("walls"));
    }

    #[test]
    fn delete_resets_active_layer_to_default() {
        let mut table = LayerTable::new();
        let id = table.insert("tmp").unwrap();
        table.set_active(id).unwrap();
        table.delete(id, 0).unwrap();
        assert_eq!(table.active(), LayerId::DEFAULT);
    }

    #[test]
    fn rename_updates_name_map() {
        let mut table = LayerTable::new();
        let id = table.insert("old").unwrap();
        table.rename(id, "new").unwrap();
        assert!(table.contains_name("new"));
        assert!(!table.contains_name("old"));
        assert_eq!(table.get(id).unwrap().name, "new");
    }

    #[test]
    fn rename_duplicate_rejected() {
        let mut table = LayerTable::new();
        let a = table.insert("A").unwrap();
        table.insert("B").unwrap();
        let err = table.rename(a, "B").unwrap_err();
        assert_eq!(
            err,
            LayerError::DuplicateName {
                name: "B".to_owned()
            }
        );
    }

    #[test]
    fn rename_default_layer_rejected() {
        let mut table = LayerTable::new();
        let err = table.rename(LayerId::DEFAULT, "anything").unwrap_err();
        assert_eq!(err, LayerError::CannotRenameDefault);
    }

    #[test]
    fn rename_nonexistent_layer() {
        let mut table = LayerTable::new();
        let err = table.rename(LayerId(999), "foo").unwrap_err();
        assert_eq!(err, LayerError::NotFound { id: 999 });
    }

    #[test]
    fn get_round_trip() {
        let mut table = LayerTable::new();
        let id = table.insert("walls").unwrap();
        let by_id = table.get(id).unwrap();
        let by_name = table.get_by_name("walls").unwrap();
        assert_eq!(by_id.name(), "walls");
        assert_eq!(by_name.id(), id);
    }

    #[test]
    fn get_mut_allows_modification() {
        let mut table = LayerTable::new();
        let id = table.insert("walls").unwrap();
        let layer = table.get_mut(id).unwrap();
        layer.linewidth = 1.0;
        assert!((table.get(id).unwrap().linewidth - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn set_active_validates_layer_exists() {
        let mut table = LayerTable::new();
        let err = table.set_active(LayerId(999)).unwrap_err();
        assert_eq!(err, LayerError::NotFound { id: 999 });
    }

    #[test]
    fn set_active_works() {
        let mut table = LayerTable::new();
        let id = table.insert("walls").unwrap();
        table.set_active(id).unwrap();
        assert_eq!(table.active(), id);
    }

    #[test]
    fn iter_returns_sorted_by_id() {
        let mut table = LayerTable::new();
        // Insert in non-sequential call order.
        let _ = table.insert("C").unwrap(); // id 1
        let _ = table.insert("B").unwrap(); // id 2
        let _ = table.insert("A").unwrap(); // id 3

        let names: Vec<&str> = table.iter().map(|l| l.name.as_str()).collect();
        // Default layer "0" comes first, then by id.0 ascending.
        assert_eq!(names, vec!["0", "C", "B", "A"]);
    }

    #[test]
    fn contains_checks() {
        let mut table = LayerTable::new();
        let id = table.insert("walls").unwrap();
        assert!(table.contains(id));
        assert!(table.contains_name("walls"));
        assert!(!table.contains(LayerId(999)));
        assert!(!table.contains_name("nope"));
    }

    #[test]
    fn all_names_returns_all() {
        let mut table = LayerTable::new();
        table.insert("A").unwrap();
        table.insert("B").unwrap();
        let mut names = table.all_names();
        names.sort();
        assert_eq!(names, vec!["0", "A", "B"]);
    }

    #[test]
    fn is_default_checks() {
        let table = LayerTable::new();
        assert!(table.is_default(LayerId(0)));
        assert!(!table.is_default(LayerId(1)));
    }

    #[test]
    fn delete_nonexistent_layer() {
        let mut table = LayerTable::new();
        let err = table.delete(LayerId(999), 0).unwrap_err();
        assert_eq!(err, LayerError::NotFound { id: 999 });
    }
}
