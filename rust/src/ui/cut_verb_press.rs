//! F2.7 (Add, Split, Remove, ⌦) — the page's half: the four doors the buttons and ⌦ fire, what each one
//! writes, and the ✕ badges. Split out of `window.rs`, which is past its size budget.
//!
//! No rule lives here. `crate::cut_verbs` decides what a verb does and what it says; this module reads the
//! page, calls the rule, prints the answer, and then does the four things the rule cannot do from inside a
//! pure function:
//!
//! - **write cut.json** — a verb that changed the segment list is an edit like any other door's, so it is
//!   saved before it is put on the history. A refusal writes NOTHING: not a file, not a history entry.
//! - **refresh the fold badges** — Remove's new gap is a folded gap the moment it exists, and the badges are
//!   drawn from the cut, so a verb that made a gap without refreshing left a page lying about the tape.
//! - **build the snap list** — Add's ends move to word edges, silences, the line and visual cuts within
//!   `P.policy.snapToleranceSeconds`; only the page knows where this session's borders, line and wave edges
//!   are, so only the page can hand the rule the candidates ([`add_snap_points`]).
//! - **take the right half in hand** — a band-less Split hands over what it just cut, and the hand has to be
//!   filled from the NEW segment list, before the band is cleared, which is why it cannot be
//!   `selection(window)` read after the clear (the old code did exactly that and stored `None`).
//!
//! The ✕ badges: `cut_verbs::Badge` names the five places one appears and what each means. [`press_badge`]
//! is the single door all of them fire through, so the enum has one dispatcher and each ✕'s widget only says
//! which badge it is. The green-bar ✕ is the one with no widget of its own yet — the band is cairo-painted,
//! so its ✕ lives in the camera row beside the plate (`scene-cross-<scene>`, built in `cut_cameras`).

use adw::prelude::*;

use crate::cut::Seg;
use crate::cut_select::{Scope, Selection};
use crate::cut_verbs::{self, Badge};
use crate::ui::cut_folds;
use crate::ui::wave_snap;
use crate::ui::window as ui;

// The right half a band-less | Split put in the hand (F2.7 S2). Newest-window slot rule as everywhere else
// on this page; ⧉ Copy reads it when the person asks for the second half of the pair without having to
// select it again.
thread_local! {
    static VERB_HAND: std::cell::RefCell<Option<Selection>> =
        const { std::cell::RefCell::new(None) };
}

/// What a Split left in the hand, newest answer. `None` when nothing has been split at a line, or the hand
/// was already taken.
pub fn verb_hand() -> Option<Selection> {
    VERB_HAND.with(|cell| cell.borrow().clone())
}

/// Put `band` in the hand. A test clears it the same way a paste spends it; no other program code writes
/// it, because a hand nobody split is not a hand.
pub fn take_into_hand(band: Option<Selection>) {
    VERB_HAND.with(|cell| *cell.borrow_mut() = band);
}

/// F2.7 ＋ Add's snap candidates, gathered from live state — the spec's `word edges, silences, line edges,
/// visual cuts`. Three sources, unioned, unsorted and unreached-filtered:
///
/// - every border of this cut ([`ui::newest_review_cut`]'s `s`/`e`): the VISUAL cuts, which is what an end
///   is nearly always aimed at;
/// - the red line ([`ui::preview_playhead`]): the LINE edge, so an end can be squared up to where the
///   person last put the line;
/// - the wave edges [`wave_snap::snap_points`] answers for this band: word edges and silences, whose seam
///   already returns an empty list when no aligner is running (§4.1 puts the points on the waveform
///   instead, and that fallback is the seam a test replaces).
///
/// The 5 s reach (`P.policy.snapToleranceSeconds`,
/// [`crate::cut_verbs::SNAP_TOLERANCE_SECONDS`]) is applied by [`cut_verbs::snap_end`] per end, NOT
/// filtered here: the reach is measured from each end separately, so a point that is 1 s from one end and
/// 30 s from the other must survive the gather and be refused only for the far end.
pub fn add_snap_points(window: &adw::ApplicationWindow, band: Option<&Selection>) -> Vec<f64> {
    let cut_ = ui::newest_review_cut();
    let mut points: Vec<f64> = Vec::new();
    for seg in &cut_.segs {
        points.push(seg.s);
        points.push(seg.e);
    }
    points.push(ui::preview_playhead(window));
    points.extend(wave_snap::snap_points(band));
    points.sort_by(f64::total_cmp);
    points.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
    points
}

