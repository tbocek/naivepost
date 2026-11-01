//! F2.8's own picture — `spec/img/05-trim.png`, "Trimming the end of clip 2".

use std::path::Path;

use crate::cut;
use crate::cut_trim::{self, Border};
use crate::snapshot_face::seed_snapshot_frame;
use crate::ui;

pub(crate) fn tape() -> Vec<crate::timeline::Recording> {
    vec![crate::timeline::Recording {
        base: "session-tape".to_string(),
        start: 0.0,
        end: 120.0,
    }]
}

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

const TRIM_TO: f64 = 46.0;
/// Which clip, zero-based: index 1 is the "clip 2" of `trim_status`'s 1-based wording.
const CLIP: usize = 1;

pub fn stage(window: &adw::ApplicationWindow, dir: &Path) -> Vec<String> {
    ui::set_session_recordings(&tape());
    ui::seed_review_cut(&seeded());
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
    let seg = ui::review_cut_segs().remove(CLIP);
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
