//! Conventional menus over the existing command path. No document behaviour lives here.
use crate::{
    WordApp,
    theme::{Appearance, Tokens},
};
use egui::Ui;
use serde_json::json;

#[derive(Clone, Copy)]
enum Action {
    Command(&'static str),
    Dialog(&'static str),
    Theme(Appearance),
    Start,
}
const FILE: &[(&str, Action)] = &[
    ("New", Action::Command("file.new")),
    ("Open…", Action::Command("file.open")),
    ("Save", Action::Command("file.save")),
    ("Save as…", Action::Command("file.saveAs")),
    ("Print…", Action::Command("file.print")),
    ("Start page / recent files", Action::Start),
    ("Options…", Action::Command("file.options")),
];
const EDIT: &[(&str, Action)] = &[
    ("Undo", Action::Command("edit.undo")),
    ("Redo", Action::Command("edit.redo")),
    ("Cut", Action::Command("edit.cut")),
    ("Copy", Action::Command("edit.copy")),
    ("Paste", Action::Command("edit.paste")),
    ("Find", Action::Command("edit.find")),
    ("Replace…", Action::Dialog("replace")),
];
const VIEW: &[(&str, Action)] = &[
    ("Classic", Action::Theme(Appearance::Classic)),
    ("Pink", Action::Theme(Appearance::Pink)),
    ("Light", Action::Theme(Appearance::Light)),
    ("Dark", Action::Theme(Appearance::Dark)),
    ("System", Action::Theme(Appearance::System)),
    ("Collapse toolbar", Action::Command("ui.collapseRibbon")),
];
const INSERT: &[(&str, Action)] =
    &[("Table…", Action::Dialog("insertTable")), ("Picture…", Action::Command("insert.picture")), ("Link…", Action::Dialog("link"))];
const FORMAT: &[(&str, Action)] = &[("Font…", Action::Dialog("font")), ("Paragraph…", Action::Dialog("paragraph"))];
const HELP: &[(&str, Action)] = &[("App updates", Action::Command(crate::app_updates::COMMAND)), ("About WordCraft Fin", Action::Dialog("about"))];

pub(crate) fn show(app: &mut WordApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    if !t.is_classic() {
        return;
    }
    egui::Panel::top("classic_menu").exact_size(24.0).frame(egui::Frame::NONE.fill(t.ribbon).inner_margin(egui::Margin::symmetric(5, 0))).show(
        ui,
        |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                for (label, items) in [("File", FILE), ("Edit", EDIT), ("View", VIEW), ("Insert", INSERT), ("Format", FORMAT), ("Help", HELP)] {
                    ui.menu_button(label, |ui| {
                        for (label, action) in items {
                            let enabled = match action {
                                Action::Command(id) => crate::widgets::enabled(app, id),
                                _ => true,
                            };
                            let selected = matches!(action, Action::Theme(value) if *value == app.ui.theme);
                            if !ui.add_enabled(enabled, egui::Button::new(*label).selected(selected)).clicked() {
                                continue;
                            }
                            run(app, *action);
                            ui.close();
                        }
                    });
                }
            });
        },
    );
}
fn run(app: &mut WordApp, action: Action) {
    let (id, params) = match action {
        Action::Command(id) => (id, json!({})),
        Action::Dialog(name) => ("ui.dialog", json!({"name":name})),
        Action::Theme(appearance) => ("ui.theme", json!({"value":appearance.code()})),
        Action::Start => ("ui.tab", json!({"tab":"File"})),
    };
    crate::widgets::run_from_ribbon(app, id, params);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn menu_commands_resolve() {
        let app = WordApp::new(wordcraft_engine::Session::new(wordcraft_doc::Document::new()), crate::Services::default());
        for items in [FILE, EDIT, VIEW, INSERT, FORMAT, HELP] {
            for (_, action) in items {
                if let Action::Command(id) = action {
                    assert!(id.starts_with("ui.") || app.session.registry.get(id).is_some(), "unregistered menu action {id}");
                }
            }
        }
    }
    #[test]
    fn menu_dialogs_open() {
        let mut app = WordApp::new(wordcraft_engine::Session::new(wordcraft_doc::Document::new()), crate::Services::default());
        for (_, items) in [("File", FILE), ("Edit", EDIT), ("Insert", INSERT), ("Format", FORMAT), ("Help", HELP)] {
            for (_, action) in items {
                if let Action::Dialog(name) = action {
                    run(&mut app, *action);
                    assert!(app.dialog.is_some(), "missing dialog {name}");
                    assert!(!app.session.dirty, "opening {name} changed the document");
                }
            }
        }
    }
}
