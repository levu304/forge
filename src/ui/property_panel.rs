//! Property inspector — narrow right side panel.
//!
//! Delegates to [`PropertyPalette`] for the draw implementation.

use hecs::World;

use crate::history::History;
use crate::layer::LayerTable;
use crate::property::palette::PropertyPalette;
use crate::selection::SelectionManager;

/// Draw the property inspector panel on the right side of the viewport.
///
/// Wraps [`PropertyPalette::draw`] inside an [`egui::Panel::right`].
pub fn draw(
    ui: &mut egui::Ui,
    world: &mut World,
    selection: &SelectionManager,
    layer_table: &LayerTable,
    history: &mut History,
) {
    let palette = PropertyPalette::default();
    egui::Panel::right("property_panel")
        .resizable(true)
        .default_width(260.0)
        .show_inside(ui, |ui| {
            palette.draw(ui, world, selection, layer_table, history);
        });
}

#[cfg(test)]
mod tests {
    use egui_kittest::{Harness, kittest::Queryable};

    use crate::history::History;
    use crate::layer::LayerTable;
    use crate::selection::SelectionManager;

    use super::draw;

    struct TestState {
        world: hecs::World,
        selection: SelectionManager,
        layer_table: LayerTable,
        history: History,
    }

    fn make_harness() -> Harness<'static, TestState> {
        let state = TestState {
            world: hecs::World::new(),
            selection: SelectionManager::new(),
            layer_table: LayerTable::new(),
            history: History::new(),
        };
        Harness::new_ui_state(
            |ui, state: &mut TestState| {
                draw(ui, &mut state.world, &state.selection, &state.layer_table, &mut state.history);
            },
            state,
        )
    }

    fn assert_label_visible(harness: &Harness<'static, TestState>, label: &str) {
        let node = harness.get_by_label(label);
        let r = node.rect();
        assert!(r.size().x > 0.0 && r.size().y > 0.0, "label '{label}' not visible");
    }

    #[test]
    fn test_property_panel_shows_no_selection() {
        let mut harness = make_harness();
        harness.run();
        assert_label_visible(&harness, "No entity selected");
    }

    #[test]
    fn test_property_panel_renders_without_panic() {
        let mut harness = make_harness();
        harness.run();
        // With no selection the panel shows the no-selection label
        assert_label_visible(&harness, "No entity selected");
    }

    #[test]
    fn test_property_panel_empty_world() {
        // Even with an empty world and no selection, the panel renders
        let mut harness = make_harness();
        harness.run();
        assert_label_visible(&harness, "No entity selected");
    }

    #[test]
    fn test_property_panel_with_entities_no_selection() {
        let mut world = hecs::World::new();
        world.spawn(());
        world.spawn(());

        let state = TestState {
            world,
            selection: SelectionManager::new(),
            layer_table: LayerTable::new(),
            history: History::new(),
        };

        let mut harness = Harness::new_ui_state(
            |ui, state: &mut TestState| {
                draw(ui, &mut state.world, &state.selection, &state.layer_table, &mut state.history);
            },
            state,
        );
        harness.run();
        // Entities exist but none selected — shows no-selection text
        assert_label_visible(&harness, "No entity selected");
    }
}
