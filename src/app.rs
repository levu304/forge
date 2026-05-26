//! Application state and render loop.
//!
//! [`ForgeApp`] owns the ECS world, GPU render state, UI system, input
//! mapper, and command state.  It provides `render()` for the per-frame
//! draw loop and `dispatch_command_text()` for processing command-line
//! input from the UI.
//!
//! # Ownership
//!
//! ```text
//! ForgeApp
//! ├── world          (hecs::World — entities & components)
//! ├── resources      (ResourceBank — camera, grid config)
//! ├── command_state  (CommandState — active command, history, errors)
//! ├── render_state   (RenderState — GPU device, pipelines, buffers)
//! ├── ui_system      (UiSystem — egui context & panels)
//! └── input_mapper   (InputMapper — winit event → InputAction)
//! ```

use std::sync::Arc;

use winit::window::Window;

use crate::commands::{
    self, line_cmd::LineCommand, Command, CommandInput, CommandResult, CommandState,
};
use crate::ecs::resources::{CameraState, GridConfig};
use crate::geometry::Point2D;
use crate::input::InputMapper;
use crate::render::RenderState;
use crate::ui::UiSystem;
use crate::util::Color;

// ─── ResourceBank ────────────────────────────────────────────────────────────

/// Holds singleton ECS resources that are not themselves entities.
///
/// In v0.1.0 these are stored outside the ECS `World` for simplicity.
/// Future versions may migrate them into `hecs` resources.
pub struct ResourceBank {
    pub camera: CameraState,
    pub grid: GridConfig,
}

// ─── ForgeApp ────────────────────────────────────────────────────────────────

/// Top-level application state.
///
/// Created once per window lifecycle in `ForgeApp::new()`, then driven
/// by the winit event loop via `render()` and `dispatch_command_text()`.
pub struct ForgeApp {
    /// ECS world holding all entities and components.
    pub world: hecs::World,
    /// Singleton resources (camera, grid config).
    pub resources: ResourceBank,
    /// Active command, history, and error state.
    pub command_state: CommandState,
    /// GPU device, surface, pipelines, and buffers.
    pub render_state: RenderState,
    /// egui context, winit integration, and panel draw functions.
    pub ui_system: UiSystem,
    /// Maps winit events to `InputAction`s.
    pub input_mapper: InputMapper,
}

impl ForgeApp {
    /// Create a new `ForgeApp` with GPU initialisation.
    ///
    /// This is an async constructor because [`RenderState::new`] performs
    /// asynchronous adapter/device enumeration.  The caller should use
    /// `pollster::block_on` to drive it to completion synchronously.
    pub async fn new(window: Arc<Window>) -> Self {
        let world = hecs::World::new();

        let resources = ResourceBank {
            camera: CameraState {
                target: Point2D::new(0.0, 0.0),
                zoom: 1.0,
                viewport_size: (1280, 720),
                clear_color: Color::from_hex(0x2B2B2B),
            },
            grid: GridConfig::default(),
        };

        let render_state = RenderState::new(window.clone())
            .await
            .unwrap_or_else(|e| {
                panic!("Failed to initialise GPU render state: {}", e)
            });

        let ui_system = UiSystem::new(&window);
        let input_mapper = InputMapper::new();

        Self {
            world,
            resources,
            command_state: CommandState::default(),
            render_state,
            ui_system,
            input_mapper,
        }
    }

