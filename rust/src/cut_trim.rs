//! F2.8 (Trim and move) — `spec/05-cut.md` F2.8.
//!
//! Two hands on the same seconds. The left or right button on a clip border drags that border: an end
//! stops at a scene's worth of length, at the next clip and at the recording's end, and on release a
//! neighbour it landed against merges back into one scene. The right button elsewhere moves — one
//! recording, the selected scenes, one scene along its recording, or a whole camera row along the clock,
//! which is where a shift correction comes from. An unmoved press is a click, and a right click never
//! moves the red line.
//!
//! The three §10 numbers this flow shares with F2.6 are read from [`crate::cut_select`] rather than
//! repeated: [`crate::cut_select::SNAP_PX`] is "flush within 8 px",
//! [`crate::cut_select::MIN_SECONDS`] is the 0.04 s that counts as touching, and
//! [`crate::cut_select::MIN_SCENE_SECONDS`] is the shortest scene a border may leave. One row per id in
//! the catalogue, one home per number in the code.
//!
//! No UI lives here and none is wired: `rust/src/ui/window.rs` renders only the Prepare page, so there is
//! no Cut screen to attach a drag gesture to yet (as with F2.1–F2.6), and no snapshot of this flow's
//! widgets exists to compare against `spec/img/05-trim.png`. The widget layer will report pixels and
//! forwards and draw what these return.

use std::collections::BTreeMap;

use crate::cut::Seg;
use crate::cut_select::{MIN_SCENE_SECONDS, MIN_SECONDS, SNAP_PX};
use crate::{preview, tools};

// --- S1: dragging a border ------------------------------------------------------------------------------

/// F2.8 S1: how near a clip border the pointer has to be for either button to take it —
/// `spec/inventory/cut.md`'s "Trim: edges within 6 px (own side first)", and the shortest of
/// `spec/10-parameters.md`'s grab reaches.
pub const EDGE_GRAB_PX: f64 = 6.0;

/// F2.8 S1: which border of a clip a press takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Border {
    Start,
    End,
}

/// F2.8 S1: the border within [`EDGE_GRAB_PX`] of `px` on a clip drawn between `start_px` and `end_px`.
///
/// Within reach of both — a clip narrower than two reaches — the nearer one wins: the hand is closer to it,
/// and that is the arrow the cursor showed. A tie cannot arise from a real clip (the start is always left of
/// the end, so the midpoint is never equidistant), and if it were handed equal pixels the end answers, being
/// the border drawn last and therefore the one on top of the other's reach.
pub fn grab_border(px: f64, start_px: f64, end_px: f64) -> Option<Border> {
    let to_start = (px - start_px).abs();
    let to_end = (px - end_px).abs();
    let start = to_start <= EDGE_GRAB_PX;
    let end = to_end <= EDGE_GRAB_PX;
    match (start, end) {
        (true, true) => Some(if to_start < to_end { Border::Start } else { Border::End }),
        (true, false) => Some(Border::Start),
        (false, true) => Some(Border::End),
        (false, false) => None,
    }
}

/// F2.8 S1: the nearest clips before and after `segs[index]` in time — what "the next area" and "the previous clip"
/// mean to a hand that can see them: the neighbour is by position on the timeline, not by index, because a
/// card laid over the footage sits between two clips of a camera without being either's neighbour. A segment
/// overlapping this one (a card laid over it, or an edit mid-trim) is not before or after it either, so it
/// bounds nothing — otherwise a clip would be fenced in by its own overlay and could not move at all.
fn neighbours(segs: &[Seg], index: usize) -> (f64, f64) {
    let Some(held) = segs.get(index) else {
        return (f64::MIN, f64::MAX);
    };
    let mut before = f64::MIN;
    let mut after = f64::MAX;
    for (other, seg) in segs.iter().enumerate() {
        if other == index {
            continue;
        }
        if seg.e <= held.s && seg.e > before {
            before = seg.e;
        }
        if seg.s >= held.e && seg.s < after {
            after = seg.s;
        }
    }
    (before, after)
}

