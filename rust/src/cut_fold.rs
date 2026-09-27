//! F2.11 (Folds and rows) — `spec/05-cut.md` F2.11, with `spec/inventory/cut.md` §B normative for what a fold
//! is and rule 11 for what an empty row is.
//!
//! A fold takes a stretch the CUT DROPS out of the way: it stays there, still selectable, still measured, and
//! simply asks for no width while the person is not looking at it. That arithmetic — runs merged, cells cut at
//! folded gaps, a folded cell drawn widthless, x↔time walking around a seam — is [`crate::timeline`]'s already;
//! this module owns what is left: which stretches may be folded at all, what a press does to one, where its
//! badge sits, and the two ✕ that take rows away.
//!
//! Two rules hold the whole thing honest (`spec/inventory/cut.md` §B): a fold is a VIEW, never an edit — so
//! nothing here touches [`crate::cut::History`] and structurally cannot open an undo step — and it is never used
//! to measure a drag, which is why every function below returns positions and seconds and none of them takes a
//! fold as a distance. Being a view does not mean being unsaved: `folds` goes into `cut.json` on every toggle
//! and comes back on open, because where someone was working is worth more than a clean file.
//!
//! No rule lives in the widget layer: `rust/src/ui/window.rs` draws the − / + badges, the gutter's fold-all
//! badge and the two ✕ as NAMED widgets (`fold-button-<i>`, `fold-all-button`, `row-cross-<n>`,
//! `lane-cross-<name>`), each forwarding to a function here and printing the sentence that comes back. See
//! `ui::refresh_fold_badges` for the drawing half.

use crate::cut::{Cut, Seg};
use crate::cut_hear;
use crate::timeline::{self, Recording, Span};

// --- the three numbers §D lists and §10 has no row for ----------------------------------------------------

/// A gap with no room to draw the − in is not offered one (`spec/inventory/cut.md` §D's `foldMin 20`;
/// `spec/10-parameters.md` has no row for it, so it lives here — as F2.8's `SCRUB_MS` does — in the module whose
/// rule reads it). The badge would be wider than the thing it folds and would sit on both its neighbours.
pub const FOLD_MIN_PX: f64 = 20.0;

/// How far inside the neighbouring bar a gap's badge sits when the gap has no middle of its own — the tape's own
/// head and tail (§D's `killIn 16`, no §10 row), and never further in than that bar's own middle.
pub const BADGE_INSIDE_PX: f64 = 16.0;

/// How close to either end of the tape a badge may come: its radius plus its pad (§D's `segKill radius 4 pad 3`,
/// no §10 row). A badge half off the page is a press that does nothing.
pub const BADGE_PLATE_PX: f64 = 7.0;

// --- S1: which stretches may be folded ---------------------------------------------------------------------

/// F2.11 S1 (`Folds cover stretches THE CUT DROPS — holes between kept clips; each filmed run's head and tail`):
/// every stretch of filmed time that no kept clip covers, in time order.
///
/// An insert (`ins` non-empty) is skipped: a card, a still or an overlaid sound brings its own picture and drops
/// no footage, so the seconds under it were never given up and have nothing to fold away.
///
/// Nothing outside a filmed run appears here, which is the other half of the step: unfilmed time is never laid
/// out, so folding it would be a control over a stretch the page does not draw.
pub fn dropped_gaps(runs: &[(f64, f64)], segs: &[Seg]) -> Vec<(f64, f64)> {
    let mut gaps = Vec::new();
    for (from, to) in runs {
        // The kept clips lying inside this run, in time order — the cut's own order is not guaranteed.
        let mut clips: Vec<(f64, f64)> = segs
            .iter()
            .filter(|seg| seg.ins.is_empty() && seg.e > *from && seg.s < *to)
            .map(|seg| (seg.s.max(*from), seg.e.min(*to)))
            .collect();
        clips.sort_by(|a, b| a.0.total_cmp(&b.0));

        // The run's head, every hole between neighbours, and its tail: one walk with a cursor for what is covered.
        let mut at = *from;
        for (start, end) in clips {
            if start > at {
                gaps.push((at, start));
            }
            at = at.max(end);
        }
        if at < *to {
            gaps.push((at, *to));
        }
    }
    gaps.sort_by(|a, b| a.0.total_cmp(&b.0));
    gaps
}

