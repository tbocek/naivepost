//! F2.8 (Trim and move), `spec/05-cut.md` F2.8.
//! Left or right button on a clip border drags it (an end stops at a scene's worth of length, the next clip and the recording's end; on release a landed-against neighbour merges). The right button elsewhere moves one recording, the selected scenes, one scene along its recording, or a whole camera row (the shift correction). An unmoved press is a click; a right click never moves the red line.
//! Shared §10 numbers are read from [`crate::cut_select`]: [`crate::cut_select::SNAP_PX`] (flush within 8 px), [`crate::cut_select::MIN_SECONDS`] (0.04 s counts as touching), [`crate::cut_select::MIN_SCENE_SECONDS`] (shortest scene a border may leave).
//! No UI is wired here; the widget layer reports pixels and draws what these return.

use std::collections::BTreeMap;

use crate::cut::Seg;
use crate::cut_select::{MIN_SCENE_SECONDS, MIN_SECONDS, SNAP_PX};
use crate::{preview, tools};

/// F2.8 S1: how near a clip border the pointer must be for either button to take it (`spec/inventory/cut.md` "Trim: edges within 6 px (own side first)"), the shortest of §10's grab reaches.
pub const EDGE_GRAB_PX: f64 = 6.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Border {
    Start,
    End,
}

/// F2.8 S1: the border within [`EDGE_GRAB_PX`] of `px` on a clip drawn between `start_px` and `end_px`. Within reach of both the nearer wins; on a tie the end answers (drawn last, on top).
pub fn grab_border(px: f64, start_px: f64, end_px: f64) -> Option<Border> {
    let to_start = (px - start_px).abs();
    let to_end = (px - end_px).abs();
    let start = to_start <= EDGE_GRAB_PX;
    let end = to_end <= EDGE_GRAB_PX;
    match (start, end) {
        (true, true) => Some(if to_start < to_end {
            Border::Start
        } else {
            Border::End
        }),
        (true, false) => Some(Border::Start),
        (false, true) => Some(Border::End),
        (false, false) => None,
    }
}

/// F2.8 S1: the nearest clips before and after `segs[index]` BY POSITION on the timeline, not index. A segment overlapping this one (a card over it, or an edit mid-trim) bounds nothing, else a clip is fenced in by its own overlay.
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

/// F2.8 S1: where the dragged border may go: an end not below `start + 1 s` and not past the next clip's start or the recording's end; a start not above `end - 1 s` and not below the previous clip's end or the recording's start (the 1 s is `// P.policy.minSceneSeconds`, read from [`crate::cut_select`]).
/// Clamped, never snapped: the border follows the pointer and stops at a wall (`39` stays `39` short of a clip at 40).
pub fn clamp_edge(
    segs: &[Seg],
    index: usize,
    border: Border,
    t: f64,
    rec_start: f64,
    rec_end: f64,
) -> f64 {
    let Some(held) = segs.get(index) else {
        return t;
    };
    let (before, after) = neighbours(segs, index);
    match border {
        Border::End => t.clamp(
            held.s + MIN_SCENE_SECONDS,
            (rec_end.min(after)).max(held.s + MIN_SCENE_SECONDS),
        ),
        Border::Start => t.clamp(
            (rec_start.max(before)).min(held.e - MIN_SCENE_SECONDS),
            held.e - MIN_SCENE_SECONDS,
        ),
    }
}

/// F2.8 S1: is this a new edge at all? A drag landing back where it started is not an edit and must not push history.
pub fn trims(
    segs: &[Seg],
    index: usize,
    border: Border,
    t: f64,
    rec_start: f64,
    rec_end: f64,
) -> bool {
    let Some(held) = segs.get(index) else {
        return false;
    };
    let now = match border {
        Border::End => held.e,
        Border::Start => held.s,
    };
    clamp_edge(segs, index, border, t, rec_start, rec_end) != now
}

/// F2.8 S1 ("picture scrubs live (throttled)"): least time between two scrub seeks, `spec/inventory/cut.md`'s "live scrub throttled 90 ms". No `P.*` id (precedent: `machine.jpegQuality`).
pub const SCRUB_MS: u64 = 90;

