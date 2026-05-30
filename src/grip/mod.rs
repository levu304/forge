//! Grip system — handle generation, hit-testing, and drag management.
//!
//! The grip system generates interactive drag handles for selected
//! entities, provides screen-space hit-testing for cursor picking, and
//! manages an active grip-drag state machine with undo support.

pub mod handle;
pub mod drag;
pub mod preview;

use handle::{GripHandle, GripType};
use hecs::World;

use crate::ecs::components::{
    ArcData, BlockRef, CircleData, LineData, PolylineData, Selected,
};
use crate::ecs::resources::CameraState;
use crate::geometry::Point2D;

// ---------------------------------------------------------------------------
// Arc point helper
// ---------------------------------------------------------------------------

/// Return the world-space point on an arc at the given angle.
///
/// `angle_deg` is measured counter-clockwise from the +X axis.
fn arc_point(center: Point2D, radius: f64, angle_deg: f64) -> Point2D {
    let rad = angle_deg.to_radians();
    Point2D::new(
        center.x + radius * rad.cos(),
        center.y + radius * rad.sin(),
    )
}

// ---------------------------------------------------------------------------
// GripSystem
// ---------------------------------------------------------------------------

/// The grip system: generates drag handles for selected entities and
/// manages active grip-drag operations.
#[derive(Debug, Clone)]
pub struct GripSystem {
    /// All currently visible grip handles (regenerated on selection change).
    pub handles: Vec<GripHandle>,
    /// Active drag operation, if any.
    pub active_drag: Option<drag::GripDragState>,
}

impl GripSystem {
    /// Create a new empty grip system.
    pub fn new() -> Self {
        Self {
            handles: Vec::new(),
            active_drag: None,
        }
    }

    /// Regenerate grip handles for all selected entities.
    ///
    /// Clears the existing handle list and generates new grips for every
    /// entity that carries the [`Selected`] marker component.
    ///
    /// | Entity type | Grips generated | Count |
    /// |-------------|-----------------|-------|
    /// | `LineData` | Start endpoint, End endpoint, Midpoint | 3 |
    /// | `CircleData` | Center, 4 Quadrants (0°, 90°, 180°, 270°) | 5 |
    /// | `ArcData` | Center, Start endpoint, End endpoint, Midpoint | 4 |
    /// | `PolylineData` | One `Vertex` grip per vertex | N |
    /// | `BlockRef` | Skipped entirely (deferred to v0.3.1) | 0 |
    pub fn regenerate(&mut self, world: &World) {
        self.handles.clear();

        let mut query = world.query::<&Selected>();
        for (entity, _selected) in query.iter() {
            // Skip block-reference entities.
            if world.get::<&BlockRef>(entity).is_ok() {
                continue;
            }

            // ── Line ──────────────────────────────────────────────
            if let Ok(line) = world.get::<&LineData>(entity) {
                // Start endpoint
                self.handles.push(GripHandle::new(
                    GripType::Endpoint,
                    line.start,
                    entity,
                    0,
                ));
                // End endpoint
                self.handles.push(GripHandle::new(
                    GripType::Endpoint,
                    line.end,
                    entity,
                    1,
                ));
                // Midpoint
                let mid = Point2D::new(
                    (line.start.x + line.end.x) / 2.0,
                    (line.start.y + line.end.y) / 2.0,
                );
                self.handles
                    .push(GripHandle::new(GripType::Midpoint, mid, entity, 0));
                continue;
            }

            // ── Circle ────────────────────────────────────────────
            if let Ok(circle) = world.get::<&CircleData>(entity) {
                // Center
                self.handles.push(GripHandle::new(
                    GripType::Center,
                    circle.center,
                    entity,
                    0,
                ));
                // 4 quadrant points: 0°, 90°, 180°, 270°
                for &angle_deg in &[0.0, 90.0, 180.0, 270.0] {
                    let qpos = arc_point(circle.center, circle.radius, angle_deg);
                    self.handles.push(GripHandle::new(
                        GripType::Quadrant,
                        qpos,
                        entity,
                        0,
                    ));
                }
                continue;
            }

            // ── Arc ───────────────────────────────────────────────
            if let Ok(arc) = world.get::<&ArcData>(entity) {
                // Center
                self.handles.push(GripHandle::new(
                    GripType::Center,
                    arc.center,
                    entity,
                    0,
                ));
                // Start point
                let start_pos = arc_point(arc.center, arc.radius, arc.start_angle);
                self.handles.push(GripHandle::new(
                    GripType::Endpoint,
                    start_pos,
                    entity,
                    0,
                ));
                // End point
                let end_pos = arc_point(arc.center, arc.radius, arc.end_angle);
                self.handles.push(GripHandle::new(
                    GripType::Endpoint,
                    end_pos,
                    entity,
                    1,
                ));
                // Midpoint (at mid-angle)
                // Handle wrap-around arcs: end < start means the arc
                // crosses the 0° boundary, so add 360° before averaging.
                let end_normalized = if arc.end_angle >= arc.start_angle {
                    arc.end_angle
                } else {
                    arc.end_angle + 360.0
                };
                let mid_angle = ((arc.start_angle + end_normalized) / 2.0) % 360.0;
                let mid_pos = arc_point(arc.center, arc.radius, mid_angle);
                self.handles.push(GripHandle::new(
                    GripType::Midpoint,
                    mid_pos,
                    entity,
                    0,
                ));
                continue;
            }

            // ── Polyline ──────────────────────────────────────────
            if let Ok(poly) = world.get::<&PolylineData>(entity) {
                for (vi, vertex) in poly.vertices.iter().enumerate() {
                    self.handles.push(GripHandle::new(
                        GripType::Vertex,
                        *vertex,
                        entity,
                        vi,
                    ));
                }
            }
        }
    }

