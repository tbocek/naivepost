//! F2.7 (Add, Split, Remove, ⌦) — the page's half: the four doors the buttons and ⌦ fire, what each one
//! writes, and the ✕ badges.

use adw::prelude::*;

use crate::cut::Seg;
use crate::cut_select::{Scope, Selection};
use crate::cut_verbs::{self, Badge};
use crate::ui::cut_folds;
use crate::ui::wave_snap;
use crate::ui::window as ui;

thread_local! {
    static VERB_HAND: std::cell::RefCell<Option<Selection>> =
        const { std::cell::RefCell::new(None) };
}

/// What a Split left in the hand, newest answer.
pub fn verb_hand() -> Option<Selection> {
    VERB_HAND.with(|cell| cell.borrow().clone())
}

/// Put `band` in the hand.
pub fn take_into_hand(band: Option<Selection>) {
    VERB_HAND.with(|cell| *cell.borrow_mut() = band);
}

/// F2.7 ＋ Add's snap candidates, gathered from live state — the spec's `word edges, silences, line edges,
/// visual cuts`.
pub fn add_snap_points(band: Option<&Selection>) -> Vec<f64> {
    let cut_ = ui::newest_review_cut();
    let mut points: Vec<f64> = Vec::new();
    for seg in &cut_.segs {
        points.push(seg.s);
        points.push(seg.e);
    }
    points.push(ui::preview_playhead());
    points.extend(wave_snap::snap_points(band));
    points.sort_by(f64::total_cmp);
    points.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
    points
}

/// F2.7 ⌦: a held effect, then a held clip, then the selection, then the scene under the line — in that
/// order, decided by [`cut_verbs::delete_verb`].
pub fn press_delete_key(window: &adw::ApplicationWindow) -> cut_verbs::Outcome {
    let segs = ui::newest_review_cut().segs;
    let band = ui::selection();
    let scene = held_scene_under_line();
    let outcome = cut_verbs::delete_verb(
        crate::ui::cut_hand::held_effect().as_ref(),
        crate::ui::cut_hand::held_clip_or_edge().as_ref(),
        band.as_ref(),
        scene.as_ref(),
        &segs,
    );
    report_verb(window, &outcome);
    if matches!(outcome, cut_verbs::Outcome::Applied { .. })
        && crate::ui::cut_hand::held_effect().is_none()
        && crate::ui::cut_hand::held_clip_or_edge().is_some()
    {
        crate::ui::cut_hand::clear_all();
    }
    outcome
}

/// F2.7 ＋ Add: keep the band as scenes, both ends snapped to [`add_snap_points`].
pub fn press_add(window: &adw::ApplicationWindow) -> cut_verbs::Outcome {
    let band = ui::selection();
    let cut_ = ui::newest_review_cut();
    let points = add_snap_points(band.as_ref());
    let outcome = cut_verbs::add(band.as_ref(), &cut_.segs, &points);
    report_verb(window, &outcome);
    outcome
}

/// F2.7 | Split: a border at each end of the band, or one at the red line when there is none, in which
/// case the right half of what was cut goes into the hand.
pub fn press_split(window: &adw::ApplicationWindow) -> cut_verbs::Outcome {
    let band = ui::selection();
    let cut_ = ui::newest_review_cut();
    let line = ui::preview_playhead();
    let outcome = cut_verbs::split(band.as_ref(), line, &cut_.segs);
    // The hand is filled from the answer's OWN segment list, before `report_verb` clears the band: the
    // right half is the scene that now starts at the line, and after a clear there is nothing left to ask.
    if let cut_verbs::Outcome::Applied {
        segs, into_hand, ..
    } = &outcome
    {
        if *into_hand {
            take_into_hand(right_half_at_line(segs, line));
        }
    }
    report_verb(window, &outcome);
    outcome
}

/// The right half a border drawn at `line` produced: the footage scene that now starts there.
fn right_half_at_line(segs: &[Seg], line: f64) -> Option<Selection> {
    let scene = segs
        .iter()
        .find(|seg| seg.ins.is_empty() && (seg.s - line).abs() < 1e-6)
        .or_else(|| {
            segs.iter()
                .find(|seg| seg.ins.is_empty() && seg.s <= line && seg.e > line)
        })?;
    Some(Selection {
        start: scene.s.max(line),
        end: scene.e,
        scope: Scope::Footage {
            row: scene.cam.max(0) as usize,
        },
    })
}

