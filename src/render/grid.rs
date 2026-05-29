//! Grid renderer.
//!
//! Vertex-buffered line list for major/minor grid lines and axis markers.
//! Regenerated when camera zoom/position changes significantly (≥10% shift).
//!
//! # Pipeline
//!
//! The grid is drawn **after** the unconditional clear pass (see
//! [`crate::app::ForgeApp::render`]) and uses `LoadOp::Load` because the
//! framebuffer is already cleared.  Subsequent passes (entities, UI) also
//! use `LoadOp::Load`.

use crate::ecs::resources::{CameraState, GridConfig};
use crate::geometry::Point2D;

// ─── Constants ───────────────────────────────────────────────────────────────

/// Maximum number of grid vertices to generate.
/// Safety cap to prevent OOM at extreme zoom-out levels.
const MAX_GRID_VERTICES: u32 = 100_000;

/// Initial vertex buffer size in bytes.
/// 32 KiB ≈ 1365 vertices at 24 bytes each.
/// Grows by doubling if grid generation overflows.
const INITIAL_BUFFER_SIZE: u64 = 32768;

// ─── Vertex ──────────────────────────────────────────────────────────────────

/// A single grid vertex: 2 × f32 position + 4 × f32 color = 24 bytes.
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct GridVertex {
    position: [f32; 2],
    color: [f32; 4],
}

// ─── GridRenderer ────────────────────────────────────────────────────────────

/// Renders the background grid as a vertex-buffered line list.
///
/// Major and minor grid lines are generated within the visible world bounds
/// (plus a 10 % margin) and regenerated when the camera zoom or position
/// changes by ≥10 %.
///
/// ## Render-pass contract
///
/// `render()` begins a new render pass with `LoadOp::Load` because the
/// framebuffer is unconditionally cleared by
/// [`crate::app::ForgeApp::render`] before the grid pass runs.
pub struct GridRenderer {
    /// The render pipeline for grid lines.
    pub pipeline: wgpu::RenderPipeline,
    /// Buffer containing grid line vertices.
    pub vertex_buffer: wgpu::Buffer,
    /// Number of vertices in the vertex buffer.
    pub num_vertices: u32,
    /// Last camera target used for grid generation.
    /// Compared against the current camera to detect ≥10 % shift
    /// before triggering regeneration.
    pub last_target: Point2D,
    /// Last camera zoom used for grid generation.
    pub last_zoom: f64,
}

