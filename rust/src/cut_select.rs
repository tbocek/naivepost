//! F2.6 (Select) — `spec/05-cut.md` F2.6.

use crate::cut::{Cut, Lane};

/// F2.6 S1: the tooltip on the drag area, naming what a press there does before a person tries it.
pub const SURFACE_TIP: &str =
    "drag across the tracks to select \u{2014} the band selects what it was drawn on";

/// F2.6 S2: the cross's label. A bare ✕ says nothing about what pressing it costs; this says clear.
pub const CLEAR_LABEL: &str = "\u{2715} Clear selection";

/// F2.6 S2: the cross's tooltip, spelling out that only the selection goes, never the cut.
pub const CLEAR_TIP: &str =
    "clear the selection \u{2014} nothing is removed, the cut keeps every clip";

/// F2.6 S3: what the Selection readout shows with no band -- the same unclicked face the clock uses
/// (§D's `--:--.-`), so an empty field never reads as zero-length rather than absent.
pub const READOUT_NONE: &str = "Selection: --:--.- \u{2013} --:--.-";

/// F2.6 S3: the readout's prefix. Kept once here so the label and any later copy agree on the word.
pub const READOUT_PREFIX: &str = "Selection:";

/// F2.6 S1: the ground a left-drag started on — every track area no control claimed: a picture row, a
/// wave strip, a lane, the ruler, the empty part of the selection band, or the effects lane (which is the
/// only one that refuses).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Surface {
    /// One row of thumbnails: that row's footage.
    PictureRow(usize),
    /// The wave strip under a row: that row's camera's own sound.
    WaveStrip,
    /// A separate recording's lane: that one recording's sound.
    Lane,
    /// The ruler: ground that belongs to no row.
    Ruler,
    /// The empty part of the selection band, where a drag starts a new band.
    SelectionBand,
    /// The effects lane: refuses a selection.
    FxLane,
}

/// F2.6 S1 (`Selection scope is the lane/row it was drawn on`, and §0's rule 5, "A selection is of what it
/// was drawn on"): what the seconds are seconds OF. The span is session time either way; what the verbs
/// make of it is not — footage for Add/Split/Remove, sound for Copy and Insert.
#[derive(Debug, Clone, PartialEq)]
pub enum Scope {
    /// Footage, and which camera row's if the drag said.
    Footage { row: usize },
    /// One recording's sound, named by its base — the same name a scene's `quiet` list uses.
    Sound { recording: String },
}

pub const ANY_ROW: usize = usize::MAX;

/// F2.6 S1: the band a drag drew — session seconds, always `start < end`, plus what it is of.
#[derive(Debug, Clone, PartialEq)]
pub struct Selection {
    pub start: f64,
    pub end: f64,
    pub scope: Scope,
}

impl Selection {
    /// The seconds the band holds. Length, not `end - start` at the call site, so a resize that clamps has
    /// one number to test against.
    pub fn length(&self) -> f64 {
        self.end - self.start
    }
}

/// F2.6 S1: what one left-drag from `from` to `to` on `surface` selects, or nothing at all.
pub fn draw(surface: Surface, recording: Option<&str>, from: f64, to: f64) -> Option<Selection> {
    let scope = match surface {
        Surface::PictureRow(row) => Scope::Footage { row },
        Surface::Ruler | Surface::SelectionBand => Scope::Footage { row: ANY_ROW },
        Surface::WaveStrip | Surface::Lane => Scope::Sound {
            recording: recording?.to_string(),
        },
        Surface::FxLane => return None,
    };
    let (start, end) = if from <= to { (from, to) } else { (to, from) };
    Some(Selection { start, end, scope })
}

/// F2.6 S1: what a press on the effects lane does instead of selecting — the only ground that refuses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FxLanePress {
    /// A held effect is put down here.
    PutsHeldEffectDown,
    /// Nothing was in hand, so nothing happens.
    Nothing,
}

