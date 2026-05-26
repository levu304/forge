//! Window-select rectangle renderer.
//!
//! Renders a translucent filled quad with a border for window selection.
//! Blue for enclosing (left → right drag), green for crossing (right → left).
//!
//! # Pipelines
//!
//! Two pipelines share the same shader (`shaders/window_select.wgsl`):
//!
//! | Pass   | Topology      | Alpha | Vertex count |
//! |--------|---------------|-------|-------------|
//! | Fill   | TriangleList  | 0.15  | 6 (2 tris)  |
//! | Border | LineStrip     | 0.80  | 5 (closed)  |
//!
//! # Render-pass contract
//!
//! `render()` uses `LoadOp::Load` — it draws on top of the grid, below
//! entities and selection highlights.  The caller (ForgeApp) should issue
//! this pass between the grid pass and the entity pass.

use bytemuck;

use crate::selection::window_select::{WindowSelectMode, WindowSelectState};

// ─── Vertex type ──────────────────────────────────────────────────────────────

/// A single window-select quad vertex: world-space position + RGBA colour.
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct WindowSelectVertex {
    position: [f32; 2],
    color: [f32; 4],
}

// ─── Colour helpers ───────────────────────────────────────────────────────────

/// Return `(fill_colour, border_colour)` for the given selection mode.
fn colours_for_mode(mode: WindowSelectMode) -> ([f32; 4], [f32; 4]) {
    match mode {
        // Enclosing (left → right): blue
        WindowSelectMode::Enclosing => (
            [0.0, 0.471, 1.0, 0.15], // fill
            [0.0, 0.471, 1.0, 0.8],  // border
        ),
        // Crossing (right → left): green
        WindowSelectMode::Crossing => (
            [0.0, 1.0, 0.471, 0.15], // fill
            [0.0, 1.0, 0.471, 0.8],  // border
        ),
    }
}

// ─── WindowSelectRenderer ─────────────────────────────────────────────────────

/// Renders a translucent window-select rectangle.
pub struct WindowSelectRenderer {
    /// Render pipeline for the filled quad (TriangleList, alpha 0.15).
    fill_pipeline: wgpu::RenderPipeline,
    /// Render pipeline for the border (LineStrip, alpha 0.8).
    border_pipeline: wgpu::RenderPipeline,
}

