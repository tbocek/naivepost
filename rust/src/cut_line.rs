//! F2.4 Place and step the line — `spec/05-cut.md` §1 S1, S1b, S2, S3, S4.
//!
//! The screen answers three questions about a press on the tracks: where the red line goes, what it takes into its hand, and whether a row is watched.
//! Every rule is a plain function; the widget layer reports pixels and draws what they return. ▶/⏸ belongs to [`crate::preview`] (F2.1).
//!
//! Spec contradiction: §1 S1 says the scene under the click comes into its hand, but `spec/inventory/cut.md` §A and `spec/05-cut.md` §11.7/§11.16 say the hand
//! closes only on a scene's own picture on the picture band. The later wording wins: see [`click_takes_scene`].

use std::path::{Path, PathBuf};

/// F2.4 S1b: the reach of an edge grab over a clip's own edge (`spec/10-parameters.md` "timeline:" block; 12 px in `spec/inventory/cut.md` §A).
/// The first left press measures this instead of the ordinary reach, so a clip edge beats a card only on the boundary.
pub const EDGE_REACH_PX: f64 = 12.0;

/// F2.4 S1: the picture band's row (`spec/inventory/cut.md` §A); only this row can be watched.
pub const PICTURE_ROW: usize = 0;

/// F2.4 S2/S3: the three things a press can hold, in the order the spec steps them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Held {
    /// A clip's edge, on its own side only.
    Edge,
    /// A whole clip.
    Clip,
    /// An effect card.
    Effect,
}

/// F2.4 S1: what one press on the tracks does, in §1 S1's order (the row is watched before the selection is cleared).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClickOutcome {
    /// The session second the red line goes to.
    pub line_at: f64,
    /// Whether the line moved at all — a gutter press leaves it alone.
    pub line_moved: bool,
    /// The scene taken into the line's hand, if the click landed on its picture.
    pub takes_scene: bool,
    /// Whether the selection is cleared; every track press clears it, a gutter press does not.
    pub clears_selection: bool,
    /// The row that starts being watched, when all of S1's watch rules answer yes.
    pub watches: Option<usize>,
}

/// F2.4 S1: does this press watch the row it landed on? `on_picture` is true only for the picture band; `playing` covers any ▶; `sources` is
/// false before the first source loads; a gutter click answers no.
pub fn watches_row(on_picture: bool, playing: bool, sources: bool, gutter: bool) -> bool {
    on_picture && !playing && sources && !gutter
}

/// F2.4 S1: WHICH row starts being watched: the one the pointer landed on, since a multi-camera session has several.
pub fn watches_index(
    on_picture: bool,
    playing: bool,
    sources: bool,
    gutter: bool,
    row: usize,
) -> Option<usize> {
    watches_row(on_picture, playing, sources, gutter).then_some(row)
}

/// F2.4 S1: everything the widget layer knows about one press, travelling together instead of as six positional booleans.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hit {
    /// The session second the pointer was over, converted at the zoom the band is drawn at.
    pub at: f64,
    /// Which picture row the y fell in; meaningless unless [`Hit::on_picture`].
    pub row: usize,
    /// The y landed on a picture row (a wave strip, a lane, the ruler or the effects lane answers no).
    pub on_picture: bool,
    /// A kept scene's own picture covers `at` — what decides whether the hand closes.
    pub on_scene_picture: bool,
    /// Something is playing: the transport's own flag OR a live media leg.
    pub playing: bool,
    /// The session has footage loaded, so there is a row to watch.
    pub sources: bool,
    /// The x landed in the band's left gutter rather than on the timeline.
    pub gutter: bool,
}

/// F2.4 S1: where the red line goes and what it takes.
///
/// A press on a track puts the line at `at`, clears the selection, and takes the scene only when it landed on that scene's picture
/// ([`click_takes_scene`]). A gutter press does neither.
pub fn click_outcome(
    at: f64,
    on_picture: bool,
    on_scene_picture: bool,
    playing: bool,
    sources: bool,
    gutter: bool,
) -> ClickOutcome {
    click_outcome_in_row(
        at,
        on_picture,
        on_scene_picture,
        playing,
        sources,
        gutter,
        PICTURE_ROW,
    )
}