impl GridRenderer {
    /// Create a new `GridRenderer`.
    ///
    /// Loads the `grid.wgsl` shader, creates a `LineList` render pipeline
    /// with alpha blending, and allocates an empty 32 KiB vertex buffer.
    ///
    /// # Arguments
    ///
    /// * `camera_bind_group_layout` — Bind-group layout for the camera
    ///   uniform at group(0), binding(0).  Shared with all other renderers.
    /// * `surface_format` — The swap-chain texture format (used as the
    ///   fragment shader's color target).
    pub fn new(
        device: &wgpu::Device,
        camera_bind_group_layout: &wgpu::BindGroupLayout,
        surface_format: wgpu::TextureFormat,
    ) -> Self {
        // ── Shader ───────────────────────────────────────────────────────
        let shader_source = include_str!("shaders/grid.wgsl");
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Grid Shader"),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });

        // ── Pipeline layout ──────────────────────────────────────────────
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Grid Pipeline Layout"),
            bind_group_layouts: &[Some(camera_bind_group_layout)],
            immediate_size: 0,
        });

        // ── Vertex buffer layout ─────────────────────────────────────────
        // position: vec2<f32> at location 0 (offset 0, 8 bytes)
        // color:    vec4<f32> at location 1 (offset 8, 16 bytes)
        // stride:   24 bytes
        let vertex_buffer_layout = wgpu::VertexBufferLayout {
            array_stride: 24,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x2,
                    offset: 0,
                    shader_location: 0,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 8,
                    shader_location: 1,
                },
            ],
        };

        // ── Render pipeline ──────────────────────────────────────────────
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Grid Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[vertex_buffer_layout],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            multiview_mask: None,
            cache: None,
        });

        // ── Empty vertex buffer (32 KiB) ─────────────────────────────────
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Grid Vertex Buffer"),
            size: INITIAL_BUFFER_SIZE,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            pipeline,
            vertex_buffer,
            num_vertices: 0,
            last_target: Point2D::default(),
            last_zoom: 0.0,
        }
    }

    /// Force grid regeneration on the next render.
    ///
    /// Resets the tracking fields so the next `render()` call treats the
    /// camera as having changed by ≥10 %.
    pub fn invalidate(&mut self) {
        self.last_zoom = 0.0;
        self.last_target = Point2D::default();
    }

    /// Render the grid into the active command encoder.
    ///
    /// This pass runs **after** the unconditional clear pass in
    /// [`crate::app::ForgeApp::render`], so the color attachment is
    /// already cleared when this draw call begins.
    ///
    /// # Regeneration
    ///
    /// Vertices are regenerated when either:
    ///
    /// * The zoom ratio moves outside [`0.9`, `1.1`] (≥10 % change).
    /// * The camera target moves more than 10 % of the viewport diagonal.
    ///
    /// The vertex buffer is grown by doubling if the generated vertices
    /// exceed its current capacity.
    ///
    /// # Parameters
    ///
    /// * `encoder` — Active command encoder for this frame.
    /// * `view` — Colour attachment texture view (from `Surface::get_current_texture`).
    /// * `camera` — Current camera state.
    /// * `grid` — Grid configuration (spacing, colors, visibility).
    /// * `camera_bind_group` — Bind group holding the camera uniform buffer.
    /// * `queue` — Command queue (used for `write_buffer` on regeneration).
    /// * `device` — GPU device (used to re-create the vertex buffer if it
    ///   needs to grow).
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        camera: &CameraState,
        grid: &GridConfig,
        camera_bind_group: &wgpu::BindGroup,
        queue: &wgpu::Queue,
        device: &wgpu::Device,
    ) {
        // ── Regeneration check ───────────────────────────────────────────
        if self.should_regenerate(camera) {
            let vertices = generate_grid_vertices(camera, grid);
            self.num_vertices = vertices.len() as u32;

            // Update tracking state for the next check.
            self.last_target = camera.target;
            self.last_zoom = camera.zoom;

            if self.num_vertices > 0 {
                // Ensure the vertex buffer is large enough.
                let needed_bytes =
                    (vertices.len() * std::mem::size_of::<GridVertex>()) as u64;
                let current_size = self.vertex_buffer.size();

                if needed_bytes > current_size {
                    let new_size = current_size
                        .checked_mul(2)
                        .map(|s| s.max(needed_bytes))
                        .unwrap_or(needed_bytes)
                        .max(INITIAL_BUFFER_SIZE);

                    self.vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("Grid Vertex Buffer"),
                        size: new_size,
                        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    });
                }

                // Upload vertex data to the GPU buffer.
                let bytes: &[u8] = bytemuck::cast_slice(&vertices);
                queue.write_buffer(&self.vertex_buffer, 0, bytes);
            }
        }

        // ── Draw — skip if there are no vertices ─────────────────────────
        if self.num_vertices == 0 {
            return;
        }

        // ── Render pass (draw only — framebuffer already cleared) ─────────
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Grid Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
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

        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, camera_bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        render_pass.draw(0..self.num_vertices, 0..1);
    }

    /// Decide whether grid vertices must be regenerated.
    ///
    /// Regeneration is triggered when:
    ///
    /// 1. `self.last_zoom == 0.0` (initial / invalidated state).
    /// 2. The zoom ratio `camera.zoom / self.last_zoom` falls outside
    ///    [`0.9`, `1.1`] (≥10 % change).
    /// 3. The camera target has moved more than 10 % of the current
    ///    viewport's world-space diagonal.
    fn should_regenerate(&self, camera: &CameraState) -> bool {
        let view_w = f64::from(camera.viewport_size.0);
        let view_h = f64::from(camera.viewport_size.1);

        if view_w == 0.0 || view_h == 0.0 {
            return false;
        }

        // Always regenerate after `invalidate()` (last_zoom == 0.0).
        let zoom_changed = if self.last_zoom == 0.0 {
            true
        } else {
            let ratio = camera.zoom / self.last_zoom;
            !(0.9..=1.1).contains(&ratio)
        };

        let pos_changed = if self.last_zoom == 0.0 {
            // Zoom change alone will trigger regeneration.
            false
        } else {
            let dx = camera.target.x - self.last_target.x;
            let dy = camera.target.y - self.last_target.y;
            let dist = (dx * dx + dy * dy).sqrt();

            // Viewport diagonal in world units.
            let z = camera.zoom.max(0.0001);
            let world_w = view_w / z;
            let world_h = view_h / z;
            let diag = (world_w * world_w + world_h * world_h).sqrt();

            dist > diag * 0.1
        };

        zoom_changed || pos_changed
    }
}

// ─── Grid vertex generation ─────────────────────────────────────────────────

