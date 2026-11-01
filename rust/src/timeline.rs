//! §05-cut#2-model: the timeline model: rows, runs, cells, folds and x↔time. The page draws these numbers and decides nothing (spec/00-principles.md §5); spec/inventory/cut.md §B is normative, field names come from spec/01-project-and-files.md §3's `cut.json`.

use crate::cut::Cut;
use crate::cut_screen;

/// One recording's span on the session clock. A cut lane is a recording too, its window starting at `at + off` (§3).
#[derive(Debug, Clone, PartialEq)]
pub struct Recording {
    pub base: String,
    pub start: f64,
    pub end: f64,
}

/// The footage this project keeps, as recordings: every kept (non-insert) segment of its own `cut.json`, one row per camera. Only maps; merging stays in [`filmed_runs`].
pub fn kept_footage_recordings(cut: &Cut) -> Vec<Recording> {
    cut.segs
        .iter()
        .filter(|seg| !seg.is_insert() && seg.e > seg.s)
        .map(|seg| Recording {
            base: format!("cam{}", seg.cam),
            start: seg.s,
            end: seg.e,
        })
        .collect()
}

/// Which row each recording is drawn on: greedy interval colouring in start order, pins applied first (spec/inventory/cut.md §B).
/// A pin is taken literally and may overlap. Everything else goes to the lowest free row. Ties keep input order, else rows move on every redraw.
pub fn rows_for(recordings: &[Recording], cut: &Cut) -> Vec<usize> {
    let mut rows = vec![0usize; recordings.len()];
    let mut taken: Vec<Vec<(f64, f64)>> = Vec::new();

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
        // A negative pin is not a row; that recording falls back to the greedy answer.
        let pinned = cut
            .rows
            .get(&recording.base)
            .filter(|row| **row >= 0)
            .map(|row| *row as usize);
        let row = pinned.unwrap_or_else(|| first_free(&taken, recording.start, recording.end));
        if taken.len() <= row {
            taken.resize(row + 1, Vec::new());
        }
        taken[row].push((recording.start, recording.end));
        rows[index] = row;
    }
    rows
}

fn first_free(taken: &[Vec<(f64, f64)>], start: f64, end: f64) -> usize {
    taken
        .iter()
        .position(|busy| !busy.iter().any(|(from, to)| start < *to && end > *from))
        .unwrap_or(taken.len())
}

/// How many rows the page draws: what the colouring needed, floored by [`Cut::row_count`]'s `nrows`, so an emptied bottom row survives until its ✕ (§B).
pub fn row_count(recordings: &[Recording], cut: &Cut) -> usize {
    let rows = rows_for(recordings, cut);
    let coloured = rows.iter().max().map(|row| row + 1).unwrap_or(0);
    cut.row_count(coloured)
}

/// The stretches with footage (§B: "Filmed runs = merged recording spans"). Recordings touching end to end are one run.
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
            Some(last) if start <= last.1 => last.1 = last.1.max(end),
            _ => runs.push((start, end)),
        }
    }
    runs
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cell {
    pub start: f64,
    pub end: f64,
    pub folded: bool,
}

