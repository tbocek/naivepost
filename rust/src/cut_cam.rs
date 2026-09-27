//! F2.10 (Cameras and hearing) — `spec/05-cut.md` F2.10.
//!
//! Two recordings that ran at once are laid out on rows by greedy interval colouring (that arithmetic is
//! [`crate::timeline`]'s, and this module does not repeat it); a kept scene takes its PICTURE from one row and
//! hears the lanes its speaker badges allow. This module owns the three things a person does about that: pick
//! which row a scene's picture comes from (S1), say whether this scene hears a lane — per scene, or for the
//! whole cut from the gutter (S2, whose badge rules live in [`crate::cut_hear`] because F2.5 already owned the
//! hearing), and watch a row in the preview until ▶ hands the preview back to the cut (S3).
//!
//! A row-drag is where the rewrite deliberately differs from the prototype: it pins only the recording dragged
//! ([`pin_for_drag`]), so an overlap made by hand gets a row of its own instead of being drawn under its
//! neighbour and becoming impossible to see or pick.
//!
//! No UI lives here — no widget is built or held by this module — but every rule in it IS wired, and the
//! Cut page's row list is the only thing between a click and these functions:
//! `watch-row-<n>` → `ui::press_watch_row` → [`click_row`],
//! `lens-badge-<n>` → `ui::press_lens_row` → [`show_scene_from`],
//! `speaker-badge-<lane>` → `ui::press_speaker_badge` → [`crate::cut_hear::toggle_heard`],
//! `gutter-switch-<lane>` → `ui::press_gutter_switch` → [`crate::cut_hear::toggle_lane_all`].
//! The rows themselves are drawn into the box named `camera-rows` by `ui::refresh_camera_rows`, which
//! reads this module's answers rather than deciding anything of its own, and ▶ releases the preview from
//! a watched row through `press_play_cut` → `ui::hand_preview_back` → [`Watch::play_hands_back`]. So
//! the flow is reachable from the page and comparable against `spec/img/05-rows.png`; what stays here
//! is the decision, so a test can check it without a display.

use std::collections::BTreeMap;

use crate::cut::{Cut, Seg};
use crate::cut_hear;
use crate::timeline::Recording;

// --- S1: which row a scene's picture comes from -------------------------------------------------------------

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

/// F2.10 S1 (`"the scene at m:ss is shown from <cam> now"`): take the scene's picture from another row. The row
/// number is what `Cut::segs` stores (§A's camera badge answers "is this scene shown from here"), and the
/// status calls it by what lies on it ([`row_name`]), because "shown from 2" says nothing to someone looking at
/// two cameras.
///
/// `None` for an insert: a card, a still or an overlaid sound has no picture of its own to be shown from a row,
/// so asking it which camera it uses has no answer and its `cam` stays as it was rather than becoming a number
/// nothing reads.
pub fn show_scene_from(cut: &mut Cut, scene: usize, row: i32, recordings: &[Recording], rows: &[usize]) -> Option<String> {
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
///
/// The shift is spelled out because a camera moved by hand looks EXACTLY like one whose file says it started
/// there, and the difference is the whole of what the right-drag did — `spec/inventory/cut.md` §B's own reason.
/// A lane cut from a recording is that recording's name over again, so ` from <m:ss>` is the only thing on the
/// page saying which part of the file the row is showing. The signed reading (`+`/`−`, two decimals) is the
/// prototype's, and it is what makes a correction read as a correction rather than a duration.
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

// --- the row-drag's pin -----------------------------------------------------------------------------------

/// F2.10 (the Prototype paragraph): a recording slid onto another row pins ONLY itself.
///
/// The prototype pinned every recording that shared the row, so a drag that created an overlap left both
/// recordings on one row — the later drawn over the earlier, whose overlapped seconds could then be neither
/// seen nor picked. Pinning just the dragged one lets [`crate::timeline::rows_for`] re-run and give the new
/// overlap a row of its own, which is what the same overlap gets when it comes from the file names instead.
///
/// Returns the map for [`Cut::rows`]; every pin already in `cut_rows` is carried over untouched (a pin read
/// from the file stays literal — the editor moved that recording knowing what was under it). The caller
/// re-runs the colouring and redraws; nothing here decides rows.
pub fn pin_for_drag(base: &str, row: usize, cut_rows: &BTreeMap<String, i32>) -> BTreeMap<String, i32> {
    let mut rows = cut_rows.clone();
    rows.insert(base.to_string(), row as i32);
    rows
}

// --- S3: watching a row -----------------------------------------------------------------------------------

/// F2.10 S3 (`A click on a row watches it in the preview until ▶ takes the preview back to the cut`): which row
/// the preview is showing, and whether this click has already been explained.
///
/// `said` exists because the sentence about another camera is worth once: the line moves, the page redraws, and
/// a status the same click re-answers on every frame drowns everything else the page has to say.
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
///
/// The watch is set whatever else follows (a click always means "show me this row"), so the returned status is
/// only about the surprise: **7** `watching camera 2 — the cut shows camera 1 here; ▶ plays the cut`, said when
/// the line lies in a KEPT scene whose picture comes from another row. An insert at the line is not a kept scene
/// and owns no camera, so watching a row over a card says nothing; nor does clicking the row already shown.
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
    let scene = segs.iter().find(|seg| seg.ins.is_empty() && seg.s <= at && at < seg.e)?;
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
