//! F5.7 Page runs — the wiring that turns a Produce-page click into a `produce_presses` walk.
//!
//! Split out of `ui::produce_page`, which is at its size budget. The page keeps one-line forwarders so
//! every call site (the `wire()` table, `wire_slot_buttons`, `produce_words`, the tests) reads exactly as
//! before; everything that *decides* what a press does lives in `crate::produce_presses`, and everything
//! that GATHERS what the press needs lives here. Three layers, one job each:
//!
//! - `produce_presses` — the order: refuse → open → do → end, and the cost counts.
//! - this module — the legs: what the page's held state, the live cut and the session folder answer when a
//!   leg asks. Nothing here decides an order or prints a spec sentence.
//! - `produce_page` — the widgets, and the status line a press ends on.
//!
//! Legs are built per press rather than held: each closure reads the world at call time, so a state change
//! between two clicks lands in the next one, and a test can swap a scripted seam between presses without
//! rebuilding anything.

use adw::prelude::*;

use crate::produce_runs as runs;
use crate::ui::produce_page;

/// The frame images the row can point at: every extracted JPEG this session has, as `(seconds in the
/// recording, path)` pairs across every footage source's prepare folder. Empty when nothing is extracted.
fn session_frame_files() -> Vec<(f64, std::path::PathBuf)> {
    frames_of(&frame_source_stems())
}

/// [`session_frame_files`] over stems already taken: the folder listing, which a worker may do.
fn frames_of(stems: &[String]) -> Vec<(f64, std::path::PathBuf)> {
    let tree = crate::ui::produce_languages::session_tree();
    let mut out: Vec<(f64, std::path::PathBuf)> = Vec::new();
    let zero = crate::frames_run::session_zero(&tree);
    for base in stems.iter().cloned() {
        out.extend(crate::produce_upload_media::listed_frames(
            &tree.frames_dir(&base),
            zero,
        ));
    }
    // One rule for "the same picture twice": two cameras showing the same instant is one image to send, and
    // the moment is what makes an instant — not the file name a second source happens to write it into.
    crate::produce_upload_media::frame_files(&out)
}

/// Which prepare folders hold frames a slot could be set from or added to the row: one per footage recording of
/// the live project, named the way Prepare names them (the file's stem), the same folders the run's request
/// reads. The first rewrite named them after the cut's cameras (`cam0`), a folder Prepare never writes, so no
/// slot ever found its picture.
pub fn frame_source_stems() -> Vec<String> {
    crate::ui::window::live_project()
        .sources
        .iter()
        .filter(|source| source.footage)
        .map(|source| crate::ui::produce_languages::source_stem(&source.path))
        .collect()
}

/// **3** Set Thumbnail from a slot: record the choice (`own`, so a later ↻ does not paint over it), put that
/// picture's cropped pixels where the print pass reads them (§F5.6 S3: the chosen frame IS the thumbnail,
/// no model, no GPU), then let the shared print pass letter the picture that now stands.
pub fn set_thumbnail(window: &adw::ApplicationWindow, index: usize) -> String {
    let state = produce_page::read_state();
    let Some(path) = state.frames.get(index) else {
        return report(window, "there is no such slot to set from".to_string());
    };
    produce_page::set_own_thumbnail();
    produce_page::refresh(window);
    let out = set_thumbnail_walk(path, &session_frame_files(), thumb_size(), &legs());
    land_outcome(window, out.clone());
    out.status
}

/// The slot's ↻ Set Thumbnail as its button fires it: the choice is recorded and drawn at once, the crop
/// and the print (a decode and an encode) walk on a worker (§03-shell §1).
pub fn set_thumbnail_from_button(window: &adw::ApplicationWindow, index: usize) {
    let state = produce_page::read_state();
    let Some(path) = state.frames.get(index).cloned() else {
        report(window, "there is no such slot to set from".to_string());
        return;
    };
    produce_page::set_own_thumbnail();
    produce_page::refresh(window);
    // The stems and the size read the live cut, so they are taken here; the folders are listed on the worker.
    let stems = frame_source_stems();
    let size = thumb_size();
    on_worker(
        window,
        "",
        move |legs| set_thumbnail_walk(&path, &frames_of(&stems), size, legs),
        land_outcome,
    );
}

