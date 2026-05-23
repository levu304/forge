//! wgpu render pipeline.
//!
//! Manages GPU state: surface, device, queue, shaders, and render passes.
//! Pipeline order per frame: clear → grid → entities → UI overlay.

pub mod camera;
pub use camera::OrthographicCamera;
pub mod grid;
pub use grid::GridRenderer;
pub mod entity_renderer;
pub mod shaders;
pub mod pipeline; // reserved — v0.2.0+ cleanup

use std::sync::Arc;

use winit::dpi::PhysicalSize;

use crate::util::ForgeError;

// ─── RenderState ────────────────────────────────────────────────────────────

/// Central GPU state for the Forge viewport.
///
/// Owns the wgpu instance, surface, device, queue, and all render-related
/// resources (pipelines, buffers, bind groups). Provides the `new` async
/// constructor for initialisation and `resize` for surface reconfiguration.
pub struct RenderState {
    /// Retained for device enumeration and re-adaptation in v0.2.0+.
    pub instance: wgpu::Instance,
    /// The window surface onto which frames are presented.
    pub surface: wgpu::Surface<'static>,
    /// The GPU device used for all resource creation.
    pub device: wgpu::Device,
    /// The command queue for submitting render work.
    pub queue: wgpu::Queue,
    /// Current surface configuration (width, height, format, present mode).
    pub config: wgpu::SurfaceConfiguration,
    /// Layout for the camera uniform bind group (shared across all shaders).
    pub camera_bind_group_layout: wgpu::BindGroupLayout,
    /// GPU buffer holding the view-projection matrix (64 bytes).
    pub camera_buffer: wgpu::Buffer,
    /// Bind group referencing the camera uniform buffer at group(0), binding(0).
    pub camera_bind_group: wgpu::BindGroup,
    /// Renders the background grid (major/minor lines, axes).
    pub grid_renderer: GridRenderer,
    /// Renders ECS geometry entities (Line, Circle, Arc, Polyline).
    pub entity_renderer: EntityRenderer,
    /// egui-wgpu renderer for UI overlay rendering.
    pub egui_renderer: egui_wgpu::Renderer,
    /// The pixel format of the surface (e.g. `Bgra8Unorm`).
    pub surface_format: wgpu::TextureFormat,
}

impl RenderState {
    /// Create a new `RenderState`, initialising the GPU device and all
    /// render resources.
    ///
    /// This is an async method because adapter/device requests involve
    /// (possibly asynchronous) backend enumeration.
    ///
    /// # Errors
    ///
    /// Returns `ForgeError::Gpu` if surface creation, adapter request, or
    /// device request fails.
    pub async fn new(window: Arc<winit::window::Window>) -> Result<Self, ForgeError> {
        // -- Instance -----------------------------------------------------------
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            flags: wgpu::InstanceFlags::default(),
            memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
            backend_options: wgpu::BackendOptions::default(),
            display: None,
        });

        // -- Surface ------------------------------------------------------------
        let surface = instance
            .create_surface(window.clone())
            .map_err(|e| ForgeError::Gpu(e.to_string()))?;

        // -- Adapter ------------------------------------------------------------
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .map_err(|e| ForgeError::Gpu(e.to_string()))?;

        // -- Device & Queue -----------------------------------------------------
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("Forge Device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                ..Default::default()
            })
            .await
            .map_err(|e| ForgeError::Gpu(e.to_string()))?;

        // -- Surface config -----------------------------------------------------
        let size = window.inner_size();
        let caps = surface.get_capabilities(&adapter);
        let surface_format = caps
            .formats
            .first()
            .copied()
            .ok_or_else(|| ForgeError::Gpu("No compatible surface texture formats".into()))?;

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: wgpu::CompositeAlphaMode::Opaque,
            view_formats: vec![],
        };
        surface.configure(&device, &config);

        // -- Camera uniform buffer (64 bytes = 16 floats for mat4x4<f32>) --------
        let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Camera Uniform Buffer"),
            size: 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // -- Camera bind group layout & bind group ------------------------------
        let camera_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Camera Bind Group Layout"),
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

        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Camera Bind Group"),
            layout: &camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });

        // -- egui renderer ------------------------------------------------------
        let egui_renderer = egui_wgpu::Renderer::new(
            &device,
            surface_format,
            egui_wgpu::RendererOptions::default(),
        );

        // -- Grid renderer -------------------------------------------------------
        let grid_renderer = GridRenderer::new(&device, &camera_bind_group_layout, surface_format);

        // -- Entity renderer (stub) --------------------------------------------
        let entity_renderer =
            EntityRenderer::new(&device, &camera_bind_group_layout, surface_format);

        Ok(Self {
            instance,
            surface,
            device,
            queue,
            config,
            camera_bind_group_layout,
            camera_buffer,
            camera_bind_group,
            grid_renderer,
            entity_renderer,
            egui_renderer,
            surface_format,
        })
    }

    /// Handle window resize by reconfiguring the surface.
    ///
    /// Updates the surface configuration width/height if both dimensions
    /// are positive, then calls `surface.configure` to apply the new config.
    pub fn resize(&mut self, new_size: PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
        }
    }
}