/// The runs cut at the folded gaps (§B). A fold covering a whole run folds the whole run.
pub fn cells(runs: &[(f64, f64)], folds: &[[f64; 2]]) -> Vec<Cell> {
    let mut folds: Vec<(f64, f64)> = folds.iter().map(|f| (f[0], f[1])).collect();
    folds.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    let mut out: Vec<Cell> = Vec::new();
    for run in runs {
        let (from, to) = *run;
        // `at` is the point the run has been cut up to; held outside the iterator because a fold ends where it ends.
        let mut at = from;
        for (start, end) in folds.iter().copied() {
            if end <= at || start >= to {
                continue;
            }
            if start > at {
                out.push(Cell {
                    start: at,
                    end: start,
                    folded: false,
                });
            }
            out.push(Cell {
                start: at.max(start),
                end: end.min(to),
                folded: true,
            });
            at = at.max(end);
        }
        if at < to {
            out.push(Cell {
                start: at,
                end: to,
                folded: false,
            });
        }
    }
    out.sort_by(|a, b| {
        a.start
            .partial_cmp(&b.start)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    out
}

/// The laid-out band: cells, the zoom, and the gutter they start after (§B).
#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    pub cells: Vec<Cell>,
    pub pps: f64,
    pub gutter_px: f64,
}

impl Span {
    pub fn new(cells: Vec<Cell>, pps: f64, gutter_px: f64) -> Self {
        Self {
            cells,
            pps,
            gutter_px,
        }
    }

    /// A folded cell is widthless: its time is still selectable but takes no room.
    fn width(&self, cell: &Cell) -> f64 {
        if cell.folded {
            0.0
        } else {
            (cell.end - cell.start) * self.pps
        }
    }

    /// The band's whole width: the gutter plus every unfolded stretch; the answer for a time past the end.
    pub fn total_px(&self) -> f64 {
        self.gutter_px + self.cells.iter().map(|cell| self.width(cell)).sum::<f64>()
    }

    /// Where a second is drawn (§B: "xOf walks spans"). Inside a fold it is the seam x, so a playhead there sits at the fold. Half-open on the right: a second AT a cell's end belongs to the NEXT cell.
    pub fn x_of(&self, t: f64) -> f64 {
        let mut x = self.gutter_px;
        for cell in &self.cells {
            if cell.folded {
                if t >= cell.start && t <= cell.end {
                    return x;
                }
            } else if t < cell.end {
                return x + (t - cell.start).max(0.0) * self.pps;
            }
            x += self.width(cell);
        }
        // Past the end: the span's width (§B "past the end → totalW"); the band clamps marks to its own tape.
        self.total_px()
    }

    /// What second an x is (§B: "tAt is the inverse, half-open on the right"). A folded cell is passed over, so a press there lands in the footage after the fold.
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
        // Past the end: the last second; left of everything: the first run's start. Both real seconds.
        self.cells
            .iter()
            .rev()
            .find(|cell| !cell.folded)
            .map(|cell| cell.end)
            .or_else(|| {
                self.cells
                    .iter()
                    .find(|cell| !cell.folded)
                    .map(|cell| cell.start)
            })
            .unwrap_or(0.0)
    }
}

/// The zoom floor: the filmed length across the view (§2); [`cut_screen::fit_pps`] owns the rule.
/// How much footage lies before `clock`: what a person reads off the ruler. The session clock runs on through the hours
/// nobody filmed (a lecture recorded over two days has a night in it); the tape does not lay those out, so neither do its numbers.
pub fn filmed_seconds(runs: &[(f64, f64)], clock: f64) -> f64 {
    if runs.is_empty() {
        return clock;
    }
    runs.iter()
        .map(|(start, end)| (clock.min(*end) - start).max(0.0))
        .sum()
}

/// The session second `filmed` seconds of footage in: the inverse of [`filmed_seconds`]. Past the end it runs on linearly.
pub fn clock_at(runs: &[(f64, f64)], filmed: f64) -> f64 {
    let mut left = filmed;
    for (start, end) in runs {
        if left < end - start {
            return start + left;
        }
        left -= end - start;
    }
    runs.last().map_or(filmed, |(_, end)| end + left)
}

pub fn floor_pps(runs: &[(f64, f64)], view_px: f64) -> f64 {
    let filmed = runs.iter().map(|(start, end)| end - start).sum::<f64>();
    cut_screen::fit_pps(filmed, view_px)
}

/// The hand correction stored for one source (§B's `shift` map), defaulting to 0.0 when never slid.
pub fn shift_of(cut: &Cut, base: &str) -> f64 {
    cut.shift.get(base).copied().unwrap_or(0.0)
}

/// §B's `at(t)`: a SESSION second to a second INSIDE one source, `t − start + off + shift` (hand correction).
pub fn local_at(session_t: f64, start: f64, off: f64, shift: f64) -> f64 {
    session_t - start + off + shift
}

/// §B's `sessionAt(local)`: the exact inverse, `start + local − off − shift`.
pub fn session_at(local: f64, start: f64, off: f64, shift: f64) -> f64 {
    start + local - off - shift
}

/// Do these two stretches share any second? Half-open on the right, so **touching is not overlapping**, as [`Span::t_at`] reads a seam (§7).
pub fn spans_overlap(a: (f64, f64), b: (f64, f64)) -> bool {
    a.0 < b.1 && b.0 < a.1
}
