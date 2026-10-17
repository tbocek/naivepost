//! F2.7's own three pictures — `spec/img/05-add.png`, `05-split.png` and `05-remove.png`.
//!
//! Each of the spec's images shows ONE state of this flow: a selected region in a gap that Add turns into
//! kept footage; a band inside one clip with a border at each end that Split leaves up on screen; the same
//! band gone again, leaving a hole with a fold badge in it. Before this module all three rendered
//! byte-identical PNGs (and identical to `05-trim`), because `snapshot::run` had no staging arm for these
//! names and every shot fell back to the bare fixture.
//!
//! Nothing is poked here. Every state change goes through the doors the buttons fire
//! (`ui::press_add`, `ui::press_split`, `ui::press_remove` — the bodies wired to `add-button`,
//! `split-button` and `remove-button` at `src/ui/window.rs`), so what the picture shows is what a click
//! writes: the same cut.json, the same Undo step, the same status sentence, the same refreshed fold badges.
//! The sentences come back from [`stage`] so the recipe prints them and a wrong status row shows up in the
//! stdout next to the PNG instead of as an unexplained picture.
//!
//! Spec silence, decided here: the three shots share ONE fixture (two clips over a full take, with a real
//! 30–40 s hole) rather than three different cuts, because the spec's images are the same tape in each of
//! its three states — "before" plus the one verb's result. The bands are aimed so each verb lands on the
//! branch the caption names: Add over the hole (nothing was kept there, so nothing is stolen and the status
//! is the plain one-row sentence), Split inside one clip (both ends are new borders, which is the only way
//! the shot can show two of them), Remove straddling a clip's middle (both remainders are ≥ 0.04 s, so the
//! hole appears between surviving pieces exactly as `05-remove.png` draws it).

use std::path::Path;

use crate::cut;
use crate::cut_select::Surface;
use crate::cut_verbs::Outcome;
use crate::snapshot_face::seed_snapshot_frame;
use crate::ui;

/// The session the three shots film: one take covering the whole tape, which is what gives a head, a hole
/// and a tail anywhere on it. `05-copy`'s arm notes the same requirement for its own picture.
fn tape() -> Vec<crate::timeline::Recording> {
    vec![crate::timeline::Recording {
        base: "session-tape".to_string(),
        start: 0.0,
        end: 120.0,
    }]
}

/// Two kept clips with a real 10 s hole between them: the figure all three of the spec's images draw as
/// "before". The hole is what Add fills, and the second clip's middle is what Remove drops.
fn seeded() -> cut::Cut {
    cut::Cut {
        segs: vec![
            cut::Seg {
                s: 10.0,
                e: 30.0,
                cam: 0,
                ..Default::default()
            },
            cut::Seg {
                s: 40.0,
                e: 60.0,
                cam: 0,
                ..Default::default()
            },
        ],
        ..Default::default()
    }
}

