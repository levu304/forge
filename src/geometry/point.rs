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