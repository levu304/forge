//! Cubic Bézier curve evaluation and polyline approximation.

use crate::geometry::Point2D;

/// A cubic Bézier curve defined by four control points.
///
/// The curve is parameterised by `t ∈ [0, 1]` and evaluated using the
/// Bernstein basis:
///
/// ```text
/// B(t) = (1-t)³·P0 + 3·(1-t)²·t·P1 + 3·(1-t)·t²·P2 + t³·P3
/// ```
///
/// Full implementation in Step 4 (split, arc-length approximation, etc.).
#[derive(Debug, Clone, Copy, Default)]
pub struct CubicBezier {
    pub p0: Point2D,
    pub p1: Point2D,
    pub p2: Point2D,
    pub p3: Point2D,
}

impl CubicBezier {
    /// Evaluate the curve at parameter `t ∈ [0, 1]` using the Bernstein basis.
    ///
    /// # Examples
    ///
    /// ```
    /// # use forge::geometry::{CubicBezier, Point2D};
    /// let c = CubicBezier {
    ///     p0: Point2D::new(0.0, 0.0),
    ///     p1: Point2D::new(0.0, 1.0),
    ///     p2: Point2D::new(1.0, 1.0),
    ///     p3: Point2D::new(1.0, 0.0),
    /// };
    /// assert_eq!(c.evaluate(0.0), c.p0);
    /// assert_eq!(c.evaluate(1.0), c.p3);
    /// ```
    pub fn evaluate(&self, t: f64) -> Point2D {
        let t2 = t * t;
        let t3 = t2 * t;
        let mt = 1.0 - t;
        let mt2 = mt * mt;
        let mt3 = mt2 * mt;

        // B(t) = (1-t)³·P0 + 3·(1-t)²·t·P1 + 3·(1-t)·t²·P2 + t³·P3
        self.p0 * mt3 + self.p1 * (3.0 * mt2 * t) + self.p2 * (3.0 * mt * t2) + self.p3 * t3
    }