/// Stage `screen`'s subject onto `window` and hand back the sentences the verb printed, newest last, for
/// the caller's `snapshot:` line. Called from `snapshot::run` for `05-add`, `05-split` and `05-remove`.
///
/// One decision worth naming: each shot ends on its verb's OWN sentence, so the status row shows what this
/// item's caption quotes ("added — …", "split at …", "removed … s — …"). The fold badge a Remove earns is
/// NOT pressed here, because F2.11's fold press writes its own status line over the Remove's and the picture
/// would then show the wrong verb's words; the gap itself is what the shot carries, and the badge's presence
/// is printed below so a check can read it (`cut_fold::badges` draws one per gap wide enough to fold).
pub fn stage(window: &adw::ApplicationWindow, screen: &str, dir: &Path) -> Vec<String> {
    ui::set_session_recordings(&tape());
    ui::seed_review_cut(window, &seeded());
    // Deepen the zoom through the toolbar's own + door: at the opening 4 px/s a 7 s band is 28 px and the
    // split borders are hairlines, while §A's ladder stops at 15 px/s where every box here has real width.
    for _ in 0..6 {
        ui::press_zoom(window, true);
    }
    // Badge 1 (the preview plate) needs a frame of THIS session, staged the way the `05-cut` and `05-lanes`
    // shots stage it, through the page's own doors. The base is named as `session-tape`, which is what
    // `row_spans` answers with once `set_session_recordings` above has been called: without a seeded tape it
    // colours rows from the segments (`cam0`) and the panel finds no recording of that name and paints an
    // empty plate. The line has to stand INSIDE kept footage for a frame to be asked for at all
    // (`cut_picture::refresh`), so each arm parks it where its subject is kept.
    seed_snapshot_frame(dir, "session-tape", 0.0);
    ui::cut_picture::set_preview_camera_base("session-tape");
    ui::cut_picture::set_snapshot_dir(dir);

    let mut said: Vec<String> = Vec::new();
    match screen {
        // ＋ Add: a band lying in the 30–40 hole, so the added scene arrives on ground that kept nothing.
        // The red line goes down AFTER the press, on the scene the verb just made: `press_add` builds its
        // snap list from this cut's borders plus wherever the line stands
        // (`cut_verb_press::add_snap_points`), and a line parked inside the band would pull one end onto it
        // — the shot would then show seconds nobody dragged. Once the scene exists the line belongs on it,
        // which is also what gives badge 1 a frame to ask for.
        "05-add" => {
            ui::set_line_position(window, crate::cut_line::LinePos { t: 20.0 });
            ui::draw_selection(window, Surface::PictureRow(0), None, 31.0, 38.0);
            said.push(say(&ui::press_add(window)));
            ui::set_line_position(window, crate::cut_line::LinePos { t: 34.0 });
        }
        // | Split: a band inside the first clip, so BOTH ends are borders that were not scene edges before.
        // The line stays outside the band (at 0 s) for the same reason: a line ON one of the two ends would
        // make that border one that was already there, and the sentence names only borders actually drawn.
        "05-split" => {
            ui::set_line_position(window, crate::cut_line::LinePos { t: 0.0 });
            ui::draw_selection(window, Surface::PictureRow(0), None, 14.0, 24.0);
            said.push(say(&ui::press_split(window)));
            // ...and afterwards it moves back onto the selection, whose right half is what the shot is about:
            // the split-off piece at 14–24, still selected, with the line standing in it.
            ui::set_line_position(window, crate::cut_line::LinePos { t: 16.0 });
        }
        // － Remove: a band across the first clip's middle, so two remainders survive around the hole.
        // Line parked outside as above; Remove snaps nothing, but the same door keeps the three shots'
        // starting state identical, which is the point of staging them from one fixture. After the drop the
        // line goes onto a surviving remainder, so badge 1 shows a frame rather than an empty plate.
        "05-remove" => {
            ui::set_line_position(window, crate::cut_line::LinePos { t: 0.0 });
            ui::draw_selection(window, Surface::PictureRow(0), None, 20.0, 26.0);
            said.push(say(&ui::press_remove(window)));
            ui::set_line_position(window, crate::cut_line::LinePos { t: 15.0 });
            // The gap this just opened, counted through the page's own badge table (`refresh_fold_badges`
            // ran inside `report_verb`): `spec/img/05-remove.png` shows the selection replaced by a hole, and
            // whether that hole wears a − badge is geometry (§B: a gap under `cut_fold::FOLD_MIN_PX` gets
            // none), so the number goes to stdout where a check can read it instead of being squinted for.
            let gaps = ui::fold_gaps(window).len();
            println!("snapshot: 05-remove fold badges over the new gap = {gaps}");
        }
        other => return vec![format!("no F2.7 staging for {other}")],
    }
    // The strip paints from the view's own door; a shot that never queued one would record the pre-press
    // tape. `report_verb` already refreshed the camera rows and the fold badges for the verb that applied.
    // `cut_picture::refresh` runs last and separately because the PREVIEW asks for its frame here: before
    // `present()` the panel measures 0x0 and picks nothing, which is why `05-cut`'s arm calls it twice too.
    let _painted = ui::cut_picture::refresh(window);
    ui::cut_view::repaint_page(window);
    said
}

/// The sentence one answer puts on the status row — the same string `report_verb` prints, so the printed
/// line and the painted row cannot disagree about what the verb claimed.
fn say(outcome: &Outcome) -> String {
    outcome.status().to_string()
}
