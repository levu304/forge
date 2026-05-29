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
//! | `Selected` | Marker: entity is currently selected |
//! | `SnapTarget` | Marker: entity can be snapped to |

use crate::geometry::{BoundingBox2D, Point2D};
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
    /// Line color (RGBA).
    pub color: Color,
    /// Line width in screen-space pixels.
    pub width: f32,
}

impl LineData {
    /// Axis-aligned bounding box encompassing the line segment's endpoints.
    pub fn bounding_box(&self) -> BoundingBox2D {
        BoundingBox2D {
            min: Point2D::new(
                self.start.x.min(self.end.x),
                self.start.y.min(self.end.y),
            ),
            max: Point2D::new(
                self.start.x.max(self.end.x),
                self.start.y.max(self.end.y),
            ),
        }
    }
}

/// A circle defined by centre and radius.
#[derive(Debug, Clone, Copy)]
pub struct CircleData {
    /// Centre point (world-space absolute coordinates).
    pub center: Point2D,
    /// Radius in world units.
    pub radius: f64,
    /// Circle color (RGBA).
    pub color: Color,
    /// Outline width in screen-space pixels.
    pub width: f32,
}

impl CircleData {
    /// Axis-aligned bounding box encompassing the full circle.
    pub fn bounding_box(&self) -> BoundingBox2D {
        BoundingBox2D {
            min: Point2D::new(self.center.x - self.radius, self.center.y - self.radius),
            max: Point2D::new(self.center.x + self.radius, self.center.y + self.radius),
        }
    }
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
    /// Arc color (RGBA).
    pub color: Color,
    /// Outline width in screen-space pixels.
    pub width: f32,
}

impl ArcData {
    /// Axis-aligned bounding box encompassing the arc's full circle.
    ///
    /// This is a **conservative** estimate (same as the bounding box of the
    /// arc's complete circle). A tighter bound could be computed by checking
    /// which quadrant boundaries the arc sweeps through, but that is deferred
    /// to a future optimisation.
    pub fn bounding_box(&self) -> BoundingBox2D {
        BoundingBox2D {
            min: Point2D::new(self.center.x - self.radius, self.center.y - self.radius),
            max: Point2D::new(self.center.x + self.radius, self.center.y + self.radius),
        }
    }
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
    /// Polyline color (RGBA).
    pub color: Color,
    /// Line width in screen-space pixels.
    pub width: f32,
}

impl PolylineData {
    /// Axis-aligned bounding box covering all vertices.
    ///
    /// Returns a degenerate bounding box at the origin if there are no
    /// vertices (defensive — an empty polyline should never exist in
    /// normal operation).
    pub fn bounding_box(&self) -> BoundingBox2D {
        let mut iter = self.vertices.iter();
        let first = match iter.next() {
            Some(v) => *v,
            None => {
                return BoundingBox2D {
                    min: Point2D::new(0.0, 0.0),
                    max: Point2D::new(0.0, 0.0),
                };
            }
        };
        let mut min_x = first.x;
        let mut max_x = first.x;
        let mut min_y = first.y;
        let mut max_y = first.y;
        for v in iter {
            min_x = min_x.min(v.x);
            max_x = max_x.max(v.x);
            min_y = min_y.min(v.y);
            max_y = max_y.max(v.y);
        }
        BoundingBox2D {
            min: Point2D::new(min_x, min_y),
            max: Point2D::new(max_x, max_y),
        }
    }
}

/// Marker component: entity is currently selected.
#[derive(Debug, Clone, Copy)]
pub struct Selected;

/// Marker component: entity can be snapped to.
/// Reserved for v0.2.0 snap candidate filtering.
#[derive(Debug, Clone, Copy)]
pub struct SnapTarget;

/// Reference to a layer by ID.
///
/// Entities without this component are considered "ByLayer" — they inherit
/// their visual properties from the current/default layer.
#[derive(Debug, Clone, Copy)]
pub struct LayerRef(pub u32); // LayerId — will be replaced with proper type in Step 3

/// Determines how a visual property is sourced.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum PropertySource {
    #[default]
    ByLayer,
    ByBlock,
    Explicit,
}

/// A property value that can be inherited from Layer or Block.
#[derive(Debug, Clone, Copy, Default)]
pub enum PropertyValue<T: Copy> {
    #[default]
    ByLayer,
    ByBlock,
    Explicit(T),
}

/// Reference to a block definition by ID.
#[derive(Debug, Clone, Copy)]
pub struct BlockRef {
    pub definition: u32, // BlockId — will be replaced with proper type in Step 6
}