/// F2.8 S1: may this tick scrub the picture? Accurate seeks flush the pipeline, so at most every [`SCRUB_MS`]; `None` (not scrubbed yet) always does.
pub fn scrubs(last_ms: Option<u64>, now_ms: u64) -> bool {
    last_ms.is_none_or(|then| now_ms.saturating_sub(then) >= SCRUB_MS)
}

/// F2.8 S1 ("the picture lands on the edge"): the second the preview shows for a border. A start shows itself; an end shows the frame before the boundary (`spec/inventory/cut.md`). `frame_seconds` is the project's own frame interval.
pub fn scrub_second(border: Border, start: f64, end: f64, frame_seconds: f64) -> f64 {
    match border {
        Border::Start => start,
        Border::End => end - frame_seconds,
    }
}

/// F2.8 S1: the status when a border is picked up (`spec/inventory/cut.md` verbatim) on the tenth-of-a-second clock; `clip_no` counts from 1.
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

/// A length of seconds as the page writes it ([`tools::tenths`], to a tenth).
fn span(seconds: f64) -> String {
    tools::tenths(seconds)
}

pub fn trim_status(clip_no: usize, start: f64, end: f64) -> String {
    format!(
        "clip {clip_no}: {} \u{2013} {} ({})",
        tools::mm_ss(start),
        tools::mm_ss(end),
        span(end - start)
    )
}

/// F2.8 S1: is this segment a card (spliced or overwriting, anything with `ins`)? [`Seg::is_insert`] alone would miss an overwriting card, and merging one would eat the clip it covers.
fn is_card(seg: &Seg) -> bool {
    !seg.ins.is_empty()
}

/// §7 (`Inserts are files: never trimmed`): a card has no edge to grab; trimming would edit a file, not the cut. The merge half is [`merge_pair`], which refuses a card partner.
pub fn trimmable(seg: &Seg) -> bool {
    !is_card(seg)
}

/// F2.8 S1 ("on release a neighbour within 0.04 s merges (same camera only, never an insert)"): the clip this one has come to touch, or nothing. [`crate::cut_select::MIN_SECONDS`] (`// P.eng.minPieceSeconds`) is a frame or two, not a visible gap, else a hole removed mid-scene would be closed again. A card ([`is_card`], wider than [`crate::cut::Seg::is_insert`]) is refused on both sides: an overwriting card's borders are other clips' own.
pub fn merge_pair(segs: &[Seg], held: usize) -> Option<usize> {
    let held_seg = segs.get(held)?;
    let near = |a: f64, b: f64| (a - b).abs() <= MIN_SECONDS;
    let pair = segs.iter().enumerate().find(|(other, seg)| {
        *other != held
            && !is_card(seg)
            && seg.cam == held_seg.cam
            && (near(seg.s, held_seg.e) || near(seg.e, held_seg.s))
    });
    if is_card(held_seg) {
        return None;
    }
    pair.map(|(other, _)| other)
}

/// F2.8 S1: join the pair in place: widen the earlier piece over the neighbour, drop it, and clear the survivor's `split` (a deliberate
/// Split survives until something joins it). Returns the held index and clips removed; the earlier piece keeps its identity.
pub fn merge(segs: &mut Vec<Seg>, held: usize) -> Option<(usize, usize)> {
    let other = merge_pair(segs, held)?;
    Some(join(segs, held, other))
}

pub fn merge_at_border(segs: &mut Vec<Seg>, held: usize, border: Border) -> Option<usize> {
    let seg = segs.get(held).filter(|seg| !is_card(seg))?;
    let at = match border {
        Border::Start => seg.s,
        Border::End => seg.e,
    };
    let cam = seg.cam;
    let other = segs.iter().enumerate().position(|(other, seg)| {
        let touching = match border {
            Border::Start => seg.e,
            Border::End => seg.s,
        };
        other != held && !is_card(seg) && seg.cam == cam && (touching - at).abs() <= MIN_SECONDS
    })?;
    join(segs, held, other);
    Some(held.min(other))
}