/// The chosen picture cropped where the print pass reads it, then the shared print: the half of Set
/// Thumbnail that reads and writes pictures.
fn set_thumbnail_walk(
    path: &str,
    files: &[(f64, std::path::PathBuf)],
    (wide, high): (u32, u32),
    legs: &crate::produce_presses::Legs,
) -> crate::produce_presses::Outcome {
    // The crop is to the video's own shape at the thumbnail tier (S6's size rule), and the file behind the
    // entry is looked up the same way the request looks its images up, so a slot press and a draw cannot
    // disagree about which JPEG a row entry means.
    let tree = crate::ui::produce_languages::session_tree();
    let record = crate::project::Publish {
        frames: vec![path.to_string()],
        ..Default::default()
    };
    if let Err(why) = crate::produce_upload_media::write_own_picture(
        &tree,
        &record,
        files,
        wide,
        high,
        crate::ui::window::log_line,
    ) {
        crate::ui::window::log_line(&format!("!!! {why}"));
    }
    crate::produce_presses::press_set_thumbnail(path, legs, crate::ui::window::log_line)
}

/// The size a chosen picture is cropped to: the cut's aspect at the thumbnail tier, falling back to the
/// first footage frame's measured shape when the cut says "source" (§9 makes that frame the video's).
/// `pub` because a test measures the crop against the SAME number rather than restating 1280x720 by hand.
pub fn thumb_size() -> (u32, u32) {
    let tree = crate::ui::produce_languages::session_tree();
    let (w, h) = crate::ui::produce_snapshot::first_footage_shape(
        &tree,
        &crate::ui::cut_folds::page_recordings(),
    );
    let aspect = produce_page::held_window()
        .map(|window| crate::ui::window::review_cut_of(&window).aspect)
        .unwrap_or_default();
    crate::produce_screen::thumb_box(&aspect, w.max(1) as u32, h.max(1) as u32)
}

/// The path a save dialog would answer with, if anyone answered it.
///
/// A headless run (this container, CI, `just snapshot`) has no dialog to answer, and neither does the
/// thumbnail's Change… door (`produce_slots::next_stood_in`): the app's convention is a STAND-IN at the seam
/// rather than a button that does nothing, so the press still runs the real copy. What the seam answers is
/// the only path a press ever gets — a real `FileChooserNative` response would forward here exactly as
/// `cut_svg::open_svg_chooser` forwards its pick to `svg_chosen`.
pub fn asked_path() -> Option<String> {
    ASKED_PATH.with(|held| held.borrow().clone())
}

/// Record the path a save dialog settled on: `Some` for a chosen one, `None` to clear it (the person moved on
/// from what the dialog was opened for, so the next press offers its own default again).
/// Called from `ui::produce_page::set_setting` when the Container row changes — a different container, a
/// different default name — and by a test that points the same door at a path of its own, the way every other
/// leg here is scripted.
pub fn answer_path(answer: Option<&str>) {
    ASKED_PATH.with(|held| *held.borrow_mut() = answer.map(str::to_string));
}

