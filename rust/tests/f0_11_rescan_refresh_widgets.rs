//! §03-shell.md F0.11 Rescan — the refresh half, through the real ⟳ button.
//!
//! `tests/rescan.rs` pins what [`naivepost::rescan`] decides; `tests/rescan_widgets.rs` pins that a
//! click names the dropped row and says "rescanned". What neither could pin is S2's second node —
//! "refresh every tab's readouts · Cut rebuilds · Narrate re-reads its file" — because the window used
//! to do those repaints only when the current page had become locked, so on a normal press the readouts
//! kept counting the dropped file, the Cut page kept the old clip list and Narrate kept lines whose
//! recording is gone. Every check below therefore presses ⟳ while the page stays put (both sources are
//! footage, so nothing vanishes out from under it) and asserts the screen moved.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut::{self, Cut, Seg};
use naivepost::layout::Tree;
use naivepost::narration::{self, Entry, Narration};
use naivepost::project::{self, Project, Source};
use naivepost::rescan::RESCANNED;
use naivepost::shell::Page;
use naivepost::ui;
#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;

/// One application for the whole binary, entered once; the checks run sequentially inside its single
/// `activate` (GTK allows one `g_application_run` per process) and each records that it finished.
fn window_round() {
    common::one_app_round(|app| {
        check_readouts_refreshed(app);
        RAN_READOUTS.store(true, Ordering::SeqCst);
        check_cut_rebuilt(app);
        RAN_CUT.store(true, Ordering::SeqCst);
        check_narrate_reread(app);
        RAN_NARRATE.store(true, Ordering::SeqCst);
        check_pruned_list_written_back(app);
        RAN_WRITEBACK.store(true, Ordering::SeqCst);
        check_slot_1_persisted(app);
        RAN_SLOT1.store(true, Ordering::SeqCst);
    });
}

static RAN_READOUTS: AtomicBool = AtomicBool::new(false);
static RAN_CUT: AtomicBool = AtomicBool::new(false);
static RAN_NARRATE: AtomicBool = AtomicBool::new(false);
static RAN_WRITEBACK: AtomicBool = AtomicBool::new(false);
static RAN_SLOT1: AtomicBool = AtomicBool::new(false);

