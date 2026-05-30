//! Grip handle types for direct-manipulation editing.
//!
//! A *grip* is a screen-space hotspot rendered on or near a selected
//! entity's geometry.  Users click and drag grips to modify the
//! underlying entity — moving endpoints, shifting centres, or adjusting
//! individual polyline vertices.

use crate::geometry::Point2D;

/// The type of a grip handle, determining its visual appearance and
/// behaviour during drag operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum GripType {
    /// Corner/endpoint grip (square).
    #[default]
    Endpoint,
    /// Midpoint grip (triangle) — non-interactive in v0.3.0.
    Midpoint,
    /// Center grip (circle).
    Center,
    /// Quadrant grip (diamond) — non-interactive in v0.3.0.
    Quadrant,
    /// Polyline vertex grip (cyan) — interactive in v0.3.0.
    Vertex,
}

/// A single grip handle on an entity, used for direct manipulation.
///
/// Each grip carries its type (which determines appearance and drag
/// behaviour), a world-space position, a reference to the owning entity,
/// and a vertex index (used to identify which vertex of a polyline or
/// which endpoint of a line segment the grip belongs to).
#[derive(Debug, Clone, Copy)]
pub struct GripHandle {
    /// The type of this grip handle.
    pub grip_type: GripType,
    /// World-space position of the grip.
    pub position: Point2D,
    /// The entity this grip belongs to.
    pub entity: hecs::Entity,
    /// Index into the entity's vertex list (for polylines) or
    /// discriminator for line endpoints (0 = start, 1 = end).
    /// For midpoints, centres, and quadrants this is always 0.
    pub vertex_index: usize,
}

impl GripHandle {
    /// Create a new grip handle.
    pub fn new(
        grip_type: GripType,
        position: Point2D,
        entity: hecs::Entity,
        vertex_index: usize,
    ) -> Self {
        Self {
            grip_type,
            position,
            entity,
            vertex_index,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_grip_type_default() {
        assert_eq!(GripType::default(), GripType::Endpoint);
    }

    #[test]
    fn test_grip_type_variants() {
        // All variants can be constructed and compared.
        let variants = [
            GripType::Endpoint,
            GripType::Midpoint,
            GripType::Center,
            GripType::Quadrant,
            GripType::Vertex,
        ];
        // No duplicates — 5 distinct variants.
        let mut unique = std::collections::HashSet::new();
        for v in &variants {
            assert!(unique.insert(v));
        }
        assert_eq!(unique.len(), 5);
    }

    #[test]
    fn test_grip_handle_construction() {
        let entity = hecs::Entity::from_bits(1u64 << 32 | 42).unwrap();
        let pos = Point2D::new(10.0, 20.0);
        let handle = GripHandle::new(GripType::Endpoint, pos, entity, 0);

        assert_eq!(handle.grip_type, GripType::Endpoint);
        assert_eq!(handle.position, pos);
        assert_eq!(handle.entity, entity);
        assert_eq!(handle.vertex_index, 0);
    }

    #[test]
    fn test_grip_handle_copy() {
        let entity = hecs::Entity::from_bits(1u64 << 32 | 7).unwrap();
        let h1 = GripHandle::new(GripType::Center, Point2D::new(5.0, 5.0), entity, 0);
        let h2 = h1; // Copy
        assert_eq!(h1.entity, h2.entity);
        assert_eq!(h1.position, h2.position);
    }
}
