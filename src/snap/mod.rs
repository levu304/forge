//! Snap engine.
//!
//! Provides 7 snap types (Endpoint, Midpoint, Center, Nearest,
//! Perpendicular, Tangent, Grid), candidate generation, tolerance
//! filtering, priority ranking, and visual marker rendering.
//!
//! # Architecture
//!
//! [`SnapEngine`] is the top-level entry point. Its [`snap`] method:
//!
//! 1. Ensures the [`SpatialIndex`] is clean (lazy rebuild if dirty).
//! 2. Queries the spatial index for the nearest neighbour.
//! 3. (Step 7+) Generates snap candidates for the nearest entity.
//! 4. (Step 7+) Filters by screen-space aperture and ranks by priority.
//! 5. Stores the best result in [`last_result`] for visual feedback.
//!
//! # Current Status (v0.2.0 Step 6)
//!
//! The [`snap`] method performs a simple nearest-neighbour lookup and
//! returns a [`SnapResult`] with type [`SnapType::Nearest`]. Full
//! candidate generation (per-entity-type snap points) is deferred to
//! Step 7; tolerance filtering and priority ranking are deferred to
//! Step 7+.
//!
//! [`snap`]: SnapEngine::snap
//! [`last_result`]: SnapEngine::last_result
//! [`SpatialIndex`]: crate::spatial::SpatialIndex

use hecs::World;

use crate::ecs::resources::SnapConfig;
use crate::geometry::Point2D;
use crate::spatial::SpatialIndex;

pub mod candidate;
pub mod filter;
pub mod visual;

// ---------------------------------------------------------------------------
// SnapType
// ---------------------------------------------------------------------------

/// The 7 snap types supported by the snap engine.
///
/// Listed below with their default priority (lower = higher precedence):
///
/// | Variant       | Priority | Description                       |
/// |---------------|----------|-----------------------------------|
/// | `Endpoint`    | 0        | End of a line / arc / poly-segment |
/// | `Midpoint`    | 1        | Middle of a segment               |
/// | `Center`      | 2        | Center of a circle / arc          |
/// | `Grid`        | 3        | Nearest grid intersection         |
/// | `Perpendicular` | 4      | Perpendicular projection          |
/// | `Tangent`     | 5        | Tangent point on circle / arc     |
/// | `Nearest`     | 6        | Closest point on any entity       |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SnapType {
    /// End of a line / arc / polyline segment.
    Endpoint,
    /// Middle of a line / arc segment.
    Midpoint,
    /// Center of a circle / arc.
    Center,
    /// Closest point on any entity.
    Nearest,
    /// Perpendicular projection to a line / arc.
    Perpendicular,
    /// Tangent point on a circle / arc.
    Tangent,
    /// Nearest grid intersection.
    Grid,
}

// ---------------------------------------------------------------------------
// SnapResult
// ---------------------------------------------------------------------------

/// The result of a single snap operation.
///
/// Returned by [`SnapEngine::snap`] and cached in
/// [`SnapEngine::last_result`] for the snap-marker renderer.
#[derive(Debug, Clone, Copy)]
pub struct SnapResult {
    /// The snapped world-space point (or the raw cursor point if no snap).
    pub point: Point2D,
    /// The type of snap that was applied.
    pub snap_type: SnapType,
    /// The entity that was snapped to, if any.
    pub source_entity: Option<hecs::Entity>,
    /// Screen-space distance from the cursor to the snap point (pixels).
    pub distance_screen: f32,
}

// ---------------------------------------------------------------------------
// SnapEngine
// ---------------------------------------------------------------------------

/// The main snap engine.
///
/// Orchestrates candidate generation, tolerance filtering, priority
/// ranking, and stores the last snap result for visual feedback.
#[derive(Debug, Clone)]
pub struct SnapEngine {
    /// Configuration (enabled, aperture, marker size, priorities).
    pub config: SnapConfig,
    /// The subset of [`SnapType`] variants currently active.
    pub active_types: Vec<SnapType>,
    /// The most recent snap result (used by the marker renderer).
    pub last_result: Option<SnapResult>,
}

impl SnapEngine {
    /// Create a new snap engine with the given configuration.
    ///
    /// All 7 snap types are active by default.
    pub fn new(config: SnapConfig) -> Self {
        Self {
            config,
            active_types: vec![
                SnapType::Endpoint,
                SnapType::Midpoint,
                SnapType::Center,
                SnapType::Nearest,
                SnapType::Perpendicular,
                SnapType::Tangent,
                SnapType::Grid,
            ],
            last_result: None,
        }
    }