/// F2.8 S1: where the dragged border may go — an end not below `start + 1 s`, past the next clip's start or
/// past the recording's end; a start not above `end - 1 s`, below the previous clip's end or below the
/// recording's start (`// P.policy.minSceneSeconds` is that 1 s, read from [`crate::cut_select`] where §10's
/// row is catalogued).
///
/// Clamped, never snapped: a border follows the pointer exactly and only stops when it would break one of
/// those bounds, which is what makes a trim feel like cutting film rather than being magnetised around. The
/// next clip's start is therefore an exact stop — `39` stays `39`, one second short of it — because "not
/// past the next clip" is a wall at 40 and not a slope leading to it.
pub fn clamp_edge(segs: &[Seg], index: usize, border: Border, t: f64, rec_start: f64, rec_end: f64) -> f64 {
    let Some(held) = segs.get(index) else {
        return t;
    };
    let (before, after) = neighbours(segs, index);
    match border {
        Border::End => t.clamp(held.s + MIN_SCENE_SECONDS, (rec_end.min(after)).max(held.s + MIN_SCENE_SECONDS)),
        Border::Start => t.clamp((rec_start.max(before)).min(held.e - MIN_SCENE_SECONDS), held.e - MIN_SCENE_SECONDS),
    }
}

/// F2.8 S1: is this a new edge at all? `false` when the clamped value is the border's own value — a drag
/// that lands back where it started is not an edit, and pushing one onto the history per mouse move would
/// be fifty steps for one act of the hand.
pub fn trims(segs: &[Seg], index: usize, border: Border, t: f64, rec_start: f64, rec_end: f64) -> bool {
    let Some(held) = segs.get(index) else {
        return false;
    };
    let now = match border {
        Border::End => held.e,
        Border::Start => held.s,
    };
    clamp_edge(segs, index, border, t, rec_start, rec_end) != now
}

/// F2.8 S1 ("picture scrubs live (throttled)"): the least time between two scrub seeks —
/// `spec/inventory/cut.md`'s "live scrub throttled 90 ms", one of §10's timeline numbers with no `P.*` id
/// of its own, so it lives with the rule that reads it (the precedent `params.rs` sets for
/// `machine.jpegQuality`).
pub const SCRUB_MS: u64 = 90;

/// F2.8 S1: may this tick scrub the picture? A flushing accurate seek per mouse-move event is a pipeline
/// that never stops flushing, so the line follows at most every [`SCRUB_MS`]. `None` is a drag that has not
/// scrubbed yet, and always does.
pub fn scrubs(last_ms: Option<u64>, now_ms: u64) -> bool {
    last_ms.is_none_or(|then| now_ms.saturating_sub(then) >= SCRUB_MS)
}

/// F2.8 S1 ("the picture lands on the edge"): which second the preview shows for a border. A start shows
/// itself; an end shows the frame before the boundary — `spec/inventory/cut.md`: "the end edge shows the
/// frame before the boundary", because the frame at the boundary is already outside the clip and would show
/// what the trim just removed. `frame_seconds` is the project's own frame interval, never a model's guess.
pub fn scrub_second(border: Border, start: f64, end: f64, frame_seconds: f64) -> f64 {
    match border {
        Border::Start => start,
        Border::End => end - frame_seconds,
    }
}

/// F2.8 S1: what the page says when a border is picked up — `spec/inventory/cut.md` verbatim, on the
/// tenth-of-a-second clock so the number matches the one beside the red line. `clip_no` counts from 1: the
/// page numbers clips as a person points at them.
pub fn picked_up_status(clip_no: usize, border: Border, t: f64) -> String {
    let which = match border {
        Border::Start => "start",
        Border::End => "end",
    };
    format!(
        "clip {clip_no}'s {which} picked up at {} \u{2014} right-drag to trim",
        preview::clock(Some(t))
    )
}

