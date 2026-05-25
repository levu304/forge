//! Transaction type.
//!
//! A [`Transaction`] bundles a list of [`AtomicOp`]s with a human-readable
//! label (e.g. "Move 3 entities"). It is pushed onto the history stack
//! on command completion and replayed in reverse on undo.

use crate::history::ops::AtomicOp;

/// A named transaction containing a list of atomic undo/redo operations.
///
/// Each `Transaction` has a human-readable label and a sequence of
/// [`AtomicOp`]s that can be replayed in reverse to undo or forward to redo.
///
/// # Examples
///
/// ```ignore
/// let mut tx = Transaction::new("Move 1 entity");
/// tx.push(AtomicOp::SetPosition { entity, old: pos1, new: pos2 });
/// assert!(!tx.is_empty());
/// ```
#[derive(Debug, Clone)]
pub struct Transaction {
    /// Human-readable description (e.g. "Erase 2 entities").
    pub label: String,
    /// Ordered list of atomic operations.
    pub ops: Vec<AtomicOp>,
}

impl Transaction {
    /// Create a new empty transaction with the given label.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            ops: Vec::new(),
        }
    }

    /// Append an atomic operation to this transaction.
    pub fn push(&mut self, op: AtomicOp) {
        self.ops.push(op);
    }

    /// Returns `true` if this transaction contains no operations.
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_transaction_is_empty() {
        let tx = Transaction::new("test");
        assert!(tx.is_empty());
        assert_eq!(tx.label, "test");
    }

    #[test]
    fn push_makes_transaction_non_empty() {
        let mut tx = Transaction::new("test");

        // Create a minimal dummy entity for the op.
        let entity = hecs::Entity::from_bits(1u64 << 32 | 1).unwrap();
        let old = crate::ecs::components::Position(crate::geometry::Point2D::new(0.0, 0.0));
        let new = crate::ecs::components::Position(crate::geometry::Point2D::new(10.0, 10.0));

        tx.push(AtomicOp::SetPosition {
            entity,
            old,
            new,
        });

        assert!(!tx.is_empty());
        assert_eq!(tx.ops.len(), 1);
    }

    #[test]
    fn transaction_clone() {
        let mut tx = Transaction::new("clone test");
        let entity = hecs::Entity::from_bits(1u64 << 32 | 2).unwrap();
        let pos = crate::ecs::components::Position(crate::geometry::Point2D::new(0.0, 0.0));
        tx.push(AtomicOp::SetPosition {
            entity,
            old: pos,
            new: pos,
        });

        let cloned = tx.clone();
        assert_eq!(cloned.label, tx.label);
        assert_eq!(cloned.ops.len(), tx.ops.len());
    }
}
