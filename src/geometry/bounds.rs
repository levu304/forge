use crate::geometry::Point2D;

/// Axis-aligned bounding box for culling and fit-to-view.
#[derive(Debug, Clone, Copy)]
pub struct BoundingBox2D {
    pub min: Point2D,
    pub max: Point2D,
}

impl BoundingBox2D {
    pub fn is_empty(&self) -> bool {
        self.min.x >= self.max.x || self.min.y >= self.max.y
    }
    pub fn center(&self) -> Point2D {
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
}