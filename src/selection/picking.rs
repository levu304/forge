//! GPU picking pass.
//!
//! Renders entity instance indices to an offscreen `Rgba32Uint` texture and
//! performs asynchronous CPU readback to determine which entity (if any) is
//! under the cursor.
//!
//! # Two-phase protocol
//!
//! 1. **Frame N (request):** Call [`request_pick_at`](PickingPass::request_pick_at)
//!    with the screen coordinates of the click. Stores the coordinates as
//!    "pending".
//! 2. **Frame N (render):** Call [`render`](PickingPass::render). If a request
//!    is pending, the pass regenerates the entity-to-index mapping from the
//!    ECS world, renders all `Renderable` entities to the offscreen ID
//!    framebuffer (each entity's instance_index is its position in the mapping),
//!    and copies the pixel at the requested coordinates to a staging buffer.
//! 3. **Frame N+1 (resolve):** Call [`resolve_pick`](PickingPass::resolve_pick)
//!    to map the staging buffer, decode the u32 pixel value back to an entity
//!    handle, and return it. Returns `None` if the pick hit empty space (the
//!    sentinel value `0xFFFFFFFF`).
//!
//! # Performance
//!
//! The picking pass renders entities individually (one draw call per entity)
//! rather than batching them. This is acceptable because picking only executes
//! on mouse click (not every frame), and typical CAD scenes have <10 000
//! entities.
//!
//! The entity mapping is regenerated every time the pass renders, which
//! prevents stale entity handles at the cost of a full ECS query per picking
//! frame.

use std::f64::consts::PI;

use bytemuck;
use hecs::World;

use crate::ecs::components::{ArcData, CircleData, LineData, PolylineData, Renderable};

// ─── Constants ────────────────────────────────────────────────────────────────

/// Sentinel value written to the picking texture for empty space.
/// Fragment shader writes this for the clear colour; any pixel that was
/// not covered by an entity will read back as `0xFFFFFFFF`.
const PICKING_SENTINEL: u32 = 0xFFFFFFFF;

/// Initial capacity of the vertex staging buffer in bytes (64 KiB).
const INITIAL_STAGING_CAPACITY: u64 = 65536;

/// Minimum number of tessellation segments for circles/arcs in the picking
/// pass. Matches the entity renderer's `CIRCLE_MIN_SEGMENTS` (8) to ensure
/// picking coverage approximately matches visible geometry.
const MIN_SEGMENTS: u32 = 8;

/// Maximum segments for circles/arcs in the picking pass. Capped lower than
/// the entity renderer (which goes to 256) because picking precision beyond
/// ~64 segments is not perceptible for hit-testing.
const MAX_SEGMENTS: u32 = 64;

// ─── Vertex Type ─────────────────────────────────────────────────────────────

/// A 2D vertex with position only — used by the picking pipeline.
///
/// Stride = 8 bytes (two `f32` values). This is intentionally simpler than
/// [`EntityVertex`](crate::render::entity_renderer::EntityVertex) because the
/// picking shader does not read colour or selection state.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct PickingVertex {
    position: [f32; 2],
}

// ─── Draw Command ────────────────────────────────────────────────────────────

/// Describes a single entity's draw call in the picking render pass.
struct EntityDrawCmd {
    /// Index into [`PickingPass::mapping`] — passed as `instance_index`.
    entity_idx: u32,
    /// Offset (in vertices) into the picking staging buffer.
    vertex_offset: u32,
    /// Number of vertices to draw.
    vertex_count: u32,
    /// `true` → `LineStrip` topology (circles, arcs);
    /// `false` → `LineList` topology (lines, polylines).
    is_strip: bool,
}

// ─── PickingPass ──────────────────────────────────────────────────────────────