    /// Render one frame.
    ///
    /// Per-frame pipeline (in order):
    ///
    /// 1. Acquire a surface texture.
    /// 2. Update the camera uniform buffer with the current view-projection matrix.
    /// 3. Grid render pass (clears the framebuffer).
    /// 4. Entity render pass (ECS geometry — `LoadOp::Load`).
    /// 5. egui UI pass (overlay — `LoadOp::Load`).
    /// 6. Submit command encoder and present.
    pub fn render(&mut self, window: &Window) {
        // ── 1. Acquire surface texture ───────────────────────────────────
        // wgpu 29: get_current_texture() returns CurrentSurfaceTexture enum
        // (not a Result).  Variants:
        //   Success(texture)  — valid frame
        //   Suboptimal(texture) — valid but reconfig recommended
        //   Timeout / Occluded / Outdated / Lost — skip frame, recover.
        let output = match self.render_state.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture)
            | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => texture,
            wgpu::CurrentSurfaceTexture::Timeout
            | wgpu::CurrentSurfaceTexture::Occluded
            | wgpu::CurrentSurfaceTexture::Validation => {
                // Transient — skip this frame.
                return;
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                tracing::warn!("Surface outdated, reconfiguring and requesting redraw");
                self.render_state.resize(window.inner_size());
                self.render_state.grid_renderer.invalidate();
                window.request_redraw();
                return;
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                tracing::warn!("Surface lost, reconfiguring and requesting redraw");
                self.render_state.resize(window.inner_size());
                self.render_state.grid_renderer.invalidate();
                window.request_redraw();
                return;
            }
        };

        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .render_state
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        // ── 2. Update camera uniform ─────────────────────────────────────
        let vp_matrix = self.resources.camera.view_proj_matrix();
        self.render_state.queue.write_buffer(
            &self.render_state.camera_buffer,
            0,
            bytemuck::cast_slice(vp_matrix.as_ref()),
        );

        // ── 3. Clear framebuffer (unconditional) ─────────────────────────
        //
        // Always clear the framebuffer to `clear_color` before any drawing.
        // The grid pass (when visible) and entity pass both depend on a clean
        // starting buffer — without this unconditional clear, a hidden grid
        // would leave stale swapchain content visible.
        {
            let cc = &self.resources.camera.clear_color;
            let _clear_pass =
                encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Clear Render Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: cc.r as f64,
                                g: cc.g as f64,
                                b: cc.b as f64,
                                a: cc.a as f64,
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
            // No draw calls — drop `clear_pass` to finish the pass.
            // The framebuffer is now clean for subsequent passes.
        }

        // ── 4. Grid pass (draw only — framebuffer already cleared) ────────
        if self.resources.grid.visible {
            self.render_state.grid_renderer.render(
                &mut encoder,
                &view,
                &self.resources.camera,
                &self.resources.grid,
                &self.render_state.camera_bind_group,
                &self.render_state.queue,
                &self.render_state.device,
            );
        }

        // ── 4. Entity pass (LoadOp::Load — grid already cleared) ─────────
        self.render_state.entity_renderer.render(
            &mut encoder,
            &view,
            self.resources.camera.zoom,
            &self.world,
            &self.render_state.camera_bind_group,
            &self.render_state.queue,
            &self.render_state.device,
        );

        // ── 5. UI pass (egui overlay) ────────────────────────────────────
        let ui_output = self.ui_system.run(
            window,
            &self.resources.camera,
            &self.input_mapper.state,
            &mut self.command_state,
            &self.world,
        );

        let screen_size = self.resources.camera.viewport_size;
        let screen_descriptor = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [screen_size.0, screen_size.1],
            pixels_per_point: window.scale_factor() as f32,
        };

        // Upload any new textures (font atlas, images).
        // egui-wgpu 0.34 uses per-texture update_texture() on each ImageDelta.
        for (texture_id, image_delta) in &ui_output.textures_delta.set {
            self.render_state.egui_renderer.update_texture(
                &self.render_state.device,
                &self.render_state.queue,
                *texture_id,
                image_delta,
            );
        }

        // Tessellate egui shapes into GPU-friendly clipped primitives.
        let clipped_primitives = self
            .ui_system
            .egui_ctx
            .tessellate(ui_output.shapes, screen_descriptor.pixels_per_point);

        // Upload vertex and index buffers for the primitives.
        // Returns command buffers (buffer uploads) that need separate submission.
        let egui_cmdbufs = self.render_state.egui_renderer.update_buffers(
            &self.render_state.device,
            &self.render_state.queue,
            &mut encoder,
            &clipped_primitives,
            &screen_descriptor,
        );

        // ── 4. UI render pass ────────────────────────────────────────────
        //
        // SAFETY: egui_wgpu::Renderer::render() takes &mut RenderPass<'static>
        // for internal reasons, but our RenderPass borrows from `encoder`.
        // We scope it in a block so it drops before encoder.finish(), and
        // use unsafe to satisfy the 'static bound.  This is the standard
        // pattern used by egui-wgpu examples — the 'static is a formality
        // (the wgpu runtime validates pass/encoder interleaving).
        {
            let mut render_pass =
                encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("UI Render Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
            // SAFETY: render_pass is dropped before encoder.finish() below.
            let rp_static: &mut wgpu::RenderPass<'static> =
                unsafe { std::mem::transmute(&mut render_pass) };
            self.render_state.egui_renderer.render(
                rp_static,
                &clipped_primitives,
                &screen_descriptor,
            );
        }

        // ── 5. Free textures no longer referenced by egui ─────────────────
        //
        // egui's `TexturesDelta::free` lists texture IDs that were evicted
        // from the atlas (e.g. after a font atlas regeneration or DPI change).
        // Failing to free them leaks GPU texture memory.
        for texture_id in &ui_output.textures_delta.free {
            self.render_state.egui_renderer.free_texture(texture_id);
        }

        // ── 6. Submit and present ──────────────────────────────────────────
        let mut cmds: Vec<wgpu::CommandBuffer> = Vec::with_capacity(1 + egui_cmdbufs.len());
        cmds.push(encoder.finish());
        cmds.extend(egui_cmdbufs);
        self.render_state.queue.submit(cmds);
        output.present();
    }

    /// Dispatch text from the UI command line to the command system.
    ///
    /// The text buffer is parsed and either:
    ///
    /// * Feeds the text as a [`CommandInput`] to the active command if one
    ///   exists (trying `Point` first, then falling back to `Text`).
    /// * Parses the full text as a command invocation when no command is
    ///   active (e.g. `"LINE 0,0 100,100"` → creates a `LineCommand`,
    ///   feeds inline points, and auto-commits if all steps are satisfied).
    ///
    /// Unimplemented commands (CIRCLE, ARC, PLINE) set `last_error` with
    /// a user-visible message.  Parse failures also surface through
    /// `last_error`.
    pub fn dispatch_command_text(&mut self, text: &str) {
        // ── Active command: feed text as input ───────────────────────────
        if let Some(ref mut cmd) = self.command_state.active {
            let trimmed = text.trim();

            let input = if let Ok((_, point)) = commands::parser::parse_point(trimmed) {
                Some(CommandInput::Point(point))
            } else {
                Some(CommandInput::Text(trimmed.to_string()))
            };

            if let Some(input) = input {
                let result = cmd.on_input(input, &mut self.world);
                match result {
                    CommandResult::Complete => {
                        self.command_state.active = None;
                        self.command_state.last_error = None;
                    }
                    CommandResult::Cancelled => {
                        self.command_state.active = None;
                        self.command_state.last_error = None;
                    }
                    CommandResult::Error(msg) => {
                        tracing::warn!("Command error: {}", msg);
                        self.command_state.last_error = Some(msg);
                    }
                    _ => {}
                }
            }
            return;
        }

        // ── No active command: parse full text ───────────────────────────
        match commands::parser::parse_command(text.trim()) {
            Ok((remaining, parsed)) => {
                if !remaining.trim().is_empty() {
                    tracing::warn!(
                        "Unrecognised trailing input after command: {:?}",
                        remaining
                    );
                }

                let mut cmd: Box<dyn Command> = match parsed {
                    commands::parser::ParsedCommand::Line(args) => {
                        let mut line_cmd = Box::new(LineCommand::new());
                        if let Some(start) = args.start {
                            let _ = line_cmd.on_input(
                                CommandInput::Point(start),
                                &mut self.world,
                            );
                        }
                        if let Some(end) = args.end {
                            let _ = line_cmd.on_input(
                                CommandInput::Point(end),
                                &mut self.world,
                            );
                        }
                        line_cmd
                    }
                    commands::parser::ParsedCommand::Circle(_) => {
                        self.command_state.last_error = Some(
                            "CIRCLE command not yet implemented (v0.2.0+)".to_string(),
                        );
                        return;
                    }
                    commands::parser::ParsedCommand::Arc(_) => {
                        self.command_state.last_error = Some(
                            "ARC command not yet implemented (v0.2.0+)".to_string(),
                        );
                        return;
                    }
                    commands::parser::ParsedCommand::Polyline(_) => {
                        self.command_state.last_error = Some(
                            "PLINE command not yet implemented (v0.2.0+)".to_string(),
                        );
                        return;
                    }
                };

                // If the command already has enough input (e.g. inline args
                // satisfied all steps), finalise immediately by sending
                // a Confirm event.
                if cmd.steps_remaining() == 0 {
                    let result = cmd.on_input(CommandInput::Confirm, &mut self.world);
                    match result {
                        CommandResult::Complete => {
                            self.command_state.last_error = None;
                        }
                        CommandResult::Cancelled => {
                            self.command_state.last_error = None;
                        }
                        CommandResult::Error(msg) => {
                            tracing::warn!("Command error: {}", msg);
                            self.command_state.last_error = Some(msg);
                        }
                        _ => {
                            // Unexpected — set as active if not yet done.
                            self.command_state.active = Some(cmd);
                        }
                    }
                } else {
                    self.command_state.active = Some(cmd);
                }
            }
            Err(nom::Err::Error(e)) | Err(nom::Err::Failure(e)) => {
                tracing::warn!("Unrecognised command: {} ({:?})", text.trim(), e);
                self.command_state.last_error = Some(format!(
                    "Unrecognised command: {}",
                    text.trim()
                ));
            }
            Err(nom::Err::Incomplete(_)) => {
                // Unreachable with complete parsers; kept for forward compat
                // if switching to streaming parsers in v0.2.0+.
            }
        }
    }

    /// Attempt to recover from a GPU device loss by recreating the entire
    /// render state.
    ///
    /// In v0.1.0 this is a basic re-initialisation.  Production recovery
    /// with progress reporting and fallback strategies is deferred to
    /// v0.2.0+.
    #[allow(dead_code)]
    pub async fn recover_device(&mut self, window: Arc<Window>) {
        tracing::warn!("Attempting GPU device recovery...");
        match RenderState::new(window).await {
            Ok(new_state) => {
                // SAFETY: We replace the entire render state atomically.
                // Old GPU resources are dropped (GPU-side cleanup handled
                // by wgpu when the old Device is dropped).
                self.render_state = new_state;
                tracing::info!("GPU device recovery successful");
            }
            Err(e) => {
                tracing::error!("GPU device recovery failed: {:?}", e);
            }
        }
    }
}
