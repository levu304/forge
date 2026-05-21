//! LINE command implementation.
//!
//! Uses a buffer-then-commit design: points are accumulated before
//! spawning entities into the ECS on completion.
