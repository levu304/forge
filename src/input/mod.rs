//! Input abstraction.
//!
//! Maps winit window events to application-level actions.
//! Mouse and keyboard events are handled here; camera control (pan/zoom)
//! is delegated to the `camera_control` submodule.
//!
//! # Snap integration
//!
//! Before emitting [`InputAction::MouseMoved`] the input mapper queries the
//! [`SnapEngine`] so that mouse world coordinates are snapped to geometry
//! (endpoints, midpoints, etc.) before commands or the UI see them.

use crate::ecs::resources::{CameraState, InputState};
use crate::geometry::Point2D;
use crate::snap::SnapEngine;
use crate::spatial::SpatialIndex;

pub mod camera_control;
pub use camera_control::apply_camera_action;
pub mod command_input; // reserved — all keyboard handling is in input/mod.rs for v0.1.0

/// Application-level input actions produced by the [`InputMapper`].
///
/// These actions are consumed by the application loop and dispatched
/// to the camera system, command system, or ignored.
#[derive(Debug, Clone, PartialEq)]
pub enum InputAction {
    /// Mouse moved to a new world-space position.
    MouseMoved(Point2D),
    /// Left mouse button clicked at a world-space position.
    Click(Point2D),
    /// Middle-drag pan: delta in screen pixels (dx, dy).
    Pan(f64, f64),
    /// Scroll-wheel zoom: delta amount and world-space pivot point.
    Zoom(f64, Point2D),
    /// Cancel / abort the current operation (Escape key).
    Cancel,
    /// Confirm the current operation (Enter key).
    Confirm,
    /// A single keyboard character (reserved — egui swallows keyboard when
    /// its text field has focus, so this is never produced in normal use).
    #[allow(dead_code)]
    Text(char),
    /// Full command-line submission from the UI (reserved — not produced
    /// by [`InputMapper::handle_event`] in v0.1.0).
    #[allow(dead_code)]
    CommandText(String),
}

/// Maps winit [`WindowEvent`]s to [`InputAction`]s.
///
/// Maintains the current [`InputState`] snapshot and tracks the last
/// mouse position for computing pan deltas during middle-button drag.
pub struct InputMapper {
    /// Current input state (mouse position, button states, modifiers).
    pub state: InputState,
    /// Previous frame's mouse position in screen pixels, used for
    /// pan-delta calculation during middle-button drag.
    pub last_mouse_screen: (f32, f32),
    /// Tracks whether the first `CursorMoved` after a middle-click should
    /// be used as the pan baseline rather than producing a pan delta.
    /// Prevents an incorrect huge pan when middle-clicking before any
    /// mouse movement has occurred.
    pub needs_pan_baseline: bool,
}

impl InputMapper {
    /// Create a default-initialised [`InputMapper`].
    ///
    /// The initial mouse position is set to `(0.0, 0.0)`, the
    /// [`InputState`] is zeroed (no buttons pressed, no modifiers
    /// active), and the pan baseline is empty (meaning the first
    /// pan delta will be ignored until a `CursorMoved` event
    /// establishes a reference position).
    pub fn new() -> Self {
        Self {
            state: InputState::default(),
            last_mouse_screen: (0.0, 0.0),
            needs_pan_baseline: false,
        }
    }

