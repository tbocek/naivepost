//! F2.10's own picture — `spec/img/05-rows.png`: TWO recordings running at the same time, one row each.
//!
//! No fixture on disk carries this figure: the demo project's `cut/cut.json` is ONE camera in three clips,
//! so a shot of it can only ever draw one row. The spec's caption says as much about its own screenshot ("the
//! lecture has no simultaneous recordings, so the second file was shifted 19 s back in a copy of the project
//! and the cut cleared"), which is exactly what [`stage`] lays out here instead — in a copy of the page's
//! state, not on disk.
//!
//! Nothing is poked. Every state change goes through the doors the page itself uses (`ui::set_session_recordings`,
//! `ui::seed_review_cut`, `ui::set_line_position`, `ui::press_watch_row`), and the strings returned are read
//! BACK OFF THE WIDGETS the page built, so what the recipe prints and what the PNG shows is what the app's own
//! rules say — a plate that reads `row 2` in this shot is a real bug in the row list, not a staging shortcut
//! papered over by labelling a button.
//!
//! The spec's numbered badges and where each comes from:
//! * **1** camera 1 (17-25-06, 0:00–0:37) — the first seeded recording, plate read off `watch-row-1`.
//! * **2** camera 2 (17-25-45) with its shift correction `−19.00 s` in the name plate — the second seeded
//!   recording, whose base carries the stamp and whose `shift` entry carries the correction; the plate is
//!   `cut_cam::name_plate`'s own string, read off `watch-row-2`.
//! * **3** the overlap 0:20–0:37 — falls out of the arithmetic below, coloured onto its own row by
//!   `timeline::rows_for`, which is the greedy colouring §B names.
//! * **4** the dashed outline on the watched row — `press_watch_row` adds the `watched-row` class.
//! * **5** the preview shows the watched row, not the cut — `cut_picture::set_preview_camera_base` names the
//!   WATCHED camera, and a frame of it is planted in the folder Prepare would have extracted into.
//! * **6** the one kept scene, taken from camera 1 — one `Seg` with `cam: 0`.
//! * **7** status "watching camera 2 — the cut shows camera 1 here; ▶ plays the cut" — the sentence
//!   `press_watch_row` returns and writes to the status row.

use std::path::Path;

use adw::prelude::*;
use gtk4 as gtk;

use crate::cut;
use crate::cut_line;
use crate::snapshot_face::seed_snapshot_frame;
use crate::timeline::Recording;
use crate::ui;
use crate::ui::window::find_widget_by_name;

/// Camera 1's file stamp: `spec/05-cut.md`'s badge **1** names the recording "camera 1 (17-25-06, 0:00–0:37)",
/// and §1's footage files carry `YYYY-MM-DD_HH-MM-SS` in their names.
pub const CAM1: &str = "2026-09-16_17-25-06";
/// Camera 2's file stamp — badge **2**'s "camera 2 (17-25-45)". Stamped 39 s after camera 1.
pub const CAM2: &str = "2026-09-16_17-25-45";
/// How far camera 2 was moved back, which is the spec's own staging ("shifted 19 s back"). 39 − 19 = 20, so
/// camera 2 sits at 0:20 on the session clock and badge **3**'s overlap 0:20–0:37 is not a number staged here
/// but the consequence of the two stamps plus the one correction.
pub const CAM2_SHIFT: f64 = -19.0;
/// The FILE STAMP difference between the two cameras, which is the spec's own stamps read as a difference:
/// 17-25-45 sits 39 s after 17-25-06. Camera 2's place on the session clock is that minus the shift.
pub const CAM2_STAMP: f64 = 39.0;
/// The line's second of camera 2's OWN file, which is `timeline::local_at`'s answer for the line below:
/// `25 − 20 + (−19) = −14`. Planted there rather than at 0 because the panel asks by that number and
/// `frame_near` takes the nearest stamp AT OR BEFORE it -- a frame planted at 0 s would be "in the future" as
/// far as the search is concerned, and the plate would stay empty in a headless run where nothing else can
/// supply the session's real extracted frames.
pub const CAM2_FILE_SECOND: f64 = -14.0;
/// The microphone the shot needs a speaker badge and a gutter switch for (badges on S2's row). A separate
/// recording, so its name must be neither camera's: `cut_cameras::row_spans` keeps a span whose base the cut
/// already accounts for, and a lane named after a camera would swallow that camera's row.
pub const MIC: &str = "mic-lav";
/// The line's second: inside the overlap, so the watch sentence is owed (badge **7**) and the preview has a
/// kept scene to convert a second for.
pub const LINE_T: f64 = 25.0;
/// The same second on the PREVIEW's playhead, which is what the band's red line actually paints
/// (`band_scene_for` reads it off the player, not off the line state). Placed through `ui::set_playhead`, the
/// door a click on the tape fires, at the point the layout has settled.
pub const LINE_PLAYHEAD: f64 = 25.0;

