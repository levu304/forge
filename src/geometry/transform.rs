//! 2D affine transform (scale → rotate → translate).
//!
//! The transform pipeline is defined as:
//!
//! ```text
//! p' = translate(rotate(scale(p)))
//! ```
//!
//! Rotations are counter-clockwise in radians.  All geometry uses `f64`.

use crate::geometry::{BoundingBox2D, Point2D};

/// A 2D affine transform with scale, rotation, and translation.
///
/// The transform order is **scale → rotate → translate** (SRT), matching
/// the common graphics convention for object-local transforms.
///
/// # Identity
///
/// ```
/// # use forge::geometry::{Point2D, Transform2D};
/// let t = Transform2D::IDENTITY;
/// assert_eq!(t.apply_to_point(Point2D::new(5.0, 3.0)), Point2D::new(5.0, 3.0));
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform2D {
    /// X-axis scale factor.
    pub scale_x: f64,
    /// Y-axis scale factor.
    pub scale_y: f64,
    /// Counter-clockwise rotation in radians.
    pub rotation: f64,
    /// Horizontal translation.
    pub translate_x: f64,
    /// Vertical translation.
    pub translate_y: f64,
}

impl Default for Transform2D {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Transform2D {
    /// The identity transform: no scale, rotation, or translation.
    pub const IDENTITY: Transform2D = Transform2D {
        scale_x: 1.0,
        scale_y: 1.0,
        rotation: 0.0,
        translate_x: 0.0,
        translate_y: 0.0,
    };

    /// Construct a transform from explicit translation, rotation, and scale.
    ///
    /// The `scale` is split into separate `scale_x` and `scale_y` parameters
    /// to distinguish it from the `translate` point (preventing argument-order
    /// confusion at the call site).
    ///
    /// # Examples
    ///
    /// ```
    /// # use forge::geometry::{Point2D, Transform2D};
    /// let t = Transform2D::new(
    ///     Point2D::new(10.0, 20.0),     // translate
    ///     std::f64::consts::FRAC_PI_2,  // rotation
    ///     2.0, 3.0,                     // scale_x, scale_y
    /// );
    /// ```
    pub fn new(translate: Point2D, rotation: f64, scale_x: f64, scale_y: f64) -> Self {
        Self {
            scale_x,
            scale_y,
            rotation,
            translate_x: translate.x,
            translate_y: translate.y,
        }
    }

    /// Apply this transform to a point (scale → rotate → translate).
    ///
    /// # Examples
    ///
    /// ```
    /// # use forge::geometry::{Point2D, Transform2D};
    /// let t = Transform2D::new(
    ///     Point2D::new(10.0, 0.0),       // translate right by 10
    ///     0.0,                            // no rotation
    ///     2.0, 2.0,                       // scale 2×
    /// );
    /// // (3, 4) scaled → (6, 8), then translated → (16, 8)
    /// assert_eq!(t.apply_to_point(Point2D::new(3.0, 4.0)), Point2D::new(16.0, 8.0));
    /// ```
    pub fn apply_to_point(&self, point: Point2D) -> Point2D {
        debug_assert!(
            point.x.is_finite() && point.y.is_finite(),
            "apply_to_point called with non-finite point ({}, {})",
            point.x, point.y,
        );
        // 1. Scale
        let sx = point.x * self.scale_x;
        let sy = point.y * self.scale_y;

        // 2. Rotate (counter-clockwise)
        let cos_r = self.rotation.cos();
        let sin_r = self.rotation.sin();
        let rx = sx * cos_r - sy * sin_r;
        let ry = sx * sin_r + sy * cos_r;

        // 3. Translate
        Point2D::new(rx + self.translate_x, ry + self.translate_y)
    }

