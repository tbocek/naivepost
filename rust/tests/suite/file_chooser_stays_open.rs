//! A press that opens a file chooser leaves it on screen, on a machine with no desktop portal.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::ui;

use crate::common;

static RAN: AtomicBool = AtomicBool::new(false);

#[test]
fn add_source_files_leaves_its_chooser_on_screen() {
    common::one_app_round(|app| {
        let model = naivepost::project::load(&common::fixture_dir()).expect("the fixture loads");
        let window = ui::build_window(app, &model, naivepost::shell::Page::Prepare.label());
        window.present();
        common::settle();
        ui::add_sources_button(&window)
            .expect("the button")
            .emit_by_name::<()>("clicked", &[]);
        common::settle();
        let shown = gtk::Window::list_toplevels()
            .into_iter()
            .filter_map(|w| w.downcast::<gtk::Window>().ok())
            .any(|w| {
                w.is_visible()
                    && w.title().as_deref() == Some(naivepost::add_sources::CHOOSER_TITLE)
            });
        assert!(shown, "the chooser is on screen");
        RAN.store(true, Ordering::SeqCst);
    });
    assert!(RAN.load(Ordering::SeqCst), "the check never ran");
}
