//! Property palette egui side panel.
//!
//! The [`PropertyPalette`] shows a read/write property editor as a
//! right-hand side panel.  It is only active when exactly one entity
//! is selected.  Every mutation captures the old state, applies the
//! change, and pushes a [`Transaction`] to the undo history.

use hecs::{Entity, World};

use crate::ecs::components::{
    ArcData, CircleData, LayerRef, LineData, PolylineData, PropertySource,
};
use crate::history::{AtomicOp, History, Transaction};
use crate::layer::{LayerId, LayerTable};
use crate::selection::SelectionManager;
use crate::util::Color;

use super::geometry;
use super::resolver::PropertyResolver;

/// A side panel widget that shows and edits the visual properties of the
/// currently selected entity.
///
/// # Selection-dependent behaviour
///
/// | Count | Behaviour |
/// |-------|-----------|
/// | 0     | "No entity selected" |
/// | 1     | Property editor (layer, colour, linewidth) |
/// | ≥2    | "N entities selected" (read-only) |
///
/// Every UI mutation records an undo transaction.
///
/// # Panel integration
///
/// Callers should wrap this inside an
/// [`egui::Panel::right(...).show_inside(ui, |ui| ...)`] — the palette
/// does **not** create its own panel frame (that is the caller's
/// responsibility).
#[derive(Debug, Clone, Default)]
pub struct PropertyPalette;

impl PropertyPalette {
    /// Draw the property palette content inside an existing egui [`Ui`].
    ///
    /// `world` is accessed for both reads (current component values)
    /// and writes (applying edits).  Mutations are recorded in `history`
    /// as undoable transactions.
    pub fn draw(
        &self,
        ui: &mut egui::Ui,
        world: &mut World,
        selection: &SelectionManager,
        layer_table: &LayerTable,
        history: &mut History,
    ) {
        let count = selection.count();
                if count == 0 {
                    ui.vertical_centered(|ui| {
                        ui.add_space(12.0);
                        ui.label("No entity selected");
                    });
                    return;
                }
                if count > 1 {
                    ui.vertical_centered(|ui| {
                        ui.add_space(12.0);
                        ui.label(format!("{} entities selected", count));
                    });
                    return;
                }

                let entity = match selection.primary {
                    Some(e) => e,
                    None => return,
                };

                // ---------------------------------------------------------
                // Read phase — capture all current state
                // ---------------------------------------------------------
                let old_source = world
                    .get::<&PropertySource>(entity)
                    .map(|r| *r)
                    .unwrap_or(PropertySource::ByLayer);
                let old_layer_u32 = world.get::<&LayerRef>(entity).ok().map(|lr| lr.0);
                let old_color = geometry::read_entity_color(world, entity).unwrap_or(Color::WHITE);
                let old_width = geometry::read_entity_linewidth(world, entity).unwrap_or(0.25);

                // Resolved values for display
                let resolved = PropertyResolver::resolve_all(world, entity, layer_table);

                // ---------------------------------------------------------
                // UI phase — egui controls that modify local variables
                // ---------------------------------------------------------
                let mut new_layer_u32 = old_layer_u32.unwrap_or(0);
                let mut egui_color = egui::Color32::from(old_color);
                let mut new_width = old_width;
                let mut color_changed = false;
                let mut width_changed = false;

                ui.heading("Properties");
                ui.separator();

                // -- Entity type --
                ui.label(format!("Entity: {}", geometry::entity_type_name(world, entity)));

                // -- Source label --
                let source_str = match old_source {
                    PropertySource::ByLayer => "ByLayer",
                    PropertySource::ByBlock => "ByBlock",
                    PropertySource::Explicit => "Explicit",
                };
                ui.label(format!("Source: {}", source_str));
                ui.separator();

                // -- Layer --
                ui.horizontal(|ui| {
                    ui.label("Layer:");
                    let combo = egui::ComboBox::from_id_source("property_layer")
                        .selected_text(
                            layer_table
                                .get(LayerId(new_layer_u32))
                                .map(|l| l.name().to_owned())
                                .unwrap_or_else(|| new_layer_u32.to_string()),
                        );
                    combo.show_ui(ui, |ui| {
                        for l in layer_table.iter() {
                            let lid = l.id().0;
                            let name = l.name().to_owned();
                            let lbl = format!("{} — {}", lid, name);
                            ui.selectable_value(&mut new_layer_u32, lid, lbl);
                        }
                    });
                });
                ui.label(format!(
                    "Resolved layer: {}",
                    layer_table
                        .get(LayerId(old_layer_u32.unwrap_or(0)))
                        .map(|l| l.name().to_owned())
                        .unwrap_or_else(|| "—".to_owned())
                ));
                ui.separator();

                // -- Color --
                ui.horizontal(|ui| {
                    ui.label("Color:");
                    if ui.color_edit_button_srgba(&mut egui_color).changed() {
                        color_changed = true;
                    }
                });
                ui.label(format!(
                    "Resolved: #{:02X}{:02X}{:02X}",
                    (resolved.color.r * 255.0).round() as u8,
                    (resolved.color.g * 255.0).round() as u8,
                    (resolved.color.b * 255.0).round() as u8,
                ));
                ui.separator();

                // -- Linewidth --
                ui.horizontal(|ui| {
                    ui.label("Width:");
                    if ui
                        .add(egui::Slider::new(&mut new_width, 0.0..=5.0))
                        .changed()
                    {
                        width_changed = true;
                    }
                });
                ui.label(format!("Resolved: {:.2}", resolved.linewidth));
                ui.separator();

                // ---------------------------------------------------------
                // Write phase — apply mutations and record history
                // ---------------------------------------------------------
                let layer_changed = new_layer_u32 != old_layer_u32.unwrap_or(0);
                if color_changed || width_changed || layer_changed {
                    let mut tx = Transaction::new("Modify Properties");

                    // Layer reference
                    if layer_changed {
                        let old_lr = old_layer_u32.map(LayerRef);
                        let new_lr = LayerRef(new_layer_u32);
                        let _ = world.insert_one(entity, new_lr);
                        tx.push(AtomicOp::SetLayerRef {
                            entity,
                            old: old_lr,
                            new: Some(new_lr),
                        });
                    }

                    // Colour / width (explicit override)
                    if color_changed || width_changed {
                        let new_color = Color::from(egui_color);

                        // Promote source to Explicit if needed
                        if old_source != PropertySource::Explicit {
                            let _ = world.insert_one(entity, PropertySource::Explicit);
                            tx.push(AtomicOp::SetPropertySource {
                                entity,
                                old: old_source,
                                new: PropertySource::Explicit,
                            });
                        }

                        // Update the geometry component
                        if let Some(op) = set_entity_color_width(world, entity, new_color, new_width) {
                            tx.push(op);
                        }
                    }

                    if !tx.is_empty() {
                        history.push(tx);
                    }
                }
    }
}

