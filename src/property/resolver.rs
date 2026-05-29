//! Property resolution through the ByLayer / ByBlock / Explicit chain.
//!
//! [`PropertyResolver`] resolves an entity's visual properties (color,
//! linewidth, linetype) through the standard CAD inheritance chain:
//!
//! 1. **Explicit** — return the entity's own value (from its geometry
//!    component such as [`LineData`], [`CircleData`], etc.).
//! 2. **ByBlock** — if the entity has a [`BlockRef`], resolve the block
//!    definition entity's properties recursively.  If there is no
//!    [`BlockRef`], fall through to **ByLayer**.
//! 3. **ByLayer** — look up the entity's [`LayerRef`] component, fetch
//!    the corresponding layer from [`LayerTable`], and return its
//!    property.  If there is no [`LayerRef`], use [`LayerId::DEFAULT`].
//!
//! [`LineData`]: crate::ecs::components::LineData
//! [`CircleData`]: crate::ecs::components::CircleData
//! [`BlockRef`]: crate::ecs::components::BlockRef
//! [`LayerRef`]: crate::ecs::components::LayerRef
//! [`LayerId::DEFAULT`]: crate::layer::LayerId::DEFAULT

use hecs::{Entity, World};

use crate::ecs::components::{
    BlockRef, LayerRef, PropertySource,
};
use crate::layer::{LayerId, LayerTable, Linetype};
use crate::util::Color;

use super::geometry;

/// Visual properties that have been fully resolved through the
/// ByLayer / ByBlock / Explicit inheritance chain.
#[derive(Debug, Clone, Copy)]
pub struct ResolvedProperties {
    /// Resolved RGBA colour.
    pub color: Color,
    /// Resolved linewidth in drawing units.
    pub linewidth: f32,
    /// Resolved line-style pattern.
    pub linetype: Linetype,
    /// Resolved layer ID (after property-source inheritance).
    pub layer_id: LayerId,
}

/// Resolves visual properties through the CAD inheritance chain.
///
/// # Resolution order
///
/// For each property the resolver checks, in order:
/// 1. Is the entity's [`PropertySource`] `Explicit`? → use the entity's
///    own value.
/// 2. Is the entity's [`PropertySource`] `ByBlock` and does it carry a
///    [`BlockRef`]? → recurse into the block definition entity.
/// 3. Otherwise → resolve from the entity's [`LayerRef`] (or fall back
///    to layer 0 when no component exists).
#[derive(Debug, Clone, Default)]
pub struct PropertyResolver;

impl PropertyResolver {
    // ------------------------------------------------------------------
    // ECS query-based resolution
    // ------------------------------------------------------------------

    /// Resolve the entity's colour through the inheritance chain.
    pub fn resolve_color(world: &World, entity: Entity, layer_table: &LayerTable) -> Color {
        let source = entity_property_source(world, entity);

        match source {
            PropertySource::Explicit => {
                geometry::read_entity_color(world, entity)
                    .unwrap_or_else(|| resolve_color_from_layer(world, entity, layer_table))
            }
            PropertySource::ByBlock => {
                // resolve_color_from_block already returns None when there
                // is no BlockRef component, so no outer guard is needed.
                resolve_color_from_block(world, entity, layer_table)
                    .unwrap_or_else(|| resolve_color_from_layer(world, entity, layer_table))
            }
            PropertySource::ByLayer => resolve_color_from_layer(world, entity, layer_table),
        }
    }

    /// Resolve the entity's linewidth through the inheritance chain.
    pub fn resolve_linewidth(world: &World, entity: Entity, layer_table: &LayerTable) -> f32 {
        let source = entity_property_source(world, entity);

        match source {
            PropertySource::Explicit => {
                geometry::read_entity_linewidth(world, entity)
                    .unwrap_or_else(|| resolve_linewidth_from_layer(world, entity, layer_table))
            }
            PropertySource::ByBlock => {
                resolve_linewidth_from_block(world, entity, layer_table)
                    .unwrap_or_else(|| resolve_linewidth_from_layer(world, entity, layer_table))
            }
            PropertySource::ByLayer => resolve_linewidth_from_layer(world, entity, layer_table),
        }
    }

