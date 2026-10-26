//! §03-shell F0.1 "Where it runs (new)": a tab switch paints the page at once and the Inputs/Outputs readouts
//! when a worker has counted the page's folders. The first rewrite walked Prepare's outputs twice and every
//! source's frame folder on the window thread at each switch. The last readout stays up until the new one lands.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::common;
use adw::prelude::*;
use naivepost::project::{self, Project, Source};
use naivepost::shell::Page;
use naivepost::startup;
use naivepost::ui;

fn scratch_root() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("naivepost-f01walk-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(startup::session_dir(&dir)).expect("session folder");
    std::env::set_current_dir(&dir).expect("chdir");
    dir
}

fn inputs(window: &adw::ApplicationWindow) -> String {
    ui::inputs_readout(window)
        .expect("the window carries inputs-readout")
        .text()
        .to_string()
}

#[test]
fn f0_1_where_it_runs_a_switch_never_walks() {
    static RAN: AtomicBool = AtomicBool::new(false);
    common::one_app_round(|app| {
        let root = scratch_root();
        // One footage source: Cut is open, and its readout reads differently from Prepare's.
        let model = Project {
            sources: vec![Source {
                path: "project:sources/lecture.mkv".into(),
                footage: true,
                ..Default::default()
            }],
            ..Default::default()
        };
        project::save(&model, &startup::session_dir(&root)).expect("session saved");
        let window = ui::build_window(app, &model, "Prepare");
        window.present();
        common::settle_workers();
        let prepare = inputs(&window);
        let read_before = ui::window::readouts_read_here_for_test();

        ui::tab_button(&window, Page::Cut)
            .expect("the tab row has Cut")
            .emit_clicked();
        assert_eq!(
            ui::window::readouts_read_here_for_test(),
            read_before,
            "the switch counted no folder on the window thread"
        );
        assert_eq!(
            inputs(&window),
            prepare,
            "the last readout stays up until the new one lands"
        );
        common::settle_workers();
        assert_ne!(
            inputs(&window),
            prepare,
            "the Cut page's readout landed: {:?}",
            inputs(&window)
        );
        window.close();
        let _ = std::fs::remove_dir_all(&root);
        RAN.store(true, Ordering::SeqCst);
    });
    assert!(RAN.load(Ordering::SeqCst), "the switch check never ran");
}
