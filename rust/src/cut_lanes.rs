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
pub fn probe_duration(
    configured_ffmpeg: &str,
    path: &str,
    log: &mut impl FnMut(&str),
) -> Result<f64, String> {
    crate::run_stages::probe_duration(configured_ffmpeg, std::path::Path::new(path), log)
}

/// The status line when the length cannot be had: names the file and the reason, per §00-principles'
/// "name the model and the reason where it failed".
pub fn no_probe_status(path: &str, reason: &str) -> String {
    format!("{path}: no length from ffprobe \u{2014} {reason}")
}

/// §05-cut F2.5 badge **2** — the lane's name plate says WHICH CHANNELS that row is.
///
/// Wording taken from the prototype the spec was written from (`go/cut_audio.go`'s `laneName`,
/// spelled again in `go/cut.go`'s Inputs tooltip): a one-channel file says `mono` rather than
/// `L`, which would imply a missing right; a two-channel file drawn on ONE lane says `L=R`,
/// because it is not a mono file but a stereo file carrying one signal in both channels, and a
/// lane calling itself `mono` would look like the page had lost a channel; a two-channel file
/// drawn on TWO lanes says `L` then `R`, in lane order.
pub fn channel_layout(
    chans: u8,
    lanes_on_this_file: usize,
    lane_index_on_file: usize,
) -> &'static str {
    if lanes_on_this_file >= 2 {
        return if lane_index_on_file == 0 { "L" } else { "R" };
    }
    if chans >= 2 {
        return "L=R";
    }
    "mono"
}

/// The plate badge 2 asks for: the lane's own name plus its layout, e.g. the spec picture's
/// `2026-09-16 17-26-20 mono`. No layout answers the bare name (§0: a probe that cannot answer is
/// said on the log, never guessed at).
pub fn plate(name: &str, layout: Option<&str>) -> String {
    match layout {
        Some(layout) => format!("{name} {layout}"),
        None => name.to_string(),
    }
}

/// The ffprobe question for ONE stream's channel count: the first audio stream only, and
/// `nk=1` drops the `channels=` key name so [`parse_channels`] sees a bare `2`.
pub fn channels_args(path: &std::path::Path) -> Vec<String> {
    vec![
        "-v".to_string(),
        "error".to_string(),
        "-select_streams".to_string(),
        "a:0".to_string(),
        "-show_entries".to_string(),
        "stream=channels".to_string(),
        "-of".to_string(),
        "default=nw=1:nk=1".to_string(),
        path.display().to_string(),
    ]
}

/// Parse what ffprobe prints for `stream=channels`: `"2\n"` → 2, `"1"` → 1. Empty output (no
/// audio stream, an unreadable file) or anything whose leading digits are not a positive count
/// answers `None`; the caller says so rather than inventing a shape.
pub fn parse_channels(answer: &str) -> Option<u8> {
    let text = answer.trim();
    if text.is_empty() {
        return None;
    }
    let digits: String = text.chars().take_while(char::is_ascii_digit).collect();
    let chans: u8 = digits.parse().ok()?;
    (chans > 0).then_some(chans)
}

/// Ask the tool. `run` is the leg that starts it — the program passes
/// [`crate::source_probe::run_ffprobe`], a test passes its stub — so this module never dials a
/// binary by itself. Same split as [`crate::cut_fps::probe`].
pub fn probe_channels(
    ffprobe: &str,
    path: &std::path::Path,
    run: &crate::source_probe::Tool,
) -> Option<u8> {
    let asked = channels_args(path);
    let flags: Vec<&str> = asked.iter().map(String::as_str).collect();
    let answer = run(ffprobe, &flags).ok()?;
    parse_channels(&answer)
}

/// One file's channel count, probed once and remembered: the band rebuilds its scene on every
/// refresh (`cut_view::band_scene_for`), so an un-cached plate would cost a subprocess per paint.
/// A miss is cached too, as `cut_fps` caches a failed rate, so a file with no audio is not
/// re-asked on every repaint either.
fn channels_of(lane_src: &str) -> Option<u8> {
    // Resolved the one way a lane's file is resolved anywhere else: `lane_hear::lane_file`
    // hands `layout::resolve` the app root and the session folder as the project. The root is
    // the one the window already works from — `layout::Tree::new(&session_dir)` is how
    // `ui::window` and `open_apply` take theirs on, so the parent of that folder is the root —
    // and with no window (a unit test, a bare probe) there is nothing to resolve against and
    // the stored string is taken as it stands.
    // Resolving matters because a lane's stored src is `project:sources/x.wav`: probed raw,
    // ffprobe is asked for `<cwd>/project:sources/x.wav`, answers nothing, and the plate loses
    // its layout for a file that is sitting in the project.
    let root: Option<std::path::PathBuf> = crate::ui::window::narrate_session_tree()
        .and_then(|tree| tree.dir().parent().map(|root| root.to_path_buf()))
        .or_else(|| std::env::current_dir().ok());
    let path = match root {
        Some(root) => {
            let project = crate::startup::open_dir();
            crate::layout::resolve(&root, &project, lane_src)
        }
        None => std::path::PathBuf::from(lane_src),
    };
    static CACHE: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<std::path::PathBuf, Option<u8>>>,
    > = std::sync::OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(held) = cache.lock().ok().and_then(|slot| slot.get(&path).copied()) {
        return held;
    }
    let configured = crate::settings::from_environment()
        .map(|paths| crate::settings::read(&paths).unwrap_or_default().ffmpeg)
        .unwrap_or_default();
    let tool = crate::services::ffprobe_for(&configured, "ffprobe");
    let found = probe_channels(&tool, &path, &crate::source_probe::run_ffprobe);
    if found.is_none() {
        crate::ui::window::log_line(&format!(
            "{tool} said nothing about {}: the lane's plate shows its name only",
            path.display()
        ));
    }
    if let Ok(mut slot) = cache.lock() {
        slot.insert(path.to_path_buf(), found);
    }
    found
}

/// The plates for every cut lane, in `Scene.lanes`' order: each lane's name with the layout of
/// the file it windows, counted against how many lanes share that file (two lanes of a stereo
/// file are L and R; one lane of it is L=R). This is what the band paints, so the rule lives here
/// and the painter only draws.
pub fn plates(lanes: &[Lane]) -> Vec<String> {
    let mut per_file: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for lane in lanes {
        *per_file.entry(lane.src.as_str()).or_insert(0) += 1;
    }
    let mut seen: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    lanes
        .iter()
        .map(|lane| {
            let index = seen.entry(lane.src.as_str()).or_insert(0);
            let on_file = *index;
            *index += 1;
            let layout = channel_layout(
                channels_of(&lane.src).unwrap_or(1),
                per_file.get(lane.src.as_str()).copied().unwrap_or(1),
                on_file,
            );
            plate(&lane.name, Some(layout))
        })
        .collect()
}
