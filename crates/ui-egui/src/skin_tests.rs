//! Keyboard, accessibility and bounded navigation contracts for the fork's chrome.
use crate::{Services, WordApp};
use egui::accesskit::{Role, Toggled};
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use serde_json::json;

fn harness(size: [f32; 2]) -> Harness<'static, WordApp> {
    let mut app = WordApp::new(wordcraft_engine::Session::new(wordcraft_doc::Document::new()), Services::default());
    app.integrated_titlebar = true;
    Harness::builder().with_size(size).build_ui_state(
        |ui, app: &mut WordApp| {
            let ctx = ui.ctx().clone();
            app.logic(&ctx);
            app.ui(ui);
        },
        app,
    )
}

#[test]
fn toolbar_names_and_keyboard() {
    let mut h = harness([1440.0, 900.0]);
    h.run_steps(4);
    assert!(h.query_by_label("Discord").is_none());
    assert_eq!(h.get_by_label_contains("Paste (").accesskit_node().role(), Role::Button);
    assert_eq!(h.get_by_label("Font").accesskit_node().role(), Role::TextInput);
    assert_eq!(h.get_by_label("Bold").accesskit_node().toggled(), Some(Toggled::False));
    h.get_by_label("Bold").focus();
    h.run_steps(2);
    h.key_press(egui::Key::Space);
    h.run_steps(4);
    assert_eq!(h.get_by_label("Bold").accesskit_node().toggled(), Some(Toggled::True));
    assert_eq!(h.state_mut().run("format.state", json!({})).unwrap()["bold"], true);
    h.get_by_role_and_label(Role::Tab, "Insert").click_accesskit();
    h.run_steps(4);
    assert_eq!(h.state().ui.tab, "Insert");
    assert_eq!(h.get_by_role_and_label(Role::Tab, "Insert").accesskit_node().is_selected(), Some(true));
}

#[test]
fn narrow_tabs_remain_reachable() {
    let mut h = harness([560.0, 600.0]);
    h.run_steps(4);
    h.get_by_label("More").click_accesskit();
    h.run_steps(3);
    h.get_all_by_role_and_label(Role::Button, "Help").find(|node| node.accesskit_node().toggled().is_some()).unwrap().click_accesskit();
    h.run_steps(4);
    assert_eq!(h.state().ui.tab, "Help");
    let help = h.get_by_role_and_label(Role::Tab, "Help");
    assert_eq!(help.accesskit_node().role(), Role::Tab);
    assert_eq!(help.accesskit_node().is_selected(), Some(true));
}

#[test]
fn scaled_status_does_not_overlap() {
    for skin in ["classic", "pink"] {
        let mut h = harness([760.0, 600.0]);
        h.state_mut().run("ui.theme", json!({"value":skin})).unwrap();
        h.state_mut().run("ui.interfaceSize", json!({"percent":200})).unwrap();
        h.run_steps(8);
        let page = h.get_by_label("Page 1 of 1").accesskit_node().bounding_box().unwrap();
        let words = h.get_by_label("0 words").accesskit_node().bounding_box().unwrap();
        let zoom = h.get_by_label("Zoom").accesskit_node().bounding_box().unwrap();
        assert!(page.x1 <= words.x0 && words.x1 < zoom.x0, "{skin}: {page:?} {words:?} {zoom:?}");
    }
}

#[test]
fn chrome_actions_are_named() {
    let mut h = harness([1440.0, 900.0]);
    h.run_steps(4);
    for label in ["Customize Quick Access Toolbar", "Undo History", "User settings", "Search commands and help", "Zoom In", "Zoom Out", "Focus"] {
        assert_eq!(h.get_by_label(label).accesskit_node().role(), Role::Button, "{label}");
    }
    assert_eq!(h.get_by_role_and_label(Role::CheckBox, "AutoSave").accesskit_node().toggled(), Some(Toggled::False));
    h.get_by_label("User settings").click_accesskit();
    h.run_steps(4);
    assert!(h.state().ui.backstage);
    assert_eq!(h.state().ui.backstage_page, "options");
}

#[test]
fn square_chrome_close_is_guarded() {
    let mut h = harness([760.0, 600.0]);
    h.state_mut().start_screen();
    h.run_steps(4);
    let back = h.get_by_label("Back to document").accesskit_node().bounding_box().unwrap();
    let close = h.get_by_label("Close window").accesskit_node().bounding_box().unwrap();
    assert!(back.y0 >= close.y1, "Back must be below the caption controls");
    h.get_by_label("Back to document").focus();
    h.run_steps(1);
    h.key_press(egui::Key::Space);
    h.run_steps(4);
    assert!(!h.state().ui.backstage);
    h.state_mut().run("text.insert", json!({"text":"Unsaved synthetic text"})).unwrap();
    h.get_by_label("Close window").click_accesskit();
    h.run_steps(4);
    assert!(!h.state().quit_requested);
    assert_eq!(h.state().dialog.as_ref().map(crate::dialogs::Dialog::name), Some("saveChanges"));
}

#[test]
fn tab_visibility_is_discoverable() {
    let mut h = harness([1440.0, 900.0]);
    h.run_steps(4);
    assert!(h.query_by_role_and_label(Role::Tab, "Mailings").is_none());
    assert!(h.query_by_role_and_label(Role::Tab, "Developer").is_none());
    h.get_by_label("Tabs…").click_accesskit();
    h.run_steps(3);
    h.get_by_role_and_label(Role::CheckBox, "Developer").click_accesskit();
    h.run_steps(4);
    assert!(h.state().session.ribbon.tabs().iter().any(|tab| tab.name == "Developer" && !tab.hidden));
    assert!(h.state_mut().run("ui.discord", json!({})).is_err());
}
