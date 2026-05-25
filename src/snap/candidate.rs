//! Snap candidate generation.
//!
//! Defines the [`SnapCandidate`] trait and implements per-entity-type
//! candidate generation for `LineData`, `CircleData`, `ArcData`, and
//! `PolylineData`.
//!
//! # Organisation
//!
//! 1. [`SnapCandidatePoint`] — the output struct.
//! 2. [`SnapCandidate`] trait — the abstraction each geometry type
//!    implements.
//! 3. Per-entity-type impls — one for each geometric primitive in
//!    [`crate::ecs::components`].

use crate::ecs::components::{ArcData, CircleData, LineData, PolylineData};
use crate::geometry::Point2D;
use crate::snap::SnapType;

// ---------------------------------------------------------------------------
// SnapCandidatePoint
// ---------------------------------------------------------------------------

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

impl SnapCandidatePoint {
    /// Create a new candidate, computing `distance_world` automatically.
    pub fn new(point: Point2D, snap_type: SnapType, cursor: Point2D) -> Self {
        Self {
            point,
            snap_type,
            distance_world: cursor.distance(point),
        }
    }
}

// ---------------------------------------------------------------------------
// SnapCandidate trait
// ---------------------------------------------------------------------------

/// Trait for generating snap points from entity geometry.
///
/// Each geometric primitive (line, circle, arc, polyline) implements this
/// trait to produce the snap candidates relevant to its shape.
pub trait SnapCandidate {
    /// Generate snap points for the given world-point cursor position,
    /// filtered to the provided set of active snap types.
    ///
    /// Implementations should only produce candidates for snap types
    /// present in `active_types`. Candidates should include the
    /// pre-computed world-space distance from `world_point` via
    /// [`SnapCandidatePoint::new`].
    fn generate_snap_points(&self, world_point: Point2D, active_types: &[SnapType]) -> Vec<SnapCandidatePoint>;
}

// ---------------------------------------------------------------------------
// Helper: check whether a snap type is in the active set
// ---------------------------------------------------------------------------

fn is_active(active_types: &[SnapType], target: SnapType) -> bool {
    active_types.contains(&target)
}

// ---------------------------------------------------------------------------
// Helper: line-segment projection (shared by LineData and PolylineData)
// ---------------------------------------------------------------------------

/// Project `world_point` onto the line through `a`–`b`.
///
/// * `clamped` — clamp `t` to `[0, 1]` (segment) or leave unbounded (infinite line).
fn project_onto_line(world_point: Point2D, a: Point2D, b: Point2D, clamped: bool) -> Option<Point2D> {
    let ab = b - a;
    let denom = ab.x * ab.x + ab.y * ab.y;
    if denom < f64::EPSILON {
        return None; // degenerate segment
    }
    let t = ((world_point.x - a.x) * ab.x + (world_point.y - a.y) * ab.y) / denom;
    let t = if clamped { t.clamp(0.0, 1.0) } else { t };
    Some(a + ab * t)
}

// ---------------------------------------------------------------------------
// Impl for LineData
// ---------------------------------------------------------------------------

impl SnapCandidate for LineData {
    fn generate_snap_points(&self, world_point: Point2D, active_types: &[SnapType]) -> Vec<SnapCandidatePoint> {
        let mut candidates = Vec::with_capacity(4);
        let degenerate = self.start.distance(self.end) < f64::EPSILON;

        if degenerate {
            // Degenerate: single point acts as both Endpoint and Nearest.
            if is_active(active_types, SnapType::Endpoint)
                || is_active(active_types, SnapType::Nearest)
            {
                candidates.push(SnapCandidatePoint::new(self.start, SnapType::Endpoint, world_point));
            }
            return candidates;
        }

        // Endpoint
        if is_active(active_types, SnapType::Endpoint) {
            candidates.push(SnapCandidatePoint::new(self.start, SnapType::Endpoint, world_point));
            candidates.push(SnapCandidatePoint::new(self.end, SnapType::Endpoint, world_point));
        }

        // Midpoint
        if is_active(active_types, SnapType::Midpoint) {
            let mid = Point2D::new(
                (self.start.x + self.end.x) / 2.0,
                (self.start.y + self.end.y) / 2.0,
            );
            candidates.push(SnapCandidatePoint::new(mid, SnapType::Midpoint, world_point));
        }

        // Nearest (closest point on segment — clamped)
        if is_active(active_types, SnapType::Nearest) {
            if let Some(pt) = project_onto_line(world_point, self.start, self.end, true) {
                candidates.push(SnapCandidatePoint::new(pt, SnapType::Nearest, world_point));
            }
        }

        // Perpendicular (projection onto infinite line — NOT clamped)
        if is_active(active_types, SnapType::Perpendicular) {
            if let Some(pt) = project_onto_line(world_point, self.start, self.end, false) {
                candidates.push(SnapCandidatePoint::new(pt, SnapType::Perpendicular, world_point));
            }
        }

        candidates
    }
}

