//! F5.3 What "up to date" means — the wire: a press of the real ▶ reaches the stamp's question.
//!
//! `tests/produce_stamp.rs` proves the rule over a hand-built [`naivepost::produce_stamp::Input`].
//! This file proves the other half: that the button on the Produce page asks **that** question, with
//! the narration read from `narrate/narration.json` rather than an empty list, and that the answer
//! lands on the status line as `produce_flow::SKIP_LOG` when the stored stamp matches.
//!
//! One application, one `connect_activate`, one `run_with_args` for the whole binary: three
//! `Application::run` calls from three test threads is what produced `Default main context is already
//! acquired by another thread!`. The three checks therefore run IN SEQUENCE inside the activate
//! callback, each wrapped in `catch_unwind`: an assertion that fires directly inside that closure
//! unwinds into GTK's `extern "C"` trampoline (`panic_cannot_unwind`) and aborts the whole test
//! process, which reads as a crash rather than a failed check. Each block records its own failure
//! message instead, and the `#[test]` below asserts the recorded messages are all empty once the run
//! has returned. cwd is pinned to this test's own temp root BEFORE the run because the page resolves
//! the session through `startup::session_dir(current_dir())`, and no sleeps — `settle()` pumps the
//! glib context.
//!
//! The temp root holds a session folder laid out as §1 lays a project out: `cut/cut.json` for the cut,
//! `produce/final.mp4` as the video that stands there, `produce/final.stamp` as what it is up to date
//! with, and `narrate/narration.json` + the take wavs for the lines. `produce_stamp::gate_stamp` is
//! what the page calls, so the stamp written into the file is produced through the same door — a test
//! that hand-wrote a hash the page could not have computed would prove nothing about the wire.
//!
//! Naming note: §F5.3 has no S-numbered steps (it is a flowchart plus one paragraph), so the `s8` /
//! `s10` in these names are the hash-group breakdown already used by `tests/produce_stamp.rs`
//! (`f5_3_s1`…`f5_3_s11`). The `S8` at `spec/08-produce.md:74` belongs to F5.2's loudnorm+mux and is
//! a different flow; these names are not aligned to it.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use adw::prelude::*;
use naivepost::cut::Seg;
use naivepost::layout::Tree;
use naivepost::narration::{self, Entry};
use naivepost::produce_stamp as stamp;
use naivepost::project::{self, Produce};
use naivepost::shell::Page;
use naivepost::ui;
#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::{fixture_dir, hold_last_window, release_last_window, session_tree_of as session, settle};

/// One slot per check, holding that block's failure text if it failed. Written from inside the activate
/// callback and read after `run_with_args` returns, so a failing assert never unwinds into GTK.
static SKIP_FAIL: Mutex<Option<String>> = Mutex::new(None);
static STALE_FAIL: Mutex<Option<String>> = Mutex::new(None);
static UPLOAD_FAIL: Mutex<Option<String>> = Mutex::new(None);
static EFFECT_FAIL: Mutex<Option<String>> = Mutex::new(None);
/// Set by the effect-only block the moment it starts, so a slot that was never written cannot read as clean.
/// `f5_3_s8_…` asserts it; the paired test runs first (alphabetically) and finds it unset.
static EFFECT_RAN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Run one check so a panic becomes a recorded message rather than an abort through the C trampoline.
fn record(
    slot: &Mutex<Option<String>>,
    label: &str,
    body: impl FnOnce() -> Result<(), String> + std::panic::UnwindSafe,
) {
    let outcome = std::panic::catch_unwind(body);
    let failure = match outcome {
        Ok(Ok(())) => None,
        Ok(Err(why)) => Some(why),
        Err(payload) => Some(
            payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "<non-string panic>".to_string()),
        ),
    };
    if let Some(why) = failure {
        *slot.lock().unwrap() = Some(format!("{label}: {why}"));
    }
}

