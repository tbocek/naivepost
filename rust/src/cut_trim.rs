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

/// A length of seconds the way the page writes one — [`tools::tenths`], the app's single spelling of a
/// duration (`spec/inventory/cut.md`'s own status spells it to a tenth). Kept as a name here because three of
/// this module's sentences read better with "span" in them than with a formatter's.
fn span(seconds: f64) -> String {
    tools::tenths(seconds)
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

/// §7 (`Inserts are files: never trimmed`): a card has no edge to grab. Its seconds are its own — a spliced one
/// carries a `dur`, an overwriting one runs for the footage it replaced — so trimming either end would be
/// editing a file rather than the cut, and the render would then disagree with what the page drew. The merge
/// half of the same rule is [`merge_pair`], which refuses a card for a partner.
pub fn trimmable(seg: &Seg) -> bool {
    !is_card(seg)
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

/// F2.8 S2: which horizontal band of the strip a press fell on, read from the press's y. The flowchart asks
/// these questions top to bottom — the recorders' band, then a wave strip, then the pictures/green bar — so
/// the answer comes from geometry rather than from a seam the page primes; that is what makes the wave-strip
/// branch reachable at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Band {
    /// The graduated ruler across the top.
    Ruler,
    /// The recorders' band: pressing here slides that one recording (flowchart M1).
    Recorders,
    /// The kept bar / green bar / clip row. Not band ground: the press falls through to scene or row.
    Bar,
    /// The wave strip under the pointer: that one recording slides (flowchart M2).
    Wave,
}

/// F2.8 S2: the vertical bands of the strip. Kept as three explicit spans instead of one height each so a
/// painter and a press can disagree about nothing: both read the same numbers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StripBands {
    /// The recorders' band, px from the strip's origin.
    pub recorders: (f64, f64),
    /// The kept/green bar row.
    pub bar: (f64, f64),
    /// The wave strip under it.
    pub wave: (f64, f64),
}

/// F2.8 S2: the placeholder band layout, derived from the two heights the painter already uses — no new
/// numbers. [`RULER_H`] (16 px) occupies the top; below it [`BAR_H`] (32 px) is split evenly between the
/// green bar and its wave strip because the placeholder draws ONE camera row and needs both halves reachable
/// by y on a strip this short. F2.10/F2.11 pass their real bands in through the same seam
/// ([`StripBands`]/[`band_at`]) once actual picture rows exist; until then this is what `track-strip` answers,
/// exactly the role `cut_select::PLACEHOLDER_BANDS` plays for the selection surface.
pub const PLACEHOLDER_STRIP_BANDS: StripBands = StripBands {
    recorders: (RULER_H, RULER_H + BAR_H / 2.0),
    // The bar's own span is stated as the FULL row under the ruler; `band_at` tests the wave band first, so
    // the wave strip takes the lower half of that row and the bar keeps the top half. Stated this way rather
    // than as two disjoint spans because the painter draws one row of height `BAR_H` — the split below is
    // where a press resolves to wave vs bar, not two separate painted heights.
    bar: (RULER_H + BAR_H / 2.0, RULER_H + BAR_H),
    // No room left under the bar on the placeholder, so the wave band sits over the bar's lower half: the
    // press still resolves it, and the painter moves when the real rows arrive.
    wave: (RULER_H + BAR_H / 2.0, RULER_H + BAR_H),
};

/// F2.8 S2: the band a press's y falls in. An unknown y answers [`Band::Bar`] rather than panicking — the
/// bar is the ground that falls through to the scene/row branch, which is the flowchart's own last resort,
/// so a stray y still produces a move instead of a crash.
///
/// NOTE the order: the WAVE band is tested before the bar. On the placeholder the two share their span
/// ([`PLACEHOLDER_STRIP_BANDS`]'s lower half), because there is no vertical room for a third row today;
/// testing the wave first is what makes the wave-strip branch reachable at all. When F2.10/F2.11 hand in
/// real, disjoint bands the order stops mattering.
pub fn band_at(y: f64, bands: &StripBands) -> Band {
    if y < RULER_H {
        return Band::Ruler;
    }
    if y >= bands.recorders.0 && y < bands.recorders.1 {
        return Band::Recorders;
    }
    if y >= bands.wave.0 && y < bands.wave.1 {
        return Band::Wave;
    }
    if y >= bands.bar.0 && y < bands.bar.1 {
        return Band::Bar;
    }
    Band::Bar
}