    /// Apply this transform to every corner of a bounding box and return
    /// the union of the transformed corners.
    ///
    /// Returns an empty `BoundingBox2D` if the input box is empty.
    ///
    /// # Examples
    ///
    /// ```
    /// # use forge::geometry::{BoundingBox2D, Point2D, Transform2D};
    /// let bbox = BoundingBox2D { min: Point2D::new(0.0, 0.0), max: Point2D::new(1.0, 1.0) };
    /// let t = Transform2D::IDENTITY;
    /// let result = t.apply_to_bounds(&bbox);
    /// assert_eq!(result.min.x, 0.0);
    /// assert_eq!(result.max.x, 1.0);
    /// ```
    pub fn apply_to_bounds(&self, bounds: &BoundingBox2D) -> BoundingBox2D {
        if bounds.is_empty() {
            return BoundingBox2D::empty();
        }

        let transformed = [
            self.apply_to_point(Point2D::new(bounds.min.x, bounds.min.y)),
            self.apply_to_point(Point2D::new(bounds.max.x, bounds.min.y)),
            self.apply_to_point(Point2D::new(bounds.min.x, bounds.max.y)),
            self.apply_to_point(Point2D::new(bounds.max.x, bounds.max.y)),
        ];

        BoundingBox2D::from_points(&transformed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::FRAC_PI_2;

    fn make_bbox(min_x: f64, min_y: f64, max_x: f64, max_y: f64) -> BoundingBox2D {
        BoundingBox2D {
            min: Point2D::new(min_x, min_y),
            max: Point2D::new(max_x, max_y),
        }
    }

    // ------------------------------------------------------------------
    // 1. IDENTITY returns input unchanged
    // ------------------------------------------------------------------
    #[test]
    fn test_identity_is_identity() {
        let pt = Point2D::new(42.0, -17.0);
        assert_eq!(Transform2D::IDENTITY.apply_to_point(pt), pt);
    }

    // ------------------------------------------------------------------
    // 2. Scale doubles coordinates
    // ------------------------------------------------------------------
    #[test]
    fn test_scale_doubles() {
        let t = Transform2D::new(
            Point2D::new(0.0, 0.0),
            0.0,
            2.0, 2.0,
        );
        let result = t.apply_to_point(Point2D::new(3.0, 4.0));
        assert_eq!(result, Point2D::new(6.0, 8.0));
    }

    // ------------------------------------------------------------------
    // 3. Non-uniform scale
    // ------------------------------------------------------------------
    #[test]
    fn test_non_uniform_scale() {
        let t = Transform2D::new(
            Point2D::new(0.0, 0.0),
            0.0,
            2.0, 3.0,
        );
        let result = t.apply_to_point(Point2D::new(1.0, 2.0));
        assert_eq!(result, Point2D::new(2.0, 6.0));
    }

    // ------------------------------------------------------------------
    // 4. Rotation of π/2 maps (1,0) to (0,1)
    // ------------------------------------------------------------------
    #[test]
    fn test_rotation_quarter_turn() {
        let t = Transform2D::new(
            Point2D::new(0.0, 0.0),
            FRAC_PI_2,
            1.0, 1.0,
        );
        let result = t.apply_to_point(Point2D::new(1.0, 0.0));
        assert!((result.x - 0.0).abs() < 1e-12,
            "expected x≈0, got {}", result.x);
        assert!((result.y - 1.0).abs() < 1e-12,
            "expected y≈1, got {}", result.y);
    }

    // ------------------------------------------------------------------
    // 5. Rotation of π maps (1,0) to (-1,0)
    // ------------------------------------------------------------------
    #[test]
    fn test_rotation_half_turn() {
        let t = Transform2D::new(
            Point2D::new(0.0, 0.0),
            std::f64::consts::PI,
            1.0, 1.0,
        );
        let result = t.apply_to_point(Point2D::new(1.0, 0.0));
        assert!((result.x - (-1.0)).abs() < 1e-12);
        assert!((result.y - 0.0).abs() < 1e-12);
    }

    // ------------------------------------------------------------------
    // 6. Translation shifts by vector
    // ------------------------------------------------------------------
    #[test]
    fn test_translation() {
        let t = Transform2D::new(
            Point2D::new(10.0, -5.0),
            0.0,
            1.0, 1.0,
        );
        let result = t.apply_to_point(Point2D::new(3.0, 7.0));
        assert_eq!(result, Point2D::new(13.0, 2.0));
    }

    // ------------------------------------------------------------------
    // 7. apply_to_bounds on non-empty box
    // ------------------------------------------------------------------
    #[test]
    fn test_apply_to_bounds_non_empty() {
        let bbox = make_bbox(0.0, 0.0, 10.0, 10.0);
        // Scale 2×, no rotation, translate (5, 5)
        let t = Transform2D::new(
            Point2D::new(5.0, 5.0),
            0.0,
            2.0, 2.0,
        );
        let result = t.apply_to_bounds(&bbox);
        // Corners: (0,0)→(5,5), (10,0)→(25,5), (0,10)→(5,25), (10,10)→(25,25)
        assert!((result.min.x - 5.0).abs() < 1e-12);
        assert!((result.min.y - 5.0).abs() < 1e-12);
        assert!((result.max.x - 25.0).abs() < 1e-12);
        assert!((result.max.y - 25.0).abs() < 1e-12);
    }

    // ------------------------------------------------------------------
    // 8. apply_to_bounds on empty box returns empty
    // ------------------------------------------------------------------
    #[test]
    fn test_apply_to_bounds_empty_input() {
        let empty = BoundingBox2D::empty();
        let t = Transform2D::new(
            Point2D::new(10.0, 10.0),
            FRAC_PI_2,
            2.0, 2.0,
        );
        let result = t.apply_to_bounds(&empty);
        assert!(result.is_empty());
    }

    // ------------------------------------------------------------------
    // 9. Transform order: scale → rotate → translate (not the inverse)
    // ------------------------------------------------------------------
    #[test]
    fn test_transform_order_srt() {
        // Use a transform where order matters:
        //   scale=2× each axis, rotation=45°, translate=(10, 0)
        // Starting point (1, 0):
        //   SRT: scale → (2,0), rotate → (≈1.414, ≈1.414), translate → (≈11.414, ≈1.414)
        //   RST (wrong): rotate → (≈0.707, ≈0.707), scale → (≈1.414, ≈1.414), translate → (≈11.414, ≈1.414)
        //   ... actually these two give same result for this specific input!
        // Let me use a better case where translation happens pre-rotation:
        // With translation=(10, 0), scale=2, rotation=π/2:
        //   SRT: scale (1,0)→(2,0), rotate→(0,2), translate→(10,2)
        //   STR: scale (1,0)→(2,0), translate→(12,0), rotate→(0,12)
        //   RST: rotate (1,0)→(0,1), scale→(0,2), translate→(10,2)
        //   RTS: rotate (1,0)→(0,1), translate→(10,1), scale→(20,2)
        //   TSR: translate (1,0)→(11,0), scale→(22,0), rotate→(0,22)
        //   TRS: translate (1,0)→(11,0), rotate→(0,11), scale→(0,22)
        let translate = Point2D::new(10.0, 0.0);
        let rotation = FRAC_PI_2;

        let t = Transform2D::new(translate, rotation, 2.0, 2.0);
        let result = t.apply_to_point(Point2D::new(1.0, 0.0));

        // SRT (correct): scale(1,0) → (2,0), rotate 90° → (0,2), translate → (10,2)
        assert!(
            (result.x - 10.0).abs() < 1e-12,
            "SRT: expected x≈10, got {}",
            result.x,
        );
        assert!(
            (result.y - 2.0).abs() < 1e-12,
            "SRT: expected y≈2, got {}",
            result.y,
        );
    }

    // ------------------------------------------------------------------
    // 10. Construct from new() matches manual field construction
    // ------------------------------------------------------------------
    #[test]
    fn test_new_constructor_round_trip() {
        let translate = Point2D::new(5.0, -3.0);
        let rotation = 0.75;

        let a = Transform2D::new(translate, rotation, 0.5, 2.0);
        let b = Transform2D {
            scale_x: 0.5,
            scale_y: 2.0,
            rotation: 0.75,
            translate_x: 5.0,
            translate_y: -3.0,
        };
        assert_eq!(a, b);
    }

    // ------------------------------------------------------------------
    // 11. Debug format
    // ------------------------------------------------------------------
    #[test]
    fn test_debug_format() {
        let t = Transform2D::IDENTITY;
        let s = format!("{:?}", t);
        assert!(s.contains("Transform2D"));
        assert!(s.contains("scale_x"));
    }
}
