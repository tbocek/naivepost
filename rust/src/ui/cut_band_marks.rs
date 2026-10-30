//! §05-cut#1-screen "Tracks" — every mark on the band a hand can press, drawn on the band and pressed there.
//!
//! §1 puts them all on the tape: the ✕ on each green bar (badge 25) and on the selection (27), the −/+ fold badges
//! in the dropped gaps (26), the fold-all badge (23), each row's sound switch (29) and the empty row's ✕ in the
//! gutter, and the speaker and lens badges on the scene under the line or held. The prototype paints them as plated
//! marks (`go/cut_edit.go` `drawKillBadge`, `go/cut_fold.go` `foldPlate`, `go/cut_hear.go` `hearPlate`/`camPlate`)
//! and answers a press by asking which mark is under it. The first port made them GTK buttons: a row of them under
//! the tape, grey button plates over the bar, and a ✕ inked on the bar that a press did nothing to.
//!
//! One list answers both questions: [`placed`] says where each mark stands, the painter draws that list, and
//! [`press_at`] hit-tests the same list and forwards to the door the old button fired. No rule is decided here.

use adw::prelude::*;

use crate::cut::Seg;
use crate::cut_verbs::Badge;
use crate::timeline::Recording;
use crate::ui::cut_tracks::{self, Rows, Scene};
use crate::{cut, cut_cam, cut_fold, cut_hear, cut_screen};

/// A ✕ or fold plate's radius (`segKillR 4 + segKillPad 3`).
pub const PLATE_R: f64 = 7.0;
/// A speaker's or a lens's plate.
pub const HEAR_PLATE_R: f64 = 8.0;
/// How far from its mark a press still takes it, either way.
pub const HIT_PX: f64 = 10.0;
/// The glyph's reach inside a speaker plate (`hearR`).
const HEAR_R: f64 = 4.5;
/// A scene narrower than this wears no speaker or lens: the plate would cover its grips.
const HEAR_MIN_PX: f64 = 28.0;
/// The scene badges' column, in from the scene's left edge.
const HEAR_IN_PX: f64 = 14.0;
/// The selection's ✕, in from its right end (`killIn`: the grip's 6 px plus the hit's 10).
const KILL_IN_PX: f64 = 16.0;

/// A ✕ or fold plate, dark and translucent over the green.
const PLATE_DARK: (f64, f64, f64, f64) = (0.06, 0.06, 0.07, 0.55);
/// A speaker whose sound is off.
const PLATE_OFF: (f64, f64, f64, f64) = (0.06, 0.06, 0.07, 0.62);
/// A speaker whose sound is heard, a lens on the row in use.
const PLATE_ON: (f64, f64, f64, f64) = (0.15, 0.65, 0.3, 0.95);
/// The mark on a plate.
const MARK_INK: (f64, f64, f64, f64) = (1.0, 1.0, 1.0, 0.92);

/// What the band's marks say, read off the cut when the page builds its scene.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BandMarks {
    /// The cut holds folds, so the gutter badge unfolds them all (+); else it folds every gap (−).
    pub folded: bool,
    /// Each camera row: what a status calls it and whether its own sound is heard in any scene.
    pub rows: Vec<(String, bool)>,
    /// Each cut lane (the scene's `lanes` order): heard in any scene.
    pub lanes_heard: Vec<bool>,
    /// The emptied bottom row its ✕ may take (F2.11 S2).
    pub empty_row: Option<usize>,
    /// The scene the speaker and lens badges are about: held, else under the line.
    pub hear: Option<HearScene>,
    /// Recordings the join pass took out entire (`retakes.tsv`'s `whole`), washed yellow on their row.
    pub whole: Vec<String>,
}

/// The scene the badges are about.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HearScene {
    /// Its index in the cut.
    pub index: usize,
    /// The row it is shown from.
    pub row: usize,
    /// A recording on that row runs under it, so it has a sound of its own to switch.
    pub has_own: bool,
    /// Its own footage sound is heard (`!mute`).
    pub own_heard: bool,
    /// Each cut lane, heard in this scene.
    pub lanes_heard: Vec<bool>,
    /// The rows that filmed any of its seconds; a lens is drawn on each when there are two or more.
    pub filming: Vec<usize>,
}

