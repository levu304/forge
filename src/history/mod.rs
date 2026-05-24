//! Undo/redo history.
//!
//! Provides a delta-based command journal with configurable depth.
//! Each `Transaction` is a list of `AtomicOp` variants. Undo replays
//! operations in reverse; redo applies them forward. Entity handle
//! remapping is managed via `EntityMapping`.

pub mod transaction;
pub mod ops;
