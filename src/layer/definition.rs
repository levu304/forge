//! Layer struct (stub)

/// A named layer that groups entities sharing visual properties.
///
/// Full implementation in Step 3.
#[derive(Debug, Clone, Default)]
pub struct Layer {
    /// Unique identifier for this layer.
    pub id: u32,
    /// Human-readable layer name.
    pub name: String,
}