fn join(segs: &mut Vec<Seg>, held: usize, other: usize) -> (usize, usize) {
    let (first, second) = if held < other {
        (held, other)
    } else {
        (other, held)
    };
    segs[first].s = segs[first].s.min(segs[second].s);
    segs[first].e = segs[first].e.max(segs[second].e);
    // The closed border may be the survivor's own split flag: the earlier piece stays and runs on through it.
    segs[first].split = false;
    segs.remove(second);
    (held, 1)
}

/// F2.8 S1: what the merge says, with its own Undo sentence.
pub fn joined_status(start: f64, end: f64) -> String {
    format!(
        "joined into one scene, {} \u{2013} {} ({}) \u{2014} \u{21b6} Undo puts the border back",
        tools::mm_ss(start),
        tools::mm_ss(end),
        span(end - start)
    )
}

/// F2.8 S2: which horizontal band of the strip a press fell on, from y, in the flowchart's order (recorders' band, wave strip, then pictures/green bar); geometry, not a primed seam, makes the wave-strip branch reachable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Band {
    Ruler,
    Recorders,
    Bar,
    Wave,
}

/// F2.8 S2: the vertical bands of the strip as three explicit spans, so painter and press read the same numbers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StripBands {
    pub recorders: (f64, f64),
    pub bar: (f64, f64),
    pub wave: (f64, f64),
}

/// F2.8 S2: the placeholder band layout from the painter's heights: [`RULER_H`] (16 px) on top; [`BAR_H`] (32 px) split between the green bar and its wave strip, as one camera row needs both reachable by y. Real bands come through [`StripBands`]/[`band_at`] with F2.10/F2.11 (as `cut_select::PLACEHOLDER_BANDS`).
pub const PLACEHOLDER_STRIP_BANDS: StripBands = StripBands {
    recorders: (RULER_H, RULER_H + BAR_H / 2.0),
    // The bar's span is the FULL row under the ruler and the wave strip takes its lower half ([`band_at`] tests wave first); the painter draws one row of `BAR_H`.
    bar: (RULER_H + BAR_H / 2.0, RULER_H + BAR_H),
    // No room under the bar on the placeholder, so the wave band overlaps its lower half.
    wave: (RULER_H + BAR_H / 2.0, RULER_H + BAR_H),
};

/// F2.8 S2: the band a press's y falls in. Unknown y answers [`Band::Bar`] (the flowchart's last resort) rather than panic. NOTE: the WAVE band is tested BEFORE the bar since on the placeholder they share a span ([`PLACEHOLDER_STRIP_BANDS`]); with real disjoint bands the order stops mattering.
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

/// F2.8 S2: the door from a press position to the flowchart inputs `recorders_band`, `wave_strip`, `on_border`, so branch order lives in tested code. `on_border` comes from the drawn boxes (`border_at`): S2 refuses to slide "unless on a border" while S1 lets EITHER button drag it.
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

#[derive(Debug, Clone, PartialEq)]
pub enum Slide {
    Recording(String),
    Selection(Vec<usize>),
    Scene(usize),
    Row(usize),
}

/// F2.8 S2: everything about a right-press that decides what it takes, as a struct rather than eight arguments.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Press<'a> {
    pub recorders_band: Option<&'a str>,
    pub wave_strip: Option<&'a str>,
    pub selection: Option<(f64, f64)>,
    pub selected: Vec<usize>,
    pub inside_selection: bool,
    pub on_border: bool,
    pub scene: Option<usize>,
    pub row: usize,
}

/// F2.8 S2: what this right-press moves, in order: a recording on the recorders' band; the recording under the wave strip; the selected scenes when the press falls inside a footage selection (even over a green bar) unless on a border, which is a trim; one scene along its recording; else the whole camera row. A selection with no scenes is not a press inside one.
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

/// F2.8 S2 ("the scene slides along its recording · length kept · clear of neighbours · flush within 8 px"): where the scene's start lands when slid to `to`. Bounded by its recording and the nearest clips, then snapped flush to a bound within [`crate::cut_select::SNAP_PX`] pixels (the near one first). Pixels because the reach is the hand's. Footage and cards share one geometry.
pub fn slide_scene(
    segs: &[Seg],
    index: usize,
    to: f64,
    rec_start: f64,
    rec_end: f64,
    pps: f64,
) -> f64 {
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
    if (to - hi).abs() <= reach {
        return hi;
    }
    to.clamp(lo, hi)
}

