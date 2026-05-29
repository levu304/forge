//! Core layer types: [`LayerId`], [`Linetype`], [`Layer`].

use std::fmt;

use crate::util::Color;

/// Unique identifier for a layer.
///
/// The default layer always has ID 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LayerId(pub u32);

impl LayerId {
    /// ID of the built-in default layer.
    pub const DEFAULT: Self = Self(0);
}

/// Line-style pattern used when rendering geometry on a layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Linetype {
    /// Continuous solid line.
    Solid,
    /// Short evenly spaced dots.
    Dotted,
    /// Regularly spaced dashes.
    Dashed,
    /// Alternating dash-dot pattern.
    DashDot,
    /// Double-dash border pattern.
    Border,
    /// Long-dash center-line pattern.
    Center,
}

impl Default for Linetype {
    fn default() -> Self {
        Self::Solid
    }
}

/// Stipple bitmask values used by the renderer for each linetype.
impl From<Linetype> for u32 {
    fn from(lt: Linetype) -> Self {
        match lt {
            Linetype::Solid => 0xFFFF,
            Linetype::Dotted => 0x3333,
            Linetype::Dashed => 0x0F0F,
            Linetype::DashDot => 0x1B1B,
            Linetype::Border => 0xAAAA,
            Linetype::Center => 0xCCCC,
        }
    }
}

impl fmt::Display for Linetype {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Solid => write!(f, "solid"),
            Self::Dotted => write!(f, "dotted"),
            Self::Dashed => write!(f, "dashed"),
            Self::DashDot => write!(f, "dashdot"),
            Self::Border => write!(f, "border"),
            Self::Center => write!(f, "center"),
        }
    }
}

/// A named layer that groups entities sharing visual properties.
///
/// Each layer has a unique ID, a display name, and rendering attributes
/// such as colour, linetype, linewidth, and visibility toggles.
#[derive(Debug, Clone)]
pub struct Layer {
    /// Unique identifier.
    pub id: LayerId,
    /// Human-readable layer name.
    pub name: String,
    /// RGBA colour used for geometry on this layer.
    pub color: Color,
    /// Line-style pattern.
    pub linetype: Linetype,
    /// Stroke width in drawing units.
    pub linewidth: f32,
    /// Whether geometry on this layer is visible.
    pub visible: bool,
    /// Whether editing of geometry on this layer is prevented.
    pub locked: bool,
    /// Whether the layer is frozen (invisible and excluded from regeneration).
    pub frozen: bool,
}

impl Default for Layer {
    fn default() -> Self {
        Self {
            id: LayerId::DEFAULT,
            name: String::new(),
            color: Color::WHITE,
            linetype: Linetype::default(),
            linewidth: 0.25,
            visible: true,
            locked: false,
            frozen: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layer_id_default_is_zero() {
        assert_eq!(LayerId::DEFAULT, LayerId(0));
    }

    #[test]
    fn layer_id_copy_and_eq() {
        let a = LayerId(42);
        let b = a; // Copy
        assert_eq!(a, b);
    }

    #[test]
    fn linetype_default_is_solid() {
        assert_eq!(Linetype::default(), Linetype::Solid);
    }

    #[test]
    fn linetype_display() {
        assert_eq!(Linetype::Solid.to_string(), "solid");
        assert_eq!(Linetype::Dotted.to_string(), "dotted");
        assert_eq!(Linetype::Dashed.to_string(), "dashed");
        assert_eq!(Linetype::DashDot.to_string(), "dashdot");
        assert_eq!(Linetype::Border.to_string(), "border");
        assert_eq!(Linetype::Center.to_string(), "center");
    }

    #[test]
    fn linetype_to_u32_bitmask() {
        assert_eq!(u32::from(Linetype::Solid), 0xFFFF);
        assert_eq!(u32::from(Linetype::Dotted), 0x3333);
        assert_eq!(u32::from(Linetype::Dashed), 0x0F0F);
        assert_eq!(u32::from(Linetype::DashDot), 0x1B1B);
        assert_eq!(u32::from(Linetype::Border), 0xAAAA);
        assert_eq!(u32::from(Linetype::Center), 0xCCCC);
    }

    #[test]
    fn layer_default_values() {
        let layer = Layer::default();
        assert_eq!(layer.id, LayerId(0));
        assert_eq!(layer.color, Color::WHITE);
        assert_eq!(layer.linetype, Linetype::Solid);
        assert!((layer.linewidth - 0.25).abs() < f32::EPSILON);
        assert!(layer.visible);
        assert!(!layer.locked);
        assert!(!layer.frozen);
    }
}