    /// Run the snap pipeline.
    ///
    /// 1. If snapping is disabled or no snap types are active, returns
    ///    the raw cursor point immediately (avoids an R-tree rebuild).
    /// 2. Ensures the spatial index is clean (lazy rebuild).
    /// 3. Performs a nearest-neighbour query at `raw`.
    /// 4. Returns a [`SnapResult`] describing the best snap point.
    ///
    /// If snapping is disabled, no active types are set, or no entity
    /// is found, the raw cursor point is returned as a fallback (with
    /// `source_entity = None`).
    ///
    /// The result is also stored in [`last_result`] for the snap-marker
    /// renderer.
    ///
    /// [`last_result`]: SnapEngine::last_result
    pub fn snap(
        &mut self,
        raw: Point2D,
        _screen_pos: (f32, f32),
        world: &World,
        spatial: &mut SpatialIndex,
    ) -> SnapResult {
        // 1. If snapping is disabled or no snap types are active,
        //    return the raw point immediately — avoids an R-tree rebuild.
        if !self.config.enabled || self.active_types.is_empty() {
            let fallback = SnapResult {
                point: raw,
                snap_type: SnapType::Nearest,
                source_entity: None,
                distance_screen: 0.0,
            };
            self.last_result = Some(fallback);
            return fallback;
        }

        // 2. Ensure the spatial index is up-to-date.
        spatial.ensure_clean(world);

        // 3. Query the spatial index for the nearest entity.
        let nearest = spatial.nearest_neighbor(raw);

        // Full per-entity-type candidate generation is deferred to Step 7.
        let result = SnapResult {
            point: raw,
            snap_type: SnapType::Nearest,
            source_entity: nearest,
            distance_screen: 0.0,
        };

        self.last_result = Some(result);
        result
    }

    /// Look up the priority value for a snap type.
    ///
    /// Returns `u8::MAX` if the type is not in the priority map.
    pub fn priority_for_type(&self, snap_type: SnapType) -> u8 {
        self.config
            .priority_map
            .get(&snap_type)
            .copied()
            .unwrap_or(u8::MAX)
    }

    /// Replace the active snap types with a new set.
    pub fn set_active_types(&mut self, types: Vec<SnapType>) {
        self.active_types = types;
    }

    /// Enable a snap type (adds to `active_types` if not already present).
    pub fn enable_type(&mut self, snap_type: SnapType) {
        if !self.active_types.contains(&snap_type) {
            self.active_types.push(snap_type);
        }
    }