/// GPU picking pass with offscreen ID framebuffer and async readback.
pub struct PickingPass {
    /// Offscreen `Rgba32Uint` texture (viewport-sized).
    texture: wgpu::Texture,
    /// Texture view for the render pass colour attachment.
    view: wgpu::TextureView,
    /// Render pipeline for `LineList` topology (lines, polylines).
    line_list_pipeline: wgpu::RenderPipeline,
    /// Render pipeline for `LineStrip` topology (circles, arcs).
    line_strip_pipeline: wgpu::RenderPipeline,
    /// Staging buffer (GPU-visible) for vertex data, uploaded each picking
    /// frame. Grows on demand.
    staging_buffer: wgpu::Buffer,
    /// Current byte capacity of the staging buffer.
    staging_capacity: u64,
    /// CPU-readable staging buffer for single-pixel readback (4 bytes).
    readback_buffer: wgpu::Buffer,
    /// Screen coordinates whose pixel value is awaiting readback on the
    /// **next** frame. `None` when no pick is in flight.
    pending_result: Option<(u32, u32)>,
    /// Cached result from the last successful resolve.
    last_entity: Option<hecs::Entity>,
    /// Mapping from instance index → entity handle, regenerated each frame.
    mapping: Vec<hecs::Entity>,
    /// Per-frame draw commands, rebuilt each time `render()` is called.
    draw_cmds: Vec<EntityDrawCmd>,
    /// Per-frame scratch vertex data, rebuilt each time `render()` is called.
    scratch: Vec<PickingVertex>,
    /// Viewport size in pixels, used for coordinate clamping.
    viewport_size: (u32, u32),
}

