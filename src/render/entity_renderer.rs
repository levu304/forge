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

use std::collections::HashSet;
use std::f64::consts::PI;
use std::slice::from_ref;

use super::EntityRenderer;
use crate::ecs::components::{
    ArcData, CircleData, LayerRef, LineData, PolylineData, Renderable, Selected,
};
use crate::layer::table::LayerTable;
use crate::layer::LayerId;

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

/// A single entity vertex: position + color + selection flag + line direction.
///
/// | Field         | Type      | Offset | Size |
/// |---------------|-----------|--------|------|
/// | `position`    | `vec2<f>` | 0      | 8    |
/// | `color`       | `vec4<f>` | 8      | 16   |
/// | `is_selected` | `u32`     | 24     | 4    |
/// | `line_dir`    | `vec2<f>` | 28     | 8    |
/// | **Total**     |           |        | 36   |
///
/// When `is_selected` is `1`, the fragment shader blends a blue tint
/// (`mix(color, selection_blue, 0.3)`) to indicate selection state.
///
/// `line_dir` is a normalised per-vertex tangent direction. For lines and
/// polylines this is the unit vector along the segment; for circles and arcs
/// it is `[0, 0]` (reserved for v0.3.1+ curved-line effects).
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct EntityVertex {
    position: [f32; 2],
    color: [f32; 4],
    is_selected: u32,
    line_dir: [f32; 2],
}

// ─── Helper: color array ────────────────────────────────────────────────────

/// Convert a `Color` value to an `[f32; 4]` array for vertex data.
#[inline]
fn color_to_array(c: &crate::util::Color) -> [f32; 4] {
    [c.r, c.g, c.b, c.a]
}

// ─── Helper: layer culling check ─────────────────────────────────────────────

/// Returns `true` when the entity's layer is hidden or frozen.
///
/// Entities without a `LayerRef` component are always visible (conservative
/// default — ByLayer semantics fall back to the active layer which is visible).
#[inline]
fn is_layer_culled(
    world: &hecs::World,
    entity: hecs::Entity,
    layer_table: &LayerTable,
) -> bool {
    if let Ok(lr) = world.get::<&LayerRef>(entity) {
        layer_table
            .get(LayerId(lr.0))
            .map(|l| !l.visible || l.frozen)
            .unwrap_or(false)
    } else {
        false
    }
}

// ─── Helper: normalise direction vector ──────────────────────────────────────

/// Normalise `(dx, dy)` to a unit `[f32; 2]`.
///
/// Returns `[0.0, 0.0]` when the segment has zero length (degenerate).
#[inline]
fn normalize_dir(dx: f64, dy: f64) -> [f32; 2] {
    let len_sq = dx * dx + dy * dy;
    if len_sq > f64::EPSILON {
        let len = len_sq.sqrt();
        [(dx / len) as f32, (dy / len) as f32]
    } else {
        [0.0, 0.0]
    }
}

// ─── Helper: generate circle vertices ────────────────────────────────────────

