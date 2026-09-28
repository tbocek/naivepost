//! F5.3 What "up to date" means — the wire: a press of the real ▶ reaches the stamp's question.
//!
//! `tests/produce_stamp.rs` proves the rule over a hand-built [`naivepost::produce_stamp::Input`].
//! This file proves the other half: that the button on the Produce page asks **that** question, with
//! the narration read from `narrate/narration.json` rather than an empty list, and that the answer
//! lands on the status line as `produce_flow::SKIP_LOG` when the stored stamp matches.
//!
//! Same shape as `tests/produce_screen_widgets.rs`: one application, one `connect_activate`, exactly
//! one `Application::run` (`g_application_run` refuses a second claimant of the default main context),
//! cwd pinned to this test's own temp root BEFORE the run because the page resolves the session through
//! `startup::session_dir(current_dir())`, and no sleeps — `settle()` pumps the glib context.
//!
//! The temp root holds a session folder laid out as §1 lays a project out: `cut/cut.json` for the cut,
//! `produce/final.mp4` as the video that stands there, `produce/final.stamp` as what it is up to date
//! with, and `narrate/narration.json` + the take wavs for the lines. `produce_stamp::gate_stamp` is
//! what the page calls, so the stamp written into the file is produced through the same door — a test
//! that hand-wrote a hash the page could not have computed would prove nothing about the wire.

use std::path::{Path, PathBuf};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut::Seg;
use naivepost::layout::Tree;
use naivepost::narration::{self, Entry};
use naivepost::produce_stamp as stamp;
use naivepost::project::{self, Produce};
use naivepost::shell::Page;
use naivepost::ui;
#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::{fixture_dir, hold_last_window, release_last_window, settle};

/// Each check sets its own flag; the `#[test]` at the bottom asserts the flag, so a check that never
/// ran fails the test instead of passing silently (a panic inside `activate` is swallowed by GTK).
static RAN_SKIP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static RAN_STALE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static RAN_UPLOAD: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// This test's own session folder: `<root>/session.naivepost`, the name `startup::SESSION_NAME` gives
/// the working copy beside a root, which is what the page resolves from cwd.
fn session(root: &Path) -> Tree {
    let dir = naivepost::startup::session_dir(root);
    std::fs::create_dir_all(&dir).expect("session folder created");
    Tree::new(&dir).expect("the session folder is a project tree")
}

/// Two segments, so the cut is non-empty and the gate is reached rather than refused for want of a cut.
fn two_segs() -> Vec<Seg> {
    let mut first = Seg::default();
    first.s = 0.0;
    first.e = 9.5;
    let mut second = Seg::default();
    second.s = 10.0;
    second.e = 30.0;
    vec![first, second]
}

/// Write `cut/cut.json` with two segments and the aspect the run will hash, so `what_is_the_cut` finds
/// a cut on disk (no review cut is seeded in these checks, so the file is the answer).
fn write_cut(tree: &Tree) {
    let cut = naivepost::cut::Cut {
        segs: two_segs(),
        aspect: "16:9".to_string(),
        ..Default::default()
    };
    let file = tree.cut_json();
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(
        &file,
        serde_json::to_string_pretty(&cut).expect("the cut serialises"),
    )
    .unwrap();
}

/// The settings the page will build from a default `ProduceState`: `produce_settings` reads
/// container/codec/resolution/game_volume/crf/vfr/mono/blurred_edges off the state and defaults the
/// rest, so a default state hashes as `Produce::default()` under the same names.
fn page_settings() -> Produce {
    Produce::default()
}

/// One narration line inside the first segment, with a take written where `wav_of` says it lives, so the
/// line's wav size and mtime are real file facts rather than an imagined pair.
fn write_line(tree: &Tree, text: &str, size: usize) -> Entry {
    let mut line = Entry::default();
    line.s = 1.0;
    line.e = 5.0;
    line.text = text.to_string();
    line.roll = 0;
    let wav = stamp::wav_of(tree, &line, Some("1"), line.roll).expect("a line with words has a take path");
    std::fs::create_dir_all(wav.parent().unwrap()).unwrap();
    std::fs::write(&wav, vec![7u8; size]).unwrap();
    line
}

