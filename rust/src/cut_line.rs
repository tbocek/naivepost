//! F2.4 Place and step the line — `spec/05-cut.md` §1 S1, S1b, S2, S3, S4.
//!
//! The screen answers three questions about a press on the tracks: where the red
//! line goes, what it takes into its hand, and whether a row starts being
//! watched. Every rule is a plain function here; the widget layer reports pixels,
//! modifiers and holds and draws what these return. ▶/⏸ itself belongs to
//! [`crate::preview`] (F2.1) — this module only says when a press means it.
//!
//! One spec contradiction is recorded rather than hidden: §1 S1 promises "the
//! scene under the click comes into its hand", while `spec/inventory/cut.md` §A
//! and `spec/05-cut.md` §11.7 ("Clicking the screen does not pick the clip up — it
//! moves the red line") say a plain click takes the scene **only** when the click
//! lands on that scene's own picture on the picture band, and §11.16 says a click
//! on empty timeline space clears the selection and takes nothing. The inventory
//! plus §11 is the finer, later wording, so [`click_takes_scene`] answers with
//! that: the line moves and the row is watched wherever §1 allows it, but the hand
//! closes only on a scene's picture.

use std::path::{Path, PathBuf};

/// F2.4 S1b: the reach of an edge grab over a clip's own edge — `spec/10-parameters.md`'s "timeline:"
/// block, "the reach of an edge grab over its own edge, px", cited as 12 px by
/// `spec/inventory/cut.md` §A. The first press of the left button measures this
/// distance instead of the ordinary reach, so a clip edge wins over a card only when
/// the pointer really is on the boundary.
pub const EDGE_REACH_PX: f64 = 12.0;

/// F2.4 S1: the row of the picture band — `spec/inventory/cut.md` §A, "Row order is
/// picture, waveform per source, effects". Only this row can watch a row, so the
/// geometry question has a number to test against.
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

/// F2.4 S1: what one press on the tracks does, in the order §1 S1 lists the effects —
/// the row is watched before the selection is cleared, because watching answers with
/// a press and a selection answers with an outcome.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClickOutcome {
    /// The session second the red line goes to.
    pub line_at: f64,
    /// Whether the line moved at all — a gutter press leaves it alone.
    pub line_moved: bool,
    /// The scene taken into the line's hand, if the click landed on its picture.
    pub takes_scene: bool,
    /// Whether the selection is cleared. Every press on a track clears it; a gutter
    /// press does not touch it.
    pub clears_selection: bool,
    /// The row that starts being watched, when all of S1's watch rules answer yes.
    pub watches: Option<usize>,
}

/// F2.4 S1: does this press watch the row it landed on? `on_picture` is true only for
/// the picture band (an audio band or the effects row answers no); `playing` covers
/// any ▶ running, preview or recording; `sources` is false before the first source
/// has loaded, when there is no row to watch; a gutter click answers no whatever else
/// is true — its whole job is the row's height.
pub fn watches_row(on_picture: bool, playing: bool, sources: bool, gutter: bool) -> bool {
    on_picture && !playing && sources && !gutter
}

/// F2.4 S1: where the red line goes and what it takes.
///
/// A press on a track puts the line at `at`, clears the selection, and takes the
/// scene under the click only when it landed on that scene's picture on the picture
/// band ([`click_takes_scene`]) — hence `on_scene_picture`, which says whether the
/// pointer was on a scene's own picture rather than on the row behind it. A press in
/// the gutter does neither: it moves no line and touches no selection.
pub fn click_outcome(at: f64, on_picture: bool, on_scene_picture: bool, playing: bool, sources: bool, gutter: bool) -> ClickOutcome {
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
        watches: watches_row(on_picture, playing, sources, false).then_some(PICTURE_ROW),
    }
}

/// F2.4 S1: the scene comes into the line's hand only when the click lands on that
/// scene's picture on the picture band. A row background, an audio band or the effects
/// row clears the selection and takes nothing — see the module doc for why this reads
/// §1 S1 through `spec/inventory/cut.md` §A and `spec/05-cut.md` §11.7/§11.16.
pub fn click_takes_scene(on_picture: bool, on_scene_picture: bool) -> bool {
    on_picture && on_scene_picture
}

/// F2.4 S1b: which of the three things under the pointer a left button's first press
/// over the picture band answers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PressPick {
    Edge,
    Border,
    Clip,
}

