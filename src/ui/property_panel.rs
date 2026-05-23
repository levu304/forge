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
