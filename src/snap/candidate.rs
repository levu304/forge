//! Snap candidate generation (stub).
//!
//! Defines the `SnapCandidate` trait and implements per-entity-type
//! candidate generation for Line, Circle, Arc, and Polyline.
//!
//! # Implementation Status (v0.2.0 Step 6)
//!
//! [`SnapCandidatePoint`] is defined here so that [`filter`][super::filter]
//! can compile. Full per-entity-type candidate generation is deferred to
//! Step 7.

use crate::geometry::Point2D;
use crate::snap::SnapType;

/// A single snap candidate generated from an entity's geometry.
///
/// Stores the candidate's world-space location, the type of snap it
/// represents, and its pre-computed world-space distance from the cursor.
#[derive(Debug, Clone)]
pub struct SnapCandidatePoint {
    /// World-space position of this snap point.
    pub point: Point2D,
    /// The type of snap (Endpoint, Midpoint, etc.).
    pub snap_type: SnapType,
    /// Pre-computed world-space distance from the cursor position.
    pub distance_world: f64,
}