/// F2.6 S1 ("Only the effects lane refuses: a press on empty lane puts a held effect down instead"): the
/// lane answers no selection either way; what changes is whether the effect in the hand stays there.
pub fn fx_lane_press(held_effect: bool) -> FxLanePress {
    if held_effect {
        FxLanePress::PutsHeldEffectDown
    } else {
        FxLanePress::Nothing
    }
}

/// F2.6 S1: how far the pointer has to move before a press is a drag — spec/10-parameters.md's "drag slop 4".
pub const DRAG_SLOP_PX: f64 = 4.0;

/// F2.6 S1: did this press move enough to be a drag? Below the slop it is a click, and a click clears the
/// selection and moves the red line (F2.4 S1) rather than drawing a new band — which is why the slop is
/// asked before anything is drawn and not after.
pub fn is_drag(moved_px: f64) -> bool {
    moved_px > DRAG_SLOP_PX
}

pub const MIN_SECONDS: f64 = 0.04;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SurfaceBands {
    /// y range of the ruler — ground that belongs to no row.
    pub ruler: (f64, f64),
    /// y range of the empty selection band, where a drag starts a new band.
    pub selection: (f64, f64),
    /// y range of the effects lane — the only ground that refuses a selection.
    pub fx: (f64, f64),
    /// y where the first picture row begins.
    pub first_picture: f64,
    /// Height of one row's picture band.
    pub row_height: f64,
    /// Height of the wave strip under a row.
    pub wave_height: f64,
}

/// F2.6 S1: the placeholder bands for the surface drawn today. A 48 px area split ruler 8 / one picture
/// row 20 + its wave strip 12 / effects lane 8, so all six surfaces are reachable by y on the box the
/// page actually shows. Chosen to fill the drawn height exactly; see [`SurfaceBands`]' placeholder note.
pub const PLACEHOLDER_BANDS: SurfaceBands = SurfaceBands {
    ruler: (0.0, 8.0),
    selection: (8.0, 8.0),
    fx: (40.0, 48.0),
    first_picture: 8.0,
    row_height: 20.0,
    wave_height: 12.0,
};

pub fn surface_at(y: f64, bands: &SurfaceBands, row_count: usize) -> Surface {
    if y >= bands.ruler.0 && y < bands.ruler.1 {
        return Surface::Ruler;
    }
    if y >= bands.fx.0 && y < bands.fx.1 {
        return Surface::FxLane;
    }
    if y >= bands.selection.0 && y < bands.selection.1 {
        return Surface::SelectionBand;
    }
    let rows_bottom =
        bands.first_picture + row_count.max(1) as f64 * (bands.row_height + bands.wave_height);
    if y >= rows_bottom.max(bands.fx.1) {
        return Surface::SelectionBand;
    }
    if y >= bands.first_picture {
        // Each row occupies picture then wave; the halves are decided by where y sits inside the pair.
        let pitch = bands.row_height + bands.wave_height;
        if pitch > 0.0 {
            let index = ((y - bands.first_picture) / pitch).floor();
            if index.is_finite() && index >= 0.0 {
                let idx = index as usize;
                if idx < row_count {
                    let within = y - bands.first_picture - index * pitch;
                    return if within < bands.row_height {
                        Surface::PictureRow(idx)
                    } else {
                        Surface::WaveStrip
                    };
                }
                // Past every drawn row: a separate recording's lane.
                return Surface::Lane;
            }
        }
        // Zero-height rows: nothing to land on but the band.
        return Surface::SelectionBand;
    }
    Surface::SelectionBand
}

pub fn keep_started_surface(started: Option<Surface>, now: Surface) -> Surface {
    started.unwrap_or(now)
}

/// F2.6 S2 (`✕ clears`): throw the band away. The answer is nothing, and the marks and the Selection
/// readout go with it — they are the band, not a copy of it ([`marks`]). A click on a track asks for the
/// same thing (F2.4's `ClickOutcome::clears_selection`).
pub fn clear(_: Option<Selection>) -> Option<Selection> {
    None
}