impl PickingPass {
    /// Create a new `PickingPass` with the given viewport dimensions.
    ///
    /// # Arguments
    ///
    /// * `device` — The wgpu device used for resource creation.
    /// * `camera_bind_group_layout` — The shared camera uniform bind-group
    ///   layout (group 0, binding 0). Must match the layout used by the
    ///   entity renderer and the rest of the render pipeline.
    /// * `viewport_size` — Initial dimensions `(width, height)` of the
    ///   offscreen picking texture.
    pub fn new(
        device: &wgpu::Device,
        camera_bind_group_layout: &wgpu::BindGroupLayout,
        viewport_size: (u32, u32),
    ) -> Self {
        let (width, height) = (viewport_size.0.max(1), viewport_size.1.max(1));

        // ── Offscreen Rgba32Uint texture ──────────────────────────────────
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Picking Pass Texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Uint,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        // ── Shader ────────────────────────────────────────────────────────
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Picking Shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../render/shaders/picking.wgsl").into(),
            ),
        });

        // ── Vertex buffer layout (position only, stride = 8) ─────────────
        let vertex_buffer_layout = wgpu::VertexBufferLayout {
            array_stride: 8, // 2 × f32
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 0,
                shader_location: 0,
            }],
        };

        // ── Pipeline layout (shares camera bind group) ───────────────────
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Picking Pipeline Layout"),
            bind_group_layouts: &[Some(camera_bind_group_layout)],
            immediate_size: 0,
        });

        // ── Helper: create a picking pipeline ────────────────────────────
        let make_pipeline = |device: &wgpu::Device,
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
                        format: wgpu::TextureFormat::Rgba32Uint,
                        // No blending for the uint format — we overwrite with
                        // the entity ID (or sentinel on clear).
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                }),
                multiview_mask: None,
                cache: None,
            })
        };

        let line_list_pipeline =
            make_pipeline(device, "Picking LineList Pipeline", wgpu::PrimitiveTopology::LineList);
        let line_strip_pipeline = make_pipeline(
            device,
            "Picking LineStrip Pipeline",
            wgpu::PrimitiveTopology::LineStrip,
        );

        // ── Staging buffer (vertex upload) ───────────────────────────────
        let staging_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Picking Vertex Staging Buffer"),
            size: INITIAL_STAGING_CAPACITY,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // ── Readback buffer (single pixel, 4 bytes) ─────────────────────
        let readback_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Picking Readback Buffer"),
            size: 4,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        Self {
            texture,
            view,
            line_list_pipeline,
            line_strip_pipeline,
            staging_buffer,
            staging_capacity: INITIAL_STAGING_CAPACITY,
            readback_buffer,
            pending_result: None,
            last_entity: None,
            mapping: Vec::new(),
            draw_cmds: Vec::new(),
            scratch: Vec::new(),
            viewport_size: (width, height),
        }
    }

    // ── Public API ─────────────────────────────────────────────────────────

    /// Record a pick request at the given screen coordinates.
    ///
    /// The actual readback happens on the **next** frame after
    /// [`render`](Self::render) has been called. Coordinates are not clamped
    /// here but will be clamped to the viewport bounds during `render()`.
    pub fn request_pick_at(&mut self, screen: (u32, u32)) {
        self.pending_result = Some(screen);
    }

    /// Render the picking pass (if a request is pending).
    ///
    /// 1. Queries the ECS world for all `Renderable` entities and builds
    ///    a mapping from instance index → entity handle.
    /// 2. Generates per-entity vertex data for every entity type (Line,
    ///    Circle, Arc, Polyline) into a single scratch buffer.
    /// 3. Uploads the scratch buffer to the GPU staging buffer.
    /// 4. Clears the offscreen `Rgba32Uint` texture to the sentinel value
    ///    (`0xFFFFFFFF`), then draws every entity individually (one draw
    ///    call per entity) with `instance_index` set to the entity's
    ///    position in the mapping.
    /// 5. Copies the pixel at the requested coordinates to the readback
    ///    buffer for CPU consumption on the next frame.
    ///
    /// If no request is pending (`pending_result` is `None`), this method
    /// is a no-op.
    ///
    /// # Note on tessellation
    ///
    /// Circles and arcs are tessellated with a fixed segment count
    /// (zoom-dependent, clamped to 8–64). This is generous enough to
    /// cover the visible geometry; picking may be slightly more permissive
    /// than the visual representation, which is acceptable for v0.2.0.
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        world: &World,
        camera_bind_group: &wgpu::BindGroup,
        queue: &wgpu::Queue,
        device: &wgpu::Device,
        zoom: f64,
    ) {
        let pending = match self.pending_result {
            Some(p) => p,
            None => return,
        };

        // ── 1. Clear per-frame state ─────────────────────────────────
        self.mapping.clear();
        self.draw_cmds.clear();
        self.scratch.clear();

        // ── 2. Collect entities and generate vertex data ─────────────

        Self::collect_lines(world, &mut self.mapping, &mut self.scratch, &mut self.draw_cmds);
        Self::collect_circles(
            world,
            zoom,
            &mut self.mapping,
            &mut self.scratch,
            &mut self.draw_cmds,
        );
        Self::collect_arcs(
            world,
            zoom,
            &mut self.mapping,
            &mut self.scratch,
            &mut self.draw_cmds,
        );
        Self::collect_polylines(
            world,
            &mut self.mapping,
            &mut self.scratch,
            &mut self.draw_cmds,
        );

        // ── 3. Upload scratch vertex data to GPU staging buffer ──────
        if !self.scratch.is_empty() {
            let needed = (self.scratch.len() * std::mem::size_of::<PickingVertex>()) as u64;
            if needed > self.staging_capacity {
                let new_size = needed
                    .max(self.staging_capacity.saturating_mul(2))
                    .max(INITIAL_STAGING_CAPACITY);
                self.staging_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("Picking Vertex Staging Buffer"),
                    size: new_size,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                self.staging_capacity = new_size;
            }
            let bytes: &[u8] = bytemuck::cast_slice(&self.scratch);
            queue.write_buffer(&self.staging_buffer, 0, bytes);
        }

        // ── 4. Render picking pass — clear to sentinel, draw entities ─
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Picking Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    // Clear to sentinel value (R = 0xFFFFFFFF, G = 0, B = 0, A = 0xFF).
                    //
                    // For integer texture formats (Rgba32Uint), wgpu casts each
                    // wgpu::Color f64 component directly to the channel's uint type:
                    //   `value as u32`
                    // It does NOT normalise through [0, 1] — that only happens for
                    // float-format textures. So to get 0xFFFFFFFF in the R channel
                    // we write 4294967295.0 (= PICKING_SENTINEL as f64), which
                    // truncates to 4294967295u32 = 0xFFFFFFFF.
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: PICKING_SENTINEL as f64,
                        g: 0.0,
                        b: 0.0,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

        if !self.draw_cmds.is_empty() {
            let stride = std::mem::size_of::<PickingVertex>() as u64;

            // Track the currently-bound pipeline to minimise state changes.
            let mut current_is_strip: Option<bool> = None;

            for cmd in &self.draw_cmds {
                // Set pipeline if this draw call needs a different topology.
                let needs_strip = cmd.is_strip;
                if current_is_strip != Some(needs_strip) {
                    if needs_strip {
                        pass.set_pipeline(&self.line_strip_pipeline);
                    } else {
                        pass.set_pipeline(&self.line_list_pipeline);
                    }
                    pass.set_bind_group(0, camera_bind_group, &[]);
                    current_is_strip = Some(needs_strip);
                }

                // Bind the vertex slice for this entity.
                let byte_start = cmd.vertex_offset as u64 * stride;
                let byte_end = byte_start + cmd.vertex_count as u64 * stride;
                pass.set_vertex_buffer(0, self.staging_buffer.slice(byte_start..byte_end));

                // Draw with entity_idx as instance_index (second range arg).
                // This is how the fragment shader knows which entity this pixel belongs to.
                pass.draw(0..cmd.vertex_count, cmd.entity_idx..cmd.entity_idx + 1);
            }
        }

        drop(pass);

        // ── 5. Copy the picked pixel to the readback buffer ───────────
        let (px, py) = pending;
        let px = px.min(self.viewport_size.0.saturating_sub(1));
        let py = py.min(self.viewport_size.1.saturating_sub(1));

        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: px,
                    y: py,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &self.readback_buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4),
                    rows_per_image: Some(1),
                },
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
    }

    /// Resolve a previously submitted pick request.
    ///
    /// Must be called **on the frame after** [`render`](Self::render).
    /// Maps the readback staging buffer, decodes the u32 pixel value, and
    /// looks up the corresponding entity handle in the mapping.
    ///
    /// Returns `Some(entity)` if a valid entity was under the cursor,
    /// or `None` if the cursor was over empty space (sentinel value) or
    /// if no pick request was pending.
    ///
    /// After this call, `pending_result` is cleared and the internal
    /// mapping is not discarded (it will be regenerated on the next
    /// `render()` call).
    pub fn resolve_pick(&mut self, device: &wgpu::Device) -> Option<hecs::Entity> {
        if self.pending_result.is_none() {
            return self.last_entity;
        }

        let slice = self.readback_buffer.slice(..);

        // Submit the map request and block until it completes.
        // For v0.2.0 this synchronous approach is acceptable; a future
        // optimisation could use a timeout or callback-based approach.
        let (sender, receiver) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        let _ = device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        });

        match receiver.recv() {
            Ok(Ok(())) => {
                let data = slice.get_mapped_range();
                let pixel_u32 =
                    u32::from_ne_bytes([data[0], data[1], data[2], data[3]]);
                drop(data);
                self.readback_buffer.unmap();

                self.pending_result = None;

                self.last_entity = if pixel_u32 == PICKING_SENTINEL {
                    None
                } else if (pixel_u32 as usize) < self.mapping.len() {
                    Some(self.mapping[pixel_u32 as usize])
                } else {
                    // Index out of bounds — treat as no hit.
                    None
                };
            }
            Ok(Err(_)) | Err(_) => {
                // Map submission or channel failure — clear the pending
                // request and keep last_entity as-is.
                self.pending_result = None;
            }
        }

        self.last_entity
    }

    /// Recreate the offscreen texture at a new viewport size.
    ///
    /// Must be called when the window is resized. The internal pipelines
    /// and staging buffers are unchanged; only the picking framebuffer
    /// texture is recreated.
    pub fn resize(&mut self, device: &wgpu::Device, new_size: (u32, u32)) {
        let (width, height) = (new_size.0.max(1), new_size.1.max(1));

        self.texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Picking Pass Texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Uint,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        self.view = self.texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.viewport_size = (width, height);
    }

    /// Access the current pending pick coordinates, if any.
    pub fn pending_coords(&self) -> Option<(u32, u32)> {
        self.pending_result
    }

    // ── Private: entity vertex collection ───────────────────────────────

    /// Collect all `LineData + Renderable` entities.
    fn collect_lines(
        world: &World,
        mapping: &mut Vec<hecs::Entity>,
        scratch: &mut Vec<PickingVertex>,
        cmds: &mut Vec<EntityDrawCmd>,
    ) {
        let mut query = world.query::<(&LineData, &Renderable)>();
        for (entity, (line, _)) in query.iter() {
            let entity_idx = mapping.len() as u32;
            mapping.push(entity);

            let vertex_offset = scratch.len() as u32;
            scratch.push(PickingVertex {
                position: line.start.to_f32_array(),
            });
            scratch.push(PickingVertex {
                position: line.end.to_f32_array(),
            });

            cmds.push(EntityDrawCmd {
                entity_idx,
                vertex_offset,
                vertex_count: 2,
                is_strip: false,
            });
        }
    }

    /// Collect all `CircleData + Renderable` entities.
    fn collect_circles(
        world: &World,
        zoom: f64,
        mapping: &mut Vec<hecs::Entity>,
        scratch: &mut Vec<PickingVertex>,
        cmds: &mut Vec<EntityDrawCmd>,
    ) {
        let mut query = world.query::<(&CircleData, &Renderable)>();
        for (entity, (circle, _)) in query.iter() {
            let entity_idx = mapping.len() as u32;
            mapping.push(entity);

            let vertex_offset = scratch.len() as u32;
            let cx = circle.center.x as f32;
            let cy = circle.center.y as f32;
            let r = circle.radius as f32;

            let num_segments = tessellation_segments(circle.radius, zoom);
            let step = 2.0 * PI / num_segments as f64;

            for i in 0..=num_segments {
                let theta = step * i as f64;
                scratch.push(PickingVertex {
                    position: [
                        cx + r * theta.cos() as f32,
                        cy + r * theta.sin() as f32,
                    ],
                });
            }

            cmds.push(EntityDrawCmd {
                entity_idx,
                vertex_offset,
                vertex_count: num_segments + 1,
                is_strip: true,
            });
        }
    }

    /// Collect all `ArcData + Renderable` entities.
    fn collect_arcs(
        world: &World,
        zoom: f64,
        mapping: &mut Vec<hecs::Entity>,
        scratch: &mut Vec<PickingVertex>,
        cmds: &mut Vec<EntityDrawCmd>,
    ) {
        let mut query = world.query::<(&ArcData, &Renderable)>();
        for (entity, (arc, _)) in query.iter() {
            let entity_idx = mapping.len() as u32;
            mapping.push(entity);

            let vertex_offset = scratch.len() as u32;
            let cx = arc.center.x as f32;
            let cy = arc.center.y as f32;
            let r = arc.radius as f32;

            let start_rad = arc.start_angle.to_radians();
            let end_rad = arc.end_angle.to_radians();
            let sweep = end_rad - start_rad;
            let full_segs = tessellation_segments(arc.radius, zoom);
            let sweep_fraction = (sweep.abs() / (2.0 * PI)).clamp(0.0, 1.0);
            let num_segments = ((full_segs as f64) * sweep_fraction).round().max(4.0) as u32;
            let step = sweep / num_segments as f64;

            for i in 0..=num_segments {
                let theta = start_rad + step * i as f64;
                scratch.push(PickingVertex {
                    position: [
                        cx + r * theta.cos() as f32,
                        cy + r * theta.sin() as f32,
                    ],
                });
            }

            cmds.push(EntityDrawCmd {
                entity_idx,
                vertex_offset,
                vertex_count: num_segments + 1,
                is_strip: true,
            });
        }
    }

    /// Collect all `PolylineData + Renderable` entities.
    fn collect_polylines(
        world: &World,
        mapping: &mut Vec<hecs::Entity>,
        scratch: &mut Vec<PickingVertex>,
        cmds: &mut Vec<EntityDrawCmd>,
    ) {
        let mut query = world.query::<(&PolylineData, &Renderable)>();
        for (entity, (poly, _)) in query.iter() {
            if poly.vertices.is_empty() {
                continue;
            }

            let entity_idx = mapping.len() as u32;
            mapping.push(entity);

            let vertex_offset = scratch.len() as u32;

            // Emit one vertex per point.
            for point in &poly.vertices {
                scratch.push(PickingVertex {
                    position: point.to_f32_array(),
                });
            }

            // If closed and has at least 2 vertices, emit first vertex again.
            let base_count = poly.vertices.len() as u32;
            if poly.closed && poly.vertices.len() > 1 {
                scratch.push(PickingVertex {
                    position: poly.vertices[0].to_f32_array(),
                });
            }

            cmds.push(EntityDrawCmd {
                entity_idx,
                vertex_offset,
                vertex_count: if poly.closed && poly.vertices.len() > 1 {
                    base_count + 1
                } else {
                    base_count
                },
                is_strip: false,
            });
        }
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Compute the number of tessellation segments for a circle/arc at the
/// given zoom level.
///
/// Scales with the effective screen-space radius (`radius × zoom`) so that
/// small circles use fewer vertices and large circles use more. Clamped to
/// [`MIN_SEGMENTS`, `MAX_SEGMENTS`].
fn tessellation_segments(radius: f64, zoom: f64) -> u32 {
    let effective_radius = radius * zoom;
    let segments = effective_radius.round().max(0.0) as u32;
    segments.clamp(MIN_SEGMENTS, MAX_SEGMENTS)
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── tessellation_segments ─────────────────────────────────────────

    #[test]
    fn tessellation_tiny_uses_minimum() {
        assert_eq!(tessellation_segments(0.5, 1.0), MIN_SEGMENTS);
    }

    #[test]
    fn tessellation_large_capped_at_max() {
        assert_eq!(tessellation_segments(500.0, 1.0), MAX_SEGMENTS);
    }

    #[test]
    fn tessellation_scales_with_zoom() {
        let s1 = tessellation_segments(50.0, 1.0);
        let s2 = tessellation_segments(50.0, 2.0);
        assert!(s2 >= s1, "higher zoom should produce more segments");
    }

    #[test]
    fn tessellation_zero_uses_minimum() {
        assert_eq!(tessellation_segments(0.0, 1.0), MIN_SEGMENTS);
    }

    // ── PickingVertex layout ──────────────────────────────────────────

    #[test]
    fn picking_vertex_size() {
        assert_eq!(std::mem::size_of::<PickingVertex>(), 8);
    }

    #[test]
    fn picking_vertex_alignment() {
        assert_eq!(std::mem::align_of::<PickingVertex>(), 4);
    }

    // ── collect_lines ─────────────────────────────────────────────────

    #[test]
    fn collect_lines_produces_two_vertices_per_entity() {
        let mut world = World::new();
        world.spawn((
            LineData {
                start: crate::geometry::Point2D::new(0.0, 0.0),
                end: crate::geometry::Point2D::new(10.0, 10.0),
                color: crate::util::Color::WHITE,
                width: 1.0,
            },
            Renderable,
        ));
        world.spawn((
            LineData {
                start: crate::geometry::Point2D::new(5.0, 5.0),
                end: crate::geometry::Point2D::new(15.0, 15.0),
                color: crate::util::Color::WHITE,
                width: 1.0,
            },
            Renderable,
        ));

        let mut mapping = Vec::new();
        let mut scratch = Vec::new();
        let mut cmds = Vec::new();
        PickingPass::collect_lines(&world, &mut mapping, &mut scratch, &mut cmds);

        assert_eq!(mapping.len(), 2);
        assert_eq!(scratch.len(), 4); // 2 lines × 2 vertices
        assert_eq!(cmds.len(), 2);

        for cmd in &cmds {
            assert!(!cmd.is_strip);
            assert_eq!(cmd.vertex_count, 2);
        }
    }

    // ── collect_circles ───────────────────────────────────────────────

    #[test]
    fn collect_circles_produces_closed_loop() {
        let mut world = World::new();
        world.spawn((
            CircleData {
                center: crate::geometry::Point2D::new(0.0, 0.0),
                radius: 10.0,
                color: crate::util::Color::WHITE,
                width: 1.0,
            },
            Renderable,
        ));

        let mut mapping = Vec::new();
        let mut scratch = Vec::new();
        let mut cmds = Vec::new();
        PickingPass::collect_circles(&world, 1.0, &mut mapping, &mut scratch, &mut cmds);

        assert_eq!(mapping.len(), 1);
        assert_eq!(cmds.len(), 1);
        assert!(cmds[0].is_strip);
        // Closed loop = num_segments + 1 vertices, first == last.
        assert!(cmds[0].vertex_count >= MIN_SEGMENTS + 1);
        // The closed loop may differ by tiny floating-point error; use
        // approximate comparison.
        let first = scratch[cmds[0].vertex_offset as usize].position;
        let last = scratch[(cmds[0].vertex_offset + cmds[0].vertex_count - 1) as usize].position;
        let dx = (first[0] - last[0]).abs();
        let dy = (first[1] - last[1]).abs();
        assert!(
            dx < 1e-5 && dy < 1e-5,
            "circle should form a closed loop (first {first:?} ≈ last {last:?})",
        );
    }

    // ── collect_polylines ──────────────────────────────────────────────

    #[test]
    fn collect_polylines_open_does_not_close() {
        let mut world = World::new();
        world.spawn((
            PolylineData {
                vertices: vec![
                    crate::geometry::Point2D::new(0.0, 0.0),
                    crate::geometry::Point2D::new(10.0, 0.0),
                    crate::geometry::Point2D::new(10.0, 10.0),
                ],
                closed: false,
                color: crate::util::Color::WHITE,
                width: 1.0,
            },
            Renderable,
        ));

        let mut mapping = Vec::new();
        let mut scratch = Vec::new();
        let mut cmds = Vec::new();
        PickingPass::collect_polylines(&world, &mut mapping, &mut scratch, &mut cmds);

        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0].vertex_count, 3);
        assert!(!cmds[0].is_strip);
    }

    #[test]
    fn collect_polylines_closed_adds_extra_vertex() {
        let mut world = World::new();
        world.spawn((
            PolylineData {
                vertices: vec![
                    crate::geometry::Point2D::new(0.0, 0.0),
                    crate::geometry::Point2D::new(10.0, 0.0),
                    crate::geometry::Point2D::new(10.0, 10.0),
                ],
                closed: true,
                color: crate::util::Color::WHITE,
                width: 1.0,
            },
            Renderable,
        ));

        let mut mapping = Vec::new();
        let mut scratch = Vec::new();
        let mut cmds = Vec::new();
        PickingPass::collect_polylines(&world, &mut mapping, &mut scratch, &mut cmds);

        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0].vertex_count, 4); // 3 + 1 closing
        // Last vertex should equal first vertex (within float tolerance).
        let off = cmds[0].vertex_offset as usize;
        let dx = (scratch[off].position[0] - scratch[off + 3].position[0]).abs();
        let dy = (scratch[off].position[1] - scratch[off + 3].position[1]).abs();
        assert!(dx < 1e-5 && dy < 1e-5, "closed polyline first {0:?} ≈ last {1:?}", scratch[off].position, scratch[off + 3].position);
    }

    #[test]
    fn collect_polylines_empty_skipped() {
        let mut world = World::new();
        world.spawn((
            PolylineData {
                vertices: vec![],
                closed: false,
                color: crate::util::Color::WHITE,
                width: 1.0,
            },
            Renderable,
        ));

        let mut mapping = Vec::new();
        let mut scratch = Vec::new();
        let mut cmds = Vec::new();
        PickingPass::collect_polylines(&world, &mut mapping, &mut scratch, &mut cmds);

        assert!(cmds.is_empty());
        assert!(scratch.is_empty());
        assert!(mapping.is_empty());
    }

    // ── Entity mapping invariants ─────────────────────────────────────

    #[test]
    fn mapping_contains_all_entities() {
        let mut world = World::new();
        let e1 = world.spawn((
            LineData {
                start: crate::geometry::Point2D::new(0.0, 0.0),
                end: crate::geometry::Point2D::new(1.0, 1.0),
                color: crate::util::Color::WHITE,
                width: 1.0,
            },
            Renderable,
        ));
        let e2 = world.spawn((
            CircleData {
                center: crate::geometry::Point2D::new(5.0, 5.0),
                radius: 2.0,
                color: crate::util::Color::WHITE,
                width: 1.0,
            },
            Renderable,
        ));

        let mut mapping = Vec::new();
        let mut scratch = Vec::new();
        let mut cmds = Vec::new();
        PickingPass::collect_lines(&world, &mut mapping, &mut scratch, &mut cmds);
        PickingPass::collect_circles(&world, 1.0, &mut mapping, &mut scratch, &mut cmds);

        assert_eq!(mapping.len(), 2);
        assert!(mapping.contains(&e1));
        assert!(mapping.contains(&e2));
    }
}
