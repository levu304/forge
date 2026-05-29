/// RGBA color stored as f32 values in [0, 1] range.
#[derive(Debug, Clone, Copy, PartialEq, Default, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    /// Create from 24-bit hex value (0xRRGGBB). High byte is ignored; alpha is always 1.0. Use From<[f32; 4]> for custom alpha.
    pub const fn from_hex(hex: u32) -> Self {
        Self {
            r: (((hex >> 16) & 0xFF) as f32) / 255.0f32,
            g: (((hex >> 8) & 0xFF) as f32) / 255.0f32,
            b: ((hex & 0xFF) as f32) / 255.0f32,
            a: 1.0,
        }
    }

    pub const WHITE: Self = Self::from_hex(0xFFFFFF);
    pub const BLACK: Self = Self::from_hex(0x000000);
    pub const GRAY_DARK: Self = Self::from_hex(0x444444);
    pub const GRAY_MEDIUM: Self = Self::from_hex(0x666666);
    pub const GRAY_LIGHT: Self = Self::from_hex(0x888888);
}

// ---------------------------------------------------------------------------
// Color ↔ egui::Color32 conversions
// ---------------------------------------------------------------------------

impl From<Color> for egui::Color32 {
    fn from(c: Color) -> Self {
        egui::Color32::from_rgba_unmultiplied(
            (c.r * 255.0).round() as u8,
            (c.g * 255.0).round() as u8,
            (c.b * 255.0).round() as u8,
            (c.a * 255.0).round() as u8,
        )
    }
}

impl From<egui::Color32> for Color {
    fn from(c: egui::Color32) -> Self {
        let [r, g, b, a] = c.to_srgba_unmultiplied();
        Self {
            r: r as f32 / 255.0,
            g: g as f32 / 255.0,
            b: b as f32 / 255.0,
            a: a as f32 / 255.0,
        }
    }
}