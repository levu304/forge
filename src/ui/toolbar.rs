//! Toolbar — narrow left side panel.
//!
//! Draw and modify command buttons: LINE, CIRCLE, ARC, PLINE,
//! and modify commands (ERASE, MOVE, COPY, ROTATE, SCALE,
//! MIRROR, OFFSET).

use crate::commands::{CommandState, PendingModifyCommand};

const BUTTON_SPACING: f32 = 4.0;

/// Draw the toolbar panel on the left side of the viewport.
///
/// Clicking a button sets [`CommandState::pending_dispatch`] (for draw
/// commands) or [`CommandState::pending_modify_command`] (for modify
/// commands), each consumed by the event-loop handler to start the command.
pub fn draw(ui: &mut egui::Ui, cmd_state: &mut CommandState) {
    egui::Panel::left("toolbar")
        .resizable(false)
        .default_size(48.0)
        .show_inside(ui, |ui| {
            ui.vertical(|ui| {
                ui.add_space(8.0);

                // ── Draw commands ────────────────────────────────────────
                for &name in &["LINE", "CIRCLE", "ARC", "PLINE"] {
                    let response = ui.button(name);
                    tracing::debug!(
                        btn = name,
                        hovered = response.hovered(),
                        y = response.rect.min.y,
                        "button"
                    );
                    if response.clicked() {
                        cmd_state.pending_dispatch = Some(name.into());
                    }
                    ui.add_space(BUTTON_SPACING);
                }

                ui.separator();
                ui.add_space(BUTTON_SPACING);

                // ── Modify commands ──────────────────────────────────────
                // NOTE: These bypass the text parser entirely; they set
                // `pending_modify_command` which is consumed by the event
                // loop to construct the concrete command with access to
                // `SelectionManager` (forge-51x).
                const MODIFY_COMMANDS: &[(&str, PendingModifyCommand)] = &[
                    ("ERASE", PendingModifyCommand::Erase),
                    ("MOVE", PendingModifyCommand::Move),
                    ("COPY", PendingModifyCommand::Copy),
                    ("ROTATE", PendingModifyCommand::Rotate),
                    ("SCALE", PendingModifyCommand::Scale),
                    ("MIRROR", PendingModifyCommand::Mirror),
                    ("OFFSET", PendingModifyCommand::Offset),
                ];
                for &(label, cmd) in MODIFY_COMMANDS {
                    if ui.button(label).clicked() {
                        cmd_state.pending_modify_command = Some(cmd);
                    }
                    ui.add_space(BUTTON_SPACING);
                }
            });
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};

    fn new_harness() -> Harness<'static, CommandState> {
        Harness::new_ui_state(
            |ui, state: &mut crate::commands::CommandState| {
                draw(ui, state);
            },
            crate::commands::CommandState::default(),
        )
    }

    #[test]
    fn button_rects_are_disjoint_and_ordered() {
        let mut harness = new_harness();

        let line = harness.get_by_label("LINE").rect();
        let circle = harness.get_by_label("CIRCLE").rect();
        let arc = harness.get_by_label("ARC").rect();
        let pline = harness.get_by_label("PLINE").rect();

        eprintln!("LINE:   y={:.1}-{:.1}", line.min.y, line.max.y);
        eprintln!("CIRCLE: y={:.1}-{:.1}", circle.min.y, circle.max.y);
        eprintln!("ARC:    y={:.1}-{:.1}", arc.min.y, arc.max.y);
        eprintln!("PLINE:  y={:.1}-{:.1}", pline.min.y, pline.max.y);

        assert!(
            line.max.y <= circle.min.y,
            "LINE overlaps CIRCLE: max_y={:.1} >= min_y={:.1}",
            line.max.y, circle.min.y
        );
        assert!(circle.max.y <= arc.min.y, "CIRCLE overlaps ARC");
        assert!(arc.max.y <= pline.min.y, "ARC overlaps PLINE");
    }

    #[test]
    fn click_line_dispatches_command() {
        let mut harness = new_harness();
        harness.get_by_label("LINE").click();
        harness.run();
        assert_eq!(harness.state().pending_dispatch.as_deref(), Some("LINE"));
    }

    #[test]
    fn click_circle_dispatches_command() {
        let mut harness = new_harness();
        harness.get_by_label("CIRCLE").click();
        harness.run();
        assert_eq!(harness.state().pending_dispatch.as_deref(), Some("CIRCLE"));
    }

    #[test]
    fn click_arc_dispatches_command() {
        let mut harness = new_harness();
        harness.get_by_label("ARC").click();
        harness.run();
        assert_eq!(harness.state().pending_dispatch.as_deref(), Some("ARC"));
    }

    #[test]
    fn click_pline_dispatches_command() {
        let mut harness = new_harness();
        harness.get_by_label("PLINE").click();
        harness.run();
        assert_eq!(harness.state().pending_dispatch.as_deref(), Some("PLINE"));
    }
}