impl WindowSelectRenderer {
    /// Create a new `WindowSelectRenderer`.
    ///
    /// # Arguments
    ///
    /// * `device` — The wgpu device used for resource creation.
    /// * `camera_bind_group_layout` — The shared camera uniform bind-group
    ///   layout (group 0, binding 0).
    /// * `surface_format` — The swap-chain texture format.
    pub fn new(
        device: &wgpu::Device,
        camera_bind_group_layout: &wgpu::BindGroupLayout,
        surface_format: wgpu::TextureFormat,
    ) -> Self {
        // ── Shared vertex buffer layout ──────────────────────────────────
        // position: vec2<f32> at location 0 (offset 0,  8 bytes)
        // color:    vec4<f32> at location 1 (offset 8, 16 bytes)
        // stride: 24 bytes
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

        // ── Shader ───────────────────────────────────────────────────────
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Window Select Shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("shaders/window_select.wgsl").into(),
            ),
        });

        // ── Single pipeline layout (shares camera bind group) ────────────
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Window Select Pipeline Layout"),
            bind_group_layouts: &[Some(camera_bind_group_layout)],
            immediate_size: 0,
        });

        // ── Helper: create a pipeline given a topology ───────────────────
        let make_pipeline =
            |device: &wgpu::Device,
             label: &str,
             topology: wgpu::PrimitiveTopology|
             -> wgpu::RenderPipeline {
                device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some(label),
                    layout: Some(&pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &shader,
                        entry_point: Some("vs_main"),
                        buffers: &[vertex_buffer_layout.clone()],
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                    },
                    primitive: wgpu::PrimitiveState {
                        topology,
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
                })
            };

        let fill_pipeline = make_pipeline(
            device,
            "Window Select Fill Pipeline (TriangleList)",
            wgpu::PrimitiveTopology::TriangleList,
        );
        let border_pipeline = make_pipeline(
            device,
            "Window Select Border Pipeline (LineStrip)",
            wgpu::PrimitiveTopology::LineStrip,
        );

        Self {
            fill_pipeline,
            border_pipeline,
        }
    }

    /// Render the window-select rectangle.
    ///
    /// If `window_select.start == window_select.current` (zero-area rect),
    /// this method is a no-op.
    ///
    /// # Parameters
    ///
    /// * `encoder` — Active command encoder for this frame.
    /// * `view` — Colour attachment texture view.
    /// * `window_select` — The current window-select drag state.
    /// * `camera_bind_group` — Bind group for the shared camera uniform.
    /// * `queue` — Command queue (used for `write_buffer` on the temporary
    ///   vertex buffer).
    ///
    /// # Vertex buffer strategy
    ///
    /// We upload a small vertex buffer (6 fill + 5 border = 11 vertices)
    /// every frame via `queue.write_buffer` using a small persistent staging
    /// buffer.  This is negligible overhead for a rectangle drawn only
    /// during mouse-drag.
    pub fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        window_select: &WindowSelectState,
        camera_bind_group: &wgpu::BindGroup,
        queue: &wgpu::Queue,
        device: &wgpu::Device,
    ) {
        // ── 1. Skip zero-area rects ──────────────────────────────────────
        if window_select.start == window_select.current {
            return;
        }

        // ── 2. Compute the normalised rectangle corners ─────────────────
        let min_x = window_select.start.x.min(window_select.current.x) as f32;
        let max_x = window_select.start.x.max(window_select.current.x) as f32;
        let min_y = window_select.start.y.min(window_select.current.y) as f32;
        let max_y = window_select.start.y.max(window_select.current.y) as f32;

        // Corner vertices (world-space, CCW from bottom-left):
        //   bl = (min_x, min_y)
        //   br = (max_x, min_y)
        //   tr = (max_x, max_y)
        //   tl = (min_x, max_y)
        let bl = [min_x, min_y];
        let br = [max_x, min_y];
        let tr = [max_x, max_y];
        let tl = [min_x, max_y];

        // ── 3. Choose colours ────────────────────────────────────────────
        let (fill_col, border_col) = colours_for_mode(window_select.mode);

        // ── 4. Build vertex data ─────────────────────────────────────────
        // Fill: 2 triangles (TriangleList, 6 verts)
        //   Tri 1: bl → br → tl  (CCW)
        //   Tri 2: br → tr → tl
        let fill_verts: [WindowSelectVertex; 6] = [
            WindowSelectVertex { position: bl, color: fill_col },
            WindowSelectVertex { position: br, color: fill_col },
            WindowSelectVertex { position: tl, color: fill_col },
            WindowSelectVertex { position: br, color: fill_col },
            WindowSelectVertex { position: tr, color: fill_col },
            WindowSelectVertex { position: tl, color: fill_col },
        ];

        // Border: LineStrip (5 verts, closed = bl → br → tr → tl → bl)
        let border_verts: [WindowSelectVertex; 5] = [
            WindowSelectVertex { position: bl, color: border_col },
            WindowSelectVertex { position: br, color: border_col },
            WindowSelectVertex { position: tr, color: border_col },
            WindowSelectVertex { position: tl, color: border_col },
            WindowSelectVertex { position: bl, color: border_col },
        ];

        // ── 5. Upload to a staging buffer via queue.write_buffer ─────────
        // 11 vertices × 24 bytes = 264 bytes — small, uploaded every frame.
        let fill_bytes: &[u8] = bytemuck::cast_slice(&fill_verts);
        let border_bytes: &[u8] = bytemuck::cast_slice(&border_verts);

        let fill_size = fill_bytes.len() as u64;
        let border_size = border_bytes.len() as u64;
        let total_size = fill_size + border_size;

        let staging = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Window Select Staging Buffer"),
            size: total_size,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&staging, 0, fill_bytes);
        queue.write_buffer(&staging, fill_size, border_bytes);

        // ── 6. Begin render pass (LoadOp::Load — draw on top of grid) ───
        let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Window Select Render Pass"),
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

        // ── 7. Draw fill (first 6 vertices) ──────────────────────────────
        rp.set_pipeline(&self.fill_pipeline);
        rp.set_vertex_buffer(0, staging.slice(..fill_size));
        rp.set_bind_group(0, camera_bind_group, &[]);
        rp.draw(0..6, 0..1);

        // ── 8. Draw border (next 5 vertices) ─────────────────────────────
        rp.set_pipeline(&self.border_pipeline);
        rp.set_vertex_buffer(0, staging.slice(fill_size..));
        rp.draw(0..5, 0..1);
    }
}
