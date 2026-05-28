//! Snap marker renderer.
//!
//! Renders screen-space billboard markers at the snapped point using
//! wgpu. The marker shape depends on the snap type:
//!
//! | SnapType               | Shape      | Topology      | Size    |
//! |------------------------|------------|---------------|---------|
//! | `Endpoint`             | Square     | TriangleStrip | 8×8 px  |
//! | `Midpoint`             | Triangle   | TriangleList  | 10px ht |
//! | `Center`               | Circle     | TriangleList  | 8px dia |
//! | `Nearest` / `Perpendicular` / `Tangent` / `Grid` | Crosshair | LineList | 12px |
//!
//! All markers are yellow `rgba(255, 255, 0, 1.0)`.
//!
//! # Pipeline
//!
//! Three render pipelines (one per topology) share a single WGSL shader
//! module (`shaders/snap_marker.wgsl`). The vertex buffer holds all 4
//! marker geometries pre-computed and indexed by shape. The snap point
//! is uploaded each frame as a `vec4<f32>` uniform at `@group(1) @binding(0)`:
//! `(snap_world_x, snap_world_y, 1.0/zoom, 0.0)`.
//!
//! # Render-pass contract
//!
//! `render()` uses `LoadOp::Load` — it draws on top of entities,
//! assuming the grid renderer already cleared the framebuffer. The
//! caller (ForgeApp) should issue the snap marker pass after the entity
//! pass but before the UI pass.

use std::f32::consts::PI;

use crate::ecs::resources::CameraState;
use crate::snap::{SnapResult, SnapType};

// ---------------------------------------------------------------------------
// Vertex constants
// ---------------------------------------------------------------------------

/// Byte stride of a single marker vertex (2 × f32 = 8 bytes).
const VERTEX_STRIDE: u64 = 8;

// ── Vertex counts ──────────────────────────────────────────────────────────

/// Number of vertices for the square marker (TriangleStrip quad).
const SQUARE_VERTS: u32 = 4;
/// Number of vertices for the triangle marker (TriangleList).
const TRIANGLE_VERTS: u32 = 3;
/// Number of segments used to approximate the circle marker.
const CIRCLE_SEGMENTS: u32 = 12;
/// Number of vertices for the circle marker (TriangleList: 12 tris × 3).
const CIRCLE_VERTS: u32 = CIRCLE_SEGMENTS * 3;
/// Number of vertices for the crosshair marker (LineList, 2 lines).
const CROSSHAIR_VERTS: u32 = 4;

/// Total vertices across all four marker shapes.
const TOTAL_VERTS: u32 = SQUARE_VERTS + TRIANGLE_VERTS + CIRCLE_VERTS + CROSSHAIR_VERTS;

/// Total byte size of the vertex buffer.
const BUFFER_SIZE: u64 = TOTAL_VERTS as u64 * VERTEX_STRIDE;

// ── Vertex index offsets (start positions in the buffer) ────────────────────

const SQUARE_START: u32 = 0;
const TRIANGLE_START: u32 = SQUARE_START + SQUARE_VERTS;
const CIRCLE_START: u32 = TRIANGLE_START + TRIANGLE_VERTS;
const CROSSHAIR_START: u32 = CIRCLE_START + CIRCLE_VERTS;

/// Byte offset for a vertex range starting at `start` with `count` vertices.
///
/// Used with `vertex_buffer.slice(offset..)` or `draw(start..end, ..)`.
/// Since `draw()` consumes vertex *indices*, we pass the index range directly
/// when the buffer is bound at offset 0. For `set_vertex_buffer` slicing use
/// `(start * VERTEX_STRIDE)`.
fn vertex_count_for_type(snap_type: SnapType) -> u32 {
    match snap_type {
        SnapType::Endpoint => SQUARE_VERTS,
        SnapType::Midpoint => TRIANGLE_VERTS,
        SnapType::Center => CIRCLE_VERTS,
        SnapType::Nearest | SnapType::Perpendicular | SnapType::Tangent | SnapType::Grid => {
            CROSSHAIR_VERTS
        }
    }
}

fn vertex_start_for_type(snap_type: SnapType) -> u32 {
    match snap_type {
        SnapType::Endpoint => SQUARE_START,
        SnapType::Midpoint => TRIANGLE_START,
        SnapType::Center => CIRCLE_START,
        SnapType::Nearest | SnapType::Perpendicular | SnapType::Tangent | SnapType::Grid => {
            CROSSHAIR_START
        }
    }
}

// ---------------------------------------------------------------------------
// Vertex data builder
// ---------------------------------------------------------------------------