/// The session folder of a check's own root. `startup::session_dir(cwd)` is what the shell resolves
/// the session to, so every check pins cwd to a root named after ITSELF: a pid-only name is shared by
/// every test binary of one `cargo test` run, and a sibling that renames cwd's folder (F0.10's Save As
/// does exactly that) would move this folder out from under the press.
fn session_folder(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("np-f11-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("temp root");
    std::env::set_current_dir(&root).expect("cwd pinned to this check's root");
    naivepost::startup::session_dir(&root)
}

fn source(path: &str, footage: bool, narrator: u32) -> Source {
    Source {
        path: path.to_string(),
        footage,
        narrator,
        sepvoice: true,
        tracks: vec![],
    }
}

/// A session folder naming two FOOTAGE rows — one whose file exists, one already deleted — plus a cut
/// and a narration on disk. Footage matters: with both rows marked, no page becomes locked when the
/// dead one drops, which is the case the refresh used to miss.
fn plant(name: &str) -> (PathBuf, Tree) {
    let dir = session_folder(name);
    fs::create_dir_all(dir.join("sources")).expect("sources/");
    fs::write(dir.join("sources/talk.mkv"), b"video").expect("the surviving file is real");
    // `gone.mkv` is deliberately NOT written: it is the row the scan must drop.
    // Frames and a transcript on the row that is ABOUT TO BE DROPPED: only a counted lane belonging to
    // `gone.mkv` makes the number move when the scan drops it (the surviving row's files stay counted,
    // and with nothing counted at all the row reads 0 whatever the session names).
    let inputs = dir.join("prepare").join("inputs");
    let frames = inputs.join("frames").join("gone");
    fs::create_dir_all(&frames).expect("frames/");
    for index in 0..3 {
        fs::write(frames.join(format!("frame{index}.png")), b"png").expect("a counted frame");
    }
    let lane = inputs.join("gone");
    fs::create_dir_all(&lane).expect("the lane's input folder");
    fs::write(lane.join("transcript.tsv"), "one\ntwo\n").expect("a counted transcript");
    project::save(
        &Project {
            sources: vec![
                source("project:sources/talk.mkv", true, 1),
                source("project:sources/gone.mkv", true, 0),
            ],
            ..Default::default()
        },
        &dir,
    )
    .expect("session written");
    let tree = Tree::new(&dir).expect("a .naivepost folder is a project");
    cut::save(
        &Cut {
            segs: vec![
                Seg { s: 0.0, e: 4.0, ..Default::default() },
                Seg { s: 4.0, e: 9.0, cam: 1, ..Default::default() },
            ],
            ..Default::default()
        },
        &tree,
    )
    .expect("cut on disk");
    narration::save(
        &Narration {
            entries: vec![
                Entry { s: 0.0, e: 4.0, text: "line one".into(), ..Default::default() },
                Entry { s: 4.0, e: 9.0, text: "line two".into(), ..Default::default() },
            ],
            silent: vec![],
        },
        &tree,
    )
    .expect("narration on disk");
    (dir, tree)
}

/// A window built over that session and showing the Cut tab: the page STAYS there through the press,
/// which is what makes these checks about the refresh rather than about F0.1's silent bounce.
fn cut_window(app: &adw::Application, dir: &Path) -> adw::ApplicationWindow {
    let window = prepare_window(app, dir);
    ui::tab_button(&window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_by_name::<()>("clicked", &[]);
    assert_eq!(ui::state(&window).page, Page::Cut, "the check starts on Cut");
    window
}

/// The same, left on Prepare -- the page whose readout row counts the session's own rows.
fn prepare_window(app: &adw::Application, dir: &Path) -> adw::ApplicationWindow {
    let model = project::load(dir).expect("the planted session loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();
    ui::window::clear_logs_for_test();
    window
}

fn press_rescan(window: &adw::ApplicationWindow) {
    ui::rescan_button(window)
        .expect("the header bar has a Rescan button")
        .emit_clicked();
}

fn inputs_text(window: &adw::ApplicationWindow) -> String {
    ui::inputs_readout(window)
        .expect("the bottom row carries the Inputs readout")
        .text()
        .to_string()
}

/// Close the window a check built BEFORE its folder goes away.
///
/// A window's close hook flushes the session into `session.naivepost` beside the cwd (F0.5 §2 S6), so a
/// check that deletes its project folder and leaves the window standing hands the next check in this
/// process a flush that rewrites -- or recreates -- a folder it no longer owns. Every check here closes
/// its own window for that reason, and each names its own root, so no sibling can be affected either way.
fn retire(window: adw::ApplicationWindow) {
    window.close();
    glib::MainContext::default().iteration(false);
}

/// Row count of the Narrate page's line list: the same walk `narrate_page`'s own row-selection uses,
/// with the empty-state placeholder excluded because it is not a line.
fn narrate_row_count(window: &adw::ApplicationWindow) -> usize {
    let list = common::widget_in(window, "narrate-lines")
        .and_then(|w| w.downcast::<gtk::ListBox>().ok())
        .expect("the Narrate page carries its line list");
    (0..).take_while(|i| list.row_at_index(*i).is_some()).count()
}

/// S2 clause 1: the readouts come off the refreshed session, on a press where nothing vanished.
fn check_readouts_refreshed(app: &adw::Application) {
    let (dir, _tree) = plant("f11-readouts");
    // A PREPARE window: its readout row counts the session's files, so the drop is visible as a number
    // this widget carries before and after one press. The Cut page's row counts clips, which a scan does
    // not change, so it could not show S2's first clause at all.
    let window = prepare_window(app, &dir);
    assert_eq!(
        inputs_text(&window),
        "Inputs: 3 frames \u{2192} 1 vision \u{b7} 2 lines \u{2192} 1 fixer",
        "before: the dropped row's frames and lines are counted"
    );

    press_rescan(&window);

    assert_eq!(ui::state(&window).status, RESCANNED, "S3 said it whatever the scan found");
    assert_eq!(
        ui::state(&window).page,
        Page::Prepare,
        "the page stayed: this was no vanished-bounce"
    );
    assert!(
        ui::window_logs()
            .iter()
            .any(|line| line == "!!! dropped project:sources/gone.mkv -- it is no longer there"),
        "S1 named the dropped row: {:?}",
        ui::window_logs()
    );
    // After: the count went to zero because every counted file belonged to the lane the scan just
    // dropped -- the SAME widget carried three frames before the press. That is S2's first clause: a
    // normal press, with no page becoming locked, still moved the number. The write-back check reads
    // the file.
    assert_eq!(
        inputs_text(&window),
        "Inputs: 0 frames \u{2192} 0 vision \u{b7} 0 lines \u{2192} 0 fixer",
        "the readout followed the drop"
    );
    retire(window);
    fs::remove_dir_all(naivepost::startup::session_dir(&std::env::current_dir().unwrap())).ok();
}

/// S2 clause 2: the Cut page shows the cut on disk, not the stale one the window held.
fn check_cut_rebuilt(app: &adw::Application) {
    let (dir, _tree) = plant("f11-cut");
    let window = cut_window(app, &dir);
    // Stale state, seeded the way a review press would leave it: three segs that no file on disk has.
    ui::seed_review_cut(
        &window,
        &Cut {
            segs: vec![
                Seg { s: 0.0, e: 1.0, ..Default::default() },
                Seg { s: 1.0, e: 2.0, ..Default::default() },
                Seg { s: 2.0, e: 3.0, ..Default::default() },
            ],
            ..Default::default()
        },
    );
    assert_eq!(ui::review_cut_segs_count(&window), 3, "the stale cut is what the window holds");

    press_rescan(&window);

    let held = ui::review_cut_segs(&window);
    assert_eq!(held.len(), 2, "the strip now paints the cut that is on disk");
    assert_eq!((held[0].s, held[0].e), (0.0, 4.0));
    assert_eq!((held[1].s, held[1].e), (4.0, 9.0));
    retire(window);
    fs::remove_dir_all(naivepost::startup::session_dir(&std::env::current_dir().unwrap())).ok();
}

/// S2 clause 3: Narrate's rows come from the file it just re-read, not from what was published before.
fn check_narrate_reread(app: &adw::Application) {
    let (dir, tree) = plant("f11-narrate");
    let window = cut_window(app, &dir);
    ui::tab_button(&window, Page::Narrate)
        .expect("the shell has a Narrate tab")
        .emit_by_name::<()>("clicked", &[]);
    // Publish a DIFFERENT number of lines than the file holds, so stale rows are observable.
    let mut stale = ui::read_state();
    stale.entries = vec![Entry {
        s: 0.0,
        e: 1.0,
        text: "a line the file does not have".into(),
        ..Default::default()
    }];
    ui::set_state(stale);
    ui::refresh(&window);
    assert_eq!(narrate_row_count(&window), 1, "the page shows the stale publication");

    press_rescan(&window);

    let on_disk = narration::load(&tree).expect("the record reads back");
    assert_eq!(on_disk.entries.len(), 2, "the file has two lines");
    assert_eq!(
        narrate_row_count(&window),
        on_disk.entries.len(),
        "the page re-read its file instead of keeping what was published"
    );
    // And the rows are the FILE's words, not merely two of them: a rebuild that kept the stale
    // publication and appended would satisfy the count above and be wrong. Each row's text box is
    // `line-text-<index>`; its content is what `write_box` puts there for that entry.
    let mut texts: Vec<String> = Vec::new();
    for index in 0.. {
        let Some(box_) = common::widget_in(&window, &format!("line-text-{index}")) else {
            break;
        };
        texts.push(
            box_
                .downcast::<gtk::Entry>()
                .expect("a line's text box is an Entry")
                .text()
                .to_string(),
        );
    }
    for entry in &on_disk.entries {
        assert!(
            texts.iter().any(|t| t.contains(&entry.text)),
            "the line {:?} from the file is on the page: {texts:?}",
            entry.text
        );
    }
    assert!(
        !texts.iter().any(|t| t.contains("a line the file does not have")),
        "the stale line is off the page: {texts:?}"
    );
    retire(window);
    fs::remove_dir_all(naivepost::startup::session_dir(&std::env::current_dir().unwrap())).ok();
}

/// S1's persistence half: the pruned list reaches `naivepost.json`, or the next launch shows the dead
/// row again and the same `!!! dropped` line repeats on every rescan.
fn check_pruned_list_written_back(app: &adw::Application) {
    let (dir, _tree) = plant("f11-writeback");
    let window = cut_window(app, &dir);

    press_rescan(&window);

    let saved = project::load(&dir).expect("the session file reads back");
    let paths: Vec<&str> = saved.sources.iter().map(|source| source.path.as_str()).collect();
    assert_eq!(paths, vec!["project:sources/talk.mkv"], "only the survivor is stored");
    retire(window);
    fs::remove_dir_all(naivepost::startup::session_dir(&std::env::current_dir().unwrap())).ok();
}

/// S1's other half through the button: slot 1 lost its holder, so the survivor takes it AND the
/// re-assignment is what lands on disk.
fn check_slot_1_persisted(app: &adw::Application) {
    let dir = session_folder("f11-slot1");
    fs::create_dir_all(dir.join("sources")).expect("sources/");
    fs::write(dir.join("sources/mic.wav"), b"audio").expect("the surviving recording is real");
    project::save(
        &Project {
            sources: vec![
                source("project:sources/dead.wav", false, 1),
                source("project:sources/mic.wav", false, 0),
            ],
            ..Default::default()
        },
        &dir,
    )
    .expect("session written");
    let model = project::load(&dir).expect("the planted session loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();
    ui::window::clear_logs_for_test();

    press_rescan(&window);

    let saved = project::load(&dir).expect("the session file reads back");
    let holder = saved
        .sources
        .iter()
        .find(|source| source.narrator == 1)
        .expect("slot 1 has a holder again");
    assert_eq!(holder.path, "project:sources/mic.wav", "the survivor took slot 1");
    assert_eq!(saved.sources.len(), 1, "the dead row did not survive either");
    retire(window);
    fs::remove_dir_all(naivepost::startup::session_dir(&std::env::current_dir().unwrap())).ok();
}

#[test]
fn f0_11_s2_a_rescan_refreshes_the_readouts_without_the_page_being_locked() {
    window_round();
    assert!(RAN_READOUTS.load(Ordering::SeqCst), "the readouts check never ran");
}

#[test]
fn f0_11_s2_a_rescan_rebuilds_the_cut_page_from_what_is_on_disk() {
    window_round();
    assert!(RAN_CUT.load(Ordering::SeqCst), "the Cut-rebuild check never ran");
}

#[test]
fn f0_11_s2_a_rescan_makes_narrate_re_read_its_file() {
    window_round();
    assert!(RAN_NARRATE.load(Ordering::SeqCst), "the Narrate re-read check never ran");
}

#[test]
fn f0_11_s1_the_pruned_list_is_written_back() {
    window_round();
    assert!(RAN_WRITEBACK.load(Ordering::SeqCst), "the write-back check never ran");
}

#[test]
fn f0_11_s1_a_rescan_reassigns_slot_1_and_persists_it() {
    window_round();
    assert!(RAN_SLOT1.load(Ordering::SeqCst), "the slot-1 check never ran");
}