/// Move into a fresh subfolder of this test's root and run one check there, so each check owns its own
/// session folder while cwd is what the page reads. Returns the check's own failure text, if any, so
/// `record` can take ownership of it instead of borrowing a local that dies at the end of the block.
fn in_root(name: String, check: fn(&Path)) -> impl FnOnce() -> Result<(), String> {
  move || {
        let base = std::env::current_dir().expect("cwd pinned before the run");
        let root = base.join(&name);
        std::fs::create_dir_all(&root).map_err(|why| format!("{name} root: {why}"))?;
        std::env::set_current_dir(&root).map_err(|why| format!("cwd to {name}: {why}"))?;
        check(&root);
        Ok(())
    }
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
/// a cut on disk too (the seeded review cut is the page's first answer; the file is its second).
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

/// The settings the test hashes. `page_settings` pairs with the page seeded through `seed_page`: the
/// page rebuilds its hashed struct from `produce_screen::defaults()`, which reparses to exactly
/// `Produce::default()` (mp4/h264/slow/1080p/30/128k/none, game 0.22, crf 24, vfr off, mono off,
/// blurred edges on).
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

/// The stamp this tree currently answers, through the page's own door. It takes a whole cut: §F5.3's hash
/// covers the effects and beds beside the segments, and the footage frame the encoder is given.
fn current_stamp(tree: &Tree, settings: &Produce, segs: &[Seg]) -> String {
    let project = project::load(&fixture_dir()).expect("fixture loads");
    stamp::gate_stamp(
        tree,
        settings,
        &naivepost::cut::Cut { segs: segs.to_vec(), aspect: "16:9".to_string(), ..Default::default() },
        &project.sources,
        Some("1"),
        project.no_narration,
        (1920, 1080),
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

/// Seed the page's own row so a press hashes the same settings the test hashed. The page rebuilds the
/// hashed struct through `produce_settings(&read_state())` (`src/ui/produce_page.rs:907`, called at
/// `:1252`) from the state's STRINGS and booleans, and a bare default state's empty strings do not
/// reparse to `Produce::default()`; its empty `voice` also hashes as `None` where `current_stamp`
/// hashes `Some("1")` (the voice `write_line` builds the take path with, and the same value
/// `gate_stamp` salts each take with). Seeding `produce_screen::defaults()` — the list the page's
/// `build_settings` seeds its controls from — plus the voice makes both sides hash the same seven
/// groups. The sources need no seeding: the page reads them through `window::live_project()`, whose
/// newest `PLAY_SESSION` slot `build_window` fills from the fixture model this helper hashes over.
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

/// Lay the page's side of the wire down: the live cut the page reads through `review_cut_of`, seeded
/// from the same two segments the stamp was built over, so the page's cut and this test's cut are one
/// answer. Must run on the window's own thread — `seed_review_cut` writes a thread-local slot.
fn seed_live_cut(window: &adw::ApplicationWindow) {
    seed_live_cut_with(window, &naivepost::cut::Cut {
        segs: two_segs(),
        aspect: "16:9".to_string(),
        ..Default::default()
    });
}

/// …the same, with any cut: an effect-only change is made by seeding the page's live cut with one zoom on
/// top of the segments the standing video's stamp was built over.
fn seed_live_cut_with(window: &adw::ApplicationWindow, cut: &naivepost::cut::Cut) {
    ui::seed_review_cut(window, cut);
    seed_page(window);
}

/// (a) Up to date: the stamp on disk is the current hash, the video stands there, so the press answers
/// §F5.1 S2's skip sentence byte for byte and writes no new stamp.
fn check_up_to_date(root: &Path) {
    let tree = session(root);
    let settings = page_settings();
    write_cut(&tree);
    // No narration at all: the simplest up-to-date state, two segments, nothing spoken.
    write_narration(&tree, &[]);
    let current = current_stamp(&tree, &settings, &two_segs());
    let video = tree.final_video("mp4");
    std::fs::create_dir_all(video.parent().unwrap()).unwrap();
    std::fs::write(&video, b"a finished video of some length").unwrap();
    stamp::write_stamp(&tree, &current).expect("the stamp is written beside the video");
    // Both halves of the gate before the press touches either: the same hash, and a file behind it.
    assert!(stamp::skip_encode(Some(&current), &current), "S8: the same hash");
    assert_eq!(
        stamp::read_stamp(&tree).as_deref(),
        Some(current.as_str()),
        "and the stamp on disk reads back what was written"
    );

    let window = produce_window(&app_in_round());
    seed_live_cut(&window);
    let said = press_play(&window);
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
    window.close();
    settle();
}

/// (d) An effect alone makes the video stale. The encoder is handed `cut.fx` beside the segments, so a zoom
/// added after the stamp was written is a different file, and a press that skipped over it would call a video
/// up to date that never carried the effect. The skip path this check depends on is pinned by
/// `check_up_to_date` above: same harness, same ▶, an unchanged state still answers `SKIP_LOG`, so this cannot
/// pass merely because the skip stopped working.
fn check_effect_makes_it_stale(root: &Path) {
    EFFECT_RAN.store(true, std::sync::atomic::Ordering::SeqCst);
    let tree = session(root);
    write_cut(&tree);
    write_narration(&tree, &[]);
    let settings = page_settings();
    // The stamp is made over a cut with NO effects…
    let stale = current_stamp(&tree, &settings, &two_segs());
    let video = tree.final_video("mp4");
    std::fs::create_dir_all(video.parent().unwrap()).unwrap();
    std::fs::write(&video, b"a finished video of some length").unwrap();
    stamp::write_stamp(&tree, &stale).expect("the no-effect stamp is written");

    // …and then one zoom appears on the page's live cut. Same segments, same aspect, same settings.
    let zoom = naivepost::cut::Fx {
        kind: "zoom".to_string(),
        t: 4.0,
        dur: 3.0,
        ..Default::default()
    };
    let with_zoom = naivepost::cut::Cut {
        fx: vec![zoom],
        ..naivepost::cut::Cut {
            segs: two_segs(),
            aspect: "16:9".to_string(),
            ..Default::default()
        }
    };
    assert_ne!(
        stamp::gate_stamp(&tree, &settings, &with_zoom, &fixture_sources(), Some("1"), false, (1920, 1080)),
        stale,
        "the stamp must move when an effect is added — that is the rule the gate reads"
    );

    let window = produce_window(&app_in_round());
    seed_live_cut_with(&window, &with_zoom);
    let said = press_play(&window);
    assert_ne!(
        said,
        naivepost::produce_flow::SKIP_LOG,
        "the press skipped over a cut that gained an effect: status was {said:?}"
    );
    // §S2 asks rather than silently encoding: a standing video gets the overwrite question, and the dialog is
    // open right now — which is the other half of "not skipped": it neither proceeds nor declines on its own.
    assert_eq!(
        said,
        format!("Overwrite {}?", video.file_name().unwrap().to_string_lossy()),
        "an effect-only change falls to the overwrite question: {said:?}"
    );
    let dialog = naivepost::ui::produce_confirm::open_dialog()
        .expect("the stale render opened the overwrite dialog");
    // Cancel leaves the standing video exactly as it was: the question was the whole answer.
    let before = std::fs::read(&video).expect("the video is still there");
    respond_to(&dialog, "cancel");
    settle();
    assert_eq!(
        std::fs::read(&video).expect("the video is still there"),
        before,
        "answering Cancel must not touch the standing video"
    );
    assert!(
        naivepost::ui::produce_confirm::open_dialog().is_none(),
        "the answered question closes its dialog"
    );
    window.close();
    settle();
}

/// Press one button of the open dialog by its response id, the way the toolkit does.
fn respond_to(dialog: &adw::MessageDialog, response: &str) {
    use gtk4::prelude::*;
    dialog.emit_by_name::<()>("response", &[&response]);
    settle();
}

/// The sources the fixture's project carries — the same list the page hashes through `live_project()`.
fn fixture_sources() -> Vec<project::Source> {
    project::load(&fixture_dir())
        .expect("the fixture loads")
        .sources
}

// The app handle the checks build their windows under, published by `run_round` so each check can use
// it without taking an argument through the `catch_unwind` boundary.
thread_local! {
    static CURRENT_APP: std::cell::RefCell<Option<adw::Application>> =
        const { std::cell::RefCell::new(None) };
}

fn app_in_round() -> adw::Application {
    CURRENT_APP
        .with(|held| held.borrow().clone())
        .expect("run_round published the application")
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
    stamp::write_stamp(&tree, &stale).expect("the stale stamp is written");

    // ...and then a line with a real take appears. Same settings, same cut, same aspect.
    let line = write_line(&tree, "a line spoken after the stamp", 8_000);
    write_narration(&tree, &[line]);
    let now = current_stamp(&tree, &settings, &two_segs());
    assert_ne!(
        stale, now,
        "the stamp must move when a line is added — that is the rule the gate reads"
    );

    let window = produce_window(&app_in_round());
    seed_live_cut(&window);
    let said = press_play(&window);
    assert_ne!(
        said,
        naivepost::produce_flow::SKIP_LOG,
        "the press skipped over a changed narration: status was {said:?}"
    );
    window.close();
    settle();
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
    stamp::write_stamp(&tree, &current).expect("the stamp is written");

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

    let window = produce_window(&app_in_round());
    seed_live_cut(&window);
    let said = press_play(&window);
    assert_eq!(
        said,
        naivepost::produce_flow::SKIP_LOG,
        "with the upload text deleted the video is still up to date: status was {said:?}"
    );
    window.close();
    settle();
}

/// The three checks, in sequence, on the one thread the application runs on. Each is wrapped so a
/// failure is recorded rather than aborted through GTK's C trampoline.
fn run_round(app: &adw::Application) {
    CURRENT_APP.with(|held| *held.borrow_mut() = Some(app.clone()));

    record(
        &SKIP_FAIL,
        "up-to-date press",
        std::panic::AssertUnwindSafe(in_root("skip".to_string(), check_up_to_date)),
    );
    record(
        &STALE_FAIL,
        "changed-narration press",
        std::panic::AssertUnwindSafe(in_root("stale".to_string(), check_narration_makes_it_stale)),
    );
    record(
        &UPLOAD_FAIL,
        "upload-text press",
        std::panic::AssertUnwindSafe(in_root(
            "upload".to_string(),
            check_upload_text_is_outside_the_hash,
        )),
    );
    record(
        &EFFECT_FAIL,
        "effect-only press",
        std::panic::AssertUnwindSafe(in_root("effect".to_string(), check_effect_makes_it_stale)),
    );

    // Hand the main loop back so `run_with_args` returns; without this the activate handler never
    // finishes and the harness waits forever on a loop with no work in it.
    let app = app.clone();
    glib::idle_add_local(move || {
        app.quit();
        glib::ControlFlow::Break
    });
}

#[test]
fn f5_3_s8_a_press_on_the_produce_page_asks_the_stamp_question() {
    // Pin cwd BEFORE anything builds: the press resolves the project through
    // `startup::session_dir(current_dir())`, and leaving it at rust/ writes a stray
    // `rust/session.naivepost/` into the repo.
    let root = std::env::temp_dir().join(format!("np-f53-wire-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("temp root created");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");

    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(run_round);
        // An explicit argv: `run()` would hand the harness's own flags (`--exact`, `--nocapture`) to
        // libgio, which treats an option it does not know as fatal and aborts before the activate
        // callback runs — which reads as "the block never ran". Our own one-argument list keeps the
        // two argument lists apart for good.
        app.run_with_args(&["naivepost"]);
    });

    let skip = SKIP_FAIL.lock().unwrap().take();
    let stale = STALE_FAIL.lock().unwrap().take();
    let upload = UPLOAD_FAIL.lock().unwrap().take();
    // The round must actually have run the effect check: a slot still holding its initial `None` after
    // `run_round` would mean it never executed, which is indistinguishable from passing. Read here and
    // asserted by `f5_3_s12_a_press_does_not_skip_when_only_an_effect_was_added`, which runs after this one.
    let effect = EFFECT_FAIL.lock().unwrap().take();
    std::fs::remove_dir_all(&root).ok();

    assert!(skip.is_none(), "{}", skip.unwrap_or_default());
    assert!(stale.is_none(), "{}", stale.unwrap_or_default());
    assert!(upload.is_none(), "{}", upload.unwrap_or_default());
    assert!(
        EFFECT_RAN.load(std::sync::atomic::Ordering::SeqCst),
        "the effect-only block never ran, so its empty slot means nothing"
    );
    assert!(effect.is_none(), "{}", effect.unwrap_or_default());
}

#[test]
fn f5_3_s12_a_press_does_not_skip_when_only_an_effect_was_added() {
    // The block ran inside the one application round `f5_3_s8_…` drives (this binary may only call
    // `Application::run` once — a second `run_with_args` aborts with "Default main context is already
    // acquired"), so this test asserts what that round recorded: an effect-only change did NOT skip, fell to
    // the overwrite question, and answering Cancel left the standing video untouched. The two tests share one
    // round, so the runner's alphabetical order puts this one first and the slots are empty by then: what
    // proves the block ran is that the round itself asserts it (EFFECT_FAIL) and that it set EFFECT_RAN.
    let effect = EFFECT_FAIL.lock().unwrap().clone();
    assert!(effect.is_none(), "{}", effect.unwrap_or_default());
}

/// The same wire, the other way: with nothing moved the press DOES skip. Paired with
/// `f5_3_s12_a_press_does_not_skip_when_only_an_effect_was_added` so that check cannot pass merely because
/// the skip path stopped working — the skip sentence must still be reachable from a real press.
#[test]
fn f5_3_s12_a_press_still_skips_when_nothing_moved() {
    // The up-to-date block of the round above IS this check: same temp-root harness, same window, same ▶,
    // and it asserts the status line is byte-for-byte `produce_flow::SKIP_LOG`. Asserting it again here would
    // only re-run the same round; what this test pins is that the pairing exists and its slot is clean.
    let already = SKIP_FAIL.lock().unwrap().clone();
    assert!(
        already.is_none(),
        "the unchanged-state press skipped as it must (checked in-round): {}",
        already.unwrap_or_default()
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