/// The two recordings of the figure: camera 1 for its whole 37 s, camera 2 from its shifted start for the
/// same length of tape. Both bases are FILE STAMPS rather than `cam0`/`cam1` on purpose: a row's plate names
/// the recording that row speaks for (prototype `gui/cut.go`'s `rowNameVid` → `v.base`), and the cut's own
/// segments can only ever say `cam<N>`, so a seeded span survives the row list's filter only while the cut
/// cannot account for its base — see `cut_cameras::row_spans`. Naming them `cam0`/`cam1` here would put the
/// second row back to the unnamed `row 2` fallback badge **2** exists to replace.
fn recordings() -> Vec<Recording> {
    vec![
        Recording {
            base: CAM1.to_string(),
            start: 0.0,
            end: 37.0,
        },
        // Where the file sits after the correction (0:39 stamped, 19 s back = 0:20), and the same length of
        // tape camera 1 ran.
        Recording {
            base: CAM2.to_string(),
            start: CAM2_STAMP + CAM2_SHIFT,
            end: CAM2_STAMP + CAM2_SHIFT + 37.0,
        },
    ]
}

/// The cut: ONE kept scene, 0–37, taken from camera 1 (badge **6**), plus the microphone lying under it and
/// camera 2's shift correction. The correction is stored under camera 2's stamp because that is what a hand
/// slide writes (§B: "shift map base→seconds") and it is the key `cut_cam::name_plate` looks up.
fn figure() -> cut::Cut {
    cut::Cut {
        segs: vec![cut::Seg {
            s: 0.0,
            e: 37.0,
            cam: 0,
            ..Default::default()
        }],
        lanes: vec![cut::Lane {
            name: MIC.to_string(),
            src: format!("project:sources/{MIC}.wav"),
            at: 12.0,
            off: 0.0,
            dur: 45.0,
        }],
        shift: [(CAM2.to_string(), CAM2_SHIFT)].into_iter().collect(),
        ..Default::default()
    }
}

/// Stage the two-camera figure onto `window` and hand back what it staged, newest last, for the caller's
/// `snapshot:` line. Called from `snapshot::run` for `05-rows`.
pub fn stage(window: &adw::ApplicationWindow, dir: &Path) -> Vec<String> {
    // Declared BEFORE the seed: `set_session_recordings` marks the session as declared, so the seed below
    // cannot trade these spans for what its segments imply (`ui::cut_folds::SESSION_DECLARED`).
    ui::set_session_recordings(&recordings());
    // The microphone's file has to exist for its plate in the recorders' band to carry a channel layout —
    // `cut_lanes::channels_of` asks ffprobe and answers `None` for an absent file, which leaves the plate a
    // bare name. Same staging the `05-lanes` shot uses: a silent mono wav, written where resolution points.
    let root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let wav = crate::layout::resolve(
        &root,
        &crate::startup::session_dir(&root),
        &format!("project:sources/{MIC}.wav"),
    );
    if let Some(folder) = wav.parent() {
        let _ = std::fs::create_dir_all(folder);
    }
    let _ = crate::snapshot::write_silence_wav(&wav, 8000, 8000 * 45);

    ui::seed_review_cut(window, &figure());
    // The band's red line: the band paints the PREVIEW's playhead (`band_scene_for`), so `set_line_position`
    // alone draws none. Placed through `ui::set_playhead`, the door a click on the tape fires, with the same
    // constant the rest of the figure uses so the picture and the state cannot drift apart.
    ui::set_playhead(window, LINE_PLAYHEAD);
    // Badge **5**: the preview shows the WATCHED row, so the frame planted is camera 2's and the base named
    // to the panel is camera 2's. One frame is enough — `cut_tracks::frame_near` takes the nearest stamp.
    seed_snapshot_frame(dir, CAM2, CAM2_FILE_SECOND);
    // And a frame of camera 1 at the line's second, so the other half of badge 5 -- a click handing the
    // preview back to the cut's own camera -- has a picture to show too.
    seed_snapshot_frame(dir, CAM1, LINE_T);
    ui::cut_picture::set_preview_camera_base(CAM2);
    ui::cut_picture::set_snapshot_dir(dir);
    // The red line inside the overlap, then the watch on row 2 — through the door a click on the plate fires,
    // so the outline, the status sentence and the frame all move the way a click moves them.
    ui::set_line_position(window, cut_line::LinePos { t: LINE_T });
    let watched = ui::press_watch_row(window, 1).unwrap_or_default();
    ui::refresh_camera_rows(window);

    // Read the plates back off the page rather than recomputing them: the printed string is then the label the
    // PNG shows, and a wrong plate shows up in stdout next to the picture instead of as an unexplained pixel.
    let plate = |name: &str| {
        find_widget_by_name(window.upcast_ref(), name)
            .and_then(|widget| widget.downcast::<gtk::Button>().ok())
            .map(|button| button.label().unwrap_or_default().to_string())
            .unwrap_or_else(|| format!("<{name} missing>"))
    };
    vec![
        format!("plate 1 {}", plate("watch-row-1")),
        format!("plate 2 {}", plate("watch-row-2")),
        format!("line {:.2}", LINE_T),
        watched,
    ]
}