/// F2.11 S1: is this stretch folded? A stored fold that OVERLAPS it, not one whose ends match.
///
/// Trimming a clip moves the edge of the gap beside it and splitting one moves a gap's whole middle; a fold
/// matched by its ends would be lost to both. It is the gap between these two clips that is folded, and overlap
/// is how that survives the cut being worked on (`spec/inventory/cut.md` §B: "matched to gaps by overlap").
pub fn is_folded(folds: &[[f64; 2]], gap: (f64, f64)) -> bool {
    folds.iter().any(|fold| fold[0] < gap.1 && fold[1] > gap.0)
}

/// F2.11 S1 (`never over a whole recording`): is this stretch a filmed run with nothing kept in it — the tape
/// reduced to one recording nobody cut? Such a gap is not a fold worth saving: folding it would hide a whole
/// recording, which has its own ways off the page (§B lists that prohibition beside "a view, not an edit").
///
/// Covering a run is not enough. A run's own head and tail gaps cover their run by definition — that is what
/// "head" and "tail" mean — so a rule about ends would refuse to fold everything around the first clip of every
/// cut. What makes a recording whole is that nothing was kept out of it, which is what `segs` says.
pub fn whole_run(gap: (f64, f64), runs: &[(f64, f64)], segs: &[Seg]) -> bool {
    runs.iter().any(|(from, to)| {
        let covers = gap.0 <= from + 0.01 && gap.1 >= to - 0.01;
        let kept_here = segs.iter().any(|seg| seg.ins.is_empty() && seg.e > *from && seg.s < *to);
        covers && !kept_here
    })
}

// --- S1: the toggles ---------------------------------------------------------------------------------------

/// F2.11 S1 (`− in such a gap folds it to a seam; + on the seam unfolds`): press one badge and the stretch is
/// folded or opened again, and this returns what the page says.
///
/// A fold is a VIEW, so this takes no [`crate::cut::History`] — structurally it cannot open an undo step, which
/// is §B's rule rather than a convention to remember. The caller writes `cut.json` after it: a fold IS saved, on
/// every toggle and read back on open, because where the person was working survives a restart while an undone
/// edit would not have needed saving.
pub fn toggle_fold(cut: &mut Cut, gap: (f64, f64)) -> String {
    if is_folded(&cut.folds, gap) {
        // Drop every fold overlapping the gap: one press cannot be expected to leave a second seam behind.
        cut.folds.retain(|fold| !(fold[0] < gap.1 && fold[1] > gap.0));
        return unfold_status(gap);
    }
    cut.folds.push([gap.0, gap.1]);
    fold_status(gap)
}

/// F2.11 S1: the status a − leaves — the stretch, and how long it is.
///
/// One number, where the prototype sometimes writes two (`(X s, Y s in the video)`): its second figure appears
/// when the folded stretch spans time the cut already dropped, which no single dropped gap can do — a gap lies
/// inside one filmed run by construction, so its seconds ARE the video's. Said here because the difference looks
/// like an omission.
pub fn fold_status(gap: (f64, f64)) -> String {
    span_status("folded", gap)
}

/// F2.11 S1: the same sentence for a + — and not [`crate::preview`]'s "unfolded m:ss — ▶ ran into it", which is
/// what a fold that ▶ walked into says instead of one the person opened.
pub fn unfold_status(gap: (f64, f64)) -> String {
    span_status("unfolded", gap)
}

fn span_status(verb: &str, gap: (f64, f64)) -> String {
    format!(
        "{verb} {} \u{2013} {} ({:.1} s)",
        cut_hear::scene_clock(gap.0),
        cut_hear::scene_clock(gap.1),
        gap.1 - gap.0
    )
}

/// F2.11 S1 (`the gutter badge folds/unfolds all`): one press for every gap, because the gutter's badge wears the
/// same − and + a single gap's badge wears — it is the same verb over all of them. `runs` is the filmed runs the
/// gaps were cut from, needed to tell a hole from a recording nobody cut.
///
/// Folded already means unfold: any fold at all makes the switch read as ON, so a page someone folded by hand
/// still has one press that clears it. Folding skips a gap that covers a whole recording and counts only the
/// folds actually stored, which is why `n` is read back from `cut.folds` rather than from the gaps handed in —
/// the prototype re-reads its own list for the same reason.
pub fn fold_all(cut: &mut Cut, gaps: &[(f64, f64)], runs: &[(f64, f64)], segs: &[Seg]) -> String {
    if !cut.folds.is_empty() {
        let n = cut.folds.len();
        cut.folds.clear();
        return seams_status("unfolded", n);
    }
    for gap in gaps {
        if !whole_run(*gap, runs, segs) {
            cut.folds.push([gap.0, gap.1]);
        }
    }
    seams_status("folded", cut.folds.len())
}

