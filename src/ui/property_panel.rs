//! Property inspector — narrow right side panel (stub).
//!
//! Read-only entity property display. Full implementation deferred to v0.2.0+.

/// Draw the property inspector panel on the right side of the viewport.
///
/// v0.1.0 stub — displays a heading and placeholder text only.
/// Entity property inspection will be added in v0.2.0+.
pub fn draw(ui: &mut egui::Ui, _world: &hecs::World) {
    egui::Panel::right("property_panel")
        .resizable(false)
        .default_size(180.0)
        .show_inside(ui, |ui| {
            ui.heading("Properties");
            ui.separator();
            ui.label("(stub — v0.2.0+)");
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};

    fn make_harness() -> Harness<'static, hecs::World> {
        Harness::new_ui_state(
            |ui, world: &mut hecs::World| {
                draw(ui, world);
            },
            hecs::World::new(),
        )
    }

    fn assert_label_visible(harness: &Harness<'static, hecs::World>, label: &str) {
        let node = harness.get_by_label(label);
        let r = node.rect();
        assert!(r.size().x > 0.0 && r.size().y > 0.0, "label '{label}' not visible");
    }

    #[test]
    fn test_property_panel_heading() {
        let mut harness = make_harness();
        harness.run();
        assert_label_visible(&harness, "Properties");
    }

    #[test]
    fn test_property_panel_stub_text() {
        let mut harness = make_harness();
        harness.run();
        assert_label_visible(&harness, "(stub — v0.2.0+)");
    }

    #[test]
    fn test_property_panel_renders_without_panic() {
        let mut harness = make_harness();
        // Should not panic on render
        harness.run();
        // Verify both elements coexist
        assert_label_visible(&harness, "Properties");
        assert_label_visible(&harness, "(stub — v0.2.0+)");
    }

    #[test]
    fn test_property_panel_empty_world() {
        // Even with an empty world, the panel should render its stub UI
        let mut harness = make_harness();
        harness.run();
        // The draw function takes _world and ignores it in stub mode
        assert_label_visible(&harness, "Properties");
    }

    #[test]
    fn test_property_panel_with_entities() {
        let mut world = hecs::World::new();
        world.spawn(());
        world.spawn(());

        let mut harness = Harness::new_ui_state(
            |ui, w: &mut hecs::World| {
                draw(ui, w);
            },
            world,
        );
        harness.run();
        // Still shows stub text — entities don't change the stub UI
        assert_label_visible(&harness, "(stub — v0.2.0+)");
    }
}
