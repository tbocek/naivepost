//! F5.7 Page runs — the wiring that turns a Produce-page click into a `produce_presses` walk.
//!
//! `crate::produce_presses` decides the order (refuse → open → do → end, and the cost counts); this module
//! builds the legs, i.e. what the page's held state, the live cut and the session folder answer; `produce_page`
//! holds the widgets. Legs are built per press so each closure reads the world at call time and a test can
//! swap a scripted seam between presses.

use adw::prelude::*;

use crate::produce_runs as runs;
use crate::ui::produce_page;
use crate::ui::produce_page_state::{ProducePage, PAGE};

/// The frame images the row can point at: every extracted JPEG this session has, as `(seconds in the
/// recording, path)` pairs across every footage source's prepare folder.
fn session_frame_files() -> Vec<(f64, std::path::PathBuf)> {
    frames_of(&frame_source_stems())
}

/// [`session_frame_files`] over stems already taken: the folder listing, which a worker may do.
fn frames_of(stems: &[String]) -> Vec<(f64, std::path::PathBuf)> {
    let tree = crate::ui::produce_languages::session_tree();
    let mut out: Vec<(f64, std::path::PathBuf)> = Vec::new();
    let zero = crate::frames_run::session_zero(&tree);
    for base in stems {
        out.extend(crate::produce_upload_media::listed_frames(
            &tree.frames_dir(base),
            zero,
        ));
    }
    // One rule for "the same picture twice": two cameras showing the same instant is one image to send.
    crate::produce_upload_media::frame_files(&out)
}

/// Which prepare folders hold frames a slot could be set from: one per footage recording of the live project,
/// named as Prepare names them (the file's stem), the folders the run's request reads. Naming them after the
/// cut's cameras (`cam0`) found no picture, as Prepare never writes that folder.
pub fn frame_source_stems() -> Vec<String> {
    crate::ui::window::live_project()
        .sources
        .iter()
        .filter(|source| source.footage)
        .map(|source| crate::ui::produce_languages::source_stem(&source.path))
        .collect()
}

/// **3** Set Thumbnail from a slot: record the choice (`own`, so ▶ keeps it), put the picture's cropped pixels
/// where the print pass reads them (§F5.6 S3: the chosen frame IS the thumbnail, no model, no GPU), then let
/// the shared print pass letter it.
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

/// The slot's ↻ Set Thumbnail as its button fires it: the choice is recorded and drawn at once; the crop
/// and the print walk on a worker (§03-shell §1).
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

/// The chosen picture cropped where the print pass reads it, then the shared print.
fn set_thumbnail_walk(
    path: &str,
    files: &[(f64, std::path::PathBuf)],
    (wide, high): (u32, u32),
    legs: &crate::produce_presses::Legs,
) -> crate::produce_presses::Outcome {
    // Crop to the video's shape at the thumbnail tier (S6), looking the file up as the request does so a
    // slot press and a draw cannot disagree about which JPEG an entry means.
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
        // With an earlier picture standing the words still go on; with none there is nothing reported as taken.
        crate::ui::window::log_line(&format!("!!! {why}"));
        if !tree.thumbnail_plain_png().is_file() {
            return crate::produce_presses::Outcome {
                status: runs::ending("the thumbnail", "", false, false),
                error: Some(why),
                ..Default::default()
            };
        }
    }
    crate::produce_presses::press_set_thumbnail(path, legs, crate::ui::window::log_line)
}

/// The size a chosen picture is cropped to: the cut's aspect at the thumbnail tier, falling back to the
/// first footage frame's measured shape when the cut says "source" (§9). `pub` so a test measures the same number.
pub fn thumb_size() -> (u32, u32) {
    let tree = crate::ui::produce_languages::session_tree();
    let (w, h) = crate::ui::produce_snapshot::first_footage_shape(
        &tree,
        &crate::ui::cut_folds::page_recordings(),
    );
    let aspect = produce_page::held_window()
        .map(|_| crate::ui::window_preview::newest_review_cut().aspect)
        .unwrap_or_default();
    crate::produce_screen::thumb_box(&aspect, w.max(1) as u32, h.max(1) as u32)
}

/// The path a save dialog would answer with, if anyone answered it.
///
/// A headless run has no dialog to answer, nor has the thumbnail's Change… door (`produce_slots::next_stood_in`):
/// the convention is a STAND-IN at the seam so the press still runs the real copy. A real `FileChooserNative`
/// response would forward here as `cut_svg::open_svg_chooser` forwards its pick to `svg_chosen`.
pub fn asked_path() -> Option<String> {
    PAGE.with(
        |ProducePage {
             asked_path: held, ..
         }| held.borrow().clone(),
    )
}

/// Record the path a save dialog settled on (`None` clears it so the next press offers its default again).
/// Called from `ui::produce_page::set_setting` when the Container row changes, and by tests.
pub fn answer_path(answer: Option<&str>) {
    PAGE.with(
        |ProducePage {
             asked_path: held, ..
         }| *held.borrow_mut() = answer.map(str::to_string),
    );
}

/// What the save dialog would be OFFERED for ⤓ Save video: `<project>.<container>`.
pub fn offered_save_name() -> String {
    runs::save_default(&project_name(), produce_page::container_of_row())
}

/// …and for ⤓ export: `<project>-thumbnail.jpg`, extension forced, beside the video. It is also the path the
/// press writes when nobody answers the dialog.
pub fn offered_export_name() -> String {
    crate::ui::produce_languages::session_tree()
        .export_jpg()
        .display()
        .to_string()
}