/// One pressable mark.
#[derive(Debug, Clone, PartialEq)]
pub enum Mark {
    /// A green bar's ✕: drop scene N.
    SceneCross(usize),
    /// The selection's ✕: clear it.
    SelectionCross,
    /// A dropped gap's badge: − folds it, + (folded) opens it.
    Fold((f64, f64), bool),
    /// The gutter's fold-all badge.
    FoldAll(bool),
    /// A camera row's sound switch for the whole cut.
    RowSound(usize, bool),
    /// The emptied bottom row's ✕.
    RowCross(usize),
    /// A cut lane's sound switch for the whole cut.
    LaneSound(usize, bool),
    /// A cut lane's ✕.
    LaneCross(usize),
    /// The badge scene's own footage sound.
    SceneSound(bool),
    /// A cut lane, heard in the badge scene or not.
    SceneLane(usize, bool),
    /// A row the badge scene could be shown from; lit on the row it is shown from.
    Lens(usize, bool),
}

/// A mark and where it stands. `pinned` marks live in the gutter and do not scroll; the others are in tape pixels.
#[derive(Debug, Clone, PartialEq)]
pub struct Placed {
    pub mark: Mark,
    pub x: f64,
    pub y: f64,
    pub pinned: bool,
}

/// The marks for one cut, read with the rules that own each answer.
pub fn marks_for(
    cut_: &cut::Cut,
    recordings: &[Recording],
    rows: &[usize],
    row_count: usize,
    line: f64,
    held: Option<&Seg>,
) -> BandMarks {
    let names: Vec<String> = (0..row_count.max(1))
        .map(|row| cut_cam::row_name(row as i32, recordings, rows))
        .collect();
    let last = row_count.max(1) - 1;
    let hear = cut_cam::held_or_line_scene(cut_, Some(line), held).map(|index| {
        let seg = &cut_.segs[index];
        let row = seg.cam.max(0) as usize;
        HearScene {
            index,
            row,
            has_own: recordings
                .iter()
                .zip(rows)
                .any(|(rec, placed)| *placed == row && rec.start < seg.e && rec.end > seg.s),
            own_heard: cut_hear::footage_sound_heard(seg),
            lanes_heard: cut_
                .lanes
                .iter()
                .map(|lane| seg.hears(&lane.name))
                .collect(),
            filming: cut_cam::filming_rows(seg, recordings, rows)
                .into_iter()
                .map(|row| row.max(0) as usize)
                .collect(),
        }
    });
    BandMarks {
        folded: !cut_.folds.is_empty(),
        rows: names
            .into_iter()
            .enumerate()
            .map(|(row, name)| (name, cut_hear::row_is_heard_anywhere(cut_, row as i32)))
            .collect(),
        lanes_heard: cut_
            .lanes
            .iter()
            .map(|lane| cut_hear::lane_is_heard_anywhere(cut_, &[lane.name.as_str()]))
            .collect(),
        empty_row: (row_count > 1 && cut_fold::row_is_empty(last, recordings, rows, &cut_.segs))
            .then_some(last),
        hear,
        whole: Vec::new(),
    }
}

thread_local! {
    /// `whole_takes`' last read: the file, its mtime then, and the bases it named.
    static WHOLE: std::cell::RefCell<Option<(std::path::PathBuf, std::time::SystemTime, Vec<String>)>> =
        const { std::cell::RefCell::new(None) };
}

