//! BlockRef, BlockInsert (stub)

use crate::geometry::Point2D;

/// A reference (insertion) of a block definition into the drawing.
///
/// Full implementation in Step 6.
#[derive(Debug, Clone, Copy)]
pub struct BlockRef {
    /// ID of the block definition this references.
    pub definition_id: u32,
    /// Insertion point in world coordinates.
    pub origin: Point2D,
}

/// Builder for creating a BlockRef insertion operation.
///
/// Full implementation in Step 6.
#[derive(Debug, Clone, Default)]
pub struct BlockInsert;