/// The path ⤓ writes: the save dialog's answer (extension forced), else [`offered_export_name`], so the
/// dialog's name and the written file cannot drift. `export_name("final")` is the `<video>` poster, not an upload.
pub fn export_path(asked: Option<&str>) -> String {
    match asked.filter(|typed| !typed.trim().is_empty()) {
        Some(typed) => runs::force_suffix(typed, runs::EXPORT_SUFFIX),
        None => offered_export_name(),
    }
}

/// **8** ↻ beside Title: the title and the description together; images, instruction and picture stay.
pub fn reword(window: &adw::ApplicationWindow) -> String {
    let run = produce_page::press_run();
    let out = crate::produce_presses::press_reword(
        &legs(),
        || with_run(run.as_ref(), crate::ui::produce_languages::reword_half),
        crate::ui::window::log_line,
    );
    land_written(window, out)
}

/// With no run the press has already refused on its missing cut.
fn with_run(
    run: Option<&crate::produce_flow::Run>,
    walk: impl FnOnce(&crate::produce_flow::Run) -> Result<(), String>,
) -> Result<(), String> {
    run.map_or(Ok(()), walk)
}

/// What the press left in `publish.json` onto the page, whichever way it ended (a failed print still wrote the text).
fn land_written(window: &adw::ApplicationWindow, out: crate::produce_presses::Outcome) -> String {
    crate::ui::produce_languages::report(&out);
    produce_page::take_written_publish(window);
    report(window, out.status)
}

/// **10** ⤓ Save video as the button fires it: the copy walks on a worker (§03-shell §1). Destination and
/// weight as [`save_video_from_button`] reads them.
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

/// The project's own name for the save default: the session folder's, else the app's name.
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
                .map(|_| {
                    !crate::ui::window_preview::newest_review_cut()
                        .segs
                        .is_empty()
                })
                .unwrap_or(false)
        },
        // §08 "⤓ Save video and ↻ Transcode both refuse while a render runs": asked of the run bar itself; the
        // page state's flag was set by nothing but tests.
        || {
            produce_page::held_window().is_some_and(|_| {
                crate::ui::window_doors::running_step() == Some(crate::run::Step::Produce)
            })
        },
        produce_page::container_word,
    );
    // This page reports its own reprint passes; the F5.6 words half counts its own elsewhere.
    pressed.count_prints = true;
    // §F5.7: ↻ over the thumbnail clears `own`, in the held state and in `publish.json`, so the picture is the app's again.
    pressed.clear_own = Box::new(|| {
        produce_page::clear_own_thumbnail();
        Ok(())
    });
    pressed
}

/// What a press's legs read off the window, taken before the press walks on its worker (§03-shell §1).
struct Found {
    has_cut: bool,
    rendering: bool,
}

/// Take [`Found`] on the window thread, asking exactly what [`legs`] asks at call time.
fn found() -> Found {
    let has_cut = produce_page::held_window()
        .map(|_| {
            !crate::ui::window_preview::newest_review_cut()
                .segs
                .is_empty()
        })
        .unwrap_or(false);
    let rendering = produce_page::held_window().is_some_and(|_| {
        crate::ui::window_doors::running_step() == Some(crate::run::Step::Produce)
    });
    Found { has_cut, rendering }
}

/// [`legs`] on the worker: the window's answers are those [`found`] took, the page's state is the copy
/// [`crate::ui::produce_worker::Carried`] loaded. A cleared `own` is noted so the landing clears the window's copy too.
fn worker_legs(
    found: Found,
) -> (
    crate::produce_presses::Legs,
    std::rc::Rc<std::cell::Cell<bool>>,
) {
    let Found { has_cut, rendering } = found;
    let mut pressed = crate::ui::produce_languages::legs(
        move || has_cut,
        move || rendering,
        produce_page::container_word,
    );
    pressed.count_prints = true;
    let cleared = std::rc::Rc::new(std::cell::Cell::new(false));
    let noted = std::rc::Rc::clone(&cleared);
    pressed.clear_own = Box::new(move || {
        noted.set(true);
        produce_page::clear_own_thumbnail();
        Ok(())
    });
    (pressed, cleared)
}

/// One page press on its worker (§03-shell §1): the press named `button` greys while `work` walks, and `land`
/// takes the answer on the window thread. Ignored while another press walks.
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
    let run = produce_page::press_run();
    on_worker(
        window,
        "title-suggest",
        move |legs| {
            crate::produce_presses::press_reword(
                legs,
                || with_run(run.as_ref(), crate::ui::produce_languages::reword_half),
                crate::ui::window::log_line,
            )
        },
        |window, out| {
            land_written(window, out);
        },
    );
}

pub fn redraw_from_button(window: &adw::ApplicationWindow) {
    let run = produce_page::press_run();
    on_worker(
        window,
        "thumbnail-redraw",
        move |legs| {
            crate::produce_presses::press_redraw_thumbnail(
                legs,
                // With no run the press has already refused on its missing cut.
                || {
                    run.as_ref().map_or_else(
                        || Err(runs::REDRAW_NO_CUT.to_string()),
                        crate::ui::produce_languages::redraw_half,
                    )
                },
                crate::ui::window::log_line,
            )
        },
        |window, out| {
            land_written(window, out);
        },
    );
}

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

/// Put one press's sentence on the status line AND in the log: the ending is what a person reads now, the log
/// afterwards. The `>>>` opening lines reach the log through the walk's own `log` leg.
fn report(window: &adw::ApplicationWindow, said: String) -> String {
    if let Some(status) = crate::ui::window::find_status(window.upcast_ref()) {
        status.set_text(&said);
    }
    crate::ui::window::log_line(&said);
    said
}
