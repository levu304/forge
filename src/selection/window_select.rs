//! Window (rectangle) selection — crossing vs enclosing.
//!
//! Distinguishes selection modes based on drag direction:
//!
//! | Drag direction | Mode | Visual | Behaviour |
//! |---|---|---|---|
//! | Left → right (screen X increases) | `Enclosing` | Blue | Entities fully inside the rectangle |
//! | Right → left (screen X decreases) | `Crossing` | Green | Entities intersecting the rectangle |
//!
//! # Architecture
//!
//! [`WindowSelectState`] tracks a drag in world-space coordinates.  The
//! [`query`] method normalises the start/current points into a
//! [`BoundingBox2D`] and delegates to the [`SpatialIndex`] — either
//! [`enclosed_in`] (strict containment) or [`intersecting`] (overlap).
//!
//! Mode is determined from *screen-space* X coordinates (the drag direction
//! on screen) so it behaves naturally regardless of the current viewport
//! pan/zoom.
//!
//! [`query`]: WindowSelectState::query
//! [`SpatialIndex`]: crate::spatial::SpatialIndex
//! [`enclosed_in`]: crate::spatial::SpatialIndex::enclosed_in
//! [`intersecting`]: crate::spatial::SpatialIndex::intersecting

use crate::geometry::BoundingBox2D;
use crate::geometry::Point2D;
use crate::spatial::SpatialIndex;

// ---------------------------------------------------------------------------
// WindowSelectMode
// ---------------------------------------------------------------------------

/// Determines whether a rectangle selects entities that *intersect* it or
/// are *fully enclosed* by it.
///
/// | Variant | Spatial query | Typical use |
/// |---|---|---|
/// | `Crossing` | [`SpatialIndex::intersecting`] | Drag right → left (green) |
/// | `Enclosing` | [`SpatialIndex::enclosed_in`] | Drag left → right (blue) |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WindowSelectMode {
    /// Selects entities whose bounding box **overlaps** the rectangle
    /// (partial overlap is sufficient).
    Crossing,
    /// Selects entities whose bounding box is **strictly inside** the
    /// rectangle.
    #[default]
    Enclosing,
}

// ---------------------------------------------------------------------------
// WindowSelectState
// ---------------------------------------------------------------------------

/// Tracks an in-progress window selection drag.
///
/// Created when the user left-clicks and starts dragging with no active
/// command.  Stores the drag start and current positions in **world
/// coordinates** so that the selection rectangle is consistent regardless
/// of viewport changes.
///
/// # Example
///
/// ```ignore
/// use forge::geometry::Point2D;
/// use forge::selection::window_select::{WindowSelectMode, WindowSelectState};
///
/// let ws = WindowSelectState::new(
///     Point2D::new(10.0, 20.0),
///     Point2D::new(50.0, 60.0),
/// );
/// assert_eq!(ws.mode, WindowSelectMode::Enclosing);
/// ```
pub struct WindowSelectState {
    /// Drag start position in world coordinates.
    pub start: Point2D,
    /// Current drag position in world coordinates.
    pub current: Point2D,
    /// Selection mode — updated from screen-space drag direction on release.
    pub mode: WindowSelectMode,
    /// Drag start position in screen coordinates (for mode determination).
    pub start_screen: (f64, f64),
    /// Current drag position in screen coordinates (for mode determination).
    pub current_screen: (f64, f64),
}

impl WindowSelectState {
    /// Creates a new window selection state.
    ///
    /// The mode defaults to [`Enclosing`](WindowSelectMode::Enclosing) and
    /// should be updated by calling [`mode_from_drag`] on mouse release with
    /// the screen-space coordinates.
    pub fn new(
        start: Point2D,
        current: Point2D,
        mode: WindowSelectMode,
        start_screen: (f64, f64),
        current_screen: (f64, f64),
    ) -> Self {
        Self {
            start,
            current,
            mode,
            start_screen,
            current_screen,
        }
    }

