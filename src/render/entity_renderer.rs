//! Entity geometry renderer.
//!
//! Renders Line, Circle, Arc, and Polyline entities from the ECS.
//! Batches all entities of the same type into a single draw call.
//!
//! # Render-pass contract
//!
//! `render()` **does not** clear the colour attachment — it uses `LoadOp::Load`
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

/// Number of segments used to approximate a full circle (line strip).
const CIRCLE_SEGMENTS: u32 = 64;

/// Minimum number of segments for an arc.
const MIN_ARC_SEGMENTS: u32 = 8;

// ─── Vertex ──────────────────────────────────────────────────────────────────

/// A single entity vertex: 2 × f32 position + 4 × f32 colour = 24 bytes.
///
/// Layout matches `GridVertex` in `grid.rs` so shaders are interchangeable.
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct EntityVertex {
    position: [f32; 2],
    color: [f32; 4],
}

// ─── Helper: colour array ────────────────────────────────────────────────────

/// Convert a `Color` value to an `[f32; 4]` array for vertex data.
#[inline]
fn color_to_array(c: &crate::util::Color) -> [f32; 4] {
    [c.r, c.g, c.b, c.a]
}

// ─── Helper: generate circle vertices ────────────────────────────────────────

/// Generate vertices for a circle approximation as a line strip.
///
/// Produces precisely `CIRCLE_SEGMENTS + 1` vertices so the strip forms a
/// closed loop (the last vertex equals the first).
fn generate_circle_vertices(circle: &CircleData) -> Vec<EntityVertex> {
    let col = color_to_array(&circle.color);
    let cx = circle.center.x;
    let cy = circle.center.y;
    let r = circle.radius;
    let step = 2.0 * PI / CIRCLE_SEGMENTS as f64;

    let mut verts = Vec::with_capacity(CIRCLE_SEGMENTS as usize + 1);
    for i in 0..=CIRCLE_SEGMENTS {
        let theta = step * i as f64;
        verts.push(EntityVertex {
            position: [(cx + r * theta.cos()) as f32, (cy + r * theta.sin()) as f32],
            color: col,
        });
    }
    verts
}

// ─── Helper: generate arc vertices ──────────────────────────────────────────

/// Generate vertices for an arc approximation as a line strip.
///
/// Segment count is proportional to the sweep angle, between
/// `MIN_ARC_SEGMENTS` and `CIRCLE_SEGMENTS`. The last vertex is
/// the end of the arc (not wrapped to start).
fn generate_arc_vertices(arc: &ArcData) -> Vec<EntityVertex> {
    let col = color_to_array(&arc.color);
    let cx = arc.center.x;
    let cy = arc.center.y;
    let r = arc.radius;

    // Angles are stored in degrees; convert to radians.
    let start_rad = arc.start_angle * PI / 180.0;
    let end_rad = arc.end_angle * PI / 180.0;
    let sweep_rad = end_rad - start_rad;

    // Scale segment count proportionally to the sweep.
    let sweep_fraction = sweep_rad.abs() / (2.0 * PI);
    let num_segments = ((CIRCLE_SEGMENTS as f64) * sweep_fraction)
        .round()
        .max(MIN_ARC_SEGMENTS as f64) as u32;

    let step = sweep_rad / num_segments as f64;

    let mut verts = Vec::with_capacity(num_segments as usize + 1);
    for i in 0..=num_segments {
        let theta = start_rad + step * i as f64;
        verts.push(EntityVertex {
            position: [(cx + r * theta.cos()) as f32, (cy + r * theta.sin()) as f32],
            color: col,
        });
    }
    verts
}

// ─── EntityRenderer implementation ──────────────────────────────────────────