    /// Convert the curve to a polyline (ordered vertex list).
    ///
    /// Evaluates the curve at `segments + 1` evenly-spaced parameter values
    /// from `0.0` to `1.0` inclusive.  A value of `segments = 1` produces
    /// two points (`p0` and `p3`); larger values give finer approximations.
    ///
    /// # Panics
    ///
    /// Panics if `segments == 0`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use forge::geometry::{CubicBezier, Point2D};
    /// let c = CubicBezier {
    ///     p0: Point2D::new(0.0, 0.0),
    ///     p1: Point2D::new(0.0, 1.0),
    ///     p2: Point2D::new(1.0, 1.0),
    ///     p3: Point2D::new(1.0, 0.0),
    /// };
    /// let poly = c.to_polyline(1);
    /// assert_eq!(poly.len(), 2);
    /// assert_eq!(poly[0], c.p0);
    /// assert_eq!(poly[1], c.p3);
    ///
    /// let poly = c.to_polyline(2);
    /// assert_eq!(poly.len(), 3);
    /// assert_eq!(poly[0], c.p0);
    /// assert_eq!(poly[1], c.evaluate(0.5));
    /// assert_eq!(poly[2], c.p3);
    /// ```
    pub fn to_polyline(&self, segments: usize) -> Vec<Point2D> {
        assert!(segments > 0, "segments must be > 0");
        let step = 1.0 / segments as f64;
        let mut points = Vec::with_capacity(segments + 1);
        for i in 0..=segments {
            let t = i as f64 * step;
            points.push(self.evaluate(t));
        }
        points
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Symmetric cubic Bézier: starts at (0,0), ends at (2,0),
    /// with control points forming a bump of height 1.
    fn symmetric_bezier() -> CubicBezier {
        CubicBezier {
            p0: Point2D::new(0.0, 0.0),
            p1: Point2D::new(0.0, 1.0),
            p2: Point2D::new(2.0, 1.0),
            p3: Point2D::new(2.0, 0.0),
        }
    }

    // ------------------------------------------------------------------
    // 1. evaluate(0.0) returns p0
    // ------------------------------------------------------------------
    #[test]
    fn test_evaluate_at_start() {
        let c = symmetric_bezier();
        assert_eq!(c.evaluate(0.0), c.p0);
    }

    // ------------------------------------------------------------------
    // 2. evaluate(1.0) returns p3
    // ------------------------------------------------------------------
    #[test]
    fn test_evaluate_at_end() {
        let c = symmetric_bezier();
        assert_eq!(c.evaluate(1.0), c.p3);
    }

    // ------------------------------------------------------------------
    // 3. to_polyline with segments=1 returns [p0, p3]
    // ------------------------------------------------------------------
    #[test]
    fn test_to_polyline_one_segment() {
        let c = symmetric_bezier();
        let poly = c.to_polyline(1);
        assert_eq!(poly.len(), 2);
        assert_eq!(poly[0], c.p0);
        assert_eq!(poly[1], c.p3);
    }

    // ------------------------------------------------------------------
    // 4. to_polyline with segments=2 returns [p0, mid, p3]
    // ------------------------------------------------------------------
    #[test]
    fn test_to_polyline_two_segments() {
        let c = symmetric_bezier();
        let poly = c.to_polyline(2);
        assert_eq!(poly.len(), 3);
        assert_eq!(poly[0], c.p0);
        assert_eq!(poly[1], c.evaluate(0.5));
        assert_eq!(poly[2], c.p3);
    }

    // ------------------------------------------------------------------
    // 5. Midpoint of symmetric Bézier is at expected center
    // ------------------------------------------------------------------
    #[test]
    fn test_midpoint_symmetric() {
        let c = symmetric_bezier();
        let mid = c.evaluate(0.5);
        // For a symmetric bezier with p0=(0,0), p1=(0,1), p2=(2,1), p3=(2,0):
        // B(0.5) = 0.125*(0,0) + 0.375*(0,1) + 0.375*(2,1) + 0.125*(2,0)
        //        = (0, 0) + (0, 0.375) + (0.75, 0.375) + (0.25, 0)
        //        = (1.0, 0.75)
        assert!((mid.x - 1.0).abs() < 1e-12,
            "expected mid.x ≈ 1.0, got {}", mid.x);
        assert!((mid.y - 0.75).abs() < 1e-12,
            "expected mid.y ≈ 0.75, got {}", mid.y);
    }

    // ------------------------------------------------------------------
    // 6. Evaluate with t=0.5 on a straight-line Bézier
    // ------------------------------------------------------------------
    #[test]
    fn test_evaluate_straight_line() {
        // A straight line from (0,0) to (4,0) with collinear control points
        let c = CubicBezier {
            p0: Point2D::new(0.0, 0.0),
            p1: Point2D::new(4.0 / 3.0, 0.0),
            p2: Point2D::new(8.0 / 3.0, 0.0),
            p3: Point2D::new(4.0, 0.0),
        };
        let mid = c.evaluate(0.5);
        assert!((mid.x - 2.0).abs() < 1e-12);
        assert!((mid.y - 0.0).abs() < 1e-12);
    }

    // ------------------------------------------------------------------
    // 7. to_polyline with 10 segments returns 11 points
    // ------------------------------------------------------------------
    #[test]
    fn test_to_polyline_ten_segments() {
        let c = symmetric_bezier();
        let poly = c.to_polyline(10);
        assert_eq!(poly.len(), 11);
        assert_eq!(poly[0], c.p0);
        assert_eq!(poly[10], c.p3);
    }

    // ------------------------------------------------------------------
    // 8. Debug format
    // ------------------------------------------------------------------
    #[test]
    fn test_debug_format() {
        let c = symmetric_bezier();
        let s = format!("{:?}", c);
        assert!(s.contains("CubicBezier"));
    }
}