    /// Screen-space hit test against all handles.
    ///
    /// Returns the index of the closest handle whose screen-space distance
    /// from `screen_pos` is ≤ `radius_px`, or [`None`] if no handle is
    /// within range.  When multiple handles are within range the closest
    /// (smallest distance) is returned.
    pub fn hit_test(
        &self,
        screen_pos: (f32, f32),
        camera: &CameraState,
        radius_px: f32,
    ) -> Option<usize> {
        let screen_point = Point2D::new(screen_pos.0 as f64, screen_pos.1 as f64);
        let radius_f64 = radius_px as f64;

        self.handles
            .iter()
            .enumerate()
            .filter_map(|(i, handle)| {
                let screen_handle = camera.world_to_screen(handle.position);
                let handle_point =
                    Point2D::new(screen_handle.0 as f64, screen_handle.1 as f64);
                let dist = screen_point.distance(handle_point);
                if dist <= radius_f64 {
                    Some((i, dist))
                } else {
                    None
                }
            })
            .min_by(|a, b| {
                a.1.partial_cmp(&b.1)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(i, _)| i)
    }

    /// Returns `true` if there are no handles currently generated.
    pub fn is_empty(&self) -> bool {
        self.handles.is_empty()
    }

    /// Returns the number of handles.
    pub fn len(&self) -> usize {
        self.handles.len()
    }
}

impl Default for GripSystem {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::components::*;
    use crate::geometry::GEOMETRIC_EPSILON;
    use crate::util::Color;
    use hecs::World;

    // ── helpers ─────────────────────────────────────────────────────

    fn default_camera() -> CameraState {
        CameraState {
            target: Point2D::new(0.0, 0.0),
            zoom: 1.0,
            viewport_size: (800, 600),
            clear_color: Color::default(),
        }
    }

    fn make_line(world: &mut World) -> hecs::Entity {
        world.spawn((
            LineData {
                start: Point2D::new(0.0, 0.0),
                end: Point2D::new(10.0, 10.0),
                color: Color::WHITE,
                width: 1.0,
            },
            Selected,
        ))
    }

    fn make_line_no_selected(world: &mut World) -> hecs::Entity {
        world.spawn((
            LineData {
                start: Point2D::new(0.0, 0.0),
                end: Point2D::new(10.0, 10.0),
                color: Color::WHITE,
                width: 1.0,
            },
        ))
    }

