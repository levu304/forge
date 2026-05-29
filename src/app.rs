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
    self, copy_cmd::CopyCommand, erase_cmd::EraseCommand, line_cmd::LineCommand,
    mirror_cmd::MirrorCommand, move_cmd::MoveCommand, offset_cmd::OffsetCommand,
    rotate_cmd::RotateCommand, scale_cmd::ScaleCommand,
    Command, CommandInput, CommandResult, CommandState, PendingModifyCommand,
};
use crate::ecs::resources::{CameraState, GridConfig, SnapConfig};
use crate::geometry::Point2D;
use crate::history::{History, Transaction};
use crate::layer::LayerTable;
use crate::input::InputMapper;
use crate::render::RenderState;
use crate::selection::{window_select::WindowSelectState, SelectionManager};
use crate::snap::SnapEngine;
use crate::spatial::SpatialIndex;
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
///
/// # v0.2.0 additions
///
/// * [`selection_manager`] — tracks the current entity selection set.
/// * [`snap_engine`]       — provides 7 snap types for precision input.
/// * [`spatial_index`]     — rstar R‑tree spatial index for snap/window queries.
/// * [`history`]           — undo/redo command journal.
/// * [`needs_picking`]     — flag for next-frame GPU picking readback (Step 15).
/// * [`window_select_state`] — active window selection drag (if any).
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
    /// Selection manager (selected entity set, primary entity, mode).
    pub selection_manager: SelectionManager,
    /// Snap engine (7 snap types, config, last result).
    pub snap_engine: SnapEngine,
    /// Spatial index (rstar R‑tree for snap + window select queries).
    pub spatial_index: SpatialIndex,
    /// Undo/redo command journal.
    pub history: History,
    /// Layer table (layer properties for visual resolution).
    pub layer_table: LayerTable,
    /// True when a GPU picking request is pending (consumed in render loop).
    pub needs_picking: bool,
    /// Active window selection drag state (None when not dragging).
    pub window_select_state: Option<WindowSelectState>,
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

        let selection_manager = SelectionManager::new();
        let snap_engine = SnapEngine::new(SnapConfig::default());
        let spatial_index = SpatialIndex::new();
        let history = History::new();
        let layer_table = LayerTable::new();
        let needs_picking = false;
        let window_select_state = None;

        Self {
            world,
            resources,
            command_state: CommandState::default(),
            render_state,
            ui_system,
            input_mapper,
            selection_manager,
            snap_engine,
            spatial_index,
            history,
            layer_table,
            needs_picking,
            window_select_state,
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
        // ── 0. Resolve previous frame's picking result (if any) ──────────
        // The picking pass renders entity IDs to an offscreen buffer during
        // this frame's render (see §4b below). The result is read back on the
        // *next* frame via `resolve_pick`. We always try here; `resolve_pick`
        // is a no-op if no pick request is pending.
        //
        // IMPORTANT: `resolve_pick` returns `None` for two different states:
        //   (a) no pick request was pending → `had_pending` is false → skip
        //   (b) pick resolved to empty space (sentinel 0xFFFFFFFF) →
        //       `had_pending` is true → call handle_picking_result(None)
        //       to deselect all.  (Fixes PR #32 review issue #2.)
        if let Some(ref mut picking_pass) = self.render_state.picking_pass {
            let had_pending = picking_pass.pending_coords().is_some();
            if let Some(entity) = picking_pass.resolve_pick(&self.render_state.device) {
                self.selection_manager
                    .handle_picking_result(&mut self.world, Some(entity));
            } else if had_pending {
                // Pick resolved but no entity under cursor → clear selection.
                self.selection_manager
                    .handle_picking_result(&mut self.world, None);
            }
        }

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

        // ── 4a. Window select rect (on top of grid, below entities) ──────
        if let Some(ref ws) = self.window_select_state {
            self.render_state.window_select_renderer.render(
                &mut encoder,
                &view,
                ws,
                &self.render_state.camera_bind_group,
                &self.render_state.queue,
            );
        }

        // ── 4b. Picking pass (draw entity IDs — only when requested) ─────
        if self.needs_picking {
            if let Some(ref mut picking_pass) = self.render_state.picking_pass {
                picking_pass.render(
                    &mut encoder,
                    &self.world,
                    &self.render_state.camera_bind_group,
                    &self.render_state.queue,
                    &self.render_state.device,
                    self.resources.camera.zoom,
                );
            }
            self.needs_picking = false;
        }

        // ── 4c. Entity pass (LoadOp::Load — grid already cleared) ─────────
        self.render_state.entity_renderer.render(
            &mut encoder,
            &view,
            self.resources.camera.zoom,
            &self.world,
            &self.render_state.camera_bind_group,
            &self.render_state.queue,
            &self.render_state.device,
        );

        // ── 4d. Snap marker (on top of entities, below UI) ─────────────────
        if let Some(ref snap_result) = self.snap_engine.last_result {
            if let Some(ref marker_renderer) = self.render_state.snap_marker_renderer {
                marker_renderer.render(
                    &mut encoder,
                    &view,
                    snap_result,
                    &self.render_state.camera_bind_group,
                    &self.resources.camera,
                    &self.render_state.queue,
                );
            }
        }

        // ── 5. UI pass (egui overlay) ────────────────────────────────────
        let ui_output = self.ui_system.run(
            window,
            &self.resources.camera,
            &self.input_mapper.state,
            &mut self.command_state,
            &mut self.world,
            &self.snap_engine,
            &self.selection_manager,
            &self.layer_table,
            &mut self.history,
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

    /// Process a terminal command result and update state accordingly.
    ///
    /// Called after extracting the optional [`Transaction`] from the command
    /// via [`Command::take_transaction`].  The caller must have already
    /// extracted the transaction before calling this method.
    ///
    /// | Result      | Clears active? | Pushes to history? |
    /// |-------------|----------------|--------------------|
    /// | `Complete`  | Yes            | Yes (if tx exists) |
    /// | `Cancelled` | Yes            | No                 |
    /// | `Error`     | No             | No                 |
    /// | `Continue`  | No             | No                 |
    pub fn handle_command_result(&mut self, result: CommandResult, transaction: Option<Transaction>) {
        match result {
            CommandResult::Complete => {
                if let Some(tx) = transaction {
                    self.history.push(tx);
                }
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
                // active is NOT cleared — command may continue after error
                // (e.g. "Invalid input. Specify a point.")
            }
            CommandResult::Continue => {
                // Non-terminal — active stays, no state change needed.
            }
        }
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
        if self.command_state.active.is_some() {
            let trimmed = text.trim();

            let input = if trimmed.is_empty() {
                // Empty dispatch from the command line while a command is
                // active → treat as Confirm (e.g. pressing Enter after
                // placing enough points via mouse clicks).
                Some(CommandInput::Confirm)
            } else if let Ok((_, point)) = commands::parser::parse_point(trimmed) {
                Some(CommandInput::Point(point))
            } else {
                Some(CommandInput::Text(trimmed.to_string()))
            };

            if let Some(input) = input {
                // Extract result + transaction inside a scope so the
                // mutable borrow of command_state.active ends before
                // handle_command_result borrows self again.
                let outcome;
                if let Some(ref mut cmd) = self.command_state.active {
                    let result = cmd.on_input(input, &mut self.world);
                    outcome = Some((result, cmd.take_transaction()));
                } else {
                    outcome = None;
                }
                if let Some((result, tx)) = outcome {
                    self.handle_command_result(result, tx);
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
                            if let Some(tx) = cmd.take_transaction() {
                                self.history.push(tx);
                            }
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

    /// Dispatch a toolbar modify command, bypassing the text parser.
    ///
    /// Consumed by the event loop from [`CommandState::pending_modify_command`].
    /// Constructs the concrete command using `SelectionManager`, checks for
    /// empty selection, and activates it if valid.
    pub fn dispatch_modify_command(&mut self, cmd_type: PendingModifyCommand) {
        /// Helper: if the selection is empty, set an error and return `Err`.
        fn require_selection(sel: &SelectionManager, name: &str) -> Result<(), String> {
            if sel.is_empty() {
                Err(format!(
                    "No entities selected. Select objects before running {name}."
                ))
            } else {
                Ok(())
            }
        }

        let cmd: Box<dyn Command> = match cmd_type {
            PendingModifyCommand::Erase => {
                if let Err(msg) = require_selection(&self.selection_manager, "ERASE") {
                    self.command_state.last_error = Some(msg);
                    return;
                }
                Box::new(EraseCommand::new(&self.selection_manager))
            }
            PendingModifyCommand::Move => {
                if let Err(msg) = require_selection(&self.selection_manager, "MOVE") {
                    self.command_state.last_error = Some(msg);
                    return;
                }
                Box::new(MoveCommand::new(&self.selection_manager))
            }
            PendingModifyCommand::Copy => {
                if let Err(msg) = require_selection(&self.selection_manager, "COPY") {
                    self.command_state.last_error = Some(msg);
                    return;
                }
                Box::new(CopyCommand::new(&self.selection_manager))
            }
            PendingModifyCommand::Rotate => {
                if let Err(msg) = require_selection(&self.selection_manager, "ROTATE") {
                    self.command_state.last_error = Some(msg);
                    return;
                }
                Box::new(RotateCommand::new(&self.selection_manager))
            }
            PendingModifyCommand::Scale => {
                if let Err(msg) = require_selection(&self.selection_manager, "SCALE") {
                    self.command_state.last_error = Some(msg);
                    return;
                }
                Box::new(ScaleCommand::new(&self.selection_manager))
            }
            PendingModifyCommand::Mirror => {
                if let Err(msg) = require_selection(&self.selection_manager, "MIRROR") {
                    self.command_state.last_error = Some(msg);
                    return;
                }
                Box::new(MirrorCommand::new(&self.selection_manager))
            }
            PendingModifyCommand::Offset => {
                if let Err(msg) = require_selection(&self.selection_manager, "OFFSET") {
                    self.command_state.last_error = Some(msg);
                    return;
                }
                Box::new(OffsetCommand::new(&self.selection_manager))
            }
        };

        self.command_state.active = Some(cmd);
        self.command_state.last_error = None;
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
