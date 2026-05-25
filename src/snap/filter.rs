//! Snap tolerance/priority filter.
//!
//! Provides functions for:
//!
//! * Converting a world-space candidate point to screen-space distance from
//!   the cursor ([`screen_distance`]).
//! * Filtering candidates that fall outside the aperture radius
//!   ([`filter_by_distance`]).
//! * Ranking surviving candidates by configurable snap-type priority
//!   ([`rank_by_priority`]).
//!
//! These functions are consumed by [`SnapEngine::snap`][super::SnapEngine::snap]
//! (integrated in Step 7).

use std::collections::HashMap;

use crate::ecs::resources::CameraState;
use crate::geometry::Point2D;
use crate::snap::candidate::SnapCandidatePoint;
use crate::snap::SnapType;

/// Convert a world-space point to screen-space pixel distance from the
/// cursor position.
///
/// 1. Projects `world_point` to screen coordinates via [`CameraState`].
/// 2. Computes the Euclidean distance (in pixels) to `screen_pos`.
pub fn screen_distance(
    world_point: Point2D,
    screen_pos: (f32, f32),
    camera: &CameraState,
) -> f32 {
    let candidate_screen = camera.world_to_screen(world_point);
    let dx = candidate_screen.0 - screen_pos.0;
    let dy = candidate_screen.1 - screen_pos.1;
    (dx * dx + dy * dy).sqrt()
}

/// Filter candidates whose screen-space distance exceeds the aperture
/// radius.
///
/// Each candidate's world-space position is converted to screen pixels
/// via [`screen_distance`]; candidates farther than `aperture_size`
/// pixels from the cursor are discarded.
pub fn filter_by_distance(
    candidates: Vec<SnapCandidatePoint>,
    screen_pos: (f32, f32),
    aperture_size: f32,
    camera: &CameraState,
) -> Vec<SnapCandidatePoint> {
    candidates
        .into_iter()
        .filter(|c| screen_distance(c.point, screen_pos, camera) <= aperture_size)
        .collect()
}