fn seams_status(verb: &str, n: usize) -> String {
    let word = if n == 1 { "seam" } else { "seams" };
    format!("{verb} {n} {word}")
}

// --- S1: where the badges go -------------------------------------------------------------------------------

/// One badge: the stretch it folds, where it is drawn along the tape, and whether it is a − (unfolded gap) or a
/// + (folded one).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Badge {
    pub gap: (f64, f64),
    pub x: f64,
    pub folded: bool,
}

/// F2.11 S1: the badges to draw, with their x already resolved against a laid-out [`Span`] — so a fold's zero
/// width is what decides where its seam is, rather than a second arithmetic of pixels kept here. `folds` is the
/// cut's own list, which is what says whether a gap wears a − or a +.
///
/// A folded gap always gets its badge: its + sits on the seam, and a folded cell has no width to run out of. An
/// unfolded gap narrower than [`FOLD_MIN_PX`] gets none — the − would be wider than the gap it folds. Everything
/// else sits mid-gap, because a gap's two ends are grips and its middle is the one part of it a press can only
/// mean one thing on; except the gaps that open and close the tape, whose far end is the edge of the page rather
/// than another bar, and which therefore go inside the neighbouring bar ([`BADGE_INSIDE_PX`], no further in than
/// its middle). Every x is clamped to [`BADGE_PLATE_PX`] from either end.
pub fn badges(folds: &[[f64; 2]], runs: &[(f64, f64)], segs: &[Seg], span: &Span) -> Vec<Badge> {
    let bars: Vec<(f64, f64)> = segs.iter().filter(|seg| seg.ins.is_empty()).map(|seg| (seg.s, seg.e)).collect();
    let mut out = Vec::new();
    for gap in dropped_gaps(runs, segs) {
        let folded = is_folded(folds, gap);
        // A fold is never used to measure a drag (§B), so this answers where a badge is and stops there.
        if folded {
            // The seam: the x the tape resumes at after the fold. `x_of` answers that for the fold's own start,
            // because every second of a folded cell shares it; clamped so the + stays pressable at the front edge.
            let seam = span.x_of(gap.0).clamp(BADGE_PLATE_PX, (span.total_px() - BADGE_PLATE_PX).max(BADGE_PLATE_PX));
            out.push(Badge { gap, x: seam, folded });
            continue;
        }
        let (x0, x1) = (span.x_of(gap.0), span.x_of(gap.1));
        if x1 - x0 < FOLD_MIN_PX {
            continue;
        }
        // Mid-gap in SECONDS, then drawn: the middle of what is between two clips is the one place a press can
        // only mean this gap, and asking for the middle of their pixels instead would shift the badge with every
        // fold elsewhere in the run.
        // Mid-gap in SECONDS, then drawn: the middle of what lies between two clips is the one place a press can
        // only mean this gap, and asking for the middle of their pixels instead would shift the badge with every
        // fold elsewhere in the run.
        let mut x = span.x_of((gap.0 + gap.1) / 2.0);
        // The tape's own head or tail go inside the bar beside them instead: their far end is the edge of the page,
        // not another clip's grip, so mid-gap would sit over a bar or off the tape. A reach in, and never past that
        // bar's middle, where it would cover the grip at its far end.
        if let Some(bar) = opening_bar(&bars, gap) {
            // A reach inside the clip's own seconds — or its middle, whichever is nearer its start, so a badge
            // never ends up past the grip at the other end of a short clip.
            let step = inside_seconds(bar, span.pps);
            x = span.x_of(bar.0 + step);
        } else if let Some(bar) = closing_bar(&bars, gap) {
            let step = inside_seconds(bar, span.pps);
            x = span.x_of(bar.1 - step);
        }
        x = x.clamp(BADGE_PLATE_PX, (span.total_px() - BADGE_PLATE_PX).max(BADGE_PLATE_PX));
        out.push(Badge { gap, x, folded });
    }
    out
}

/// How far into a neighbouring bar a badge steps: [`BADGE_INSIDE_PX`] of pixels converted to that clip's seconds,
/// or its own middle — whichever is nearer its start, so a badge never ends up past the grip at the other end of a
/// short clip.
fn inside_seconds(bar: (f64, f64), pps: f64) -> f64 {
    (BADGE_INSIDE_PX / pps).min((bar.1 - bar.0) / 2.0)
}

