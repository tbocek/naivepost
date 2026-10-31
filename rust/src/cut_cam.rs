//! F2.10 (Cameras and hearing) — `spec/05-cut.md` F2.10.

use std::collections::BTreeMap;

use crate::cut::{Cut, Seg};
use crate::cut_hear;
use crate::timeline::Recording;

/// F2.10 (`the scene under the line, or the held one`): which kept scene a lens or speaker badge means.
pub fn held_or_line_scene(cut_: &Cut, line: Option<f64>, held: Option<&Seg>) -> Option<usize> {
    if let Some(held) = held {
        // Found by identity, not by trusting the index the hand came from: a renumbering (an Add, a Remove)
        // between the pick and the badge press would otherwise edit whatever moved into that slot.
        return cut_
            .segs
            .iter()
            .position(|seg| seg.ins.is_empty() && seg.s == held.s && seg.e == held.e);
    }
    let at = line?;
    cut_.segs
        .iter()
        .position(|seg| seg.ins.is_empty() && seg.s <= at && at < seg.e)
}

pub fn filming_rows(seg: &Seg, recordings: &[Recording], rows: &[usize]) -> Vec<i32> {
    let mut filming: Vec<i32> = recordings
        .iter()
        .zip(rows)
        .filter(|(rec, _)| rec.start < seg.e && seg.s < rec.end)
        .map(|(_, placed)| *placed as i32)
        .collect();
    filming.sort_unstable();
    filming.dedup();
    filming
}

/// The row a 🔍 press lands on: the FIRST filming row that is not the scene's current one, wrapping to the
/// lowest when the scene already sits on the highest. `None` when there is nothing else to switch to — one
/// camera filmed the whole span, so the badge has nowhere to go.
pub fn next_filming_row(current: i32, filming: &[i32]) -> Option<i32> {
    let rotating: Vec<i32> = filming
        .iter()
        .filter(|row| **row != current)
        .copied()
        .collect();
    // Wrap rather than stop: a scene on the top camera must still be able to get back to the bottom one.
    rotating
        .iter()
        .find(|row| **row > current)
        .or_else(|| rotating.first())
        .copied()
}

/// F2.10 S1 (`🔍 lens badge: which row its picture comes from`): is this scene's badge lit on that row? The
/// badge answers the question the row asks — "are you shown from here" — rather than being told it is, so a
/// scene drawn on three rows lights exactly one badge (`spec/inventory/cut.md` §A: the camera badge).
pub fn shown_from(cam: i32, row: i32) -> bool {
    cam == row
}

/// F2.10 S1: the row a scene's picture is taken from — its `cam`, which is all the lens badge reads and writes.
pub fn scene_row(scene: &Seg) -> i32 {
    scene.cam
}

pub fn show_scene_from(
    cut: &mut Cut,
    scene: usize,
    row: i32,
    recordings: &[Recording],
    rows: &[usize],
) -> Option<String> {
    let seg = cut.segs.get_mut(scene)?;
    if !seg.ins.is_empty() {
        return None;
    }
    seg.cam = row;
    Some(format!(
        "the scene at {} is shown from {} now",
        cut_hear::scene_clock(seg.s),
        row_name(row, recordings, rows)
    ))
}

/// What a status calls a row: the recording lying on it, or `row N` counting from one. The fallback matters
/// because a row can be empty — an emptied bottom row survives until its ✕ (`spec/inventory/cut.md` §B), and a
/// lane's row may hold a file nothing names — and "the cut shows  here" is not a sentence.
pub fn row_name(row: i32, recordings: &[Recording], rows: &[usize]) -> String {
    recordings
        .iter()
        .zip(rows)
        .find(|(_, placed)| **placed as i32 == row)
        .map(|(rec, _)| rec.base.clone())
        .unwrap_or_else(|| format!("row {}", row + 1))
}

/// The name plate over a row: the recording it speaks for, which part of that file it is showing, and the shift
/// correction the page was opened with (**2** in `spec/img/05-rows.png`: "camera 2 (17-25-45) with its shift
/// correction −19.00 s in the name plate").
pub fn name_plate(base: &str, shift: f64, off: f64) -> String {
    let mut plate = base.to_string();
    if off > 0.0 {
        plate.push_str(&format!(" from {}", cut_hear::scene_clock(off)));
    }
    if shift != 0.0 {
        plate.push_str(&format!(" {shift:+.2} s"));
    }
    plate
}

/// F2.10 (the Prototype paragraph): a recording slid onto another row pins ONLY itself.
pub fn pin_for_drag(
    base: &str,
    row: usize,
    cut_rows: &BTreeMap<String, i32>,
) -> BTreeMap<String, i32> {
    let mut rows = cut_rows.clone();
    rows.insert(base.to_string(), row as i32);
    rows
}

/// F2.10 S3 (`A click on a row watches it in the preview until ▶ takes the preview back to the cut`): which row
/// the preview is showing, and whether this click has already been explained.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Watch {
    pub row: Option<i32>,
    pub said: bool,
}

impl Watch {
    /// F2.10 S3 (**5** — "the preview shows the watched row, not the cut"): which row the preview is showing.
    /// The watched row while one is watched, else the row the cut itself shows.
    pub fn preview_shows(&self, cut_row: i32) -> i32 {
        self.row.unwrap_or(cut_row)
    }

    /// F2.10 S3 (`until ▶ takes the preview back to the cut`): clear the watch. `true` when something was handed
    /// back, so the page only says the row is no longer watched when it had been.
    pub fn play_hands_back(&mut self) -> bool {
        let was = self.row.take();
        self.said = false;
        was.is_some()
    }
}

/// F2.10 S3: what a click on a row did — the row it watches, and the sentence to leave in the status line if
/// the cut shows a different camera where the line stands.
pub fn click_row(watch: &mut Watch, row: i32, segs: &[Seg], line: Option<f64>) -> Option<String> {
    watch.row = Some(row);
    watch.said = false;
    watch_status(watch, segs, line)
}

/// F2.10 S3 (`if the line is in a kept scene shown from another row, status says ONCE …`): the sentence, at most
/// once per click. The page may call this on every move of the line without repeating itself; a fresh
/// [`click_row`] arms it again.
pub fn watch_status(watch: &mut Watch, segs: &[Seg], line: Option<f64>) -> Option<String> {
    let watched = watch.row?;
    if watch.said {
        return None;
    }
    // The half-open reading is cut_hear::scene_at's, and "kept" means a scene with footage under it — an insert
    // brings its own picture and answers to no row.
    let at = line?;
    let scene = segs
        .iter()
        .find(|seg| seg.ins.is_empty() && seg.s <= at && at < seg.e)?;
    if scene.cam == watched {
        return None;
    }
    watch.said = true;
    Some(format!(
        "watching camera {} \u{2014} the cut shows camera {} here; \u{25b6} plays the cut",
        watched + 1,
        scene.cam + 1
    ))
}