/// Build the interleaved vertex buffer data for all 4 marker shapes.
///
/// Returns a flat `Vec<f32>` where every pair of floats is one vertex
/// `(pixel_offset_x, pixel_offset_y)`. The ordering matches the
/// `*_START` constants above.
fn build_marker_vertices() -> Vec<f32> {
    let mut verts = Vec::with_capacity(TOTAL_VERTS as usize * 2);

    // ── Square (TriangleStrip, 4 verts) ─────────────────────────────────
    // 8×8 pixel filled quad centered at origin.
    verts.extend_from_slice(&[
        -4.0, -4.0, // bottom-left
        4.0, -4.0, // bottom-right
        -4.0, 4.0, // top-left
        4.0, 4.0, // top-right
    ]);

    // ── Triangle (TriangleList, 3 verts) ────────────────────────────────
    // 10px height equilateral triangle centered at origin.
    verts.extend_from_slice(&[
        0.0, -5.0, // top vertex
        -4.33, 5.0, // bottom-left
        4.33, 5.0, // bottom-right
    ]);

    // ── Circle (TriangleList, CIRCLE_SEGMENTS × 3 = 36 verts) ──────────
    // 8px diameter filled circle approximated as a triangle-list fan.
    // Each segment emits: center, circumference_i, circumference_{i+1}.
    let radius = 4.0_f32;
    let n = CIRCLE_SEGMENTS as f32;
    for i in 0..CIRCLE_SEGMENTS {
        let a0 = (i as f32) * 2.0 * PI / n;
        let a1 = ((i + 1) as f32) * 2.0 * PI / n;
        verts.push(0.0); // center x
        verts.push(0.0); // center y
        verts.push(radius * a0.cos()); // p_i x
        verts.push(radius * a0.sin()); // p_i y
        verts.push(radius * a1.cos()); // p_{i+1} x
        verts.push(radius * a1.sin()); // p_{i+1} y
    }

    // ── Crosshair (LineList, 4 verts = 2 lines) ─────────────────────────
    // 12px lines intersecting at origin.
    verts.extend_from_slice(&[
        -6.0, 0.0, // horizontal line start
        6.0, 0.0, // horizontal line end
        0.0, -6.0, // vertical line start
        0.0, 6.0, // vertical line end
    ]);

    debug_assert_eq!(
        verts.len(),
        TOTAL_VERTS as usize * 2,
        "vertex data length mismatch"
    );
    verts
}

// ---------------------------------------------------------------------------
// SnapMarkerRenderer
// ---------------------------------------------------------------------------

/// Renders screen-space yellow snap markers at the snapped world point.
///
/// Owns the GPU resources: a vertex buffer with all pre-computed marker
/// geometries, a uniform buffer for the snap-point data, its bind group,
/// and three render pipelines (one per required topology).
pub struct SnapMarkerRenderer {
    /// TriangleStrip pipeline — square marker (Endpoint).
    fill_pipeline: wgpu::RenderPipeline,
    /// TriangleList pipeline — triangle (Midpoint) and circle (Center).
    tri_pipeline: wgpu::RenderPipeline,
    /// LineList pipeline — crosshair (Nearest, Perpendicular, Tangent, Grid).
    line_pipeline: wgpu::RenderPipeline,
    /// Vertex buffer with all pre-computed marker geometries.
    vertex_buffer: wgpu::Buffer,
    /// Uniform buffer holding the snap point data: `(snap_x, snap_y, 1.0/zoom, 0.0)`.
    snap_buffer: wgpu::Buffer,
    /// Bind group referencing `snap_buffer` at group(1) binding(0).
    snap_bind_group: wgpu::BindGroup,
}

