use thiserror::Error;

/// Application-level error type for Forge.
///
/// # Error Handling Strategy (v0.1.0)
/// In v0.1.0, most fallible operations use `.expect()` or `.unwrap()` for
/// simplicity during early development (startup, GPU init, etc.). Proper
/// error recovery with user-facing messages is deferred to v0.2.0+.
/// This enum exists to define the error surface and will be wired into
/// the command system and render pipeline in the next iteration.
#[derive(Error, Debug)]
pub enum ForgeError {
    #[error("GPU error: {0}")]
    Gpu(String),
    #[error("Surface error: {0}")]
    Surface(String),
    #[error("Command error: {0}")]
    Command(String), // Reserved: command errors surface via CommandState::last_error in v0.1.0
    #[error("Parse error: {0}")]
    Parse(String), // Currently unused — nom errors surface through CommandState::last_error.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

impl From<wgpu::Error> for ForgeError {
    fn from(e: wgpu::Error) -> Self {
        ForgeError::Gpu(e.to_string())
    }
}

// Note: wgpu 29 changed SurfaceError handling - get_current_texture() returns CurrentSurfaceTexture
// instead of Result<SurfaceTexture, SurfaceError>. We'll handle surface errors differently in the render code.