// ---------------------------------------------------------------------------
// Impl for CircleData
// ---------------------------------------------------------------------------

impl SnapCandidate for CircleData {
    fn generate_snap_points(&self, world_point: Point2D, active_types: &[SnapType]) -> Vec<SnapCandidatePoint> {
        let mut candidates = Vec::with_capacity(4);

        // Zero-radius guard: only Center is meaningful.
        if self.radius < f64::EPSILON {
            if is_active(active_types, SnapType::Center) {
                candidates.push(SnapCandidatePoint::new(self.center, SnapType::Center, world_point));
            }
            return candidates;
        }

        // Center
        if is_active(active_types, SnapType::Center) {
            candidates.push(SnapCandidatePoint::new(self.center, SnapType::Center, world_point));
        }

        // Nearest (closest point on circumference)
        if is_active(active_types, SnapType::Nearest) {
            let dir = (world_point - self.center).normalized();
            let pt = if dir.x == 0.0 && dir.y == 0.0 {
                // Cursor is exactly at the centre — pick an arbitrary
                // direction (+X) to avoid a zero-length normalized vector.
                Point2D::new(self.center.x + self.radius, self.center.y)
            } else {
                self.center + dir * self.radius
            };
            candidates.push(SnapCandidatePoint::new(pt, SnapType::Nearest, world_point));
        }

        // Tangent (from external point to circle)
        if is_active(active_types, SnapType::Tangent) {
            let d = world_point.distance(self.center);
            if d >= self.radius {
                // External or on the circle — two tangents exist.
                let angle = (self.radius / d).acos();
                let base_angle = (world_point.y - self.center.y).atan2(world_point.x - self.center.x);
                let t1 = Point2D::new(
                    self.center.x + (base_angle + angle).cos() * self.radius,
                    self.center.y + (base_angle + angle).sin() * self.radius,
                );
                let t2 = Point2D::new(
                    self.center.x + (base_angle - angle).cos() * self.radius,
                    self.center.y + (base_angle - angle).sin() * self.radius,
                );
                candidates.push(SnapCandidatePoint::new(t1, SnapType::Tangent, world_point));
                // If the two tangents are distinct (cursor not exactly on circle),
                // add both. If cursor is on the circle they coincide.
                if (t1.x - t2.x).abs() > f64::EPSILON || (t1.y - t2.y).abs() > f64::EPSILON {
                    candidates.push(SnapCandidatePoint::new(t2, SnapType::Tangent, world_point));
                }
            }
            // If cursor is INSIDE the circle, no tangents exist — no candidates added.
        }

        candidates
    }
}

// ---------------------------------------------------------------------------
// Impl for ArcData
// ---------------------------------------------------------------------------

impl SnapCandidate for ArcData {
    fn generate_snap_points(&self, world_point: Point2D, active_types: &[SnapType]) -> Vec<SnapCandidatePoint> {
        let mut candidates = Vec::with_capacity(3);

        // Zero-radius guard.
        if self.radius < f64::EPSILON {
            if is_active(active_types, SnapType::Center) {
                candidates.push(SnapCandidatePoint::new(self.center, SnapType::Center, world_point));
            }
            return candidates;
        }

        // Helper: convert degrees → radians and compute a point on the arc.
        let angle_to_point = |deg: f64| -> Point2D {
            let rad = deg * std::f64::consts::PI / 180.0;
            Point2D::new(
                self.center.x + rad.cos() * self.radius,
                self.center.y + rad.sin() * self.radius,
            )
        };

        // Center
        if is_active(active_types, SnapType::Center) {
            candidates.push(SnapCandidatePoint::new(self.center, SnapType::Center, world_point));
        }

        // Endpoint
        if is_active(active_types, SnapType::Endpoint) {
            candidates.push(SnapCandidatePoint::new(
                angle_to_point(self.start_angle),
                SnapType::Endpoint,
                world_point,
            ));
            candidates.push(SnapCandidatePoint::new(
                angle_to_point(self.end_angle),
                SnapType::Endpoint,
                world_point,
            ));
        }

        // Midpoint (mid-angle of the arc sweep)
        if is_active(active_types, SnapType::Midpoint) {
            // Handle arcs that wrap through 360° (end < start).
            let end = if self.end_angle > self.start_angle {
                self.end_angle
            } else {
                self.end_angle + 360.0
            };
            let mid_angle_deg = ((self.start_angle + end) / 2.0) % 360.0;
            candidates.push(SnapCandidatePoint::new(
                angle_to_point(mid_angle_deg),
                SnapType::Midpoint,
                world_point,
            ));
        }

        candidates
    }
}