/// Generate grid vertices for the given camera view and grid configuration.
///
/// Produces a flat `Vec<GridVertex>` encoding a `LineList` primitive with
/// vertex colors for minor lines, major lines, and axis markers.
///
/// # Algorithm
///
/// 1. Compute visible world bounds from the camera.
/// 2. Extend bounds by 10 % on each side (margin).
/// 3. Emit minor grid lines at `minor_spacing` intervals.
/// 4. Emit major grid lines at `major_spacing` intervals.
/// 5. Emit axis lines (X = 0 and Y = 0) if they cross the viewport.
///
/// # Panics
///
/// Does not panic.  If the viewport has zero width or height an empty
/// `Vec` is returned.
fn generate_grid_vertices(camera: &CameraState, grid: &GridConfig) -> Vec<GridVertex> {
    if camera.viewport_size.0 == 0 || camera.viewport_size.1 == 0 {
        return Vec::new();
    }

    // ── Visible world bounds (plus 10 % margin) ─────────────────────────
    let z = camera.zoom.max(0.0001);
    let half_w = f64::from(camera.viewport_size.0) / (2.0 * z);
    let half_h = f64::from(camera.viewport_size.1) / (2.0 * z);

    let left = camera.target.x - half_w;
    let right = camera.target.x + half_w;
    let bottom = camera.target.y - half_h;
    let top = camera.target.y + half_h;

    let ext_left = left - (right - left) * 0.1;
    let ext_right = right + (right - left) * 0.1;
    let ext_bottom = bottom - (top - bottom) * 0.1;
    let ext_top = top + (top - bottom) * 0.1;

    // Pre-compute color arrays for the three line categories.
    let minor_col = [
        grid.minor_color.r,
        grid.minor_color.g,
        grid.minor_color.b,
        grid.minor_color.a,
    ];
    let major_col = [
        grid.major_color.r,
        grid.major_color.g,
        grid.major_color.b,
        grid.major_color.a,
    ];
    let axis_col = [
        grid.axis_color.r,
        grid.axis_color.g,
        grid.axis_color.b,
        grid.axis_color.a,
    ];

    let mut vertices = Vec::new();
    // Track vertex count separately to avoid borrow conflicts with the
    // inline helper macro below.
    let mut vcount: u32 = 0;

    // ── Helper: emit one line segment (two vertices) ─────────────────────
    fn push_line(
        vertices: &mut Vec<GridVertex>,
        vcount: &mut u32,
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        color: [f32; 4],
    ) {
        if *vcount >= MAX_GRID_VERTICES {
            return;
        }
        vertices.push(GridVertex {
            position: [x1 as f32, y1 as f32],
            color,
        });
        vertices.push(GridVertex {
            position: [x2 as f32, y2 as f32],
            color,
        });
        *vcount += 2;
    }

    // ── Minor grid lines ────────────────────────────────────────────────
    if grid.minor_spacing > 0.0 {
        // Vertical lines (varying X).
        let first_x = (ext_left / grid.minor_spacing).ceil() * grid.minor_spacing;
        let mut x = first_x;
        while x <= ext_right && vcount < MAX_GRID_VERTICES {
            push_line(&mut vertices, &mut vcount, x, ext_bottom, x, ext_top, minor_col);
            x += grid.minor_spacing;
        }

        // Horizontal lines (varying Y).
        let first_y = (ext_bottom / grid.minor_spacing).ceil() * grid.minor_spacing;
        let mut y = first_y;
        while y <= ext_top && vcount < MAX_GRID_VERTICES {
            push_line(&mut vertices, &mut vcount, ext_left, y, ext_right, y, minor_col);
            y += grid.minor_spacing;
        }
    }

    // ── Major grid lines ────────────────────────────────────────────────
    if grid.major_spacing > 0.0 {
        // Vertical lines (varying X).
        let first_x = (ext_left / grid.major_spacing).ceil() * grid.major_spacing;
        let mut x = first_x;
        while x <= ext_right && vcount < MAX_GRID_VERTICES {
            push_line(&mut vertices, &mut vcount, x, ext_bottom, x, ext_top, major_col);
            x += grid.major_spacing;
        }

        // Horizontal lines (varying Y).
        let first_y = (ext_bottom / grid.major_spacing).ceil() * grid.major_spacing;
        let mut y = first_y;
        while y <= ext_top && vcount < MAX_GRID_VERTICES {
            push_line(&mut vertices, &mut vcount, ext_left, y, ext_right, y, major_col);
            y += grid.major_spacing;
        }
    }

    // ── Axis lines (X = 0, Y = 0) ───────────────────────────────────────
    // Only draw an axis if it crosses the extended viewport bounds.
    if ext_bottom <= 0.0 && 0.0 <= ext_top && vcount < MAX_GRID_VERTICES {
        push_line(&mut vertices, &mut vcount, ext_left, 0.0, ext_right, 0.0, axis_col);
    }
    if ext_left <= 0.0 && 0.0 <= ext_right && vcount < MAX_GRID_VERTICES {
        push_line(&mut vertices, &mut vcount, 0.0, ext_bottom, 0.0, ext_top, axis_col);
    }

    vertices
}