/// F2.6 S2: how far from an end a press still grabs that end. Pixels, because a grip is a reach for the
/// hand and not a length of film — the same reason [`SNAP_PX`] is pixels. spec/10-parameters.md lists it
/// among "grab reaches", whose shortest is 6 px.
pub const GRIP_PX: f64 = 6.0;

/// §6's third pixel reach (`spec/05-cut.md` §6: "pixel reaches (6, 8, 10, 12 px)", and `spec/inventory/cut.md`
/// §I's press order: "selection ends on the pictures within **10 px**"): how far from an end a press still
/// grabs that end WHEN THE PRESS IS ON A PICTURE ROW.
pub const PIC_GRIP_PX: f64 = 10.0;

pub fn grip_px(surface: Surface) -> f64 {
    match surface {
        Surface::PictureRow(_) => PIC_GRIP_PX,
        Surface::WaveStrip
        | Surface::Lane
        | Surface::Ruler
        | Surface::SelectionBand
        | Surface::FxLane => GRIP_PX,
    }
}

/// F2.6 S2: which part of the band a press lands on — what decides whether the drag resizes, moves or
/// starts something new.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    Start,
    End,
    Middle,
    Outside,
}

pub fn part_at(px: f64, from_px: f64, to_px: f64) -> Part {
    part_at_reach(px, from_px, to_px, GRIP_PX)
}

pub fn part_at_reach(px: f64, from_px: f64, to_px: f64, grip_px: f64) -> Part {
    if (px - from_px).abs() <= grip_px && px <= to_px {
        return Part::Start;
    }
    if (px - to_px).abs() <= grip_px && px >= from_px {
        return Part::End;
    }
    if px > from_px && px < to_px {
        Part::Middle
    } else {
        Part::Outside
    }
}

/// F2.6 S2 ("ends snap within 8 px to clip borders, recording ends, effect ends, the line"): how near a
/// landmark an end has to come before it is pulled onto it. Pixels rather than seconds so the pull is the
/// same distance for the hand at every zoom — [`snap`] turns it into seconds with the current `pps`.
pub const SNAP_PX: f64 = 8.0;

/// F2.6 S2: the landmarks a dragged end lands on — every border of the cut (what an end is nearly always
/// aimed at), every recording's start and end, the ends of every effect's band, and the red line last.
pub fn snap_marks(cut: &Cut, lanes: &[Lane], playhead: f64) -> Vec<f64> {
    let mut marks: Vec<f64> = Vec::new();
    for seg in &cut.segs {
        marks.push(seg.s);
        marks.push(seg.e);
    }
    for lane in lanes {
        marks.push(lane.at);
        if lane.dur > 0.0 {
            marks.push(lane.at + lane.dur);
        }
    }
    for effect in &cut.fx {
        let (from, to) = effect.spans();
        marks.push(from);
        marks.push(to);
    }
    marks.push(playhead);
    marks.sort_by(f64::total_cmp);
    // `dedup_by` hands its closure a mutable pair, so the distance is taken between copies.
    marks.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
    marks
}

/// F2.6 S2: pull `t` onto the nearest landmark within [`SNAP_PX`] pixels at `pps` px/s, or leave it where
/// the hand left it. `pps` is floored because a zoom of nought has no pixel to measure against.
pub fn snap(t: f64, marks: &[f64], pps: f64) -> f64 {
    let reach = SNAP_PX / pps.max(0.001);
    nearest_within(t, marks, reach).unwrap_or(t)
}

/// F2.6 S2 ([`snap_span`]'s rule for one second): the closest mark to `t` no further than `reach`, if any.
fn nearest_within(t: f64, marks: &[f64], reach: f64) -> Option<f64> {
    let mut best: Option<(f64, f64)> = None;
    for mark in marks {
        let distance = (t - mark).abs();
        if distance <= reach && best.is_none_or(|(_, near)| distance < near) {
            best = Some((*mark, distance));
        }
    }
    best.map(|(mark, _)| mark)
}

