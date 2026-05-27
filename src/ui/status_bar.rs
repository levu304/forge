//! Status bar — bottom edge of the viewport.
//!
//! Displays cursor coordinates, zoom level, snap type indicators,
//! selection count, and version info.

use crate::ecs::resources::{CameraState, InputState};
use crate::selection::SelectionManager;
use crate::snap::SnapEngine;

// ---------------------------------------------------------------------------
// Pure helper functions (unit-testable without egui)
// ---------------------------------------------------------------------------

/// Format a coordinate value to 4 decimal places.
///
/// Example: `3.14159265` → `"3.1416"`.
pub fn format_coord(value: f64) -> String {
    format!("{:.4}", value)
}

/// Format a zoom factor as a percentage with 2 decimal places.
///
/// Example: `1.0` → `"100.00%"`.
pub fn format_zoom(zoom: f64) -> String {
    format!("{:.2}%", zoom * 100.0)
}

// ---------------------------------------------------------------------------
// UI draw function
// ---------------------------------------------------------------------------

/// Draw the status bar panel at the bottom of the screen.
///
/// Shows:
/// * Mouse world-space X and Y coordinates (4 decimal places).
/// * Current zoom level as a percentage.
/// * Active snap type indicators (yellow labels).
/// * Number of selected entities.
/// * "Forge v0.2.0" right-aligned.
pub fn draw(
    ui: &mut egui::Ui,
    camera: &CameraState,
    input: &InputState,
    snap: &SnapEngine,
    selection: &SelectionManager,
) {
    egui::Panel::bottom("status_bar")
        .exact_size(28.0)
        .show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(format!("X: {}", format_coord(input.mouse_world.x)));
                ui.label(format!("Y: {}", format_coord(input.mouse_world.y)));
                ui.separator();
                ui.label(format!("Zoom: {}", format_zoom(camera.zoom)));
                ui.separator();
                // Active snap type indicators
                for snap_type in &snap.active_types {
                    ui.colored_label(egui::Color32::YELLOW, format!("{:?}", snap_type));
                }
                ui.separator();
                ui.label(format!("Selected: {}", selection.count()));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label("Forge v0.2.0");
                });
            });
        });
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    use crate::ecs::resources::CameraState;
    use crate::geometry::Point2D;
    use crate::snap::{SnapEngine, SnapType};
    use crate::util::Color;
    use egui_kittest::{Harness, kittest::Queryable};

    // -- Helpers -------------------------------------------------------------

    /// Test state bundle that holds all parameters required by [`draw`].
    struct StatusBarTestState {
        camera: CameraState,
        input: InputState,
        snap: SnapEngine,
        selection: SelectionManager,
    }

    impl StatusBarTestState {
        fn new() -> Self {
            Self {
                camera: CameraState {
                    target: Point2D::new(0.0, 0.0),
                    zoom: 1.0,
                    viewport_size: (800, 600),
                    clear_color: Color::BLACK,
                },
                input: InputState::default(),
                snap: SnapEngine::new(crate::ecs::resources::SnapConfig::default()),
                selection: SelectionManager::new(),
            }
        }

        fn with_coords(mut self, x: f64, y: f64) -> Self {
            self.input.mouse_world = Point2D::new(x, y);
            self
        }

        fn with_zoom(mut self, zoom: f64) -> Self {
            self.camera.zoom = zoom;
            self
        }

        fn with_snap_types(mut self, types: Vec<SnapType>) -> Self {
            self.snap.set_active_types(types);
            self
        }

        fn with_selection_count(mut self, world: &mut hecs::World, count: usize) -> Self {
            for _ in 0..count {
                let e = world.spawn(());
                self.selection.select(world, e);
            }
            self
        }
    }

    /// Build a harness for testing the status bar with the given state.
    fn make_harness(state: StatusBarTestState) -> Harness<'static, StatusBarTestState> {
        Harness::new_ui_state(
            move |ui, st: &mut StatusBarTestState| {
                draw(ui, &st.camera, &st.input, &st.snap, &st.selection);
            },
            state,
        )
    }

    /// Shortcut: create a harness with default state.
    fn default_harness() -> Harness<'static, StatusBarTestState> {
        make_harness(StatusBarTestState::new())
    }

    // Helper: check that a node is considered visible (has non-zero rect)
    fn assert_visible(harness: &Harness<'_, StatusBarTestState>, label: &str) {
        let node = harness.get_by_label(label);
        let r = node.rect();
        assert!(r.size().x > 0.0 && r.size().y > 0.0, "label '{label}' not visible (rect: {r:?})");
    }

    // -- format_coord -- //

    #[test]
    fn test_format_coord_positive() {
        assert_eq!(format_coord(3.14159265), "3.1416");
    }

    #[test]
    fn test_format_coord_negative() {
        assert_eq!(format_coord(-42.0), "-42.0000");
    }

    #[test]
    fn test_format_coord_zero() {
        assert_eq!(format_coord(0.0), "0.0000");
    }

    #[test]
    fn test_format_coord_rounding() {
        // Fifth decimal is 9 → rounds up
        assert_eq!(format_coord(1.2345678), "1.2346");
    }

    #[test]
    fn test_format_coord_large_value() {
        assert_eq!(format_coord(123456.789), "123456.7890");
    }

    // -- format_zoom -- //

    #[test]
    fn test_format_zoom_100_percent() {
        assert_eq!(format_zoom(1.0), "100.00%");
    }

    #[test]
    fn test_format_zoom_zero() {
        assert_eq!(format_zoom(0.0), "0.00%");
    }

    #[test]
    fn test_format_zoom_large() {
        assert_eq!(format_zoom(100.0), "10000.00%");
    }

    #[test]
    fn test_format_zoom_half() {
        assert_eq!(format_zoom(0.5), "50.00%");
    }

    #[test]
    fn test_format_zoom_tiny() {
        assert_eq!(format_zoom(0.001), "0.10%");
    }

    // -- UI: Coordinates ----------------------------------------------------

    #[test]
    fn test_status_bar_shows_x_coordinate() {
        let mut harness = make_harness(
            StatusBarTestState::new().with_coords(42.5, 99.9),
        );
        harness.run();
        assert_visible(&harness, "X: 42.5000");
    }

    #[test]
    fn test_status_bar_shows_y_coordinate() {
        let mut harness = make_harness(
            StatusBarTestState::new().with_coords(42.5, 99.9),
        );
        harness.run();
        assert_visible(&harness, "Y: 99.9000");
    }

    #[test]
    fn test_status_bar_shows_negative_coordinates() {
        let mut harness = make_harness(
            StatusBarTestState::new().with_coords(-10.5, -20.3),
        );
        harness.run();
        assert_visible(&harness, "X: -10.5000");
    }

    // -- UI: Zoom -----------------------------------------------------------

    #[test]
    fn test_status_bar_shows_zoom() {
        let mut harness = make_harness(
            StatusBarTestState::new().with_zoom(1.0),
        );
        harness.run();
        assert_visible(&harness, "Zoom: 100.00%");
    }

    #[test]
    fn test_status_bar_shows_zoom_half() {
        let mut harness = make_harness(
            StatusBarTestState::new().with_zoom(0.5),
        );
        harness.run();
        assert_visible(&harness, "Zoom: 50.00%");
    }

    #[test]
    fn test_status_bar_shows_zoom_zero() {
        let mut harness = make_harness(
            StatusBarTestState::new().with_zoom(0.0),
        );
        harness.run();
        assert_visible(&harness, "Zoom: 0.00%");
    }

    // -- UI: Snap indicators ------------------------------------------------

    #[test]
    fn test_status_bar_shows_snap_indicators() {
        let mut harness = make_harness(
            StatusBarTestState::new()
                .with_snap_types(vec![SnapType::Endpoint, SnapType::Midpoint]),
        );
        harness.run();
        assert_visible(&harness, "Endpoint");
        assert_visible(&harness, "Midpoint");
    }

    #[test]
    fn test_status_bar_shows_all_snap_types() {
        let mut harness = make_harness(
            StatusBarTestState::new()
                .with_snap_types(vec![
                    SnapType::Endpoint,
                    SnapType::Midpoint,
                    SnapType::Center,
                    SnapType::Nearest,
                    SnapType::Perpendicular,
                    SnapType::Tangent,
                    SnapType::Grid,
                ]),
        );
        harness.run();
        for snap_type in &[
            SnapType::Endpoint,
            SnapType::Midpoint,
            SnapType::Center,
            SnapType::Nearest,
            SnapType::Perpendicular,
            SnapType::Tangent,
            SnapType::Grid,
        ] {
            assert_visible(&harness, &format!("{:?}", snap_type));
        }
    }

    #[test]
    fn test_status_bar_no_snap_indicators_when_empty() {
        let mut harness = make_harness(
            StatusBarTestState::new().with_snap_types(vec![]),
        );

        // Run once to render
        harness.run();

        // All labels we expect to be present:
        let snapshot = format!("{:?}", harness);
        // No SnapType variant should appear as a label
        // (we check indirectly — if no snap types are active, no colored_label
        //  calls are made for snap types; the separator still appears).
        // This test is mostly a smoke check that no panic occurs with empty types.
        assert!(!snapshot.contains("Endpoint"), "No Endpoint indicator when empty");
        assert!(!snapshot.contains("Midpoint"), "No Midpoint indicator when empty");
    }

    // -- UI: Selection count ------------------------------------------------

    #[test]
    fn test_status_bar_shows_selection_count() {
        let mut world = hecs::World::new();
        let mut selection = SelectionManager::new();
        for _ in 0..3 {
            let e = world.spawn(());
            selection.select(&mut world, e);
        }

        let harness_state = StatusBarTestState {
            camera: CameraState {
                target: Point2D::new(0.0, 0.0),
                zoom: 1.0,
                viewport_size: (800, 600),
                clear_color: Color::BLACK,
            },
            input: InputState::default(),
            snap: SnapEngine::new(crate::ecs::resources::SnapConfig::default()),
            selection,
        };

        let mut harness = make_harness(harness_state);
        harness.run();
        assert_visible(&harness, "Selected: 3");
    }

    #[test]
    fn test_status_bar_zero_selection() {
        let mut harness = make_harness(StatusBarTestState::new());
        harness.run();
        assert_visible(&harness, "Selected: 0");
    }

    // -- UI: Version --------------------------------------------------------

    #[test]
    fn test_status_bar_shows_version() {
        let mut harness = default_harness();
        harness.run();
        assert_visible(&harness, "Forge v0.2.0");
    }

    // -- UI: Combined full status bar --------------------------------------

    #[test]
    fn test_status_bar_all_elements_together() {
        let mut world = hecs::World::new();
        let mut selection = SelectionManager::new();
        let e = world.spawn(());
        selection.select(&mut world, e);

        let harness_state = StatusBarTestState {
            camera: CameraState {
                target: Point2D::new(10.0, 20.0),
                zoom: 2.0,
                viewport_size: (800, 600),
                clear_color: Color::BLACK,
            },
            input: InputState {
                mouse_world: Point2D::new(15.5, 25.5),
                ..InputState::default()
            },
            snap: SnapEngine::new(crate::ecs::resources::SnapConfig::default()),
            selection,
        };

        let mut harness = make_harness(harness_state);
        harness.run();

        assert_visible(&harness, "X: 15.5000");
        assert_visible(&harness, "Y: 25.5000");
        assert_visible(&harness, "Zoom: 200.00%");
        assert_visible(&harness, "Selected: 1");
        assert_visible(&harness, "Forge v0.2.0");
    }
}