thread_local! {
    /// The path the save door last answered with; `None` means nobody typed anything.
    static ASKED_PATH: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

/// What the save dialog would be OFFERED for ⤓ Save video: `<project>.<container>`, the row's own container.
/// A test reads this instead of restating the spelling.
pub fn offered_save_name() -> String {
    runs::save_default(&project_name(), produce_page::container_of_row())
}

/// …and for ⤓ export: `<project>-thumbnail.jpg`, with the extension already forced, beside the video. This is
/// what the export dialog is OPENED on and, when nobody answers it, the path the press writes — `export_path`
/// is the only caller besides a test that reads the offer back.
pub fn offered_export_name() -> String {
    crate::ui::produce_languages::session_tree()
        .export_jpg()
        .display()
        .to_string()
}

/// The path ⤓ writes: the save dialog's answer when it answered (extension forced), else this project's own
/// offer from [`offered_export_name`] — one rule stated once, so the name the dialog shows and the file the
/// ladder writes cannot drift apart. The old `export_name("final")` name is the `<video>` tag's poster file,
/// not what a person uploads elsewhere.
pub fn export_path(asked: Option<&str>) -> String {
    match asked.filter(|typed| !typed.trim().is_empty()) {
        Some(typed) => runs::force_suffix(typed, runs::EXPORT_SUFFIX),
        None => offered_export_name(),
    }
}

/// **6** ⤓ export: the JPEG an uploader takes, under its 2 MB limit. The walk runs the ladder (every rung
/// really encoded) and reports the weight of the file it wrote, so no byte count is ever assumed.
pub fn export_thumbnail(window: &adw::ApplicationWindow) -> String {
    let pressed = legs();
    if !(pressed.has_picture)() {
        return report(window, runs::EXPORT_NO_PICTURE.to_string());
    }
    // The dialog's answer is what the path door holds: nothing typed headless, the typed name when a chooser
    // ran (and the page's own stand-in, whose answer a test installs the same way).
    let out = crate::produce_presses::press_export(&export_path(asked_path().as_deref()), &pressed);
    if out.refused {
        return report(window, out.status);
    }
    report(window, out.status)
}

/// **6** ↻ redraw: one sd.cpp call, nothing rewritten when the words are left alone.
pub fn redraw_thumbnail(window: &adw::ApplicationWindow, only_thumbnail: bool) -> String {
    let out = crate::produce_presses::press_redraw_thumbnail(
        only_thumbnail,
        &legs(),
        crate::ui::window::log_line,
    );
    crate::ui::produce_languages::report(&out);
    produce_page::refresh(window);
    report(window, out.status)
}

/// **8** ↻ beside Title: the only thing on this page that asks a model to rewrite text, and it rewrites
/// all three written things at once so they cannot disagree about what the video is. No draw.
pub fn reword(window: &adw::ApplicationWindow) -> String {
    let out = crate::produce_presses::press_reword(&legs(), crate::ui::window::log_line);
    crate::ui::produce_languages::report(&out);
    report(window, out.status)
}

/// **10** ↻ Transcode: an encode with no model call. Refused while a render runs. The file is the page's OWN
/// output — `produce/final.<container>` for whatever the Container row says (§1's Output line), not a
/// hardcoded `.mp4`: with the row on webm this must re-encode the webm, and `transcode_args` picks its audio
/// codec and mux off the same row's container.
pub fn transcode_again(window: &adw::ApplicationWindow) -> String {
    let settings = produce_page::row_settings();
    let video = crate::ui::produce_languages::session_tree()
        .final_video(produce_page::container_word().as_str())
        .display()
        .to_string();
    let out = crate::produce_presses::press_transcode(
        &video,
        &video,
        &settings,
        &legs(),
        crate::ui::window::log_line,
    );
    crate::ui::produce_languages::report(&out);
    report(window, out.status)
}

/// **10** ⤓ Save video: a copy out to a chosen path, so `produce/final` keeps its place as the stamped
/// output. The path is what the host's save dialog answered.
pub fn save_video(window: &adw::ApplicationWindow, to: &str, bytes: u64) -> String {
    let out =
        crate::produce_presses::press_save_video(to, bytes, &legs(), crate::ui::window::log_line);
    crate::ui::produce_languages::report(&out);
    report(window, out.status)
}

/// **10** ⤓ Save video as the widget fires it: the destination is what the save door answered (§F5.7 "a copy
/// to a chosen path"), falling back to that door's own offer — `<project>.<container>` — and the weight is
/// read off the RENDER's own file rather than assumed (the copy has not been made yet when the line prints).
pub fn save_video_from_button(window: &adw::ApplicationWindow) -> String {
    let to = save_destination();
    let bytes = std::fs::metadata(produce_page::rendered_video())
        .map(|m| m.len())
        .unwrap_or(0);
    save_video(window, &to, bytes)
}

/// **10** ⤓ Save video as the button fires it: the copy is a whole video's bytes, so it walks on a worker
/// (§03-shell §1) and is said when it lands. Destination and weight as [`save_video_from_button`] reads them.
pub fn save_video_on_worker(window: &adw::ApplicationWindow) {
    let to = save_destination();
    let bytes = std::fs::metadata(produce_page::rendered_video())
        .map(|m| m.len())
        .unwrap_or(0);
    on_worker(
        window,
        "transcode-save",
        move |legs| {
            crate::produce_presses::press_save_video(&to, bytes, legs, crate::ui::window::log_line)
        },
        land_outcome,
    );
}

/// Where ⤓ Save video copies to: the save door's answer, else its own offer `<project>.<container>`.
pub(crate) fn save_destination() -> String {
    match asked_path().filter(|typed| !typed.trim().is_empty()) {
        Some(typed) => typed,
        None => crate::ui::produce_languages::session_tree()
            .produce_dir()
            .join(offered_save_name())
            .display()
            .to_string(),
    }
}

/// The project's own name for the save default: the session folder's, falling back to the app's name when
/// the folder is not a project tree at all.
fn project_name() -> String {
    let dir = crate::startup::open_dir();
    match crate::layout::Tree::new(dir) {
        Ok(tree) => tree.name().unwrap_or("naivepost").to_string(),
        Err(_) => "naivepost".to_string(),
    }
}

/// This page's legs for one press.
fn legs() -> crate::produce_presses::Legs {
    let mut pressed = crate::ui::produce_languages::legs(
        || {
            produce_page::held_window()
                .map(|w| !crate::ui::window::review_cut_of(&w).segs.is_empty())
                .unwrap_or(false)
        },
        // §08 "⤓ Save video and ↻ Transcode both refuse while a render runs": asked of the run bar itself, the
        // one place that knows; the page state's flag it read before was set by nothing but tests.
        || {
            produce_page::held_window().is_some_and(|w| {
                crate::ui::window_doors::running_step(&w) == Some(crate::run::Step::Produce)
            })
        },
        produce_page::container_word,
        produced_spans,
    );
    // This page reports its own reprint passes; the F5.6 words half counts its own elsewhere.
    pressed.count_prints = true;
    // §F5.7: ↻ over the thumbnail clears `own` — in the page's held state and in `publish.json`, so the
    // picture is the app's again after the press rather than only on screen.
    pressed.clear_own = Box::new(clear_own);
    pressed
}

/// §F5.7's "clears `own`": in the page's held state and in `publish.json`.
fn clear_own() -> Result<(), String> {
    produce_page::clear_own_thumbnail();
    let tree = crate::ui::produce_languages::session_tree();
    // The held state merged over the file, then the flag cleared: the file also carries what no page row
    // holds (the crop, the title box), and writing the rows alone would drop them.
    let mut publish = crate::ui::produce_snapshot::merge_publish(
        Some(crate::ui::produce_snapshot::publish_of_state(
            &produce_page::read_state(),
        )),
        &crate::publish::load(&tree).unwrap_or_default(),
    );
    publish.own = false;
    crate::ui::produce_languages::save_publish(&tree, &publish)
}

/// What a press's legs read off the window, taken there before the press walks on its worker (§03-shell §1):
/// the live cut, the run bar and the produced spans are the window's, not the worker's.
struct Found {
    has_cut: bool,
    rendering: bool,
    spans: Vec<(f64, f64)>,
}

/// Take [`Found`] on the window thread, asking exactly what [`legs`] asks at call time.
fn found() -> Found {
    let has_cut = produce_page::held_window()
        .map(|w| !crate::ui::window::review_cut_of(&w).segs.is_empty())
        .unwrap_or(false);
    let rendering = produce_page::held_window().is_some_and(|w| {
        crate::ui::window_doors::running_step(&w) == Some(crate::run::Step::Produce)
    });
    Found {
        has_cut,
        rendering,
        spans: produced_spans(),
    }
}

/// [`legs`] on the worker: the window's answers are the ones [`found`] took, the page's state is the copy
/// [`crate::ui::produce_worker::Carried`] loaded. A cleared `own` is noted so the landing clears the
/// window's own copy too.
fn worker_legs(
    found: Found,
) -> (
    crate::produce_presses::Legs,
    std::rc::Rc<std::cell::Cell<bool>>,
) {
    let Found {
        has_cut,
        rendering,
        spans,
    } = found;
    let mut pressed = crate::ui::produce_languages::legs(
        move || has_cut,
        move || rendering,
        produce_page::container_word,
        move || spans.clone(),
    );
    pressed.count_prints = true;
    let cleared = std::rc::Rc::new(std::cell::Cell::new(false));
    let noted = std::rc::Rc::clone(&cleared);
    pressed.clear_own = Box::new(move || {
        noted.set(true);
        clear_own()
    });
    (pressed, cleared)
}

/// One page press on its worker (§03-shell §1): the press named `button` greys while `work` walks with the
/// worker's legs, and `land` takes the answer on the window thread. Ignored while another press walks.
fn on_worker<T: Send + 'static>(
    window: &adw::ApplicationWindow,
    button: &str,
    work: impl FnOnce(&crate::produce_presses::Legs) -> T + Send + 'static,
    land: impl FnOnce(&adw::ApplicationWindow, T) + 'static,
) {
    let found = found();
    let w = window.clone();
    crate::ui::produce_worker::press(
        crate::ui::line_step_button(window, button),
        move || {
            let (legs, cleared) = worker_legs(found);
            let answer = work(&legs);
            (answer, cleared.get())
        },
        move |(answer, cleared)| {
            if cleared {
                produce_page::clear_own_thumbnail();
            }
            land(&w, answer);
        },
    );
}