    /// Resolve the entity's linetype through the inheritance chain.
    ///
    /// Linetype is not stored on geometry components, so linetype always
    /// resolves from the entity's layer in v0.3.0 (the match on source
    /// would be three identical arms, so we skip it).
    pub fn resolve_linetype(world: &World, entity: Entity, layer_table: &LayerTable) -> Linetype {
        resolve_linetype_from_layer(world, entity, layer_table)
    }

    /// Resolve all three visual properties at once.
    pub fn resolve_all(world: &World, entity: Entity, layer_table: &LayerTable) -> ResolvedProperties {
        ResolvedProperties {
            color: Self::resolve_color(world, entity, layer_table),
            linewidth: Self::resolve_linewidth(world, entity, layer_table),
            linetype: Self::resolve_linetype(world, entity, layer_table),
            layer_id: entity_layer_id(world, entity),
        }
    }

    // ------------------------------------------------------------------
    // Direct resolution (for BlockEntity — no geometry components)
    // ------------------------------------------------------------------
    //
    // These methods accept pre-extracted field values and resolve
    // through the inheritance chain without querying the ECS world for
    // geometry components.  When the source is `Explicit` the caller
    // must pass the explicit value; for `ByBlock` the caller passes the
    // block definition's own resolved value (or the method falls through
    // to `ByLayer` when the value would require recursion).

    /// Resolve colour from pre-extracted values.
    ///
    /// `explicit` is used when `source` is [`PropertySource::Explicit`].
    /// `layer_ref` is the entity's [`LayerRef`] component value (0‑based
    /// layer index), or `None` if absent.
    pub fn resolve_color_direct(
        source: PropertySource,
        explicit: Color,
        layer_ref: Option<u32>,
        layer_table: &LayerTable,
    ) -> Color {
        match source {
            PropertySource::Explicit => explicit,
            // ByBlock falls through to ByLayer in v0.3.0 (block definitions
            // cannot yet carry independent colour).
            PropertySource::ByBlock | PropertySource::ByLayer => {
                resolve_layer_color(layer_ref, layer_table)
            }
        }
    }

    /// Resolve linewidth from pre-extracted values.
    pub fn resolve_linewidth_direct(
        source: PropertySource,
        explicit: f32,
        layer_ref: Option<u32>,
        layer_table: &LayerTable,
    ) -> f32 {
        match source {
            PropertySource::Explicit => explicit,
            PropertySource::ByBlock | PropertySource::ByLayer => {
                resolve_layer_linewidth(layer_ref, layer_table)
            }
        }
    }

    /// Resolve linetype from pre-extracted values.
    ///
    /// Linetype is only stored at the layer level in v0.3.0, so source is
    /// irrelevant — the value always comes from the entity's layer.
    pub fn resolve_linetype_direct(
        layer_ref: Option<u32>,
        layer_table: &LayerTable,
    ) -> Linetype {
        resolve_layer_linetype(layer_ref, layer_table)
    }
}

// ---------------------------------------------------------------------------
// Internal helpers — ByLayer resolution
// ---------------------------------------------------------------------------

/// Resolve colour from the entity's layer.
fn resolve_color_from_layer(world: &World, entity: Entity, layer_table: &LayerTable) -> Color {
    let layer_id = entity_layer_id(world, entity);
    layer_table
        .get(layer_id)
        .map_or(Color::WHITE, |l| l.color)
}

/// Resolve linewidth from the entity's layer.
fn resolve_linewidth_from_layer(world: &World, entity: Entity, layer_table: &LayerTable) -> f32 {
    let layer_id = entity_layer_id(world, entity);
    layer_table
        .get(layer_id)
        .map_or(0.25, |l| l.linewidth)
}