/// F2.8 S2: the single door from a press position to the three flowchart inputs the move needs —
/// `recorders_band`, `wave_strip` and `on_border`. The UI calls this instead of passing literals, so the
/// branch order lives in tested code rather than inside a gesture callback.
///
/// `on_border` is answered from the boxes actually drawn (`border_at`), not from a flag: a border only
/// exists where something was painted, and S2 refuses to slide "unless on a border" while S1 lets EITHER
/// button drag it, so the two sentences meet here.
pub fn press_targets(
    y: f64,
    x: f64,
    boxes: &[Box_],
    pps: f64,
    band_name: Option<&str>,
) -> (Option<String>, Option<String>, bool) {
    let _ = pps;
    let on_border = border_at(boxes, x).is_some();
    match band_at(y, &PLACEHOLDER_STRIP_BANDS) {
        Band::Recorders => (band_name.map(str::to_string), None, on_border),
        Band::Wave => (None, band_name.map(str::to_string), on_border),
        Band::Ruler | Band::Bar => (None, None, on_border),
    }
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

/// F2.8 S1: the tooltip that says a border is grabbable before a person tries it — the reach is
/// [`EDGE_GRAB_PX`], and either button takes it. `// layout.edgeGrabPx`
pub const TRIM_TIP: &str = "press within 6 px of a clip border to drag that border \u{2014} either button";

/// F2.8 S2: the tooltip for the right hand, naming what a right-drag does before it is tried.
pub const MOVE_TIP: &str =
    "right-drag moves what is under the pointer \u{2014} a recording, the selected scenes, one scene along its recording, or the whole row";

/// F2.8 S2/S3: the whole right-press gesture answered at once.
///
/// The page needs one answer per event rather than six questions asked in the right order every time, so
/// this composes the module's rules into the two things that can happen: nothing moved (a click, S3), or
/// something slid (S2) with everything the page must act on carried out — the corrected shift map, the
/// row it changed to if any, and the folds to open for the drag.
#[derive(Debug, Clone, PartialEq)]
pub enum Gesture {
    /// S3: an unmoved press is a click — nothing moved, nothing pushed, nothing to undo.
    Click,
    /// S2: something slid. `what` names it in the sentence, `status` IS that sentence, `shift` is the
    /// map after the correction, `row` is the row it changed to when the move was a row change, and
    /// `open` lists the folds to open for the drag.
    Slid {
        what: String,
        status: String,
        shift: BTreeMap<String, f64>,
        row: Option<usize>,
        open: Vec<usize>,
    },
}

/// What the page calls each thing the right button can take, in the words its status sentences use. Named
/// here because the same four nouns appear in the sentence whether the slide came from the band, the
/// selection, one scene or the row.
fn slide_noun(slide: &Slide) -> &'static str {
    match slide {
        Slide::Recording(_) => "the recording",
        Slide::Selection(_) => "the selected scenes",
        Slide::Scene(_) => "the scene",
        Slide::Row(_) => "the camera row",
    }
}