/// Sort candidates in-place by snap-type priority (ascending).
///
/// Lower priority values correspond to higher precedence (Endpoint = 0
/// is the highest priority).  Types not present in `priority_map` are
/// treated as `u8::MAX` (lowest priority).
pub fn rank_by_priority(
    candidates: &mut [SnapCandidatePoint],
    priority_map: &HashMap<SnapType, u8>,
) {
    candidates.sort_by_key(|c| priority_map.get(&c.snap_type).copied().unwrap_or(u8::MAX));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: build a simple camera for tests.
    fn make_camera(zoom: f64) -> CameraState {
        CameraState {
            target: Point2D::new(0.0, 0.0),
            zoom,
            viewport_size: (800, 600),
            clear_color: crate::util::Color::default(),
        }
    }

    /// Helper: build a candidate at a given world position and snap type.
    fn candidate(point: Point2D, snap_type: SnapType, dist: f64) -> SnapCandidatePoint {
        SnapCandidatePoint {
            point,
            snap_type,
            distance_world: dist,
        }
    }

    // ------------------------------------------------------------------
    // screen_distance
    // ------------------------------------------------------------------

    #[test]
    fn test_screen_distance_zero_when_at_cursor() {
        let cam = make_camera(1.0);
        // world (0,0) maps to screen (400, 300) with zoom=1, 800×600
        let dist = screen_distance(Point2D::new(0.0, 0.0), (400.0, 300.0), &cam);
        assert!(
            dist < 0.01,
            "expected ~0 pixels, got {dist}",
        );
    }

    #[test]
    fn test_screen_distance_scales_with_zoom() {
        let cam = make_camera(2.0);
        // world (1,0) → screen.x = 400 + 1*2 = 402
        let dist = screen_distance(Point2D::new(1.0, 0.0), (400.0, 300.0), &cam);
        assert!(
            (dist - 2.0).abs() < 0.01,
            "expected 2 pixels (zoom=2), got {dist}",
        );
    }

    // ------------------------------------------------------------------
    // filter_by_distance
    // ------------------------------------------------------------------

    #[test]
    fn test_filter_by_distance_keeps_close_candidates() {
        let cam = make_camera(1.0);
        let candidates = vec![
            candidate(Point2D::new(0.0, 0.0), SnapType::Endpoint, 0.0),
            candidate(Point2D::new(10.0, 0.0), SnapType::Midpoint, 10.0),
        ];

        // aperture = 15 pixels, zoom = 1 → world dist ≤ 15
        let filtered = filter_by_distance(candidates, (400.0, 300.0), 15.0, &cam);
        assert_eq!(filtered.len(), 2, "both candidates within aperture");
    }

    #[test]
    fn test_filter_by_distance_removes_far_candidates() {
        let cam = make_camera(1.0);
        let candidates = vec![
            candidate(Point2D::new(0.0, 0.0), SnapType::Endpoint, 0.0),
            // (100, 0) → screen (500, 300) → 100 pixels away → beyond aperture
            candidate(Point2D::new(100.0, 0.0), SnapType::Nearest, 100.0),
        ];

        let filtered = filter_by_distance(candidates, (400.0, 300.0), 20.0, &cam);
        assert_eq!(filtered.len(), 1, "far candidate should be filtered out");
        assert_eq!(filtered[0].snap_type, SnapType::Endpoint);
    }

    #[test]
    fn test_filter_by_distance_empty_input() {
        let cam = make_camera(1.0);
        let filtered = filter_by_distance(vec![], (400.0, 300.0), 10.0, &cam);
        assert!(filtered.is_empty());
    }

    // ------------------------------------------------------------------
    // rank_by_priority
    // ------------------------------------------------------------------

    #[test]
    fn test_rank_by_priority_orders_by_lower_value_first() {
        let mut priority_map = HashMap::new();
        priority_map.insert(SnapType::Endpoint, 0u8);
        priority_map.insert(SnapType::Midpoint, 1u8);
        priority_map.insert(SnapType::Nearest, 6u8);

        let mut candidates = vec![
            candidate(Point2D::new(3.0, 0.0), SnapType::Nearest, 0.0),
            candidate(Point2D::new(1.0, 0.0), SnapType::Endpoint, 0.0),
            candidate(Point2D::new(2.0, 0.0), SnapType::Midpoint, 0.0),
        ];

        rank_by_priority(&mut candidates, &priority_map);

        assert_eq!(
            candidates
                .iter()
                .map(|c| c.snap_type)
                .collect::<Vec<_>>(),
            vec![SnapType::Endpoint, SnapType::Midpoint, SnapType::Nearest],
            "lower priority value = higher precedence",
        );
    }

    #[test]
    fn test_rank_by_priority_unknown_type_at_end() {
        let mut priority_map = HashMap::new();
        priority_map.insert(SnapType::Endpoint, 0u8);

        let mut candidates = vec![
            candidate(Point2D::new(2.0, 0.0), SnapType::Grid, 0.0),   // not in map
            candidate(Point2D::new(1.0, 0.0), SnapType::Endpoint, 0.0),
        ];

        rank_by_priority(&mut candidates, &priority_map);

        assert_eq!(candidates[0].snap_type, SnapType::Endpoint);
        assert_eq!(candidates[1].snap_type, SnapType::Grid);
    }

    #[test]
    fn test_rank_by_priority_stable_for_equal_priority() {
        let mut priority_map = HashMap::new();
        priority_map.insert(SnapType::Endpoint, 0u8);
        priority_map.insert(SnapType::Midpoint, 0u8);

        let mut candidates = vec![
            candidate(Point2D::new(2.0, 0.0), SnapType::Midpoint, 0.0),
            candidate(Point2D::new(1.0, 0.0), SnapType::Endpoint, 0.0),
        ];

        rank_by_priority(&mut candidates, &priority_map);

        // Both have same priority (0). sort_by_key is stable, so
        // Midpoint (first in) stays before Endpoint.
        assert_eq!(candidates[0].snap_type, SnapType::Midpoint);
        assert_eq!(candidates[1].snap_type, SnapType::Endpoint);
    }
}
