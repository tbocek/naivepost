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

/// **3** Set Thumbnail from a slot: record the choice (`own`, so a later ↻ does not paint over it), then
/// let the shared print pass put the marked words over the picture that now stands.
pub fn set_thumbnail(window: &adw::ApplicationWindow, index: usize) -> String {
    let state = produce_page::read_state();
    let Some(path) = state.frames.get(index) else {
        return report(window, "there is no such slot to set from".to_string());
    };
    produce_page::set_own_thumbnail();
    produce_page::refresh(window);
    let out = crate::produce_presses::press_set_thumbnail(path, &legs(), crate::ui::window::log_line);
    crate::ui::produce_languages::report(&out);
    report(window, out.status)
}

/// **6** ⤓ export: the JPEG an uploader takes, under its 2 MB limit. The walk picks the rung; the byte
/// count this press was handed is what gets reported, so the log never prints a guessed size.
pub fn export_thumbnail(window: &adw::ApplicationWindow, bytes: u64) -> String {
    let pressed = legs();
    if !(pressed.has_picture)() {
        return report(window, runs::EXPORT_NO_PICTURE.to_string());
    }
    let out = crate::produce_presses::press_export(&runs::export_name("final"), &pressed);
    if out.refused {
        return report(window, out.status);
    }
    report(window, runs::exported_log(&runs::export_name("final"), bytes))
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

/// **10** ↻ Transcode: an encode with no model call. Refused while a render runs.
pub fn transcode_again(window: &adw::ApplicationWindow) -> String {
    let settings = produce_page::row_settings();
    let out = crate::produce_presses::press_transcode(
        "final.mp4",
        "final.mp4",
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
    let out = crate::produce_presses::press_save_video(to, bytes, &legs(), crate::ui::window::log_line);
    crate::ui::produce_languages::report(&out);
    report(window, out.status)
}

/// **10** ⤓ Save video as the widget fires it: no path arrives from a chooser in a headless run, so the
/// destination is §F5.7's own default — `<project>.<container>` — and the weight is read off the RENDER's
/// own file rather than assumed (the copy has not been made yet when the line is printed).
pub fn save_video_from_button(window: &adw::ApplicationWindow) -> String {
    let container = produce_page::container_of_row();
    let name = runs::save_default(&project_name(), container);
    let bytes = std::fs::metadata(produce_page::rendered_video())
        .map(|m| m.len())
        .unwrap_or(0);
    save_video(window, &name, bytes)
}

/// The project's own name for the save default: the session folder's, falling back to the app's name when
/// the folder is not a project tree at all. `Tree::name` borrows the tree, so the copy happens inside.
fn project_name() -> String {
    let tree = crate::layout::Tree::new(&crate::startup::session_dir(
        &std::env::current_dir().unwrap_or_default(),
    ))
    .ok();
    match tree {
        Some(tree) => tree.name().unwrap_or("naivepost").to_string(),
        None => "naivepost".to_string(),
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
        || produce_page::read_state().rendering,
        || produce_page::container_word(),
        produced_spans,
    );
    // This page reports its own reprint passes; the F5.6 words half counts its own elsewhere.
    pressed.count_prints = true;
    pressed
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
        let raw = if seg.dur > 0.0 { seg.dur } else { (seg.e - seg.s).max(0.0) };
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
