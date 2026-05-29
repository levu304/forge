use forge::geometry::{Point2D, BoundingBox2D};
use forge::util::Color;

#[test]
fn test_point2d() {
    let p = Point2D::new(3.0, 4.0);
    let arr = p.to_f32_array();
    assert_eq!(arr, [3.0, 4.0]);
}

#[test]
fn test_color() {
    let white = Color::WHITE;
    assert_eq!(white.r, 1.0);
    assert_eq!(white.g, 1.0);
    assert_eq!(white.b, 1.0);
    assert_eq!(white.a, 1.0);
    
    let black = Color::BLACK;
    assert_eq!(black.r, 0.0);
    assert_eq!(black.g, 0.0);
    assert_eq!(black.b, 0.0);
    assert_eq!(black.a, 1.0);
    
    let red = Color::from_hex(0xFF0000);
    assert_eq!(red.r, 1.0);
    assert_eq!(red.g, 0.0);
    assert_eq!(red.b, 0.0);
    assert_eq!(red.a, 1.0);
    
    let gray_dark = Color::GRAY_DARK;
    assert_eq!(gray_dark.r, 0x44 as f32 / 255.0);
    assert_eq!(gray_dark.g, 0x44 as f32 / 255.0);
    assert_eq!(gray_dark.b, 0x44 as f32 / 255.0);
    assert_eq!(gray_dark.a, 1.0);
}

#[test]
fn test_bounding_box() {
    let bbox = BoundingBox2D {
        min: Point2D::new(0.0, 0.0),
        max: Point2D::new(10.0, 10.0),
    };
    assert!(!bbox.is_empty());
    let center = bbox.center();
    assert_eq!(center.x, 5.0);
    assert_eq!(center.y, 5.0);
    
    let empty_bbox = BoundingBox2D {
        min: Point2D::new(5.0, 5.0),
        max: Point2D::new(5.0, 5.0),
    };
    assert!(empty_bbox.is_empty());
    
    let union_bbox = bbox.union(&empty_bbox);
    assert_eq!(union_bbox.min.x, 0.0);
    assert_eq!(union_bbox.min.y, 0.0);
    assert_eq!(union_bbox.max.x, 10.0);
    assert_eq!(union_bbox.max.y, 10.0);
    
    let bbox2 = BoundingBox2D {
        min: Point2D::new(-5.0, -5.0),
        max: Point2D::new(15.0, 8.0),
    };
    let union_bbox2 = bbox.union(&bbox2);
    assert_eq!(union_bbox2.min.x, -5.0);
    assert_eq!(union_bbox2.min.y, -5.0);
    assert_eq!(union_bbox2.max.x, 15.0);
    assert_eq!(union_bbox2.max.y, 10.0);
}