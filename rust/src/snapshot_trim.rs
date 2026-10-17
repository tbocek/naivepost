//! F2.8's own picture — `spec/img/05-trim.png`, "Trimming the end of clip 2".
//!
//! The spec's image is one state: two kept clips, and clip 2's RIGHT border dragged inward so a gap opened
//! where its tail was, with the status row carrying the sentence S1 says a trim prints
//! ("clip N: a – b (X s)"). Before this module `just snapshot 05-trim` rendered the bare fixture — the name
//! reached `screen_page` but no staging arm matched it, so it came out byte-identical to every other Cut-page
//! shot that had no arm of its own.
//!
//! Nothing is poked here. The border moves through `cut_trim_press::press_border_move`, which is exactly what
//! the strip's own left-button drag calls (`src/ui/window_trim_strip.rs`, `connect_drag_update` /
//! `connect_drag_end`), in the same order the handler uses: `begin_move` at the press, the first write with
//! `first: true` so the gesture spends its ONE Undo step, later ticks with `false`, then `end_move` +
//! `end_release` at the release, which is where §7 lets the file be written again. So the picture shows what
//! a hand dragging a border leaves behind: the clamped edge, the same cut.json, the same status sentence.
//!
//! Spec silence, decided here: the shot drags clip 2's END rather than its start, because that is the caption
//! ("after dragging the end of clip 2 left"), and it stops well clear of both neighbours so the merge branch
//! does NOT fire — a joined scene would show one wide bar, and the picture is about the gap a trim opens. The
//! lane from the `05-cut` staging stays in the fixture because §F2.8's strip needs a recorder band AND a wave
//! strip to have both rows the spec's image draws.

use std::path::Path;

use crate::cut;
use crate::cut_trim::{self, Border};
use crate::snapshot_face::seed_snapshot_frame;
use crate::ui;

/// The session the shot films: one take over the whole tape, which is what gives the trimmed tail somewhere to
/// reopen into. Same requirement the `05-copy` and F2.7 arms note for their pictures.
fn tape() -> Vec<crate::timeline::Recording> {
    vec![crate::timeline::Recording {
        base: "session-tape".to_string(),
        start: 0.0,
        end: 120.0,
    }]
}

/// The lane's file has to exist on disk for its plate to carry a channel layout — `cut_lanes::channels_of`
/// asks ffprobe and answers `None` for an absent file, which would leave the recorder row a bare name where
/// the spec's strip shows a labelled band. A silent mono wav is the smallest thing that really is mono, and it
/// goes where `layout::resolve` points from the process cwd, which is where the plate looks (the `05-lanes`
/// arm makes the same call for the same reason).
fn write_lane_file(_dir: &Path) {
    let root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let wav = crate::layout::resolve(
        &root,
        &crate::startup::session_dir(&root),
        "project:sources/mic-lav.wav",
    );
    if let Some(folder) = wav.parent() {
        let _ = std::fs::create_dir_all(folder);
    }
    let _ = crate::snapshot::write_silence_wav(&wav, 8000, 8000 * 45);
}

/// The "before" figure: two clips with a real hole between them, plus the separate recording whose band and
/// wave strip the spec's strip draws under the bars.
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
        lanes: vec![cut::Lane {
            name: "mic-lav".to_string(),
            src: "project:sources/mic-lav.wav".to_string(),
            at: 12.0,
            off: 0.0,
            dur: 46.0,
        }],
        ..Default::default()
    }
}

/// Clip 2's end, dragged from 60 s to here. Chosen so the moved border is unmistakable at any zoom the strip
/// settles on (20 s of travel) while staying 14 s clear of the next clip's start and above the ±1 s floor, so
/// neither the clamp nor the 0.04 s merge touches the answer the shot is judged by.
const TRIM_TO: f64 = 46.0;
/// Which clip, zero-based: index 1 is the "clip 2" of `trim_status`'s 1-based wording.
const CLIP: usize = 1;

/// Stage the trim onto `window` and hand back the sentences the drag printed, newest last, for the caller's
/// `snapshot:` line. Called from `snapshot::run` for `05-trim`.
pub fn stage(window: &adw::ApplicationWindow, dir: &Path) -> Vec<String> {
    ui::set_session_recordings(&tape());
    ui::seed_review_cut(window, &seeded());
    write_lane_file(dir);
    // Deepen the zoom through the toolbar's own + door, as the other Cut-page shots do: at the opening 4 px/s
    // the 10 s hole is 40 px and a trimmed edge reads as nothing. Six steps lands on §A's ladder stop.
    for _ in 0..6 {
        ui::press_zoom(window, true);
    }
    seed_snapshot_frame(dir, "session-tape", 0.0);
    ui::cut_picture::set_preview_camera_base("session-tape");
    ui::cut_picture::set_snapshot_dir(dir);
    // The red line goes down INSIDE clip 2 before the drag: the panel asks for a frame wherever the line
    // stands, and with the line at 0 s it sits in the head gap where the cut keeps nothing, so badge 1 painted
    // an empty plate. On the trimmed clip it shows the footage the trim kept — which is S1's other half, the
    // picture following the edge.
    ui::set_line_position(window, crate::cut_line::LinePos { t: 44.0 });

    let mut said: Vec<String> = Vec::new();
    // The press: the strip's own begin, so the hold starts with no Undo step spent and §7's one-step rule is
    // the handler's, not a shortcut around it.
    ui::cut_trim_press::begin_move();
    // The first WRITE spends the single step (`first: true`), exactly as the strip's first un-throttled
    // `drag-update` does; the second call stands for the ticks after it. Both go through the door that also
    // clamps the edge, refreshes what reads the cut, scrubs the picture to the landed edge and prints the
    // sentence — so the status row in this PNG is the app's own words about its own edit.
    ui::cut_trim_press::press_border_move(window, CLIP, Border::End, TRIM_TO, true);
    let landed = ui::cut_trim_press::press_border_move(window, CLIP, Border::End, TRIM_TO, false);
    // The release: refold what the drag opened (nothing did — a trim opens no fold) and let the writer run
    // again, which is where §7 puts a trim into cut.json.
    ui::cut_trim_press::end_move(window);
    ui::cut_trim_press::end_release();
    // The sentence the shot is judged by, recomputed from the cut the drag LEFT rather than echoed from the
    // call above: if the clamp moved the edge somewhere else, this says where it really landed.
    let seg = ui::review_cut_segs(window).remove(CLIP);
    said.push(cut_trim::trim_status(CLIP + 1, seg.s, seg.e));
    println!(
        "snapshot: 05-trim dragged clip {}'s end border to {TRIM_TO} s, it landed at {landed} s",
        CLIP + 1
    );
    // And the ink: the panel asks for the frame at the edge, the strip repaints from the trimmed cut, and the
    // camera rows redraw so the green bar ends where the drag left it. The line moves LAST so the frame on the
    // panel is the one at the landed edge, not the one at where the drag started.
    ui::set_line_position(window, crate::cut_line::LinePos { t: landed - 1.0 });
    let _painted = ui::cut_picture::refresh(window);
    ui::cut_view::repaint_page(window);
    ui::refresh_camera_rows(window);
    said
}