/// Resolve linetype from the entity's layer.
fn resolve_linetype_from_layer(world: &World, entity: Entity, layer_table: &LayerTable) -> Linetype {
    let layer_id = entity_layer_id(world, entity);
    layer_table
        .get(layer_id)
        .map_or(Linetype::Solid, |l| l.linetype)
}

/// Determine the layer ID for an entity.
///
/// Returns the entity's [`LayerRef`] value, or [`LayerId::DEFAULT`] if
/// the entity has no such component.
fn entity_layer_id(world: &World, entity: Entity) -> LayerId {
    world
        .get::<&LayerRef>(entity)
        .map(|lr| LayerId(lr.0))
        .unwrap_or(LayerId::DEFAULT)
}

/// Read the entity's property source, defaulting to [`PropertySource::ByLayer`].
fn entity_property_source(world: &World, entity: Entity) -> PropertySource {
    world
        .get::<&PropertySource>(entity)
        .ok()
        .map(|r| *r)
        .unwrap_or(PropertySource::ByLayer)
}

// ---------------------------------------------------------------------------
// Internal helpers — direct ByLayer resolution
// ---------------------------------------------------------------------------

fn resolve_layer_color(layer_ref: Option<u32>, layer_table: &LayerTable) -> Color {
    let id = layer_ref.map_or(LayerId::DEFAULT, LayerId);
    layer_table.get(id).map(|l| l.color).unwrap_or(Color::WHITE)
}

fn resolve_layer_linewidth(layer_ref: Option<u32>, layer_table: &LayerTable) -> f32 {
    let id = layer_ref.map_or(LayerId::DEFAULT, LayerId);
    layer_table
        .get(id)
        .map_or(0.25, |l| l.linewidth)
}

fn resolve_layer_linetype(layer_ref: Option<u32>, layer_table: &LayerTable) -> Linetype {
    let id = layer_ref.map_or(LayerId::DEFAULT, LayerId);
    layer_table
        .get(id)
        .map_or(Linetype::Solid, |l| l.linetype)
}

// ---------------------------------------------------------------------------
// Internal helpers — ByBlock resolution
// ---------------------------------------------------------------------------

/// Resolve colour through a block definition entity.
///
/// Scans all entities sharing the same [`BlockRef::definition`] ID.
/// The definition entity is identified by having
/// [`PropertySource::ByLayer`] (block definitions use ByLayer, instances
/// use ByBlock).  If the block cannot be found, falls back to `None` so
/// the caller can decide the fallback behaviour.
fn resolve_color_from_block(world: &World, entity: Entity, layer_table: &LayerTable) -> Option<Color> {
    let block_ref = world.get::<&BlockRef>(entity).ok()?;
    let def_id = block_ref.definition;

    for (e, br) in world.query::<&BlockRef>().iter() {
        if br.definition == def_id && e != entity {
            // Block definitions carry PropertySource::ByLayer.
            if *world.get::<&PropertySource>(e).ok()? == PropertySource::ByLayer {
                return Some(PropertyResolver::resolve_color(world, e, layer_table));
            }
        }
    }

    None
}

