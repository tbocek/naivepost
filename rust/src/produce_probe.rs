//! §F5.2 S5's size question: what shape an ENCODED clip actually came out at.
//!
//! The join is a stream copy and cannot mix sizes ([`crate::produce_render::join_mismatch`] is the sentence).
//! The only honest way to know a clip's size is to ask the file: ffprobe, not byte counts, which differ at
//! the same 1920×1080 every time.
//!
//! The real call sits behind [`probe`], and a test re-points it at a fake script through [`ffprobe_program`]'s
//! environment seam (as `play_back::player_program`'s `NAIVEPOST_PLAYER`). A session with no ffprobe gets
//! `None` and the render says nothing about sizes; the missing binary is `checks.rs`'s diagnosis.

use std::path::Path;

/// ffprobe's argv for one file's video-stream size: `-select_streams v:0` (audio has no size), `csv=p=0` (one line of two numbers).
pub fn args_for(file: &Path) -> Vec<String> {
    vec![
        "-v".into(),
        "error".into(),
        "-select_streams".into(),
        "v:0".into(),
        "-show_entries".into(),
        "stream=width,height".into(),
        "-of".into(),
        "csv=p=0".into(),
        file.display().to_string(),
    ]
}

/// `"1920,1080"` → `Some((1920, 1080))`. Anything else (no video stream, truncated line, non-number) is `None`.
pub fn parse_sizes(text: &str) -> Option<(i32, i32)> {
    let line = text.trim().lines().next()?;
    let (width, height) = line.trim().split_once(',')?;
    let (w, h) = (
        width.trim().parse::<i32>().ok()?,
        height.trim().parse::<i32>().ok()?,
    );
    // A zero edge is not a frame: a stub or truncated file would blame every other clip for mismatching it.
    (w > 0 && h > 0).then_some((w, h))
}

/// Where ffprobe runs from, in the order this app reads every program name: `NAIVEPOST_FFPROBE` when set and
/// non-empty, else ffprobe beside the configured ffmpeg (§1), read per call so a re-typed box takes effect at once.
///
/// The environment line is also how a test points the leg at a fake script, the SAME door the program reads.
fn ffprobe_program() -> String {
    let pointed = std::env::var("NAIVEPOST_FFPROBE").unwrap_or_default();
    if !pointed.trim().is_empty() {
        return pointed.trim().to_string();
    }
    let configured = crate::settings::from_environment()
        .map(|paths| crate::settings::read(&paths).unwrap_or_default().ffmpeg)
        .unwrap_or_default();
    crate::services::ffprobe_for(&configured, "ffprobe")
}

/// The real leg: run ffprobe over one file and read its size back; any failure is `None`.
///
/// Nothing is logged: §S5's check runs over EVERY clip and decides whether ONE sentence is said, so a command
/// line per clip would drown the mismatch. Absence is the answer, not an error to report.
pub fn size_of(file: &Path) -> Option<(i32, i32)> {
    let args = args_for(file);
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let reply = crate::subprocess::run(&ffprobe_program(), &refs, |_| ()).ok()?;
    parse_sizes(&reply)
}

/// How long a file runs as the join counts it: its container duration. `None` when ffprobe cannot say.
pub fn seconds_of(file: &Path) -> Option<f64> {
    let args = [
        "-v",
        "error",
        "-show_entries",
        "format=duration",
        "-of",
        "csv=p=0",
    ];
    let name = file.display().to_string();
    let refs: Vec<&str> = args.iter().copied().chain([name.as_str()]).collect();
    let reply = crate::subprocess::run(&ffprobe_program(), &refs, |_| ()).ok()?;
    reply.trim().parse::<f64>().ok().filter(|s| *s > 0.0)
}

/// Does this file carry a sound stream? One ffprobe cannot answer for counts as having one: a clip that then
/// fails says why, where a silently dropped sound would not.
pub fn has_sound(file: &Path) -> bool {
    std::process::Command::new(ffprobe_program())
        .args([
            "-v",
            "error",
            "-select_streams",
            "a:0",
            "-show_entries",
            "stream=index",
            "-of",
            "csv=p=0",
        ])
        .arg(file)
        .output()
        .map_or(true, |out| !out.status.success() || !out.stdout.is_empty())
}

/// The first video stream's frame rate (`r_frame_rate`, "30000/1001" read as 29.97), for "source" on a frame grid.
pub fn frame_rate_of(file: &Path) -> Option<f64> {
    let name = file.display().to_string();
    let args = ["-v", "error", "-select_streams", "v:0", "-show_entries", "stream=r_frame_rate", "-of", "csv=p=0", name.as_str()];
    let reply = crate::subprocess::run(&ffprobe_program(), &args, |_| ()).ok()?;
    parse_rate(reply.trim())
}

/// "60/1" → 60, "30000/1001" → 29.97…, "25" → 25; nothing for 0 or a malformed answer.
pub fn parse_rate(text: &str) -> Option<f64> {
    let rate = match text.split_once('/') {
        Some((n, d)) => n.trim().parse::<f64>().ok()? / d.trim().parse::<f64>().ok().filter(|d| *d > 0.0)?,
        None => text.parse::<f64>().ok()?,
    };
    (rate > 0.0 && rate.is_finite()).then_some(rate)
}
