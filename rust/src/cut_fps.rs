//! F2.4 S2: the frame rate of the recording under the line, asked of the file (ffprobe via an injected subprocess leg).
//! With nothing to ask the step uses [`crate::cut_line::FALLBACK_FPS`] and the reason is logged (§0).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use crate::source_probe::Tool;

/// The probe's arguments: the stream's own rate. `r_frame_rate` not `avg_frame_rate`, so a variable-rate file steps the same each press.
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

/// Parse what ffprobe prints (`"30000/1001"`, `"25"`); empty, zero denominator (`"0/0"`) or unparsable is `None`.
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
    (rate.is_finite() && rate > 0.0).then_some(rate)
}

/// Ask the tool via `run` ([`crate::source_probe::run_ffprobe`] in the program, a stub in a test).
pub fn probe(ffprobe: &str, path: &Path, run: &Tool) -> Option<f64> {
    let asked = fps_args(path);
    let flags: Vec<&str> = asked.iter().map(String::as_str).collect();
    let answer = run(ffprobe, &flags).ok()?;
    parse_rate(&answer)
}

/// Which ffprobe to ask: next to the configured ffmpeg (§1), as [`crate::ui::cut_lane`] does.
fn ffprobe_program() -> String {
    let configured = crate::settings::from_environment()
        .map(|paths| crate::settings::read(&paths).unwrap_or_default().ffmpeg)
        .unwrap_or_default();
    crate::services::ffprobe_for(&configured, "ffprobe")
}

/// One probe per file per session, failures cached too as the fallback rate.
fn cache() -> &'static Mutex<HashMap<PathBuf, f64>> {
    static SLOT: OnceLock<Mutex<HashMap<PathBuf, f64>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The rate to step at for this window's session: the footage file's own, else [`crate::cut_line::FALLBACK_FPS`] with a logged reason.
pub fn session_fps() -> f64 {
    let Some(file) = footage_file() else {
        crate::ui::window::log_line(&format!(
            "no footage file in this session: stepping at {:.0} fps",
            crate::cut_line::FALLBACK_FPS
        ));
        return crate::cut_line::FALLBACK_FPS;
    };
    ask(&file)
}

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

/// The footage row's file, resolved with `layout::resolve`, if it exists.
/// DECISION where the spec is silent: with several cameras the footage-marked row is what ▶ plays ([`crate::ui::cut_record_play`]) and its rate is the session's.
fn footage_file() -> Option<PathBuf> {
    let written = crate::ui::cut_lane::footage_source_path()?;
    let root = std::env::current_dir().unwrap_or_default();
    let project_dir = crate::startup::open_dir();
    let path = crate::layout::resolve(&root, &project_dir, &written);
    path.is_file().then_some(path)
}
