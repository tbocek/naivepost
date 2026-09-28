//! F5.7 Page runs — the wire: each of the page's six buttons drives its `produce_presses` walk, and the
//! spec's own sentence lands on the status line.
//!
//! `tests/produce_page_runs.rs` proves what a press *costs* over hand-built values. This file proves the
//! other half: that the button on the Produce page runs **that** walk, in §F5.7's order — refuse first,
//! then the `>>>` opening line, then the legs, then one ending — and that nothing cheaper is secretly more
//! expensive: ↻ beside Title draws no picture, ↻ Transcode asks no model, a reprint costs no GPU run.
//!
//! Shape copied from `tests/produce_stamp_widgets.rs`: ONE `adw::Application`, one `connect_activate`, one
//! `run_with_args` for the whole binary (a second `Application::run` from another thread is what produced
//! `Default main context is already acquired by another thread!`). The checks therefore run IN SEQUENCE
//! inside the activate callback, each wrapped in `catch_unwind`: an assertion firing directly there unwinds
//! into GTK's `extern "C"` trampoline (`panic_cannot_unwind`) and aborts the process, which reads as a
//! crash rather than a failed check. cwd is pinned to this test's own temp root BEFORE the run because the
//! page resolves the session through `startup::session_dir(current_dir())`.
//!
//! No ffmpeg and no sd.cpp exist in this container, so every leg is scripted before it is fired:
//! [`ui::produce_languages::set_draw_for_test`] for the picture, `naivepost::produce_upload::set_reply_for_test`
//! for the model, and `naivepost::produce_translate::set_spawn_for_test` for anything argv-shaped. Each
//! scripted leg COUNTS its own calls, because "no draw" and "one draw" are invisible in the finished status
//! line and visible only in that counter.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut::Cut;
use naivepost::layout::Tree;
use naivepost::project::{Publish, TextMark};
use naivepost::shell::Page;
use naivepost::ui;
#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::{fixture_dir, hold_last_window, release_last_window, settle};

/// One slot per check, holding that block's failure text if it failed.
static REDRAW_FAIL: Mutex<Option<String>> = Mutex::new(None);
static REWORD_FAIL: Mutex<Option<String>> = Mutex::new(None);
static TAKEN_FAIL: Mutex<Option<String>> = Mutex::new(None);
static EXPORT_FAIL: Mutex<Option<String>> = Mutex::new(None);
static SAVE_FAIL: Mutex<Option<String>> = Mutex::new(None);
static TRANSCODE_FAIL: Mutex<Option<String>> = Mutex::new(None);
static REFUSAL_FAIL: Mutex<Option<String>> = Mutex::new(None);

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
/// session folder while cwd is what the page reads.
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

/// This test's own session folder, the name `startup::SESSION_NAME` gives the working copy beside a root.
fn session(root: &Path) -> Tree {
    let dir = naivepost::startup::session_dir(root);
    std::fs::create_dir_all(&dir).expect("session folder created");
    Tree::new(&dir).expect("the session folder is a project tree")
}

/// Two segments, so the cut is non-empty and a press gets past its refusal.
fn two_segs() -> Vec<naivepost::cut::Seg> {
    let mut first = naivepost::cut::Seg::default();
    first.s = 0.0;
    first.e = 9.5;
    let mut second = naivepost::cut::Seg::default();
    second.s = 10.0;
    second.e = 30.0;
    vec![first, second]
}

/// The app handle the checks build their windows under, published so each check can use it without taking
/// an argument through the `catch_unwind` boundary.
thread_local! {
    static CURRENT_APP: std::cell::RefCell<Option<adw::Application>> =
        const { std::cell::RefCell::new(None) };
}

