//! Asking a file how many audio streams it really holds (§03-shell.md §4's track menu).
//!
//! §4 puts a track menu on a row only "when the file holds ≥ 2 audio streams", and the face of that
//! button is "<on>/<total>" — the total is a fact about the FILE, not about what the session happens
//! to have ticked, so it has to be measured. The answer comes from the real ffprobe, in the shape the
//! project already uses for a tool call: the caller hands over the subprocess leg (`run_ffprobe` in
//! the program, a stub in a test) so no test ever waits on a binary or needs one installed.
//!
//! Nothing here guesses. A probe that fails, answers nothing parseable, or finds no audio stream is an
//! error naming the file and the reason (§0: failure is specific and local); inventing a count would
//! put a menu over audio nobody measured, which is worse than a row with no menu.

use std::path::Path;

use crate::sources::TrackInfo;
use crate::subprocess;

/// The ffprobe arguments: audio streams only, and just the three fields a row needs. `-v error` keeps
/// ffprobe's own chatter out of what gets parsed; `-select_streams a` is why the answer counts audio
/// and not the video stream too.
fn args(path: &Path) -> Vec<String> {
    vec![
        "-v".to_string(),
        "error".to_string(),
        "-select_streams".to_string(),
        "a".to_string(),
        "-show_entries".to_string(),
        "stream=index:stream_tags=title,channels".to_string(),
        "-of".to_string(),
        "json".to_string(),
        path.display().to_string(),
    ]
}

/// The leg that starts the tool: same shape every caller of [`subprocess::run`] uses, named so the
/// signature below reads as one thing.
pub type Tool = dyn Fn(&str, &[&str]) -> Result<String, String>;

/// The program's own leg: run ffprobe through [`subprocess::run`] so the command reaches the log like
/// every other tool call. A page passes this; a test passes its stub.
pub fn run_ffprobe(ffprobe: &str, args: &[&str]) -> Result<String, String> {
    subprocess::run(ffprobe, args, |_| {})
}

/// The file's audio streams, as the tool reported them.
///
/// `ffprobe` is the resolved tool name (the caller takes it from `services::ffprobe_for`, so a Settings
/// folder naming its own ffmpeg gets its neighbour), and `run` is the leg that actually starts it.
///
/// Every object under `"streams"` becomes a [`TrackInfo`]. `index` is stored ONE-BASED: ffprobe numbers
/// streams from 0 inside the container, while §4's popover rows read "Track 1 — …", so the +1 happens
/// here once rather than at every place a track is drawn or ticked. A stream with no title tag gets a
/// placeholder naming its number, because an untitled lane still has to be distinguishable from its
/// neighbour. Absent or unparsable `channels` means mono, which is what a stream with no channel layout
/// plays as.
pub fn audio_streams(ffprobe: &str, path: &Path, run: &Tool) -> Result<Vec<TrackInfo>, String> {
    let asked = args(path);
    let flags: Vec<&str> = asked.iter().map(String::as_str).collect();
    let answer = run(ffprobe, &flags).map_err(|err| format!("{}: {err}", path.display()))?;
    streams_of(&answer, path)
}

/// The same question asked of an answer already in hand, instead of a tool that is started.
///
/// This is the half that parses; [`audio_streams`] is the half that asks. Splitting them keeps the parse
/// testable on the JSON ffprobe really prints without any test starting a binary — the leg a caller
/// swaps for a stub lives one level up, in [`audio_streams`]'s `run` argument.
pub fn streams_of(answer: &str, path: &Path) -> Result<Vec<TrackInfo>, String> {
    let value: serde_json::Value = serde_json::from_str(answer)
        .map_err(|err| format!("{}: ffprobe answered no JSON ({err})", path.display()))?;
    let streams = value
        .get("streams")
        .and_then(|streams| streams.as_array())
        .ok_or_else(|| format!("{}: ffprobe named no streams", path.display()))?;
    let mut tracks = Vec::with_capacity(streams.len());
    for stream in streams {
        // ffprobe's own index, made 1-based for the popover; a stream without one is not a track.
        let Some(index) = stream.get("index").and_then(|index| index.as_i64()) else {
            return Err(format!(
                "{}: ffprobe reported a stream with no index",
                path.display()
            ));
        };
        let tags = stream.get("tags");
        let title = tags
            .and_then(|tags| tags.get("title"))
            .and_then(|title| title.as_str())
            .filter(|title| !title.trim().is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| format!("stream {}", index + 1));
        let channels = stream
            .get("channels")
            .and_then(|channels| channels.as_i64())
            .unwrap_or(1)
            .max(1) as u8;
        tracks.push(TrackInfo {
            index: (index + 1) as u32,
            title,
            channels,
        });
    }
    if tracks.is_empty() {
        return Err(format!("{}: ffprobe found no audio stream", path.display()));
    }
    Ok(tracks)
}
