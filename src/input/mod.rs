//! Input abstraction.
//!
//! Maps winit window events to application-level actions.
//! Mouse and keyboard events are handled here; camera control (pan/zoom)
//! is delegated to the `camera_control` submodule.

use crate::ecs::resources::{CameraState, InputState};
use crate::geometry::Point2D;

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
                self.state.mouse_world = camera.screen_to_world(self.state.mouse_screen);
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
                        let click_world = camera.screen_to_world(self.state.mouse_screen);
                        actions.push(InputAction::Click(click_world));
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
                    MouseScrollDelta::PixelDelta(pos) => pos.y as f64 / 100.0,
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