/// A length of seconds the way the page writes one: to a tenth, which is what `spec/inventory/cut.md`'s own
/// status spells ("clip N: a – b (X s[, Y s in the video])" comes from the prototype's `%.1f`). Whole
/// seconds therefore read `20.0 s` — the same reading at both ends of the range, not a whole number that
/// quietly means "we did not measure".
fn span(seconds: f64) -> String {
    format!("{:.1} s", seconds)
}

/// F2.8 S1: what the page says when the border is let go — the clip, the span it now covers, and its length.
pub fn trim_status(clip_no: usize, start: f64, end: f64) -> String {
    format!(
        "clip {clip_no}: {} \u{2013} {} ({})",
        tools::mm_ss(start),
        tools::mm_ss(end),
        span(end - start)
    )
}

// --- S1: the neighbour a drop joins back to -------------------------------------------------------------

/// F2.8 S1: is this segment a card — spliced or overwriting, anything with `ins`? The page calls any of them
/// an insert (`Seg::Ins != ""` in the prototype); [`Seg::is_insert`] alone would miss an overwriting card, and
/// merging one into the clip it covers would eat that clip whole. A rule of F2.8 about cards, so it lives here
/// rather than as a second helper on [`Seg`].
fn is_card(seg: &Seg) -> bool {
    !seg.ins.is_empty()
}

/// F2.8 S1 ("on release a neighbour within 0.04 s merges (same camera only, never an insert)"): the clip
/// this one has come to touch, or nothing. [`crate::cut_select::MIN_SECONDS`] is that tolerance —
/// `// P.eng.minPieceSeconds` — and it is a frame or two, not a visible gap: anything a hand can see at the
/// top zoom is wider, and a hole removed from the middle of a scene would otherwise be closed again by the
/// merge. A card is refused on both sides — any segment with `ins`, spliced or overwriting, which is what
/// "insert" means to the page ([`is_card`]) and wider than [`crate::cut::Seg::is_insert`], that only knows a
/// spliced card. An overwriting card runs for exactly the footage it covers, so its borders are other clips'
/// own borders and merging on them would eat a clip whole.
pub fn merge_pair(segs: &[Seg], held: usize) -> Option<usize> {
    let held_seg = segs.get(held)?;
    let near = |a: f64, b: f64| (a - b).abs() <= MIN_SECONDS;
    let pair = segs.iter().enumerate().find(|(other, seg)| {
        *other != held && !is_card(seg) && seg.cam == held_seg.cam && (near(seg.s, held_seg.e) || near(seg.e, held_seg.s))
    });
    if is_card(held_seg) {
        return None;
    }
    pair.map(|(other, _)| other)
}

/// F2.8 S1: join the pair in place — widen the earlier piece over the neighbour, drop the neighbour, and
/// clear the survivor's `split`, which is what "the drag clears a Split border between them" means in the
/// data: a border | Split put there on purpose survives every rearrangement until something joins it back.
/// Returns the held index and how many clips were removed (one, when it merged). The earlier piece keeps its
/// identity so the page still calls it clip N.
pub fn merge(segs: &mut Vec<Seg>, held: usize) -> Option<(usize, usize)> {
    let other = merge_pair(segs, held)?;
    let (first, second) = if held < other { (held, other) } else { (other, held) };
    segs[first].s = segs[first].s.min(segs[second].s);
    segs[first].e = segs[first].e.max(segs[second].e);
    // The border being closed may be the survivor's own split flag: the earlier piece is the one that stays,
    // and it now runs on through where the border was.
    segs[first].split = false;
    segs.remove(second);
    Some((held, 1))
}

/// F2.8 S1: what the merge says — with its own Undo sentence, since taking the border back is the thing a
/// person who just watched two clips become one may want most.
pub fn joined_status(start: f64, end: f64) -> String {
    format!(
        "joined into one scene, {} \u{2013} {} ({}) \u{2014} \u{21b6} Undo puts the border back",
        tools::mm_ss(start),
        tools::mm_ss(end),
        span(end - start)
    )
}

