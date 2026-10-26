//! §03-shell §7: a write a person's action asks for never fails silently. Each check makes the write fail for real
//! (the file it writes is a folder) and drives the real widget: the `!!!` line reaches the log, and where the
//! action reports on the status line, the failure is the status line's last word. The first rewrite dropped
//! these results with `let _ =`: a fold, an insert or a suggestion that could not be saved, a prompt edit, and the
//! narration flushed on a tab leave or on close all looked written and were not.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::common;
use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut::{self, Seg};
use naivepost::layout::Tree;
use naivepost::narration::Entry;
use naivepost::project::{Project, Source};
use naivepost::shell::Page;
use naivepost::timeline::Recording;
use naivepost::ui;

use common::{clip, settle, status_text};

/// A session folder of its own for one check, with cwd and the open folder pointed at it, so every door that
/// resolves "the session" (cwd or the open folder) writes into THIS folder.
fn session(tag: &str) -> Tree {
    let root = common::temp_dir_created(&format!("write-said-{tag}"));
    std::env::set_current_dir(&root).expect("cwd pinned");
    let dir = naivepost::startup::session_dir(&root);
    std::fs::create_dir_all(&dir).expect("session folder");
    naivepost::startup::set_open_dir(&dir);
    Tree::new(&dir).expect("a .naivepost folder")
}

/// Make `file` impossible to write: a non-empty folder stands where the file would go.
fn block(file: &Path) {
    std::fs::create_dir_all(file).expect("a folder in the file's place");
    std::fs::write(file.join("keep"), b"x").expect("the folder is not empty");
}

/// The log lines said since `before`.
fn said_since(before: usize) -> Vec<String> {
    ui::window_logs()[before..].to_vec()
}

/// A project with one footage row, saved into `tree`, so the Cut and Narrate tabs are open.
fn project_in(tree: &Tree) -> Project {
    let mut project = Project::default();
    project.policy.language.value = "en".to_string();
    project.sources.push(Source {
        path: "/media/lecture.mkv".into(),
        footage: true,
        ..Default::default()
    });
    naivepost::project::save(&project, tree.dir()).expect("naivepost.json written");
    project
}

/// F2.11's fold, through its real badge: the fold lands on the page, and the cut.json it writes cannot be written.
fn check_a_fold_that_cannot_be_saved_says_so(app: &adw::Application) {
    let tree = session("fold");
    let project = project_in(&tree);
    let mut seeded = cut::Cut::default();
    seeded.segs = vec![clip(10.0, 30.0), clip(40.0, 60.0)];
    let window = ui::build_window(app, &project, "Prepare");
    window.present();
    ui::tab_button(&window, Page::Cut)
        .expect("the Cut tab")
        .emit_clicked();
    ui::set_session_recordings(&[Recording {
        base: "session-tape".to_string(),
        start: 0.0,
        end: 120.0,
    }]);
    ui::seed_review_cut(&window, &seeded);
    ui::refresh_camera_rows(&window);
    ui::refresh_fold_badges(&window);
    settle();
    block(&tree.cut_json());
    let index = ui::fold_gaps(&window)
        .iter()
        .position(|gap| *gap == (30.0, 40.0))
        .expect("a badge over the hole");
    let overlay = ui::fold_overlay(&window).expect("the band's fold overlay");
    let badge = find_button(overlay.upcast_ref(), &format!("fold-button-{}", index + 1))
        .expect("the hole's fold badge");
    let before = ui::window_logs().len();
    badge.emit_clicked();
    settle();
    let said = said_since(before);
    assert!(
        said.iter()
            .any(|line| line.starts_with("!!! could not save the cut -- ")),
        "the failed write is said: {said:?}"
    );
    assert!(
        status_text(&window).starts_with("could not save the cut -- "),
        "and it is the status line's last word: {}",
        status_text(&window)
    );
    window.close();
    settle();
}

