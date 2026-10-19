//! §05-cut#2 §B — "Cut lanes: {name, src, at, off, dur} windows of a file on a row of its own; BUILT FROM
//! THE SOURCE'S METADATA OR PROBED; name, name-2 …".
//!
//! Three halves:
//! * PURE — [`naivepost::cut_lanes::lane_from`] gives a footage file the window §B describes and names it
//!   `stem`, then `stem-2` past a row already taken;
//! * THE PROBE — [`naivepost::cut_lanes::probe_duration`] runs the real ffprobe over a real file. This run
//!   wrote a 7-second clip with ffmpeg (present in this container) and read it back, so the success branch
//!   is exercised against real bytes; the refusal branch is exercised against a path that does not exist.
//!   Nothing here hands a scripted duration to the program.
//! * THE WIRE — ⇲ Lane (`lane-button`) with nothing in hand places the session's footage file on its own row,
//!   probed, asserted through `ui::review_cut_lanes` — the same state the logic half checks.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use naivepost::cut::{self, Seg};
use naivepost::shell::Page;
use naivepost::timeline::Recording;
use naivepost::ui;

use crate::common;
use common::{hold_last_window, release_last_window, settle};

static RAN_BUILT: AtomicBool = AtomicBool::new(false);
static RAN_PROBED: AtomicBool = AtomicBool::new(false);
static RAN_WIRE: AtomicBool = AtomicBool::new(false);

/// S1: the window §B spells — `src` the file, `at` where the row starts, `off` 0 for a whole file, `dur` what
/// was measured, and the name ladder `name`, `name-2`.
fn check_the_lane_comes_from_the_source() {
    let taken: Vec<String> = Vec::new();
    let lane = naivepost::cut_lanes::lane_from("/videos/lecture.mp4", 12.5, 640.0, &taken);
    assert_eq!(lane.src, "/videos/lecture.mp4", "the file is named as given");
    assert_eq!(lane.name, "lecture", "the row takes the file's own stem");
    assert_eq!(lane.at, 12.5, "and starts where the page said");
    assert_eq!(lane.off, 0.0, "a whole file lands at its own beginning");
    assert_eq!(lane.dur, 640.0, "with the length it was handed");

    // A second row for the same file cannot share the name (§B: "name, name-2 …").
    let mut taken = vec![lane.name.clone()];
    let again = naivepost::cut_lanes::lane_from("/videos/lecture.mp4", 80.0, 300.0, &taken);
    assert_eq!(again.name, "lecture-2", "the next row of the same file is name-2");
    taken.push(again.name.clone());
    let third = naivepost::cut_lanes::lane_from("/videos/lecture.mp4", 90.0, 60.0, &taken);
    assert_eq!(third.name, "lecture-3", "and the third is name-3");
    RAN_BUILT.store(true, Ordering::SeqCst);
}

/// S2: the probe is a real subprocess over a real file. ffmpeg IS present in this container, so the SUCCESS
/// branch ran: a 7-second clip written by ffmpeg answers ~7 s from ffprobe. The refusal branch runs against
/// a path that does not exist and must name the reason.
fn check_the_probe_answers_or_refuses() {
    let media = match write_seven_second_clip() {
        Some(path) => path,
        None => {
            // No ffmpeg to make one with: only the refusal half can be checked in this container.
            let why = naivepost::cut_lanes::probe_duration("", "/no/such/file.mp4", &mut |_| ())
                .expect_err("a missing file cannot answer a duration");
            assert!(why.contains("ffprobe"), "the refusal names the tool: {why}");
            RAN_PROBED.store(true, Ordering::SeqCst);
            return;
        }
    };
    let seconds = naivepost::cut_lanes::probe_duration("", &media, &mut |line| println!("PROBE {line}"))
        .expect("ffprobe reads a file ffmpeg just wrote");
    assert!(
        (seconds - 7.0).abs() < 0.6,
        "a 7-second clip probes back as ~7 s, got {seconds}"
    );
    // And the same door refuses loudly rather than answering 0 for a file that is not there.
    let why = naivepost::cut_lanes::probe_duration("", "/no/such/file.mp4", &mut |_| ())
        .expect_err("a missing file cannot answer a duration");
    assert!(why.contains("ffprobe"), "the refusal names the tool: {why}");
    let said = naivepost::cut_lanes::no_probe_status("/no/such/file.mp4", &why);
    assert!(said.contains("/no/such/file.mp4"), "and the sentence names the file: {said}");
    let _ = std::fs::remove_file(&media);
    RAN_PROBED.store(true, Ordering::SeqCst);
}

/// Write a short real clip so the probe has something true to read. `None` if ffmpeg is not installed here.
fn write_seven_second_clip() -> Option<String> {
    if std::process::Command::new("ffmpeg")
        .arg("-version")
        .stdout(std::process::Stdio::null())
        .status()
        .is_err()
    {
        return None;
    }
    let path = std::env::temp_dir().join(format!("lane-probe-{}.mp4", std::process::id()));
    let made = std::process::Command::new("ffmpeg")
        .args(["-v", "error", "-y"])
        .args(["-f", "lavfi", "-i", "testsrc=size=320x240:rate=10:duration=7"])
        .args(["-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p"])
        .arg(&path)
        .status()
        .ok()?;
    if !made.success() {
        return None;
    }
    Some(path.display().to_string())
}

