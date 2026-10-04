//! F1.1 §S7 and §S8 — how a ▶ Prepare press ENDS (spec/04-prepare.md §F1.1 S7/S8).
//!
//! §S7 gives the three endings their exact words and §S8 says what the window does after them: refresh
//! the page, the gates and the Cut page, and hand the audio unload over. The rules live in
//! [`naivepost::prepare_run`] (`Ending`, `ending_of`, `ending_lines`, `ending_status`) and the window
//! half in [`naivepost::ui::prepare_guard`] — so each case here asks the same question the person at the
//! screen asks: what the log says, what the status row ends with, and how far the bar got.
//!
//! No case here writes a fake sentence: every expected string is either quoted from §S7 or read back
//! from the door that produced it, which is what makes a wording change fail here rather than silently.

use std::fs;
use std::path::PathBuf;

use adw::prelude::*;
use naivepost::layout::Tree;
use naivepost::prepare_run::{self, Ending};
use naivepost::project::{Project, Source};
use naivepost::run_stages::Drive;
use naivepost::ui;

#[allow(dead_code)] // every test binary compiles this whole module; an unused helper is not a warning here
mod common;
use common::{fixture_dir, status_text};

/// A scratch project folder with `prepare/` filled by `files`, each path relative to the project root.
/// The files are real because §S7's count is a count OF DISK, taken through the same door the Outputs
/// row uses (`prepare::outputs_count`) — an invented number here would pass against a rule that counts
/// something else.
fn planted(tag: &str, files: &[&str]) -> (PathBuf, Tree) {
    let root = std::env::temp_dir().join(format!("naivepost-f11-s7-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let dir = root.join(format!("{tag}.naivepost"));
    fs::create_dir_all(&dir).expect("project folder");
    let tree = Tree::new(&dir).expect("a .naivepost folder is a project");
    for file in files {
        let path = dir.join(file);
        fs::create_dir_all(path.parent().expect("a file has a folder")).unwrap();
        fs::write(&path, "x").expect("plant the file the run wrote");
    }
    (root, tree)
}

fn project() -> Project {
    Project {
        sources: vec![Source {
            path: "project:sources/lecture.mkv".to_string(),
            footage: true,
            narrator: 0,
            sepvoice: false,
            tracks: vec![],
        }],
        ..Project::default()
    }
}

/// §S7: the log names the three folders and the status counts what is on disk — the same number the
/// Outputs row shows, because `ending_of` takes it from `prepare::outputs_count`.
#[test]
fn f1_1_s7_a_finished_run_says_prepared_with_the_file_count() {
    let (_root, tree) = planted(
        "done",
        &[
            "prepare/inputs/lecture/meta.env",
            "prepare/inputs/lecture/audio.wav",
            "prepare/describe/lecture/events.tsv",
            "prepare/transcript/lecture.tsv",
            "prepare/transcript/lecture.words",
        ],
    );
    let drive = Drive::Done { ran: 3, skipped: 1 };
    let ending = prepare_run::ending_of(&drive, &tree);
    assert_eq!(ending, Ending::Prepared { files: 5 }, "five files under prepare/");

    // The log: §S7's ">>> prepare wrote:" plus one line per folder. Three folders, four lines.
    let lines = prepare_run::ending_lines(&ending);
    assert_eq!(
        lines,
        vec![
            ">>> prepare wrote:",
            ">>>   prepare/inputs/",
            ">>>   prepare/describe/",
            ">>>   prepare/transcript/",
        ],
        "the three folders §S7 names, verbatim"
    );
    // The status: the count, in the same sentence `finished_status` writes.
    assert_eq!(
        prepare_run::ending_status(&ending),
        "prepared \u{2014} 5 files",
        "and the number the Outputs row shows: {:?}",
        naivepost::prepare::outputs_count(&tree)
    );
    assert_eq!(
        prepare_run::ending_status(&ending),
        prepare_run::finished_status(5),
        "one sentence, written once"
    );
}

/// §S7: a run that counted nothing still finishes. `prepared — 0 files` is the honest sentence, and
/// the log is unchanged: what the run wrote does not depend on how much it wrote.
#[test]
fn f1_1_s7_a_finished_run_with_nothing_on_disk_says_zero() {
    let (_root, tree) = planted("empty", &[]);
    let ending = prepare_run::ending_of(&Drive::Done { ran: 0, skipped: 0 }, &tree);
    assert_eq!(ending, Ending::Prepared { files: 0 });
    assert_eq!(prepare_run::ending_status(&ending), "prepared \u{2014} 0 files");
}

/// §S7: ⏹ is neither a failure nor a success. The status says the finished work is kept, and the log
/// gets no new line: `run_stages::say_outcome` already wrote where the walk stopped.
#[test]
fn f1_1_s7_a_stopped_run_says_finished_work_is_kept() {
    let (_root, tree) = planted("stopped", &["prepare/inputs/lecture/meta.env"]);
    let ending = prepare_run::ending_of(&Drive::Stopped { ran: 2, left: 5 }, &tree);
    assert_eq!(ending, Ending::Stopped, "a stopped walk never counts files");
    assert_eq!(
        prepare_run::ending_status(&ending),
        "stopped \u{2014} finished work is kept",
        "§S7's sentence, and NOT a failure: it says nothing about FAILED"
    );
    assert!(
        !prepare_run::ending_status(&ending).contains("FAILED"),
        "a stop is not a failure"
    );
    assert!(
        prepare_run::ending_lines(&ending).is_empty(),
        "the walk already logged the stop; the ending adds no second line"
    );
}

/// §S7: a failed run names its reason in the log and points at the log on the status row. The reason
/// is the walk's own (§Failure-is-specific), so the sentence is `failed_status`'s, not a new one.
#[test]
fn f1_1_s7_a_failed_run_names_the_reason_and_points_at_the_log() {
    let (_root, tree) = planted("failed", &["prepare/inputs/lecture/meta.env"]);
    let drive = Drive::Failed {
        stage: "speech",
        error: "audio.cpp went away mid-take".to_string(),
    };
    let ending = prepare_run::ending_of(&drive, &tree);
    assert_eq!(
        ending,
        Ending::Failed { reason: "audio.cpp went away mid-take".to_string() },
        "the walk's reason, carried through untouched"
    );
    assert_eq!(
        prepare_run::ending_lines(&ending),
        vec!["prepare FAILED: audio.cpp went away mid-take"],
        "the log says what failed, with the §S7 marker"
    );
    assert_eq!(
        prepare_run::ending_status(&ending),
        "prepare FAILED: audio.cpp went away mid-take"
    );
    // And the reason the test insists on: a failure that names nothing still says where to look.
    let blank = Ending::Failed { reason: "  ".to_string() };
    assert_eq!(
        prepare_run::ending_status(&blank),
        "prepare failed \u{2014} see log",
        "a red run is never a blank row"
    );
}

// ---- the window half: §S7 on screen, §S8's refresh ---------------------------------------------

/// One application, one window, then the three endings driven through the window's own doors.
///
/// `drive_page` runs inside the ▶ handler, so a test cannot hand it a Drive; what it CAN do is call
/// the door the handler calls, with each of the three outcomes — which is how the bar's fraction, the
/// status row and the log are all reached for real.
/// One application for this binary: `g_application_run` refuses a second claimant of the default main
/// context, so every window case below runs inside ONE `activate`, in order.
fn window_round() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(|app| {
            check_finished_run_on_screen(app);
            RAN_DONE.store(true, std::sync::atomic::Ordering::SeqCst);
            check_stopped_run_on_screen(app);
            RAN_STOPPED.store(true, std::sync::atomic::Ordering::SeqCst);
            check_the_close_hands_the_unload_over(app);
            RAN_UNLOAD.store(true, std::sync::atomic::Ordering::SeqCst);
            app.quit();
        });
        app.run_with_args::<String>(&[]);
    });
}