// ---------------------------------------------------------------------------
// Internal helpers — writing geometry properties
// ---------------------------------------------------------------------------

/// Update colour **and** width on an entity's geometry component and return
/// an [`AtomicOp`] that can undo the change.
///
/// Returns `None` if the entity has no recognised geometry component.
///
/// # Borrow-checker safety
///
/// Each branch eagerly reads the old value via a chained `.ok().map()`.  The
/// [`hecs::Ref`] temporary is consumed inside the `map` closure so there is
/// no outstanding immutable borrow when [`World::insert_one`] is called.
fn set_entity_color_width(
    world: &mut World,
    entity: Entity,
    new_color: Color,
    new_width: f32,
) -> Option<AtomicOp> {
    // LineData (Copy)
    if let Some(old) = world.get::<&LineData>(entity).ok().map(|r| *r) {
        let new = LineData {
            color: new_color,
            width: new_width,
            ..old
        };
        let _ = world.insert_one(entity, new);
        return Some(AtomicOp::SetLineData { entity, old, new });
    }
    // CircleData (Copy)
    if let Some(old) = world.get::<&CircleData>(entity).ok().map(|r| *r) {
        let new = CircleData {
            color: new_color,
            width: new_width,
            ..old
        };
        let _ = world.insert_one(entity, new);
        return Some(AtomicOp::SetCircleData { entity, old, new });
    }
    // ArcData (Copy)
    if let Some(old) = world.get::<&ArcData>(entity).ok().map(|r| *r) {
        let new = ArcData {
            color: new_color,
            width: new_width,
            ..old
        };
        let _ = world.insert_one(entity, new);
        return Some(AtomicOp::SetArcData { entity, old, new });
    }
    // PolylineData (Clone — not Copy)
    if let Some(old) = world.get::<&PolylineData>(entity).ok().map(|r| (&*r).clone()) {
        let new = PolylineData {
            color: new_color,
            width: new_width,
            ..old.clone()
        };
        let _ = world.insert_one(entity, new.clone());
        return Some(AtomicOp::SetPolylineData {
            entity,
            old,
            new,
        });
    }

    None
}




