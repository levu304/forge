//! Command system.
//!
//! Defines the Command trait, CommandState machine, nom-based parser,
//! and concrete command implementations (LINE, CIRCLE, ARC, PLINE).

pub mod parser;
pub mod line_cmd;
pub mod circle_cmd;
pub mod arc_cmd;
pub mod polyline_cmd;
