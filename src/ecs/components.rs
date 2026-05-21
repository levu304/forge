//! ECS component types for geometric primitives and markers.
//!
//! All types in this module are `Send + Sync + 'static`, which satisfies
//! hecs 0.10's requirement for component types. **hecs does not provide a
//! `Component` derive macro** — any `Send + Sync + 'static` type is
//! automatically a valid component.
//!
//! # Components
//!
//! | Component | Description |
//! |-----------|-------------|
//! | `Renderable` | Marker: entity participates in rendering |
//! | `Position` | World-space origin (placeholder for v0.2.0+ transforms) |
//! | `LineData` | 2D line segment with color and width |
//! | `CircleData` | Circle with center, radius, color, width |
//! | `ArcData` | Arc with center, radius, angles (degrees, CCW from +X) |
//! | `PolylineData` | Ordered vertex sequence with optional closed flag |

use crate::geometry::Point2D;
use crate::util::Color;

/// Marker component indicating that an entity participates in rendering.
///
/// Any entity with this component will be picked up by the entity renderer
/// and drawn in the geometry pass.
#[derive(Debug, Clone, Copy)]
pub struct Renderable;

/// World-space position (origin for local geometry).
///
/// ## Design Note (v0.1.0)
/// `Position` is included for future use (v0.2.0+ transforms, instancing).
/// In v0.1.0, geometry data (`LineData`, `CircleData`, etc.) stores **absolute
/// coordinates** for simplicity. `Position` is **not** read by entity renderers
/// yet; it is carried as a placeholder so transform-aware code can be added
/// without schema migration.
#[derive(Debug, Clone, Copy)]
pub struct Position(pub Point2D);

/// A 2D line segment defined by start and end points.
#[derive(Debug, Clone, Copy)]
pub struct LineData {
    /// Start point of the segment (world-space absolute coordinates).
    pub start: Point2D,
    /// End point of the segment (world-space absolute coordinates).
    pub end: Point2D,
    /// Line colour (RGBA).
    pub color: Color,
    /// Line width in screen-space pixels.
    pub width: f32,
}

/// A circle defined by centre and radius.
#[derive(Debug, Clone, Copy)]
pub struct CircleData {
    /// Centre point (world-space absolute coordinates).
    pub center: Point2D,
    /// Radius in world units.
    pub radius: f64,
    /// Circle colour (RGBA).
    pub color: Color,
    /// Outline width in screen-space pixels.
    pub width: f32,
}

/// An arc defined by centre, radius, and start/end angles.
///
/// Angles are in **degrees** with 0.0 pointing along the +X axis,
/// increasing counter-clockwise. The arc sweeps from `start_angle`
/// to `end_angle` (CCW). A full sweep (360°) occurs when
/// `end_angle - start_angle ≥ 360.0`.
#[derive(Debug, Clone, Copy)]
pub struct ArcData {
    /// Centre point (world-space absolute coordinates).
    pub center: Point2D,
    /// Radius in world units.
    pub radius: f64,
    /// Start angle in degrees (0.0 = +X axis).
    pub start_angle: f64,
    /// End angle in degrees. Must be > `start_angle` for a valid CCW arc.
    /// If equal to `start_angle`, the spawner should reject it as a zero-sweep arc.
    pub end_angle: f64,
    /// Arc colour (RGBA).
    pub color: Color,
    /// Outline width in screen-space pixels.
    pub width: f32,
}

/// An ordered sequence of vertices forming a polyline or polygon.
///
/// Derives `Clone` but not `Copy` because the vertex list is heap-allocated.
///
/// When `closed` is `true`, a closing segment from the last vertex back
/// to the first is implied.
#[derive(Debug, Clone)]
pub struct PolylineData {
    /// Ordered vertex positions (world-space absolute coordinates).
    pub vertices: Vec<Point2D>,
    /// If `true`, a closing segment from last vertex to first vertex is drawn.
    pub closed: bool,
    /// Polyline colour (RGBA).
    pub color: Color,
    /// Line width in screen-space pixels.
    pub width: f32,
}