/// F2.7 － Remove: drop exactly the band.
pub fn press_remove(window: &adw::ApplicationWindow) -> cut_verbs::Outcome {
    let band = ui::selection();
    let cut_ = ui::newest_review_cut();
    let outcome = cut_verbs::remove(band.as_ref(), &cut_.segs);
    report_verb(window, &outcome);
    outcome
}

fn held_scene_under_line() -> Option<Seg> {
    let cut_ = ui::newest_review_cut();
    crate::cut_delete::scene_under(&cut_.segs, ui::preview_playhead()).cloned()
}

/// Print the verb's sentence and honour what the answer says about the band.
pub fn report_verb(window: &adw::ApplicationWindow, outcome: &cut_verbs::Outcome) {
    if let Some(status_line) = ui::find_status(window.upcast_ref()) {
        status_line.set_text(outcome.status());
    }
    match outcome {
        cut_verbs::Outcome::Applied {
            segs,
            keeps_selection,
            ..
        } => {
            if *segs != ui::newest_review_cut().segs {
                ui::set_review_segs(segs);
                let now = ui::newest_review_cut();
                ui::save_insert_cut(&now);
                ui::record_edit(window, &now);
                ui::clamp_effects_to_cut(window);
                // The gap this verb just made is a folded gap now: the badges are built from the cut, so
                // without this the page keeps showing the tape as it was before the Remove.
                cut_folds::refresh_fold_badges(window);
                crate::ui::cut_cameras::refresh_camera_rows(window);
            }
            if !keeps_selection {
                ui::clear_selection(window);
            }
        }
        cut_verbs::Outcome::Refused(_) => {}
    }
}

/// F2.7 ✕ badges: press the ✕ at `badge`, wherever it was drawn, and do what that badge means.
pub fn press_badge(window: &adw::ApplicationWindow, badge: Badge, target: usize) -> String {
    match badge {
        Badge::Selection => {
            ui::clear_selection(window);
            cut_verbs::badge_action(Badge::Selection).to_string()
        }
        Badge::CutLane => {
            let name = ui::newest_review_cut()
                .lanes
                .get(target)
                .map(|lane| lane.name.clone())
                .unwrap_or_default();
            if name.is_empty() {
                return format!("no lane at #{target} \u{2014} nothing to take away");
            }
            cut_folds::press_lane_cross(window, &name)
        }
        Badge::EmptyRow => cut_folds::press_row_cross(window, target).unwrap_or_else(|| {
            "this row still has footage \u{2014} nothing to remove here".to_string()
        }),
        Badge::Effect => ui::remove_effect_at(window, target),
        Badge::GreenBar => remove_scene_at(window, target),
    }
}

/// The green bar's ✕ (`drop that scene`): take ONE scene out of the cut, the same way every other door
/// that edits the cut does it — write the file, spend one Undo step, repaint the things that read the
/// scenes.
pub fn remove_scene_at(window: &adw::ApplicationWindow, index: usize) -> String {
    let mut cut_ = ui::newest_review_cut();
    if index >= cut_.segs.len() {
        return "that scene is gone \u{2014} nothing to remove".to_string();
    }
    let was = cut_.segs.len();
    cut_.segs.remove(index);
    ui::save_insert_cut(&cut_);
    ui::record_edit(window, &cut_);
    ui::clamp_effects_to_cut(window);
    cut_folds::refresh_fold_badges(window);
    crate::ui::cut_cameras::refresh_camera_rows(window);
    ui::refresh_effects_lane(window);
    let said =
        format!("removed \u{2014} 1 segment(s), was {was} \u{2014} \u{21b6} Undo takes it back");
    if let Some(status_line) = ui::find_status(window.upcast_ref()) {
        status_line.set_text(&said);
    }
    said
}

/// The scene ✕'s offer rule, asked of the same threshold the effect ✕ uses: a bar too narrow to carry a ✕
/// clear of its own grips offers none, so a drawn ✕ is always a ✕ the rule will honour.
pub fn scene_cross_live(dur: f64) -> bool {
    crate::fx_band::kill_open(ui::cut_bar_width_px(dur))
}