/// F2.8 S2 ("sideways travel counts from 3 px"): how far the pointer must go sideways before a right-press is a drag. Shorter than the left button's 4 px since a stray slide is undone by one key.
pub const ROW_TRAVEL_PX: f64 = 3.0;

/// F2.8 S2: does this press open a gesture? Sideways travel from [`ROW_TRAVEL_PX`]; a move to another row needs none.
pub fn opens_gesture(travel_px: f64, row_change: bool) -> bool {
    travel_px >= ROW_TRAVEL_PX || row_change
}

/// F2.8 S3: an unmoved press is a click, the inverse of [`opens_gesture`].
pub fn is_click(travel_px: f64, row_change: bool) -> bool {
    !opens_gesture(travel_px, row_change)
}

/// F2.8 S2 ("one Undo per gesture"): push only on the first real move of a hold.
pub fn pushes_undo(moved: bool, already_pushed: bool) -> bool {
    moved && !already_pushed
}

/// F2.8 S2 (the shift correction): the map after `sources` slid by `d`. A source not in the map was at 0. Read from the gesture's OPENING map: asking the live one each mouse-move would compound the nudges, and this makes one Undo enough.
pub fn slide_shift(
    from: &BTreeMap<String, f64>,
    sources: &[String],
    d: f64,
) -> BTreeMap<String, f64> {
    let mut shifted = from.clone();
    for source in sources {
        let start = shifted.get(source).copied().unwrap_or(0.0);
        shifted.insert(source.clone(), start + d);
    }
    shifted
}

/// F2.8 S2 ("folds near the grab open for the drag, refold after"): indices into `folds` the gesture travels through: from the press's column to where the drag has got to. A fold elsewhere is untouched.
pub fn folds_to_open(folds: &[[f64; 2]], grab: f64, travel: f64) -> Vec<usize> {
    let (from, to) = if travel >= 0.0 {
        (grab, grab + travel)
    } else {
        (grab + travel, grab)
    };
    folds
        .iter()
        .enumerate()
        .filter(|(_, [start, end])| *end >= from && *start <= to)
        .map(|(index, _)| index)
        .collect()
}

/// F2.8 S1: the tooltip saying a border is grabbable; the reach is [`EDGE_GRAB_PX`] (`// layout.edgeGrabPx`).
pub const TRIM_TIP: &str =
    "press within 6 px of a clip border to drag that border \u{2014} either button";

pub const MOVE_TIP: &str =
    "right-drag moves what is under the pointer \u{2014} a recording, the selected scenes, one scene along its recording, or the whole row";

/// F2.8 S2/S3: the whole right-press gesture answered at once: nothing moved (a click, S3) or something slid (S2) with the corrected shift map, any row change and the folds to open.
#[derive(Debug, Clone, PartialEq)]
pub enum Gesture {
    Click,
    /// S2: something slid. `what` names it, `status` IS the sentence, `shift` is the corrected map, `row` the new row for a row change, `open` the folds to open.
    Slid {
        what: String,
        status: String,
        shift: BTreeMap<String, f64>,
        row: Option<usize>,
        open: Vec<usize>,
        /// F2.10: WHICH recording this drag took, if one; the page pins that one only (`cut_cam::pin_for_drag`) so an overlap gets its own row. `None` for scenes or a whole row.
        dragged: Option<String>,
        /// The scenes after the slide: a scene or selection slides along its recording by moving its own start and end. Unchanged for a recording or a row (they slide the shift).
        segs: Vec<Seg>,
    },
}

/// What the page calls each thing the right button can take, in its status sentences' words.
fn slide_noun(slide: &Slide) -> &'static str {
    match slide {
        Slide::Recording(_) => "the recording",
        Slide::Selection(_) => "the selected scenes",
        Slide::Scene(_) => "the scene",
        Slide::Row(_) => "the camera row",
    }
}

