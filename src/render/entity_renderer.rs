//! Entity geometry renderer.
//!
//! Renders Line, Circle, Arc, and Polyline entities from the ECS.
//! Batches all entities of the same type into a single draw call.
//!
//! # Render-pass contract
//!
//! `render()` **does not** clear the color attachment — it uses `LoadOp::Load`
//! because the grid renderer (which runs first) already cleared the framebuffer.
//!
//! # Batching strategy
//!
//! Each entity type produces vertices into its own per-type staging buffer.
//! A single render pass then issues one draw call per non-empty type.
//! This minimises state changes while keeping pipelines simple.

use std::f64::consts::PI;

use super::EntityRenderer;
use crate::ecs::components::{
    ArcData, CircleData, LineData, PolylineData, Renderable,
};

// ─── Constants ───────────────────────────────────────────────────────────────

/// Minimum number of segments used to approximate a full circle.
/// Tiny circles (<1 screen pixel radius) use this minimum to avoid
/// wasteful over-tessellation.
const CIRCLE_MIN_SEGMENTS: u32 = 8;

/// Maximum number of segments used to approximate a full circle.
/// Prevents buffer explosion for huge circles at high zoom.
const CIRCLE_MAX_SEGMENTS: u32 = 256;

/// Minimum number of segments for an arc (partial circle).
const MIN_ARC_SEGMENTS: u32 = 4;

// ─── Helper: compute circle/arc segment count ────────────────────────────────

/// Compute the number of tessellation segments for a circle of the given
/// radius at the given zoom level.
///
/// The segment count scales with the effective screen-space radius
/// (`radius × zoom`) so that small circles use fewer segments (saving GPU
/// bandwidth) and large circles use more (preserving visual quality).
///
/// Clamped to [`CIRCLE_MIN_SEGMENTS`, `CIRCLE_MAX_SEGMENTS`].
///
/// # Examples
///
/// - `radius * zoom = 0.5`   → 0.5 → clamped to 8
/// - `radius * zoom = 50.0`  → 50
/// - `radius * zoom = 500.0` → 256 (capped)
#[inline]
fn circle_segments_for_radius(radius: f64, zoom: f64) -> u32 {
    let effective_radius = radius * zoom;
    let segments = effective_radius.round().max(0.0) as u32;
    segments.clamp(CIRCLE_MIN_SEGMENTS, CIRCLE_MAX_SEGMENTS)
}

// ─── Vertex ──────────────────────────────────────────────────────────────────

/// A single entity vertex: 2 × f32 position + 4 × f32 color = 24 bytes.
///
/// Layout matches `GridVertex` in `grid.rs` so shaders are interchangeable.
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct EntityVertex {
    position: [f32; 2],
    color: [f32; 4],
}

// ─── Helper: color array ────────────────────────────────────────────────────

/// Convert a `Color` value to an `[f32; 4]` array for vertex data.
#[inline]
fn color_to_array(c: &crate::util::Color) -> [f32; 4] {
    [c.r, c.g, c.b, c.a]
}

// ─── Helper: generate circle vertices ────────────────────────────────────────

/// Generate vertices for a circle approximation as a line strip.
///
/// Segment count is computed from the effective screen-space radius
/// (`radius × zoom`) so small circles use fewer vertices and large circles
/// stay smooth.  Produces `num_segments + 1` vertices so the strip forms a
/// closed loop (the last vertex equals the first).  Pushes into `out` to
/// avoid per-entity Vec allocations.
fn generate_circle_vertices(circle: &CircleData, zoom: f64, out: &mut Vec<EntityVertex>) {
    let col = color_to_array(&circle.color);
    let cx = circle.center.x;
    let cy = circle.center.y;
    let r = circle.radius;
    let num_segments = circle_segments_for_radius(r, zoom);
    let step = 2.0 * PI / num_segments as f64;

    out.reserve(num_segments as usize + 1);
    for i in 0..=num_segments {
        let theta = step * i as f64;
        out.push(EntityVertex {
            position: [(cx + r * theta.cos()) as f32, (cy + r * theta.sin()) as f32],
            color: col,
        });
    }
}

// ─── Helper: generate arc vertices ──────────────────────────────────────────

