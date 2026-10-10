use bn_client_egui::setup::{Launcher, Starter};
use bn_client_egui::{App, FakeModel};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use std::cell::RefCell;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::rc::Rc;

fn harness<'a>(tag: &str) -> (Harness<'a, Launcher>, Rc<RefCell<Vec<PathBuf>>>, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bn-setup-ui-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let calls = Rc::new(RefCell::new(vec![]));
    let log = calls.clone();
    let start: Starter = Box::new(move |_, path| {
        log.borrow_mut().push(path);
        Ok(Box::new(App::new(FakeModel::default())))
    });
    let launcher = Launcher::new("searched: /nowhere/engine".into(), start);
    let harness = Harness::builder()
        .with_size(egui::vec2(600.0, 300.0))
        .build_ui_state(|ui, l: &mut Launcher| l.show(ui), launcher);
    (harness, calls, dir)
}

fn type_and_use(harness: &mut Harness<'_, Launcher>, text: &str) {
    harness
        .get_by_role(egui::accesskit::Role::TextInput)
        .focus();
    harness.run();
    harness
        .get_by_role(egui::accesskit::Role::TextInput)
        .type_text(text);
    harness.run();
    harness.get_by_label("Use").click();
    harness.run();
}

#[test]
fn setup_window_lists_searched_locations() {
    let (harness, _, _) = harness("list");
    harness.get_by_label("searched: /nowhere/engine");
}

fn rejected(tag: &str, make: impl Fn(&std::path::Path) -> PathBuf) {
    let (mut harness, calls, dir) = harness(tag);
    type_and_use(&mut harness, make(&dir).to_str().unwrap());
    harness.get_by_label_contains("is not an executable file");
    assert!(calls.borrow().is_empty());
    assert!(!harness.state().is_running());
}

#[test]
fn a_missing_path_is_rejected_without_starting() {
    rejected("missing", |dir| dir.join("missing"));
}

#[test]
fn a_non_executable_file_is_rejected_without_starting() {
    rejected("plain", |dir| {
        std::fs::write(dir.join("plain"), "").unwrap();
        dir.join("plain")
    });
}

#[test]
fn an_executable_path_starts_the_engine() {
    let (mut harness, calls, dir) = harness("start");
    let engine = dir.join("engine");
    std::fs::write(&engine, "").unwrap();
    std::fs::set_permissions(&engine, std::fs::Permissions::from_mode(0o755)).unwrap();
    type_and_use(&mut harness, engine.to_str().unwrap());
    assert_eq!(*calls.borrow(), vec![engine]);
    assert!(harness.state().is_running());
}