/// The ending every outcome-shaped press lands on: the failure in the log, the sentence on the status line.
fn land_outcome(window: &adw::ApplicationWindow, out: crate::produce_presses::Outcome) {
    crate::ui::produce_languages::report(&out);
    report(window, out.status);
}

/// **8** ↻ beside Title, as the button fires it: [`reword`] on a worker.
pub fn reword_from_button(window: &adw::ApplicationWindow) {
    on_worker(
        window,
        "title-suggest",
        |legs| crate::produce_presses::press_reword(legs, crate::ui::window::log_line),
        land_outcome,
    );
}

/// **6** ↻ redraw, as the button fires it: [`redraw_thumbnail`] on a worker.
pub fn redraw_from_button(window: &adw::ApplicationWindow, only_thumbnail: bool) {
    on_worker(
        window,
        "thumbnail-redraw",
        move |legs| {
            crate::produce_presses::press_redraw_thumbnail(
                only_thumbnail,
                legs,
                crate::ui::window::log_line,
            )
        },
        |window, out| {
            crate::ui::produce_languages::report(&out);
            produce_page::refresh(window);
            report(window, out.status);
        },
    );
}

/// **6** ⤓ export, as the button fires it: [`export_thumbnail`] on a worker.
pub fn export_from_button(window: &adw::ApplicationWindow) {
    let path = export_path(asked_path().as_deref());
    on_worker(
        window,
        "thumbnail-export",
        move |legs| {
            if !(legs.has_picture)() {
                return Err(runs::EXPORT_NO_PICTURE.to_string());
            }
            Ok(crate::produce_presses::press_export(&path, legs).status)
        },
        |window, said: Result<String, String>| {
            report(window, said.unwrap_or_else(|refused| refused));
        },
    );
}