/// F2.8 S2/S3: one right gesture on one cut, answered whole. `d_seconds` is the hand's travel; `travel_px` and `row_change` say whether it counts as a drag ([`opens_gesture`]). `shift_at_open` and `already_pushed` come from the press's start, else each update compounds ([`slide_shift`]). A gesture that changed nothing answers [`Gesture::Click`] even if it travelled (a pinned scene has nowhere to go).
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
    if !opens_gesture(travel_px, row_change) {
        return Gesture::Click;
    }

    let slide = slide_for(press);
    let grab = match &slide {
        Slide::Scene(index) => segs.get(*index).map(|seg| seg.s).unwrap_or(0.0),
        _ => press.selection.map(|(start, _)| start).unwrap_or(0.0),
    };

    let mut shift = shift_at_open.clone();
    let mut slid = segs.to_vec();
    let mut moved = 0.0_f64;
    match &slide {
        Slide::Recording(base) => {
            shift = slide_shift(&shift, std::slice::from_ref(base), d_seconds);
            moved = d_seconds;
        }
        Slide::Selection(scenes) => {
            // The marked scenes move together by ONE amount; each is bounded by the neighbours that stay put and the shortest landing any allows is taken by all.
            let staying: Vec<usize> = (0..segs.len()).filter(|i| !scenes.contains(i)).collect();
            let mut together: Option<f64> = None;
            for scene in scenes {
                let Some(seg) = segs.get(*scene) else {
                    continue;
                };
                let mut fence: Vec<Seg> = staying.iter().map(|i| segs[*i].clone()).collect();
                fence.push(seg.clone());
                let own = fence.len() - 1;
                let delta =
                    slide_scene(&fence, own, seg.s + d_seconds, rec_start, rec_end, pps) - seg.s;
                together = Some(match together {
                    Some(held) if held.abs() <= delta.abs() => held,
                    _ => delta,
                });
            }
            moved = together.unwrap_or(0.0);
            for scene in scenes {
                if let Some(seg) = slid.get_mut(*scene) {
                    seg.s += moved;
                    seg.e += moved;
                }
            }
        }
        Slide::Scene(index) => {
            if let Some(seg) = segs.get(*index) {
                let landed = slide_scene(segs, *index, seg.s + d_seconds, rec_start, rec_end, pps);
                moved = landed - seg.s;
                slid[*index].s = landed;
                slid[*index].e = landed + (seg.e - seg.s);
            }
        }
        Slide::Row(_) => {
            shift = slide_shift(shift_at_open, sources, d_seconds);
            moved = d_seconds;
        }
    }

    if !pushes_undo(moved != 0.0, false) && !(row_change && moved == 0.0) {
        return Gesture::Click;
    }

    let what = slide_noun(&slide).to_string();
    let new_row = match (&slide, row_change) {
        (Slide::Scene(_), true) | (Slide::Recording(_), true) => Some(press.row),
        _ => None,
    };
    let status = match new_row {
        Some(row) => row_status(&what, row),
        None => shift_label(&what, moved),
    };

    let open = folds_to_open(folds, grab, d_seconds);

    // A repeat update of an already-pushed hold only decides click-vs-slid on a no-op.
    if already_pushed && moved == 0.0 && new_row.is_none() {
        return Gesture::Click;
    }

    Gesture::Slid {
        what,
        status,
        shift,
        row: new_row,
        open,
        // Only a press that took ONE recording can pin one.
        dragged: match &slide {
            Slide::Recording(base) => Some(base.clone()),
            _ => None,
        },
        segs: slid,
    }
}

/// F2.8 S2: the status for a slide: signed seconds to two decimals (so a shift is repeatable for the same two devices); zero says so.
pub fn shift_label(what: &str, d: f64) -> String {
    if d == 0.0 {
        return format!("{what} is back where it started");
    }
    format!("{what} moved {d:+.2} s")
}

/// F2.8 S2: what a row change says. The sentence counts rows from 1, `Cut::rows` from 0.
pub fn row_status(what: &str, row: usize) -> String {
    format!(
        "{what} moved to row {} \u{2014} its kept scenes came along",
        row + 1
    )
}

