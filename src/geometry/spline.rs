//! CubicBezier curve (stub)

use crate::geometry::Point2D;

/// A cubic Bézier curve defined by four control points.
///
/// Full implementation in Step 4.
#[derive(Debug, Clone, Copy, Default)]
pub struct CubicBezier {
    pub p0: Point2D,
    pub p1: Point2D,
    pub p2: Point2D,
    pub p3: Point2D,
}