/// F2.4 S1b: edge beats border beats clip. `edge_px` is the pointer's distance to the
/// nearer clip edge and `inside` whether it is inside the clip's box at all, both for
/// the one clip under the pointer: within [`EDGE_REACH_PX`] the edge is picked up;
/// otherwise inside the box its border — a whole clip; only outside it does the press
/// fall through to the line. Only the first press is a grab (F1.3), so this never runs
/// on a second one.
pub fn first_press_pick(edge_px: f64, inside: bool) -> PressPick {
    if edge_px <= EDGE_REACH_PX {
        PressPick::Edge
    } else if inside {
        PressPick::Border
    } else {
        PressPick::Clip
    }
}

/// F2.4 S2 + `spec/inventory/cut.md` §D: what a frame step does while something is
/// held — the hold steps, never the line: the edge first, then the clip, then the
/// effect. `None` means nothing was held, and the step belongs to the line (S2's seek)
/// or does not happen at all (S3's arrows).
pub fn step_hold(held: Option<Held>) -> Option<&'static str> {
    match held {
        Some(Held::Edge) => Some("edge"),
        Some(Held::Clip) => Some("clip"),
        Some(Held::Effect) => Some("effect"),
        None => None,
    }
}

/// F2.4 S2: how many frames one press steps. `f` and ← / → are one frame of the
/// recording under the line; `Shift` (or ‹‹ / ››) is five.
pub fn step_frames(shift: bool) -> i64 {
    if shift {
        5
    } else {
        1
    }
}

/// F2.4 S2: with nothing held a frame step pauses the preview and moves the line by
/// `frames` frames of the recording under it, at `fps` frames a second — the project's
/// own frame interval, never a model's guess. No clamping here: running into an end is
/// F2.1 S5's rule (the line walks on), and a clamp in two places is two rules.
pub fn step_line(line: f64, frames: i64, fps: f64) -> f64 {
    line + frames as f64 / fps
}

/// F2.4 S3: ← and → mean the same as ‹f / f›, but **only** while the line already has
/// something in its hand; with nothing held they do nothing and the press belongs to
/// whichever box has the focus.
pub fn arrow_moves_line(held: Option<Held>) -> bool {
    held.is_some()
}

/// F2.4 S3: Space toggles the preview — recording or cut, whichever is on the screen,
/// which is [`crate::preview`] to decide — from any focus but a text box or a
/// multi-line entry, where Space is a character. Whether the focus is such a box is
/// what the caller answers.
pub fn space_toggles_preview(text_focus: bool) -> bool {
    !text_focus
}

/// F2.4 S4: the contents of `cut/line.json`. Its own file, not `cut/`: the cut coming
/// into existence is what wakes Narrate and Produce up, and a line moved over a project
/// with no cut yet is not a cut — and not the project file, which a comparing autosave
/// writes and which would find itself different ten times a second.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LinePos {
    /// The session second the line stood on.
    pub t: f64,
}

/// F2.4 S4: `{"t": <session second>}` plus the trailing newline every file this app
/// writes carries. Written with one float so a restored line reads exactly what the
/// prototype's `lineFile` wrote.
pub fn line_json(pos: LinePos) -> String {
    format!("{{\"t\":{}}}\n", pos.t)
}

/// F2.4 S4: read `cut/line.json` back. Anything unreadable, not an object, or without a
/// usable `t` is no remembered line, and the line stays where the page put it. A number
/// written as an integer still counts — the file's own shape, not its spelling, is what
/// matters — so `as_f64` falls back to `as_i64`.
pub fn parse_line_json(text: &str) -> Option<LinePos> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    let raw = value.get("t")?;
    let t = raw.as_f64().or_else(|| raw.as_i64().map(|n| n as f64))?;
    if !t.is_finite() || t < 0.0 {
        return None;
    }
    Some(LinePos { t })
}

/// F2.4 S4: is the remembered line worth restoring? Only when the recording covers that
/// second — a line 40 minutes in is not restored into a recording that ends at 12, since
/// the sources changed in between. The cue (F1.4's thumbnail at the line's own frame)
/// and the scroll into view follow from the position rather than from a rule of their own.
pub fn restores_line(t: f64, recording_start: f64, recording_end: f64) -> bool {
    t >= recording_start && t < recording_end
}

/// F2.4 S4: the least time between two writes of the line. `spec/10-parameters.md`'s
/// "timeline:" block carries the ruler's and the scrollbar's numbers but none for this
/// rule, so it lives here with the rule that reads it (the prototype's `lineSaveMs`).
pub const LINE_WRITE_MS: u64 = 1000;