/// F2.8 S2/S3: one right gesture on one cut, answered whole.
///
/// `d_seconds` is how far the hand travelled, in seconds; `travel_px` and `row_change` say whether that
/// travel counts as a drag at all ([`opens_gesture`]: 3 px sideways, or any row change). The opening
/// state is read rather than the live one — `shift_at_open` and `already_pushed` come from the moment the
/// press began — because a drag fires an update per mouse-move event and asking the live map each time
/// compounds ten nudges into ten times the distance ([`slide_shift`] states the same rule for the map).
///
/// A gesture that changed nothing answers [`Gesture::Click`] even when it travelled: a scene pinned by a
/// neighbour on both sides has nowhere to go, and reporting "moved +0.00 s" for a hand that moved would
/// be a sentence about a change that did not happen.
pub fn right_gesture(
    press: &Press,
    travel_px: f64,
    row_change: bool,
    d_seconds: f64,
    segs: &[Seg],
    rec_start: f64,
    rec_end: f64,
    pps: f64,
    folds: &[[f64; 2]],
    shift_at_open: &BTreeMap<String, f64>,
    sources: &[String],
    already_pushed: bool,
) -> Gesture {
    // (a) S3: no travel past the slop and no row change means the press was a click.
    if !opens_gesture(travel_px, row_change) {
        return Gesture::Click;
    }

    let slide = slide_for(press);
    // Where the grab is, in seconds: what the folds are measured against.
    let grab = match &slide {
        Slide::Scene(index) => segs.get(*index).map(|seg| seg.s).unwrap_or(0.0),
        _ => press.selection.map(|(start, _)| start).unwrap_or(0.0),
    };

    // (b) the movement itself, per what was taken.
    let mut shift = shift_at_open.clone();
    let mut moved = 0.0_f64;
    match &slide {
        Slide::Recording(base) => {
            // The band and the wave strip both slide one recording: that is a shift of its own column.
            shift = slide_shift(&shift, std::slice::from_ref(base), d_seconds);
            moved = d_seconds;
        }
        Slide::Selection(scenes) => {
            // The marked scenes move together, each bounded by its own neighbours, so the cut shifts and
            // the film does not. Each scene's own landing is computed; the gesture reports the first
            // scene's delta as the number the sentence quotes.
            for scene in scenes {
                if let Some(seg) = segs.get(*scene) {
                    let landed = slide_scene(segs, *scene, seg.s + d_seconds, rec_start, rec_end, pps);
                    moved = landed - seg.s;
                }
            }
        }
        Slide::Scene(index) => {
            if let Some(seg) = segs.get(*index) {
                let landed = slide_scene(segs, *index, seg.s + d_seconds, rec_start, rec_end, pps);
                moved = landed - seg.s;
            }
        }
        Slide::Row(_) => {
            shift = slide_shift(shift_at_open, sources, d_seconds);
            moved = d_seconds;
        }
    }

    // (e) a gesture that moved nothing is a click, whatever the hand did.
    if !pushes_undo(moved != 0.0, false) && !(row_change && moved == 0.0) {
        return Gesture::Click;
    }

    // (c) the sentence: a row change says where it went, anything else says how far the clock moved.
    let what = slide_noun(&slide).to_string();
    let new_row = match (&slide, row_change) {
        (Slide::Scene(_), true) | (Slide::Recording(_), true) => Some(press.row),
        _ => None,
    };
    let status = match new_row {
        Some(row) => row_status(&what, row),
        None => shift_label(&what, moved),
    };

    // (d) the folds this drag travels through.
    let open = folds_to_open(folds, grab, d_seconds);

    // A second update of a hold whose first update already pushed still reports the state, but says so
    // with `already_pushed` handled by the caller; here it only decides click-vs-slid on a no-op repeat.
    if already_pushed && moved == 0.0 && new_row.is_none() {
        return Gesture::Click;
    }

    Gesture::Slid {
        what,
        status,
        shift,
        row: new_row,
        open,
    }
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

// --- S1: the strip's own geometry (what the draw callback paints) ---------------------------------------

/// F2.8 S1: what the strip says when the cut has nothing in it. A blank rectangle reads as a broken
/// widget; these words say the truth in plain words, and they are a constant so a test can pin them.
pub const STRIP_EMPTY_HINT: &str = "nothing cut yet — the kept stretches show up here";

/// F2.8 S1: how tall the strip's rows are, in px. The ruler takes the top band and the kept bar the rest,
/// so the painter and the size request agree on one number instead of two that can drift apart.
pub const RULER_H: f64 = 16.0;
pub const BAR_H: f64 = 32.0;

/// F2.8 S1: where each clip border is drawn, in px from the strip's origin.
///
/// Kept out of the painter because the press side has to ask for exactly the same numbers: a border the
/// hand grabs and a border the eye sees must be the same pixel, or a trim lands somewhere the pointer was
/// never pointing at. An insert contributes no borders at all (`spec/05-cut.md` rule 8).
pub fn border_positions(boxes: &[Box_]) -> Vec<(f64, usize, Border)> {
    boxes
        .iter()
        .filter(|b| !b.insert)
        .flat_map(|b| [(b.x, b.index, Border::Start), (b.x + b.w, b.index, Border::End)])
        .collect()
}

/// F2.8 S1: which border of which box a press x takes, over the whole drawn layout.
///
/// This is [`grab_border`] applied to every box at once, nearest wins, inserts skipped — the multi-clip
/// version of the single-box rule, so `ui` can seed a drag from one call and hold no arithmetic itself.
pub fn border_at(boxes: &[Box_], px: f64) -> Option<(usize, Border)> {
    let mut best: Option<(f64, usize, Border)> = None;
    for (edge, index, border) in border_positions(boxes) {
        let d = (px - edge).abs();
        if d <= EDGE_GRAB_PX && best.map_or(true, |(bd, _, _)| d < bd) {
            best = Some((d, index, border));
        }
    }
    best.map(|(_, index, border)| (index, border))
}


/// F2.8: one box on the placeholder strip — where a drawn thing sits and how wide it is, in pixels.
///
/// Kept as a struct rather than a tuple because the two flags are what the painter needs to know about a
/// segment and a `(f64, f64)` cannot carry them: an insert must not be painted as a trimmable clip
/// (`spec/05-cut.md` rule 8: inserts are files, never trimmed), and `index` is what the press hands back
/// to [`press_trim_border`]-style seams so a click on box N trims segment N and not its neighbour.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Box_ {
    /// Left edge, px from the strip's origin.
    pub x: f64,
    /// Width, px. Never negative: a zero-length segment draws as nothing rather than as a box that
    /// runs backwards into the one before it.
    pub w: f64,
    /// The index into the `segs` slice this box came from.
    pub index: usize,
    /// True for an insert (`Seg::is_insert`) — violet in the spec's colour list, and outside every trim
    /// reach for as long as rule 8 holds.
    pub insert: bool,
}