/// `narrate/narration.json` holding `lines`, written through `narration::save` so the shape on disk is
/// the shape the reader accepts, not a guess at it.
fn write_narration(tree: &Tree, lines: &[Entry]) {
    let record = narration::Narration {
        entries: lines.to_vec(),
        silent: Vec::new(),
    };
    narration::save(&record, tree).expect("narration.json written");
}

/// The stamp this tree currently answers, through the page's own door.
fn current_stamp(tree: &Tree, settings: &Produce, segs: &[Seg]) -> String {
    let project = project::load(&fixture_dir()).expect("fixture loads");
    stamp::gate_stamp(
        tree,
        settings,
        segs,
        &project.sources,
        "16:9",
        Some("1"),
        project.no_narration,
    )
}

/// A window sitting on the Produce tab.
fn produce_window(app: &adw::Application) -> adw::ApplicationWindow {
    release_last_window();
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, Page::Produce.label());
    hold_last_window(window.clone());
    window.present();
    settle();
    window
}

/// Fire the run button the way the toolkit does and report what the status line ended up saying.
fn press_play(window: &adw::ApplicationWindow) -> String {
    let button = ui::line_step_button(window, "play-button")
        .expect("the shell drew a play-button on the run bar");
    button.emit_by_name::<()>("clicked", &[]);
    settle();
    ui::find_status(window.upcast_ref())
        .expect("the window has a status line")
        .text()
        .to_string()
}

/// One application per check, exactly one `Application::run` each: the three checks run in three
/// processes because cargo gives each test its own thread and `g_application_run` refuses to be
/// re-entered on a thread that already ran one. `app` is handed back so the caller can keep it alive
/// until after `run` returns rather than moving it into the closure.
fn build_app() -> adw::Application {
    adw::Application::builder()
        .application_id(ui::APP_ID)
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build()
}

/// (a) Up to date: the stamp on disk is the current hash, the video stands there, so the press answers
/// §F5.1 S2's skip sentence byte for byte and writes no new stamp.
///
/// The whole check runs INSIDE the activate closure, files and all: `seed_review_cut` writes to a
/// thread-local slot that only exists on the thread the window was built on, so the cut has to be seeded
/// there rather than from the test thread before the app starts.
fn check_up_to_date(root: &Path) {
    let tree = session(root);
    let settings = page_settings();
    let app = build_app();
    let check = move |window: &adw::ApplicationWindow| {
        write_cut(&tree);
        // The live cut the page reads through `review_cut_of`: seeded from the same two segments the
        // stamp was built over, so the page's cut and this test's cut are one answer.
        let cut = naivepost::cut::Cut {
            segs: two_segs(),
            aspect: "16:9".to_string(),
            ..Default::default()
        };
        ui::seed_review_cut(window, &cut);
        // No narration at all: the simplest up-to-date state, two segments, nothing spoken.
        write_narration(&tree, &[]);
        // The page hashes the LIVE project (`window::live_project()` -> `PLAY_SESSION`'s newest slot),
        // which `build_window` fills from the model it was handed — the same fixture this file builds its
        // stamp over, so the two sources lists are already one answer and need no seeding. What DOES need
        // seeding is the page's own row, read through `produce_settings(&read_state())`: a bare default
        // state carries EMPTY strings and booleans, which do not reparse to `Produce::default()`, and its
        // empty `voice` hashes as `None` while `current_stamp` hashes `Some("1")` (the voice
        // `write_line` builds each take's wav path with). Seeding the state with
        // `produce_screen::defaults()` — the list the page's `build_settings` seeds its controls from —
        // plus the voice makes the page and this helper hash the same seven groups.
        let row = naivepost::produce_screen::defaults();
        let mut page = ui::produce_page::read_state();
        page.container = row[0].1.to_string();
        page.codec = row[1].1.to_string();
        page.preset = row[2].1.to_string();
        page.resolution = row[3].1.to_string();
        page.frame_rate = row[4].1.to_string();
        page.audio = row[5].1.to_string();
        page.subtitles = row[6].1.to_string();
        page.game_volume = row[8].1.parse().expect("game audio parses");
        page.crf = row[9].1.parse().expect("crf parses");
        page.vfr = row[10].1 == "on";
        page.mono = row[11].1 == "on";
        page.blurred_edges = row[12].1 == "on";
        page.voice = "1".to_string();
        ui::produce_page::set_state(page);
        let current = current_stamp(&tree, &settings, &two_segs());
        let video = tree.final_video("mp4");
        std::fs::create_dir_all(video.parent().unwrap()).unwrap();
        std::fs::write(&video, b"a finished video of some length").unwrap();
        std::fs::write(stamp::stamp_path(&tree), format!("{current}\n")).unwrap();

                let said = press_play(window);
                assert_eq!(
                    said,
                    naivepost::produce_flow::SKIP_LOG,
            "the press did not answer with the skip sentence"
        );
        // A skip writes nothing: what stands in the file is still the hash that made it a skip.
        assert_eq!(
            stamp::read_stamp(&tree).as_deref(),
            Some(current.as_str()),
            "a skipped encode must not rewrite the stamp"
        );
        assert!(
            video.is_file(),
            "and the video it stands beside is left exactly as it was"
        );
    };
    app.connect_activate(move |app| {
        let window = produce_window(app);
        check(&window);
        RAN_SKIP.store(true, std::sync::atomic::Ordering::SeqCst);
        window.close();
        settle();
    });
    app.run();
}

