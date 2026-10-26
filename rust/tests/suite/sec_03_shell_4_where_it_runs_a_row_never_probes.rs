//! §03-shell §4 "Where it runs (new)": a source row is drawn from what ffprobe already read for that file, and
//! a file nobody has measured yet is probed on a worker. The first rewrite ran one ffprobe per row on the window
//! thread at every repaint, so a 🗑 on one row re-probed every other row with the window frozen.
//!
//! The check builds a Prepare window over two sources, presses 🗑 on the first and asserts no probe ran on the
//! window thread, then lets the workers land and asserts the row that stayed was measured off-thread.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::common;
use adw::prelude::*;
use gtk4 as gtk;
use naivepost::project::{self, Project, Source};
use naivepost::startup;
use naivepost::ui;

/// A scratch root with `session.naivepost/sources/` holding the two files, cwd pinned there.
fn scratch_root() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("naivepost-s4probe-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let sess = startup::session_dir(&dir);
    std::fs::create_dir_all(sess.join("sources")).expect("sources folder");
    for name in ["take-a.wav", "take-b.wav"] {
        std::fs::write(sess.join("sources").join(name), b"not really audio").expect("source file");
    }
    std::env::set_current_dir(&dir).expect("chdir");
    dir
}

fn source(name: &str) -> Source {
    Source {
        path: format!("project:sources/{name}"),
        ..Default::default()
    }
}

#[test]
fn sec_03_shell_4_where_it_runs_a_row_never_probes() {
    static RAN: AtomicBool = AtomicBool::new(false);
    common::one_app_round(|app| {
        let root = scratch_root();
        let model = Project {
            sources: vec![source("take-a.wav"), source("take-b.wav")],
            ..Default::default()
        };
        project::save(&model, &startup::session_dir(&root)).expect("session saved");
        let window = ui::build_window(app, &model, "Prepare");
        window.present();
        common::settle_workers();
        let before = ui::source_rows::probed_here_for_test();

        ui::find_source_widget(&window, "remove-0")
            .and_then(|widget| widget.downcast::<gtk::Button>().ok())
            .expect("the first row has its 🗑")
            .emit_clicked();
        assert_eq!(
            ui::source_rows::probed_here_for_test(),
            before,
            "the repaint after 🗑 probed nothing on the window thread"
        );
        assert_eq!(
            ui::session_sources(&window),
            ["project:sources/take-b.wav"],
            "the row went"
        );
        common::settle_workers();
        assert_eq!(
            ui::source_rows::probed_here_for_test(),
            0,
            "and no probe ran on the window thread at the build either"
        );
        window.close();
        let _ = std::fs::remove_dir_all(&root);
        RAN.store(true, Ordering::SeqCst);
    });
    assert!(RAN.load(Ordering::SeqCst), "the probe check never ran");
}
