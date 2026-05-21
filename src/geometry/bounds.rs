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