/// Does this gap open the tape — is there no kept clip before it, so its badge would hang over the bar that starts
/// the page? Asked of the footage rather than of which side has more: an interior hole also has less beside it than
/// the rest of the cut, and would be moved off its own middle by a rule about size.
fn opening_bar(bars: &[(f64, f64)], gap: (f64, f64)) -> Option<(f64, f64)> {
    let first = bars.iter().map(|bar| bar.0).fold(f64::INFINITY, f64::min);
    (gap.1 <= first + 0.01).then(|| bars.iter().find(|bar| (bar.0 - gap.1).abs() < 0.01)).flatten().copied()
}

/// …or closes it — is there no kept clip after it, so its badge would hang over the bar that ends the page?
fn closing_bar(bars: &[(f64, f64)], gap: (f64, f64)) -> Option<(f64, f64)> {
    let last = bars.iter().map(|bar| bar.1).fold(f64::NEG_INFINITY, f64::max);
    (gap.0 >= last - 0.01).then(|| bars.iter().find(|bar| (bar.1 - gap.0).abs() < 0.01)).flatten().copied()
}

// --- S2: rows ----------------------------------------------------------------------------------------------

/// F2.11 S2: is this row empty? Nothing was coloured on it and no kept scene is shown from it — a scene still
/// drawn there means the row is not empty whatever the colouring says, since the row is where its pictures are.
///
/// `recordings`/`rows` are the pair [`crate::timeline::rows_for`] produced, zipped: a recording sitting on the row
/// makes it a row of that camera whether or not anything was cut from it yet, which is why "empty" is asked of the
/// layout and not of the scenes alone.
pub fn row_is_empty(row: usize, recordings: &[Recording], rows: &[usize], segs: &[Seg]) -> bool {
    let coloured = recordings.iter().zip(rows).any(|(_, placed)| *placed == row);
    let shown = segs.iter().any(|seg| seg.ins.is_empty() && seg.cam as usize == row);
    !coloured && !shown
}

/// F2.11 S2 (`An emptied bottom row stays until its ✕`): take the row away — the rows above it come down one and
/// every scene keeps the camera it was showing, so no `cam` is touched.
///
/// Only an EMPTY row has this way off the band: a row with footage has its own exits (a cut lane's ✕, Prepare),
/// each of which says what happens to the footage, and this deliberately has no answer for that. And only while
/// more than one row is drawn — a page with no rows is not a page, so the last row survives however empty.
pub fn kill_row(cut: &mut Cut, row: usize, recordings: &[Recording], rows: &[usize]) -> Option<String> {
    let drawn = timeline::row_count(recordings, cut);
    // The ✕ is drawn over an empty row the page shows, so a row number nothing is drawn at has none over it — and
    // taking one away would spend a floor that was never holding anything.
    if drawn <= 1 || row >= drawn || !row_is_empty(row, recordings, rows, &cut.segs) {
        return None;
    }
    // The pins above the row come down with it; a pin below it names a row that did not move.
    for placed in cut.rows.values_mut() {
        if *placed > row as i32 {
            *placed -= 1;
        }
    }
    // The floor is what was holding the emptied row, so taking the row away spends it.
    cut.nrows = (drawn - 1) as i32;
    Some(format!("removed the empty row {}", row + 1))
}

/// F2.11 S2 (`a cut lane's ✕ removes the lane, its pins, shift, pictures and sound`).
///
/// `quiet` lists naming the lane are LEFT alone: a lane that no longer exists silences nothing, so rewriting
/// saved scenes to remove a word that changes nothing heard would move bytes the autosave compares (§2's
/// byte-stability rule) and make an unchanged cut look edited. An unknown name says the same sentence and changes
/// nothing — the ✕ was pressed over a lane that is already gone.
pub fn remove_lane(cut: &mut Cut, name: &str) -> String {
    let sources: Vec<String> = cut.lanes.iter().filter(|lane| lane.name == name).map(|lane| lane.src.clone()).collect();
    cut.lanes.retain(|lane| lane.name != name);
    cut.rows.remove(name);
    cut.shift.remove(name);
    cut.segs.retain(|seg| {
        let its_sound = seg.lane == name;
        let its_picture = sources.iter().any(|src| !seg.ins.is_empty() && &seg.ins == src);
        !(its_sound || its_picture)
    });
    format!("removed the {name} lane")
}

/// F2.11 S2 (`spec/inventory/cut.md` §B rule 11: "nRows holds an emptied bottom row"): how many rows the page
/// draws, which is what keeps a row nobody is on any more until its ✕ spends the floor.
pub fn bottom_row_survives(recordings: &[Recording], cut: &Cut) -> usize {
    timeline::row_count(recordings, cut)
}