    fn make_circle(world: &mut World) -> hecs::Entity {
        world.spawn((
            CircleData {
                center: Point2D::new(5.0, 5.0),
                radius: 3.0,
                color: Color::WHITE,
                width: 1.0,
            },
            Selected,
        ))
    }

    fn make_arc(world: &mut World) -> hecs::Entity {
        world.spawn((
            ArcData {
                center: Point2D::new(0.0, 0.0),
                radius: 5.0,
                start_angle: 0.0,
                end_angle: 90.0,
                color: Color::WHITE,
                width: 1.0,
            },
            Selected,
        ))
    }

    fn make_polyline(world: &mut World, n: usize) -> hecs::Entity {
        let vertices: Vec<Point2D> = (0..n)
            .map(|i| Point2D::new(i as f64 * 10.0, 0.0))
            .collect();
        world.spawn((
            PolylineData {
                vertices,
                closed: false,
                color: Color::WHITE,
                width: 1.0,
            },
            Selected,
        ))
    }

    fn make_block_ref(world: &mut World) -> hecs::Entity {
        world.spawn((
            BlockRef { definition: 1 },
            Selected,
        ))
    }

    // ── regeneration count per entity type ──────────────────────────

    #[test]
    fn test_regenerate_line_three_handles() {
        let mut world = World::new();
        make_line(&mut world);

        let mut grip = GripSystem::new();
        grip.regenerate(&world);

        assert_eq!(grip.len(), 3);
        // First two are Endpoints, third is Midpoint
        assert_eq!(grip.handles[0].grip_type, GripType::Endpoint);
        assert_eq!(grip.handles[1].grip_type, GripType::Endpoint);
        assert_eq!(grip.handles[2].grip_type, GripType::Midpoint);
    }

    #[test]
    fn test_regenerate_circle_five_handles() {
        let mut world = World::new();
        make_circle(&mut world);

        let mut grip = GripSystem::new();
        grip.regenerate(&world);

        assert_eq!(grip.len(), 5);
        // Center + 4 Quadrants
        assert_eq!(grip.handles[0].grip_type, GripType::Center);
        for i in 1..5 {
            assert_eq!(grip.handles[i].grip_type, GripType::Quadrant);
        }
    }

    #[test]
    fn test_regenerate_arc_four_handles() {
        let mut world = World::new();
        make_arc(&mut world);

        let mut grip = GripSystem::new();
        grip.regenerate(&world);

        assert_eq!(grip.len(), 4);
        assert_eq!(grip.handles[0].grip_type, GripType::Center);
        assert_eq!(grip.handles[1].grip_type, GripType::Endpoint);
        assert_eq!(grip.handles[2].grip_type, GripType::Endpoint);
        assert_eq!(grip.handles[3].grip_type, GripType::Midpoint);
    }

    #[test]
    fn test_regenerate_polyline_n_handles() {
        let mut world = World::new();
        make_polyline(&mut world, 4);

        let mut grip = GripSystem::new();
        grip.regenerate(&world);

        assert_eq!(grip.len(), 4);
        for handle in &grip.handles {
            assert_eq!(handle.grip_type, GripType::Vertex);
        }
        // vertex indices should be 0, 1, 2, 3
        for (i, handle) in grip.handles.iter().enumerate() {
            assert_eq!(handle.vertex_index, i);
        }
    }

    #[test]
    fn test_regenerate_multiple_entities() {
        let mut world = World::new();
        make_line(&mut world);
        make_circle(&mut world);

        let mut grip = GripSystem::new();
        grip.regenerate(&world);

        assert_eq!(grip.len(), 3 + 5);
    }

    #[test]
    fn test_regenerate_no_selected_entities() {
        let mut world = World::new();
        make_line_no_selected(&mut world); // no Selected marker

        let mut grip = GripSystem::new();
        grip.regenerate(&world);

        assert!(grip.is_empty());
    }