static RAN_DONE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static RAN_STOPPED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static RAN_UNLOAD: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// §S7 + §S8 on the real window: the ending's lines reach the log, its sentence ends the status row,
/// the bar finishes at 1, and the Outputs readout has moved to the count the run wrote.
fn check_finished_run_on_screen(app: &adw::Application) {
    let (root, tree) = planted(
        "window-done",
        &[
            "prepare/inputs/lecture/meta.env",
            "prepare/describe/lecture/events.tsv",
            "prepare/transcript/lecture.tsv",
        ],
    );
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.set_default_size(1400, 900);
    window.present();
    common::settle();
    // The window's own folder for this ending: the door takes the tree it counts from, so the number
    // below and the readout are about ONE folder.
    naivepost::ui::prepare_guard::say_ending_in_for_test(
        &Drive::Done { ran: 4, skipped: 0 },
        &window,
        &project(),
        tree.dir(),
    );
    common::settle();

    let logs = ui::window_logs();
    assert!(
        logs.contains(&">>> prepare wrote:".to_string()),
        "§S7's success line is in the log: {logs:?}"
    );
    assert_eq!(
        status_text(&window),
        "prepared \u{2014} 3 files",
        "the ending is the status row's last word"
    );
    assert_eq!(
        naivepost::ui::progress_bar(&window).expect("the window carries a progress bar").fraction(),
        1.0,
        "§S7: a finished run leaves the bar at 1"
    );
    // §S8: the readout the run was supposed to refresh shows the new count, not the launch's. The
    // row's own wording is `Shell::outputs`' ("Prepare: N", §04 #1 badge 6), so what is pinned here is
    // the NUMBER and that it moved with the run — the sentence shape belongs to the shell's test.
    let readout = ui::outputs_readout(&window)
        .expect("the shell carries the Outputs readout")
        .text()
        .to_string();
    assert!(
        readout.ends_with("3") && readout.contains("Prepare"),
        "§S8: the Outputs row sees what the run wrote, not the stale count: {readout:?}"
    );
    // The queue is emptied by the close a real press runs; the bar's own copy still holds the sentence.
    let bar = ui::run_bar(&window).expect("the window carries a run bar");
    assert_eq!(bar.borrow().status, "prepared \u{2014} 3 files");
    window.close();
    let _ = fs::remove_dir_all(&root);
}

