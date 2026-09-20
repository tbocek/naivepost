//! §05-cut#2-model — the timeline's model: rows, runs, cells, folds and x↔time.
//!
//! The page draws these numbers and decides nothing (spec/00-principles.md §5): which row a recording
//! lands on, where footage is continuous, what a fold does to the picture band, and how a pixel meets
//! a second. spec/inventory/cut.md §B is normative for all four; the field names come from
//! spec/01-project-and-files.md §3's `cut.json`.

use crate::cut::Cut;
use crate::cut_screen;

/// One recording's span on the session clock. A cut lane is a recording too: its window of a file
/// starts at `at + off` (§3's `at` and `off`), which is why it needs no second type here.
#[derive(Debug, Clone, PartialEq)]
pub struct Recording {
    pub base: String,
    pub start: f64,
    pub end: f64,
}

/// Which row each recording is drawn on: greedy interval colouring in start order, with the file's
/// own pins applied first (spec/inventory/cut.md §B: "pins (cut.json rows) applied first").
///
/// A pin is taken literally — it may overlap what already sits on that row, because the editor moved
/// the recording there knowing what was under it (§3's `rows`). Everything else goes to the lowest row
/// it does not overlap, so two cameras recorded together cost exactly one row more than one camera.
/// Recordings with the same start keep the order they were handed in; a colouring that shuffled them
/// would move rows on every redraw.
pub fn rows_for(recordings: &[Recording], cut: &Cut) -> Vec<usize> {
    let mut rows = vec![0usize; recordings.len()];
    // What each row is busy with, as the spans already placed on it.
    let mut taken: Vec<Vec<(f64, f64)>> = Vec::new();

    // Order of decision: pinned first (they are fixed), then start order, ties by input order.
    let mut order: Vec<usize> = (0..recordings.len()).collect();
    order.sort_by(|a, b| {
        let pinned = |i: &usize| cut.rows.contains_key(&recordings[*i].base);
        pinned(b).cmp(&pinned(a)).then(
            recordings[*a]
                .start
                .partial_cmp(&recordings[*b].start)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.cmp(b)),
        )
    });

    for index in order {
        let recording = &recordings[index];
        // A pin names a row of its own accord (§3's `rows`), so it may overlap what is already there —
        // the editor moved this recording here knowing what was under it. A negative pin is not a row:
        // such a recording falls back to the greedy answer rather than indexing off the front.
        let pinned = cut.rows.get(&recording.base).filter(|row| **row >= 0).map(|row| *row as usize);
        let row = pinned.unwrap_or_else(|| first_free(&taken, recording.start, recording.end));
        if taken.len() <= row {
            taken.resize(row + 1, Vec::new());
        }
        taken[row].push((recording.start, recording.end));
        rows[index] = row;
    }
    rows
}

/// The lowest row with nothing overlapping `[start, end)` — a new one when every row is busy.
fn first_free(taken: &[Vec<(f64, f64)>], start: f64, end: f64) -> usize {
    taken
        .iter()
        .position(|busy| !busy.iter().any(|(from, to)| start < *to && end > *from))
        .unwrap_or(taken.len())
}

/// How many rows the page draws: what the colouring needed, through [`Cut::row_count`]'s `nrows` floor
/// — which is a floor and not a count, so an emptied bottom row survives until its ✕ (§B).
pub fn row_count(recordings: &[Recording], cut: &Cut) -> usize {
    let rows = rows_for(recordings, cut);
    let coloured = rows.iter().max().map(|row| row + 1).unwrap_or(0);
    cut.row_count(coloured)
}

/// The stretches with footage on them: merged recording spans (§B: "Filmed runs = merged recording
/// spans"). Two recordings that touch end to end are one run — the timeline has no seam inside
/// continuous footage, and a fold badge or a selection would otherwise straddle a join that is not one.
pub fn filmed_runs(recordings: &[Recording]) -> Vec<(f64, f64)> {
    let mut spans: Vec<(f64, f64)> = recordings
        .iter()
        .filter(|rec| rec.end > rec.start)
        .map(|rec| (rec.start, rec.end))
        .collect();
    spans.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    let mut runs: Vec<(f64, f64)> = Vec::new();
    for (start, end) in spans {
        match runs.last_mut() {
            // Touching counts as merged; a gap of anything over zero is a real gap.
            Some(last) if start <= last.1 => last.1 = last.1.max(end),
            _ => runs.push((start, end)),
        }
    }
    runs
}

/// One stretch of the picture band: a piece of a filmed run, or a folded gap that takes no width but
/// keeps its place in the order (that is what gives its + badge somewhere to sit).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cell {
    pub start: f64,
    pub end: f64,
    pub folded: bool,
}