/// Resolve linewidth through a block definition entity.
///
/// Same matching logic as [`resolve_color_from_block`] — requires
/// [`PropertySource::ByLayer`] on the candidate definition entity.
fn resolve_linewidth_from_block(world: &World, entity: Entity, layer_table: &LayerTable) -> Option<f32> {
    let block_ref = world.get::<&BlockRef>(entity).ok()?;
    let def_id = block_ref.definition;

    for (e, br) in world.query::<&BlockRef>().iter() {
        if br.definition == def_id && e != entity {
            if *world.get::<&PropertySource>(e).ok()? == PropertySource::ByLayer {
                return Some(PropertyResolver::resolve_linewidth(world, e, layer_table));
            }
        }
    }

    None
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use hecs::World;

    use super::*;
    use crate::ecs::components::{BlockRef, LayerRef, LineData, PropertySource};
    use crate::geometry::Point2D;
    use crate::layer::{LayerId, LayerTable, Linetype};

    /// Helper: create a [`LayerTable`] with layers 0 (default, white) and
    /// 1 (red, 1.0 linewidth, dashed).
    fn make_table() -> LayerTable {
        let mut table = LayerTable::new();
        let id = table.insert("geometry").unwrap();
        // Tweak layer-0 defaults
        let def_layer = table.get_mut(LayerId::DEFAULT).unwrap();
        def_layer.color = Color::WHITE;
        def_layer.linewidth = 0.25;
        def_layer.linetype = Linetype::Solid;
        // Tweak layer 1
        let geo = table.get_mut(id).unwrap();
        geo.color = Color::from_hex(0xFF0000);
        geo.linewidth = 1.0;
        geo.linetype = Linetype::Dashed;
        table
    }

    /// Spawn a line entity with all three property components.
    fn spawn_line(
        world: &mut World,
        source: PropertySource,
        layer: u32,
        block: Option<u32>,
        color: Color,
        width: f32,
    ) -> Entity {
        let ent = world.spawn((
            LineData {
                start: Point2D::new(0.0, 0.0),
                end: Point2D::new(10.0, 10.0),
                color,
                width,
            },
            source,
            LayerRef(layer),
        ));
        if let Some(def) = block {
            world.insert_one(ent, BlockRef { definition: def }).ok();
        }
        ent
    }

    // ------------------------------------------------------------------
    // ByLayer resolution
    // ------------------------------------------------------------------

    #[test]
    fn resolve_color_by_layer_default() {
        let mut world = World::new();
        let table = make_table();
        let entity = world.spawn((LayerRef(0), PropertySource::ByLayer));
        let color = PropertyResolver::resolve_color(&world, entity, &table);
        assert_eq!(color, Color::WHITE);
    }

    #[test]
    fn resolve_color_by_layer_explicit_layer() {
        let mut world = World::new();
        let table = make_table();
        let entity = world.spawn((LayerRef(1), PropertySource::ByLayer));
        let color = PropertyResolver::resolve_color(&world, entity, &table);
        // Layer 1 is red (0xFF0000)
        assert_eq!(color, Color::from_hex(0xFF0000));
    }

    #[test]
    fn resolve_linewidth_by_layer() {
        let mut world = World::new();
        let table = make_table();
        let entity = world.spawn((LayerRef(1), PropertySource::ByLayer));
        let w = PropertyResolver::resolve_linewidth(&world, entity, &table);
        assert!((w - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn resolve_linetype_by_layer() {
        let mut world = World::new();
        let table = make_table();
        let entity = world.spawn((LayerRef(1), PropertySource::ByLayer));
        let lt = PropertyResolver::resolve_linetype(&world, entity, &table);
        assert_eq!(lt, Linetype::Dashed);
    }

    // ------------------------------------------------------------------
    // Explicit resolution
    // ------------------------------------------------------------------

    #[test]
    fn resolve_color_explicit() {
        let mut world = World::new();
        let table = make_table();
        let entity = spawn_line(
            &mut world,
            PropertySource::Explicit,
            0,
            None,
            Color::from_hex(0x00FF00),
            2.0,
        );
        let color = PropertyResolver::resolve_color(&world, entity, &table);
        assert_eq!(color, Color::from_hex(0x00FF00));
    }

    #[test]
    fn resolve_linewidth_explicit() {
        let mut world = World::new();
        let table = make_table();
        let entity = spawn_line(
            &mut world,
            PropertySource::Explicit,
            0,
            None,
            Color::WHITE,
            3.5,
        );
        let w = PropertyResolver::resolve_linewidth(&world, entity, &table);
        assert!((w - 3.5).abs() < f32::EPSILON);
    }

    // ------------------------------------------------------------------
    // ByBlock resolution
    // ------------------------------------------------------------------

    #[test]
    fn resolve_color_by_block_without_block_ref_falls_to_layer() {
        let mut world = World::new();
        let table = make_table();
        // Entity has ByBlock but NO BlockRef component → falls to ByLayer
        let entity = world.spawn((LayerRef(1), PropertySource::ByBlock));
        let color = PropertyResolver::resolve_color(&world, entity, &table);
        assert_eq!(color, Color::from_hex(0xFF0000), "ByBlock without BlockRef should fall to layer");
    }

    #[test]
    fn resolve_color_by_block_with_block_ref() {
        let mut world = World::new();
        let table = make_table();

        // Create a block definition entity (ByLayer, layer-1 colour)
        let _def = world.spawn((
            LineData {
                start: Point2D::new(0.0, 0.0),
                end: Point2D::new(1.0, 1.0),
                color: Color::from_hex(0x0000FF),
                width: 0.5,
            },
            PropertySource::ByLayer,
            LayerRef(1),
            BlockRef { definition: 42 },
        ));

        // Create an instance entity pointing to definition 42
        let instance = world.spawn((
            PropertySource::ByBlock,
            LayerRef(0),
            BlockRef { definition: 42 },
        ));

        let color = PropertyResolver::resolve_color(&world, instance, &table);
        // The block definition has ByLayer on layer 1 which is red (0xFF0000)
        assert_eq!(color, Color::from_hex(0xFF0000));
    }

    // ------------------------------------------------------------------
    // Default layer when no LayerRef
    // ------------------------------------------------------------------

    #[test]
    fn resolve_color_default_layer_when_no_layer_ref() {
        let mut world = World::new();
        let table = make_table();
        // Entity with no LayerRef component → uses LayerId::DEFAULT (0)
        let entity = world.spawn((PropertySource::ByLayer,));
        let color = PropertyResolver::resolve_color(&world, entity, &table);
        assert_eq!(color, Color::WHITE);
    }

    // ------------------------------------------------------------------
    // resolve_all convenience
    // ------------------------------------------------------------------

    #[test]
    fn resolve_all_returns_correct_struct() {
        let mut world = World::new();
        let table = make_table();
        let entity = world.spawn((LayerRef(1), PropertySource::ByLayer));
        let resolved = PropertyResolver::resolve_all(&world, entity, &table);
        assert_eq!(resolved.color, Color::from_hex(0xFF0000));
        assert!((resolved.linewidth - 1.0).abs() < f32::EPSILON);
        assert_eq!(resolved.linetype, Linetype::Dashed);
        assert_eq!(resolved.layer_id, LayerId(1));
    }

    #[test]
    fn resolve_all_no_layer_ref_defaults_to_layer_0() {
        let mut world = World::new();
        let table = make_table();
        let entity = world.spawn((PropertySource::ByLayer,));
        let resolved = PropertyResolver::resolve_all(&world, entity, &table);
        assert_eq!(resolved.layer_id, LayerId::DEFAULT);
    }

    // ------------------------------------------------------------------
    // Direct methods
    // ------------------------------------------------------------------

    #[test]
    fn resolve_color_direct_explicit() {
        let table = make_table();
        let color = PropertyResolver::resolve_color_direct(
            PropertySource::Explicit,
            Color::from_hex(0xFF00FF),
            Some(0),
            &table,
        );
        assert_eq!(color, Color::from_hex(0xFF00FF));
    }

    #[test]
    fn resolve_color_direct_by_layer() {
        let table = make_table();
        let color = PropertyResolver::resolve_color_direct(
            PropertySource::ByLayer,
            Color::WHITE,
            Some(1),
            &table,
        );
        assert_eq!(color, Color::from_hex(0xFF0000));
    }

    #[test]
    fn resolve_linewidth_direct_by_layer() {
        let table = make_table();
        let w = PropertyResolver::resolve_linewidth_direct(
            PropertySource::ByLayer,
            0.0,
            Some(1),
            &table,
        );
        assert!((w - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn resolve_linetype_direct_by_layer() {
        let table = make_table();
        let lt = PropertyResolver::resolve_linetype_direct(Some(1), &table);
        assert_eq!(lt, Linetype::Dashed);
    }
}