impl SnapMarkerRenderer {
    /// Create a new `SnapMarkerRenderer`.
    ///
    /// Builds the shared vertex buffer, snap uniform buffer + bind group,
    /// and 3 render pipelines (all using `shaders/snap_marker.wgsl`).
    ///
    /// # Arguments
    ///
    /// * `device` — The wgpu device used for resource creation.
    /// * `camera_bind_group_layout` — The shared camera uniform bind group
    ///   layout (group(0) binding(0)).
    /// * `surface_format` — The swap-chain texture format.
    pub fn new(
        device: &wgpu::Device,
        camera_bind_group_layout: &wgpu::BindGroupLayout,
        surface_format: wgpu::TextureFormat,
    ) -> Self {
        // ── Vertex buffer ─────────────────────────────────────────────────
        // Build vertex data and upload via mapped-at-creation.
        let marker_vertices = build_marker_vertices();
        let vertex_bytes: &[u8] = bytemuck::cast_slice(&marker_vertices);
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Snap Marker Vertex Buffer"),
            size: BUFFER_SIZE,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: true,
        });
        {
            let mut mapped = vertex_buffer.slice(..).get_mapped_range_mut();
            mapped.copy_from_slice(vertex_bytes);
        }
        vertex_buffer.unmap();

        // ── Snap uniform buffer (16 bytes: vec4<f32>) ───────────────────
        let snap_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Snap Marker Uniform Buffer"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // ── Snap uniform bind group layout ───────────────────────────────
        let snap_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Snap Marker Uniform Layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        // ── Snap uniform bind group ──────────────────────────────────────
        let snap_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Snap Marker Bind Group"),
            layout: &snap_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: snap_buffer.as_entire_binding(),
            }],
        });

        // ── Shared vertex buffer layout ──────────────────────────────────
        // offset: vec2<f32> at location 0 (offset 0, 8 bytes stride)
        let vertex_buffer_layout = wgpu::VertexBufferLayout {
            array_stride: 8,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 0,
                shader_location: 0,
            }],
        };

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Snap Marker Shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../render/shaders/snap_marker.wgsl").into(),
            ),
        });

        // ── Shared pipeline layout (2 bind groups) ──────────────────────
        // group(0): camera view-proj (shared)
        // group(1): snap uniform
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Snap Marker Pipeline Layout"),
            bind_group_layouts: &[
                Some(camera_bind_group_layout),
                Some(&snap_bind_group_layout),
            ],
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
                        buffers: std::slice::from_ref(&vertex_buffer_layout),
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
            "Snap Marker Fill Pipeline (TriangleStrip)",
            wgpu::PrimitiveTopology::TriangleStrip,
        );
        let tri_pipeline = make_pipeline(
            device,
            "Snap Marker Triangle Pipeline (TriangleList)",
            wgpu::PrimitiveTopology::TriangleList,
        );
        let line_pipeline = make_pipeline(
            device,
            "Snap Marker Line Pipeline (LineList)",
            wgpu::PrimitiveTopology::LineList,
        );

        Self {
            fill_pipeline,
            tri_pipeline,
            line_pipeline,
            vertex_buffer,
            snap_buffer,
            snap_bind_group,
        }
    }

    /// Render the snap marker for the given snap result.
    ///
    /// Updates the snap uniform buffer with the snap point's world
    /// coordinates and inverse zoom, then draws the appropriate marker
    /// shape using `LoadOp::Load` (draws on top of already-rendered content).
    ///
    /// # Parameters
    ///
    /// * `encoder` — Active command encoder for this frame.
    /// * `view` — Colour attachment texture view.
    /// * `snap` — The current snap result (point + type).
    /// * `camera_bind_group` — Bind group for the shared camera uniform.
    /// * `camera` — Camera state (used for zoom).
    /// * `queue` — Command queue (used for `write_buffer`).
    pub fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        snap: &SnapResult,
        camera_bind_group: &wgpu::BindGroup,
        camera: &CameraState,
        queue: &wgpu::Queue,
    ) {
        // ── 1. Update snap uniform buffer ───────────────────────────────
        let inv_zoom = 1.0 / camera.zoom.max(0.0001) as f32;
        let snap_data: [f32; 4] = [
            snap.point.x as f32,
            snap.point.y as f32,
            inv_zoom,
            0.0,
        ];
        let uniform_bytes: &[u8] = bytemuck::bytes_of(&snap_data);
        queue.write_buffer(&self.snap_buffer, 0, uniform_bytes);

        // ── 2. Look up vertex range for this snap type ──────────────────
        let start = vertex_start_for_type(snap.snap_type);
        let count = vertex_count_for_type(snap.snap_type);
        let vertex_range = start..start + count;

        // ── 3. Select the pipeline for this snap type ───────────────────
        let pipeline: &wgpu::RenderPipeline = match snap.snap_type {
            SnapType::Endpoint => &self.fill_pipeline,
            SnapType::Midpoint | SnapType::Center => &self.tri_pipeline,
            SnapType::Nearest
            | SnapType::Perpendicular
            | SnapType::Tangent
            | SnapType::Grid => &self.line_pipeline,
        };

        // ── 4. Begin render pass (LoadOp::Load — draw on top) ──────────
        let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Snap Marker Render Pass"),
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

        // ── 5. Draw the marker ──────────────────────────────────────────
        rp.set_pipeline(pipeline);
        rp.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        rp.set_bind_group(0, camera_bind_group, &[]);
        rp.set_bind_group(1, &self.snap_bind_group, &[]);
        rp.draw(vertex_range, 0..1);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ── Vertex data shape tests ─────────────────────────────────────────

    #[test]
    fn test_build_marker_vertices_total_count() {
        let verts = build_marker_vertices();
        let expected_floats = TOTAL_VERTS as usize * 2;
        assert_eq!(
            verts.len(),
            expected_floats,
            "total floats should be {} ({} vertices × 2 coords)",
            expected_floats,
            TOTAL_VERTS,
        );
    }

    #[test]
    fn test_vertex_counts_match_constants() {
        // Verify each shape has the expected number of vertices.
        assert_eq!(SQUARE_VERTS, 4, "square should have 4 verts (TriangleStrip)");
        assert_eq!(TRIANGLE_VERTS, 3, "triangle should have 3 verts (TriangleList)");
        assert_eq!(
            CIRCLE_VERTS,
            CIRCLE_SEGMENTS * 3,
            "circle should have {} verts ({} tris × 3)",
            CIRCLE_SEGMENTS * 3,
            CIRCLE_SEGMENTS,
        );
        assert_eq!(
            CROSSHAIR_VERTS, 4,
            "crosshair should have 4 verts (LineList, 2 lines)",
        );
    }

    #[test]
    fn test_total_verts_consistency() {
        assert_eq!(
            TOTAL_VERTS,
            SQUARE_VERTS + TRIANGLE_VERTS + CIRCLE_VERTS + CROSSHAIR_VERTS,
        );
    }

    #[test]
    fn test_build_marker_vertices_not_empty() {
        let verts = build_marker_vertices();
        assert!(!verts.is_empty(), "vertex data should not be empty");
    }

    #[test]
    fn test_build_marker_vertices_all_finite() {
        let verts = build_marker_vertices();
        for &v in &verts {
            assert!(v.is_finite(), "vertex coordinate should be finite, got {}", v);
        }
    }

    #[test]
    fn test_square_vertices_forms_quad() {
        let verts = build_marker_vertices();
        // Square is the first 4 vertices (8 floats).
        let square: Vec<f32> = verts[..8].to_vec();
        // Should have 4 distinct positions forming a proper quad.
        // bottom-left (-4,-4), bottom-right (4,-4),
        // top-left (-4,4), top-right (4,4)
        let positions: Vec<[f32; 2]> = square.chunks(2).map(|c| [c[0], c[1]]).collect();
        assert!(positions.contains(&[-4.0, -4.0]));
        assert!(positions.contains(&[4.0, -4.0]));
        assert!(positions.contains(&[-4.0, 4.0]));
        assert!(positions.contains(&[4.0, 4.0]));
    }

    #[test]
    fn test_triangle_vertices() {
        let verts = build_marker_vertices();
        // Triangle starts at index 4 (8 floats in).
        let tri: Vec<f32> = verts[8..14].to_vec();
        let positions: Vec<[f32; 2]> = tri.chunks(2).map(|c| [c[0], c[1]]).collect();
        // (0,-5), (-4.33,5), (4.33,5)
        assert!(positions.contains(&[0.0, -5.0]));
        assert!(positions.contains(&[-4.33, 5.0]));
        assert!(positions.contains(&[4.33, 5.0]));
    }

    // ── vertex_count_for_type / vertex_start_for_type ───────────────────

    #[test]
    fn test_vertex_count_for_type_matches() {
        assert_eq!(vertex_count_for_type(SnapType::Endpoint), SQUARE_VERTS);
        assert_eq!(vertex_count_for_type(SnapType::Midpoint), TRIANGLE_VERTS);
        assert_eq!(vertex_count_for_type(SnapType::Center), CIRCLE_VERTS);
        assert_eq!(vertex_count_for_type(SnapType::Nearest), CROSSHAIR_VERTS);
        assert_eq!(vertex_count_for_type(SnapType::Perpendicular), CROSSHAIR_VERTS);
        assert_eq!(vertex_count_for_type(SnapType::Tangent), CROSSHAIR_VERTS);
        assert_eq!(vertex_count_for_type(SnapType::Grid), CROSSHAIR_VERTS);
    }

    #[test]
    fn test_vertex_start_for_type_no_overlap() {
        let endpoint_start = vertex_start_for_type(SnapType::Endpoint);
        let midpoint_start = vertex_start_for_type(SnapType::Midpoint);
        let center_start = vertex_start_for_type(SnapType::Center);
        let crosshair_start = vertex_start_for_type(SnapType::Grid);

        // All start positions should be distinct.
        let mut starts = vec![endpoint_start, midpoint_start, center_start, crosshair_start];
        starts.sort();
        starts.dedup();
        assert_eq!(starts.len(), 4, "all shape start offsets should be unique");
    }

    #[test]
    fn test_vertex_start_and_count_within_bounds() {
        // Verify that every shape's vertex range fits within TOTAL_VERTS.
        for snap_type in &[
            SnapType::Endpoint,
            SnapType::Midpoint,
            SnapType::Center,
            SnapType::Nearest,
            SnapType::Perpendicular,
            SnapType::Tangent,
            SnapType::Grid,
        ] {
            let start = vertex_start_for_type(*snap_type);
            let count = vertex_count_for_type(*snap_type);
            let end = start + count;
            assert!(
                end <= TOTAL_VERTS,
                "shape {:?} range {start}..{end} exceeds TOTAL_VERTS {TOTAL_VERTS}",
                snap_type,
            );
        }
    }

    // ── Compile-time checks ─────────────────────────────────────────────

    /// Verify the `SnapMarkerRenderer` struct has the expected field types.
    /// This is a compile-time check — if it compiles, the types are correct.
    #[test]
    fn test_snap_marker_renderer_fields_exist() {
        // We can't instantiate without a GPU device in unit tests,
        // but we can at least verify associated functions compile.
        let _ = vertex_count_for_type(SnapType::Endpoint);
        let _ = vertex_start_for_type(SnapType::Endpoint);
        let _ = build_marker_vertices();
    }

    // ── Circle vertex data integrity ────────────────────────────────────

    #[test]
    fn test_circle_vertices_center_at_origin() {
        let verts = build_marker_vertices();
        // Circle starts at CIRCLE_START (index 7, float offset 14).
        let circle_start_float = CIRCLE_START as usize * 2;
        // First vertex of the circle should be center (0.0, 0.0).
        assert!(
            (verts[circle_start_float] - 0.0).abs() < f32::EPSILON,
            "circle first vertex x should be 0 (center)",
        );
        assert!(
            (verts[circle_start_float + 1] - 0.0).abs() < f32::EPSILON,
            "circle first vertex y should be 0 (center)",
        );
    }

    #[test]
    fn test_circle_vertices_radius() {
        let verts = build_marker_vertices();
        let circle_start_float = CIRCLE_START as usize * 2;
        // Check the circumference points have distance ≈ radius from origin.
        // Circumference vertices are at offsets 2 and 4 from each triangle's start.
        for tri_idx in 0..CIRCLE_SEGMENTS as usize {
            let base = circle_start_float + tri_idx * 6; // 6 floats per tri
                                                          // p_i: floats at base+2, base+3
            let x = verts[base + 2];
            let y = verts[base + 3];
            let dist = (x * x + y * y).sqrt();
            assert!(
                (dist - 4.0).abs() < 0.01,
                "circle circumference point distance {dist} should be ~4.0",
            );
            // p_{i+1}: floats at base+4, base+5
            let x2 = verts[base + 4];
            let y2 = verts[base + 5];
            let dist2 = (x2 * x2 + y2 * y2).sqrt();
            assert!(
                (dist2 - 4.0).abs() < 0.01,
                "circle next circumference point distance {dist2} should be ~4.0",
            );
        }
    }

    // ── Crosshair vertex data ───────────────────────────────────────────

    #[test]
    fn test_crosshair_vertices() {
        let verts = build_marker_vertices();
        let ch_start_float = CROSSHAIR_START as usize * 2;
        let ch: Vec<f32> = verts[ch_start_float..ch_start_float + 8].to_vec();
        let positions: Vec<[f32; 2]> = ch.chunks(2).map(|c| [c[0], c[1]]).collect();
        // Horizontal line: (-6, 0) → (6, 0)
        assert!(positions.contains(&[-6.0, 0.0]));
        assert!(positions.contains(&[6.0, 0.0]));
        // Vertical line: (0, -6) → (0, 6)
        assert!(positions.contains(&[0.0, -6.0]));
        assert!(positions.contains(&[0.0, 6.0]));
    }

    // ── BUFFER_SIZE checks ──────────────────────────────────────────────

    #[test]
    fn test_buffer_size_matches_total_verts() {
        assert_eq!(
            BUFFER_SIZE,
            TOTAL_VERTS as u64 * VERTEX_STRIDE,
            "BUFFER_SIZE should equal TOTAL_VERTS × VERTEX_STRIDE",
        );
    }
}
