//! §03-shell.md §4 Sources list — the LOAD side of the cleaning rule.
//!
//! The rules themselves live in [`naivepost::sources::clean_on_load`] and are pinned by
//! `tests/suite/sources_list.rs`. What that file could not pin is that the PROGRAM calls it: §4 says
//! "a loaded project with two rows in one slot or footage on an audio file has the offending flag
//! cleared on load", and the strip is one of the four moments §4 names for auto-filling slot 1 ("after
//! … a load that stripped a bad tag"). Both doors that take a project on — `open_project::apply` behind
//! the Open button, and `ui::launch::load_for_launch` at start — go through it now, so these tests
//! drive those doors rather than the rule.
//!
//! Which clause each test pins:
//!
//! * "…has the offending flag cleared on load" + "Slot 1 auto-filled whenever unheld … after … a load
//!   that stripped a bad tag" — `…_a_load_clears_a_duplicated_slot_and_fills_slot_1`.
//! * "A loaded project … keeps whatever the user left in slot 1" (no strip, no report) —
//!   `…_a_clean_load_keeps_what_the_user_left_in_slot_1`.
//! * The same through the shell's own door and the launch's —
//!   `…_the_open_button_clears_what_the_file_could_not_mean`.

use std::fs;
use std::path::{Path, PathBuf};

use crate::common;
use adw::prelude::*;
use common::source_row as row;
use gtk4 as gtk;
use naivepost::project::Project;
use naivepost::{open_project, sources, ui};

/// The item's id, interpolated into every assertion message so a failure names the item.
const ITEM: &str =
    "sec_03_shell_4_sources_list_lives_on_prepare_specified_here_because_the_shell_snapshots_it";

/// §4's sentence for the auto-fill, spelled once here and asserted nowhere else so the wording cannot
/// drift without a red test.
const SLOT_1_SEATED: &str = "is the narrator now -- slot 1 was empty";

/// A real `.naivepost` folder whose `naivepost.json` lists `rows`, with a file on disk for each so the
/// drop rule keeps every row and only the FLAGS are wrong. Written under the caller's own temp dir.
fn dirty_folder(dir: &Path, rows: &[naivepost::project::Source]) -> PathBuf {
    let folder = dir.join("dirty.naivepost");
    fs::create_dir_all(folder.join("sources")).expect("sources folder");
    for source in rows {
        let name = source.path.rsplit('/').next().unwrap_or(&source.path);
        fs::write(folder.join("sources").join(name), "bytes").expect("source file");
    }
    let project = Project {
        sources: rows.to_vec(),
        ..Default::default()
    };
    naivepost::project::save(&project, &folder).expect("the project writes");
    folder
}

/// Two rows claiming slot 1, a slot outside the range the narrator button can reach, and footage on an
/// audio file: everything §4 says a load must clear, spelled in a project file.
fn contradictory_rows() -> Vec<naivepost::project::Source> {
    vec![
        row("project:sources/cam.wav", false, 1),
        row("project:sources/second.wav", false, 1),
        row("project:sources/wild.mkv", false, 9),
        row("project:sources/mic.wav", true, 0),
    ]
}

/// §4: a load clears what the file cannot mean, says each clearing, and fills the slot the strip
/// emptied. Driven through `open_project::apply`, the ONE door that decides what a load keeps.
#[test]
fn sec_03_shell_4_sources_list_lives_on_prepare_specified_here_because_the_shell_snapshots_it_a_load_clears_a_duplicated_slot_and_fills_slot_1(
) {
    common::pin_private_cwd();
    let root = common::temp_dir_created("clean-apply");
    let folder = dirty_folder(&root, &contradictory_rows());

    let applied = open_project::apply(&folder, |path| path.is_file())
        .expect("a hand-edited project still opens");
    let project = &applied.project;

    // The second claimant loses its tag; the first holder keeps it (§4: one row per narrator slot).
    assert_eq!(
        project.sources[1].narrator, 0,
        "{ITEM}: two rows cannot hold slot 1"
    );
    // A slot past the last one is not honoured either.
    assert_eq!(
        project.sources[2].narrator, 0,
        "{ITEM}: slot 9 is past the last slot"
    );
    // Only a video may be footage: the audio row loses the flag, not the row.
    assert!(
        !project.sources[3].footage,
        "{ITEM}: an audio file has no frames"
    );
    assert_eq!(
        sources::slot_holder(project, 1),
        Some(0),
        "{ITEM}: slot 1 stays held"
    );

    // Each clearing is REPORTED -- silently losing a tag someone typed is worse than a line about it --
    // and the lines ride `Applied.lines`, which is what the shell logs on an open.
    for said in [
        "second.wav: slot 1 is taken -- untagged",
        "wild.mkv: slot 9 is taken -- untagged",
        "mic.wav: an audio file has no frames -- not footage",
    ] {
        assert!(
            applied.lines.iter().any(|line| line.contains(said)),
            "{ITEM}: the load never said {said:?}: {:?}",
            applied.lines
        );
    }
    fs::remove_dir_all(&root).ok();
}