/// F2.4 S1 on a named row: the same rule, with the clicked row recorded in `watches`.
pub fn click_outcome_in_row(
    at: f64,
    on_picture: bool,
    on_scene_picture: bool,
    playing: bool,
    sources: bool,
    gutter: bool,
    row: usize,
) -> ClickOutcome {
    if gutter {
        return ClickOutcome {
            line_at: at,
            line_moved: false,
            takes_scene: false,
            clears_selection: false,
            watches: None,
        };
    }
    ClickOutcome {
        line_at: at,
        line_moved: true,
        takes_scene: click_takes_scene(on_picture, on_scene_picture),
        clears_selection: true,
        watches: watches_index(on_picture, playing, sources, false, row),
    }
}

/// F2.4 S1: the scene comes into the line's hand only when the click lands on its picture on the picture band (see the module doc).
pub fn click_takes_scene(on_picture: bool, on_scene_picture: bool) -> bool {
    on_picture && on_scene_picture
}

/// F2.4 S1b: is this press one the wider 12 px reach may answer? Only a picture-band press clear of the green and the gutter:
/// the reach exists to take a clip whose green sits on ANOTHER row; a press on the green is S1's click.
pub fn press_tries_reach(on_picture: bool, gutter: bool, on_scene_picture: bool) -> bool {
    on_picture && !gutter && !on_scene_picture
}

/// F2.4 S1b: which of the three things under the pointer a first left press over the picture band answers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PressPick {
    Edge,
    Border,
    Clip,
}

/// F2.4 S1b: edge beats border beats clip. `edge_px` is the distance to the nearer clip edge, `inside` whether the pointer is in the clip's box:
/// within [`EDGE_REACH_PX`] the edge; otherwise inside the box a whole clip; else the line. Only the first press is a grab (F1.3).
pub fn first_press_pick(edge_px: f64, inside: bool) -> PressPick {
    if edge_px <= EDGE_REACH_PX {
        PressPick::Edge
    } else if inside {
        PressPick::Border
    } else {
        PressPick::Clip
    }
}

/// F2.4 S2 + `spec/inventory/cut.md` §D: what a frame step does while something is held: the hold steps, never the line (edge, clip, effect).
/// `None` means nothing was held.
pub fn step_hold(held: Option<Held>) -> Option<&'static str> {
    match held {
        Some(Held::Edge) => Some("edge"),
        Some(Held::Clip) => Some("clip"),
        Some(Held::Effect) => Some("effect"),
        None => None,
    }
}

/// F2.4 S2: frames per press: one for `f` and ← / →, five with `Shift` (or ‹‹ / ››).
pub fn step_frames(shift: bool) -> i64 {
    if shift {
        5
    } else {
        1
    }
}

/// F2.4 S2: the step rate when the file cannot be asked (no footage row, no tool, unparsable answer). No `P.*` row; 25 fps is the prototype's.
/// The real number comes from [`crate::cut_fps::session_fps`]: a frame step is a frame OF THE RECORDING.
pub const FALLBACK_FPS: f64 = 25.0;

/// F2.4 S2: with nothing held a frame step pauses the preview and moves the line by `frames` frames at `fps`. No clamping here:
/// running into an end is F2.1 S5's rule, and a clamp in two places is two rules.
pub fn step_line(line: f64, frames: i64, fps: f64) -> f64 {
    line + frames as f64 / fps
}

/// F2.4 S3: ← and → mean the same as ‹f / f›, but only while the line holds something; otherwise the press belongs to the focused box.
pub fn arrow_moves_line(held: Option<Held>) -> bool {
    held.is_some()
}

/// F2.4 S3: Space toggles the preview ([`crate::preview`] decides which) from any focus but a text box or multi-line entry.
pub fn space_toggles_preview(text_focus: bool) -> bool {
    !text_focus
}

/// F2.4 S4: the contents of `cut/line.json`. Its own file: the cut appearing wakes Narrate and Produce, and the project file is rewritten by a comparing autosave.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LinePos {
    pub t: f64,
}

/// F2.4 S4: `{"t": <session second>}` plus the trailing newline, as the prototype's `lineFile` wrote it.
pub fn line_json(pos: LinePos) -> String {
    format!("{{\"t\":{}}}\n", pos.t)
}