impl EntityRenderer {
    /// Create a new `EntityRenderer`.
    ///
    /// Loads four WGSL shaders (line, circle — arc and polyline reuse one of
    /// the two) and creates a dedicated render pipeline per entity type.
    /// All pipelines share the `camera_bind_group_layout` for group(0).
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
                             shader_source: &str,
                             topology: wgpu::PrimitiveTopology,
                             layout: &wgpu::PipelineLayout|
         -> wgpu::RenderPipeline {
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(label),
                source: wgpu::ShaderSource::Wgsl(shader_source.into()),
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

        // ── Pipeline layouts ─────────────────────────────────────────────
        let line_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Line Entity Pipeline Layout"),
            bind_group_layouts: &[Some(camera_bind_group_layout)],
            immediate_size: 0,
        });
        let circle_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Circle Entity Pipeline Layout"),
            bind_group_layouts: &[Some(camera_bind_group_layout)],
            immediate_size: 0,
        });
        let arc_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Arc Entity Pipeline Layout"),
            bind_group_layouts: &[Some(camera_bind_group_layout)],
            immediate_size: 0,
        });
        let polyline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Polyline Entity Pipeline Layout"),
            bind_group_layouts: &[Some(camera_bind_group_layout)],
            immediate_size: 0,
        });

        // ── Shader sources ───────────────────────────────────────────────
        let line_shader = include_str!("shaders/line.wgsl");
        let circle_shader = include_str!("shaders/circle.wgsl");
        // Arc and polyline reuse the line/circle shaders — topology differences
        // are set in the pipeline primitive state, not in WGSL.
        let arc_shader = circle_shader;
        let polyline_shader = line_shader;

        // ── Pipelines ────────────────────────────────────────────────────
        let line_pipeline = make_pipeline(
            device,
            "Line Entity Render Pipeline",
            line_shader,
            wgpu::PrimitiveTopology::LineList,
            &line_layout,
        );
        let circle_pipeline = make_pipeline(
            device,
            "Circle Entity Render Pipeline",
            circle_shader,
            wgpu::PrimitiveTopology::LineStrip,
            &circle_layout,
        );
        let arc_pipeline = make_pipeline(
            device,
            "Arc Entity Render Pipeline",
            arc_shader,
            wgpu::PrimitiveTopology::LineStrip,
            &arc_layout,
        );
        let polyline_pipeline = make_pipeline(
            device,
            "Polyline Entity Render Pipeline",
            polyline_shader,
            wgpu::PrimitiveTopology::LineList,
            &polyline_layout,
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
        }
    }

    /// Render all entities from the ECS world.
    ///
    /// For each entity type (Line, Circle, Arc, Polyline) with a `Renderable`
    /// marker, vertex data is generated, uploaded to the respective staging
    /// buffer, and drawn in a single render pass.  Staging buffers are
    /// automatically doubled if they overflow.
    ///
    /// # Parameters
    ///
    /// * `encoder` — Active command encoder for this frame.
    /// * `view` — Colour attachment texture view.
    /// * `world` — The ECS world to query for entities.
    /// * `camera_bind_group` — Bind group holding the camera uniform buffer.
    /// * `queue` — Command queue (used for `write_buffer` on the staging buffers).
    /// * `device` — GPU device (used to re-create a staging buffer if it
    ///   needs to grow).
    pub fn render(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        world: &hecs::World,
        camera_bind_group: &wgpu::BindGroup,
        queue: &wgpu::Queue,
        device: &wgpu::Device,
    ) {
        // ── 1. Generate per-type vertex lists ────────────────────────────

        let line_vertices = self.collect_line_vertices(world);
        let circle_vertices = self.collect_circle_vertices(world);
        let arc_vertices = self.collect_arc_vertices(world);
        let polyline_vertices = self.collect_polyline_vertices(world);

        // ── 2. Upload to staging buffers (resize if needed) ──────────────

        Self::upload_vertices::<EntityVertex>(
            queue,
            device,
            &mut self.line_staging,
            &mut self.line_staging_capacity,
            &line_vertices,
            "Line Entity Staging Buffer",
        );
        Self::upload_vertices::<EntityVertex>(
            queue,
            device,
            &mut self.circle_staging,
            &mut self.circle_staging_capacity,
            &circle_vertices,
            "Circle Entity Staging Buffer",
        );
        Self::upload_vertices::<EntityVertex>(
            queue,
            device,
            &mut self.arc_staging,
            &mut self.arc_staging_capacity,
            &arc_vertices,
            "Arc Entity Staging Buffer",
        );
        Self::upload_vertices::<EntityVertex>(
            queue,
            device,
            &mut self.polyline_staging,
            &mut self.polyline_staging_capacity,
            &polyline_vertices,
            "Polyline Entity Staging Buffer",
        );

        // ── 3. Single render pass (LoadOp::Load — grid already cleared) ──
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

        // ── 4. Draw each non-empty type ───────────────────────────────────
        if !line_vertices.is_empty() {
            render_pass.set_pipeline(&self.line_pipeline);
            render_pass.set_bind_group(0, camera_bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.line_staging.slice(..));
            render_pass.draw(0..line_vertices.len() as u32, 0..1);
        }

        if !circle_vertices.is_empty() {
            render_pass.set_pipeline(&self.circle_pipeline);
            render_pass.set_bind_group(0, camera_bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.circle_staging.slice(..));
            render_pass.draw(0..circle_vertices.len() as u32, 0..1);
        }

        if !arc_vertices.is_empty() {
            render_pass.set_pipeline(&self.arc_pipeline);
            render_pass.set_bind_group(0, camera_bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.arc_staging.slice(..));
            render_pass.draw(0..arc_vertices.len() as u32, 0..1);
        }

        if !polyline_vertices.is_empty() {
            render_pass.set_pipeline(&self.polyline_pipeline);
            render_pass.set_bind_group(0, camera_bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.polyline_staging.slice(..));
            render_pass.draw(0..polyline_vertices.len() as u32, 0..1);
        }
    }

    // ── Private helpers ───────────────────────────────────────────────────

    /// Collect vertices for all `LineData + Renderable` entities.
    fn collect_line_vertices(&self, world: &hecs::World) -> Vec<EntityVertex> {
        let mut query = world.query::<(&LineData, &Renderable)>();
        let mut verts = Vec::new();
        for (_, (line, _)) in query.iter() {
            let col = color_to_array(&line.color);
            verts.push(EntityVertex {
                position: line.start.to_f32_array(),
                color: col,
            });
            verts.push(EntityVertex {
                position: line.end.to_f32_array(),
                color: col,
            });
        }
        verts
    }

    /// Collect vertices for all `CircleData + Renderable` entities.
    fn collect_circle_vertices(&self, world: &hecs::World) -> Vec<EntityVertex> {
        let mut query = world.query::<(&CircleData, &Renderable)>();
        let mut verts = Vec::new();
        for (_, (circle, _)) in query.iter() {
            verts.extend(generate_circle_vertices(circle));
        }
        verts
    }

    /// Collect vertices for all `ArcData + Renderable` entities.
    fn collect_arc_vertices(&self, world: &hecs::World) -> Vec<EntityVertex> {
        let mut query = world.query::<(&ArcData, &Renderable)>();
        let mut verts = Vec::new();
        for (_, (arc, _)) in query.iter() {
            verts.extend(generate_arc_vertices(arc));
        }
        verts
    }

    /// Collect vertices for all `PolylineData + Renderable` entities.
    fn collect_polyline_vertices(&self, world: &hecs::World) -> Vec<EntityVertex> {
        let mut query = world.query::<(&PolylineData, &Renderable)>();
        let mut verts = Vec::new();
        for (_, (poly, _)) in query.iter() {
            let col = color_to_array(&poly.color);
            // Emit one vertex per point.
            for point in &poly.vertices {
                verts.push(EntityVertex {
                    position: point.to_f32_array(),
                    color: col,
                });
            }
            // If closed, emit the first vertex again to close the loop.
            if poly.closed && poly.vertices.len() > 1 {
                verts.push(EntityVertex {
                    position: poly.vertices[0].to_f32_array(),
                    color: col,
                });
            }
        }
        verts
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
