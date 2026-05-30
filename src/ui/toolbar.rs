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
                    ("EXPLODE", PendingModifyCommand::Explode),
                    ("MATCHPROP", PendingModifyCommand::MatchProp),
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
        let harness = new_harness();

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

    // -- Draw command dispatch -----------------------------------------------

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

    // -- Modify command button existence -------------------------------------

    #[test]
    fn modify_buttons_exist() {
        let mut harness = new_harness();
        harness.run();
        for &label in &[
            "ERASE", "MOVE", "COPY", "ROTATE", "SCALE", "MIRROR", "OFFSET",
            "EXPLODE", "MATCHPROP",
        ] {
            let node = harness.get_by_label(label);
            let r = node.rect();
            assert!(r.size().x > 0.0 && r.size().y > 0.0, "Button '{label}' should be visible (rect: {r:?})");
        }
    }

    // -- Modify command dispatch ---------------------------------------------

    #[test]
    fn click_erase_dispatches_modify_command() {
        let mut harness = new_harness();
        harness.get_by_label("ERASE").click();
        harness.run();
        assert_eq!(
            harness.state().pending_modify_command,
            Some(PendingModifyCommand::Erase),
        );
    }

    #[test]
    fn click_move_dispatches_modify_command() {
        let mut harness = new_harness();
        harness.get_by_label("MOVE").click();
        harness.run();
        assert_eq!(
            harness.state().pending_modify_command,
            Some(PendingModifyCommand::Move),
        );
    }

    #[test]
    fn click_copy_dispatches_modify_command() {
        let mut harness = new_harness();
        harness.get_by_label("COPY").click();
        harness.run();
        assert_eq!(
            harness.state().pending_modify_command,
            Some(PendingModifyCommand::Copy),
        );
    }

    #[test]
    fn click_rotate_dispatches_modify_command() {
        let mut harness = new_harness();
        harness.get_by_label("ROTATE").click();
        harness.run();
        assert_eq!(
            harness.state().pending_modify_command,
            Some(PendingModifyCommand::Rotate),
        );
    }

    #[test]
    fn click_scale_dispatches_modify_command() {
        let mut harness = new_harness();
        harness.get_by_label("SCALE").click();
        harness.run();
        assert_eq!(
            harness.state().pending_modify_command,
            Some(PendingModifyCommand::Scale),
        );
    }

    #[test]
    fn click_mirror_dispatches_modify_command() {
        let mut harness = new_harness();
        harness.get_by_label("MIRROR").click();
        harness.run();
        assert_eq!(
            harness.state().pending_modify_command,
            Some(PendingModifyCommand::Mirror),
        );
    }

    #[test]
    fn click_offset_dispatches_modify_command() {
        let mut harness = new_harness();
        harness.get_by_label("OFFSET").click();
        harness.run();
        assert_eq!(
            harness.state().pending_modify_command,
            Some(PendingModifyCommand::Offset),
        );
    }

    #[test]
    fn click_explode_dispatches_modify_command() {
        let mut harness = new_harness();
        harness.get_by_label("EXPLODE").click();
        harness.run();
        assert_eq!(
            harness.state().pending_modify_command,
            Some(PendingModifyCommand::Explode),
        );
    }

    #[test]
    fn click_matchprop_dispatches_modify_command() {
        let mut harness = new_harness();
        harness.get_by_label("MATCHPROP").click();
        harness.run();
        assert_eq!(
            harness.state().pending_modify_command,
            Some(PendingModifyCommand::MatchProp),
        );
    }

    // -- Interaction: draw + modify together ----------------------------------

    #[test]
    fn draw_then_modify_buttons_both_work() {
        let mut harness = new_harness();
        // Click LINE
        harness.get_by_label("LINE").click();
        harness.run();
        assert_eq!(harness.state().pending_dispatch.as_deref(), Some("LINE"));

        // Click ERASE (now pending_modify_command is set)
        harness.get_by_label("ERASE").click();
        harness.run();
        assert_eq!(
            harness.state().pending_modify_command,
            Some(PendingModifyCommand::Erase),
        );
        // pending_dispatch should still be "LINE" (not cleared by modify button)
        assert_eq!(harness.state().pending_dispatch.as_deref(), Some("LINE"));
    }

    // -- Button rect ordering for modify commands ----------------------------

    #[test]
    fn modify_button_rects_are_disjoint_and_ordered() {
        let mut harness = new_harness();
        harness.run();

        let erase = harness.get_by_label("ERASE").rect();
        let mv = harness.get_by_label("MOVE").rect();
        let copy = harness.get_by_label("COPY").rect();
        let rotate = harness.get_by_label("ROTATE").rect();
        let scale = harness.get_by_label("SCALE").rect();
        let mirror = harness.get_by_label("MIRROR").rect();
        let offset = harness.get_by_label("OFFSET").rect();
        let explode = harness.get_by_label("EXPLODE").rect();
        let matchprop = harness.get_by_label("MATCHPROP").rect();

        assert!(erase.max.y <= mv.min.y, "ERASE overlaps MOVE");
        assert!(mv.max.y <= copy.min.y, "MOVE overlaps COPY");
        assert!(copy.max.y <= rotate.min.y, "COPY overlaps ROTATE");
        assert!(rotate.max.y <= scale.min.y, "ROTATE overlaps SCALE");
        assert!(scale.max.y <= mirror.min.y, "SCALE overlaps MIRROR");
        assert!(mirror.max.y <= offset.min.y, "MIRROR overlaps OFFSET");
        assert!(offset.max.y <= explode.min.y, "OFFSET overlaps EXPLODE");
        assert!(explode.max.y <= matchprop.min.y, "EXPLODE overlaps MATCHPROP");
    }
}