/// The other half of §4's sentence: a project that loads clean is not talked to, and whatever the user
/// left in slot 1 stays there -- the auto-fill answers a STRIP, not every load.
#[test]
fn sec_03_shell_4_sources_list_lives_on_prepare_specified_here_because_the_shell_snapshots_it_a_clean_load_keeps_what_the_user_left_in_slot_1(
) {
    common::pin_private_cwd();
    let root = common::temp_dir_created("clean-quiet");
    let rows = vec![
        row("project:sources/cam.mkv", true, 1),
        row("project:sources/mic.wav", false, 2),
    ];
    let folder = dirty_folder(&root, &rows);

    let applied =
        open_project::apply(&folder, |path| path.is_file()).expect("a good project opens");

    assert_eq!(
        applied.project.sources[0].narrator, 1,
        "{ITEM}: the holder stays"
    );
    assert_eq!(
        applied.project.sources[1].narrator, 2,
        "{ITEM}: the second slot stays"
    );
    assert!(
        applied.lines.is_empty(),
        "{ITEM}: a clean load has nothing to report: {:?}",
        applied.lines
    );
    fs::remove_dir_all(&root).ok();
}

/// Seat a narrator when the load itself emptied slot 1 by dropping its holder.
///
/// §4's auto-fill is not tied to a strip alone: "Slot 1 auto-filled whenever unheld -- after an add, a
/// removal, a rescan that dropped a row, or a load that stripped a bad tag". `apply` drops missing rows
/// before it cleans, so a project whose slot-1 file has gone loses the narrator with nothing stripped, and
/// the session would open with nobody to speak in. This test drops the holder and checks another present,
/// untagged recording takes the slot -- and says so on the load's own report.
#[test]
fn sec_03_shell_4_sources_list_lives_on_prepare_specified_here_because_the_shell_snapshots_it_a_load_that_dropped_the_narrator_seats_another_one(
) {
    common::pin_private_cwd();
    let root = common::temp_dir_created("clean-dropped-slot1");
    // A real folder whose slot-1 holder has NO file on disk (so `apply`'s drop rule removes it), plus a
    // present untagged recording and a present footage row: §4 seats the recording, footage last.
    let folder = root.join("gone.naivepost");
    fs::create_dir_all(folder.join("sources")).expect("sources folder");
    for name in ["mic.wav", "cam.mkv"] {
        fs::write(folder.join("sources").join(name), "bytes").expect("source file");
    }
    let project = Project {
        sources: vec![
            row("project:sources/old.wav", false, 1),
            row("project:sources/mic.wav", false, 0),
            row("project:sources/cam.mkv", true, 0),
        ],
        ..Default::default()
    };
    naivepost::project::save(&project, &folder).expect("the project writes");

    let applied = open_project::apply(&folder, |path| path.is_file())
        .expect("a project with one missing source still opens");

    assert_eq!(
        applied.dropped,
        ["project:sources/old.wav"],
        "{ITEM}: only the missing row goes"
    );
    assert_eq!(
        sources::slot_holder(&applied.project, 1),
        Some(0),
        "{ITEM}: the load left slot 1 unheld -- §4 fills it whenever it is empty"
    );
    assert_eq!(
        applied.project.sources[0].path, "project:sources/mic.wav",
        "{ITEM}: a recording is seated before footage"
    );
    assert!(
        applied
            .lines
            .iter()
            .any(|line| line.contains(SLOT_1_SEATED)),
        "{ITEM}: the load never said who took slot 1: {:?}",
        applied.lines
    );
    // The SAME rule through the launch's door, which is the other half of "a load": it takes a project on
    // exactly as the button does, so its own drop must seat a narrator too.
    let (launched, report) = ui::launch::load_for_launch(&folder);
    assert_eq!(
        sources::slot_holder(&launched, 1),
        Some(0),
        "{ITEM}: the launch dropped the holder and left slot 1 empty"
    );
    assert!(
        report.iter().any(|line| line.contains(SLOT_1_SEATED)),
        "{ITEM}: the launch never filled the slot its own drop emptied: {report:?}"
    );
    fs::remove_dir_all(&root).ok();
}