    /// Translate a winit [`WindowEvent`] into zero or more [`InputAction`]s.
    ///
    /// Updates `self.state` as a side effect (mouse position, button state,
    /// modifier keys). The returned [`Vec<InputAction>`] is consumed by the
    /// application loop.
    ///
    /// ## Snap integration
    ///
    /// On [`CursorMoved`](winit::event::WindowEvent::CursorMoved) the raw
    /// world-space cursor position is passed through [`SnapEngine::snap`]
    /// before being stored in [`state.mouse_world`](InputState::mouse_world)
    /// and emitted as [`MouseMoved`](InputAction::MouseMoved).
    ///
    /// ## Event priority (delegated to caller)
    ///
    /// This method does **not** decide whether an event is consumed by egui.
    /// The caller is responsible for passing events to egui first and only
    /// forwarding unconsumed events here.
    ///
    /// ## Pan tracking
    ///
    /// Pan actions are produced at the end of processing by comparing the
    /// current mouse position against `last_mouse_screen` when the middle
    /// mouse button is held down.
    ///
    /// ## Performance note (v0.1.0)
    ///
    /// A fresh `Vec` is allocated on every call (including mouse-move events
    /// at 60 fps). This is acceptable for v0.1.0. Consider a small-vector
    /// optimisation or a reusable buffer for v0.2.0+.
    #[must_use = "returned actions must be dispatched to the app loop (e.g., Click, Pan, Zoom)"]
    pub fn handle_event(
        &mut self,
        event: &winit::event::WindowEvent,
        camera: &CameraState,
        snap_engine: &mut SnapEngine,
        world: &hecs::World,
        spatial: &mut SpatialIndex,
    ) -> Vec<InputAction> {
        use winit::{
            event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
            keyboard::{Key, KeyCode, PhysicalKey},
        };

        let mut actions = Vec::new();

        match event {
            // ── Mouse movement ────────────────────────────────────────
            WindowEvent::CursorMoved { position, .. } => {
                let sx = position.x.clamp(0.0, 65536.0) as f32;
                let sy = position.y.clamp(0.0, 65536.0) as f32;
                self.state.mouse_screen = (sx, sy);
                let raw_world = camera.screen_to_world(self.state.mouse_screen);
                // Snap the world coordinates before storing / emitting.
                let snapped = snap_engine.snap(
                    raw_world,
                    self.state.mouse_screen,
                    world,
                    spatial,
                    camera,
                );
                self.state.mouse_world = snapped.point;
                actions.push(InputAction::MouseMoved(self.state.mouse_world));
            }

            // ── Mouse button press / release ──────────────────────────
            WindowEvent::MouseInput {
                state: press_state,
                button,
                ..
            } => match press_state {
                ElementState::Pressed => match button {
                    MouseButton::Left => {
                        self.state.left_down = true;
                        // Use snapped mouse_world (set during last CursorMoved) so
                        // commands receive the exact snap target coordinate.
                        actions.push(InputAction::Click(self.state.mouse_world));
                    }
                    MouseButton::Middle => {
                        self.state.middle_down = true;
                        self.needs_pan_baseline = true;
                        self.last_mouse_screen = self.state.mouse_screen;
                    }
                    MouseButton::Right => {
                        self.state.right_down = true;
                        // Right-click reserved for CAD context menu (v0.2.0+).
                    }
                    _ => {}
                },
                ElementState::Released => match button {
                    MouseButton::Left => self.state.left_down = false,
                    MouseButton::Middle => self.state.middle_down = false,
                    MouseButton::Right => self.state.right_down = false,
                    _ => {}
                },
            },

            // ── Scroll wheel ──────────────────────────────────────────
            WindowEvent::MouseWheel { delta, .. } => {
                // LineDelta: raw line ticks (positive = scroll up / zoom in).
                // PixelDelta: precise pixel deltas; normalise to ~line scale.
                let dy = match delta {
                    MouseScrollDelta::LineDelta(_, y) => *y as f64,
                    // PixelDelta follows screen convention: positive Y = scroll DOWN.
                    // LineDelta uses logical convention: positive Y = scroll UP.
                    // Negate PixelDelta so both produce consistent Zoom delta direction.
                    MouseScrollDelta::PixelDelta(pos) => -pos.y as f64 / 100.0,
                };
                // Defensive clamp: limits zoom to ~2.6× per event (1.1^10 ≈ 2.59).
                // Prevents extreme zoom from buggy drivers or synthetic events.
                let dy = dy.clamp(-10.0, 10.0);
                let zoom_pivot = camera.screen_to_world(self.state.mouse_screen);
                actions.push(InputAction::Zoom(dy, zoom_pivot));
            }

            // ── Keyboard ──────────────────────────────────────────────
            WindowEvent::KeyboardInput { event, .. } => match event.state {
                ElementState::Pressed => match event.physical_key {
                    PhysicalKey::Code(KeyCode::Escape) => {
                        actions.push(InputAction::Cancel);
                    }
                    PhysicalKey::Code(KeyCode::Enter) => {
                        actions.push(InputAction::Confirm);
                    }
                    PhysicalKey::Code(KeyCode::ShiftLeft)
                    | PhysicalKey::Code(KeyCode::ShiftRight) => {
                        self.state.shift = true;
                    }
                    PhysicalKey::Code(KeyCode::ControlLeft)
                    | PhysicalKey::Code(KeyCode::ControlRight) => {
                        self.state.ctrl = true;
                    }
                    PhysicalKey::Code(KeyCode::AltLeft)
                    | PhysicalKey::Code(KeyCode::AltRight) => {
                        self.state.alt = true;
                    }
                    _ => {
                        // Individual keystrokes (not swallowed by egui).
                        if let Key::Character(c) = &event.logical_key {
                            if let Some(ch) = c.chars().next() {
                                actions.push(InputAction::Text(ch));
                            }
                        }
                    }
                },
                ElementState::Released => match event.physical_key {
                    PhysicalKey::Code(KeyCode::ShiftLeft)
                    | PhysicalKey::Code(KeyCode::ShiftRight) => {
                        self.state.shift = false;
                    }
                    PhysicalKey::Code(KeyCode::ControlLeft)
                    | PhysicalKey::Code(KeyCode::ControlRight) => {
                        self.state.ctrl = false;
                    }
                    PhysicalKey::Code(KeyCode::AltLeft)
                    | PhysicalKey::Code(KeyCode::AltRight) => {
                        self.state.alt = false;
                    }
                    _ => {}
                },
            },

            // All other window events are ignored.
            _ => {}
        }

        // ── Pan tracking (middle-drag) ────────────────────────────────
        // Compute delta between the current cursor position and the last
        // recorded position. The delta is in screen pixels and is converted
        // to world units by the camera controller.
        if self.state.middle_down {
            if self.needs_pan_baseline {
                // First CursorMoved after middle-click: use current position
                // as baseline without producing a pan delta. This prevents a
                // huge incorrect pan when the user middle-clicks before any
                // CursorMoved event has been received.
                self.last_mouse_screen = self.state.mouse_screen;
                self.needs_pan_baseline = false;
            } else {
                let dx = self.state.mouse_screen.0 - self.last_mouse_screen.0;
                let dy = self.state.mouse_screen.1 - self.last_mouse_screen.1;
                if dx != 0.0 || dy != 0.0 {
                    actions.push(InputAction::Pan(dx as f64, dy as f64));
                }
                self.last_mouse_screen = self.state.mouse_screen;
            }
        }

        actions
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::components::{LineData, Renderable};
    use crate::ecs::resources::SnapConfig;
    use crate::snap::SnapEngine;
    use crate::spatial::SpatialIndex;
    use crate::util::Color;
    use hecs::World;
    use winit::{
        dpi::PhysicalPosition,
        event::{DeviceId, MouseScrollDelta, TouchPhase, WindowEvent},
    };

    /// Helper: create default test resources (snap engine, world, spatial index).
    fn test_resources() -> (SnapEngine, World, SpatialIndex) {
        let snap_engine = SnapEngine::new(SnapConfig::default());
        let world = World::new();
        let spatial = SpatialIndex::new();
        (snap_engine, world, spatial)
    }

    /// Helper to construct a `MouseWheel` event for testing.
    fn make_wheel_event(delta: MouseScrollDelta) -> WindowEvent {
        WindowEvent::MouseWheel {
            device_id: DeviceId::dummy(),
            delta,
            phase: TouchPhase::Moved,
        }
    }

    /// `LineDelta` with positive Y produces a **positive** `Zoom` delta.
    #[test]
    fn line_delta_positive_y_zooms_in() {
        let mut mapper = InputMapper::new();
        let camera = CameraState {
            target: Point2D::new(0.0, 0.0),
            zoom: 1.0,
            viewport_size: (1000, 1000),
            clear_color: Color::BLACK,
        };
        let (mut snap_engine, world, mut spatial) = test_resources();

        let event = make_wheel_event(MouseScrollDelta::LineDelta(0.0, 1.0));
        let actions = mapper.handle_event(&event, &camera, &mut snap_engine, &world, &mut spatial);

        let zoom_action = actions.iter().find_map(|a| {
            if let InputAction::Zoom(dy, _) = a { Some(*dy) } else { None }
        });
        assert!(zoom_action.is_some(), "expected a Zoom action");
        assert!(
            zoom_action.unwrap() > 0.0,
            "LineDelta positive Y should zoom in (positive delta), got {}",
            zoom_action.unwrap()
        );
    }

    /// Helper: build a MouseInput press event for left click.
    fn make_left_click_event() -> WindowEvent {
        WindowEvent::MouseInput {
            device_id: DeviceId::dummy(),
            state: winit::event::ElementState::Pressed,
            button: winit::event::MouseButton::Left,
        }
    }

    #[test]
    fn snap_integration_click_uses_snapped_world() {
        let mut mapper = InputMapper::new();
        let camera = CameraState {
            target: Point2D::new(0.0, 0.0),
            zoom: 1.0,
            viewport_size: (800, 600),
            clear_color: Color::BLACK,
        };

        let mut snap_engine = SnapEngine::new(SnapConfig::default());
        let mut world = World::new();
        let mut spatial = SpatialIndex::new();

        // Line endpoint at (0,0) — within snap aperture of screen centre.
        let _entity = world.spawn((
            LineData {
                start: Point2D::new(0.0, 0.0),
                end: Point2D::new(10.0, 10.0),
                color: Color::WHITE,
                width: 1.0,
            },
            Renderable,
        ));
        spatial.rebuild(&world);

        // Step 1: move cursor near screen centre — raw world (5,0), endpoint (0,0) is
        // 5px away (within 12px aperture). Snapped must correct to (0,0).
        // If snap is broken, we'd get (5,0) and the test fails.
        let move_event = make_cursor_event(405.0, 300.0);
        mapper.handle_event(&move_event, &camera, &mut snap_engine, &world, &mut spatial);

        // Step 2: click — should emit Click action with already-snapped coords.
        let click_event = make_left_click_event();
        let actions = mapper.handle_event(&click_event, &camera, &mut snap_engine, &world, &mut spatial);

        let click = actions.iter().find_map(|a| {
            if let InputAction::Click(p) = a { Some(*p) } else { None }
        });
        assert!(
            click.is_some(),
            "expected a Click action",
        );
        let clicked = click.unwrap();
        assert!(
            (clicked.x - 0.0).abs() < 0.01,
            "expected click x near 0.0 (snapped endpoint), got {}",
            clicked.x,
        );
        assert!(
            (clicked.y - 0.0).abs() < 0.01,
            "expected click y near 0.0 (snapped endpoint), got {}",
            clicked.y,
        );
    }

    // ── InputMapper construction ─────────────────────────────────

    #[test]
    fn test_input_mapper_new_default_state() {
        let mapper = InputMapper::new();
        // No buttons pressed.
        assert!(!mapper.state.left_down);
        assert!(!mapper.state.middle_down);
        assert!(!mapper.state.right_down);
        // No modifiers active.
        assert!(!mapper.state.shift);
        assert!(!mapper.state.ctrl);
        assert!(!mapper.state.alt);
        // Pan baseline not pending.
        assert!(!mapper.needs_pan_baseline);
    }

    #[test]
    fn test_input_state_default_values() {
        let state = InputState::default();
        assert_eq!(state.mouse_screen, (0.0, 0.0));
        assert_eq!(state.mouse_world, Point2D::default());
        assert!(!state.left_down);
        assert!(!state.middle_down);
        assert!(!state.right_down);
        assert!(!state.shift);
        assert!(!state.ctrl);
        assert!(!state.alt);
    }

    #[test]
    fn input_mapper_initial_mouse_position() {
        let mapper = InputMapper::new();
        assert_eq!(mapper.last_mouse_screen, (0.0, 0.0));
    }

    /// `PixelDelta` with positive Y produces a **negative** `Zoom` delta
    /// (screen Y+ is down, world Y+ is up).
    #[test]
    fn pixel_delta_positive_y_zooms_out() {
        let mut mapper = InputMapper::new();
        let camera = CameraState {
            target: Point2D::new(0.0, 0.0),
            zoom: 1.0,
            viewport_size: (1000, 1000),
            clear_color: Color::BLACK,
        };
        let (mut snap_engine, world, mut spatial) = test_resources();

        let event = make_wheel_event(MouseScrollDelta::PixelDelta(
            PhysicalPosition::new(0.0, 100.0),
        ));
        let actions = mapper.handle_event(&event, &camera, &mut snap_engine, &world, &mut spatial);

        let zoom_action = actions.iter().find_map(|a| {
            if let InputAction::Zoom(dy, _) = a { Some(*dy) } else { None }
        });
        assert!(zoom_action.is_some(), "expected a Zoom action");
        assert!(
            zoom_action.unwrap() < 0.0,
            "PixelDelta positive Y should zoom out (negative delta), got {}",
            zoom_action.unwrap()
        );
    }

    // ------------------------------------------------------------------
    // Snap integration
    // ------------------------------------------------------------------

    /// Helper: build a CursorMoved event at a given screen position.
    fn make_cursor_event(x: f64, y: f64) -> WindowEvent {
        WindowEvent::CursorMoved {
            device_id: DeviceId::dummy(),
            position: PhysicalPosition::new(x, y),
        }
    }

    #[test]
    fn snap_integration_snaps_to_endpoint_on_mouse_move() {
        let mut mapper = InputMapper::new();
        let camera = CameraState {
            target: Point2D::new(0.0, 0.0),
            zoom: 1.0,
            viewport_size: (800, 600),
            clear_color: Color::BLACK,
        };

        let mut snap_engine = SnapEngine::new(SnapConfig::default());
        let mut world = World::new();
        let mut spatial = SpatialIndex::new();

        // Create a line from (0,0) to (10,10) so the endpoint (0,0) is
        // within snap aperture when the cursor is at screen centre.
        let _entity = world.spawn((
            LineData {
                start: Point2D::new(0.0, 0.0),
                end: Point2D::new(10.0, 10.0),
                color: Color::WHITE,
                width: 1.0,
            },
            Renderable,
        ));
        spatial.rebuild(&world);

        // Screen (405,300) maps to raw world (5,0) at zoom=1.
        // The line endpoint (0,0) is 5px away in screen-space (within 12px aperture).
        // If snap is broken, we'd get (5,0) and the test fails.
        let event = make_cursor_event(405.0, 300.0);
        let actions = mapper.handle_event(
            &event,
            &camera,
            &mut snap_engine,
            &world,
            &mut spatial,
        );

        let moved = actions.iter().find_map(|a| {
            if let InputAction::MouseMoved(p) = a { Some(*p) } else { None }
        });
        assert!(
            moved.is_some(),
            "expected a MouseMoved action",
        );
        let snapped = moved.unwrap();
        assert!(
            (snapped.x - 0.0).abs() < 0.01,
            "expected snapped x near 0.0, got {}",
            snapped.x,
        );
        assert!(
            (snapped.y - 0.0).abs() < 0.01,
            "expected snapped y near 0.0, got {}",
            snapped.y,
        );
        assert!(
            snap_engine.last_result.is_some(),
            "snap_engine.last_result should be set after snap",
        );
    }
}