// --- S2: what the right button takes ---------------------------------------------------------------------

/// F2.8 S2: what a right-press picked up, in the order the flowchart asks.
#[derive(Debug, Clone, PartialEq)]
pub enum Slide {
    /// That recording slides — the recorders' band, or the wave strip under the pointer.
    Recording(String),
    /// The selected scenes slide across the footage: the cut moves, not the film.
    Selection(Vec<usize>),
    /// One scene slides along its recording, length kept.
    Scene(usize),
    /// A whole camera row slides along the clock — the shift correction.
    Row(usize),
}

/// F2.8 S2: everything about a right-press that decides what it takes. A struct rather than eight arguments
/// so a call site reads as the questions the flowchart asks, in its order.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Press<'a> {
    /// A recording on the recorders' band under the pointer.
    pub recorders_band: Option<&'a str>,
    /// The recording whose wave strip is under the pointer.
    pub wave_strip: Option<&'a str>,
    /// The footage selection's span, if there is one.
    pub selection: Option<(f64, f64)>,
    /// The scenes that selection holds.
    pub selected: Vec<usize>,
    /// Whether the press fell inside that selection.
    pub inside_selection: bool,
    /// Whether it fell on a border, which is a trim and never a slide of the selection.
    pub on_border: bool,
    /// The scene under the pointer — its green, bar or clip.
    pub scene: Option<usize>,
    /// The camera row the press landed on.
    pub row: usize,
}

/// F2.8 S2: what this right-press moves. In order: a recording on the recorders' band; the recording under
/// the pointer's wave strip; the selected scenes when the press falls inside a footage selection — even over
/// a green bar, because the ask is the marked seconds and not the clip under them — unless it landed on a
/// border, which is a trim; one scene along its recording; else the whole camera row along the clock.
///
/// A selection with no scenes in it is not a press inside one: there is nothing to slide, and falling
/// through to the row would move footage the person never marked.
pub fn slide_for(press: &Press) -> Slide {
    if let Some(recording) = press.recorders_band {
        return Slide::Recording(recording.to_string());
    }
    if let Some(recording) = press.wave_strip {
        return Slide::Recording(recording.to_string());
    }
    let inside = press.selection.is_some() && press.inside_selection && !press.on_border;
    if inside && !press.selected.is_empty() {
        return Slide::Selection(press.selected.clone());
    }
    match press.scene {
        Some(scene) => Slide::Scene(scene),
        None => Slide::Row(press.row),
    }
}

/// F2.8 S2 ("the scene slides along its recording · length kept · clear of neighbours · flush within 8 px"):
/// where the scene's start lands when it is slid to `to`.
///
/// Bounded by its recording and by the nearest clips before and after, then snapped flush against a bound it
/// came within [`crate::cut_select::SNAP_PX`] pixels of — the near one first, so a hand nudging a clip up
/// behind another meets it exactly instead of a pixel short. Pixels because the reach is the hand's, the
/// same reason F2.6's snap is pixels. An insert may go anywhere the neighbours allow: this clamps footage
/// and cards by one geometry and leaves the caller to decide which to accept.
pub fn slide_scene(segs: &[Seg], index: usize, to: f64, rec_start: f64, rec_end: f64, pps: f64) -> f64 {
    let Some(held) = segs.get(index) else {
        return to;
    };
    let length = held.e - held.s;
    let (before, after) = neighbours(segs, index);
    let lo = rec_start.max(before);
    let hi = (rec_end.min(after) - length).max(lo);
    let reach = SNAP_PX / pps.max(0.001);
    if (to - lo).abs() <= reach {
        return lo;
    }
    // The far bound is a start, so it is compared as the end it implies: `hi` already has the length off it.
    if (to - hi).abs() <= reach {
        return hi;
    }
    to.clamp(lo, hi)
}

