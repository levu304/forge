use std::ops::{Add, Mul, Sub};

use super::GEOMETRIC_EPSILON;

/// CAD-grade 2D point using f64 for precision.
/// All world-space coordinates use this type.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point2D {
    pub x: f64,
    pub y: f64,
}

impl Point2D {
    pub fn new(x: f64, y: f64) -> Self { Self { x, y } }
    pub fn to_f32_array(&self) -> [f32; 2] {
        [self.x as f32, self.y as f32]
    }

    /// Euclidean distance to another point.
    pub fn distance(self, other: Point2D) -> f64 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        (dx * dx + dy * dy).sqrt()
    }

    /// Return a unit vector in the same direction.
    ///
    /// Returns `(0, 0)` if the vector is zero-length (avoiding NaN).
    pub fn normalized(self) -> Point2D {
        let len = (self.x * self.x + self.y * self.y).sqrt();
        if len < GEOMETRIC_EPSILON {
            Point2D::new(0.0, 0.0)
        } else {
            Point2D::new(self.x / len, self.y / len)
        }
    }
}

// ---------------------------------------------------------------------------
// Operator overloads for ergonomic vector arithmetic
// ---------------------------------------------------------------------------

impl Add for Point2D {
    type Output = Point2D;
    fn add(self, rhs: Point2D) -> Point2D {
        Point2D::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl Sub for Point2D {
    type Output = Point2D;
    fn sub(self, rhs: Point2D) -> Point2D {
        Point2D::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl Mul<f64> for Point2D {
    type Output = Point2D;
    fn mul(self, rhs: f64) -> Point2D {
        Point2D::new(self.x * rhs, self.y * rhs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_and_accessors() {
        let p = Point2D::new(1.0, 2.0);
        assert_eq!(p.x, 1.0);
        assert_eq!(p.y, 2.0);
    }

    #[test]
    fn test_equality() {
        assert_eq!(Point2D::new(3.0, 4.0), Point2D::new(3.0, 4.0));
        assert_ne!(Point2D::new(1.0, 2.0), Point2D::new(1.0, 3.0));
    }

    #[test]
    fn test_default_is_origin() {
        let p = Point2D::default();
        assert_eq!(p.x, 0.0);
        assert_eq!(p.y, 0.0);
    }

    #[test]
    fn test_to_f32_array_round_trip() {
        let p = Point2D::new(1.5, 2.75);
        let arr = p.to_f32_array();
        assert_eq!(arr, [1.5_f32, 2.75_f32]);
    }

    #[test]
    fn test_to_f32_array_negative() {
        let p = Point2D::new(-42.0, -99.9);
        let arr = p.to_f32_array();
        assert!((arr[0] as f64 - (-42.0)).abs() < 0.002);
        assert!((arr[1] as f64 - (-99.9)).abs() < 0.002);
    }

    #[test]
    fn test_to_f32_array_zero() {
        let p = Point2D::new(0.0, 0.0);
        assert_eq!(p.to_f32_array(), [0.0, 0.0]);
    }

    #[test]
    fn test_to_f32_array_large_numbers() {
        // f64 → f32 conversion: large values may lose precision but should not panic.
        let p = Point2D::new(1e15, -1e15);
        let arr = p.to_f32_array();
        assert!(arr[0].is_finite());
        assert!(arr[1].is_finite());
    }

    #[test]
    fn test_to_f32_array_precision_loss() {
        // A value that cannot be represented exactly in f32.
        let p = Point2D::new(1.234_567_89, 0.0);
        let arr = p.to_f32_array();
        // Should still be close (within f32 precision).
        assert!((arr[0] - 1.234_567_9_f32).abs() < 1e-6);
    }

    #[test]
    fn test_clone_and_copy() {
        let p1 = Point2D::new(7.0, 8.0);
        let p2 = p1; // Copy via assignment
        assert_eq!(p1, p2);
    }

    #[test]
    fn test_debug_format() {
        let p = Point2D::new(1.0, 2.0);
        let debug = format!("{:?}", p);
        assert!(debug.contains("1.0"));
        assert!(debug.contains("2.0"));
    }

    // ------------------------------------------------------------------
    // Arithmetic operators
    // ------------------------------------------------------------------

    #[test]
    fn test_add() {
        let a = Point2D::new(1.0, 2.0);
        let b = Point2D::new(3.0, 4.0);
        let c = a + b;
        assert_eq!(c.x, 4.0);
        assert_eq!(c.y, 6.0);
    }

    #[test]
    fn test_sub() {
        let a = Point2D::new(5.0, 8.0);
        let b = Point2D::new(3.0, 2.0);
        let c = a - b;
        assert_eq!(c.x, 2.0);
        assert_eq!(c.y, 6.0);
    }

    #[test]
    fn test_mul_scalar() {
        let a = Point2D::new(2.0, 3.0);
        let b = a * 2.5;
        assert_eq!(b.x, 5.0);
        assert_eq!(b.y, 7.5);
    }

    #[test]
    fn test_mul_scalar_zero() {
        let a = Point2D::new(42.0, -7.0);
        let b = a * 0.0;
        assert_eq!(b.x, 0.0);
        assert_eq!(b.y, 0.0);
    }

    #[test]
    fn test_mul_scalar_negative() {
        let a = Point2D::new(3.0, -4.0);
        let b = a * (-1.0);
        assert_eq!(b.x, -3.0);
        assert_eq!(b.y, 4.0);
    }

    // ------------------------------------------------------------------
    // distance
    // ------------------------------------------------------------------

    #[test]
    fn test_distance_same_point() {
        let a = Point2D::new(3.0, 4.0);
        assert_eq!(a.distance(a), 0.0);
    }

    #[test]
    fn test_distance_basic() {
        let a = Point2D::new(0.0, 0.0);
        let b = Point2D::new(3.0, 4.0);
        assert_eq!(a.distance(b), 5.0);
    }

    #[test]
    fn test_distance_commutative() {
        let a = Point2D::new(1.0, 2.0);
        let b = Point2D::new(4.0, 6.0);
        assert!((a.distance(b) - b.distance(a)).abs() < f64::EPSILON);
    }

    #[test]
    fn test_distance_negative_coords() {
        let a = Point2D::new(-1.0, -1.0);
        let b = Point2D::new(2.0, 3.0);
        // dx=3, dy=4 → dist=5
        assert_eq!(a.distance(b), 5.0);
    }

    // ------------------------------------------------------------------
    // normalized
    // ------------------------------------------------------------------

    #[test]
    fn test_normalized_unit_x() {
        let v = Point2D::new(5.0, 0.0).normalized();
        assert!((v.x - 1.0).abs() < f64::EPSILON);
        assert!((v.y - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_normalized_unit_y() {
        let v = Point2D::new(0.0, -3.0).normalized();
        assert!((v.x - 0.0).abs() < f64::EPSILON);
        assert!((v.y - (-1.0)).abs() < f64::EPSILON);
    }

    #[test]
    fn test_normalized_diagonal() {
        let v = Point2D::new(3.0, 4.0).normalized();
        assert!((v.x - 0.6).abs() < f64::EPSILON);
        assert!((v.y - 0.8).abs() < f64::EPSILON);
        // Length of normalized vector must be 1.
        let len = (v.x * v.x + v.y * v.y).sqrt();
        assert!((len - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_normalized_zero_vector() {
        let v = Point2D::new(0.0, 0.0).normalized();
        assert_eq!(v.x, 0.0);
        assert_eq!(v.y, 0.0);
    }

    #[test]
    fn test_normalized_negative() {
        let v = Point2D::new(-2.0, -2.0).normalized();
        let expected = -2.0_f64 / (8.0_f64.sqrt());
        assert!((v.x - expected).abs() < f64::EPSILON);
        assert!((v.y - expected).abs() < f64::EPSILON);
    }
}