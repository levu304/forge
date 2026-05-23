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
        assert!((arr[0] - (-42.0_f32)).abs() < f32::EPSILON);
        assert!((arr[1] - (-99.9_f32)).abs() < f32::EPSILON);
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
}