use crate::geometry::Point2D;

/// Axis-aligned bounding box for culling and fit-to-view.
#[derive(Debug, Clone, Copy)]
pub struct BoundingBox2D {
    pub min: Point2D,
    pub max: Point2D,
}

impl BoundingBox2D {
    /// Create an empty (degenerate) bounding box.
    ///
    /// The min point is set to `(f64::MAX, f64::MAX)` and max to
    /// `(f64::MIN, f64::MIN)`, guaranteeing that `is_empty()` returns
    /// `true`.  Used as a starting accumulator for `from_points()`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use forge::geometry::BoundingBox2D;
    /// let b = BoundingBox2D::empty();
    /// assert!(b.is_empty());
    /// ```
    pub fn empty() -> Self {
        Self {
            min: Point2D::new(f64::MAX, f64::MAX),
            max: Point2D::new(f64::MIN, f64::MIN),
        }
    }

    /// Construct a bounding box from a slice of points.
    ///
    /// Returns an empty box if the slice is empty.
    ///
    /// # Notes
    ///
    /// Uses inline min/max accumulation rather than `union()` because
    /// single-point boxes (where `min == max`) are considered empty
    /// by `is_empty()`, which would cause `union()` to discard them.
    ///
    /// # Examples
    ///
    /// ```
    /// # use forge::geometry::{BoundingBox2D, Point2D};
    /// let pts = vec![
    ///     Point2D::new(0.0, 0.0),
    ///     Point2D::new(5.0, 3.0),
    /// ];
    /// let b = BoundingBox2D::from_points(&pts);
    /// assert_eq!(b.min.x, 0.0);
    /// assert_eq!(b.max.x, 5.0);
    /// assert_eq!(b.min.y, 0.0);
    /// assert_eq!(b.max.y, 3.0);
    /// ```
    pub fn from_points(points: &[Point2D]) -> Self {
        let mut iter = points.iter().copied();
        let first = match iter.next() {
            Some(p) => p,
            None => return Self::empty(),
        };
        let mut min_x = first.x;
        let mut min_y = first.y;
        let mut max_x = first.x;
        let mut max_y = first.y;
        for p in iter {
            debug_assert!(
                p.x.is_finite() && p.y.is_finite(),
                "from_points encountered non-finite point ({}, {})",
                p.x, p.y,
            );
            min_x = min_x.min(p.x);
            min_y = min_y.min(p.y);
            max_x = max_x.max(p.x);
            max_y = max_y.max(p.y);
        }
        Self {
            min: Point2D::new(min_x, min_y),
            max: Point2D::new(max_x, max_y),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.min.x >= self.max.x || self.min.y >= self.max.y
    }
    pub fn center(&self) -> Point2D {
        debug_assert!(!self.is_empty(), "center() called on empty bounding box");
        Point2D::new(
            (self.min.x + self.max.x) / 2.0,
            (self.min.y + self.max.y) / 2.0,
        )
    }
    /// Expand this bounding box to include another.
    /// If either is empty, returns the non-empty one.
    pub fn union(&self, other: &Self) -> Self {
        if self.is_empty() {
            return *other;
        }
        if other.is_empty() {
            return *self;
        }
        Self {
            min: Point2D::new(
                self.min.x.min(other.min.x),
                self.min.y.min(other.min.y),
            ),
            max: Point2D::new(
                self.max.x.max(other.max.x),
                self.max.y.max(other.max.y),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_bbox(min_x: f64, min_y: f64, max_x: f64, max_y: f64) -> BoundingBox2D {
        BoundingBox2D {
            min: Point2D::new(min_x, min_y),
            max: Point2D::new(max_x, max_y),
        }
    }

    #[test]
    fn test_is_empty_standard() {
        let bbox = make_bbox(0.0, 0.0, 10.0, 10.0);
        assert!(!bbox.is_empty());
    }

    #[test]
    fn test_is_empty_degenerate_point() {
        // min == max => empty (one-dimensional, no area)
        let bbox = make_bbox(5.0, 5.0, 5.0, 5.0);
        assert!(bbox.is_empty());
    }

    #[test]
    fn test_is_empty_reversed_x() {
        let bbox = make_bbox(10.0, 0.0, 0.0, 10.0);
        assert!(bbox.is_empty());
    }

    #[test]
    fn test_is_empty_reversed_y() {
        let bbox = make_bbox(0.0, 10.0, 10.0, 0.0);
        assert!(bbox.is_empty());
    }

    #[test]
    fn test_is_empty_zero_width() {
        let bbox = make_bbox(5.0, 0.0, 5.0, 10.0);
        assert!(bbox.is_empty());
    }

    #[test]
    fn test_center() {
        let bbox = make_bbox(-10.0, -10.0, 10.0, 10.0);
        let c = bbox.center();
        assert_eq!(c.x, 0.0);
        assert_eq!(c.y, 0.0);
    }

    #[test]
    fn test_center_asymmetric() {
        let bbox = make_bbox(2.0, 3.0, 10.0, 11.0);
        let c = bbox.center();
        assert_eq!(c.x, 6.0);
        assert_eq!(c.y, 7.0);
    }

    #[test]
    fn test_center_negative() {
        let bbox = make_bbox(-20.0, -30.0, -10.0, -20.0);
        let c = bbox.center();
        assert_eq!(c.x, -15.0);
        assert_eq!(c.y, -25.0);
    }

    #[test]
    fn test_union_overlapping() {
        let a = make_bbox(0.0, 0.0, 10.0, 10.0);
        let b = make_bbox(5.0, 5.0, 15.0, 15.0);
        let u = a.union(&b);
        assert_eq!(u.min.x, 0.0);
        assert_eq!(u.min.y, 0.0);
        assert_eq!(u.max.x, 15.0);
        assert_eq!(u.max.y, 15.0);
    }

    #[test]
    fn test_union_non_overlapping() {
        let a = make_bbox(0.0, 0.0, 10.0, 10.0);
        let b = make_bbox(20.0, 20.0, 30.0, 30.0);
        let u = a.union(&b);
        assert_eq!(u.min.x, 0.0);
        assert_eq!(u.min.y, 0.0);
        assert_eq!(u.max.x, 30.0);
        assert_eq!(u.max.y, 30.0);
    }

    #[test]
    fn test_union_empty_first() {
        let empty = make_bbox(5.0, 5.0, 5.0, 5.0);
        let non_empty = make_bbox(0.0, 0.0, 10.0, 10.0);
        let u = empty.union(&non_empty);
        assert_eq!(u.min.x, 0.0);
        assert_eq!(u.max.x, 10.0);
        assert_eq!(u.min.y, 0.0);
        assert_eq!(u.max.y, 10.0);
    }

    #[test]
    fn test_union_empty_second() {
        let non_empty = make_bbox(0.0, 0.0, 10.0, 10.0);
        let empty = make_bbox(5.0, 5.0, 5.0, 5.0);
        let u = non_empty.union(&empty);
        assert_eq!(u.min.x, 0.0);
        assert_eq!(u.max.x, 10.0);
        assert_eq!(u.min.y, 0.0);
        assert_eq!(u.max.y, 10.0);
    }

    #[test]
    fn test_union_both_empty() {
        let empty_a = make_bbox(0.0, 0.0, 0.0, 0.0);
        let empty_b = make_bbox(5.0, 5.0, 5.0, 5.0);
        let u = empty_a.union(&empty_b);
        // Both empty: first is empty → return second (which is also empty).
        assert!(u.is_empty());
    }

    #[test]
    fn test_union_with_self() {
        let bbox = make_bbox(-5.0, -5.0, 5.0, 5.0);
        let u = bbox.union(&bbox);
        assert_eq!(u.min.x, -5.0);
        assert_eq!(u.max.x, 5.0);
    }

    #[test]
    fn test_clone() {
        let a = make_bbox(1.0, 2.0, 3.0, 4.0);
        let b = a;
        assert_eq!(a.min.x, b.min.x);
        assert_eq!(a.max.y, b.max.y);
    }

    #[test]
    fn test_debug_format() {
        let bbox = make_bbox(1.0, 2.0, 3.0, 4.0);
        let s = format!("{:?}", bbox);
        assert!(s.contains("BoundingBox2D"));
        assert!(s.contains("1.0"));
    }

    // ------------------------------------------------------------------
    // empty()
    // ------------------------------------------------------------------
    #[test]
    fn test_empty_returns_empty() {
        let b = BoundingBox2D::empty();
        assert!(b.is_empty());
    }

    #[test]
    fn test_empty_min_greater_than_max() {
        let b = BoundingBox2D::empty();
        assert!(b.min.x > b.max.x);
        assert!(b.min.y > b.max.y);
    }

    // ------------------------------------------------------------------
    // from_points()
    // ------------------------------------------------------------------
    #[test]
    fn test_from_points_single_point() {
        let p = Point2D::new(3.0, 7.0);
        let b = BoundingBox2D::from_points(&[p]);
        assert_eq!(b.min.x, 3.0);
        assert_eq!(b.min.y, 7.0);
        assert_eq!(b.max.x, 3.0);
        assert_eq!(b.max.y, 7.0);
    }

    #[test]
    fn test_from_points_multiple_points() {
        let pts = vec![
            Point2D::new(0.0, 10.0),
            Point2D::new(5.0, -5.0),
            Point2D::new(-3.0, 3.0),
        ];
        let b = BoundingBox2D::from_points(&pts);
        assert_eq!(b.min.x, -3.0);
        assert_eq!(b.min.y, -5.0);
        assert_eq!(b.max.x, 5.0);
        assert_eq!(b.max.y, 10.0);
    }

    #[test]
    fn test_from_points_empty_slice() {
        let b = BoundingBox2D::from_points(&[]);
        assert!(b.is_empty());
    }

    #[test]
    fn test_from_points_negative_coords() {
        let pts = vec![
            Point2D::new(-10.0, -10.0),
            Point2D::new(-1.0, -1.0),
        ];
        let b = BoundingBox2D::from_points(&pts);
        assert_eq!(b.min.x, -10.0);
        assert_eq!(b.min.y, -10.0);
        assert_eq!(b.max.x, -1.0);
        assert_eq!(b.max.y, -1.0);
    }

    #[test]
    fn test_from_points_all_same() {
        let p = Point2D::new(5.0, 5.0);
        let pts = vec![p, p, p];
        let b = BoundingBox2D::from_points(&pts);
        assert_eq!(b.min.x, 5.0);
        assert_eq!(b.max.x, 5.0);
        assert_eq!(b.min.y, 5.0);
        assert_eq!(b.max.y, 5.0);
    }
}