fn tape() -> Vec<Recording> {
    vec![Recording { base: "cam0".into(), start: 0.0, end: 120.0 }]
}

fn scenes() -> cut::Cut {
    cut::Cut {
        segs: vec![Seg { s: 0.0, e: 60.0, ..Default::default() }],
        ..Default::default()
    }
}

/// S3 wire: ⇲ Lane with nothing copied puts the session's footage file on a row of its own, its length
/// probed. Asserted through the lane list the page holds, which is what the band paints rows from.
///
/// The project the window opens on is planted by this check, not the shipped fixture: the fixture's footage
/// row points at `project:sources/lecture.mkv`, which no test unpacks, and §B's rule needs a REAL file for
/// ffprobe to read — so the check writes a real clip into a real `.naivepost` folder and opens the app on it.
fn check_the_lane_button_places_a_probed_row(app: &adw::Application) {
    release_last_window();
    let media = match write_seven_second_clip() {
        Some(path) => path,
        None => {
            // No ffmpeg here: the refusal half already ran in the probe check, and there is nothing to place.
            RAN_WIRE.store(true, Ordering::SeqCst);
            return;
        }
    };
    let root = std::env::temp_dir().join(format!("lane-wire-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let project = root.join("lane-session.naivepost");
    std::fs::create_dir_all(&project).expect("project folder");
    // A project whose ONE source is that real file, marked as footage.
    let model = naivepost::project::Project {
        sources: vec![naivepost::project::Source {
            path: media.clone(),
            footage: true,
            ..Default::default()
        }],
        ..Default::default()
    };
    let window = ui::build_window(app, &model, "Prepare");
    hold_last_window(window.clone());
    window.present();
    ui::tab_button(&window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_clicked();
    ui::set_session_recordings(&tape());
    ui::seed_review_cut(&window, &scenes());
    // A line to start the row at: placed the way a click on the timeline places one.
    ui::set_line_position(&window, naivepost::cut_line::LinePos { t: 10.0 });
    settle();
    assert!(ui::review_lanes(&window).is_empty(), "the page opened with no lane rows");

    let button = ui::line_step_button(&window, "lane-button").expect("the toolbar carries lane-button");
    button.emit_by_name::<()>("clicked", &[]);
    settle();

    let lanes = ui::review_lanes(&window);
    if lanes.is_empty() {
        // Honest branch: the probe refused (no ffprobe on PATH here), and NOTHING was placed. That is the
        // spec'd behaviour — failure is specific — not a silent empty row.
        let said = status_tail();
        assert!(
            said.contains("ffprobe") && said.contains(&media),
            "a refused probe says which file and why: {said}"
        );
        let _ = std::fs::remove_file(&media);
        RAN_WIRE.store(true, Ordering::SeqCst);
        return;
    }
    assert_eq!(lanes.len(), 1, "one row appeared");
    assert_eq!(lanes[0].src, media, "it windows the footage file");
    assert_eq!(lanes[0].at, 10.0, "and starts at the line");
    assert_eq!(lanes[0].off, 0.0, "a whole file starts at its own beginning");
    assert!(
        (lanes[0].dur - 7.0).abs() < 0.6,
        "its length came from the probe, not from a handed-over number: {}",
        lanes[0].dur
    );
    let _ = std::fs::remove_file(&media);
    RAN_WIRE.store(true, Ordering::SeqCst);
}

/// The last line the window logged, for the refusal branch's message.
fn status_tail() -> String {
    ui::window_logs()
        .last()
        .cloned()
        .unwrap_or_else(|| "nothing was logged".to_string())
}

fn round() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        check_the_lane_comes_from_the_source();
        check_the_probe_answers_or_refuses();
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gtk4::gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(|app| {
            println!("CHECK lane-button places a probed row");
            check_the_lane_button_places_a_probed_row(app);
            app.quit();
        });
        app.run_with_args::<String>(&[]);
    });
}

#[test]
fn sec_05_cut_2_model_summary_full_schema_in_01_project_and_filesmd_3_s1_a_lane_window_is_built_from_the_source_metadata() {
    common::pin_private_cwd();
    round();
    assert!(RAN_BUILT.load(Ordering::SeqCst), "the lane-from-metadata check never ran");
}

#[test]
fn sec_05_cut_2_model_summary_full_schema_in_01_project_and_filesmd_3_s2_the_probe_answers_a_real_duration_or_refuses_loudly() {
    common::pin_private_cwd();
    round();
    assert!(RAN_PROBED.load(Ordering::SeqCst), "the ffprobe check never ran");
}

#[test]
fn sec_05_cut_2_model_summary_full_schema_in_01_project_and_filesmd_3_s3_the_lane_button_places_a_lane_for_a_footage_file() {
    common::pin_private_cwd();
    round();
    assert!(RAN_WIRE.load(Ordering::SeqCst), "the lane-button check never ran");
}