// ─── EntityRenderer ───────────────────────────────────────────────────────

/// Renders ECS geometry entities (Line, Circle, Arc, Polyline).
///
/// Each entity type has its own render pipeline and staging buffer.
/// All entities of the same type are batched into a single vertex buffer
/// per frame to minimise draw-call overhead. For v0.1.0, the `new()` and
/// `render()` methods are stubs.
pub struct EntityRenderer {
    /// Render pipeline for line entities (`LineList` topology).
    pub line_pipeline: wgpu::RenderPipeline,
    /// Render pipeline for circle entities (`LineStrip` topology).
    pub circle_pipeline: wgpu::RenderPipeline,
    /// Render pipeline for arc entities (`LineStrip` topology).
    pub arc_pipeline: wgpu::RenderPipeline,
    /// Render pipeline for polyline entities (`LineStrip` or `LineList`).
    pub polyline_pipeline: wgpu::RenderPipeline,
    /// Staging buffer for line vertices.
    pub line_staging: wgpu::Buffer,
    /// Staging buffer for circle vertices.
    pub circle_staging: wgpu::Buffer,
    /// Staging buffer for arc vertices.
    pub arc_staging: wgpu::Buffer,
    /// Staging buffer for polyline vertices.
    pub polyline_staging: wgpu::Buffer,
    /// Capacity (in bytes) of the line staging buffer.
    pub line_staging_capacity: u64,
    /// Capacity (in bytes) of the circle staging buffer.
    pub circle_staging_capacity: u64,
    /// Capacity (in bytes) of the arc staging buffer.
    pub arc_staging_capacity: u64,
    /// Capacity (in bytes) of the polyline staging buffer.
    pub polyline_staging_capacity: u64,
}

impl EntityRenderer {
    /// Initial staging buffer size in bytes (~1365 vertices at 24 bytes each).
    /// Unused while `new()` is a stub; will be used in follow-up implementation.
    #[allow(dead_code)]
    const INITIAL_STAGING_SIZE: u64 = 32768;

    /// Create a new `EntityRenderer`.
    ///
    /// # Stub
    ///
    /// This method is a stub for v0.1.0. It creates minimal pipelines and
    /// staging buffers so the struct compiles. Full entity batching and
    /// rendering is implemented in a follow-up step.
    ///
    /// # Arguments
    /// * `camera_bind_group_layout` — Used to create the pipeline layout so
    ///   entity shaders can bind the camera uniform at group(0), binding(0).
    #[allow(unused_variables)]
    pub fn new(
        device: &wgpu::Device,
        camera_bind_group_layout: &wgpu::BindGroupLayout,
        surface_format: wgpu::TextureFormat,
    ) -> Self {
        todo!("Implement in follow-up step")
    }

    /// Render all entities from the ECS world.
    ///
    /// Queries the `hecs::World` for `Renderable` entities, generates
    /// vertex data for each entity, writes it to the appropriate staging
    /// buffer, and issues draw calls.
    ///
    /// # Stub
    ///
    /// This method is a stub for v0.1.0 and will be implemented
    /// in a follow-up step.
    #[allow(unused_variables)]
    pub fn render(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        world: &hecs::World,
        camera_bind_group: &wgpu::BindGroup,
    ) {
        todo!("Implement in follow-up step")
    }
}
