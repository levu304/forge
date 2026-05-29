//! UI chrome (egui).
//!
//! Immediate-mode UI panels: status bar, toolbar, command line,
//! and property inspector. Runs as a post-render overlay.
//!
//! ## Integration
//!
//! [`UiSystem`] owns the egui context and egui-winit state. Call
//! [`UiSystem::run`] once per frame, passing the current application
//! state so each panel can draw itself.

pub mod status_bar;
pub mod toolbar;
pub mod command_line;
pub mod property_panel;
pub mod layer_panel;

use crate::commands::CommandState;
use crate::ecs::resources::{CameraState, InputState};
use crate::selection::SelectionManager;
use crate::snap::SnapEngine;
use egui::ViewportId;

/// Output from one egui frame, carrying both rendered shapes and
/// texture changes (font atlas, images). The caller must pass
/// `textures_delta` to [`egui_wgpu::Renderer::update_textures`]
/// before the UI render pass so GPU-side textures stay in sync.
pub struct UiOutput {
    pub shapes: Vec<egui::epaint::ClippedShape>,
    pub textures_delta: egui::TexturesDelta,
}

/// Owns the egui context and winit integration state.
///
/// Dispatches draw calls to the four UI panels every frame.
pub struct UiSystem {
    pub egui_ctx: egui::Context,
    pub egui_state: egui_winit::State,
}

impl UiSystem {
    /// Create a new egui context and initialise the winit integration.
    ///
    /// `window` is used to determine the initial scale factor and theme.
    /// The egui-winit `State` handles event translation, clipboard, IME,
    /// and cursor management internally.
    pub fn new(window: &winit::window::Window) -> Self {
        let egui_ctx = egui::Context::default();
        let egui_state = egui_winit::State::new(
            egui_ctx.clone(),
            ViewportId::ROOT,
            window,
            Some(window.scale_factor() as f32), // native_pixels_per_point
            window.theme(),                      // initial theme
            None,                                // max_texture_side
        );
        Self { egui_ctx, egui_state }
    }

    /// Run one egui frame and return the rendered primitives.
    ///
    /// # Parameters
    ///
    /// * `window`    – The winit window (for input state and cursor management).
    /// * `camera`    – Current camera state (used by the status bar).
    /// * `input`     – Current input state (mouse coords shown in status bar).
    /// * `cmd_state` – Command state (command line prompt, text buffer, errors).
    /// * `world`     – The ECS world (used by the property panel).
    /// * `snap`      – Snap engine (snap type indicators in status bar).
    /// * `selection` – Selection manager (selection count in status bar).
    #[allow(clippy::too_many_arguments)]
    pub fn run(
        &mut self,
        window: &winit::window::Window,
        camera: &CameraState,
        input: &InputState,
        cmd_state: &mut CommandState,
        world: &hecs::World,
        snap: &SnapEngine,
        selection: &SelectionManager,
    ) -> UiOutput {
        let raw_input = self.egui_state.take_egui_input(window);

        let full_output = self.egui_ctx.run_ui(raw_input, |ui| {
            // Panel drawing functions accept &mut egui::Ui and use
            // egui::Panel::show_inside for proper window-edge docking.
            // This is the non-deprecated API in egui 0.34 (TopBottomPanel
            // and SidePanel aliases are deprecated; use Panel directly).
            status_bar::draw(ui, camera, input, snap, selection);
            toolbar::draw(ui, cmd_state);
            property_panel::draw(ui, world);
            command_line::draw(ui, cmd_state);
        });

        self.egui_state
            .handle_platform_output(window, full_output.platform_output);

        UiOutput {
            shapes: full_output.shapes,
            textures_delta: full_output.textures_delta,
        }
    }

    /// Update the egui pixels-per-point for a new scale factor.
    ///
    /// Should be called in response to `WindowEvent::ScaleFactorChanged`.
    pub fn set_scale_factor(&mut self, scale_factor: f32) {
        self.egui_ctx.set_pixels_per_point(scale_factor);
    }
}
