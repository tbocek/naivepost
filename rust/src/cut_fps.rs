//! F2.4 S2 — the frame rate of the recording under the line, asked of the file.
//!
//! A frame step is a frame OF THE RECORDING (§1's "step the recording under the line"), so its width is a
//! fact about that file, not a constant the app carries: 30000/1001 footage steps by 33.37 ms and 25 fps
//! footage by 40 ms. The answer comes from the real ffprobe, in the shape this project already uses for a
//! tool call — the caller hands over the subprocess leg (`source_probe::run_ffprobe` in the program, a stub
//! in a test), so no test waits on a binary or needs one installed.
//!
//! When there is nothing to ask (no footage row, no file, no tool, an answer that parses to nothing) the
//! step still happens at [`crate::cut_line::FALLBACK_FPS`] and the reason is logged: a step that moved by a
//! slightly wrong amount beats a key that does nothing, but the log says which it was (§0: failure is
//! specific and local).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use crate::source_probe::Tool;

/// The probe's arguments: the video stream's own rate, printed bare. `r_frame_rate` rather than
/// `avg_frame_rate` because a step is one frame of the track as recorded, and a variable-frame file's
/// average would make two presses of the same key move by different amounts.
pub fn fps_args(path: &Path) -> Vec<String> {
    vec![
        "-v".to_string(),
        "error".to_string(),
        "-select_streams".to_string(),
        "v:0".to_string(),
        "-show_entries".to_string(),
        "stream=r_frame_rate".to_string(),
        "-of".to_string(),
        "default=nw=1:nk=1".to_string(),
        path.display().to_string(),
    ]
}

/// Parse what ffprobe prints: `"30000/1001"` → 29.970030…, `"25"` or `"25.000000"` → 25.0. An empty
/// answer, a zero denominator (`"0/0"` on a file with no video stream) or anything unparsable is `None`.
pub fn parse_rate(answer: &str) -> Option<f64> {
    let text = answer.trim();
    if text.is_empty() {
        return None;
    }
    let rate = match text.split_once('/') {
        Some((num, den)) => {
            let num: f64 = num.trim().parse().ok()?;
            let den: f64 = den.trim().parse().ok()?;
            if den == 0.0 {
                return None;
            }
            num / den
        }
        None => text.parse::<f64>().ok()?,
    };
    // A rate of 0, a negative one or NaN is not a frame rate; ffprobe prints 0/0 for a stream with none.
    (rate.is_finite() && rate > 0.0).then_some(rate)
}

/// Ask the tool. `run` is the leg that starts it — the program passes [`crate::source_probe::run_ffprobe`],
/// a test passes its stub — so this module never dials a binary by itself.
pub fn probe(ffprobe: &str, path: &Path, run: &Tool) -> Option<f64> {
    let asked = fps_args(path);
    let flags: Vec<&str> = asked.iter().map(String::as_str).collect();
    let answer = run(ffprobe, &flags).ok()?;
    parse_rate(&answer)
}

/// Which ffprobe to ask: Settings' configured ffmpeg decides where it lives (§1's "ffprobe from the same
/// folder"), exactly as [`crate::ui::cut_lane`]'s duration probe decides it.
fn ffprobe_program() -> String {
    let configured = crate::settings::from_environment()
        .map(|paths| crate::settings::read(&paths).unwrap_or_default().ffmpeg)
        .unwrap_or_default();
    crate::services::ffprobe_for(&configured, "ffprobe")
}

/// One probe per file per session: a step costs a subprocess otherwise, and a person stepping through a
/// minute presses the key six hundred times. A failed probe is cached too, as the fallback rate, so a file
/// with no video stream does not get re-asked on every press either.
fn cache() -> &'static Mutex<HashMap<PathBuf, f64>> {
    static SLOT: OnceLock<Mutex<HashMap<PathBuf, f64>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The frame rate to step at for THIS window's session: the footage file's own rate when it can be asked,
/// [`crate::cut_line::FALLBACK_FPS`] with a logged reason when it cannot.
pub fn session_fps(window: &adw::ApplicationWindow) -> f64 {
    let Some(file) = footage_file(window) else {
        crate::ui::window::log_line(&format!(
            "no footage file in this session: stepping at {:.0} fps",
            crate::cut_line::FALLBACK_FPS
        ));
        return crate::cut_line::FALLBACK_FPS;
    };
    ask(&file)
}

/// One file's rate, probed once and remembered. Split out so `session_fps` stays about WHICH file and this
/// stays about asking for it.
fn ask(file: &Path) -> f64 {
    if let Some(held) = cache().lock().ok().and_then(|slot| slot.get(file).copied()) {
        return held;
    }
    let tool = ffprobe_program();
    let found = probe(&tool, file, &crate::source_probe::run_ffprobe);
    let rate = match found {
        Some(rate) => rate,
        None => {
            crate::ui::window::log_line(&format!(
                "{tool} said nothing about {}: stepping at {:.0} fps",
                file.display(),
                crate::cut_line::FALLBACK_FPS
            ));
            crate::cut_line::FALLBACK_FPS
        }
    };
    if let Ok(mut slot) = cache().lock() {
        slot.insert(file.to_path_buf(), rate);
    }
    rate
}

/// The file whose rate the line steps at: the session's footage row, resolved the one way every flow
/// resolves a path (`layout::resolve`, the session folder as the project), and only if it is really there.
///
/// DECISION where the spec is silent: §1 says "the recording under the line" without saying which file to
/// ask when a session has several cameras. The footage-marked row is what Prepare filmed and what ▶ plays
/// (see [`crate::ui::cut_record_play`]), so its rate is the session's; a multi-camera session records at one
/// frame rate anyway, and asking each row per key press would cost a probe per press for the same number.
fn footage_file(window: &adw::ApplicationWindow) -> Option<PathBuf> {
    let written = crate::ui::cut_lane::footage_source_path(window)?;
    let root = std::env::current_dir().unwrap_or_default();
    let project_dir = crate::startup::session_dir(&root);
    let path = crate::layout::resolve(&root, &project_dir, &written);
    path.is_file().then_some(path)
}
