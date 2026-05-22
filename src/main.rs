use std::sync::Arc;
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowId},
};

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "forge=info,wgpu=warn".into()),
        )
        .try_init()
        .ok();

    let event_loop = EventLoop::new().unwrap();
    let mut app = ForgeAppHandler::new();
    event_loop.run_app(&mut app).expect("Event loop failed");
}

#[derive(Default)]
pub struct ForgeAppHandler {
    window: Option<Arc<Window>>,
}

impl ForgeAppHandler {
    pub fn new() -> Self {
        Self::default()
    }
}

impl ApplicationHandler for ForgeAppHandler {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window_attributes = winit::window::WindowAttributes::default()
            .with_title("Forge v0.1.0")
            .with_inner_size(winit::dpi::LogicalSize::new(1280, 720));

        let window = match event_loop.create_window(window_attributes) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                tracing::error!("Failed to create window: {}", e);
                event_loop.exit();
                return;
            }
        };

        tracing::info!("Window created: Forge v0.1.0");
        self.window = Some(window);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match &event {
            WindowEvent::CloseRequested => {
                tracing::info!("Window close requested, shutting down");
                event_loop.exit();
            }
            WindowEvent::Destroyed => {
                tracing::info!("Window destroyed, exiting event loop");
                self.window = None;
                event_loop.exit();
            }
            WindowEvent::Resized(new_size) => {
                tracing::debug!("Window resized to {}x{}", new_size.width, new_size.height);
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}