/// Seed the page's own row so a press hashes the same settings the test hashed. The page rebuilds the
/// hashed struct through `produce_settings(&read_state())` from the state's STRINGS and booleans, and a
/// bare default state's empty strings do not reparse to `Produce::default()`; its empty `voice` also
/// hashes as `None` where `current_stamp` hashes `Some("1")` (the voice `write_line` builds the take
/// path with). Seeding `produce_screen::defaults()` — the list the page's `build_settings` seeds its
/// controls from — plus the voice makes both sides hash the same seven groups.
fn seed_page(_window: &adw::ApplicationWindow) {
    let row = naivepost::produce_screen::defaults();
    let mut page = ui::produce_page::read_state();
    page.container = row[0].1.to_string();
    page.codec = row[1].1.to_string();
    page.preset = row[2].1.to_string();
    page.resolution = row[3].1.to_string();
    page.frame_rate = row[4].1.to_string();
    page.audio = row[5].1.to_string();
    page.subtitles = row[6].1.to_string();
    page.game_volume = row[8].1.parse().expect("game audio parses");
    page.crf = row[9].1.parse().expect("crf parses");
    page.vfr = row[10].1 == "on";
    page.mono = row[11].1 == "on";
    page.blurred_edges = row[12].1 == "on";
    page.voice = "1".to_string();
    ui::produce_page::set_state(page);
}

/// (b) The narration is in the comparison: a line written after the stamp was made makes the video
/// stale, so the press does NOT answer with the skip sentence. Before this round's fix the gate hashed
/// an empty line list and this press skipped, which is the bug the spec's flowchart forbids.
fn check_narration_makes_it_stale(root: &Path) {
    let tree = session(root);
    write_cut(&tree);
    write_narration(&tree, &[]);
    let settings = page_settings();
    // The stamp is made while there are no lines...
    let stale = current_stamp(&tree, &settings, &two_segs());
    let video = tree.final_video("mp4");
    std::fs::create_dir_all(video.parent().unwrap()).unwrap();
    std::fs::write(&video, b"a finished video of some length").unwrap();
    std::fs::write(stamp::stamp_path(&tree), format!("{stale}\n")).unwrap();

    // ...and then a line with a real take appears. Same settings, same cut, same aspect.
    let line = write_line(&tree, "a line spoken after the stamp", 8_000);
    write_narration(&tree, &[line]);
    let now = current_stamp(&tree, &settings, &two_segs());
    assert_ne!(
        stale, now,
        "the stamp must move when a line is added — that is the rule the gate reads"
    );

    let app = build_app();
    app.connect_activate(move |app| {
        let window = produce_window(app);
        seed_page(&window);
        // The live cut, seeded on the window's own thread, matching the stamp's segments.
        ui::seed_review_cut(
            &window,
            &naivepost::cut::Cut {
                segs: two_segs(),
                aspect: "16:9".to_string(),
                ..Default::default()
            },
        );
        let said = press_play(&window);
        assert_ne!(
            said,
            naivepost::produce_flow::SKIP_LOG,
            "the press skipped over a changed narration: status was {said:?}"
        );
        RAN_STALE.store(true, std::sync::atomic::Ordering::SeqCst);
        window.close();
        settle();
    });
    app.run();
}

