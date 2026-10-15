//! §F5.2 S5's size question: what shape an ENCODED clip actually came out at.
//!
//! The join is a stream copy and cannot mix sizes, so a clip that left the encoder at another frame size
//! breaks the video at exactly that clip ([`crate::produce_render::join_mismatch`] is the sentence). The
//! only honest way to know a clip's size after the fact is to ask the file: ffprobe reads it back off the
//! container, the same tool `checks.rs` diagnoses beside ffmpeg. Byte counts cannot answer this — two clips
//! at the same 1920×1080 differ in bytes every time, and a 1280×720 clip can match either by chance.
//!
//! Like every other leg here, the real call sits behind a door the program itself uses
//! ([`probe`]), and a test re-points that door at a fake script through [`ffprobe_program`]'s environment
//! seam — the same shape `play_back::player_program`'s `NAIVEPOST_PLAYER` and `speak_leg`'s
//! `NAIVEPOST_TTS_URL` use. No for-test helper is ever asked for an answer.
//! A session with no ffprobe gets `None` from every probe and the render says nothing about sizes: absence
//! is not a claim that the clips matched, and the missing binary is `checks.rs`'s diagnosis, not this one.

use std::path::Path;

/// The frame size of one encoded file, read off the file by ffprobe.
///
/// This is the door `produce_exec::Materials::probe` carries, so the walk never names ffprobe itself, and it
/// is only ever the real leg: a scripted answer would mean the size question is answered by the test that
/// asks it. Every failure — no binary, a non-zero exit, an unparseable reply — answers `None`, which the
/// walk reads as "no claim about this clip", never as a size.
pub fn probe(file: &Path) -> Option<(i32, i32)> {
    size_of(file)
}

/// ffprobe's argv for one file's video-stream size. `-select_streams v:0` because the audio stream has no
/// size to report, and `csv=p=0` because it is the one output shape that is one line of two numbers.
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

/// `"1920,1080"` → `Some((1920, 1080))`. Anything else — an empty reply from a file with no video stream,
/// a truncated line, a non-number — answers `None`, which the caller reads as "no claim", never as a size.
pub fn parse_sizes(text: &str) -> Option<(i32, i32)> {
    let line = text.trim().lines().next()?;
    let (width, height) = line.trim().split_once(',')?;
    let (w, h) = (width.trim().parse::<i32>().ok()?, height.trim().parse::<i32>().ok()?);
    // A zero edge is not a frame: a stub or truncated file decodes to something with no size, and calling
    // that a size would blame every other clip for mismatching it.
    (w > 0 && h > 0).then_some((w, h))
}

/// Where ffprobe runs from, in the order this app reads every program name: `NAIVEPOST_FFPROBE` when set
/// and non-empty, else ffprobe beside the configured ffmpeg (§1: "ffprobe from the same folder"), read per
/// call like every other settings value so a re-typed box takes effect at once.
///
/// The environment line is also how a test points the leg at a fake script: it is the SAME door the program
/// reads, which is what lets a check measure the real argv-and-parse path instead of a slot only tests fill.
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

/// The real leg: run ffprobe over one file and read its size back. Any failure — no binary, a non-zero
/// exit, an unparseable reply — is `None`: this reports sizes, it does not diagnose installations.
///
/// Nothing is logged here, which is why [`probe`] can be the thin door it is: §S5's check runs over EVERY
/// clip after the encodes and its answer decides whether ONE sentence is said, so a command line per clip
/// would put more noise on the screen than the mismatch it looks for. Unlike every other tool step there is
/// nothing to blame when one probe fails — absence is the answer, not an error to report.
pub fn size_of(file: &Path) -> Option<(i32, i32)> {
    let args = args_for(file);
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let reply = crate::subprocess::run(&ffprobe_program(), &refs, |_| ()).ok()?;
    parse_sizes(&reply)
}