/// F2.4 S4: may this write go out? While the line moves, at most one write a second —
/// playback moves it ten times a second and a file write per tick is a write for
/// nothing — so the second's last position is what lands. `last_write_ms` is the
/// previous write's clock and `now_ms` this one's; a window closing always writes, so
/// the position you closed on is the one you open on.
pub fn may_write_line(last_write_ms: Option<u64>, now_ms: u64, closing: bool) -> bool {
    closing || last_write_ms.is_none_or(|then| now_ms.saturating_sub(then) >= LINE_WRITE_MS)
}

// --- S4: the file itself ---------------------------------------------------------------------------

/// The path S4 writes to, resolved off a project root without needing a [`crate::layout::Tree`]: the
/// same `cut/line.json` the tree hands out, spelled once here so a caller that only has the root (the
/// window, which holds no `Tree` yet) can still reach it.
pub fn line_file(root: &Path) -> PathBuf {
    root.join("cut/line.json")
}

/// S4: write the line to `<root>/cut/line.json`, creating `cut/` if it is not there.
///
/// Bytes exactly as [`line_json`] formats them — a restored line must read back what was written, and
/// the prototype's `lineFile` is the format being matched. Errors are returned rather than swallowed:
/// a line that could not be saved is worth knowing about, unlike a line that was never placed.
pub fn write_line(root: &Path, pos: LinePos) -> std::io::Result<()> {
    let file = line_file(root);
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&file, line_json(pos))
}

/// S4: read the saved line back. A missing or unreadable file is `None`, not an error — a project with
/// no line yet is the normal case, and so is a half-written file from a crash; both mean "no position
/// to restore", which is what [`parse_line_json`] already answers for bad bytes.
pub fn read_line(root: &Path) -> Option<LinePos> {
    let text = std::fs::read_to_string(line_file(root)).ok()?;
    parse_line_json(&text)
}

/// S4: the rate limit with its memory. The line moves ten times a second while playing and only needs
/// saving once a second, so something has to remember when the last write went out — that is this
/// struct's whole job.
#[derive(Debug, Default, Clone)]
pub struct LineWriter {
    last_write_ms: Option<u64>,
}

impl LineWriter {
    /// One move of the line. Writes only when [`may_write_line`] allows it and records the timestamp
    /// when it does. Returns whether a write happened, so a test sees the rate limit working rather
    /// than inferring it from a file that might have been written by something else.
    pub fn note_move(&mut self, pos: LinePos, root: &Path, now_ms: u64) -> bool {
        if !may_write_line(self.last_write_ms, now_ms, false) {
            return false;
        }
        // A failed write still counts as "tried at" this instant: retrying every tick after an ENOSPC
        // would be the same wasted-write loop the rate limit exists to prevent.
        self.last_write_ms = Some(now_ms);
        write_line(root, pos).is_ok()
    }

    /// Closing the window: always writes, whatever the rate limit says, so the position you closed on
    /// is the one you open on.
    pub fn flush(&mut self, pos: LinePos, root: &Path, now_ms: u64) -> bool {
        if !may_write_line(self.last_write_ms, now_ms, true) {
            return false;
        }
        self.last_write_ms = Some(now_ms);
        write_line(root, pos).is_ok()
    }

    /// When the last write went out, for a caller that wants to show or log it.
    pub fn last_write_ms(&self) -> Option<u64> {
        self.last_write_ms
    }

    /// Record a throttled write that happened somewhere else — used by a caller that has no filesystem
    /// to write to yet but must keep the same one-second rule. Mirrors what [`note_move`] does to the
    /// timestamp without touching a path.
    pub fn note_throttled(&mut self, now_ms: u64) {
        self.last_write_ms = Some(now_ms);
    }
}

/// S4: the line to restore on opening a project, if any.
///
/// Kept only when some recording still covers it ([`restores_line`]): a saved position in a gap that no
/// longer exists would cue the page to nothing. `recordings` is merged, sorted, disjoint session spans
/// — the same contract [`crate::timeline::filmed_runs`] produces — so "some recording covers it" is
/// one pass with no ambiguity about overlapping rows.
pub fn restore(root: &Path, recordings: &[(f64, f64)]) -> Option<LinePos> {
    let pos = read_line(root)?;
    recordings
        .iter()
        .any(|(start, end)| restores_line(pos.t, *start, *end))
        .then_some(pos)
}