/// F2.4 S4: read `cut/line.json` back. Anything unreadable or without a usable `t` is no remembered line; an integer `t` counts (`as_f64` falls back to `as_i64`).
pub fn parse_line_json(text: &str) -> Option<LinePos> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    let raw = value.get("t")?;
    let t = raw.as_f64().or_else(|| raw.as_i64().map(|n| n as f64))?;
    if !t.is_finite() || t < 0.0 {
        return None;
    }
    Some(LinePos { t })
}

/// F2.4 S4: is the remembered line worth restoring? Only when the recording covers that second (sources may have changed).
pub fn restores_line(t: f64, recording_start: f64, recording_end: f64) -> bool {
    t >= recording_start && t < recording_end
}

/// F2.4 S4: the least time between two writes of the line; no `spec/10-parameters.md` row (the prototype's `lineSaveMs`).
pub const LINE_WRITE_MS: u64 = 1000;

/// F2.4 S4: may this write go out? At most one a second while the line moves (playback moves it ten times a second); the last position lands.
/// `last_write_ms` is the previous write's clock; a closing window always writes.
pub fn may_write_line(last_write_ms: Option<u64>, now_ms: u64, closing: bool) -> bool {
    closing || last_write_ms.is_none_or(|then| now_ms.saturating_sub(then) >= LINE_WRITE_MS)
}

// --- S4: the file itself ---------------------------------------------------------------------------

/// The path S4 writes to, resolved off a project root so callers without a [`crate::layout::Tree`] (the window) can reach it.
pub fn line_file(root: &Path) -> PathBuf {
    root.join("cut/line.json")
}

/// S4: write the line to `<root>/cut/line.json`, creating `cut/` if it is not there.
///
/// Bytes exactly as [`line_json`] formats them. Errors are returned: a line that could not be saved is worth knowing about.
pub fn write_line(root: &Path, pos: LinePos) -> std::io::Result<()> {
    let file = line_file(root);
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&file, line_json(pos))
}

/// S4: read the saved line back. A missing or half-written file is `None`, the normal no-position case.
pub fn read_line(root: &Path) -> Option<LinePos> {
    let text = std::fs::read_to_string(line_file(root)).ok()?;
    parse_line_json(&text)
}

/// S4: the rate limit with its memory: the line moves ten times a second but is saved once a second.
#[derive(Debug, Default, Clone)]
pub struct LineWriter {
    last_write_ms: Option<u64>,
}

impl LineWriter {
    /// One move of the line. Writes only when [`may_write_line`] allows it; returns whether a write happened, so a test sees the limit working.
    pub fn note_move(&mut self, pos: LinePos, root: &Path, now_ms: u64) -> bool {
        if !may_write_line(self.last_write_ms, now_ms, false) {
            return false;
        }
        // A failed write still counts as tried: retrying every tick after ENOSPC is the loop the limit prevents.
        self.last_write_ms = Some(now_ms);
        write_line(root, pos).is_ok()
    }

    /// Closing the window: always writes, whatever the rate limit says.
    pub fn flush(&mut self, pos: LinePos, root: &Path, now_ms: u64) -> bool {
        if !may_write_line(self.last_write_ms, now_ms, true) {
            return false;
        }
        self.last_write_ms = Some(now_ms);
        write_line(root, pos).is_ok()
    }

    pub fn last_write_ms(&self) -> Option<u64> {
        self.last_write_ms
    }

    /// Record a throttled write that happened elsewhere (a caller with no filesystem yet), as `note_move` does to the timestamp.
    pub fn note_throttled(&mut self, now_ms: u64) {
        self.last_write_ms = Some(now_ms);
    }
}

/// S4: the line to restore on opening a project, if any.
///
/// Kept only when some recording still covers it ([`restores_line`]). `recordings` is merged, sorted, disjoint session spans
/// as [`crate::timeline::filmed_runs`] produces.
pub fn restore(root: &Path, recordings: &[(f64, f64)]) -> Option<LinePos> {
    let pos = read_line(root)?;
    recordings
        .iter()
        .any(|(start, end)| restores_line(pos.t, *start, *end))
        .then_some(pos)
}