    /// Disable a snap type (removes from `active_types`).
    pub fn disable_type(&mut self, snap_type: SnapType) {
        self.active_types.retain(|t| *t != snap_type);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::components::{LineData, Renderable};
    use crate::util::Color;

    // ------------------------------------------------------------------
    // Helper: build a line entity in the world
    // ------------------------------------------------------------------
    fn make_line_entity(
        world: &mut World,
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
    ) -> hecs::Entity {
        world.spawn((
            LineData {
                start: Point2D::new(x1, y1),
                end: Point2D::new(x2, y2),
                color: Color::WHITE,
                width: 1.0,
            },
            Renderable,
        ))
    }

    // ------------------------------------------------------------------
    // Priority ranking
    // ------------------------------------------------------------------

    #[test]
    fn test_priority_ranking_all_types_unique() {
        let config = SnapConfig::default();
        let priorities: Vec<(SnapType, u8)> = vec![
            (SnapType::Endpoint, config.priority_map[&SnapType::Endpoint]),
            (SnapType::Midpoint, config.priority_map[&SnapType::Midpoint]),
            (SnapType::Center, config.priority_map[&SnapType::Center]),
            (SnapType::Grid, config.priority_map[&SnapType::Grid]),
            (
                SnapType::Perpendicular,
                config.priority_map[&SnapType::Perpendicular],
            ),
            (SnapType::Tangent, config.priority_map[&SnapType::Tangent]),
            (SnapType::Nearest, config.priority_map[&SnapType::Nearest]),
        ];

        // Verify the list is already in ascending priority order.
        for window in priorities.windows(2) {
            assert!(
                window[0].1 <= window[1].1,
                "priorities not in ascending order: {:?} > {:?}",
                window[0],
                window[1],
            );
        }

        // Verify all 7 priorities are unique.
        let mut values: Vec<u8> = priorities.iter().map(|(_, v)| *v).collect();
        values.sort();
        values.dedup();
        assert_eq!(values.len(), 7, "all 7 priorities must be unique");
    }

    // ------------------------------------------------------------------
    // SnapConfig defaults
    // ------------------------------------------------------------------

    #[test]
    fn test_snap_config_defaults() {
        let config = SnapConfig::default();
        assert!(config.enabled, "snap should be enabled by default");
        assert_eq!(config.marker_size, 10.0);
        assert_eq!(config.aperture_size, 12.0);

        // Verify all 7 types have the correct default priorities.
        assert_eq!(
            config.priority_map[&SnapType::Endpoint],
            0,
            "Endpoint should have highest priority (0)",
        );
        assert_eq!(config.priority_map[&SnapType::Midpoint], 1);
        assert_eq!(config.priority_map[&SnapType::Center], 2);
        assert_eq!(config.priority_map[&SnapType::Grid], 3);
        assert_eq!(config.priority_map[&SnapType::Perpendicular], 4);
        assert_eq!(config.priority_map[&SnapType::Tangent], 5);
        assert_eq!(
            config.priority_map[&SnapType::Nearest],
            6,
            "Nearest should have lowest priority (6)",
        );
    }

    // ------------------------------------------------------------------
    // priority_for_type
    // ------------------------------------------------------------------

    #[test]
    fn test_priority_for_type() {
        let engine = SnapEngine::new(SnapConfig::default());
        assert_eq!(engine.priority_for_type(SnapType::Endpoint), 0);
        assert_eq!(engine.priority_for_type(SnapType::Nearest), 6);
    }

    #[test]
    fn test_priority_for_type_unknown_returns_max() {
        // Construct an engine with an empty priority map.
        let mut config = SnapConfig::default();
        config.priority_map.clear();
        let engine = SnapEngine::new(config);

        assert_eq!(
            engine.priority_for_type(SnapType::Endpoint),
            u8::MAX,
            "unknown type should get u8::MAX",
        );
    }

    // ------------------------------------------------------------------
    // No-entity fallback
    // ------------------------------------------------------------------

    #[test]
    fn test_no_entity_fallback() {
        let mut engine = SnapEngine::new(SnapConfig::default());
        let mut world = World::new();
        let mut spatial = SpatialIndex::new();

        let raw = Point2D::new(42.0, 99.0);
        let result = engine.snap(raw, (0.0, 0.0), &world, &mut spatial);

        assert_eq!(
            result.point, raw,
            "raw point should be returned when no entity exists",
        );
        assert_eq!(result.snap_type, SnapType::Nearest);
        assert!(
            result.source_entity.is_none(),
            "source_entity should be None when no entity found",
        );
    }

    // ------------------------------------------------------------------
    // last_result updated
    // ------------------------------------------------------------------

    #[test]
    fn test_last_result_none_before_snap() {
        let engine = SnapEngine::new(SnapConfig::default());
        assert!(
            engine.last_result.is_none(),
            "last_result should be None initially",
        );
    }

    #[test]
    fn test_last_result_updated_after_snap() {
        let mut engine = SnapEngine::new(SnapConfig::default());
        let mut world = World::new();
        let mut spatial = SpatialIndex::new();

        let raw = Point2D::new(10.0, 20.0);
        engine.snap(raw, (0.0, 0.0), &world, &mut spatial);

        assert!(
            engine.last_result.is_some(),
            "last_result should be Some after snap()",
        );
        let last = engine.last_result.expect("last_result should be Some after snap()");
        assert_eq!(last.point.x, 10.0);
        assert_eq!(last.point.y, 20.0);
    }

    // ------------------------------------------------------------------
    // Active types filtering
    // ------------------------------------------------------------------

    #[test]
    fn test_empty_active_types_returns_raw() {
        let mut engine = SnapEngine::new(SnapConfig::default());
        let mut world = World::new();
        let mut spatial = SpatialIndex::new();

        engine.set_active_types(vec![]);

        let raw = Point2D::new(5.0, 5.0);
        let result = engine.snap(raw, (0.0, 0.0), &world, &mut spatial);

        assert_eq!(
            result.point, raw,
            "empty active types should return raw point",
        );
        assert!(result.source_entity.is_none());
    }

    // ------------------------------------------------------------------
    // Disabled snap returns raw
    // ------------------------------------------------------------------

    #[test]
    fn test_snap_disabled_returns_raw() {
        let mut config = SnapConfig::default();
        config.enabled = false;
        let mut engine = SnapEngine::new(config);
        let mut world = World::new();
        let mut spatial = SpatialIndex::new();

        let raw = Point2D::new(100.0, 200.0);
        let result = engine.snap(raw, (0.0, 0.0), &world, &mut spatial);

        assert_eq!(
            result.point, raw,
            "disabled snap should return raw point",
        );
        assert!(result.source_entity.is_none());
    }

    // ------------------------------------------------------------------
    // Nearest neighbour snap
    // ------------------------------------------------------------------

    #[test]
    fn test_nearest_neighbor_snap() {
        let mut engine = SnapEngine::new(SnapConfig::default());
        let mut world = World::new();
        let mut spatial = SpatialIndex::new();

        // Spawn an entity at (0,0)–(10,10).
        let entity = make_line_entity(&mut world, 0.0, 0.0, 10.0, 10.0);
        spatial.rebuild(&world);

        // Snap near the entity's bounding box centre (5,5).
        let raw = Point2D::new(5.0, 5.0);
        let result = engine.snap(raw, (0.0, 0.0), &world, &mut spatial);

        assert_eq!(
            result.snap_type,
            SnapType::Nearest,
            "Step 6 always returns Nearest",
        );
        assert_eq!(
            result.source_entity,
            Some(entity),
            "should snap to the nearest entity",
        );
    }

    #[test]
    fn test_nearest_neighbor_prefers_closest_entity() {
        let mut engine = SnapEngine::new(SnapConfig::default());
        let mut world = World::new();
        let mut spatial = SpatialIndex::new();

        // Two entities: one near (0,0) and one at (100,100).
        let near_entity = make_line_entity(&mut world, 0.0, 0.0, 2.0, 2.0);
        let _far_entity = make_line_entity(&mut world, 100.0, 100.0, 102.0, 102.0);
        spatial.rebuild(&world);

        // Snap at (1,1) should find the near entity.
        let result = engine.snap(Point2D::new(1.0, 1.0), (0.0, 0.0), &world, &mut spatial);
        assert_eq!(result.source_entity, Some(near_entity));
    }

    // ------------------------------------------------------------------
    // enable / disable type
    // ------------------------------------------------------------------

    #[test]
    fn test_enable_disable_type() {
        let mut engine = SnapEngine::new(SnapConfig::default());

        // All 7 types start enabled.
        assert_eq!(engine.active_types.len(), 7);

        // Disable one.
        engine.disable_type(SnapType::Grid);
        assert_eq!(engine.active_types.len(), 6);
        assert!(!engine.active_types.contains(&SnapType::Grid));

        // Re-enable.
        engine.enable_type(SnapType::Grid);
        assert_eq!(engine.active_types.len(), 7);

        // Enabling again is a no-op (no duplicate).
        engine.enable_type(SnapType::Grid);
        assert_eq!(engine.active_types.len(), 7);
    }

    #[test]
    fn test_disable_all_types() {
        let mut engine = SnapEngine::new(SnapConfig::default());

        for &t in &[
            SnapType::Endpoint,
            SnapType::Midpoint,
            SnapType::Center,
            SnapType::Nearest,
            SnapType::Perpendicular,
            SnapType::Tangent,
            SnapType::Grid,
        ] {
            engine.disable_type(t);
        }

        assert!(engine.active_types.is_empty());
    }

    // ------------------------------------------------------------------
    // set_active_types
    // ------------------------------------------------------------------

    #[test]
    fn test_set_active_types_replaces() {
        let mut engine = SnapEngine::new(SnapConfig::default());
        engine.set_active_types(vec![SnapType::Endpoint, SnapType::Nearest]);

        assert_eq!(engine.active_types.len(), 2);
        assert!(engine.active_types.contains(&SnapType::Endpoint));
        assert!(engine.active_types.contains(&SnapType::Nearest));
    }

    // ------------------------------------------------------------------
    // Snap engine creation
    // ------------------------------------------------------------------

    #[test]
    fn test_new_has_all_types() {
        let engine = SnapEngine::new(SnapConfig::default());
        assert_eq!(engine.active_types.len(), 7);

        for &t in &[
            SnapType::Endpoint,
            SnapType::Midpoint,
            SnapType::Center,
            SnapType::Nearest,
            SnapType::Perpendicular,
            SnapType::Tangent,
            SnapType::Grid,
        ] {
            assert!(
                engine.active_types.contains(&t),
                "new engine should have {t:?} active",
            );
        }
    }

    #[test]
    fn test_snap_result_is_copy() {
        // Compile-time check: SnapResult derives Copy.
        let a = SnapResult {
            point: Point2D::new(1.0, 2.0),
            snap_type: SnapType::Endpoint,
            source_entity: None,
            distance_screen: 0.0,
        };
        let b = a; // Copy (not move).
        assert_eq!(a.point, b.point);
    }
}