/// Generate vertices for a circle approximation as a line strip.
///
/// Segment count is computed from the effective screen-space radius
/// (`radius × zoom`) so small circles use fewer vertices and large circles
/// stay smooth.  Produces `num_segments + 1` vertices so the strip forms a
/// closed loop (the last vertex equals the first).  Pushes into `out` to
/// avoid per-entity Vec allocations.
fn generate_circle_vertices(
    circle: &CircleData,
    zoom: f64,
    is_selected: u32,
    out: &mut Vec<EntityVertex>,
) {
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
            is_selected,
            line_dir: [0.0, 0.0],
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
fn generate_arc_vertices(
    arc: &ArcData,
    zoom: f64,
    is_selected: u32,
    out: &mut Vec<EntityVertex>,
) {
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
            is_selected,
            line_dir: [0.0, 0.0],
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
        // position:    vec2<f32> at location 0 (offset  0,  8 bytes)
        // color:       vec4<f32> at location 1 (offset  8, 16 bytes)
        // is_selected: u32       at location 2 (offset 24,  4 bytes)
        // line_dir:    vec2<f32> at location 3 (offset 28,  8 bytes)
        // stride:    computed from size_of::<EntityVertex>()
        let vertex_buffer_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<EntityVertex>() as u64,
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
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Uint32,
                    offset: 24,
                    shader_location: 2,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x2,
                    offset: 28,
                    shader_location: 3,
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
                    buffers: from_ref(&vertex_buffer_layout),
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
    /// Entities on layers that are hidden (`visible == false`) or frozen
    /// (`frozen == true`) are skipped.  Entities without a `LayerRef`
    /// component are rendered (conservative default).
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
    /// * `layer_table` — Layer table used to check entity visibility before
    ///   generating vertices.
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        zoom: f64,
        world: &hecs::World,
        camera_bind_group: &wgpu::BindGroup,
        queue: &wgpu::Queue,
        device: &wgpu::Device,
        layer_table: &LayerTable,
    ) {
        // ── 1. Build selection set ──────────────────────────────────────
        // Query all entities with the Selected marker component and collect
        // into a HashSet for O(1) membership checks during vertex collection.
        let selected: HashSet<hecs::Entity> = world
            .query::<&Selected>()
            .iter()
            .map(|(e, _)| e)
            .collect();

        // ── 2. Clear per-type scratch buffers ───────────────────────────
        // These are reused every frame — clearing sets len = 0 without
        // freeing the backing allocation, so the buffer grows only to its
        // watermark and stops allocating.

        self.line_scratch.clear();
        self.circle_scratch.clear();
        self.arc_scratch.clear();
        self.polyline_scratch.clear();

        // ── 3. Collect vertices into scratch buffers ────────────────────

        Self::collect_line_vertices(world, &selected, layer_table, &mut self.line_scratch);
        Self::collect_circle_vertices(
            world,
            zoom,
            &selected,
            layer_table,
            &mut self.circle_scratch,
        );
        Self::collect_arc_vertices(world, zoom, &selected, layer_table, &mut self.arc_scratch);
        Self::collect_polyline_vertices(world, &selected, layer_table, &mut self.polyline_scratch);

        // ── 4. Upload to staging buffers (resize if needed) ──────────────

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

        // ── 5. Single render pass (LoadOp::Load — grid already cleared) ──
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

        // ── 6. Draw each non-empty type ──────────────────────────────────
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
    /// Sets `is_selected = 1` when the entity is in the selection set.
    /// Computes `line_dir` as the normalised vector from start to end
    /// (falls back to `[0, 0]` for zero-length lines).
    /// Skips entities whose layer is hidden or frozen.
    fn collect_line_vertices(
        world: &hecs::World,
        selected: &HashSet<hecs::Entity>,
        layer_table: &LayerTable,
        out: &mut Vec<EntityVertex>,
    ) {
        let mut query = world.query::<(&LineData, &Renderable)>();
        for (entity, (line, _)) in query.iter() {
            if is_layer_culled(world, entity, layer_table) {
                continue;
            }

            let col = color_to_array(&line.color);
            let is_sel = u32::from(selected.contains(&entity));

            // line_dir = normalize(end - start), guard against zero length.
            let dx = line.end.x - line.start.x;
            let dy = line.end.y - line.start.y;
            let line_dir = normalize_dir(dx, dy);

            out.push(EntityVertex {
                position: line.start.to_f32_array(),
                color: col,
                is_selected: is_sel,
                line_dir,
            });
            out.push(EntityVertex {
                position: line.end.to_f32_array(),
                color: col,
                is_selected: is_sel,
                line_dir,
            });
        }
    }

    /// Collect vertices for all `CircleData + Renderable` entities.
    ///
    /// Calls `generate_circle_vertices` for each circle and appends to `out`.
    /// The `zoom` parameter controls tessellation density (more zoom → more
    /// segments for large circles). Sets `is_selected = 1` when the entity
    /// is in the selection set.
    /// Skips entities whose layer is hidden or frozen.
    fn collect_circle_vertices(
        world: &hecs::World,
        zoom: f64,
        selected: &HashSet<hecs::Entity>,
        layer_table: &LayerTable,
        out: &mut Vec<EntityVertex>,
    ) {
        let mut query = world.query::<(&CircleData, &Renderable)>();
        for (entity, (circle, _)) in query.iter() {
            if is_layer_culled(world, entity, layer_table) {
                continue;
            }

            let is_sel = u32::from(selected.contains(&entity));
            generate_circle_vertices(circle, zoom, is_sel, out);
        }
    }

    /// Collect vertices for all `ArcData + Renderable` entities.
    ///
    /// Calls `generate_arc_vertices` for each arc and appends to `out`.
    /// The `zoom` parameter controls tessellation density. Sets
    /// `is_selected = 1` when the entity is in the selection set.
    /// Skips entities whose layer is hidden or frozen.
    fn collect_arc_vertices(
        world: &hecs::World,
        zoom: f64,
        selected: &HashSet<hecs::Entity>,
        layer_table: &LayerTable,
        out: &mut Vec<EntityVertex>,
    ) {
        let mut query = world.query::<(&ArcData, &Renderable)>();
        for (entity, (arc, _)) in query.iter() {
            if is_layer_culled(world, entity, layer_table) {
                continue;
            }

            let is_sel = u32::from(selected.contains(&entity));
            generate_arc_vertices(arc, zoom, is_sel, out);
        }
    }

    /// Collect vertices for all `PolylineData + Renderable` entities.
    ///
    /// Emits one vertex per point for each polyline; if `closed` and has
    /// at least 2 vertices, emits the first vertex again to close the loop.
    /// Sets `is_selected = 1` when the entity is in the selection set.
    /// Computes `line_dir` per vertex as the normalised direction toward the
    /// next point; the last vertex in an open polyline reuses the last
    /// segment's direction.  Falls back to `[0, 0]` for degenerate segments.
    /// Skips entities whose layer is hidden or frozen.
    ///
    /// Entities with an empty vertex list are silently skipped to prevent
    /// degenerate buffer ranges.
    fn collect_polyline_vertices(
        world: &hecs::World,
        selected: &HashSet<hecs::Entity>,
        layer_table: &LayerTable,
        out: &mut Vec<EntityVertex>,
    ) {
        let mut query = world.query::<(&PolylineData, &Renderable)>();
        for (entity, (poly, _)) in query.iter() {
            if is_layer_culled(world, entity, layer_table) {
                continue;
            }

            if poly.vertices.is_empty() {
                continue;
            }
            let col = color_to_array(&poly.color);
            let is_sel = u32::from(selected.contains(&entity));

            // ── Per-vertex line_dir ────────────────────────────────────
            // Each vertex gets the direction toward the next point.
            // The last vertex in an open polyline uses the previous segment.
            // For a closed polyline, the last interior vertex points to the
            // first vertex (the closing segment).
            let points = &poly.vertices;
            for i in 0..points.len() {
                let (dx, dy) = if i + 1 < points.len() {
                    // Direction from this point to the next.
                    (points[i + 1].x - points[i].x, points[i + 1].y - points[i].y)
                } else if poly.closed && points.len() > 1 {
                    // Closed polyline: last interior vertex points back
                    // to the first vertex (closing segment).
                    (points[0].x - points[i].x, points[0].y - points[i].y)
                } else if i > 0 {
                    // Open polyline last vertex: reuse previous segment.
                    (points[i].x - points[i - 1].x, points[i].y - points[i - 1].y)
                } else {
                    (0.0, 0.0) // Single vertex
                };
                let line_dir = normalize_dir(dx, dy);
                out.push(EntityVertex {
                    position: points[i].to_f32_array(),
                    color: col,
                    is_selected: is_sel,
                    line_dir,
                });
            }

            // If closed, emit the first vertex again — its line_dir points
            // from the last vertex back to the first.
            if poly.closed && points.len() > 1 {
                let last = points.last().unwrap();
                let first = &points[0];
                let dx = first.x - last.x;
                let dy = first.y - last.y;
                let line_dir = normalize_dir(dx, dy);
                out.push(EntityVertex {
                    position: first.to_f32_array(),
                    color: col,
                    is_selected: is_sel,
                    line_dir,
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

        let needed_bytes = std::mem::size_of_val(vertices) as u64;

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
    use crate::geometry::Point2D;
    use crate::layer::table::LayerTable;
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
        generate_circle_vertices(&circle, 1.0, 0, &mut buf);
        // num_segments + 1 vertices (closed loop)
        assert_eq!(buf.len(), 101);

        // At zoom 0.05: radius*zoom = 5 → clamped to 8 → 9 vertices
        buf.clear();
        generate_circle_vertices(&circle, 0.05, 0, &mut buf);
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
        generate_circle_vertices(&circle, 1.0, 0, &mut buf);

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
        generate_circle_vertices(&circle, 1.0, 0, &mut buf);

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
        generate_arc_vertices(&arc, 1.0, 0, &mut buf);
        assert_eq!(buf.len(), 26); // 25 + 1

        // quarter circle at zoom 0.05: full=8 (min), sweep_fraction=0.25
        // 8 * 0.25 = 2 → clamped to MIN_ARC_SEGMENTS (4) → 5 vertices
        buf.clear();
        generate_arc_vertices(&arc, 0.05, 0, &mut buf);
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
        generate_arc_vertices(&arc, 1.0, 0, &mut buf);

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

        let layer_table = LayerTable::new();
        let selected = HashSet::new();
        let mut buf = Vec::new();
        EntityRenderer::collect_polyline_vertices(&world, &selected, &layer_table, &mut buf);

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

        let layer_table = LayerTable::new();
        let selected = HashSet::new();
        let mut buf = Vec::new();
        EntityRenderer::collect_polyline_vertices(&world, &selected, &layer_table, &mut buf);
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

        let layer_table = LayerTable::new();
        let selected = HashSet::new();
        let mut buf = Vec::new();
        EntityRenderer::collect_line_vertices(&world, &selected, &layer_table, &mut buf);

        // 2 lines × 2 vertices each = 4
        assert_eq!(buf.len(), 4);
    }

    // ── Selection-aware vertex tests ──────────────────────────────────────

    #[test]
    fn test_collect_line_vertices_selected_entity_has_is_selected_1() {
        let mut world = hecs::World::new();
        let entity = world.spawn((
            LineData {
                start: Point2D::new(0.0, 0.0),
                end: Point2D::new(10.0, 10.0),
                color: Color::WHITE,
                width: 1.0,
            },
            Renderable,
        ));
        world.insert_one(entity, Selected).ok();

        let layer_table = LayerTable::new();
        let mut selected = HashSet::new();
        selected.insert(entity);

        let mut buf = Vec::new();
        EntityRenderer::collect_line_vertices(&world, &selected, &layer_table, &mut buf);

        assert_eq!(buf.len(), 2, "one line should produce 2 vertices");
        for v in &buf {
            assert_eq!(v.is_selected, 1, "selected entity vertices should have is_selected=1");
        }
    }

    #[test]
    fn test_collect_line_vertices_non_selected_entity_has_is_selected_0() {
        let mut world = hecs::World::new();
        let _entity = world.spawn((
            LineData {
                start: Point2D::new(5.0, 5.0),
                end: Point2D::new(15.0, 15.0),
                color: Color::WHITE,
                width: 1.0,
            },
            Renderable,
        ));
        // entity does NOT get Selected component, and is NOT in the selection set.

        let layer_table = LayerTable::new();
        let selected = HashSet::new(); // empty — entity is not selected

        let mut buf = Vec::new();
        EntityRenderer::collect_line_vertices(&world, &selected, &layer_table, &mut buf);

        assert_eq!(buf.len(), 2);
        for v in &buf {
            assert_eq!(v.is_selected, 0, "non-selected entity vertices should have is_selected=0");
        }
    }

    #[test]
    fn test_collect_line_vertices_mixed_selection_has_correct_is_selected() {
        let mut world = hecs::World::new();
        let sel_entity = world.spawn((
            LineData {
                start: Point2D::new(0.0, 0.0),
                end: Point2D::new(10.0, 10.0),
                color: Color::WHITE,
                width: 1.0,
            },
            Renderable,
        ));
        let _unsel_entity = world.spawn((
            LineData {
                start: Point2D::new(100.0, 100.0),
                end: Point2D::new(200.0, 200.0),
                color: Color::WHITE,
                width: 1.0,
            },
            Renderable,
        ));
        world.insert_one(sel_entity, Selected).ok();

        let layer_table = LayerTable::new();
        let mut selected = HashSet::new();
        selected.insert(sel_entity);

        let mut buf = Vec::new();
        EntityRenderer::collect_line_vertices(&world, &selected, &layer_table, &mut buf);

        // Two lines → 4 vertices. Count by selection flag (order-independent).
        assert_eq!(buf.len(), 4, "2 lines should produce 4 vertices");
        let sel_count = buf.iter().filter(|v| v.is_selected == 1).count();
        let unsel_count = buf.iter().filter(|v| v.is_selected == 0).count();
        assert_eq!(sel_count, 2, "exactly 2 vertices should be from the selected entity");
        assert_eq!(unsel_count, 2, "exactly 2 vertices should be from the non-selected entity");
    }

    #[test]
    fn test_collect_circle_vertices_selected_entity_has_is_selected_1() {
        let mut world = hecs::World::new();
        let entity = world.spawn((
            CircleData {
                center: Point2D::new(0.0, 0.0),
                radius: 10.0,
                color: Color::WHITE,
                width: 1.0,
            },
            Renderable,
        ));
        world.insert_one(entity, Selected).ok();

        let layer_table = LayerTable::new();
        let mut selected = HashSet::new();
        selected.insert(entity);

        let mut buf = Vec::new();
        EntityRenderer::collect_circle_vertices(&world, 1.0, &selected, &layer_table, &mut buf);

        assert!(!buf.is_empty(), "circle should produce vertices");
        for v in &buf {
            assert_eq!(v.is_selected, 1, "selected circle vertices should have is_selected=1");
        }
    }

    #[test]
    fn test_collect_circle_vertices_not_selected_has_is_selected_0() {
        let mut world = hecs::World::new();
        world.spawn((
            CircleData {
                center: Point2D::new(0.0, 0.0),
                radius: 10.0,
                color: Color::WHITE,
                width: 1.0,
            },
            Renderable,
        ));

        let layer_table = LayerTable::new();
        let selected = HashSet::new();
        let mut buf = Vec::new();
        EntityRenderer::collect_circle_vertices(&world, 1.0, &selected, &layer_table, &mut buf);

        assert!(!buf.is_empty());
        for v in &buf {
            assert_eq!(v.is_selected, 0, "non-selected circle vertices should have is_selected=0");
        }
    }

    #[test]
    fn test_collect_polyline_vertices_selected_entity_has_is_selected_1() {
        let mut world = hecs::World::new();
        let entity = world.spawn((
            PolylineData {
                vertices: vec![
                    Point2D::new(0.0, 0.0),
                    Point2D::new(50.0, 0.0),
                    Point2D::new(50.0, 50.0),
                ],
                closed: false,
                color: Color::WHITE,
                width: 1.0,
            },
            Renderable,
        ));
        world.insert_one(entity, Selected).ok();

        let layer_table = LayerTable::new();
        let mut selected = HashSet::new();
        selected.insert(entity);

        let mut buf = Vec::new();
        EntityRenderer::collect_polyline_vertices(&world, &selected, &layer_table, &mut buf);

        assert_eq!(buf.len(), 3, "open polyline with 3 points should produce 3 vertices");
        for v in &buf {
            assert_eq!(v.is_selected, 1, "selected polyline vertices should have is_selected=1");
        }
    }

    #[test]
    fn test_collect_polyline_vertices_not_selected_has_is_selected_0() {
        let mut world = hecs::World::new();
        world.spawn((
            PolylineData {
                vertices: vec![
                    Point2D::new(0.0, 0.0),
                    Point2D::new(30.0, 0.0),
                ],
                closed: false,
                color: Color::WHITE,
                width: 1.0,
            },
            Renderable,
        ));

        let layer_table = LayerTable::new();
        let selected = HashSet::new();
        let mut buf = Vec::new();
        EntityRenderer::collect_polyline_vertices(&world, &selected, &layer_table, &mut buf);

        assert_eq!(buf.len(), 2);
        for v in &buf {
            assert_eq!(v.is_selected, 0, "non-selected polyline vertices should have is_selected=0");
        }
    }

    // ── EntityVertex size ─────────────────────────────────────────────────

    #[test]
    fn test_entity_vertex_size_is_36() {
        assert_eq!(
            std::mem::size_of::<EntityVertex>(),
            36,
            "EntityVertex must be exactly 36 bytes (28 + 8 for line_dir)"
        );
    }

    #[test]
    fn test_entity_vertex_offset_of_line_dir() {
        let v = EntityVertex {
            position: [1.0, 2.0],
            color: [3.0, 4.0, 5.0, 6.0],
            is_selected: 7,
            line_dir: [8.0, 9.0],
        };
        // line_dir should be at byte offset 28.
        let bytes: &[u8] = bytemuck::bytes_of(&v);
        let line_dir_start = 28;
        let dir_x = f32::from_ne_bytes([
            bytes[line_dir_start],
            bytes[line_dir_start + 1],
            bytes[line_dir_start + 2],
            bytes[line_dir_start + 3],
        ]);
        let dir_y = f32::from_ne_bytes([
            bytes[line_dir_start + 4],
            bytes[line_dir_start + 5],
            bytes[line_dir_start + 6],
            bytes[line_dir_start + 7],
        ]);
        assert!((dir_x - 8.0).abs() < f32::EPSILON, "line_dir.x at offset 28");
        assert!((dir_y - 9.0).abs() < f32::EPSILON, "line_dir.y at offset 32");
    }

    // ── line_dir correctness for Line entities ─────────────────────────────

    #[test]
    fn test_line_vertices_have_line_dir() {
        let mut world = hecs::World::new();
        world.spawn((
            LineData {
                start: Point2D::new(0.0, 0.0),
                end: Point2D::new(10.0, 0.0), // horizontal right
                color: Color::WHITE,
                width: 1.0,
            },
            Renderable,
        ));

        let layer_table = LayerTable::new();
        let selected = HashSet::new();
        let mut buf = Vec::new();
        EntityRenderer::collect_line_vertices(&world, &selected, &layer_table, &mut buf);

        assert_eq!(buf.len(), 2, "one line → 2 vertices");
        for v in &buf {
            assert!((v.line_dir[0] - 1.0).abs() < f32::EPSILON, "line_dir.x should be 1.0 for (10,0)");
            assert!((v.line_dir[1] - 0.0).abs() < f32::EPSILON, "line_dir.y should be 0.0 for horizontal line");
        }
    }

    #[test]
    fn test_line_vertices_diagonal_dir() {
        let mut world = hecs::World::new();
        world.spawn((
            LineData {
                start: Point2D::new(0.0, 0.0),
                end: Point2D::new(3.0, 4.0), // 5-unit diagonal
                color: Color::WHITE,
                width: 1.0,
            },
            Renderable,
        ));

        let layer_table = LayerTable::new();
        let selected = HashSet::new();
        let mut buf = Vec::new();
        EntityRenderer::collect_line_vertices(&world, &selected, &layer_table, &mut buf);

        assert_eq!(buf.len(), 2);
        // normalize(3,4) = (0.6, 0.8)
        for v in &buf {
            assert!((v.line_dir[0] - 0.6).abs() < 1e-6, "line_dir.x should be 0.6, got {}", v.line_dir[0]);
            assert!((v.line_dir[1] - 0.8).abs() < 1e-6, "line_dir.y should be 0.8, got {}", v.line_dir[1]);
        }
    }

    // ── Zero-length line guard ─────────────────────────────────────────────

    #[test]
    fn test_zero_length_line_has_zero_dir() {
        let mut world = hecs::World::new();
        world.spawn((
            LineData {
                start: Point2D::new(5.0, 5.0),
                end: Point2D::new(5.0, 5.0), // same point → zero length
                color: Color::WHITE,
                width: 1.0,
            },
            Renderable,
        ));

        let layer_table = LayerTable::new();
        let selected = HashSet::new();
        let mut buf = Vec::new();
        EntityRenderer::collect_line_vertices(&world, &selected, &layer_table, &mut buf);

        assert_eq!(buf.len(), 2, "zero-length line still produces 2 vertices");
        for v in &buf {
            assert_eq!(v.line_dir, [0.0, 0.0], "zero-length line should have [0,0] dir");
        }
    }

    // ── line_dir for Polyline entities ─────────────────────────────────────

    #[test]
    fn test_polyline_vertices_have_per_segment_dir() {
        let mut world = hecs::World::new();
        world.spawn((
            PolylineData {
                vertices: vec![
                    Point2D::new(0.0, 0.0),
                    Point2D::new(10.0, 0.0),  // segment 0: right
                    Point2D::new(10.0, 10.0), // segment 1: up
                ],
                closed: false,
                color: Color::WHITE,
                width: 1.0,
            },
            Renderable,
        ));

        let layer_table = LayerTable::new();
        let selected = HashSet::new();
        let mut buf = Vec::new();
        EntityRenderer::collect_polyline_vertices(&world, &selected, &layer_table, &mut buf);

        // 3 points → 3 vertices (open polyline)
        assert_eq!(buf.len(), 3);
        // Point 0 → direction to point 1 = (1, 0)
        assert!((buf[0].line_dir[0] - 1.0).abs() < f32::EPSILON);
        assert!((buf[0].line_dir[1] - 0.0).abs() < f32::EPSILON);
        // Point 1 → direction to point 2 = (0, 1)
        assert!((buf[1].line_dir[0] - 0.0).abs() < f32::EPSILON);
        assert!((buf[1].line_dir[1] - 1.0).abs() < f32::EPSILON);
        // Point 2 (last) → reuses direction from previous segment = (0, 1)
        assert!((buf[2].line_dir[0] - 0.0).abs() < f32::EPSILON);
        assert!((buf[2].line_dir[1] - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_polyline_closed_loop_dir() {
        let mut world = hecs::World::new();
        world.spawn((
            PolylineData {
                vertices: vec![
                    Point2D::new(0.0, 0.0),
                    Point2D::new(10.0, 0.0),
                    Point2D::new(10.0, 10.0),
                ],
                closed: true,
                color: Color::WHITE,
                width: 1.0,
            },
            Renderable,
        ));

        let layer_table = LayerTable::new();
        let selected = HashSet::new();
        let mut buf = Vec::new();
        EntityRenderer::collect_polyline_vertices(&world, &selected, &layer_table, &mut buf);

        // 3 points + 1 closing vertex = 4 vertices for closed polyline
        assert_eq!(buf.len(), 4);

        // Interior vertex C (index 2) = (10,10), direction toward A (0,0)
        // = normalize(-10, -10) ≈ (-0.707, -0.707)
        let expected = -std::f64::consts::FRAC_1_SQRT_2 as f32;
        assert!((buf[2].line_dir[0] - expected).abs() < 1e-6,
            "interior vertex C line_dir.x should be {}, got {}", expected, buf[2].line_dir[0]);
        assert!((buf[2].line_dir[1] - expected).abs() < 1e-6,
            "interior vertex C line_dir.y should be {}, got {}", expected, buf[2].line_dir[1]);

        // Closing vertex (index 3) = first point, direction from last→first
        // last = (10,10), first = (0,0) → same direction
        assert!((buf[3].line_dir[0] - expected).abs() < 1e-6,
            "closing vertex line_dir.x should be {}, got {}", expected, buf[3].line_dir[0]);
        assert!((buf[3].line_dir[1] - expected).abs() < 1e-6,
            "closing vertex line_dir.y should be {}, got {}", expected, buf[3].line_dir[1]);
    }

    // ── line_dir for Circle / Arc (placeholder) ────────────────────────────

    #[test]
    fn test_circle_vertices_line_dir_is_zero() {
        let circle = CircleData {
            center: Point2D::new(0.0, 0.0),
            radius: 10.0,
            color: Color::WHITE,
            width: 1.0,
        };

        let mut buf = Vec::new();
        generate_circle_vertices(&circle, 1.0, 0, &mut buf);

        assert!(!buf.is_empty());
        for v in &buf {
            assert_eq!(v.line_dir, [0.0, 0.0], "circle vertices should have [0,0] line_dir");
        }
    }

    #[test]
    fn test_arc_vertices_line_dir_is_zero() {
        let arc = ArcData {
            center: Point2D::new(0.0, 0.0),
            radius: 50.0,
            start_angle: 0.0,
            end_angle: 90.0,
            color: Color::WHITE,
            width: 1.0,
        };

        let mut buf = Vec::new();
        generate_arc_vertices(&arc, 1.0, 0, &mut buf);

        assert!(!buf.is_empty());
        for v in &buf {
            assert_eq!(v.line_dir, [0.0, 0.0], "arc vertices should have [0,0] line_dir");
        }
    }

    // ── Layer culling ──────────────────────────────────────────────────────

    /// Helper: create a world with one line entity on the default layer.
    fn make_line_world() -> (hecs::World, hecs::Entity) {
        let mut world = hecs::World::new();
        let e = world.spawn((
            LineData {
                start: Point2D::new(0.0, 0.0),
                end: Point2D::new(10.0, 0.0),
                color: Color::WHITE,
                width: 1.0,
            },
            Renderable,
        ));
        (world, e)
    }

    #[test]
    fn test_layer_culling_visible_line_passes() {
        let (world, _e) = make_line_world();
        // Default layer (ID 0) is visible and not frozen by default.

        let mut table = LayerTable::new();
        // Default layer properties: visible=true, frozen=false
        let layer = table.get(LayerId::DEFAULT).unwrap();
        assert!(layer.visible, "default layer should be visible");
        assert!(!layer.frozen, "default layer should not be frozen");

        let layer_table = &table;
        let selected = HashSet::new();
        let mut buf = Vec::new();
        EntityRenderer::collect_line_vertices(&world, &selected, layer_table, &mut buf);

        // Entity has no LayerRef component → .unwrap_or(true) keeps it visible.
        assert_eq!(buf.len(), 2, "entity without LayerRef should be rendered");
    }

    #[test]
    fn test_layer_culling_entity_with_layer_ref_on_visible_layer() {
        let (mut world, e) = make_line_world();
        // Attach a LayerRef pointing to the default layer (ID 0).
        world.insert_one(e, LayerRef(0)).ok();

        let table = LayerTable::new();
        // Default layer is visible and not frozen.
        let mut buf = Vec::new();
        let selected = HashSet::new();
        EntityRenderer::collect_line_vertices(
            &world,
            &selected,
            &table,
            &mut buf,
        );

        assert_eq!(
            buf.len(),
            2,
            "entity on visible layer should produce vertices"
        );
    }

    #[test]
    fn test_layer_culling_hidden_layer_skips_entity() {
        let (mut world, e) = make_line_world();
        world.insert_one(e, LayerRef(0)).ok();

        let mut table = LayerTable::new();
        // Hide the default layer.
        let default = table.get_mut(LayerId::DEFAULT).unwrap();
        default.visible = false;

        let mut buf = Vec::new();
        let selected = HashSet::new();
        EntityRenderer::collect_line_vertices(
            &world,
            &selected,
            &table,
            &mut buf,
        );

        assert!(
            buf.is_empty(),
            "entity on hidden layer should be culled"
        );
    }

    #[test]
    fn test_layer_culling_frozen_layer_skips_entity() {
        let (mut world, e) = make_line_world();
        world.insert_one(e, LayerRef(0)).ok();

        let mut table = LayerTable::new();
        // Freeze the default layer.
        let default = table.get_mut(LayerId::DEFAULT).unwrap();
        default.frozen = true;

        let mut buf = Vec::new();
        let selected = HashSet::new();
        EntityRenderer::collect_line_vertices(
            &world,
            &selected,
            &table,
            &mut buf,
        );

        assert!(
            buf.is_empty(),
            "entity on frozen layer should be culled"
        );
    }

    #[test]
    fn test_layer_culling_missing_layer_renders_conservatively() {
        // Entity references a layer ID that doesn't exist in the table.
        // The lookup returns None → .unwrap_or(true) should keep it visible.

        let (mut world, e) = make_line_world();
        world.insert_one(e, LayerRef(999)).ok(); // non-existent layer

        let table = LayerTable::new(); // only has default layer (ID 0)
        let mut buf = Vec::new();
        let selected = HashSet::new();
        EntityRenderer::collect_line_vertices(
            &world,
            &selected,
            &table,
            &mut buf,
        );

        assert_eq!(
            buf.len(),
            2,
            "entity referencing missing layer should be rendered (conservative)"
        );
    }

    // ── Layer culling: Circle ──────────────────────────────────────────────

    #[test]
    fn test_layer_culling_hidden_layer_skips_circle() {
        let mut world = hecs::World::new();
        let e = world.spawn((
            CircleData {
                center: Point2D::new(0.0, 0.0),
                radius: 10.0,
                color: Color::WHITE,
                width: 1.0,
            },
            Renderable,
        ));
        world.insert_one(e, LayerRef(0)).ok();

        let mut table = LayerTable::new();
        table.get_mut(LayerId::DEFAULT).unwrap().visible = false;

        let mut buf = Vec::new();
        let selected = HashSet::new();
        EntityRenderer::collect_circle_vertices(&world, 1.0, &selected, &table, &mut buf);
        assert!(buf.is_empty(), "circle on hidden layer should be culled");
    }

    #[test]
    fn test_layer_culling_frozen_layer_skips_circle() {
        let mut world = hecs::World::new();
        let e = world.spawn((
            CircleData {
                center: Point2D::new(0.0, 0.0),
                radius: 10.0,
                color: Color::WHITE,
                width: 1.0,
            },
            Renderable,
        ));
        world.insert_one(e, LayerRef(0)).ok();

        let mut table = LayerTable::new();
        table.get_mut(LayerId::DEFAULT).unwrap().frozen = true;

        let mut buf = Vec::new();
        let selected = HashSet::new();
        EntityRenderer::collect_circle_vertices(&world, 1.0, &selected, &table, &mut buf);
        assert!(buf.is_empty(), "circle on frozen layer should be culled");
    }

    // ── Layer culling: Arc ────────────────────────────────────────────────

    #[test]
    fn test_layer_culling_hidden_layer_skips_arc() {
        let mut world = hecs::World::new();
        let e = world.spawn((
            ArcData {
                center: Point2D::new(0.0, 0.0),
                radius: 10.0,
                start_angle: 0.0,
                end_angle: 90.0,
                color: Color::WHITE,
                width: 1.0,
            },
            Renderable,
        ));
        world.insert_one(e, LayerRef(0)).ok();

        let mut table = LayerTable::new();
        table.get_mut(LayerId::DEFAULT).unwrap().visible = false;

        let mut buf = Vec::new();
        let selected = HashSet::new();
        EntityRenderer::collect_arc_vertices(&world, 1.0, &selected, &table, &mut buf);
        assert!(buf.is_empty(), "arc on hidden layer should be culled");
    }

    #[test]
    fn test_layer_culling_frozen_layer_skips_arc() {
        let mut world = hecs::World::new();
        let e = world.spawn((
            ArcData {
                center: Point2D::new(0.0, 0.0),
                radius: 10.0,
                start_angle: 0.0,
                end_angle: 90.0,
                color: Color::WHITE,
                width: 1.0,
            },
            Renderable,
        ));
        world.insert_one(e, LayerRef(0)).ok();

        let mut table = LayerTable::new();
        table.get_mut(LayerId::DEFAULT).unwrap().frozen = true;

        let mut buf = Vec::new();
        let selected = HashSet::new();
        EntityRenderer::collect_arc_vertices(&world, 1.0, &selected, &table, &mut buf);
        assert!(buf.is_empty(), "arc on frozen layer should be culled");
    }

    // ── Layer culling: Polyline ───────────────────────────────────────────

    #[test]
    fn test_layer_culling_hidden_layer_skips_polyline() {
        let mut world = hecs::World::new();
        let e = world.spawn((
            PolylineData {
                vertices: vec![
                    Point2D::new(0.0, 0.0),
                    Point2D::new(10.0, 0.0),
                ],
                closed: false,
                color: Color::WHITE,
                width: 1.0,
            },
            Renderable,
        ));
        world.insert_one(e, LayerRef(0)).ok();

        let mut table = LayerTable::new();
        table.get_mut(LayerId::DEFAULT).unwrap().visible = false;

        let mut buf = Vec::new();
        let selected = HashSet::new();
        EntityRenderer::collect_polyline_vertices(&world, &selected, &table, &mut buf);
        assert!(buf.is_empty(), "polyline on hidden layer should be culled");
    }

    #[test]
    fn test_layer_culling_frozen_layer_skips_polyline() {
        let mut world = hecs::World::new();
        let e = world.spawn((
            PolylineData {
                vertices: vec![
                    Point2D::new(0.0, 0.0),
                    Point2D::new(10.0, 0.0),
                ],
                closed: false,
                color: Color::WHITE,
                width: 1.0,
            },
            Renderable,
        ));
        world.insert_one(e, LayerRef(0)).ok();

        let mut table = LayerTable::new();
        table.get_mut(LayerId::DEFAULT).unwrap().frozen = true;

        let mut buf = Vec::new();
        let selected = HashSet::new();
        EntityRenderer::collect_polyline_vertices(&world, &selected, &table, &mut buf);
        assert!(buf.is_empty(), "polyline on frozen layer should be culled");
    }

    // ── Layer culling: mixed visible + hidden in same world ────────────────

    #[test]
    fn test_layer_culling_mixed_visible_hidden() {
        let mut world = hecs::World::new();

        // Entity on hidden layer.
        let hidden_e = world.spawn((
            LineData {
                start: Point2D::new(0.0, 0.0),
                end: Point2D::new(10.0, 0.0),
                color: Color::WHITE,
                width: 1.0,
            },
            Renderable,
        ));
        world.insert_one(hidden_e, LayerRef(0)).ok();

        // Entity on visible layer (default, no LayerRef).
        world.spawn((
            LineData {
                start: Point2D::new(100.0, 0.0),
                end: Point2D::new(110.0, 0.0),
                color: Color::WHITE,
                width: 1.0,
            },
            Renderable,
        ));

        let mut table = LayerTable::new();
        table.get_mut(LayerId::DEFAULT).unwrap().visible = false;

        let mut buf = Vec::new();
        let selected = HashSet::new();
        EntityRenderer::collect_line_vertices(&world, &selected, &table, &mut buf);

        // Entity without LayerRef is always visible (conservative default).
        assert_eq!(buf.len(), 2, "entity without LayerRef should still render");
        assert!(
            buf.iter().all(|v| v.position == [100.0, 0.0] || v.position == [110.0, 0.0]),
            "only visible entity vertices should be in the buffer"
        );
    }

    // ── line_dir: vertical line ────────────────────────────────────────────

    #[test]
    fn test_line_vertices_vertical_dir() {
        let mut world = hecs::World::new();
        world.spawn((
            LineData {
                start: Point2D::new(0.0, 0.0),
                end: Point2D::new(0.0, 10.0), // vertical up
                color: Color::WHITE,
                width: 1.0,
            },
            Renderable,
        ));

        let layer_table = LayerTable::new();
        let selected = HashSet::new();
        let mut buf = Vec::new();
        EntityRenderer::collect_line_vertices(&world, &selected, &layer_table, &mut buf);

        assert_eq!(buf.len(), 2);
        // normalize(0, 10) = (0, 1)
        assert!(
            (buf[0].line_dir[0] - 0.0).abs() < f32::EPSILON,
            "vertical line line_dir.x should be 0.0, got {}",
            buf[0].line_dir[0]
        );
        assert!(
            (buf[0].line_dir[1] - 1.0).abs() < f32::EPSILON,
            "vertical line line_dir.y should be 1.0, got {}",
            buf[0].line_dir[1]
        );
    }
}
