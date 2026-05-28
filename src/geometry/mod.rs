//! Math & geometric types.
//!
//! CAD-grade 2D geometry types including Point2D, BoundingBox2D,
//! Vector2D (reserved), and Transform2D (reserved).

/// Domain-appropriate geometric tolerance for floating-point comparisons.
///
/// Chosen at `1e-12` — tight enough for sub-micrometre CAD precision at
/// typical scales, but loose enough to survive IEEE 754 rounding from
/// trigonometric operations.  Used instead of `f64::EPSILON` (~2.2e-16)
/// for degenerate segment / zero-radius / coincident-point checks.
pub const GEOMETRIC_EPSILON: f64 = 1e-12;

pub mod point;
pub mod vector;    // reserved — v0.2.0+ transforms
pub mod transform; // reserved — v0.2.0+
pub mod bounds;
pub mod spline; // reserved — v0.3.0+ cubic Bézier curves

pub use point::Point2D;
pub use bounds::BoundingBox2D;