/// **10** ↻ Transcode, as the button fires it: [`transcode_again`] on a worker.
pub fn transcode_from_button(window: &adw::ApplicationWindow) {
    let settings = produce_page::row_settings();
    let video = crate::ui::produce_languages::session_tree()
        .final_video(produce_page::container_word().as_str())
        .display()
        .to_string();
    on_worker(
        window,
        "transcode-again",
        move |legs| {
            crate::produce_presses::press_transcode(
                &video,
                &video,
                &settings,
                legs,
                crate::ui::window::log_line,
            )
        },
        land_outcome,
    );
}

/// Each kept segment as (start, end) on the produced clock — the numbers ↻ beside Title speaks of, and the
/// same accumulation the run's brief uses.
fn produced_spans() -> Vec<(f64, f64)> {
    let Some(window) = produce_page::held_window() else {
        return Vec::new();
    };
    let mut spans = Vec::new();
    let mut at = 0.0;
    for seg in crate::ui::window::review_cut_of(&window).segs {
        let raw = if seg.dur > 0.0 {
            seg.dur
        } else {
            (seg.e - seg.s).max(0.0)
        };
        let kept = crate::narration::output_seconds(raw, seg.rate);
        spans.push((at, at + kept));
        at += kept;
    }
    spans
}

/// Put one press's sentence on the status line AND in the log, the page's two output channels: the ending
/// is what a person reads now, and the log is what they read afterwards. The `>>>` opening lines reach the
/// log through the walk's own `log` leg; this is where the closing sentence goes to both.
fn report(window: &adw::ApplicationWindow, said: String) -> String {
    if let Some(status) = crate::ui::window::find_status(window.upcast_ref()) {
        status.set_text(&said);
    }
    crate::ui::window::log_line(&said);
    said
}