/// The recordings the open project's join pass took out entire, read off `retakes.tsv` and re-read only when
/// that file changes (the band asks on every draw).
pub fn whole_takes() -> Vec<String> {
    let Ok(tree) = crate::layout::Tree::new(crate::startup::open_dir()) else {
        return Vec::new();
    };
    let file = tree.retakes_tsv();
    let Ok(changed) = std::fs::metadata(&file).and_then(|meta| meta.modified()) else {
        return Vec::new();
    };
    if let Some(held) = WHOLE.with(|cell| {
        cell.borrow()
            .as_ref()
            .filter(|(path, at, _)| *path == file && *at == changed)
            .map(|(_, _, bases)| bases.clone())
    }) {
        return held;
    }
    let bases: Vec<String> = crate::retakes::read_marks(&file)
        .unwrap_or_default()
        .iter()
        .flat_map(|mark| {
            mark.whole
                .split(',')
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .filter(|base| !base.is_empty())
        .collect();
    WHOLE.with(|cell| *cell.borrow_mut() = Some((file, changed, bases.clone())));
    bases
}

/// Where every mark stands on this scene's band, in the order a press asks them (gutter first: it sits over the
/// tape's head).
pub fn placed(scene: &Scene, rows: Rows) -> Vec<Placed> {
    let mut out = Vec::new();
    let band_y = rows.selection + cut_tracks::SELECTION_BAND_PX / 2.0;
    let rail = cut_screen::GUTTER_PX / 2.0;
    let marks = &scene.marks;
    let pin = |mark, y| Placed {
        mark,
        x: rail,
        y,
        pinned: true,
    };
    let has_gaps = !cut_fold::dropped_gaps(&scene.runs(), &scene.segs).is_empty();
    if has_gaps || marks.folded {
        out.push(pin(Mark::FoldAll(marks.folded), band_y));
    }
    for (row, (_, heard)) in marks.rows.iter().enumerate() {
        out.push(pin(
            Mark::RowSound(row, *heard),
            rows.wave_top(row) + cut_tracks::row_wave_px() / 2.0,
        ));
    }
    if let Some(row) = marks.empty_row {
        let picture = rows.pictures + row as f64 * rows.row_h;
        out.push(pin(
            Mark::RowCross(row),
            picture + (rows.row_h - cut_tracks::row_wave_px()) / 2.0,
        ));
    }
    for (lane, heard) in marks.lanes_heard.iter().enumerate() {
        let middle = rows.lane_top(scene, lane) + rows.row_h / 2.0;
        out.push(pin(Mark::LaneSound(lane, *heard), middle - HIT_PX));
        out.push(pin(Mark::LaneCross(lane), middle + HIT_PX));
    }
    for (index, (x, w)) in cut_tracks::band_boxes(scene).into_iter().enumerate() {
        if scene.segs[index].is_insert() || w < cut_tracks::CROSS_MIN_PX {
            continue;
        }
        out.push(Placed {
            mark: Mark::SceneCross(index),
            x: x + w / 2.0,
            y: band_y,
            pinned: false,
        });
    }
    if let Some((from, to)) = scene.selection {
        let (x, w) = cut_tracks::band_span_px(scene, from, to);
        if w >= 2.0 * KILL_IN_PX {
            out.push(Placed {
                mark: Mark::SelectionCross,
                x: x + w - KILL_IN_PX,
                y: band_y,
                pinned: false,
            });
        }
    }
    let span = cut_tracks::band_span(scene);
    for badge in cut_fold::badges(&scene.folds, &scene.runs(), &scene.segs, &span) {
        out.push(Placed {
            mark: Mark::Fold(badge.gap, badge.folded),
            x: badge.x,
            y: band_y,
            pinned: false,
        });
    }
    if let Some(hear) = &marks.hear {
        let seg = &scene.segs[hear.index];
        let (x, w) = cut_tracks::band_span_px(scene, seg.s, seg.e);
        if w >= HEAR_MIN_PX {
            let cx = x + HEAR_IN_PX;
            let at = |mark, y| Placed {
                mark,
                x: cx,
                y,
                pinned: false,
            };
            if hear.has_own {
                out.push(at(
                    Mark::SceneSound(hear.own_heard),
                    rows.wave_top(hear.row) + cut_tracks::row_wave_px() / 2.0,
                ));
            }
            for (lane, heard) in hear.lanes_heard.iter().enumerate() {
                out.push(at(
                    Mark::SceneLane(lane, *heard),
                    rows.lane_top(scene, lane) + rows.row_h / 2.0,
                ));
            }
            if hear.filming.len() >= 2 {
                for row in &hear.filming {
                    let picture = rows.pictures + *row as f64 * rows.row_h;
                    out.push(at(
                        Mark::Lens(*row, *row == hear.row),
                        picture + (rows.row_h - cut_tracks::row_wave_px()) / 2.0,
                    ));
                }
            }
        }
    }
    out
}

/// Paint the tape's marks. Called inside the band's scroll translation, so x is tape pixels.
pub fn paint_tape(cr: &cairo::Context, scene: &Scene, rows: Rows) {
    // The badge scene's own stretch of each strip it speaks for: green where heard, grey where not, so a scene
    // too narrow for a badge still says what it does (`drawHearBadges`).
    if let Some(hear) = &scene.marks.hear {
        let seg = &scene.segs[hear.index];
        let (x, w) = cut_tracks::band_span_px(scene, seg.s, seg.e);
        if hear.has_own {
            wash(cr, hear.own_heard, x, rows.wave_top(hear.row), w);
        }
        for (lane, heard) in hear.lanes_heard.iter().enumerate() {
            wash(cr, *heard, x, rows.lane_top(scene, lane), w);
        }
    }
    for placed in placed(scene, rows).iter().filter(|p| !p.pinned) {
        paint_mark(cr, &placed.mark, placed.x, placed.y);
    }
}

/// Paint the gutter's marks, after the gutter itself.
pub fn paint_pinned(cr: &cairo::Context, scene: &Scene, rows: Rows) {
    for placed in placed(scene, rows).iter().filter(|p| p.pinned) {
        paint_mark(cr, &placed.mark, placed.x, placed.y);
    }
}

fn wash(cr: &cairo::Context, heard: bool, x: f64, top: f64, w: f64) {
    if heard {
        cr.set_source_rgba(0.2, 0.85, 0.35, 0.22);
    } else {
        cr.set_source_rgba(0.55, 0.55, 0.6, 0.3);
    }
    cr.rectangle(x, top, w.max(0.0), cut_tracks::row_wave_px());
    let _ = cr.fill();
}

fn paint_mark(cr: &cairo::Context, mark: &Mark, cx: f64, cy: f64) {
    match mark {
        Mark::SceneCross(_) | Mark::RowCross(_) | Mark::LaneCross(_) => {
            plate(cr, cx, cy, PLATE_R, PLATE_DARK);
            cross(cr, cx, cy, 4.0, (1.0, 1.0, 1.0, 0.9));
        }
        Mark::SelectionCross => {
            cross(cr, cx, cy, 3.0, (1.0, 1.0, 1.0, 0.85));
        }
        Mark::Fold(_, folded) | Mark::FoldAll(folded) => {
            plate(cr, cx, cy, PLATE_R, PLATE_DARK);
            sign(cr, cx, cy, *folded);
        }
        Mark::RowSound(_, on)
        | Mark::LaneSound(_, on)
        | Mark::SceneSound(on)
        | Mark::SceneLane(_, on) => {
            plate(
                cr,
                cx,
                cy,
                HEAR_PLATE_R,
                if *on { PLATE_ON } else { PLATE_OFF },
            );
            speaker(cr, cx, cy, *on);
        }
        Mark::Lens(_, on) => {
            plate(
                cr,
                cx,
                cy,
                HEAR_PLATE_R,
                if *on { PLATE_ON } else { PLATE_OFF },
            );
            lens(cr, cx, cy, *on);
        }
    }
}

fn plate(cr: &cairo::Context, cx: f64, cy: f64, r: f64, rgba: (f64, f64, f64, f64)) {
    cr.set_source_rgba(rgba.0, rgba.1, rgba.2, rgba.3);
    cr.arc(cx, cy, r, 0.0, std::f64::consts::TAU);
    let _ = cr.fill();
}

/// Two crossing strokes, 1.6 px.
fn cross(cr: &cairo::Context, cx: f64, cy: f64, reach: f64, ink: (f64, f64, f64, f64)) {
    cr.set_source_rgba(ink.0, ink.1, ink.2, ink.3);
    cr.set_line_width(1.6);
    cr.move_to(cx - reach, cy - reach);
    cr.line_to(cx + reach, cy + reach);
    cr.move_to(cx + reach, cy - reach);
    cr.line_to(cx - reach, cy + reach);
    let _ = cr.stroke();
}

/// − on an open gap, + on a folded one.
fn sign(cr: &cairo::Context, cx: f64, cy: f64, plus: bool) {
    cr.set_source_rgba(MARK_INK.0, MARK_INK.1, MARK_INK.2, MARK_INK.3);
    cr.set_line_width(1.6);
    cr.move_to(cx - 4.0, cy);
    cr.line_to(cx + 4.0, cy);
    if plus {
        cr.move_to(cx, cy - 4.0);
        cr.line_to(cx, cy + 4.0);
    }
    let _ = cr.stroke();
}

/// The speaker: a cone, with two waves when heard and a small ✕ when not (`drawSpeaker`).
fn speaker(cr: &cairo::Context, cx: f64, cy: f64, on: bool) {
    let r = HEAR_R;
    cr.set_source_rgba(MARK_INK.0, MARK_INK.1, MARK_INK.2, MARK_INK.3);
    cr.move_to(cx - r, cy - r / 2.5);
    cr.line_to(cx - r / 2.5, cy - r / 2.5);
    cr.line_to(cx + r / 4.0, cy - r);
    cr.line_to(cx + r / 4.0, cy + r);
    cr.line_to(cx - r / 2.5, cy + r / 2.5);
    cr.line_to(cx - r, cy + r / 2.5);
    cr.close_path();
    let _ = cr.fill();
    cr.set_line_width(1.4);
    if on {
        for reach in [0.65, 1.05] {
            cr.new_sub_path();
            cr.arc(cx + r / 4.0, cy, r * reach, -0.9, 0.9);
        }
    } else {
        cr.move_to(cx + 0.55 * r, cy - 0.55 * r);
        cr.line_to(cx + 1.25 * r, cy + 0.55 * r);
        cr.move_to(cx + 1.25 * r, cy - 0.55 * r);
        cr.line_to(cx + 0.55 * r, cy + 0.55 * r);
    }
    let _ = cr.stroke();
}

/// The lens: a ring, filled on the row in use (`drawLens`).
fn lens(cr: &cairo::Context, cx: f64, cy: f64, on: bool) {
    cr.set_source_rgba(MARK_INK.0, MARK_INK.1, MARK_INK.2, MARK_INK.3);
    cr.set_line_width(1.4);
    cr.arc(cx, cy, HEAR_R * 0.82, 0.0, std::f64::consts::TAU);
    let _ = cr.stroke();
    if on {
        cr.arc(cx, cy, HEAR_R * 0.4, 0.0, std::f64::consts::TAU);
        let _ = cr.fill();
    }
}

/// This window's marks where its band draws them, in the band's own pixels: tape marks less the scroll, so each
/// `(x, y)` is where a press on the surface lands on it.
pub fn placed_on(window: &adw::ApplicationWindow) -> Vec<Placed> {
    let scene = crate::ui::cut_view::band_scene_for(window);
    let rows = cut_tracks::rows_for_press(&scene);
    placed(&scene, rows)
        .into_iter()
        .map(|placed| Placed {
            x: if placed.pinned {
                placed.x
            } else {
                placed.x - scene.origin
            },
            ..placed
        })
        .collect()
}

/// The mark under a press at band pixel `(x, y)`, gutter first.
pub fn mark_at(scene: &Scene, rows: Rows, x: f64, y: f64) -> Option<Mark> {
    placed(scene, rows)
        .into_iter()
        .find(|p| {
            let px = if p.pinned { x } else { x + scene.origin };
            (px - p.x).abs() <= HIT_PX && (y - p.y).abs() <= HIT_PX
        })
        .map(|p| p.mark)
}

/// A press on the band: if a mark is under it, fire that mark's door, say what happened, repaint, and answer
/// true so the press places no line and starts no selection.
pub fn press_at(window: &adw::ApplicationWindow, x: f64, y: f64) -> bool {
    let scene = crate::ui::cut_view::band_scene_for(window);
    let rows = cut_tracks::rows_for_press(&scene);
    let Some(mark) = mark_at(&scene, rows, x, y) else {
        return false;
    };
    let said = fire(window, &scene, &mark);
    if let (Some(said), Some(status)) = (said, crate::ui::window::find_status(window.upcast_ref()))
    {
        status.set_text(&said);
    }
    crate::ui::cut_view::repaint_page(window);
    true
}

/// The door each mark stands for: the same seams the old buttons fired.
fn fire(window: &adw::ApplicationWindow, scene: &Scene, mark: &Mark) -> Option<String> {
    use crate::ui::{cut_cameras, cut_folds, cut_verb_press::press_badge};
    let lane_name = |lane: usize| scene.lanes.get(lane).map(|lane| lane.name.clone());
    match mark {
        Mark::SceneCross(index) => Some(press_badge(window, Badge::GreenBar, *index)),
        Mark::SelectionCross => Some(press_badge(window, Badge::Selection, 0)),
        Mark::Fold(gap, _) => cut_folds::press_fold_at(window, *gap),
        Mark::FoldAll(_) => Some(cut_folds::press_fold_all(window)),
        Mark::RowSound(row, _) => {
            let name = scene.marks.rows.get(*row).map(|(name, _)| name.clone())?;
            Some(cut_cameras::press_row_switch(window, *row as i32, &name))
        }
        Mark::RowCross(row) => Some(press_badge(window, Badge::EmptyRow, *row)),
        Mark::LaneSound(lane, _) => {
            Some(cut_cameras::press_gutter_switch(window, &lane_name(*lane)?))
        }
        Mark::LaneCross(lane) => Some(press_badge(window, Badge::CutLane, *lane)),
        Mark::SceneSound(_) => cut_cameras::press_scene_sound(window),
        Mark::SceneLane(lane, _) => cut_cameras::press_speaker_badge(window, &lane_name(*lane)?),
        Mark::Lens(row, _) => cut_cameras::press_lens_row(window, *row),
    }
}