/// §S7 through the window: a stopped run's sentence lands on the row and leaves the bar short of 1 —
/// only a finished run is allowed to fill it.
fn check_stopped_run_on_screen(app: &adw::Application) {
    let (root, tree) = planted("window-stopped", &["prepare/inputs/lecture/meta.env"]);
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.set_default_size(1400, 900);
    window.present();
    common::settle();
    naivepost::ui::prepare_guard::say_ending_in_for_test(
        &Drive::Stopped { ran: 1, left: 3 },
        &window,
        &project(),
        tree.dir(),
    );
    common::settle();
    assert_eq!(
        status_text(&window),
        "stopped \u{2014} finished work is kept",
        "§S7's stop sentence, on the row the person reads last"
    );
    let bar = ui::run_bar(&window).expect("the window carries a run bar");
    assert_eq!(bar.borrow().status, "stopped \u{2014} finished work is kept");
    window.close();
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn f1_1_s7_the_endings_reach_the_status_row_and_the_log() {
    window_round();
    assert!(RAN_DONE.load(std::sync::atomic::Ordering::SeqCst));
    assert!(RAN_STOPPED.load(std::sync::atomic::Ordering::SeqCst));
}

/// Poll `ask` until it answers with something, or give up after ~5 s. The unload runs on its own
/// thread by design (§S4 "off-thread"), so a check for its effect waits for a thread.
fn wait_for<T>(ask: impl Fn() -> Option<T>) -> Option<T> {
    for _ in 0..250 {
        if let Some(found) = ask() {
            return Some(found);
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    None
}

/// §S8's second half, on the real window: closing the run hands the audio unload over — asserted
/// through the door the unload thread reports into, so it is the program's own hand-off (§S4, which
/// §S8 reuses) rather than a claim about a comment.
fn check_the_close_hands_the_unload_over(app: &adw::Application) {
    naivepost::ui::run_close::clear_log_for_test();
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.set_default_size(1400, 900);
    window.present();
    common::settle();
    let (bar, queue) = (
        ui::run_bar(&window).expect("the window carries a run bar"),
        ui::run_queue(&window).expect("the window carries a run queue"),
    );
    let mut exchange = naivepost::exchanges::RunLog::new();
    naivepost::runqueue::start_run(
        &mut bar.borrow_mut(),
        &mut queue.borrow_mut(),
        &mut exchange,
        naivepost::run::Step::Prepare,
        naivepost::run::snapshot_sources(&project()),
    );
    naivepost::ui::run_close::note_started_run(&bar);
    let status = ui::find_status(window.upcast_ref()).expect("the shell has a status line");
    let progress = ui::progress_bar(&window).expect("the window carries a progress bar");
    let log = ui::log_expander(&window).expect("the window carries the log expander");
    // The view is the expander's own child, which is what `close_step_run` repaints.
    let view: gtk4::TextView = log
        .child()
        .and_then(|child| child.downcast().ok())
        .expect("the log expander holds its text view");
    naivepost::ui::run_close::close_step_run(
        &bar,
        &queue,
        &std::rc::Rc::new(std::cell::RefCell::new(exchange)),
        &status,
        &progress,
        &log,
        &view,
        None,
    );
    // The unload is off-thread by design (20 s ceiling, never fails the run), so wait for its report
    // rather than assuming it already landed.
    naivepost::settings_probe::drain_unload(std::time::Duration::from_secs(5));
    let line = wait_for(naivepost::ui::run_close::unload_line_for_test).unwrap_or_default();
    assert!(
        !line.is_empty(),
        "§S8: closing the run reported its audio housekeeping — the unload really went out"
    );
    window.close();
}

#[test]
fn f1_1_s8_the_run_hands_the_audio_unload_over() {
    window_round();
    assert!(RAN_UNLOAD.load(std::sync::atomic::Ordering::SeqCst));
}
