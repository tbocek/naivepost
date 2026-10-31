//! F2.7's own three pictures — `spec/img/05-add.png`, `05-split.png` and `05-remove.png`.
//!
//! The three images share ONE fixture (two clips over a full take with a real 30–40 s hole), the same tape in each state: Add over the hole, Split inside one clip (both ends new borders), Remove straddling a clip's middle (both remainders ≥ 0.04 s).
//! Every state change goes through the doors the buttons fire (`ui::press_add`, `ui::press_split`, `ui::press_remove`, wired at `src/ui/window.rs`), so the picture shows what a click writes. The sentences come back from [`stage`] so a wrong status row shows up in stdout next to the PNG.

use std::path::Path;

use crate::cut;
use crate::cut_select::Surface;
use crate::cut_verbs::Outcome;
use crate::snapshot_face::seed_snapshot_frame;
use crate::ui;

/// The session the three shots film: one take covering the whole tape, which gives a head, a hole and a tail anywhere on it.
fn tape() -> Vec<crate::timeline::Recording> {
    vec![crate::timeline::Recording {
        base: "session-tape".to_string(),
        start: 0.0,
        end: 120.0,
    }]
}

/// Two kept clips with a real 10 s hole between them: the "before" all three images draw. Add fills the hole; Remove drops the second clip's middle.
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
/// Each shot ends on its verb's OWN sentence ("added — …", "split at …", "removed … s — …"). The fold badge a Remove earns is NOT pressed: F2.11's fold press would overwrite the status with the wrong verb's words;
/// the badge's presence is printed instead (`cut_fold::badges` draws one per gap wide enough to fold).
pub fn stage(window: &adw::ApplicationWindow, screen: &str, dir: &Path) -> Vec<String> {
    ui::set_session_recordings(&tape());
    ui::seed_review_cut(window, &seeded());
    // Deepen the zoom through the toolbar's own + door: at 4 px/s the split borders are hairlines; §A's ladder stops at 15 px/s where every box has real width.
    for _ in 0..6 {
        ui::press_zoom(window, true);
    }
    // Badge 1 (the preview plate) needs a frame of THIS session, staged like the `05-cut` and `05-lanes` shots. The base is `session-tape`, which `row_spans` answers with after `set_session_recordings`;
    // otherwise the panel finds no recording and paints an empty plate. The line must stand INSIDE kept footage for a frame to be asked for (`cut_picture::refresh`), so each arm parks it where its subject is kept.
    seed_snapshot_frame(dir, "session-tape", 0.0);
    ui::cut_picture::set_preview_camera_base("session-tape");
    ui::cut_picture::set_snapshot_dir(dir);

    let mut said: Vec<String> = Vec::new();
    match screen {
        // ＋ Add: a band lying in the 30–40 hole, so the added scene arrives on ground that kept nothing.
        // The red line goes down AFTER the press, on the scene the verb made: `press_add` snaps to this cut's borders plus the line (`cut_verb_press::add_snap_points`), so a line parked inside the band would pull an end onto it.
        // Once the scene exists the line belongs on it, which also gives badge 1 a frame.
        "05-add" => {
            ui::set_line_position(window, crate::cut_line::LinePos { t: 20.0 });
            ui::draw_selection(window, Surface::PictureRow(0), None, 31.0, 38.0);
            said.push(say(&ui::press_add(window)));
            ui::set_line_position(window, crate::cut_line::LinePos { t: 34.0 });
        }
        // | Split: a band inside the first clip, so BOTH ends are new borders. The line stays outside the band (at 0 s): a line ON an end would make that border one that was already there.
        "05-split" => {
            ui::set_line_position(window, crate::cut_line::LinePos { t: 0.0 });
            ui::draw_selection(window, Surface::PictureRow(0), None, 14.0, 24.0);
            said.push(say(&ui::press_split(window)));
            // ...afterwards it moves onto the selection's right half: the split-off piece at 14–24, still selected.
            ui::set_line_position(window, crate::cut_line::LinePos { t: 16.0 });
        }
        // － Remove: a band across the first clip's middle, so two remainders survive around the hole. Line parked outside as above, keeping the three shots' starting state identical; after the drop it goes onto a remainder so badge 1 shows a frame.
        "05-remove" => {
            ui::set_line_position(window, crate::cut_line::LinePos { t: 0.0 });
            ui::draw_selection(window, Surface::PictureRow(0), None, 20.0, 26.0);
            said.push(say(&ui::press_remove(window)));
            ui::set_line_position(window, crate::cut_line::LinePos { t: 15.0 });
            // The gap this opened, counted through the page's own badge table (`refresh_fold_badges` ran inside `report_verb`): whether it wears a − badge is geometry (§B: under `cut_fold::FOLD_MIN_PX` gets none), so the number goes to stdout.
            let gaps = ui::fold_gaps(window).len();
            println!("snapshot: 05-remove fold badges over the new gap = {gaps}");
        }
        other => return vec![format!("no F2.7 staging for {other}")],
    }
    // The strip paints from the view's own door; `report_verb` already refreshed the camera rows and fold badges. `cut_picture::refresh` runs last because the PREVIEW asks for its frame here: before `present()` the panel measures 0x0 (as in `05-cut`'s arm).
    let _painted = ui::cut_picture::refresh(window);
    ui::cut_view::repaint_page(window);
    said
}

/// The sentence one answer puts on the status row, the same string `report_verb` prints.
fn say(outcome: &Outcome) -> String {
    outcome.status().to_string()
}
