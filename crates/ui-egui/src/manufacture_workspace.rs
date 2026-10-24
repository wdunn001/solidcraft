//! Manufacture workspace: the MANUFACTURE ribbon hosts `cam.*` and `laser.*` commands.
//!
//! The viewport stays Design's. Toolpaths are computed at post time through solidcraft-cam;
//! they are not stored on the design. A right-hand mill panel is how a setup is added: the
//! ribbon button used to run `cam.setup` with no name.

use crate::ui_kit::{self, Button, Container, Edge, Surface};
use serde_json::json;

use crate::SolveApp;
use crate::theme::Tokens;

const OFFSETS: [&str; 6] = ["G54", "G55", "G56", "G57", "G58", "G59"];

#[derive(Default)]
pub struct State {
    pub active: bool,
    name: String,
    work_offset: usize,
}

pub fn active(app: &SolveApp) -> bool {
    app.manufacture_workspace.active
}

pub fn enter(app: &mut SolveApp) {
    if active(app) {
        fill_name(app);
        return;
    }
    if crate::render_workspace::active(app) {
        let _ = crate::render_workspace::enter(app, false);
    }
    app.manufacture_workspace.active = true;
    fill_name(app);
    if app.ui.tab != "MANUFACTURE" {
        app.ui.tab = "MANUFACTURE".into();
    }
}

fn fill_name(app: &mut SolveApp) {
    if app.manufacture_workspace.name.trim().is_empty() {
        app.manufacture_workspace.name = app.session.doc.cam.unused_setup_name();
    }
}

pub fn exit(app: &mut SolveApp) {
    if !active(app) {
        return;
    }
    app.manufacture_workspace.active = false;
    if app.ui.tab == "MANUFACTURE" {
        app.ui.tab = "MODEL".into();
    }
}

pub fn reset_document(app: &mut SolveApp) {
    app.manufacture_workspace = State::default();
}

pub fn close_floating_dialogs(_app: &mut SolveApp) {}

/// Return false to cancel starting `id`.
pub fn start_hook(app: &mut SolveApp, id: &str) -> bool {
    if id != "cam.setup" {
        return true;
    }
    enter(app);
    add_setup(app);
    false
}

pub fn tick(_app: &mut SolveApp, _ctx: &egui::Context) {}

pub fn panel(app: &mut SolveApp, ui: &mut egui::Ui) {
    Container::new(ui.ctx(), Surface::Panel)
        .edge("sc_manufacture_setup", Edge::Right)
        .exact_size(285.0)
        .resizable(false)
        .frame(Container::new(ui.ctx(), Surface::Panel).edge_frame(Tokens::get().panel).inner_margin(10))
        .show(ui, |ui| {
            crate::ui_kit::scope(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| mill_panel(app, ui));
            });
        });
}

fn mill_panel(app: &mut SolveApp, ui: &mut egui::Ui) {
    ui.heading(app.tr("cmd.cam_setup"));
    ui.separator();
    let setups: Vec<(String, u8)> = app.session.doc.cam.setups.iter().map(|s| (s.name.clone(), s.work_offset)).collect();
    if setups.is_empty() {
        ui.small(app.tr("cam.no_setups"));
    } else {
        for (name, offset) in &setups {
            let label = format!("{name}  (G{})", 53 + offset);
            if ui_kit::button(ui, label, Button::Ghost).clicked() {
                app.manufacture_workspace.name = name.clone();
                app.manufacture_workspace.work_offset = offset.saturating_sub(1).min(5) as usize;
            }
        }
    }
    ui.separator();
    let (enter, _) = ui_kit::text_row(ui, "Name", &mut app.manufacture_workspace.name, "Setup 1");
    ui_kit::choice_row(ui, app.tr("cam.work_offset"), "cam_work_offset", &mut app.manufacture_workspace.work_offset, &OFFSETS);
    let add = app.tr("cam.add_setup").to_string();
    if ui_kit::button(ui, add, Button::Primary).clicked() || enter {
        add_setup(app);
    }
}

fn add_setup(app: &mut SolveApp) {
    fill_name(app);
    let name = app.manufacture_workspace.name.trim().to_string();
    let work_offset = (app.manufacture_workspace.work_offset as u8) + 1;
    match app.run("cam.setup", json!({"name": name, "work_offset": work_offset})) {
        Ok(_) => {
            app.manufacture_workspace.name.clear();
            fill_name(app);
            app.set_status(app.tr("cmd.cam_setup"), false);
        }
        Err(e) => app.set_status(e, true),
    }
}

pub fn show(app: &mut SolveApp, ui: &mut egui::Ui) {
    crate::viewport::show(app, ui);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenario::Harness;
    use serde_json::json;

    #[test]
    fn clicking_cam_setup_adds_a_named_setup() {
        let mut h = Harness::new();
        h.step(&json!({"start": "cam.setup"})).unwrap();
        h.frames(2);
        assert!(active(&h.app));
        assert_eq!(h.app.session.doc.cam.setups.len(), 1);
        assert_eq!(h.app.session.doc.cam.setups[0].name, "Setup 1");
        assert_eq!(h.app.session.doc.cam.setups[0].work_offset, 1);
        assert_eq!(h.app.manufacture_workspace.name, "Setup 2");
        h.step(&json!({"start": "cam.setup"})).unwrap();
        assert_eq!(h.app.session.doc.cam.setups.len(), 2);
        assert_eq!(h.app.session.doc.cam.setups[1].name, "Setup 2");
        let add = h.app.tr("cam.add_setup").to_string();
        h.frames(2);
        h.step(&json!({"widget": add})).unwrap();
        assert_eq!(h.app.session.doc.cam.setups[2].name, "Setup 3");
    }
}