/// F2.8 S2 ("sideways travel counts from 3 px"): how far the pointer has to go sideways before a right-press
/// is a drag rather than a click. Shorter than the left button's 4 px slop on purpose — the right button has
/// no click action that a stray move could destroy, and a slide you did not mean is undone by one key.
pub const ROW_TRAVEL_PX: f64 = 3.0;

/// F2.8 S2: does this press open a gesture? Sideways travel counts from [`ROW_TRAVEL_PX`]; a vertical move to
/// another row needs none, because pointing at another row that has room *is* the ask.
pub fn opens_gesture(travel_px: f64, row_change: bool) -> bool {
    travel_px >= ROW_TRAVEL_PX || row_change
}

/// F2.8 S3: an unmoved press is a click — the inverse of [`opens_gesture`], kept as its own name because the
/// page reads the two questions at different moments.
pub fn is_click(travel_px: f64, row_change: bool) -> bool {
    !opens_gesture(travel_px, row_change)
}

/// F2.8 S2 ("one Undo per gesture"): push only on the first real move of a hold. A drag is one act of the
/// hand, and fifty history entries for it would be the whole undo depth spent on one clip.
pub fn pushes_undo(moved: bool, already_pushed: bool) -> bool {
    moved && !already_pushed
}

/// F2.8 S2 (the shift correction): the map after `sources` have been slid by `d`, everyone else untouched.
/// A source not in the map yet was at 0 and lands on `d`.
///
/// Read from the gesture's OPENING map, not the current one: a drag fires an update per mouse-move event,
/// and each asking the live map would compound ten nudges into ten times the distance the hand travelled.
/// That is also what makes one Undo enough — the whole gesture is one step from the opening state to this.
pub fn slide_shift(from: &BTreeMap<String, f64>, sources: &[String], d: f64) -> BTreeMap<String, f64> {
    let mut shifted = from.clone();
    for source in sources {
        let start = shifted.get(source).copied().unwrap_or(0.0);
        shifted.insert(source.clone(), start + d);
    }
    shifted
}

/// F2.8 S2 ("folds near the grab open for the drag, refold after"): the folds this gesture travels through,
/// as indices into `folds`.
///
/// "Near" is read as "over what moves under the hand" — the stretch from the press's column to where the
/// drag has got to, which with no travel yet is just that column. A fold elsewhere is untouched by this drag,
/// and opening it would have nothing to refold afterwards.
pub fn folds_to_open(folds: &[[f64; 2]], grab: f64, travel: f64) -> Vec<usize> {
    let (from, to) = if travel >= 0.0 { (grab, grab + travel) } else { (grab + travel, grab) };
    folds
        .iter()
        .enumerate()
        .filter(|(_, [start, end])| *end >= from && *start <= to)
        .map(|(index, _)| index)
        .collect()
}

/// F2.8 S2: what the status line says about a slide — signed seconds to two decimals, because lining two
/// waveforms up by eye is arithmetic nobody wants to do twice and the number is what makes the correction
/// repeatable on the next project shot with the same two devices. Zero says so rather than "moved +0.00 s".
pub fn shift_label(what: &str, d: f64) -> String {
    if d == 0.0 {
        return format!("{what} is back where it started");
    }
    format!("{what} moved {d:+.2} s")
}

/// F2.8 S2: what a row change says. Rows are counted from 1 in the sentence and numbered from 0 in `Cut::rows`,
/// so the page's "row 3" is the map's `2`.
pub fn row_status(what: &str, row: usize) -> String {
    format!("{what} moved to row {} \u{2014} its kept scenes came along", row + 1)
}

// --- S3: what the right button never does ----------------------------------------------------------------

/// F2.8 S3: a right click never moves the red line. Where the left button puts it is F2.4's
/// [`crate::cut_line::click_outcome`]; this is the one rule that says what the other button leaves alone, so
/// the two hands can aim at a second and move footage there without the preview jumping.
pub fn right_press_moves_line() -> bool {
    false
}
