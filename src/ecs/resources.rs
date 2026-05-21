//! ECS singleton resources.
//!
//! Defines CameraState, GridConfig, and InputState — data that has
//! at-most-one instance in the world and is accessed via
//! `world.get::<T>()` / `world.insert(T)` in hecs.
