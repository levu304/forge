//! Shared helpers for reading and writing entity geometry properties.
//!
//! These functions are used by both the property resolver (for resolving
//! visual properties through the inheritance chain) and the property
//! palette (for editing and displaying entity properties).  Keeping them
//! in one place avoids duplication when a new geometry type is added.

use hecs::{Entity, World};

use crate::ecs::components::{
    ArcData, BlockRef, CircleData, LineData, PolylineData,
};
use crate::util::Color;

/// Try to read the `color` field from whichever geometry component the
/// entity carries.  Returns `None` when the entity has no recognised
/// geometry component.
pub fn read_entity_color(world: &World, entity: Entity) -> Option<Color> {
    if let Ok(data) = world.get::<&LineData>(entity) {
        Some(data.color)
    } else if let Ok(data) = world.get::<&CircleData>(entity) {
        Some(data.color)
    } else if let Ok(data) = world.get::<&ArcData>(entity) {
        Some(data.color)
    } else if let Ok(data) = world.get::<&PolylineData>(entity) {
        Some(data.color)
    } else {
        None
    }
}

/// Try to read the `width` field from whichever geometry component the
/// entity carries.  Returns `None` when the entity has no recognised
/// geometry component.
pub fn read_entity_linewidth(world: &World, entity: Entity) -> Option<f32> {
    if let Ok(data) = world.get::<&LineData>(entity) {
        Some(data.width)
    } else if let Ok(data) = world.get::<&CircleData>(entity) {
        Some(data.width)
    } else if let Ok(data) = world.get::<&ArcData>(entity) {
        Some(data.width)
    } else if let Ok(data) = world.get::<&PolylineData>(entity) {
        Some(data.width)
    } else {
        None
    }
}

/// Return a human-readable name for the entity's primary geometry type.
pub fn entity_type_name(world: &World, entity: Entity) -> String {
    if world.get::<&LineData>(entity).is_ok() {
        "Line".into()
    } else if world.get::<&CircleData>(entity).is_ok() {
        "Circle".into()
    } else if world.get::<&ArcData>(entity).is_ok() {
        "Arc".into()
    } else if world.get::<&PolylineData>(entity).is_ok() {
        "Polyline".into()
    } else if world.get::<&BlockRef>(entity).is_ok() {
        "Block Instance".into()
    } else {
        "Unknown".into()
    }
}