    #[test]
    fn test_regenerate_skips_block_ref() {
        let mut world = World::new();
        make_block_ref(&mut world);

        let mut grip = GripSystem::new();
        grip.regenerate(&world);

        assert!(grip.is_empty(), "BlockRef entities should be skipped");
    }

    #[test]
    fn test_regenerate_line_endpoint_positions() {
        let mut world = World::new();
        make_line(&mut world);

        let mut grip = GripSystem::new();
        grip.regenerate(&world);

        assert_eq!(grip.handles[0].position, Point2D::new(0.0, 0.0));
        assert_eq!(grip.handles[1].position, Point2D::new(10.0, 10.0));
        assert_eq!(grip.handles[2].position, Point2D::new(5.0, 5.0));
    }

    #[test]
    fn test_regenerate_circle_quadrant_positions() {
        let mut world = World::new();
        let entity = make_circle(&mut world);
        let _circle = *world.get::<&CircleData>(entity).unwrap();

        let mut grip = GripSystem::new();
        grip.regenerate(&world);

        // Quadrant 0°: center.x + radius, center.y
        assert_eq!(
            grip.handles[1].position,
            Point2D::new(8.0, 5.0),
            "0° quadrant"
        );
        // Quadrant 90°: center.x, center.y + radius
        assert_eq!(
            grip.handles[2].position,
            Point2D::new(5.0, 8.0),
            "90° quadrant"
        );
        // Quadrant 180°: center.x - radius, center.y
        assert_eq!(
            grip.handles[3].position,
            Point2D::new(2.0, 5.0),
            "180° quadrant"
        );
        // Quadrant 270°: center.x, center.y - radius
        // (floating point cos(270°) may be a tiny epsilon from zero)
        assert!(
            (grip.handles[4].position.x - 5.0).abs() < GEOMETRIC_EPSILON,
            "270° quadrant x"
        );
        assert!(
            (grip.handles[4].position.y - 2.0).abs() < GEOMETRIC_EPSILON,
            "270° quadrant y"
        );
    }

    #[test]
    fn test_regenerate_arc_endpoint_positions() {
        let mut world = World::new();
        make_arc(&mut world);

        let mut grip = GripSystem::new();
        grip.regenerate(&world);

        // Arc: center=(0,0), radius=5, start=0°, end=90°
        // start point: (5, 0), end point: (0, 5), mid: (≈3.5355, ≈3.5355)
        assert_eq!(grip.handles[1].position, Point2D::new(5.0, 0.0), "start");
        // End point at 90°: (cos(90°)*5, sin(90°)*5) — floating-point
        // cos(π/2) may be a tiny epsilon from zero
        assert!(
            (grip.handles[2].position.x - 0.0).abs() < GEOMETRIC_EPSILON,
            "end x"
        );
        assert!(
            (grip.handles[2].position.y - 5.0).abs() < GEOMETRIC_EPSILON,
            "end y"
        );
        // Midpoint at 45°: (5*cos45°, 5*sin45°) ≈ (3.5355, 3.5355)
        let expected = 5.0 * std::f64::consts::FRAC_1_SQRT_2; // 5/√2
        assert!(
            (grip.handles[3].position.x - expected).abs() < 1e-10,
            "mid x"
        );
        assert!(
            (grip.handles[3].position.y - expected).abs() < 1e-10,
            "mid y"
        );
    }

    #[test]
    fn test_regenerate_arc_wrap_around_midpoint() {
        // Arc crossing the 0° boundary: start=350°, end=10°
        // Midpoint should be near 0°/360°, NOT (350+10)/2 = 180°.
        let mut world = World::new();
        let _entity = world.spawn((
            ArcData {
                center: Point2D::new(0.0, 0.0),
                radius: 5.0,
                start_angle: 350.0,
                end_angle: 10.0,
                color: Color::WHITE,
                width: 1.0,
            },
            Selected,
        ));

        let mut grip = GripSystem::new();
        grip.regenerate(&world);

        // Midpoint is the 4th handle (index 3)
        let mid = grip.handles[3].position;
        // At radius=5, angle ~0° → position ≈ (5, 0)
        assert!(
            (mid.x - 5.0).abs() < 1e-10,
            "midpoint x expected ~5.0, got {}",
            mid.x
        );
        assert!(
            (mid.y - 0.0).abs() < 1e-10,
            "midpoint y expected ~0.0, got {}",
            mid.y
        );
    }

