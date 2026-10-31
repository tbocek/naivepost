//! Asking a file how many audio streams it really holds (§03-shell.md §4's track menu).

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
pub fn audio_streams(ffprobe: &str, path: &Path, run: &Tool) -> Result<Vec<TrackInfo>, String> {
    let asked = args(path);
    let flags: Vec<&str> = asked.iter().map(String::as_str).collect();
    let answer = run(ffprobe, &flags).map_err(|err| format!("{}: {err}", path.display()))?;
    streams_of(&answer, path)
}

/// The same question asked of an answer already in hand, instead of a tool that is started.
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
