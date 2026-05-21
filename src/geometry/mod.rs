//! Math & geometric types.
//!
//! CAD-grade 2D geometry types including Point2D, BoundingBox2D,
//! Vector2D (reserved), and Transform2D (reserved).

pub mod point;
pub mod vector;    // reserved — v0.2.0+ transforms
pub mod transform; // reserved — v0.2.0+
pub mod bounds;

pub use point::Point2D;
pub use bounds::BoundingBox2D;