/// Generate vertices for an arc approximation as a line strip.
///
/// Segment count is proportional to the sweep angle and the effective
/// screen-space radius (`radius × zoom`), between `MIN_ARC_SEGMENTS` and
/// `CIRCLE_MAX_SEGMENTS`.  The last vertex is the end of the arc (not
/// wrapped to start).  Pushes into `out` to avoid per-entity Vec allocations.
fn generate_arc_vertices(arc: &ArcData, zoom: f64, out: &mut Vec<EntityVertex>) {
    let col = color_to_array(&arc.color);
    let cx = arc.center.x;
    let cy = arc.center.y;
    let r = arc.radius;

    // Angles are stored in degrees; convert to radians.
    let start_rad = arc.start_angle * PI / 180.0;
    let end_rad = arc.end_angle * PI / 180.0;
    let sweep_rad = end_rad - start_rad;

    // Full-circle segments at this radius/zoom, then scale by sweep.
    let full_segments = circle_segments_for_radius(r, zoom);
    let sweep_fraction = sweep_rad.abs() / (2.0 * PI);
    let num_segments = ((full_segments as f64) * sweep_fraction)
        .round()
        .max(MIN_ARC_SEGMENTS as f64) as u32;

    let step = sweep_rad / num_segments as f64;

    out.reserve(num_segments as usize + 1);
    for i in 0..=num_segments {
        let theta = start_rad + step * i as f64;
        out.push(EntityVertex {
            position: [(cx + r * theta.cos()) as f32, (cy + r * theta.sin()) as f32],
            color: col,
        });
    }
}

// ─── EntityRenderer implementation ──────────────────────────────────────────

impl EntityRenderer {
    /// Create a new `EntityRenderer`.
    ///
    /// Loads a single WGSL shader (`shaders/entity.wgsl`) and creates a
    /// dedicated render pipeline per entity type (the shared shader is valid
    /// for both LineList and LineStrip topologies).  All pipelines share a
    /// single `PipelineLayout` built from `camera_bind_group_layout`.
    ///
    /// Initial staging buffers are 32 KiB each, doubling on overflow.
    ///
    /// # Arguments
    ///
    /// * `camera_bind_group_layout` — Bind-group layout for the camera
    ///   uniform at group(0), binding(0).  Shared with all other renderers.
    /// * `surface_format` — The swap-chain texture format.
    pub fn new(
        device: &wgpu::Device,
        camera_bind_group_layout: &wgpu::BindGroupLayout,
        surface_format: wgpu::TextureFormat,
    ) -> Self {
        // ── Shared vertex buffer layout ───────────────────────────────────
        // position: vec2<f32> at location 0 (offset 0,  8 bytes)
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

        // ── Helper: create a pipeline given a shader source and topology ──
        let make_pipeline = |device: &wgpu::Device,
                             label: &str,
                             topology: wgpu::PrimitiveTopology,
                             layout: &wgpu::PipelineLayout|
         -> wgpu::RenderPipeline {
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(label),
                source: wgpu::ShaderSource::Wgsl(
                    include_str!("shaders/entity.wgsl").into(),
                ),
            });

            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(layout),
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

