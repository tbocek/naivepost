//! §03-shell.md F0.10 Save as — through the window, where the two halves meet.
//!
//! `tests/suite/save_as.rs` pins what [`naivepost::save_as`] decides and `tests/suite/save_as_widgets.rs` pins that
//! the header bar's Save only asks. What neither could pin is the pair of facts this round fixed: that a
//! save writes the session the WINDOW is holding (not the copy it was built from) and that the folder it
//! writes into follows a rename instead of staying wherever the launch happened to be. Both are window
//! state, so both are checked here, through the same door the chooser's callback calls
//! ([`naivepost::ui::save_apply::save_named`]) — the seam `ui::open_folder` is for Open, because no
//! headless run can answer the desktop's folder sheet.

use std::path::{Path, PathBuf};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::project::{self, Project};
use naivepost::save_as::{self, SAVED};
use naivepost::ui;
use crate::common;
use common::temp_dir_created as temp_dir;

/// One running GTK application for this whole binary, entered once and shared by every check below:
/// `g_application_run` may be entered only once per process, so the checks run sequentially inside one
/// `connect_activate` (the shape `tests/suite/f0_6_open_at_start_launch.rs` uses) and each records that it
/// ran, so a check that never happened is a failure rather than a quiet pass.
fn window_round() {
    common::one_app_round(|app| {
        check_live_session_is_what_gets_saved(app);
        RAN_LIVE.store(true, std::sync::atomic::Ordering::SeqCst);
        check_rename_moves_the_window_too(app);
        RAN_RENAME.store(true, std::sync::atomic::Ordering::SeqCst);
        check_refused_move_leaves_the_folder_alone(app);
        RAN_REFUSED_MOVE.store(true, std::sync::atomic::Ordering::SeqCst);
        check_button_refused_during_a_run(app);
        RAN_BUTTON.store(true, std::sync::atomic::Ordering::SeqCst);
    });
}

static RAN_LIVE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static RAN_RENAME: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static RAN_REFUSED_MOVE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static RAN_BUTTON: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn ran(flag: &std::sync::atomic::AtomicBool) -> bool {
    flag.load(std::sync::atomic::Ordering::SeqCst)
}

/// A project folder on disk. One footage source: a page whose prerequisites are missing is bounced
/// back to Prepare when it is switched to, and the checks below need the Cut page to stay where they
/// put it.
fn plant(dir: &Path) -> PathBuf {
    std::fs::create_dir_all(dir).expect("project folder");
    project::save(
        &Project {
            sources: vec![naivepost::project::Source {
                path: "project:sources/talk.mkv".to_string(),
                footage: true,
                ..Default::default()
            }],
            ..Default::default()
        },
        dir,
    )
    .expect("project written");
    dir.to_path_buf()
}

/// The window's status label — the same widget `ui::state` reads, handed over so `save_named`'s
/// sentence lands where a press's sentence lands.
fn status(window: &adw::ApplicationWindow) -> gtk::Label {
    ui::find_status(window.upcast_ref()).expect("the shell has a status line")
}

/// Read the open-folder memory while it belongs to THIS check's window. Every check below that asks the
/// question "did the folder move?" answers it here and hands the answer back, because the slot is one
/// per process and the checks share one application: read after a later window was built, it would
/// name that window's folder instead.
fn opened_now() -> Option<PathBuf> {
    ui::save_apply::open_dir()
}

