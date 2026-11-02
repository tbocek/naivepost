//! §05-cut#2 §B — ⇲ Lane with **nothing in hand**: put a footage FILE on a row of its own.

use crate::cut_copy;
use crate::ui::window;
use crate::ui::window_page_state::{WindowPage, PAGE as WINDOW_PAGE};

/// ⇲ Lane with nothing copied: the row starts at the red line, windows the session's footage file, and runs
/// for what ffprobe says that file is.
pub fn lane_for_footage_file(window: &adw::ApplicationWindow) -> String {
    let Some(at) = paste_line_opt() else {
        return cut_copy::NO_LANE_LINE_YET.to_string();
    };
    let Some(file) = footage_source_path() else {
        return "no source is marked as footage \u{2014} Inputs says which files are tape"
            .to_string();
    };
    let probed = probe_file_seconds(&file);
    file_lane(window, &file, at, probed)
}

/// [`lane_for_footage_file`] with the probe on a worker (§03-shell §1): the same refusals at once, then the
/// row filed, and its sentence on the status line, when ffprobe has answered.
pub fn lane_for_footage_file_on_worker(window: &adw::ApplicationWindow) -> String {
    let Some(at) = paste_line_opt() else {
        return cut_copy::NO_LANE_LINE_YET.to_string();
    };
    let Some(file) = footage_source_path() else {
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
        source_base(&footage_source_path().unwrap_or_default())
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

/// Is a line placed at all?
fn line_is_placed() -> bool {
    WINDOW_PAGE.with(
        |WindowPage {
             window_insert_place_known: cell,
             ..
         }| *cell.borrow(),
    ) == Some(true)
}

/// The red line's second when there is one — the row's start — or `None` when the page has no line yet.
pub(crate) fn paste_line_opt() -> Option<f64> {
    line_is_placed().then(|| paste_line())
}

/// The line the copy lands on — where the eye is looking, which is what "paste at the red line" asks for.
pub fn paste_line() -> f64 {
    window::live_player()
        .filter(|player| player.borrow().transport.playing)
        .and_then(|player| player.borrow().playhead)
        .unwrap_or_else(|| window::line_position().t)
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
pub(crate) fn footage_source_path() -> Option<String> {
    window::session_handle()
        .borrow()
        .sources
        .iter()
        .find(|source| source.footage)
        .map(|source| source.path.clone())
}

/// Ask ffprobe for this file's length, through the configured ffmpeg's own ffprobe (§1's "ffprobe from the
/// same folder").
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