/// F2.8 S1: the strip's words when the cut is empty (a constant so a test can pin them).
pub const STRIP_EMPTY_HINT: &str = "nothing cut yet — the kept stretches show up here";

/// F2.8 S1: strip row heights in px: ruler on top, kept bar the rest, shared by painter and size request.
pub const RULER_H: f64 = 16.0;
pub const BAR_H: f64 = 32.0;

/// F2.8 S1: where each clip border is drawn, px from the strip's origin. Shared with the press side so a grabbed border is the seen pixel. An insert has no borders (`spec/05-cut.md` rule 8).
pub fn border_positions(boxes: &[Box_]) -> Vec<(f64, usize, Border)> {
    boxes
        .iter()
        .filter(|b| !b.insert)
        .flat_map(|b| {
            [
                (b.x, b.index, Border::Start),
                (b.x + b.w, b.index, Border::End),
            ]
        })
        .collect()
}

/// F2.8 S1: which border of which box a press x takes, over the whole layout: [`grab_border`] on every box, nearest wins, inserts skipped.
pub fn border_at(boxes: &[Box_], px: f64) -> Option<(usize, Border)> {
    let mut best: Option<(f64, usize, Border)> = None;
    for (edge, index, border) in border_positions(boxes) {
        let d = (px - edge).abs();
        if d <= EDGE_GRAB_PX && best.is_none_or(|(bd, _, _)| d < bd) {
            best = Some((d, index, border));
        }
    }
    best.map(|(_, index, border)| (index, border))
}

/// F2.8: one box on the placeholder strip: position and width in px, with the flags the painter needs (inserts are files, never trimmed, `spec/05-cut.md` rule 8) and `index` for `press_trim_border`-style seams.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Box_ {
    pub x: f64,
    /// Width in px, never negative.
    pub w: f64,
    pub index: usize,
    /// True for an insert (`Seg::is_insert`): violet, outside every trim reach (rule 8).
    pub insert: bool,
}

/// F2.8 S1: the boxes the kept-bar band draws, one per segment at `pps`, held here so a test and the press side share the mapping. Inserts come back flagged, not skipped.
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

/// F2.8 S1: which box a press falls on and which border the pointer took: nearest wins within [`EDGE_GRAB_PX`] (as [`grab_border`], `// layout.edgeGrabPx`); inside but away from both borders is the box with `None`; inserts answer `None` everywhere (rule 8).
pub fn box_at(boxes: &[Box_], px: f64) -> Option<(usize, Option<Border>)> {
    let mut best: Option<(f64, usize, Border)> = None;
    let mut trimmable = boxes.iter().filter(|b| !b.insert);

    for b in trimmable.clone() {
        for (edge, border) in [(b.x, Border::Start), (b.x + b.w, Border::End)] {
            let d = (px - edge).abs();
            if d <= EDGE_GRAB_PX && best.is_none_or(|(bd, _, _)| d < bd) {
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

/// F2.8 S1: the ruler row: a tick every `every_seconds` up to `rec_end`, labelled as `spec/img/05-trim.png` prints them (`0:00`, `0:30`, `1:00`), without the tenth [`preview::clock`] carries. A non-positive span, zoom or spacing answers with no ticks.
pub fn ruler_ticks(rec_end: f64, pps: f64, every_seconds: f64) -> Vec<(f64, String)> {
    if rec_end <= 0.0 || pps <= 0.0 || every_seconds <= 0.0 {
        return Vec::new();
    }
    let sub_second = every_seconds < 1.0;
    let mut ticks = Vec::new();
    let mut n = 0u64;
    loop {
        let t = n as f64 * every_seconds;
        if t > rec_end + 1e-9 {
            break;
        }
        let label = if sub_second {
            let tenths = (t * 10.0).round() as u64;
            format!("{}.{}", tools::mm_ss((tenths / 10) as f64), tenths % 10)
        } else {
            tools::mm_ss(t)
        };
        ticks.push((t * pps, label));
        n += 1;
    }
    ticks
}
