//! Forge v0.1.0 — Entry point and winit event loop.
//!
//! Initialises structured logging, creates the window and `ForgeApp`,
//! then runs the winit [`ApplicationHandler`] event loop.
//!
//! # Architecture
//!
//! The event loop is driven by winit 0.30's [`ApplicationHandler`] trait.
//! Lifecycle management follows the winit 0.30 pattern: `resumed()` is
//! called when the application has a window (macOS: on app activation),
//! and `window_event()` dispatches events to egui first, then to the
//! input mapper and command system.
//!
//! # Continuous rendering
//!
//! `about_to_wait()` calls `window.request_redraw()` every idle frame,
//! so the viewport runs at the display's refresh rate.  Occlusion-aware
//! rendering is deferred to v0.2.0+.

use std::sync::Arc;

use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowId},
};

use forge::app::ForgeApp;
use forge::commands::{CommandInput, CommandResult};
use forge::input::{apply_camera_action, InputAction};
use forge::selection::window_select::WindowSelectState;

// ─── ForgeState ──────────────────────────────────────────────────────────────

/// State that lives for the duration of the window's lifetime.
///
/// Created in [`ApplicationHandler::resumed`] and dropped when the
/// window is destroyed or the application is suspended.
pub struct ForgeState {
    window: Arc<Window>,
    app: ForgeApp,
}

// ─── ForgeAppHandler ─────────────────────────────────────────────────────────

/// Top-level event handler for the winit event loop.
///
/// `state` is `None` between `resumed` calls (e.g. before window creation
/// or after suspension on macOS).
#[derive(Default)]
pub struct ForgeAppHandler {
    state: Option<ForgeState>,
}

impl ForgeAppHandler {
    pub fn new() -> Self {
        Self::default()
    }
}

impl ApplicationHandler for ForgeAppHandler {
    // ── resumed ───────────────────────────────────────────────────────────
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // Drop any existing state before recreating.  `resumed` can fire
        // multiple times on macOS when the app is reactivated after being
        // hidden — without this check the previous GPU resources would leak.
        if self.state.is_some() {
            tracing::debug!(
                "resumed called with existing state — dropping prior window/GPU resources"
            );
            self.state = None;
        }

        let window_attributes = winit::window::WindowAttributes::default()
            .with_title("Forge v0.1.0")
            .with_inner_size(LogicalSize::new(1280, 720));

