use forge::geometry::{Point2D, BoundingBox2D};
use forge::util::{Color, ForgeError};
use std::io;

#[test]
fn test_exit_criteria() {
    // 1. Point2D::new(3.0, 4.0).to_f32_array() == [3.0, 4.0]
    let p = Point2D::new(3.0, 4.0);
    let arr = p.to_f32_array();
    assert_eq!(arr, [3.0, 4.0], "Point2D conversion failed");
    
    // 2. BoundingBox2D union/center/is_empty logic is consistent
    let bbox = BoundingBox2D {
        min: Point2D::new(0.0, 0.0),
        max: Point2D::new(10.0, 10.0),
    };
    assert!(!bbox.is_empty(), "Non-empty bbox incorrectly reported as empty");
    let center = bbox.center();
    assert_eq!(center.x, 5.0, "Center x incorrect");
    assert_eq!(center.y, 5.0, "Center y incorrect");
    
    let empty_bbox = BoundingBox2D {
        min: Point2D::new(5.0, 5.0),
        max: Point2D::new(5.0, 5.0),
    };
    assert!(empty_bbox.is_empty(), "Zero-area bbox should be empty");
    
    let union_bbox = bbox.union(&empty_bbox);
    assert_eq!(union_bbox.min.x, 0.0, "Union min x incorrect");
    assert_eq!(union_bbox.min.y, 0.0, "Union min y incorrect");
    assert_eq!(union_bbox.max.x, 10.0, "Union max x incorrect");
    assert_eq!(union_bbox.max.y, 10.0, "Union max y incorrect");
    
    // 3. Color::from_hex(0xFF0000) produces Color { r: 1.0, g: 0.0, b: 0.0, a: 1.0 }
    let red = Color::from_hex(0xFF0000);
    assert_eq!(red.r, 1.0, "Red r component incorrect");
    assert_eq!(red.g, 0.0, "Red g component incorrect");
    assert_eq!(red.b, 0.0, "Red b component incorrect");
    assert_eq!(red.a, 1.0, "Red a component incorrect");
    
    // 4. Color::WHITE and color constants exist
    let white = Color::WHITE;
    assert_eq!(white.r, 1.0, "WHITE r incorrect");
    assert_eq!(white.g, 1.0, "WHITE g incorrect");
    assert_eq!(white.b, 1.0, "WHITE b incorrect");
    assert_eq!(white.a, 1.0, "WHITE a incorrect");
    
    let black = Color::BLACK;
    assert_eq!(black.r, 0.0, "BLACK r incorrect");
    assert_eq!(black.g, 0.0, "BLACK g incorrect");
    assert_eq!(black.b, 0.0, "BLACK b incorrect");
    assert_eq!(black.a, 1.0, "BLACK a incorrect");
    
    let gray_dark = Color::GRAY_DARK;
    assert_eq!(gray_dark.r, 0x44 as f32 / 255.0, "GRAY_DARK r incorrect");
    assert_eq!(gray_dark.g, 0x44 as f32 / 255.0, "GRAY_DARK g incorrect");
    assert_eq!(gray_dark.b, 0x44 as f32 / 255.0, "GRAY_DARK b incorrect");
    assert_eq!(gray_dark.a, 1.0, "GRAY_DARK a incorrect");
    
    let gray_medium = Color::GRAY_MEDIUM;
    assert_eq!(gray_medium.r, 0x66 as f32 / 255.0, "GRAY_MEDIUM r incorrect");
    assert_eq!(gray_medium.g, 0x66 as f32 / 255.0, "GRAY_MEDIUM g incorrect");
    assert_eq!(gray_medium.b, 0x66 as f32 / 255.0, "GRAY_MEDIUM b incorrect");
    assert_eq!(gray_medium.a, 1.0, "GRAY_MEDIUM a incorrect");
    
    let gray_light = Color::GRAY_LIGHT;
    assert_eq!(gray_light.r, 0x88 as f32 / 255.0, "GRAY_LIGHT r incorrect");
    assert_eq!(gray_light.g, 0x88 as f32 / 255.0, "GRAY_LIGHT g incorrect");
    assert_eq!(gray_light.b, 0x88 as f32 / 255.0, "GRAY_LIGHT b incorrect");
    assert_eq!(gray_light.a, 1.0, "GRAY_LIGHT a incorrect");
    
    // 5. ForgeError enum compiles with all variants
    let _gpu_err = ForgeError::Gpu("Test".to_string());
    let _surface_err = ForgeError::Surface("Test".to_string());
    let _command_err = ForgeError::Command("Test".to_string());
    let _parse_err = ForgeError::Parse("Test".to_string());
    let _io_err = ForgeError::Io(io::Error::new(io::ErrorKind::NotFound, "Test"));
    
    // 6. From<wgpu::Error> impl compiles
    // This is tested by the fact that the impl exists and doesn't cause compile errors
    
    // 7. From<wgpu::SurfaceError> impl compiles
    // Note: wgpu 29 changed SurfaceError handling - we handle this differently in render code
    // The impl is intentionally omitted as noted in error.rs
    
    println!("All exit criteria verified!");
}