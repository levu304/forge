//! Layer error types.

use thiserror::Error;

/// Errors returned by [`LayerTable`](super::table::LayerTable) operations.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum LayerError {
    /// The requested layer ID does not exist in the table.
    #[error("layer not found: {id}")]
    NotFound { id: u32 },

    /// A layer with the given name already exists.
    #[error("layer '{name}' already exists")]
    DuplicateName { name: String },

    /// The caller attempted to delete the immutable default layer (ID 0).
    #[error("cannot delete the default layer")]
    CannotDeleteDefault,

    /// The layer still has active references and cannot be deleted.
    #[error("layer has {count} references and cannot be deleted")]
    HasReferences { count: usize },

    /// The caller attempted to rename the immutable default layer (ID 0).
    #[error("cannot rename the default layer")]
    CannotRenameDefault,
}
