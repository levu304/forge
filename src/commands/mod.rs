//! Command system.
//!
//! Defines the Command trait, CommandState machine, nom-based parser,
//! and concrete command implementations (LINE, CIRCLE, ARC, PLINE).

use crate::geometry::Point2D;
use crate::util::Color;
use hecs::World;

/// Lightweight descriptor for preview geometry drawn during command
/// execution (e.g., rubber-band line, temporary circle outline).
pub struct PreviewEntity {
    pub points: Vec<Point2D>,
    pub color: Color,
    pub width: f32,
}

/// A command is an interactive operation with multiple steps.
/// Each step prompts the user for input (point, distance, angle, etc.).
///
/// NOTE: Send + Sync not required in v0.1.0 (single-threaded event loop).
/// Add bounds if commands are dispatched across threads in v0.2.0+.
pub trait Command {
    /// Command name used in CLI (e.g., "LINE", "CIRCLE").
    fn name(&self) -> &'static str;
    /// Human-readable prompt for current step.
    fn prompt(&self) -> String;
    /// Number of steps remaining (0 = complete).
    fn steps_remaining(&self) -> usize;
    /// Process a user input event (point pick, text entry, etc.).
    fn on_input(&mut self, input: CommandInput, world: &mut World) -> CommandResult;
    /// Cancel the command, cleaning up any preview entities.
    fn on_cancel(&mut self, world: &mut World);
    /// Render preview geometry (e.g., rubber-band line).
    fn preview(&self) -> Vec<PreviewEntity>;
}

pub enum CommandInput {
    Point(Point2D),           // Mouse click or typed coordinate
    Text(String),             // Command-line text entry
    #[allow(dead_code)]
    Distance(f64),            // Typed distance value (reserved: CIRCLE radius, etc.)
    #[allow(dead_code)]
    Angle(f64),               // Typed angle in degrees (reserved: ARC rotation)
    Cancel,                   // Escape key
    Confirm,                  // Enter key
}

pub enum CommandResult {
    Continue,                 // Await next input
    Complete,                 // Command finished successfully
    Error(String),            // Invalid input, show error, continue
    Cancelled,                // User cancelled
}

/// Manages the active command and command history.
#[derive(Default)]
pub struct CommandState {
    pub active: Option<Box<dyn Command>>,
    pub history: Vec<String>, // Previous command strings
    pub buffer: String,       // Current command-line text
    pub last_error: Option<String>, // Most recent command error (displayed in UI)
    pub pending_dispatch: Option<String>, // Text waiting to be dispatched from UI command line

    /// Set to `true` by the UI when Escape is pressed with an active command.
    /// Consumed by the event-loop layer (which has `&mut World`) to call
    /// [`Command::on_cancel`] on the active command.
    pub cancel_requested: bool,
}

impl CommandState {
    /// If `cancel_requested` is set, calls `on_cancel` on the active command
    /// (if any) and resets state. Should be called by the event-loop handler
    /// at the start of each frame, where `World` is available mutably.
    pub fn process_pending_cancel(&mut self, world: &mut World) {
        if self.cancel_requested {
            self.cancel_requested = false;
            if let Some(ref mut cmd) = self.active {
                cmd.on_cancel(world);
            }
            self.active = None;
            self.last_error = None;
        }
    }
}

pub mod parser;
pub mod line_cmd;
pub mod circle_cmd;
pub mod arc_cmd;
pub mod polyline_cmd;
pub mod erase_cmd;
pub mod move_cmd;
pub mod copy_cmd;
pub mod rotate_cmd;
pub mod scale_cmd;
pub mod mirror_cmd;
pub mod offset_cmd;
