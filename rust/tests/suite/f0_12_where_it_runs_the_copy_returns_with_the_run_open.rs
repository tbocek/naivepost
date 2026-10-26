//! §03-shell F0.12 "Where it runs (new)": "Add source files…" hands the copy to a worker. The press returns at
//! once with the copy standing as the run (so ▶ waits for it and ⏹ can end it), the bar counts the bytes as
//! they land, and each file's row appears when that file is on disk. The first rewrite copied every byte inside
//! the chooser's callback, the window frozen for as long as the card took.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::common;
use adw::prelude::*;
use naivepost::project::{self, Project};
use naivepost::startup;
use naivepost::ui;

fn scratch_root() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("naivepost-f012copy-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(startup::session_dir(&dir)).expect("session folder");
    std::env::set_current_dir(&dir).expect("chdir");
    dir
}

#[test]
fn f0_12_where_it_runs_the_copy_returns_with_the_run_open() {
    static RAN: AtomicBool = AtomicBool::new(false);
    common::one_app_round(|app| {
        let root = scratch_root();
        let model = Project::default();
        project::save(&model, &startup::session_dir(&root)).expect("session saved");
        let card = root.join("card");
        std::fs::create_dir_all(&card).expect("card folder");
        let file = card.join("2026-10-18 19-21-52.mkv");
        std::fs::write(&file, vec![7u8; 4 * 1024 * 1024]).expect("a recording on the card");
        let window = ui::build_window(app, &model, "Prepare");
        window.present();
        common::settle_workers();

        assert!(ui::add_files(&window, &[file]), "the copy started");
        assert_eq!(
            ui::running_step(&window),
            Some(naivepost::run::Step::Prepare),
            "the copy stands as the run while its bytes go down"
        );
        assert!(
            ui::session_sources(&window).is_empty(),
            "no row before its file has landed"
        );
        common::settle_workers();
        assert_eq!(
            ui::running_step(&window),
            None,
            "the copy ended its own run"
        );
        assert_eq!(
            ui::session_sources(&window),
            ["project:sources/2026-10-18 19-21-52.mkv"],
            "the row came with its file"
        );
        assert!(
            ui::progress_bar(&window).expect("run-progress").fraction() > 0.0,
            "the bar counted the bytes"
        );
        window.close();
        let _ = std::fs::remove_dir_all(&root);
        RAN.store(true, Ordering::SeqCst);
    });
    assert!(RAN.load(Ordering::SeqCst), "the copy check never ran");
}
