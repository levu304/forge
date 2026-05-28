//! BlockDef, BlockEntity (stub)

/// A block definition — a named collection of entities that can be
/// instanced as BlockRefs throughout the drawing.
///
/// Full implementation in Step 6.
#[derive(Debug, Clone, Default)]
pub struct BlockDef {
    /// Unique identifier for this block definition.
    pub id: u32,
    /// Human-readable block name.
    pub name: String,
}

/// A single entity instance within a block definition.
///
/// Full implementation in Step 6.
#[derive(Debug, Clone, Default)]
pub struct BlockEntity;