/// A window whose launch folder IS `root`: everything the shell computes from the working directory
/// (`session.naivepost` beside it, the log's paths) belongs to this check's own temp tree.
///
/// The project under test is planted as that working copy and handed to the window too, so "the folder
/// this window has open" is ONE path rather than two -- which is what Save renames.
fn window_at(app: &adw::Application, root: &Path) -> (adw::ApplicationWindow, PathBuf) {
    let open = naivepost::startup::session_dir(root);
    std::env::set_current_dir(root).expect("cwd pinned to this check's root");
    let model = project::load(&open).expect("the planted working copy loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();
    ui::window::clear_logs_for_test();
    (window, open)
}

/// S4 through the window: what reaches disk is the session the window holds NOW. Before this wiring
/// the handler saved the `&Project` the window was built from, so an edit made after the window opened
/// never reached the file however many times Save was pressed.
fn check_live_session_is_what_gets_saved(app: &adw::Application) {
    let root = temp_dir("f10-live");
    let folder = plant(&naivepost::startup::session_dir(&root));
    let (window, open) = window_at(app, &root);
    assert_eq!(open, folder);

    // An edit made in the window, through the handle every page reads: the narration flag is the
    // simplest thing a person can change after the window opened, and the build-time clone used to lose
    // it however many times Save was pressed.
    let session = ui::session_handle(&window);
    session.borrow_mut().no_narration = true;

    // The save renames the working copy to `jan.naivepost` -- the planted folder has only its project
    // file in it, so nothing is logged as moved -- and what it writes there is the session as the
    // window holds it now.
    let saved = ui::save_apply::save_named(&session, &status(&window), &folder, "jan")
        .expect("a new name saves");
    assert_eq!(saved.status, SAVED);
    assert_eq!(status(&window).text().as_str(), SAVED, "S4's sentence");
    assert_eq!(saved.path, root.join("jan.naivepost"));

    let on_disk = project::load(&saved.path).expect("the saved file reads back");
    assert!(
        on_disk.no_narration,
        "the file holds what the window holds, not what the launch handed over"
    );

    std::fs::remove_dir_all(&root).ok();
}

/// S3 + S4 through the window: the rename moves the folder AND the window's idea of which folder is
/// open, so the next save lands in the new one instead of resurrecting the old path.
fn check_rename_moves_the_window_too(app: &adw::Application) {
    let root = temp_dir("f10-rename");
    let jan = plant(&naivepost::startup::session_dir(&root));
    std::fs::write(jan.join("render.mp4"), b"the whole video").expect("an output to move");
    let (window, _open) = window_at(app, &root);
    let session = ui::session_handle(&window);
    // The folder the window believes is open at its build; the rename has to move THIS memory.
    let was_open = opened_now().expect("a window always names a folder");
    assert_eq!(was_open, jan);

    let feb = root.join("feb.naivepost");
    let saved = ui::save_apply::save_named(&session, &status(&window), &was_open, "feb")
        .expect("a free name renames");
    assert_eq!(saved.path, feb);
    assert!(!jan.exists(), "never a copy: the old folder is gone");
    assert_eq!(
        std::fs::read(feb.join("render.mp4")).unwrap(),
        b"the whole video",
        "the outputs followed the name"
    );
    assert_eq!(
        ui::save_apply::open_dir().as_deref(),
        Some(feb.as_path()),
        "the window now believes the renamed folder is the one it has open"
    );
    assert!(
        ui::window_logs()
            .iter()
            .any(|line| line == &format!(">>> moved the output folder to {}", feb.display())),
        "the move is logged: {:?}",
        ui::window_logs()
    );
    assert_eq!(status(&window).text().as_str(), SAVED);

    // And the SECOND save goes to the new folder: same name as it now is, so nothing moves again and
    // no second `>>> moved` line appears. This is the whole point of remembering the folder — without
    // it this save would write into `jan.naivepost`, which no longer exists.
    ui::window::clear_logs_for_test();
    let again = ui::save_apply::save_named(&session, &status(&window), &feb, "feb")
        .expect("saving under the name it already has just writes the file");
    assert_eq!(again.path, feb);
    assert_eq!(again.logs, Vec::<String>::new(), "nothing moved the second time");
    assert_eq!(status(&window).text().as_str(), SAVED);
    assert!(
        !ui::window_logs()
            .iter()
            .any(|line| line.starts_with(">>> moved")),
        "no move claimed when nothing moved: {:?}",
        ui::window_logs()
    );
    assert!(feb.join(project::PROJECT_FILE).is_file());
    assert!(!jan.exists(), "and the second save did not recreate the old folder");

    std::fs::remove_dir_all(&root).ok();
}

/// S3's refusal through the window: the files stay, the sentence names where they are, and the window
/// keeps believing the ORIGINAL folder is the open one — a move that did not happen must not re-point
/// the session at a folder nothing wrote into.
fn check_refused_move_leaves_the_folder_alone(app: &adw::Application) {
    let root = temp_dir("f10-refused");
    let apr = plant(&naivepost::startup::session_dir(&root));
    std::fs::write(apr.join("render.mp4"), b"the whole video").expect("an output to count");
    let taken = root.join("taken.naivepost");
    std::fs::create_dir_all(&taken).expect("the taken folder");
    std::fs::write(taken.join("somebody-elses.mp4"), b"not ours").expect("work in the way");
    let (window, _open) = window_at(app, &root);
    let session = ui::session_handle(&window);
    // What this window believes is open at its build; the refusal must leave exactly this standing.
    let before_refusal = opened_now();
    let was_open = before_refusal.clone().expect("a window always names a folder");

    let err = ui::save_apply::save_named(&session, &status(&window), &was_open, "taken")
        .expect_err("a folder with work in it is not overwritten");
    assert!(
        err.starts_with("!!! could not move the output folder to "),
        "{err}"
    );
    assert_eq!(
        status(&window).text().as_str(),
        err,
        "the refusal is said, not swallowed"
    );
    assert!(
        ui::window_logs().iter().any(|line| line == &err),
        "and logged, because it names where the files still are: {:?}",
        ui::window_logs()
    );
    assert!(apr.join("render.mp4").is_file(), "the files stayed put");
    assert_eq!(
        fs_read_marker(&taken),
        Some(b"not ours".to_vec()),
        "nothing was written into the folder that was in the way"
    );
    // The refusal wrote no folder memory at all: whatever the window believed before still stands.
    assert_eq!(
        opened_now(),
        before_refusal,
        "a refused move leaves the window on the folder it was already on"
    );

    std::fs::remove_dir_all(&root).ok();
}

fn fs_read_marker(path: &Path) -> Option<Vec<u8>> {
    std::fs::read(path.join("somebody-elses.mp4")).ok()
}

/// S1 through the real button, kept here as well as in `save_as_widgets.rs` because the handler's
/// signature changed: the refusal must still reach the status line from a click, before any chooser.
fn check_button_refused_during_a_run(app: &adw::Application) {
    let root = temp_dir("f10-button");
    let folder = plant(&naivepost::startup::session_dir(&root));
    let (window, seeded) = window_at(app, &root);
    // The refusal must leave the memory where THIS window's build put it, whatever earlier checks did.
    assert_eq!(ui::save_apply::open_dir().as_deref(), Some(seeded.as_path()));

    // A run under way, started the way ▶ starts one (F0.5 S4 closes what a single-step press opens).
    let model = project::load(&folder).expect("the planted working copy loads");
    ui::play_button(&window).expect("the run bar has a ▶").emit_clicked();
    if let (Some(bar), Some(queue)) = (ui::run_bar(&window), ui::run_queue(&window)) {
        naivepost::runqueue::start_run(
            &mut bar.borrow_mut(),
            &mut queue.borrow_mut(),
            &mut naivepost::exchanges::RunLog::new(),
            naivepost::run::Step::Prepare,
            naivepost::run::snapshot_sources(&model),
        );
    }
    ui::save_button(&window)
        .expect("the header bar has a Save button")
        .emit_clicked();
    assert_eq!(ui::state(&window).status, save_as::RUN_REFUSAL);
    assert!(
        ui::window_logs().iter().any(|line| line == save_as::RUN_REFUSAL),
        "the refusal is logged too: {:?}",
        ui::window_logs()
    );
    // Refused before the chooser, so nothing about the folder changed: the memory still names the
    // folder this window was built on. Read back from the build rather than recomputed, because the
    // slot is one per process and earlier checks in this binary moved it.
    assert_eq!(
        ui::save_apply::open_dir().as_deref(),
        Some(seeded.as_path()),
        "a refused save does not move the window's folder"
    );

    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn f0_10_s4_a_save_writes_the_live_session_not_the_one_the_window_opened_with() {
    window_round();
    assert!(ran(&RAN_LIVE), "the live-session check never ran");
}

#[test]
fn f0_10_s3_a_rename_moves_the_folder_and_the_window_follows_it() {
    window_round();
    assert!(ran(&RAN_RENAME), "the rename check never ran");
}

#[test]
fn f0_10_s3_a_refused_move_says_where_the_files_stay_and_leaves_the_folder_alone() {
    window_round();
    assert!(ran(&RAN_REFUSED_MOVE), "the refused-move check never ran");
}

#[test]
fn f0_10_s1_the_save_button_is_still_refused_during_a_run() {
    window_round();
    assert!(ran(&RAN_BUTTON), "the button-refusal check never ran");
}