/// §04-prepare #6's prompt bench: every keystroke stores, and a prompt file that cannot be written is said once
/// for the run of failing keystrokes, not once per key.
fn check_a_prompt_that_cannot_be_saved_says_so_once(app: &adw::Application) {
    let tree = session("prompt");
    let project = project_in(&tree);
    let row = &naivepost::bench::ROWS[6];
    let paths = naivepost::settings::from_environment().expect("a settings folder");
    block(&paths.prompt_file(row.key));
    let window = ui::build_window(app, &project, "Prepare");
    window.present();
    settle();
    let picker = ui::find_source_widget(&window, "bench-picker")
        .expect("the bench's row picker")
        .downcast::<gtk::DropDown>()
        .expect("a dropdown");
    picker.set_selected(6);
    settle();
    let buffer = ui::find_source_widget(&window, "bench-text")
        .expect("the bench's text box")
        .downcast::<gtk::TextView>()
        .expect("a text view")
        .buffer();
    let before = ui::window_logs().len();
    buffer.set_text("a prompt edit that cannot land");
    buffer.set_text("a prompt edit that cannot land, typed on");
    settle();
    let failures: Vec<String> = said_since(before)
        .into_iter()
        .filter(|line| line.starts_with("!!! could not save the prompt -- "))
        .collect();
    assert_eq!(
        failures.len(),
        1,
        "two failing keystrokes, one sentence: {failures:?}"
    );
    assert!(
        status_text(&window).starts_with("could not save the prompt -- "),
        "on the status line too: {}",
        status_text(&window)
    );
    window.close();
    settle();
}

/// §F4.7's flushes: a half-typed line is written when the tab is left and when the window closes; a narration.json
/// that cannot be written is said both times.
fn check_a_narration_that_cannot_be_saved_says_so(app: &adw::Application) {
    let tree = session("narration");
    let project = project_in(&tree);
    let cut_ = cut::Cut {
        segs: vec![
            Seg {
                s: 0.0,
                e: 10.0,
                ..Default::default()
            },
            Seg {
                s: 10.0,
                e: 20.0,
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    cut::save(&cut_, &tree).expect("cut.json written, so Narrate is open");
    let window = ui::build_window(app, &project, "Prepare");
    window.present();
    ui::tab_button(&window, Page::Narrate)
        .expect("the Narrate tab")
        .emit_clicked();
    settle();
    ui::set_state(ui::NarrateState {
        entries: vec![Entry {
            s: 0.0,
            e: 10.0,
            at: 2.0,
            text: "a spoken line".into(),
            emotion: "calm".into(),
            ..Default::default()
        }],
        segs: cut_.segs.clone(),
        voice: "narrator1".into(),
        length: 20.0,
        clips: 2,
        has_cut: true,
        has_timeline: true,
        narrators: 1,
        recording_covers_clip: true,
        language: "en".into(),
        ..Default::default()
    });
    ui::refresh(&window);
    settle();
    block(&tree.narration_json());
    let type_in_row = |text: &str| {
        common::widget_in(&window, "line-text-0")
            .expect("row 0's text box")
            .downcast::<gtk::TextView>()
            .map(|view| naivepost::ui::narrate_row::set_box_text(&view, text))
            .expect("a text view");
    };
    // Left before the 400 ms beat: the tab leave is the flush that writes it.
    let before = ui::window_logs().len();
    type_in_row("half-typed when the tab changed");
    ui::tab_button(&window, Page::Prepare)
        .expect("the Prepare tab")
        .emit_clicked();
    settle();
    let said = said_since(before);
    assert!(
        said.iter().any(
            |line| line.starts_with("!!! could not save the narration -- ")
                || line.starts_with("!!! could not write ")
        ),
        "the tab leave's failed write is said: {said:?}"
    );
    // And the close.
    ui::tab_button(&window, Page::Narrate)
        .expect("the Narrate tab")
        .emit_clicked();
    settle();
    type_in_row("typed just before the close");
    let before = ui::window_logs().len();
    window.close();
    settle();
    let said = said_since(before);
    assert!(
        said.iter().any(
            |line| line.starts_with("!!! could not save the narration -- ")
                || line.starts_with("!!! could not write ")
        ),
        "the close's failed write is said: {said:?}"
    );
}

/// A button by name anywhere under `root`.
fn find_button(root: &gtk::Widget, name: &str) -> Option<gtk::Button> {
    if root.widget_name() == name {
        return root.clone().downcast::<gtk::Button>().ok();
    }
    let mut child = root.first_child();
    while let Some(node) = child {
        if let Some(found) = find_button(&node, name) {
            return Some(found);
        }
        child = node.next_sibling();
    }
    None
}

#[test]
fn sec_03_shell_7_a_write_a_person_asks_for_never_fails_silently() {
    static RAN: AtomicBool = AtomicBool::new(false);
    common::one_app_round(|app| {
        check_a_fold_that_cannot_be_saved_says_so(app);
        check_a_prompt_that_cannot_be_saved_says_so_once(app);
        check_a_narration_that_cannot_be_saved_says_so(app);
        RAN.store(true, Ordering::SeqCst);
        app.quit();
    });
    assert!(RAN.load(Ordering::SeqCst), "the round never ran");
}