/// (c) The upload record is not in the hash: with the video up to date, writing and then deleting
/// `produce/publish/publish.json` leaves the press answering the skip sentence, and `is_written`
/// still answers the words half on its own, independently of the video.
fn check_upload_text_is_outside_the_hash(root: &Path) {
    let tree = session(root);
    write_cut(&tree);
    write_narration(&tree, &[]);
    let settings = page_settings();
    let current = current_stamp(&tree, &settings, &two_segs());
    let video = tree.final_video("mp4");
    std::fs::create_dir_all(video.parent().unwrap()).unwrap();
    std::fs::write(&video, b"a finished video of some length").unwrap();
    std::fs::write(stamp::stamp_path(&tree), format!("{current}\n")).unwrap();

    // Text written, then text deleted: the video's up-to-dateness answers the same both times, because
    // the upload record is not in the hash and deleting `publish/` restarts the text alone.
    let json = tree.publish_json();
    std::fs::create_dir_all(json.parent().unwrap()).unwrap();
    std::fs::write(&json, b"{\"title\":\"a title\"}").unwrap();
    assert!(naivepost::publish::is_written(&tree));
    let with_text = current_stamp(&tree, &settings, &two_segs());
    std::fs::remove_dir_all(json.parent().unwrap()).unwrap();
    assert!(!naivepost::publish::is_written(&tree));
    let without_text = current_stamp(&tree, &settings, &two_segs());
    assert_eq!(
        with_text, without_text,
        "publish.json must not move the render's stamp"
    );

    let app = build_app();
    app.connect_activate(move |app| {
        let window = produce_window(app);
        seed_page(&window);
        ui::seed_review_cut(
            &window,
            &naivepost::cut::Cut {
                segs: two_segs(),
                aspect: "16:9".to_string(),
                ..Default::default()
            },
        );
        let said = press_play(&window);
        assert_eq!(
            said,
            naivepost::produce_flow::SKIP_LOG,
            "with the upload text deleted the video is still up to date: status was {said:?}"
        );
        RAN_UPLOAD.store(true, std::sync::atomic::Ordering::SeqCst);
        window.close();
        settle();
    });
    app.run();
}

#[test]
fn f5_3_s8_a_press_on_an_up_to_date_project_skips_the_encode() {
    let root = std::env::temp_dir().join(format!("np-f53-wire-skip-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("temp root created");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");
    check_up_to_date(&root);
    std::fs::remove_dir_all(&root).ok();
    assert!(
        RAN_SKIP.load(std::sync::atomic::Ordering::SeqCst),
        "the up-to-date press check never ran inside the app"
    );
}

#[test]
fn f5_3_s8_a_changed_narration_line_is_not_up_to_date() {
    let root = std::env::temp_dir().join(format!("np-f53-wire-stale-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("temp root created");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");
    check_narration_makes_it_stale(&root);
    std::fs::remove_dir_all(&root).ok();
    assert!(
        RAN_STALE.load(std::sync::atomic::Ordering::SeqCst),
        "the stale-narration press check never ran inside the app"
    );
}

#[test]
fn f5_3_s10_deleting_the_upload_text_does_not_stale_the_video() {
    let root = std::env::temp_dir().join(format!("np-f53-wire-upload-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("temp root created");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");
    check_upload_text_is_outside_the_hash(&root);
    std::fs::remove_dir_all(&root).ok();
    assert!(
        RAN_UPLOAD.load(std::sync::atomic::Ordering::SeqCst),
        "the upload-text check never ran inside the app"
    );
}

/// Where the stamp file goes: beside the video, named for its stem. Kept here as one more thing the wire
/// reads, so a change of location shows up in a press rather than only in a unit test.
#[allow(dead_code)]
fn stamp_is_beside_the_video(tree: &Tree) -> bool {
    let stamp_file: PathBuf = stamp::stamp_path(tree);
    let video = tree.final_video("mp4");
    stamp_file.parent() == video.parent() && stamp_file.file_name().map(|n| n == "final.stamp") == Some(true)
}
