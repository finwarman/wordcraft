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
    let mut h = harness([760.0, 600.0]);
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