/// F2.7 ⌦: a held effect, then a held clip, then the selection, then the scene under the line — in that
/// order, decided by [`cut_verbs::delete_verb`]. The two "held" values are the page's own seams, set when a
/// person actually picks something up.
pub fn press_delete_key(window: &adw::ApplicationWindow) -> cut_verbs::Outcome {
    let segs = ui::newest_review_cut().segs;
    let band = ui::selection(window);
    let scene = held_scene_under_line(window);
    let outcome = cut_verbs::delete_verb(
        ui::held_effect().as_ref(),
        ui::held_segment().as_ref(),
        band.as_ref(),
        scene.as_ref(),
        &segs,
    );
    report_verb(window, &outcome);
    outcome
}

/// F2.7 ＋ Add: keep the band as scenes, both ends snapped to [`add_snap_points`].
pub fn press_add(window: &adw::ApplicationWindow) -> cut_verbs::Outcome {
    let band = ui::selection(window);
    let cut_ = ui::newest_review_cut();
    let points = add_snap_points(window, band.as_ref());
    let outcome = cut_verbs::add(band.as_ref(), &cut_.segs, &points);
    report_verb(window, &outcome);
    outcome
}

/// F2.7 | Split: a border at each end of the band, or one at the red line when there is none, in which
/// case the right half of what was cut goes into the hand.
pub fn press_split(window: &adw::ApplicationWindow) -> cut_verbs::Outcome {
    let band = ui::selection(window);
    let cut_ = ui::newest_review_cut();
    let line = ui::preview_playhead(window);
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

/// The right half a border drawn at `line` produced: the footage scene that now starts there. `None` when
/// the split fell on ground with nothing after it, which is a hand of nothing and so no hand at all.
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
    let band = ui::selection(window);
    let cut_ = ui::newest_review_cut();
    let outcome = cut_verbs::remove(band.as_ref(), &cut_.segs);
    report_verb(window, &outcome);
    outcome
}

/// The scene under the red line, or `None` where the cut keeps nothing — [`crate::cut_delete::scene_under`]'s
/// answer, so ⌦ asks the same question every other ⌦-adjacent rule asks.
fn held_scene_under_line(window: &adw::ApplicationWindow) -> Option<Seg> {
    let cut_ = ui::newest_review_cut();
    crate::cut_delete::scene_under(&cut_.segs, ui::preview_playhead(window)).cloned()
}

/// Print the verb's sentence and honour what the answer says about the band. A refusal changes nothing at
/// all — no status-free silence, no file write and no edit either, which is the point of refusing.
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
            // Only a verb that produced segments rewrites the cut; ⌦ on a held card reports through the
            // status and leaves the list to the round that owns removal of inserts.
            if !segs.is_empty() {
                ui::set_review_segs(window, segs);
                let now = ui::newest_review_cut();
                // cut.json FIRST, then the history: every other door that edits the cut (`remove_effect_at`,
                // the fold crosses, insert) writes the file and then records the step, and a verb that only
                // recorded would vanish on reopen while ↶ still pretended it was there.
                ui::save_insert_cut(&now);
                ui::record_edit(window, &now);
                // F3.12: the segments moved, so every effect chosen against the old seconds is now pointed
                // at whatever those seconds became.
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

/// F2.7 ✕ badges: press the ✕ at `badge`, wherever it was drawn, and do what that badge means. Every ✕ on
/// the Cut page fires through here, so `cut_verbs::Badge` has one dispatcher and a widget only ever has to
/// say which badge it is.
///
/// `target` means what the badge needs: the scene index for a green bar, the row number for an empty row,
/// the lane's place in `Cut::lanes` for a cut lane, the record's index in `Cut::fx` for an effect. The
/// selection's ✕ needs nothing and ignores it — that one is F2.6's ✕, which clears and removes nothing.
pub fn press_badge(window: &adw::ApplicationWindow, badge: Badge, target: usize) -> String {
    match badge {
        // F2.6 S2's own button prints nothing of its own; the readout and the greying are the answer.
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
/// scenes. Shares `delete_verb`'s footage wording so a scene removed by ✕ and one removed by ⌦ on a
/// selection report the same shape of sentence.
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
/// (`fx_band::KILL_MIN_PX`, `effects.killMinPx`; the width is the page's own `cut_bar_width_px`.)
pub fn scene_cross_live(dur: f64) -> bool {
    crate::fx_band::kill_open(ui::cut_bar_width_px(dur))
}