/// The runs cut at the folded gaps (§B: "cells = runs cut at folded gaps"). A fold that covers a whole
/// run folds the whole run rather than leaving two zero-length cells either side of it.
pub fn cells(runs: &[(f64, f64)], folds: &[[f64; 2]]) -> Vec<Cell> {
    let mut folds: Vec<(f64, f64)> = folds.iter().map(|f| (f[0], f[1])).collect();
    folds.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    let mut out: Vec<Cell> = Vec::new();
    for run in runs {
        let (from, to) = *run;
        // `at` is the point the run has been cut up to so far; each fold that still lies ahead of it
        // cuts one more piece off. Held outside the iterator because a fold ends where it ends and the
        // loop has to stop seeing runs' worth of folds after it.
        let mut at = from;
        for (start, end) in folds.iter().copied() {
            if end <= at || start >= to {
                continue;
            }
            if start > at {
                out.push(Cell { start: at, end: start, folded: false });
            }
            out.push(Cell { start: at.max(start), end: end.min(to), folded: true });
            at = at.max(end);
        }
        if at < to {
            out.push(Cell { start: at, end: to, folded: false });
        }
    }
    out.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap_or(std::cmp::Ordering::Equal));
    out
}

/// The laid-out band: cells, the zoom, and the gutter they start after (§B: "spans carry pixel origins
/// (gutter first; a folded cell has zero width)").
#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    pub cells: Vec<Cell>,
    pub pps: f64,
    pub gutter_px: f64,
}

impl Span {
    pub fn new(cells: Vec<Cell>, pps: f64, gutter_px: f64) -> Self {
        Self { cells, pps, gutter_px }
    }

    /// A folded cell is widthless — that is the whole trick behind a fold: the time is still there and
    /// still selectable, it simply takes no room while it is out of the way.
    fn width(&self, cell: &Cell) -> f64 {
        if cell.folded {
            0.0
        } else {
            (cell.end - cell.start) * self.pps
        }
    }

    /// The band's whole width: the gutter plus every unfollowed stretch. Past this x there is nothing,
    /// which is what makes it the answer for a time past the end.
    pub fn total_px(&self) -> f64 {
        self.gutter_px + self.cells.iter().map(|cell| self.width(cell)).sum::<f64>()
    }

    /// Where a second is drawn (§B: "xOf walks spans"). Inside a folded stretch the answer is that
    /// stretch's seam — its start x — because every second of a fold shares the one pixel where the
    /// fold begins, and a playhead inside a fold should sit at the fold rather than vanish.
    pub fn x_of(&self, t: f64) -> f64 {
        let mut x = self.gutter_px;
        for cell in &self.cells {
            if cell.folded {
                if t >= cell.start && t <= cell.end {
                    return x;
                }
            } else if t < cell.end {
                // Half-open on the right, so a second at a cell's end belongs to the next cell and both
                // directions agree about which take a press on a seam is in.
                return x + (t - cell.start).max(0.0) * self.pps;
            }
            x += self.width(cell);
        }
        self.total_px()
    }

    /// What second an x is (§B: "tAt is the inverse, half-open on the right (the seam second belongs to
    /// the later take)"). A folded cell is passed over rather than answered: it has no pixels of its
    /// own, so a press there lands in the footage after the fold — which is the take the person can see.
    pub fn t_at(&self, x: f64) -> f64 {
        let mut cursor = self.gutter_px;
        for cell in &self.cells {
            if cell.folded {
                continue;
            }
            let width = self.width(cell);
            if x < cursor + width {
                return cell.start + (x - cursor).max(0.0) / self.pps;
            }
            cursor += width;
        }
        // Past the end: the last second there, and left of everything the first run's start — both
        // answers are real seconds, which a negative or an infinity would not be.
        self.cells
            .iter()
            .rev()
            .find(|cell| !cell.folded)
            .map(|cell| cell.end)
            .or_else(|| self.cells.iter().find(|cell| !cell.folded).map(|cell| cell.start))
            .unwrap_or(0.0)
    }
}

/// The zoom floor: the filmed length across the view (§2: "floor = fit the filmed length"). One number
/// from one module — [`cut_screen::fit_pps`] is where the rule lives, this only sums what it fits.
pub fn floor_pps(runs: &[(f64, f64)], view_px: f64) -> f64 {
    let filmed = runs.iter().map(|(start, end)| end - start).sum::<f64>();
    cut_screen::fit_pps(filmed, view_px)
}
