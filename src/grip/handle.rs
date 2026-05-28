//! GripHandle, GripType (stub)

/// The type of a grip handle, determining its visual appearance and
/// behaviour during drag operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GripType {
    /// Corner/endpoint grip (square).
    Endpoint,
    /// Midpoint grip (triangle).
    Midpoint,
    /// Center grip (circle).
    Center,
}

impl Default for GripType {
    fn default() -> Self {
        Self::Endpoint
    }
}

/// A single grip handle on an entity, used for direct manipulation.
///
/// Full implementation in Step 5.
#[derive(Debug, Clone, Copy, Default)]
pub struct GripHandle {
    /// The type of this grip handle.
    pub grip_type: GripType,
    /// World-space position of the grip.
    pub position: (f64, f64),
}