        let window = match event_loop.create_window(window_attributes) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                tracing::error!("Failed to create window: {}", e);
                event_loop.exit();
                return;
            }
        };

        tracing::info!("Window created, initialising GPU renderer...");
        let app = pollster::block_on(ForgeApp::new(window.clone()));

        self.state = Some(ForgeState { window, app });
        // Request the first redraw so the viewport renders immediately.
        // Subsequent frames are driven by `about_to_wait()` → `request_redraw()`.
        if let Some(state) = &self.state {
            state.window.request_redraw();
        }
        tracing::info!("Forge v0.1.0 ready");
    }

    // ── window_event ──────────────────────────────────────────────────────
    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(state) = &mut self.state else {
            return;
        };

        // ── Lifecycle events (handled before egui) ────────────────────────
        match &event {
            WindowEvent::CloseRequested => {
                tracing::info!("Window close requested, shutting down");
                event_loop.exit();
                return;
            }
            WindowEvent::Destroyed => {
                tracing::info!("Window destroyed, exiting event loop");
                self.state = None;
                event_loop.exit();
                return;
            }
            WindowEvent::Resized(new_size) => {
                state.app.render_state.resize(*new_size);
                state.app.resources.camera.viewport_size =
                    (new_size.width, new_size.height);
                state.app.render_state.grid_renderer.invalidate();
                state.window.request_redraw();
                // Fall through to egui so panels re-layout at the new size.
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                state
                    .app
                    .ui_system
                    .set_scale_factor(*scale_factor as f32);
                state.window.request_redraw();
                return;
            }
            WindowEvent::RedrawRequested => {
                // Render one frame.  Continuous rendering is driven by
                // `about_to_wait()` which calls `request_redraw()`.
                state.app.render(&state.window);
                return;
            }
            _ => {}
        }

        // ── Dispatch pending command text from the UI command line ────────
        if let Some(text) = state.app.command_state.pending_dispatch.take() {
            state.app.dispatch_command_text(&text);
        }

        // ── Process pending cancel requests (set by egui Escape handler) ──
        state
            .app
            .command_state
            .process_pending_cancel(&mut state.app.world);

        // ── 1. Let egui consume events first (UI priority) ────────────────
        let egui_consumed = state
            .app
            .ui_system
            .egui_state
            .on_window_event(&state.window, &event)
            .consumed;

        if egui_consumed {
            state.window.request_redraw();
            return;
        }

        // ── 2. Map remaining events to input actions (with snap) ─────────
        let actions = state.app.input_mapper.handle_event(
            &event,
            &state.app.resources.camera,
            &mut state.app.snap_engine,
            &state.app.world,
            &mut state.app.spatial_index,
        );

        for action in actions {
            match action {
                InputAction::Click(point) => {
                    if let Some(ref mut cmd) = state.app.command_state.active {
                        let result =
                            cmd.on_input(CommandInput::Point(point), &mut state.app.world);
                        match result {
                            CommandResult::Complete => {
                                state.app.command_state.active = None;
                                state.app.command_state.last_error = None;
                            }
                            CommandResult::Cancelled => {
                                state.app.command_state.active = None;
                                state.app.command_state.last_error = None;
                            }
                            CommandResult::Error(msg) => {
                                tracing::warn!("Command error: {}", msg);
                                state.app.command_state.last_error = Some(msg);
                            }
                            _ => {}
                        }
                    } else {
                        // No active command: start window select drag.
                        state.app.window_select_state =
                            Some(WindowSelectState::new(point, point));
                    }
                }
                InputAction::Pan(_, _) | InputAction::Zoom(_, _) => {
                    apply_camera_action(&mut state.app.resources.camera, &action);
                }
                InputAction::Cancel => {
                    if let Some(ref mut cmd) = state.app.command_state.active {
                        cmd.on_cancel(&mut state.app.world);
                    }
                    state.app.command_state.active = None;
                }
                InputAction::CommandText(ref text) => {
                    state.app.dispatch_command_text(text);
                }
                InputAction::MouseMoved(point) => {
                    // Update window select rectangle if dragging
                    if let Some(ref mut ws) = state.app.window_select_state {
                        ws.current = point;
                    }
                    // Request redraw so active command previews or the
                    // selection rectangle update.
                    if state.app.command_state.active.is_some()
                        || state.app.window_select_state.is_some()
                    {
                        state.window.request_redraw();
                    }
                }
                // Confirm, Text — handled by egui command line or deferred.
                _ => {}
            }
        }

        // ── 3. Window select: finalize on left release ─────────────────
        // When left_down becomes false while window_select_state is active,
        // query the spatial index and apply the selection.
        if state.app.window_select_state.is_some()
            && !state.app.input_mapper.state.left_down
        {
            let ws = state.app.window_select_state.take().unwrap();
            let entities = ws.query(&state.app.spatial_index);
            // Replace current selection with window-select results.
            state.app.selection_manager.clear(&mut state.app.world);
            for entity in entities {
                state.app
                    .selection_manager
                    .select(&mut state.app.world, entity);
            }
            state.window.request_redraw();
        }

        state.window.request_redraw();
    }

    // ── about_to_wait (continuous rendering) ──────────────────────────────
    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(state) = &self.state {
            // Continuous rendering: request redraw on every idle frame.
            // This keeps the viewport at the display's refresh rate.
            //
            // NOTE: This does not check window occlusion (minimised/hidden).
            // On macOS and Windows this may waste GPU cycles when the window
            // is not visible.  Track `WindowEvent::Occluded(bool)` and skip
            // redraw when occluded.  Deferred to v0.2.0+.
            state.window.request_redraw();
        }
    }
}

// ─── main ────────────────────────────────────────────────────────────────────

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "forge=info,wgpu=warn".into()),
        )
        .try_init()
        .ok();

    let event_loop = EventLoop::new().unwrap();
    let mut app = ForgeAppHandler::new();
    event_loop.run_app(&mut app).expect("Event loop failed");
}