/// The launch's door takes a project on exactly as the button does, so it cleans identically: same
/// flags gone, same sentences in the report the launch logs.
#[test]
fn sec_03_shell_4_sources_list_lives_on_prepare_specified_here_because_the_shell_snapshots_it_a_launch_reopen_clears_the_same_flags(
) {
    common::pin_private_cwd();
    let root = common::temp_dir_created("clean-launch");
    let folder = dirty_folder(&root, &contradictory_rows());

    let (project, report) = ui::launch::load_for_launch(&folder);

    assert_eq!(
        project.sources[1].narrator, 0,
        "{ITEM}: the launch strips a duplicated slot"
    );
    assert!(
        !project.sources[3].footage,
        "{ITEM}: the launch strips footage off an audio file"
    );
    assert_eq!(
        sources::slot_holder(&project, 1),
        Some(0),
        "{ITEM}: slot 1 stays held"
    );
    assert!(
        report
            .iter()
            .any(|line| line.contains("an audio file has no frames -- not footage")),
        "{ITEM}: the launch says what it cleared: {report:?}"
    );
    fs::remove_dir_all(&root).ok();
}

// --- the wire: the Open button's own body -------------------------------------

static RAN_OPEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Round the window once: the checks run inside one `#[gtk::test]` because GTK drives its single
/// thread, which is how every widget round in this suite works.
#[gtk::test]
fn sec_03_shell_4_sources_list_lives_on_prepare_specified_here_because_the_shell_snapshots_it_the_open_button_clears_what_the_file_could_not_mean(
) {
    let app = adw::Application::builder()
        .application_id(format!("org_naivepost.clean_load_{}", std::process::id()))
        .build();
    app.register(None::<&gio::Cancellable>).expect("register");
    open_through_the_open_button(&app);
    assert!(
        RAN_OPEN.load(std::sync::atomic::Ordering::SeqCst),
        "the Open-button check never ran"
    );
}

/// Press Open for real (the header-bar button, fired the way the toolkit fires it) to prove the wire is
/// live, then hand the SAME folder to the body the chooser's callback calls (`ui::open_folder`) -- the
/// desktop's sheet cannot be typed into headlessly, and every other Open check here reaches it the same
/// way. The assertions land on the window's own session, the state a run snapshots.
fn open_through_the_open_button(app: &adw::Application) {
    let root = common::temp_dir_created("clean-open");
    let folder = dirty_folder(&root, &contradictory_rows());
    let model = Project::default();
    let window = ui::build_window(app, &model, "Prepare");
    window.present();

    // The button exists and is wired: pressing it puts up the folder chooser the answer comes from.
    ui::open_button(&window)
        .expect("the header bar has an Open button")
        .emit_clicked();
    assert!(
        ui::window::last_open_ask_for_test().is_some(),
        "{ITEM}: the Open press did not put up its chooser"
    );

    let bar = ui::run_bar(&window).expect("the window carries a run bar");
    let status = ui::find_status(window.upcast_ref()).expect("the window has a status line");
    let session = ui::session_handle(&window);

    let opened = ui::open_folder(&bar, &status, &session, &folder);
    assert!(
        opened,
        "{ITEM}: opening a hand-edited project still succeeds"
    );

    // The live session -- what ▶ freezes as the run's snapshot -- holds the CLEANED rows.
    let held = session.borrow();
    assert_eq!(
        held.sources.len(),
        4,
        "{ITEM}: nothing was dropped, only flagged wrong"
    );
    assert_eq!(
        held.sources[1].narrator, 0,
        "{ITEM}: the duplicate slot survived the open"
    );
    assert_eq!(
        held.sources[2].narrator, 0,
        "{ITEM}: the out-of-range slot survived the open"
    );
    assert!(
        !held.sources[3].footage,
        "{ITEM}: footage survived on an audio file"
    );
    assert_eq!(
        sources::slot_holder(&held, 1),
        Some(0),
        "{ITEM}: slot 1 was left unheld after a strip"
    );
    drop(held);
    // And the shell said what it changed: the strip lines went to the log the open writes.
    let log = ui::window::logs_for_test();
    assert!(
        log.iter()
            .any(|line| line.contains("slot 1 is taken -- untagged")),
        "{ITEM}: the open never logged the strip: {log:?}"
    );

    fs::remove_dir_all(&root).ok();
    window.close();
    RAN_OPEN.store(true, std::sync::atomic::Ordering::SeqCst);
}