        // ── Single pipeline layout (shared by all 4 pipelines) ───────────
        let entity_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Entity Pipeline Layout"),
            bind_group_layouts: &[Some(camera_bind_group_layout)],
            immediate_size: 0,
        });

        // ── Pipelines ────────────────────────────────────────────────────
        let line_pipeline = make_pipeline(
            device,
            "Line Entity Render Pipeline",
            wgpu::PrimitiveTopology::LineList,
            &entity_layout,
        );
        let circle_pipeline = make_pipeline(
            device,
            "Circle Entity Render Pipeline",
            wgpu::PrimitiveTopology::LineStrip,
            &entity_layout,
        );
        let arc_pipeline = make_pipeline(
            device,
            "Arc Entity Render Pipeline",
            wgpu::PrimitiveTopology::LineStrip,
            &entity_layout,
        );
        let polyline_pipeline = make_pipeline(
            device,
            "Polyline Entity Render Pipeline",
            wgpu::PrimitiveTopology::LineList,
            &entity_layout,
        );

        // ── Staging buffers (32 KiB each) ────────────────────────────────
        let staging_size = EntityRenderer::INITIAL_STAGING_SIZE;
        let create_staging =
            |device: &wgpu::Device, label: &str| -> wgpu::Buffer {
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some(label),
                    size: staging_size,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                })
            };

        Self {
            line_pipeline,
            circle_pipeline,
            arc_pipeline,
            polyline_pipeline,
            line_staging: create_staging(device, "Line Entity Staging Buffer"),
            circle_staging: create_staging(device, "Circle Entity Staging Buffer"),
            arc_staging: create_staging(device, "Arc Entity Staging Buffer"),
            polyline_staging: create_staging(device, "Polyline Entity Staging Buffer"),
            line_staging_capacity: staging_size,
            circle_staging_capacity: staging_size,
            arc_staging_capacity: staging_size,
            polyline_staging_capacity: staging_size,
            line_scratch: Vec::new(),
            circle_scratch: Vec::new(),
            arc_scratch: Vec::new(),
            polyline_scratch: Vec::new(),
        }
    }

    /// Render all entities from the ECS world.
    ///
    /// For each entity type (Line, Circle, Arc, Polyline) with a `Renderable`
    /// marker, vertex data is generated into per-type scratch buffers (reused
    /// each frame to avoid allocation), uploaded to the respective staging
    /// buffer, and drawn in a single render pass.  Staging buffers are
    /// automatically doubled if they overflow.
    ///
    /// # Parameters
    ///
    /// * `encoder` — Active command encoder for this frame.
    /// * `view` — Colour attachment texture view.
    /// * `zoom` — Current camera zoom factor (used for radius-dependent
    ///   tessellation — larger zoom → more circle/arc segments).
    /// * `world` — The ECS world to query for entities.
    /// * `camera_bind_group` — Bind group holding the camera uniform buffer.
    /// * `queue` — Command queue (used for `write_buffer` on the staging buffers).
    /// * `device` — GPU device (used to re-create a staging buffer if it
    ///   needs to grow).
    pub fn render(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        zoom: f64,
        world: &hecs::World,
        camera_bind_group: &wgpu::BindGroup,
        queue: &wgpu::Queue,
        device: &wgpu::Device,
    ) {
        // ── 1. Clear per-type scratch buffers ────────────────────────────
        // These are reused every frame — clearing sets len = 0 without
        // freeing the backing allocation, so the buffer grows only to its
        // watermark and stops allocating.

        self.line_scratch.clear();
        self.circle_scratch.clear();
        self.arc_scratch.clear();
        self.polyline_scratch.clear();

        // ── 2. Collect vertices into scratch buffers ─────────────────────

        Self::collect_line_vertices(world, &mut self.line_scratch);
        Self::collect_circle_vertices(world, zoom, &mut self.circle_scratch);
        Self::collect_arc_vertices(world, zoom, &mut self.arc_scratch);
        Self::collect_polyline_vertices(world, &mut self.polyline_scratch);

        // ── 3. Upload to staging buffers (resize if needed) ──────────────

        Self::upload_vertices::<EntityVertex>(
            queue,
            device,
            &mut self.line_staging,
            &mut self.line_staging_capacity,
            &self.line_scratch,
            "Line Entity Staging Buffer",
        );
        Self::upload_vertices::<EntityVertex>(
            queue,
            device,
            &mut self.circle_staging,
            &mut self.circle_staging_capacity,
            &self.circle_scratch,
            "Circle Entity Staging Buffer",
        );
        Self::upload_vertices::<EntityVertex>(
            queue,
            device,
            &mut self.arc_staging,
            &mut self.arc_staging_capacity,
            &self.arc_scratch,
            "Arc Entity Staging Buffer",
        );
        Self::upload_vertices::<EntityVertex>(
            queue,
            device,
            &mut self.polyline_staging,
            &mut self.polyline_staging_capacity,
            &self.polyline_scratch,
            "Polyline Entity Staging Buffer",
        );

        // ── 4. Single render pass (LoadOp::Load — grid already cleared) ──
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Entity Render Pass"),
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

        // ── 5. Draw each non-empty type ──────────────────────────────────
        if !self.line_scratch.is_empty() {
            render_pass.set_pipeline(&self.line_pipeline);
            render_pass.set_bind_group(0, camera_bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.line_staging.slice(..));
            render_pass.draw(0..self.line_scratch.len() as u32, 0..1);
        }

        if !self.circle_scratch.is_empty() {
            render_pass.set_pipeline(&self.circle_pipeline);
            render_pass.set_bind_group(0, camera_bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.circle_staging.slice(..));
            render_pass.draw(0..self.circle_scratch.len() as u32, 0..1);
        }

        if !self.arc_scratch.is_empty() {
            render_pass.set_pipeline(&self.arc_pipeline);
            render_pass.set_bind_group(0, camera_bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.arc_staging.slice(..));
            render_pass.draw(0..self.arc_scratch.len() as u32, 0..1);
        }

        if !self.polyline_scratch.is_empty() {
            render_pass.set_pipeline(&self.polyline_pipeline);
            render_pass.set_bind_group(0, camera_bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.polyline_staging.slice(..));
            render_pass.draw(0..self.polyline_scratch.len() as u32, 0..1);
        }
    }

    // ── Private helpers ───────────────────────────────────────────────────

    /// Collect vertices for all `LineData + Renderable` entities.
    ///
    /// Appends 2 vertices (start, end) per entity to `out`.
    fn collect_line_vertices(world: &hecs::World, out: &mut Vec<EntityVertex>) {
        let mut query = world.query::<(&LineData, &Renderable)>();
        for (_, (line, _)) in query.iter() {
            let col = color_to_array(&line.color);
            out.push(EntityVertex {
                position: line.start.to_f32_array(),
                color: col,
            });
            out.push(EntityVertex {
                position: line.end.to_f32_array(),
                color: col,
            });
        }
    }

    /// Collect vertices for all `CircleData + Renderable` entities.
    ///
    /// Calls `generate_circle_vertices` for each circle and appends to `out`.
    /// The `zoom` parameter controls tessellation density (more zoom → more
    /// segments for large circles).
    fn collect_circle_vertices(world: &hecs::World, zoom: f64, out: &mut Vec<EntityVertex>) {
        let mut query = world.query::<(&CircleData, &Renderable)>();
        for (_, (circle, _)) in query.iter() {
            generate_circle_vertices(circle, zoom, out);
        }
    }

    /// Collect vertices for all `ArcData + Renderable` entities.
    ///
    /// Calls `generate_arc_vertices` for each arc and appends to `out`.
    /// The `zoom` parameter controls tessellation density.
    fn collect_arc_vertices(world: &hecs::World, zoom: f64, out: &mut Vec<EntityVertex>) {
        let mut query = world.query::<(&ArcData, &Renderable)>();
        for (_, (arc, _)) in query.iter() {
            generate_arc_vertices(arc, zoom, out);
        }
    }

    /// Collect vertices for all `PolylineData + Renderable` entities.
    ///
    /// Emits one vertex per point for each polyline; if `closed` and has
    /// at least 2 vertices, emits the first vertex again to close the loop.
    ///
    /// Entities with an empty vertex list are silently skipped to prevent
    /// degenerate buffer ranges.
    fn collect_polyline_vertices(world: &hecs::World, out: &mut Vec<EntityVertex>) {
        let mut query = world.query::<(&PolylineData, &Renderable)>();
        for (_, (poly, _)) in query.iter() {
            if poly.vertices.is_empty() {
                continue;
            }
            let col = color_to_array(&poly.color);
            // Emit one vertex per point.
            for point in &poly.vertices {
                out.push(EntityVertex {
                    position: point.to_f32_array(),
                    color: col,
                });
            }
            // If closed, emit the first vertex again to close the loop.
            if poly.closed && poly.vertices.len() > 1 {
                out.push(EntityVertex {
                    position: poly.vertices[0].to_f32_array(),
                    color: col,
                });
            }
        }
    }

    /// Upload vertex data to a staging buffer, growing it if necessary.
    ///
    /// The buffer is doubled (or larger) when the current capacity is
    /// insufficient.
    ///
    /// This is an associated function (not a method) to avoid borrow-checker
    /// conflicts when called with multiple `&mut self` fields in the same
    /// scope.
    fn upload_vertices<V: bytemuck::Pod>(
        queue: &wgpu::Queue,
        device: &wgpu::Device,
        buffer: &mut wgpu::Buffer,
        capacity: &mut u64,
        vertices: &[V],
        label: &str,
    ) {
        if vertices.is_empty() {
            return;
        }

        let needed_bytes = (vertices.len() * std::mem::size_of::<V>()) as u64;

        if needed_bytes > *capacity {
            // Resize strategy:
            //   - If capacity * 2 overflows u64 → jump to needed_bytes directly.
            //   - If needed_bytes is within 2× of current capacity → double.
            //   - If needed_bytes is huge (e.g. a single giant polyline) →
            //     jump directly to needed_bytes (no wasteful over-doubling).
            //   - Never drop below INITIAL_STAGING_SIZE.
            let new_size = (*capacity)
                .checked_mul(2)
                .map(|s| s.max(needed_bytes))
                .unwrap_or(needed_bytes)
                .max(EntityRenderer::INITIAL_STAGING_SIZE);

            *buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: new_size,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            *capacity = new_size;
        }

        let bytes: &[u8] = bytemuck::cast_slice(vertices);
        queue.write_buffer(buffer, 0, bytes);
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::components::{ArcData, CircleData, PolylineData};
    use crate::util::Color;

    // ── circle_segments_for_radius ─────────────────────────────────────────

    #[test]
    fn test_segments_tiny_radius_uses_minimum() {
        // radius * zoom = 0.5 → clamped to CIRCLE_MIN_SEGMENTS (8)
        let segs = circle_segments_for_radius(0.5, 1.0);
        assert_eq!(segs, CIRCLE_MIN_SEGMENTS);
    }

    #[test]
    fn test_segments_large_radius_capped_at_max() {
        // radius * zoom = 500.0 → clamped to CIRCLE_MAX_SEGMENTS (256)
        let segs = circle_segments_for_radius(500.0, 1.0);
        assert_eq!(segs, CIRCLE_MAX_SEGMENTS);
    }

    #[test]
    fn test_segments_scales_with_zoom() {
        // radius 50 at zoom 1.0 → 50 segments
        let segs_1x = circle_segments_for_radius(50.0, 1.0);
        assert_eq!(segs_1x, 50);

        // radius 50 at zoom 2.0 → 100 segments
        let segs_2x = circle_segments_for_radius(50.0, 2.0);
        assert_eq!(segs_2x, 100);

        // radius 50 at zoom 0.1 → 5 → clamped to 8
        let segs_half = circle_segments_for_radius(50.0, 0.1);
        assert_eq!(segs_half, CIRCLE_MIN_SEGMENTS);
    }

    #[test]
    fn test_segments_zero_radius_uses_minimum() {
        let segs = circle_segments_for_radius(0.0, 1.0);
        assert_eq!(segs, CIRCLE_MIN_SEGMENTS);
    }

    // ── generate_circle_vertices ──────────────────────────────────────────

    #[test]
    fn test_circle_vertex_count_varies_with_zoom() {
        let circle = CircleData {
            center: crate::geometry::Point2D::new(0.0, 0.0),
            radius: 100.0,
            color: Color::WHITE,
            width: 1.0,
        };

        // At zoom 1.0: radius*zoom = 100 → 100 segments → 101 vertices
        let mut buf = Vec::new();
        generate_circle_vertices(&circle, 1.0, &mut buf);
        // num_segments + 1 vertices (closed loop)
        assert_eq!(buf.len(), 101);

        // At zoom 0.05: radius*zoom = 5 → clamped to 8 → 9 vertices
        buf.clear();
        generate_circle_vertices(&circle, 0.05, &mut buf);
        assert_eq!(buf.len(), 9);
    }

    #[test]
    fn test_circle_vertices_form_closed_loop() {
        let circle = CircleData {
            center: crate::geometry::Point2D::new(10.0, 20.0),
            radius: 50.0,
            color: Color::WHITE,
            width: 1.0,
        };

        let mut buf = Vec::new();
        generate_circle_vertices(&circle, 1.0, &mut buf);

        // First and last vertex should be identical (closed loop)
        assert!(!buf.is_empty());
        assert_eq!(buf.first().unwrap().position, buf.last().unwrap().position);
    }

    #[test]
    fn test_circle_vertices_conserves_color() {
        let circle = CircleData {
            center: crate::geometry::Point2D::new(0.0, 0.0),
            radius: 10.0,
            color: Color::from_hex(0xFF0000), // red
            width: 1.0,
        };

        let mut buf = Vec::new();
        generate_circle_vertices(&circle, 1.0, &mut buf);

        let expected_col = [1.0, 0.0, 0.0, 1.0];
        for v in &buf {
            assert_eq!(v.color, expected_col);
        }
    }

    // ── generate_arc_vertices ─────────────────────────────────────────────

    #[test]
    fn test_arc_vertex_count_varies_with_zoom_and_sweep() {
        let arc = ArcData {
            center: crate::geometry::Point2D::new(0.0, 0.0),
            radius: 100.0,
            start_angle: 0.0,
            end_angle: 90.0, // quarter circle
            color: Color::WHITE,
            width: 1.0,
        };

        // quarter circle at zoom 1.0: full=100, sweep_fraction=0.25 → 25
        let mut buf = Vec::new();
        generate_arc_vertices(&arc, 1.0, &mut buf);
        assert_eq!(buf.len(), 26); // 25 + 1

        // quarter circle at zoom 0.05: full=8 (min), sweep_fraction=0.25
        // 8 * 0.25 = 2 → clamped to MIN_ARC_SEGMENTS (4) → 5 vertices
        buf.clear();
        generate_arc_vertices(&arc, 0.05, &mut buf);
        assert_eq!(buf.len(), 5);
    }

    #[test]
    fn test_arc_vertices_conserves_color() {
        let arc = ArcData {
            center: crate::geometry::Point2D::new(0.0, 0.0),
            radius: 50.0,
            start_angle: 0.0,
            end_angle: 180.0,
            color: Color::from_hex(0x00FF00), // green
            width: 1.0,
        };

        let mut buf = Vec::new();
        generate_arc_vertices(&arc, 1.0, &mut buf);

        let expected_col = [0.0, 1.0, 0.0, 1.0];
        for v in &buf {
            assert_eq!(v.color, expected_col);
        }
    }

    // ── collect_polyline_vertices (empty guard) ───────────────────────────

    #[test]
    fn test_empty_polyline_skipped() {
        let mut world = hecs::World::new();

        // Spawn a polyline with no vertices — should be silently skipped.
        world.spawn((
            PolylineData {
                vertices: vec![],
                closed: false,
                color: Color::WHITE,
                width: 1.0,
            },
            Renderable,
        ));

        // Also spawn a non-empty polyline to prove the collection loop runs.
        world.spawn((
            PolylineData {
                vertices: vec![
                    crate::geometry::Point2D::new(0.0, 0.0),
                    crate::geometry::Point2D::new(100.0, 0.0),
                ],
                closed: false,
                color: Color::WHITE,
                width: 1.0,
            },
            Renderable,
        ));

        let mut buf = Vec::new();
        EntityRenderer::collect_polyline_vertices(&world, &mut buf);

        // Only the non-empty polyline should produce vertices (2 points).
        assert_eq!(buf.len(), 2);
    }

    #[test]
    fn test_empty_polyline_does_not_crash() {
        let mut world = hecs::World::new();

        // Only an empty polyline — should produce no vertices.
        world.spawn((
            PolylineData {
                vertices: vec![],
                closed: false,
                color: Color::WHITE,
                width: 1.0,
            },
            Renderable,
        ));

        let mut buf = Vec::new();
        EntityRenderer::collect_polyline_vertices(&world, &mut buf);
        assert!(buf.is_empty());
    }

    // ── collect_line_vertices ─────────────────────────────────────────────

    #[test]
    fn test_line_vertices_two_per_entity() {
        let mut world = hecs::World::new();

        world.spawn((
            crate::ecs::components::LineData {
                start: crate::geometry::Point2D::new(0.0, 0.0),
                end: crate::geometry::Point2D::new(100.0, 100.0),
                color: Color::WHITE,
                width: 1.0,
            },
            Renderable,
        ));
        world.spawn((
            crate::ecs::components::LineData {
                start: crate::geometry::Point2D::new(10.0, 20.0),
                end: crate::geometry::Point2D::new(30.0, 40.0),
                color: Color::WHITE,
                width: 1.0,
            },
            Renderable,
        ));

        let mut buf = Vec::new();
        EntityRenderer::collect_line_vertices(&world, &mut buf);

        // 2 lines × 2 vertices each = 4
        assert_eq!(buf.len(), 4);
    }
}
