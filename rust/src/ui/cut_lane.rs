//! §05-cut#2 §B — ⇲ Lane with **nothing in hand**: put a footage FILE on a row of its own.
//!
//! §B spells a lane as "a window of a file on a row of its own; built from the source's metadata or
//! probed". With a copy in hand the seconds come along with it (that half stays in [`crate::cut_copy`] and
//! `window::press_lane`); with nothing copied there is no hand to read, so the page asks ffprobe how long
//! the session's footage file is and files that as the row. A probe that cannot answer is refused by name
//! rather than filed as an empty row (§00-principles: failure is specific and local).
//!
//! Split out of `window.rs`, which is past its size budget; every rule about WHICH second and WHICH file
//! stays with the doors that hold them (`paste_line_opt`, `footage_file`, `newest_review_cut`).

use crate::cut_copy;
use crate::ui::window;

/// ⇲ Lane with nothing copied: the row starts at the red line, windows the session's footage file, and runs
/// for what ffprobe says that file is. Returns the sentence for the status line either way.
pub fn lane_for_footage_file(window: &adw::ApplicationWindow) -> String {
    let Some(at) = paste_line_opt(window) else {
        return cut_copy::NO_LANE_LINE_YET.to_string();
    };
    let Some(file) = footage_source_path(window) else {
        return "no source is marked as footage \u{2014} Inputs says which files are tape"
            .to_string();
    };
    let probed = probe_file_seconds(&file);
    file_lane(window, &file, at, probed)
}

/// [`lane_for_footage_file`] with the probe on a worker (§03-shell §1): the same refusals at once, then the
/// row filed, and its sentence on the status line, when ffprobe has answered.
pub fn lane_for_footage_file_on_worker(window: &adw::ApplicationWindow) -> String {
    let Some(at) = paste_line_opt(window) else {
        return cut_copy::NO_LANE_LINE_YET.to_string();
    };
    let Some(file) = footage_source_path(window) else {
        return "no source is marked as footage \u{2014} Inputs says which files are tape"
            .to_string();
    };
    let held = window.clone();
    let asked = file.clone();
    crate::ui::background::run(
        "lane-probe",
        move || probe_file_seconds(&asked),
        move |probed| {
            let said = file_lane(&held, &file, at, probed);
            if let Some(status) = window::find_status(gtk4::prelude::Cast::upcast_ref(&held)) {
                status.set_text(&said);
            }
        },
    );
    format!(
        "measuring {} \u{2026}",
        source_base(&footage_source_path(window).unwrap_or_default())
    )
}

/// File the footage file's row once its length is known: the half after the probe.
fn file_lane(
    window: &adw::ApplicationWindow,
    file: &str,
    at: f64,
    probed: Result<f64, String>,
) -> String {
    let seconds = match probed {
        Ok(seconds) => seconds,
        Err(said) => {
            window::log_line(&said);
            return said;
        }
    };
    let mut cut_ = window::newest_review_cut();
    let taken: Vec<String> = cut_.lanes.iter().map(|lane| lane.name.clone()).collect();
    let lane = crate::cut_lanes::lane_from(file, at, seconds, &taken);
    let name = lane.name.clone();
    cut_.lanes.push(lane);
    // §7's one writer: ⇲ Lane is an act of the hand that ends here, so the line belongs on disk before the
    // page is repainted from it.
    window::save_insert_cut(&cut_);
    window::record_edit(window, &cut_);
    cut_copy::lane_status(seconds, 0.0, &name, at)
}

/// Is a line placed at all? `None` (never placed) and `Some(false)` both mean there is nothing to start a
/// row at — the page has exactly one notion of this, set by `note_place` / `set_line_position`, and every
/// sibling module reads that same cell (`cut_svg`, `cut_speed_hand`, …).
fn line_is_placed() -> bool {
    window::INSERT_PLACE_KNOWN.with(|cell| *cell.borrow()) == Some(true)
}

/// The red line's second when there is one — the row's start — or `None` when the page has no line yet.
fn paste_line_opt(window: &adw::ApplicationWindow) -> Option<f64> {
    line_is_placed().then(|| window::paste_line(window))
}

/// The line the copy lands on — where the eye is looking, which is what "paste at the red line" asks for.
///
/// A transport that is actually PLAYING answers with its own playhead: while the preview runs, the picture on
/// screen is at that second and the click that placed the stored line is already behind it. A player that
/// exists but is NOT playing does not answer at all — its `playhead` is whatever a past seek or pause left
/// there, and a paste aimed by that stale number would land somewhere the person is not looking (the earlier
/// version assumed "both are the same number while nothing plays", which a parked preview refutes: pausing
/// leaves the playhead where the pause happened and the line elsewhere). So a parked player falls through to
/// the stored line, which every press on the band and the strip writes through `place_line_press` →
/// `move_line_and_save`.
pub fn paste_line(window: &adw::ApplicationWindow) -> f64 {
    window::live_player(window)
        .filter(|player| player.borrow().transport.playing)
        .and_then(|player| player.borrow().playhead)
        .unwrap_or_else(|| window::line_position(window).t)
}

/// The base name of a project path — what a `Scope::Sound { recording }` names.
pub(crate) fn source_base(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(path)
        .to_string()
}

/// The session's first footage-marked file, by path: the file ⇲ Lane can put on a row of its own.
pub(crate) fn footage_source_path(window: &adw::ApplicationWindow) -> Option<String> {
    window::session_handle(window)
        .borrow()
        .sources
        .iter()
        .find(|source| source.footage)
        .map(|source| source.path.clone())
}

/// Ask ffprobe for this file's length, through the configured ffmpeg's own ffprobe (§1's "ffprobe from the
/// same folder"). `Err` carries the sentence to print: named file, named tool, stated reason.
fn probe_file_seconds(file: &str) -> Result<f64, String> {
    // §1's "ffprobe from the same folder": the configured ffmpeg decides where ffprobe lives; `""` means
    // this machine has no settings home, which the probe reports as its own refusal reason.
    let configured = crate::settings::from_environment()
        .map(|paths| crate::settings::read(&paths).unwrap_or_default().ffmpeg)
        .unwrap_or_default();
    match crate::cut_lanes::probe_duration(&configured, file, &mut |line| window::log_line(line)) {
        Ok(seconds) if seconds > 0.0 => Ok(seconds),
        Ok(_) => Err(crate::cut_lanes::no_probe_status(
            file,
            "ffprobe answered 0 seconds",
        )),
        Err(why) => Err(crate::cut_lanes::no_probe_status(file, &why)),
    }
}
