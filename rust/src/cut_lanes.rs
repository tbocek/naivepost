//! §05-cut#2 — cut lanes built from a source's own metadata. `spec/05-cut.md` §B: "Cut lanes: {name, src,
//! at, off, dur} windows of a file on a row of its own; **built from the source's metadata or probed**; name,
//! name-2 …".
//!
//! The Copy verb already made a lane because a copy carries its own length. A footage file that was never
//! copied carries none, so before this module ⇲ Lane could only place a lane someone had already handed over
//! — the model's lane windows existed only when a test wrote them. This module asks ffprobe, which is what §B
//! says to do, and refuses with the reason when the answer does not come.
//!
//! Split out of `window.rs`, which is past its size budget.

use crate::cut::Lane;

/// §B's naming rule: the base name first, then `-2`, `-3` … past whatever rows are taken. The ladder itself
/// is `cut_copy::lane_name`'s — one answer to "what does a second row of this file get called" — called here
/// with the FILE's own stem instead of the Copy verb's word "Copied".
pub fn lane_name(taken: &[String], src: &str) -> String {
    crate::cut_copy::lane_name(src, taken)
}

/// The window §B describes: `src` is the file, `at` the session second the row starts at, `off` 0 because a
/// whole file lands at its own beginning (the Copy verb passes the FILE second it took from instead), and
/// `dur` the length the probe answered.
pub fn lane_from(source_path: &str, at: f64, duration_seconds: f64, taken: &[String]) -> Lane {
    let stem = std::path::Path::new(source_path)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(source_path)
        .to_string();
    Lane {
        name: lane_name(taken, &stem),
        src: source_path.to_string(),
        at,
        off: 0.0,
        dur: duration_seconds,
    }
}

/// Ask ffprobe how long a file is — §B's "or probed", through the one runner the program already has
/// ([`crate::run_stages::probe_duration`], which logs its command like every other subprocess). `None` plus
/// the reason when there is no answer: no ffprobe, an unreadable file, or output that is not a number.
///
/// Returns `Result` rather than `Option` so the caller can say WHY (§00-principles: failure is specific and
/// local), and the caller decides whether the sentence is a refusal or a log line.
pub fn probe_duration(configured_ffmpeg: &str, path: &str, log: &mut impl FnMut(&str)) -> Result<f64, String> {
    crate::run_stages::probe_duration(configured_ffmpeg, std::path::Path::new(path), log)
}

/// The status line when the length cannot be had: names the file and the reason, per §00-principles'
/// "name the model and the reason where it failed".
pub fn no_probe_status(path: &str, reason: &str) -> String {
    format!("{path}: no length from ffprobe \u{2014} {reason}")
}