pub fn snap_span(from: f64, length: f64, marks: &[f64], pps: f64) -> f64 {
    let reach = SNAP_PX / pps.max(0.001);
    match (
        nearest_within(from, marks, reach),
        nearest_within(from + length, marks, reach),
    ) {
        // Neither end near anything: exactly where the hand left it.
        (None, None) => from,
        (Some(start), None) => start,
        (None, Some(end)) => end - length,
        (Some(start), Some(end)) => {
            if (from - start).abs() <= (from + length - end).abs() {
                start
            } else {
                end - length
            }
        }
    }
}

/// F2.6 S2 (`ends resize`): move one end of the band to `to`, snapped, and leave the other where it is.
pub fn resize(sel: &Selection, end: bool, to: f64, marks: &[f64], pps: f64) -> Selection {
    let moved = snap(to, marks, pps);
    // Clamped from the stationary end rather than towards it: an end dragged past its own still stops at
    // the shortest band there is instead of crossing over and inverting the selection.
    let (start, stop) = if end {
        (sel.start, moved.max(sel.start + MIN_SECONDS))
    } else {
        (moved.min(sel.end - MIN_SECONDS), sel.end)
    };
    Selection {
        start,
        end: stop,
        scope: sel.scope.clone(),
    }
}

/// F2.6 S2 (`middle moves`): slide the whole band so its start goes to `to`, snapped by [`snap_span`],
/// length untouched, and never off the front of the session.
pub fn move_band(sel: &Selection, to: f64, marks: &[f64], pps: f64) -> Selection {
    let length = sel.length();
    let start = snap_span(to, length, marks, pps).max(0.0);
    Selection {
        start,
        end: start + length,
        scope: sel.scope.clone(),
    }
}

pub fn marks(sel: Option<&Selection>) -> Option<(f64, f64)> {
    sel.map(|band| (band.start, band.end))
}

/// F2.6 S4 (`greys Add, Split, Remove · Copy and Insert aim at sound`): what ⧉ Insert puts in the selected
/// seconds — a card between the footage, or a sound over them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertAim {
    Footage,
    Sound,
}

/// F2.6 S4: which of the linked verbs are live and what ⧉ Insert aims at. A `false` is a greyed button, and
/// every grey one has a sentence behind it saying why ([`add_tip`] is the ＋ Add case; the rest belong to
/// the verb flows' own rounds).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Verbs {
    pub add: bool,
    pub split: bool,
    pub remove: bool,
    pub copy: bool,
    pub insert: InsertAim,
}

/// F2.6 S4 (`P.policy.minSceneSeconds`, spec/10-parameters.md: "shortest stretch worth suggesting, keeping
/// or copying"): the shortest region ＋ Add will keep and ⧉ Copy will take. Remove has no such floor — §0's
/// rule 13 says Remove takes exactly the selection, whose only bound is [`MIN_SECONDS`].
pub const MIN_SCENE_SECONDS: f64 = 1.0;

/// F2.6 S4: the verbs' state for the band there is now.
pub fn verbs(sel: Option<&Selection>, has_footage: bool) -> Verbs {
    let Some(band) = sel else {
        return Verbs {
            add: false,
            split: has_footage,
            remove: false,
            copy: false,
            insert: InsertAim::Footage,
        };
    };
    let long = band.length() >= MIN_SCENE_SECONDS;
    match band.scope {
        Scope::Sound { .. } => Verbs {
            add: false,
            split: false,
            remove: false,
            copy: long,
            insert: InsertAim::Sound,
        },
        Scope::Footage { .. } => Verbs {
            add: long,
            split: true,
            remove: true,
            copy: long,
            insert: InsertAim::Footage,
        },
    }
}
