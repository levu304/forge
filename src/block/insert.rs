//! Block instance types: BlockRef (ECS component).
//!
//! A `BlockRef` component on an ECS entity indicates that the entity is an
//! instance (insertion) of a block definition.  The block's geometry is
//! resolved at render time by reading the definition from `BlockTable` and
//! applying this transform.

use crate::block::definition::BlockId;
use crate::geometry::Transform2D;

/// ECS component: this entity is an instance of a block definition.
///
/// The block's geometry is stored in the [`BlockTable`](crate::block::BlockTable)
/// and referenced by `definition`.  At render time the geometry is transformed
/// from local definition space to world space using `transform`.
///
/// ## Compatibility note (v0.3.0)
///
/// Entities with a `BlockRef` component also carry a
/// [`Position`](crate::ecs::components::Position) component that mirrors
/// `transform.translate` for compatibility with v0.2.0 modify commands.
/// Modify commands update `Position` but **not** `BlockRef.transform` in
/// v0.3.0 — this is a known limitation that will be resolved when modify
/// commands are updated to work with transforms directly.
#[derive(Debug, Clone, Copy)]
pub struct BlockRef {
    /// ID of the block definition this entity instantiates.
    pub definition: BlockId,
    /// Local-to-world transform applied to the definition's geometry.
    pub transform: Transform2D,
}

/// Builder/reserved type for block insertion operations.
///
/// Full implementation deferred to a future step.
#[derive(Debug, Clone, Default)]
pub struct BlockInsert;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::BlockId;
    use crate::geometry::Transform2D;

    #[test]
    fn test_block_ref_creation() {
        let id = BlockId(0);
        let transform = Transform2D::IDENTITY;
        let bref = BlockRef {
            definition: id,
            transform,
        };
        assert_eq!(bref.definition, id);
    }

    #[test]
    fn test_block_ref_debug() {
        let bref = BlockRef {
            definition: BlockId(3),
            transform: Transform2D::IDENTITY,
        };
        let dbg = format!("{:?}", bref);
        assert!(dbg.contains("BlockRef"));
        assert!(dbg.contains("definition"));
        assert!(dbg.contains("transform"));
    }

    #[test]
    fn test_block_insert_default() {
        let insert = BlockInsert;
        let dbg = format!("{:?}", insert);
        assert!(dbg.contains("BlockInsert"));
    }
}