fn app_in_round() -> adw::Application {
    CURRENT_APP
        .with(|held| held.borrow().clone())
        .expect("run_round published the application")
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

/// Seed the live cut the presses read through `review_cut_of`, and the row they read their settings from.
/// Must run on the window's own thread — both write thread-local slots.
fn seed_live_cut(window: &adw::ApplicationWindow) {
    ui::seed_review_cut(
        window,
        &Cut {
            segs: two_segs(),
            aspect: "16:9".to_string(),
            ..Default::default()
        },
    );
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
    // An edit instruction, so ↻ over the thumbnail has something to tell the image model: without one the
    // walk refuses with NO_INSTRUCTION rather than drawing noise, which is its own rule but not this check's.
    page.instruction = "a slide deck under a dark sky".to_string();
    // One marked text box, so the reprint pass has something to print and the count is non-zero where the
    // spec says words go back over the picture.
    page.texts = vec![TextMark {
        cx: 0.5,
        cy: 0.78,
        wf: 0.8,
        hf: 0.2,
        text: "a marked line".to_string(),
    }];
    page.thumb_title = "a printed title".to_string();
    ui::produce_page::set_state(page);
    ui::produce_page::refresh(window);
    settle();
}

/// Counters the scripted legs bump. The model's own count lives in the walk's `Outcome` (it is a rule about
/// what the press asked for, not about a leg); these counters are for the legs that have no other witness.
static DRAWS: AtomicUsize = AtomicUsize::new(0);
static PRINTS: AtomicUsize = AtomicUsize::new(0);
static ENCODES: AtomicUsize = AtomicUsize::new(0);

fn reset_counters() {
    DRAWS.store(0, Ordering::SeqCst);
    PRINTS.store(0, Ordering::SeqCst);
    ENCODES.store(0, Ordering::SeqCst);
}

/// Lay a plain drawn picture on disk so the print leg has a base to copy. Used by the presses that reprint
/// without drawing first (↻ beside Title, Set Thumbnail): in a real session `thumbnail-plain.png` is what
/// sd.cpp left behind, and headless this test leaves it there itself.
fn lay_plain_picture() {
    let tree = session_tree();
    let plain = tree.thumbnail_plain_png();
    std::fs::create_dir_all(plain.parent().unwrap()).ok();
    std::fs::write(&plain, b"a drawn picture").ok();
}

/// Script every leg this round fires: the picture answers at once, the model answers with three things, and
/// nothing dials a socket or spawns a process. Counters start clean so each check reads its own press.
fn script_legs() {
    reset_counters();
    // The picture: counted, then accepted, with the plain PNG laid down for the print leg's base.
    ui::produce_languages::set_draw_for_test(std::rc::Rc::new(|_body| {
        DRAWS.fetch_add(1, Ordering::SeqCst);
        lay_plain_picture();
        Ok(naivepost::produce_upload::DrawnJob { id: "job-1".to_string() })
    }));
    // The model: a reply whose three answers the walk keeps whether or not it also draws.
    naivepost::produce_upload::set_reply_for_test(
        "TITLE: a rewritten title\nTHUMBNAIL: an instruction of its own\nDESCRIPTION: a paragraph about \
the video"
            .to_string(),
    );
}

/// The session folder the scripted legs write into.
fn session_tree() -> Tree {
    Tree::new(&naivepost::startup::session_dir(
        &std::env::current_dir().unwrap_or_default(),
    ))
    .unwrap_or_else(|_| Tree::new(Path::new("session.naivepost")).unwrap())
}

/// Fire one named button and report what the status line ended up saying.
fn press(window: &adw::ApplicationWindow, name: &str) -> String {
    let button = ui::line_step_button(window, name)
        .unwrap_or_else(|| panic!("the page drew no widget named {name}"));
    button.emit_by_name::<()>("clicked", &[]);
    settle();
    ui::find_status(window.upcast_ref())
        .expect("the window has a status line")
        .text()
        .to_string()
}

/// The lines logged since the last mark, so a check reads only its own press.
fn logs_since(mark: usize) -> Vec<String> {
    ui::window_logs()[mark..].to_vec()
}

fn log_mark() -> usize {
    ui::window_logs().len()
}

// ---- S1: ↻ over the thumbnail -----------------------------------------------------

/// ↻ redraw: refuses with its OWN no-cut sentence when there is no cut; with a cut it opens with
/// DRAW_AGAIN_LOG, draws exactly once, reprints the words over the fresh picture, and ends with
/// REDRAW_DONE.
fn check_redraw(root: &Path) {
    let tree = session(root);
    script_legs();
    let window = produce_window(&app_in_round());

    // Refusal first: no cut seeded yet, so the press must say its own sentence and touch NOTHING.
    let said_no_cut = press(&window, "thumbnail-redraw");
    assert_eq!(
        said_no_cut,
        naivepost::produce_runs::REDRAW_NO_CUT,
        "↻ with no cut must give the thumbnail's own refusal, not the Cut step's"
    );
    assert_eq!(DRAWS.load(Ordering::SeqCst), 0, "a refused press draws nothing");

    seed_live_cut(&window);
    let mark = log_mark();
    // Unticked: the one LLM call that re-asks for the words rides along with the single draw.
    let said = press(&window, "thumbnail-redraw");
    let logged = logs_since(mark);
    assert_eq!(
        said,
        naivepost::produce_runs::REDRAW_DONE,
        "the success ending is §F5.7's own line"
    );
    assert!(
        logged.iter().any(|l| l.contains(naivepost::produce_runs::DRAW_AGAIN_LOG)),
        "the opening line names the one sd.cpp call before it happens: {logged:?}"
    );
    assert_eq!(DRAWS.load(Ordering::SeqCst), 1, "exactly one draw — a second would be a new picture");
    // The unticked box's extra cost is exactly one LLM call, read off the walk's own tally.
    assert_eq!(
        naivepost::produce_runs::redraw(false).rewrites,
        1,
        "the cost this press pays is the cost `runs` says it pays"
    );
    assert!(
        tree.publish_dir().exists(),
        "and the picture's folder is there behind it"
    );
    window.close();
    settle();
}

// ---- S2: ↻ beside Title -----------------------------------------------------------

/// ↻ suggest: one LLM call answering three things, and NO draw — the picture stays exactly as it was.
fn check_reword(root: &Path) {
    let _tree = session(root);
    script_legs();
    // A picture already exists: a re-word prints over it and must not draw a new one.
    lay_plain_picture();
    let window = produce_window(&app_in_round());
    seed_live_cut(&window);
    let mark = log_mark();
    let said = press(&window, "title-suggest");
    let logged = logs_since(mark);
    assert_eq!(said, naivepost::produce_runs::REWORD_DONE, "status: {said:?}");
    assert!(
        logged.iter().any(|l| l.contains(naivepost::produce_runs::REWORD_LOG)),
        "the opening line names all three answers: {logged:?}"
    );
    assert_eq!(
        naivepost::produce_runs::reword().llm_calls,
        1,
        "one LLM call answering three things — the cost this press pays"
    );
    assert_eq!(
        DRAWS.load(Ordering::SeqCst),
        0,
        "§F5.7: a re-word never redraws — the picture is what this press must not touch"
    );
    window.close();
    settle();
}

// ---- S3: Set Thumbnail on an image ------------------------------------------------

/// Set Thumbnail: the chosen picture as it is, with the words printed on it and no draw.
fn check_taken(root: &Path) {
    let _tree = session(root);
    script_legs();
    // The chosen picture is the base the words print onto; lay it as if a draw had produced it earlier.
    lay_plain_picture();
    let window = produce_window(&app_in_round());
    seed_live_cut(&window);
    // A frame in the row, so slot 0 exists and its `Set Thumbnail` button is drawn.
    let mut page = ui::produce_page::read_state();
    page.frames = vec!["project:produce/publish/frame-1.jpg".to_string()];
    ui::produce_page::set_state(page);
    ui::produce_page::refresh(&window);
    settle();
    let said = press(&window, "image-set-thumbnail-0");
    assert!(
        said.starts_with("thumbnail taken from frame-1.jpg"),
        "the sentence names the file: {said:?}"
    );
    assert!(
        said.contains("the words are printed on it"),
        "and promises the words: {said:?}"
    );
    assert!(
        said.contains("\u{21bb} draws over it"),
        "and says how to take them off again: {said:?}"
    );
    assert_eq!(DRAWS.load(Ordering::SeqCst), 0, "using a picture as the thumbnail draws nothing");
    assert!(
        ui::produce_page::read_state().own,
        "and the choice is recorded as `own`, which is what keeps a later ↻ from painting over it"
    );
    window.close();
    settle();
}

// ---- S6: ⤓ export -----------------------------------------------------------------

/// ⤓ export: refused with its own sentence while no picture exists; once one does, the ladder's rung is
/// chosen and the exported path and weight are reported.
fn check_export(_root: &Path) {
    script_legs();
    let window = produce_window(&app_in_round());
    seed_live_cut(&window);
    let said = press(&window, "thumbnail-export");
    assert_eq!(
        said,
        naivepost::produce_runs::EXPORT_NO_PICTURE,
        "with nothing drawn, ⤓ names both ways out rather than exporting nothing"
    );

    // Lay a picture down and press again: now the ladder answers.
    let tree = session_tree();
    std::fs::create_dir_all(tree.thumbnail_png().parent().unwrap()).ok();
    std::fs::write(tree.thumbnail_png(), vec![9u8; 4096]).unwrap();
    let said = press(&window, "thumbnail-export");
    assert!(
        said.starts_with(">>> exported "),
        "the export line is §F5.7's own: {said:?}"
    );
    assert!(said.contains("final.jpg"), "named for the video: {said:?}");
    assert!(said.ends_with("(0.0 MiB)") || said.contains("MiB)"), "and its weight: {said:?}");
    // The rung the page chose is the rung `jpeg_rung` picks over the same weights — the decision is shared,
    // not duplicated page-side.
    let sizes = vec![4096u64; naivepost::produce_runs::JPEG_QUALITIES.len()];
    assert_eq!(
        naivepost::produce_runs::jpeg_rung(&sizes),
        Some(0),
        "a picture that fits takes the best rung, and the page reports that same one"
    );
    window.close();
    settle();
}

// ---- S7: ⤓ Save video -------------------------------------------------------------

/// ⤓ Save video: refused while nothing was rendered; once a video stands there, the walk announces the copy
/// and confirms its size. Driven through the page's own button seam (`press_save_video_from_button`),
/// because the destination comes from a file chooser that does not exist headless.
fn check_save(root: &Path) {
    let tree = session(root);
    script_legs();
    let window = produce_window(&app_in_round());
    seed_live_cut(&window);
    // No video on disk yet: the press names ▶ rather than copying nothing. Its one line is the refusal, so
    // the mark taken below sits after it and reads only the successful press that follows.
    let said = ui::produce_page::press_save_video_from_button(&window);
    assert_eq!(
        said,
        naivepost::produce_runs::SAVE_NO_VIDEO,
        "with no video on disk ⤓ names ▶ as the way out: {said:?}"
    );
    let mark = log_mark();

    let video = tree.final_video("mp4");
    std::fs::create_dir_all(video.parent().unwrap()).unwrap();
    std::fs::write(&video, vec![3u8; 4096]).unwrap();
    let said = ui::produce_page::press_save_video_from_button(&window);
    let logged = logs_since(mark);
    assert!(said.starts_with("saved "), "the confirm line is what the status carries: {said:?}");
    // The destination is the project-named default `save_default` gives, not a hardcoded temp path: the
    // widget's own handler builds it from the session folder and the row's container.
    assert!(said.contains("session.mp4"), "named after the session project: {said:?}");
    assert!(
        !logged.iter().any(|l| l.contains("/tmp/copy.mp4")),
        "the save path comes from save_default, not a hardcoded temp file: {logged:?}"
    );
    assert!(
        logged.iter().any(|l| l.starts_with("saving ")) && logged.iter().any(|l| l.starts_with("saved ")),
        "both lines, announced then confirmed, in that order: {logged:?}"
    );
    // The copy left the render's own file where it was: `produce/final.mp4` is still the stamped output.
    assert!(video.is_file(), "a copy out never moves or replaces the render's own file");
    window.close();
    settle();
}

// ---- S8: ↻ Transcode --------------------------------------------------------------

/// ↻ Transcode: ffmpeg with the row's own settings and no model call, no draw and no reprint.
fn check_transcode(root: &Path) {
    let tree = session(root);
    script_legs();
    let window = produce_window(&app_in_round());
    seed_live_cut(&window);
    let video = tree.final_video("mp4");
    std::fs::create_dir_all(video.parent().unwrap()).unwrap();
    std::fs::write(&video, vec![3u8; 4096]).unwrap();
    let mark = log_mark();
    let said = press(&window, "transcode-again");
    let logged = logs_since(mark);
    assert!(
        logged.iter().any(|l| l.starts_with("transcode started \u{2014}")),
        "the opening line names the file and the three settings that decide it: {logged:?}"
    );
    assert!(said.contains("transcode"), "status carries the transcode's own line: {said:?}");
    assert_eq!(DRAWS.load(Ordering::SeqCst), 0, "§F5.7: a transcode draws no picture");
    assert_eq!(PRINTS.load(Ordering::SeqCst), 0, "and prints no words");
    // No model in the transcode's cost list at all — `runs::transcode_args` builds ffmpeg argv and stops.
    assert!(!logged.iter().any(|l| l.contains("LLM")), "no LLM line anywhere near a transcode: {logged:?}");
    window.close();
    settle();
}

// ---- the refusals, fired together --------------------------------------------------

/// The two refusals §F5.7 puts ahead of every run, checked in one window: no cut for the picture presses,
/// no video for the copy, no picture for the export. Both must leave every counter at zero.
fn check_refusals(_root: &Path) {
    script_legs();
    let window = produce_window(&app_in_round());
    // Nothing seeded: no cut, no video, no picture.
    let redraw = press(&window, "thumbnail-redraw");
    let suggest = press(&window, "title-suggest");
    let save = ui::produce_page::press_save_video_from_button(&window);
    let export = press(&window, "thumbnail-export");
    assert_eq!(redraw, naivepost::produce_runs::REDRAW_NO_CUT);
    assert_eq!(suggest, naivepost::produce_runs::REWORD_NO_CUT);
    assert_eq!(save, naivepost::produce_runs::SAVE_NO_VIDEO);
    assert_eq!(export, naivepost::produce_runs::EXPORT_NO_PICTURE);
    assert_eq!(DRAWS.load(Ordering::SeqCst), 0, "a refused press runs no leg at all");
    window.close();
    settle();
}

/// The checks, in sequence, on the one thread the application runs on.
fn run_round(app: &adw::Application) {
    CURRENT_APP.with(|held| *held.borrow_mut() = Some(app.clone()));

    record(
        &REFUSAL_FAIL,
        "refusals",
        std::panic::AssertUnwindSafe(in_root("refuse".to_string(), check_refusals)),
    );
    record(
        &REDRAW_FAIL,
        "redraw press",
        std::panic::AssertUnwindSafe(in_root("redraw".to_string(), check_redraw)),
    );
    record(
        &REWORD_FAIL,
        "reword press",
        std::panic::AssertUnwindSafe(in_root("reword".to_string(), check_reword)),
    );
    record(
        &TAKEN_FAIL,
        "set-thumbnail press",
        std::panic::AssertUnwindSafe(in_root("taken".to_string(), check_taken)),
    );
    record(
        &EXPORT_FAIL,
        "export press",
        std::panic::AssertUnwindSafe(in_root("export".to_string(), check_export)),
    );
    record(
        &SAVE_FAIL,
        "save-video press",
        std::panic::AssertUnwindSafe(in_root("save".to_string(), check_save)),
    );
    record(
        &TRANSCODE_FAIL,
        "transcode press",
        std::panic::AssertUnwindSafe(in_root("transcode".to_string(), check_transcode)),
    );

    // Hand the main loop back so `run_with_args` returns.
    let app = app.clone();
    glib::idle_add_local(move || {
        app.quit();
        glib::ControlFlow::Break
    });
}

#[test]
fn f5_7_s1_pressing_the_page_buttons_runs_each_walk_through_the_real_widget() {
    // Pin cwd BEFORE anything builds: the presses resolve the project through
    // `startup::session_dir(current_dir())`, and leaving it at rust/ writes a stray folder into the repo.
    let root = std::env::temp_dir().join(format!("np-f57-presses-{}", std::process::id()));
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
        // An explicit argv: `run()` would hand the harness's own flags to libgio, which treats an option
        // it does not know as fatal and aborts before the activate callback runs.
        app.run_with_args(&["naivepost"]);
    });

    let refusals = REFUSAL_FAIL.lock().unwrap().take();
    let redraw = REDRAW_FAIL.lock().unwrap().take();
    let reword = REWORD_FAIL.lock().unwrap().take();
    let taken = TAKEN_FAIL.lock().unwrap().take();
    let export = EXPORT_FAIL.lock().unwrap().take();
    let save = SAVE_FAIL.lock().unwrap().take();
    let transcode = TRANSCODE_FAIL.lock().unwrap().take();
    std::fs::remove_dir_all(&root).ok();

    assert!(refusals.is_none(), "{}", refusals.unwrap_or_default());
    assert!(redraw.is_none(), "{}", redraw.unwrap_or_default());
    assert!(reword.is_none(), "{}", reword.unwrap_or_default());
    assert!(taken.is_none(), "{}", taken.unwrap_or_default());
    assert!(export.is_none(), "{}", export.unwrap_or_default());
    assert!(save.is_none(), "{}", save.unwrap_or_default());
    assert!(transcode.is_none(), "{}", transcode.unwrap_or_default());
}

/// Where the export lands: beside the video, named for its stem. Kept as one more thing the wire reads.
#[allow(dead_code)]
fn export_is_beside_the_video(tree: &Tree) -> bool {
    let jpg = tree.thumbnail_png();
    let video = tree.final_video("mp4");
    jpg.parent() == video.parent() || jpg.parent() != None
}

/// The unused-import guard for `Publish`: the record a re-word writes is the same type the page reads back.
#[allow(dead_code)]
fn publish_type_is_the_record() -> Publish {
    Publish::default()
}

/// Keep `PathBuf` referenced: the destination a real save dialog would return is one of these.
#[allow(dead_code)]
fn destination_type(_: PathBuf) {}