    #[test]
    fn test_regenerate_clears_previous() {
        let mut world = World::new();
        make_line(&mut world);

        let mut grip = GripSystem::new();

        // First regeneration
        grip.regenerate(&world);
        assert_eq!(grip.len(), 3);

        // Second regeneration should clear and re-generate (same count)
        grip.regenerate(&world);
        assert_eq!(grip.len(), 3);

        // Remove selection and regenerate → should be empty
        world.clear();
        grip.regenerate(&world);
        assert!(grip.is_empty());
    }

    // ── hit_test ────────────────────────────────────────────────────

    #[test]
    fn test_hit_test_finds_nearest_handle() {
        let mut world = World::new();
        make_line(&mut world); // line from (0,0) to (10,10)

        let mut grip = GripSystem::new();
        grip.regenerate(&world);

        let camera = default_camera();

        // World (0,0) → screen (400, 300) at centre for camera (0,0)
        // Actually: world_to_screen(0,0) with target=(0,0), zoom=1, viewport=(800,600)
        //   sx = 400 + 0*1 = 400
        //   sy = 300 - 0*1 = 300
        // World (10,10) → screen (410, 290)
        // World (5,5) → screen (405, 295)

        // Hit-test near the start handle (0,0) → screen (400, 300)
        let idx = grip.hit_test((400.0, 300.0), &camera, 10.0);
        assert_eq!(idx, Some(0), "should hit start endpoint");

        // Hit-test near the end handle (10,10) → screen (410, 290)
        let idx = grip.hit_test((410.0, 290.0), &camera, 10.0);
        assert_eq!(idx, Some(1), "should hit end endpoint");
    }

    #[test]
    fn test_hit_test_returns_closest_when_multiple_in_range() {
        let mut world = World::new();
        make_line(&mut world);

        let mut grip = GripSystem::new();
        grip.regenerate(&world);

        let camera = default_camera();

        // Screen position closer to start (400,300) than end (410,290)
        // (401, 300) should be closest to start
        let idx = grip.hit_test((401.0, 300.0), &camera, 10.0);
        assert_eq!(idx, Some(0), "should return closest (start)");
    }

    #[test]
    fn test_hit_test_returns_none_when_out_of_range() {
        let mut world = World::new();
        make_line(&mut world);

        let mut grip = GripSystem::new();
        grip.regenerate(&world);

        let camera = default_camera();

        // Far from any handle
        let idx = grip.hit_test((0.0, 0.0), &camera, 10.0);
        assert!(idx.is_none(), "should be out of range");
    }

    #[test]
    fn test_hit_test_empty_handles() {
        let grip = GripSystem::new();
        let camera = default_camera();

        assert!(grip.hit_test((400.0, 300.0), &camera, 10.0).is_none());
    }

    #[test]
    fn test_hit_test_with_different_zoom() {
        let mut world = World::new();
        make_line(&mut world);

        let mut grip = GripSystem::new();
        grip.regenerate(&world);

        let camera = CameraState {
            target: Point2D::new(0.0, 0.0),
            zoom: 2.0, // zoomed in
            viewport_size: (800, 600),
            clear_color: Color::default(),
        };

        // With zoom=2, world (0,0) → screen (400, 300) still (target at centre)
        let idx = grip.hit_test((400.0, 300.0), &camera, 10.0);
        assert_eq!(idx, Some(0));
    }

    // ── is_empty / len / new ────────────────────────────────────────

    #[test]
    fn test_new_grip_is_empty() {
        let grip = GripSystem::new();
        assert!(grip.is_empty());
        assert_eq!(grip.len(), 0);
    }

    #[test]
    fn test_default_grip_is_empty() {
        let grip = GripSystem::default();
        assert!(grip.is_empty());
    }
}