    /// Determines the selection mode from the **screen-space** drag direction.
    ///
    /// * **Left → right** (`current_screen.x >= start_screen.x`) →
    ///   [`Enclosing`](WindowSelectMode::Enclosing)
    /// * **Right → left** (`current_screen.x < start_screen.x`) →
    ///   [`Crossing`](WindowSelectMode::Crossing)
    ///
    /// This is a static method so it can be called on mouse release without
    /// mutating an existing `WindowSelectState`.
    ///
    /// # Arguments
    ///
    /// * `start_screen` — The screen-space (viewport pixel) position where
    ///   the drag started `(x, y)`.
    /// * `current_screen` — The screen-space position at the end of the drag.
    ///
    /// Only the X coordinate matters for mode determination.
    pub fn mode_from_drag(
        start_screen: (f64, f64),
        current_screen: (f64, f64),
    ) -> WindowSelectMode {
        if current_screen.0 >= start_screen.0 {
            WindowSelectMode::Enclosing
        } else {
            WindowSelectMode::Crossing
        }
    }

    /// Queries the spatial index for entities matching the selection
    /// rectangle.
    ///
    /// Computes the normalised bounding box from [`self.start`] and
    /// [`self.current`] (swapping min/max as needed so the rect is valid
    /// regardless of drag direction), then dispatches to the spatial index
    /// based on [`self.mode`]:
    ///
    /// * [`Enclosing`](WindowSelectMode::Enclosing) →
    ///   [`spatial.enclosed_in(&rect)`](SpatialIndex::enclosed_in)
    /// * [`Crossing`](WindowSelectMode::Crossing) →
    ///   [`spatial.intersecting(&rect)`](SpatialIndex::intersecting)
    ///
    /// [`self.start`]: WindowSelectState::start
    /// [`self.current`]: WindowSelectState::current
    /// [`self.mode`]: WindowSelectState::mode
    pub fn query(&self, spatial: &SpatialIndex) -> Vec<hecs::Entity> {
        let rect = BoundingBox2D {
            min: Point2D::new(
                self.start.x.min(self.current.x),
                self.start.y.min(self.current.y),
            ),
            max: Point2D::new(
                self.start.x.max(self.current.x),
                self.start.y.max(self.current.y),
            ),
        };

        // A zero-area rectangle (user clicked without dragging) should
        // never produce a selection — neither enclosed nor intersecting.
        if rect.is_empty() {
            return Vec::new();
        }

        match self.mode {
            WindowSelectMode::Enclosing => spatial.enclosed_in(&rect),
            WindowSelectMode::Crossing => spatial.intersecting(&rect),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------
    // new()
    // ------------------------------------------------------------------

    #[test]
    fn new_creates_with_default_mode() {
        let ws = WindowSelectState::new(
            Point2D::new(10.0, 20.0),
            Point2D::new(50.0, 60.0),
            WindowSelectMode::Enclosing,
            (10.0, 20.0),
            (50.0, 60.0),
        );
        assert_eq!(ws.start, Point2D::new(10.0, 20.0));
        assert_eq!(ws.current, Point2D::new(50.0, 60.0));
        assert_eq!(ws.mode, WindowSelectMode::Enclosing);
        assert_eq!(ws.start_screen, (10.0, 20.0));
        assert_eq!(ws.current_screen, (50.0, 60.0));
    }

    #[test]
    fn new_reversed_points_stores_as_given() {
        // start > current is valid — the rect normalisation happens in query().
        let ws = WindowSelectState::new(
            Point2D::new(100.0, 200.0),
            Point2D::new(0.0, 0.0),
            WindowSelectMode::Enclosing,
            (100.0, 200.0),
            (0.0, 0.0),
        );
        assert_eq!(ws.start, Point2D::new(100.0, 200.0));
        assert_eq!(ws.current, Point2D::new(0.0, 0.0));
    }

    // ------------------------------------------------------------------
    // mode_from_drag()
    // ------------------------------------------------------------------

    #[test]
    fn mode_from_drag_left_to_right_is_enclosing() {
        let mode = WindowSelectState::mode_from_drag((100.0, 0.0), (300.0, 0.0));
        assert_eq!(mode, WindowSelectMode::Enclosing);
    }

    #[test]
    fn mode_from_drag_right_to_left_is_crossing() {
        let mode = WindowSelectState::mode_from_drag((300.0, 0.0), (100.0, 0.0));
        assert_eq!(mode, WindowSelectMode::Crossing);
    }

    #[test]
    fn mode_from_drag_equal_x_is_enclosing() {
        // No horizontal movement defaults to enclosing.
        let mode = WindowSelectState::mode_from_drag((100.0, 0.0), (100.0, 50.0));
        assert_eq!(mode, WindowSelectMode::Enclosing);
    }

    #[test]
    fn mode_from_drag_only_y_change_is_enclosing() {
        // Dragging straight down with no X change.
        let mode = WindowSelectState::mode_from_drag((200.0, 100.0), (200.0, 500.0));
        assert_eq!(mode, WindowSelectMode::Enclosing);
    }

    #[test]
    fn mode_from_drag_negative_coordinates() {
        // Both positions in negative screen space.
        let mode = WindowSelectState::mode_from_drag((-100.0, 0.0), (-50.0, 0.0));
        assert_eq!(mode, WindowSelectMode::Enclosing, "-50 >= -100 => enclosing");

        let mode = WindowSelectState::mode_from_drag((-50.0, 0.0), (-100.0, 0.0));
        assert_eq!(mode, WindowSelectMode::Crossing, "-100 < -50 => crossing");
    }

    // ------------------------------------------------------------------
    // query() — mode dispatch with real SpatialIndex
    // ------------------------------------------------------------------

    /// Helper: create a SpatialIndex with a single entity whose bounding
    /// box is `(0, 0)` to `(10, 10)`.
    fn index_with_one_entity() -> (SpatialIndex, hecs::Entity) {
        use crate::ecs::components::{LineData, Renderable};
        use crate::util::Color;

        let mut world = hecs::World::new();
        let entity = world.spawn((
            LineData {
                start: Point2D::new(0.0, 0.0),
                end: Point2D::new(10.0, 10.0),
                color: Color::WHITE,
                width: 1.0,
            },
            Renderable,
        ));

        let mut index = SpatialIndex::new();
        index.rebuild(&world);
        (index, entity)
    }

    /// Helper: create a SpatialIndex with one entity fully inside a rect
    /// and one entity partially overlapping.
    fn index_with_inside_and_overlapping() -> (SpatialIndex, hecs::Entity, hecs::Entity) {
        use crate::ecs::components::{LineData, Renderable};
        use crate::util::Color;

        let mut world = hecs::World::new();

        // Entity with bbox (2,2) to (8,8) — fully inside a (0,0)-(10,10) rect.
        let inside = world.spawn((
            LineData {
                start: Point2D::new(2.0, 2.0),
                end: Point2D::new(8.0, 8.0),
                color: Color::WHITE,
                width: 1.0,
            },
            Renderable,
        ));

        // Entity with bbox (5,5) to (15,15) — overlaps (0,0)-(10,10) but
        // NOT strictly inside.
        let overlap = world.spawn((
            LineData {
                start: Point2D::new(5.0, 5.0),
                end: Point2D::new(15.0, 15.0),
                color: Color::WHITE,
                width: 1.0,
            },
            Renderable,
        ));

        let mut index = SpatialIndex::new();
        index.rebuild(&world);
        (index, inside, overlap)
    }

    #[test]
    fn query_enclosing_returns_entities_fully_inside() {
        let (index, inside, overlap) = index_with_inside_and_overlapping();

        let ws = WindowSelectState {
            start: Point2D::new(0.0, 0.0),
            current: Point2D::new(10.0, 10.0),
            mode: WindowSelectMode::Enclosing,
            start_screen: (0.0, 0.0),
            current_screen: (10.0, 10.0),
        };

        let mut results = ws.query(&index);
        results.sort_by_key(|e| e.to_bits());

        assert_eq!(
            results,
            vec![inside],
            "only the fully inside entity should be enclosed"
        );
        assert!(
            !results.contains(&overlap),
            "the overlapping entity should NOT be enclosed"
        );
    }

    #[test]
    fn query_crossing_returns_intersecting_entities() {
        let (index, inside, overlap) = index_with_inside_and_overlapping();

        let ws = WindowSelectState {
            start: Point2D::new(0.0, 0.0),
            current: Point2D::new(10.0, 10.0),
            mode: WindowSelectMode::Crossing,
            start_screen: (0.0, 0.0),
            current_screen: (10.0, 10.0),
        };

        let mut results = ws.query(&index);
        results.sort_by_key(|e| e.to_bits());

        let mut expected = vec![inside, overlap];
        expected.sort_by_key(|e| e.to_bits());

        assert_eq!(
            results, expected,
            "both the inside and overlapping entity should intersect"
        );
    }

    #[test]
    fn query_rect_normalised_regardless_of_drag_direction() {
        // Dragging from (10,10) to (0,0) should produce the same rect
        // as dragging from (0,0) to (10,10).
        let (index, entity) = index_with_one_entity();

        let ws_reverse = WindowSelectState {
            start: Point2D::new(10.0, 10.0),
            current: Point2D::new(0.0, 0.0),
            mode: WindowSelectMode::Enclosing,
            start_screen: (10.0, 10.0),
            current_screen: (0.0, 0.0),
        };

        let results_reverse = ws_reverse.query(&index);
        assert_eq!(results_reverse, vec![entity]);
    }

    #[test]
    fn query_zero_area_rect_returns_empty() {
        // When start == current, the rect has no area.
        let (index, _entity) = index_with_one_entity();

        let ws = WindowSelectState {
            start: Point2D::new(5.0, 5.0),
            current: Point2D::new(5.0, 5.0),
            mode: WindowSelectMode::Enclosing,
            start_screen: (5.0, 5.0),
            current_screen: (5.0, 5.0),
        };

        let results = ws.query(&index);
        assert!(
            results.is_empty(),
            "zero-area rect should select nothing"
        );
    }

    #[test]
    fn query_zero_area_crossing_also_empty() {
        let (index, _entity) = index_with_one_entity();

        let ws = WindowSelectState {
            start: Point2D::new(5.0, 5.0),
            current: Point2D::new(5.0, 5.0),
            mode: WindowSelectMode::Crossing,
            start_screen: (5.0, 5.0),
            current_screen: (5.0, 5.0),
        };

        let results = ws.query(&index);
        assert!(
            results.is_empty(),
            "zero-area rect in crossing mode should also select nothing"
        );
    }

    #[test]
    fn query_empty_index_returns_empty() {
        let index = SpatialIndex::new();

        let ws = WindowSelectState {
            start: Point2D::new(-100.0, -100.0),
            current: Point2D::new(100.0, 100.0),
            mode: WindowSelectMode::Enclosing,
            start_screen: (-100.0, -100.0),
            current_screen: (100.0, 100.0),
        };

        let results = ws.query(&index);
        assert!(results.is_empty());
    }

    #[test]
    fn query_crossing_on_empty_index_returns_empty() {
        let index = SpatialIndex::new();

        let ws = WindowSelectState {
            start: Point2D::new(-100.0, -100.0),
            current: Point2D::new(100.0, 100.0),
            mode: WindowSelectMode::Crossing,
            start_screen: (-100.0, -100.0),
            current_screen: (100.0, 100.0),
        };

        let results = ws.query(&index);
        assert!(results.is_empty());
    }
}