/// F2.8 S1: the boxes the kept-bar band draws, one per segment, at `pps` pixels per second.
///
/// This is the whole mapping from seconds to pixels that the strip is made of, held here instead of inside
/// the draw callback so a test can assert the numbers and so the press side can ask the same question in
/// reverse (which box is under this x). Insert segments come back flagged, not skipped: the band still has
/// to show that something is there, it just must not offer their borders to a trim.
pub fn clip_boxes(segs: &[Seg], pps: f64) -> Vec<Box_> {
    segs.iter()
        .enumerate()
        .map(|(index, seg)| Box_ {
            x: seg.s.max(0.0) * pps,
            w: ((seg.e - seg.s).max(0.0)) * pps,
            index,
            insert: seg.is_insert(),
        })
        .collect()
}

/// F2.8 S1: which box a press falls on, and which of its borders the pointer took.
///
/// A press within [`EDGE_GRAB_PX`] of a box's border reports that box with that border — nearest border
/// wins, so a clip narrower than two reaches still hands over the one the pointer is closer to, exactly as
/// [`grab_border`] does for one box (`// layout.edgeGrabPx`). A press inside a box but away from both
/// borders reports the box with `None`, which is what makes "on the green bar" and "on a border" two
/// different answers to the same lookup. Inserts answer `None` at every position: rule 8 keeps their
/// borders out of every trim reach.
pub fn box_at(boxes: &[Box_], px: f64) -> Option<(usize, Option<Border>)> {
    let mut best: Option<(f64, usize, Border)> = None;
    let mut trimmable = boxes.iter().filter(|b| !b.insert);

    for b in trimmable.clone() {
        for (edge, border) in [(b.x, Border::Start), (b.x + b.w, Border::End)] {
            let d = (px - edge).abs();
            if d <= EDGE_GRAB_PX && best.map_or(true, |(bd, _, _)| d < bd) {
                best = Some((d, b.index, border));
            }
        }
    }
    if let Some((_, index, border)) = best {
        return Some((index, Some(border)));
    }

    trimmable
        .find(|b| px >= b.x && px <= b.x + b.w)
        .map(|b| (b.index, None))
}

/// F2.8 S1: a spliced insert's box on the strip. Its `s == e` gives it no width of its own, so the card
/// is drawn standing at `at` (the second it plays from) for as long as its `dur` — otherwise every card
/// would be a hairline nobody can see or click. Flagged `insert`, so neither its edges nor its body ever
/// answer a press (`spec/05-cut.md` rule 8).
pub fn insert_box(seg: &Seg, at: f64, pps: f64) -> Box_ {
    Box_ {
        x: at.max(0.0) * pps,
        w: seg.dur.max(0.0) * pps,
        index: 0,
        insert: true,
    }
}

/// F2.8 S1: the ruler row above the band — a tick every `every_seconds` up to `rec_end`, each with its
/// clock label, spelled the way `spec/img/05-trim.png` prints them (`0:00`, `0:30`, `1:00`).
///
/// The label drops the tenth that [`preview::clock`] carries: a ruler marks whole seconds, and a tenth on
/// every tick would read like a measurement of something moving. A non-positive span, zoom or spacing
/// answers with no ticks at all rather than an infinite loop or a single stray mark.
pub fn ruler_ticks(rec_end: f64, pps: f64, every_seconds: f64) -> Vec<(f64, String)> {
    if rec_end <= 0.0 || pps <= 0.0 || every_seconds <= 0.0 {
        return Vec::new();
    }
    let mut ticks = Vec::new();
    let mut t = 0.0;
    while t <= rec_end + f64::EPSILON {
        // `tools::mm_ss` and not `preview::clock`: the ruler marks whole seconds, and the tenth the
        // playhead clock carries would turn every label into `00:300`.
        ticks.push((t * pps, tools::mm_ss(t)));
        t += every_seconds;
    }
    ticks
}

// --- S3: what the right button never does ----------------------------------------------------------------

/// F2.8 S3: a right click never moves the red line. Where the left button puts it is F2.4's
/// [`crate::cut_line::click_outcome`]; this is the one rule that says what the other button leaves alone, so
/// the two hands can aim at a second and move footage there without the preview jumping.
pub fn right_press_moves_line() -> bool {
    false
}