// ---------------------------------------------------------------------------
// Impl for PolylineData
// ---------------------------------------------------------------------------

impl SnapCandidate for PolylineData {
    fn generate_snap_points(&self, world_point: Point2D, active_types: &[SnapType]) -> Vec<SnapCandidatePoint> {
        let n = self.vertices.len();
        if n < 2 {
            return Vec::new(); // Not enough vertices to form a segment.
        }

        // Upper estimate: 4 candidates per segment, up to n segments.
        let max_segments = if self.closed { n } else { n - 1 };
        let mut candidates = Vec::with_capacity((max_segments * 4).min(128));

        // Iterate over every segment.
        for i in 0..max_segments {
            let a = self.vertices[i];
            let b = self.vertices[(i + 1) % n];
            let degenerate = a.distance(b) < f64::EPSILON;

            if degenerate {
                // Degenerate segment: only output the shared endpoint once
                // (de-duplicated via the `first_vertex` rule below).
                if is_active(active_types, SnapType::Endpoint)
                    || is_active(active_types, SnapType::Nearest)
                {
                    // Only add when we haven't already added this vertex
                    // as an endpoint. The end of segment i-1 == start of
                    // segment i, so we only emit for the first segment
                    // that starts at this vertex.
                    // Implementation: skip endpoint for segment i > 0
                    // (the start vertex was already emitted as the end
                    // of the previous segment). For degenerate segments
                    // this is slightly tricky — we just skip endpoints
                    // for degenerate segments entirely to avoid bloat.
                }
                continue;
            }

            // --- Endpoint ---
            // Emit the start vertex only for the first segment (i == 0).
            // The end vertex is emitted for every segment; for open
            // polylines the last segment's end won't be re-emitted by
            // the next segment.
            if is_active(active_types, SnapType::Endpoint) {
                // Start vertex: only for i == 0 (first segment)
                if i == 0 {
                    candidates.push(SnapCandidatePoint::new(a, SnapType::Endpoint, world_point));
                }
                // End vertex: always emit (for segment i).
                // For open polylines, each interior vertex gets emitted
                // twice (as end of segment i and start of segment i+1).
                // We avoid this by only emitting the end vertex, and the
                // next segment's start is suppressed (i > 0 guard above).
                // For closed polylines, the last segment's end = vertex[0],
                // which was already emitted at i == 0 — skip it.
                let is_last_segment = i == max_segments - 1;
                if !(self.closed && is_last_segment) {
                    candidates.push(SnapCandidatePoint::new(b, SnapType::Endpoint, world_point));
                }
            }

            // --- Midpoint ---
            if is_active(active_types, SnapType::Midpoint) {
                let mid = Point2D::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
                candidates.push(SnapCandidatePoint::new(mid, SnapType::Midpoint, world_point));
            }

            // --- Nearest ---
            if is_active(active_types, SnapType::Nearest) {
                if let Some(pt) = project_onto_line(world_point, a, b, true) {
                    candidates.push(SnapCandidatePoint::new(pt, SnapType::Nearest, world_point));
                }
            }

            // --- Perpendicular ---
            if is_active(active_types, SnapType::Perpendicular) {
                if let Some(pt) = project_onto_line(world_point, a, b, false) {
                    candidates.push(SnapCandidatePoint::new(pt, SnapType::Perpendicular, world_point));
                }
            }
        }

        // Cap at 128: sort by distance and keep nearest.
        if candidates.len() > 128 {
            candidates.sort_by(|a, b| a.distance_world.partial_cmp(&b.distance_world).unwrap_or(std::cmp::Ordering::Equal));
            candidates.truncate(128);
        }

        candidates
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snap::SnapType::*;

    // ------------------------------------------------------------------
    // Helpers
    // ------------------------------------------------------------------

    /// Shortcut to call generate_snap_points with all 7 types active.
    fn all_candidates<T: SnapCandidate>(entity: &T, cursor: Point2D) -> Vec<SnapCandidatePoint> {
        entity.generate_snap_points(cursor, &[
            Endpoint, Midpoint, Center, Nearest, Perpendicular, Tangent, Grid,
        ])
    }

    /// Filter candidates to a specific snap type.
    fn filter_type(candidates: &[SnapCandidatePoint], t: SnapType) -> Vec<&SnapCandidatePoint> {
        candidates.iter().filter(|c| c.snap_type == t).collect()
    }

    fn pt(x: f64, y: f64) -> Point2D {
        Point2D::new(x, y)
    }

    // ==================================================================
    // LineData tests
    // ==================================================================

    mod line_tests {
        use super::*;

        fn line(start: Point2D, end: Point2D) -> LineData {
            LineData {
                start,
                end,
                color: crate::util::Color::WHITE,
                width: 1.0,
            }
        }

        #[test]
        fn generates_two_endpoints() {
            let l = line(pt(0.0, 0.0), pt(10.0, 0.0));
            let cands = all_candidates(&l, pt(5.0, 5.0));
            let eps = filter_type(&cands, Endpoint);
            assert_eq!(eps.len(), 2, "line should produce 2 endpoints");

            // Both endpoints should be present (order-independent check).
            let points: Vec<Point2D> = eps.iter().map(|c| c.point).collect();
            assert!(points.contains(&pt(0.0, 0.0)));
            assert!(points.contains(&pt(10.0, 0.0)));
        }

        #[test]
        fn generates_one_midpoint() {
            let l = line(pt(0.0, 0.0), pt(10.0, 0.0));
            let cands = all_candidates(&l, pt(5.0, 5.0));
            let mps = filter_type(&cands, Midpoint);
            assert_eq!(mps.len(), 1);
            assert_eq!(mps[0].point, pt(5.0, 0.0));
        }

        #[test]
        fn nearest_on_segment_returns_clamped_projection() {
            let l = line(pt(0.0, 0.0), pt(10.0, 0.0));
            // Cursor is at (3, 5) — nearest point on segment is (3, 0).
            let cands = all_candidates(&l, pt(3.0, 5.0));
            let nearest = filter_type(&cands, Nearest);
            assert!(!nearest.is_empty());
            let best = nearest.iter().min_by(|a, b| a.distance_world.partial_cmp(&b.distance_world).unwrap()).unwrap();
            assert!((best.point.x - 3.0).abs() < f64::EPSILON);
            assert!((best.point.y - 0.0).abs() < f64::EPSILON);
        }

        #[test]
        fn nearest_beyond_end_returns_endpoint() {
            let l = line(pt(0.0, 0.0), pt(10.0, 0.0));
            // Cursor is at (15, 5) — nearest point on segment is (10, 0).
            let cands = all_candidates(&l, pt(15.0, 5.0));
            let nearest = filter_type(&cands, Nearest);
            assert!(!nearest.is_empty());
            let best = nearest.iter().min_by(|a, b| a.distance_world.partial_cmp(&b.distance_world).unwrap()).unwrap();
            assert!((best.point.x - 10.0).abs() < f64::EPSILON);
            assert!((best.point.y - 0.0).abs() < f64::EPSILON);
        }

        #[test]
        fn perpendicular_returns_unclamped_projection() {
            let l = line(pt(0.0, 0.0), pt(10.0, 0.0));
            // Cursor at (12, 5) — perpendicular projection is (12, 0),
            // which is BEYOND the segment end. Nearest would give (10, 0),
            // but Perpendicular should give (12, 0).
            let cands = all_candidates(&l, pt(12.0, 5.0));
            let perps = filter_type(&cands, Perpendicular);
            assert!(!perps.is_empty());
            let best = perps.iter().min_by(|a, b| a.distance_world.partial_cmp(&b.distance_world).unwrap()).unwrap();
            assert!((best.point.x - 12.0).abs() < f64::EPSILON);
            assert!((best.point.y - 0.0).abs() < f64::EPSILON);
            // Verify it's different from Nearest for this case.
            let nearest = filter_type(&cands, Nearest);
            let nearest_best = nearest.iter().min_by(|a, b| a.distance_world.partial_cmp(&b.distance_world).unwrap()).unwrap();
            assert!((nearest_best.point.x - 10.0).abs() < f64::EPSILON, "Nearest should be clamped to segment end");
        }

        #[test]
        fn degenerate_line_returns_single_endpoint() {
            let l = line(pt(5.0, 5.0), pt(5.0, 5.0));
            let cands = all_candidates(&l, pt(0.0, 0.0));
            // Only Endpoint (aliased as Nearest) should be produced.
            assert!(!cands.is_empty());
            // Should have at most 1 candidate (Endpoint only, no Midpoint/Perpendicular).
            assert_eq!(cands.len(), 1, "degenerate line should produce exactly 1 candidate");
            assert_eq!(cands[0].point, pt(5.0, 5.0));
            assert_eq!(cands[0].snap_type, Endpoint);
        }

        #[test]
        fn active_types_filters_correctly() {
            let l = line(pt(0.0, 0.0), pt(10.0, 0.0));
            // Only Midpoint active.
            let cands = l.generate_snap_points(pt(5.0, 5.0), &[Midpoint]);
            assert_eq!(cands.len(), 1);
            assert_eq!(cands[0].snap_type, Midpoint);

            // Only Perpendicular active.
            let cands = l.generate_snap_points(pt(5.0, 5.0), &[Perpendicular]);
            assert_eq!(cands.len(), 1);
            assert_eq!(cands[0].snap_type, Perpendicular);
        }
    }

    // ==================================================================
    // CircleData tests
    // ==================================================================

    mod circle_tests {
        use super::*;

        fn circle(center: Point2D, radius: f64) -> CircleData {
            CircleData {
                center,
                radius,
                color: crate::util::Color::WHITE,
                width: 1.0,
            }
        }

        #[test]
        fn generates_center() {
            let c = circle(pt(10.0, 20.0), 5.0);
            let cands = all_candidates(&c, pt(0.0, 0.0));
            let centers = filter_type(&cands, Center);
            assert_eq!(centers.len(), 1);
            assert_eq!(centers[0].point, pt(10.0, 20.0));
        }

        #[test]
        fn nearest_external_point() {
            let c = circle(pt(0.0, 0.0), 10.0);
            // Cursor at (20, 0) → nearest point on circumference is (10, 0).
            let cands = all_candidates(&c, pt(20.0, 0.0));
            let nearest = filter_type(&cands, Nearest);
            assert!(!nearest.is_empty());
            let best = nearest.iter().min_by(|a, b| a.distance_world.partial_cmp(&b.distance_world).unwrap()).unwrap();
            assert!((best.point.x - 10.0).abs() < f64::EPSILON);
            assert!((best.point.y - 0.0).abs() < f64::EPSILON);
        }

        #[test]
        fn nearest_interior_point() {
            let c = circle(pt(0.0, 0.0), 10.0);
            // Cursor at (3, 4) inside circle. Nearest is on the circumference
            // in the direction of (3,4) i.e. (6, 8) normalized * 10 = (6, 8).
            let cands = all_candidates(&c, pt(3.0, 4.0));
            let nearest = filter_type(&cands, Nearest);
            assert!(!nearest.is_empty());
            let best = nearest.iter().min_by(|a, b| a.distance_world.partial_cmp(&b.distance_world).unwrap()).unwrap();
            // Direction from center to (3,4) is (0.6, 0.8), times 10 = (6, 8).
            assert!((best.point.x - 6.0).abs() < f64::EPSILON);
            assert!((best.point.y - 8.0).abs() < f64::EPSILON);
        }

        #[test]
        fn nearest_cursor_at_center_fallback() {
            let c = circle(pt(0.0, 0.0), 5.0);
            let cands = all_candidates(&c, pt(0.0, 0.0));
            let nearest = filter_type(&cands, Nearest);
            assert!(!nearest.is_empty());
            let best = nearest.iter().min_by(|a, b| a.distance_world.partial_cmp(&b.distance_world).unwrap()).unwrap();
            // Fallback: center + (radius, 0) = (5, 0).
            assert!((best.point.x - 5.0).abs() < f64::EPSILON);
            assert!((best.point.y - 0.0).abs() < f64::EPSILON);
        }

        #[test]
        fn tangent_external_point_generates_two() {
            let c = circle(pt(0.0, 0.0), 5.0);
            // External point at (13, 0).
            let cands = all_candidates(&c, pt(13.0, 0.0));
            let tangents = filter_type(&cands, Tangent);
            assert_eq!(tangents.len(), 2, "external point should generate 2 tangents");
            // Both tangents should be on the circumference.
            for t in &tangents {
                let dist = t.point.distance(pt(0.0, 0.0));
                assert!((dist - 5.0).abs() < 1e-9, "tangent point must be on circumference");
            }
        }

        #[test]
        fn tangent_interior_point_falls_back_to_nearest() {
            let c = circle(pt(0.0, 0.0), 10.0);
            // Cursor inside circle at (3, 4).
            let cands = all_candidates(&c, pt(3.0, 4.0));
            let tangents = filter_type(&cands, Tangent);
            assert_eq!(tangents.len(), 0, "interior point should generate 0 tangents");
            // Nearest should still be returned.
            let nearest = filter_type(&cands, Nearest);
            assert!(!nearest.is_empty(), "Nearest should still be returned");
        }

        #[test]
        fn zero_radius_returns_center_only() {
            let c = circle(pt(5.0, 5.0), 0.0);
            let cands = all_candidates(&c, pt(0.0, 0.0));
            assert_eq!(cands.len(), 1, "zero-radius circle should return only center");
            assert_eq!(cands[0].snap_type, Center);
            assert_eq!(cands[0].point, pt(5.0, 5.0));
        }

        #[test]
        fn tangent_point_on_circle_generates_one() {
            let c = circle(pt(0.0, 0.0), 5.0);
            // Cursor exactly on circle at (5, 0) — the two tangents coincide.
            let cands = all_candidates(&c, pt(5.0, 0.0));
            let tangents = filter_type(&cands, Tangent);
            assert_eq!(tangents.len(), 1, "cursor on circle generates 1 tangent (coincident)");
        }
    }

    // ==================================================================
    // ArcData tests
    // ==================================================================

    mod arc_tests {
        use super::*;

        fn arc(center: Point2D, radius: f64, start_angle: f64, end_angle: f64) -> ArcData {
            ArcData {
                center,
                radius,
                start_angle,
                end_angle,
                color: crate::util::Color::WHITE,
                width: 1.0,
            }
        }

        #[test]
        fn generates_center() {
            let a = arc(pt(5.0, 10.0), 20.0, 0.0, 90.0);
            let cands = all_candidates(&a, pt(0.0, 0.0));
            let centers = filter_type(&cands, Center);
            assert_eq!(centers.len(), 1);
            assert_eq!(centers[0].point, pt(5.0, 10.0));
        }

        #[test]
        fn generates_two_endpoints() {
            let a = arc(pt(0.0, 0.0), 10.0, 0.0, 90.0);
            let cands = all_candidates(&a, pt(5.0, 5.0));
            let eps = filter_type(&cands, Endpoint);
            assert_eq!(eps.len(), 2);
            // Start angle 0° → point at (10, 0).
            // End angle 90° → point at (0, 10) (approximate; cos(π/2) ≠ exactly 0.0).
            assert!(
                (eps[0].point.x - 10.0).abs() < 1e-9 || (eps[1].point.x - 10.0).abs() < 1e-9,
                "expected start endpoint at (10, 0)",
            );
            assert!(
                (eps[0].point.y - 10.0).abs() < 1e-9 || (eps[1].point.y - 10.0).abs() < 1e-9,
                "expected end endpoint at approx (0, 10)",
            );
            // Verify both are on the circumference.
            for e in &eps {
                let d = e.point.distance(pt(0.0, 0.0));
                assert!((d - 10.0).abs() < 1e-9, "endpoint must be on circumference");
            }
        }

        #[test]
        fn generates_one_midpoint() {
            let a = arc(pt(0.0, 0.0), 10.0, 0.0, 90.0);
            let cands = all_candidates(&a, pt(5.0, 5.0));
            let mps = filter_type(&cands, Midpoint);
            assert_eq!(mps.len(), 1);
            // Mid-angle is 45°, point at (10*cos(45°), 10*sin(45°)) ≈ (7.07, 7.07).
            let expected_x = 10.0 * (45.0_f64 * std::f64::consts::PI / 180.0).cos();
            let expected_y = 10.0 * (45.0_f64 * std::f64::consts::PI / 180.0).sin();
            assert!((mps[0].point.x - expected_x).abs() < 1e-9);
            assert!((mps[0].point.y - expected_y).abs() < 1e-9);
        }

        #[test]
        fn endpoints_use_correct_angles() {
            // Arc at 180° to 270° → endpoints at (-10, 0) and (0, -10).
            let a = arc(pt(0.0, 0.0), 10.0, 180.0, 270.0);
            let cands = all_candidates(&a, pt(0.0, 0.0));
            let eps = filter_type(&cands, Endpoint);
            assert_eq!(eps.len(), 2);
            // Tolerance for trig imprecision.
            let tol = 1e-9;
            let on_neg_x = |p: Point2D| (p.x + 10.0).abs() < tol && (p.y - 0.0).abs() < tol;
            let on_neg_y = |p: Point2D| (p.x - 0.0).abs() < tol && (p.y + 10.0).abs() < tol;
            assert!(
                on_neg_x(eps[0].point) || on_neg_x(eps[1].point),
                "expected 180° endpoint ≈ (-10, 0)",
            );
            assert!(
                on_neg_y(eps[0].point) || on_neg_y(eps[1].point),
                "expected 270° endpoint ≈ (0, -10)",
            );
        }

        #[test]
        fn degree_to_radian_conversion() {
            // 180° arc from 0° to 180°.
            let a = arc(pt(0.0, 0.0), 5.0, 0.0, 180.0);
            let cands = all_candidates(&a, pt(0.0, 0.0));
            let eps = filter_type(&cands, Endpoint);
            assert_eq!(eps.len(), 2);
            let tol = 1e-9;
            let on_pos_x = |p: Point2D| (p.x - 5.0).abs() < tol && (p.y - 0.0).abs() < tol;
            let on_neg_x = |p: Point2D| (p.x + 5.0).abs() < tol && (p.y - 0.0).abs() < tol;
            assert!(
                on_pos_x(eps[0].point) || on_pos_x(eps[1].point),
                "expected 0° endpoint ≈ (5, 0)",
            );
            assert!(
                on_neg_x(eps[0].point) || on_neg_x(eps[1].point),
                "expected 180° endpoint ≈ (-5, 0)",
            );
        }

        #[test]
        fn zero_radius_returns_center_only() {
            let a = arc(pt(3.0, 4.0), 0.0, 0.0, 90.0);
            let cands = all_candidates(&a, pt(0.0, 0.0));
            assert_eq!(cands.len(), 1, "zero-radius arc should return only center");
            assert_eq!(cands[0].snap_type, Center);
        }

        #[test]
        fn midpoint_crossing_zero_degrees() {
            // Arc from 350° to 10° (20° sweep crossing 0°).
            // Midpoint should be 0° → (10, 0), NOT (−10, 0).
            let a = arc(pt(0.0, 0.0), 10.0, 350.0, 10.0);
            let cands = all_candidates(&a, pt(0.0, 0.0));
            let mps = filter_type(&cands, Midpoint);
            assert_eq!(mps.len(), 1);
            assert!((mps[0].point.x - 10.0).abs() < 1e-9,
                "midpoint should be at 0° (10, 0), got ({}, {})", mps[0].point.x, mps[0].point.y);
            assert!((mps[0].point.y - 0.0).abs() < 1e-9);
        }

        #[test]
        fn midpoint_crossing_via_360() {
            // Arc from 270° to 90° (180° sweep crossing 0°).
            // Midpoint should be 0° → (10, 0), NOT (180° → -10, 0).
            let a = arc(pt(0.0, 0.0), 10.0, 270.0, 90.0);
            let cands = all_candidates(&a, pt(0.0, 0.0));
            let mps = filter_type(&cands, Midpoint);
            assert_eq!(mps.len(), 1);
            assert!((mps[0].point.x - 10.0).abs() < 1e-9,
                "midpoint should be at 0° (10, 0), got ({}, {})", mps[0].point.x, mps[0].point.y);
            assert!((mps[0].point.y - 0.0).abs() < 1e-9);
        }
    }

    // ==================================================================
    // PolylineData tests
    // ==================================================================

    mod polyline_tests {
        use super::*;

        fn polyline(vertices: Vec<Point2D>, closed: bool) -> PolylineData {
            PolylineData {
                vertices,
                closed,
                color: crate::util::Color::WHITE,
                width: 1.0,
            }
        }

        #[test]
        fn open_polyline_generates_endpoints_without_duplicates() {
            // 3 vertices (2 segments): (0,0)-(10,0)-(10,10)
            let p = polyline(vec![pt(0.0, 0.0), pt(10.0, 0.0), pt(10.0, 10.0)], false);
            let cands = all_candidates(&p, pt(5.0, 5.0));
            let eps = filter_type(&cands, Endpoint);
            // 3 unique vertices → 3 endpoints
            // (start of segment 0, end of segment 0, end of segment 1)
            assert_eq!(eps.len(), 3);
            let points: Vec<Point2D> = eps.iter().map(|c| c.point).collect();
            assert!(points.contains(&pt(0.0, 0.0)));
            assert!(points.contains(&pt(10.0, 0.0)));
            assert!(points.contains(&pt(10.0, 10.0)));
        }

        #[test]
        fn closed_polyline_includes_closing_segment() {
            // Triangle: (0,0)-(10,0)-(0,10), closed.
            let p = polyline(vec![pt(0.0, 0.0), pt(10.0, 0.0), pt(0.0, 10.0)], true);
            let cands = all_candidates(&p, pt(3.0, 3.0));
            let mps = filter_type(&cands, Midpoint);
            // 3 segments → 3 midpoints.
            assert_eq!(mps.len(), 3, "closed triangle should have 3 midpoints");

            // The closing segment from (0,10) back to (0,0) has midpoint (0, 5).
            let closing_mid = mps.iter().find(|c| {
                (c.point.x - 0.0).abs() < f64::EPSILON && (c.point.y - 5.0).abs() < f64::EPSILON
            });
            assert!(closing_mid.is_some(), "closing segment midpoint (0, 5) should be present");
        }

        #[test]
        fn polyline_caps_at_128_candidates() {
            // 100 segments → up to 400 candidates without cap.
            let mut verts = Vec::with_capacity(101);
            for i in 0..=100 {
                verts.push(pt(i as f64, 0.0));
            }
            let p = polyline(verts, false);
            let cands = all_candidates(&p, pt(50.0, 5.0));
            assert!(cands.len() <= 128, "polyline candidates should be capped at 128, got {}", cands.len());
        }

        #[test]
        fn closed_two_vertex_polyline() {
            // 2 vertices, closed: single segment from (0,0) to (10,0) and back.
            let p = polyline(vec![pt(0.0, 0.0), pt(10.0, 0.0)], true);
            let cands = all_candidates(&p, pt(5.0, 5.0));
            let eps = filter_type(&cands, Endpoint);
            // Open 2-vertex closed polyline is essentially a degenerate case.
            // The closing segment from (10,0) back to (0,0) is the same segment
            // as the forward one. For closed, we skip last segment's end vertex
            // (= vertex[0]) to avoid duplicate. So endpoints = 2 (both vertices).
            // Actually let's trace:
            // seg 0: a=(0,0) b=(10,0) → emit a (i==0) and b (not last-closed)
            // seg 1 (closing): a=(10,0) b=(0,0) → skip a (i>0), skip b (is_last_segment && closed)
            // So endpoints: (0,0) and (10,0) → 2 endpoints.
            assert_eq!(eps.len(), 2, "closed 2-vertex polyline should have 2 endpoints");
        }

        #[test]
        fn single_segment_open() {
            let p = polyline(vec![pt(0.0, 0.0), pt(10.0, 0.0)], false);
            let cands = all_candidates(&p, pt(5.0, 5.0));
            // Should behave like a LineData.
            let eps = filter_type(&cands, Endpoint);
            assert_eq!(eps.len(), 2);
            let mps = filter_type(&cands, Midpoint);
            assert_eq!(mps.len(), 1);
            assert_eq!(mps[0].point, pt(5.0, 0.0));

            let nearest = filter_type(&cands, Nearest);
            assert!(!nearest.is_empty());
            let perps = filter_type(&cands, Perpendicular);
            assert!(!perps.is_empty());
        }

        #[test]
        fn empty_polyline_returns_no_candidates() {
            let p = polyline(vec![], false);
            let cands = all_candidates(&p, pt(0.0, 0.0));
            assert!(cands.is_empty());

            let p = polyline(vec![pt(0.0, 0.0)], false);
            let cands = all_candidates(&p, pt(0.0, 0.0));
            assert!(cands.is_empty(), "single-vertex polyline should return no candidates");
        }

        #[test]
        fn active_types_respected() {
            let p = polyline(vec![pt(0.0, 0.0), pt(10.0, 0.0), pt(10.0, 10.0)], false);
            // Only Midpoint + Nearest active.
            let cands = p.generate_snap_points(pt(5.0, 5.0), &[Midpoint, Nearest]);
            for c in &cands {
                assert!(c.snap_type == Midpoint || c.snap_type == Nearest,
                    "unexpected snap type {:?}", c.snap_type);
            }
            // Should have 2 midpoints + 2 nearest = 4 candidates.
            assert_eq!(cands.len(), 4);
        }
    }

    // ==================================================================
    // SnapCandidate trait dispatch test
    // ==================================================================

    mod dispatch_tests {
        use super::*;

        #[test]
        fn line_data_implements_snap_candidate() {
            let l = LineData {
                start: pt(0.0, 0.0),
                end: pt(10.0, 0.0),
                color: crate::util::Color::WHITE,
                width: 1.0,
            };
            // Compile-time check: LineData: SnapCandidate.
            fn assert_snap_candidate<T: SnapCandidate>() {}
            assert_snap_candidate::<LineData>();

            let cands: Vec<SnapCandidatePoint> = SnapCandidate::generate_snap_points(&l, pt(5.0, 5.0), &[Endpoint]);
            assert!(!cands.is_empty());
        }

        #[test]
        fn circle_data_implements_snap_candidate() {
            fn assert_snap_candidate<T: SnapCandidate>() {}
            assert_snap_candidate::<CircleData>();
        }

        #[test]
        fn arc_data_implements_snap_candidate() {
            fn assert_snap_candidate<T: SnapCandidate>() {}
            assert_snap_candidate::<ArcData>();
        }

        #[test]
        fn polyline_data_implements_snap_candidate() {
            fn assert_snap_candidate<T: SnapCandidate>() {}
            assert_snap_candidate::<PolylineData>();
        }
    }
}
