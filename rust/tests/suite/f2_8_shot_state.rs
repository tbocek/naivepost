//! F2.8's picture, pinned by state instead of by eye.
//!
//! `spec/img/05-trim.png` shows one state: clip 2's end border dragged inward, with the status row carrying
//! S1's trim sentence. The completion check that reopened this item found `just snapshot 05-trim` rendering
//! the bare fixture — no staged trim, no dragged border, no sentence — so the staging arm in
//! `snapshot_trim` is the fix and this file is what keeps it fixed: each check calls the same `stage` the
//! recipe calls (or the strip's own drag gesture) and asserts on the cut, the status row and the frame the
//! panel asked for. A staging arm that quietly fell back to the fixture would fail here.
//!
//! One application, one `connect_activate` (`tests/suite/f2_7_shot_states.rs`'s shape): a second `adw::Application`
//! per process is refused by GIO, and a panic inside `connect_activate` escapes into GTK's C loop and aborts
//! with no message, so the checks run under `catch_unwind` inside the single activation and their failures are
//! asserted outside it.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use naivepost::cut::{Cut, Seg};
use naivepost::cut_trim::{self, Border};
use naivepost::shell::Page;
use naivepost::ui;

use crate::common;
use common::{fixture_dir, hold_last_window, release_last_window, settle, status_text};

static RAN_SHOT: AtomicBool = AtomicBool::new(false);
static RAN_DRAG: AtomicBool = AtomicBool::new(false);
static RAN_LINE: AtomicBool = AtomicBool::new(false);

/// The fixture the shot starts from: two clips with a hole, matching `snapshot_trim::seeded`.
fn fixture_cut() -> Cut {
    Cut {
        segs: vec![
            Seg {
                s: 10.0,
                e: 30.0,
                cam: 0,
                ..Default::default()
            },
            Seg {
                s: 40.0,
                e: 60.0,
                cam: 0,
                ..Default::default()
            },
        ],
        ..Default::default()
    }
}

/// A Cut-page window on that fixture, in its own temp folder so the drag has a real `cut/cut.json` to write.
fn shot_window(app: &adw::Application, slug: &str) -> adw::ApplicationWindow {
    let root = std::env::temp_dir().join(format!(
        "naivepost-f28-shots-{}-{slug}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let session = naivepost::startup::session_dir(&root);
    std::fs::create_dir_all(session.join("cut")).expect("a session cut folder");
    std::fs::create_dir_all(session.join("prepare")).expect("a session prepare folder");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");
    release_last_window();
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    hold_last_window(window.clone());
    window.present();
    ui::tab_button(&window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_by_name::<()>("clicked", &[]);
    ui::reopen_history_on(&window, &fixture_cut());
    ui::seed_review_cut(&window, &fixture_cut());
    // The shot's tape: the trimmed tail has to reopen into filmed material, and the recorded length is what
    // sets the strip's zoom floor (see [`strip_pps_now`]). Seeded BEFORE any pixel maths, as the program does.
    ui::set_session_recordings(&tape());
    ui::refresh_camera_rows(&window);
    settle();
    window
}

fn seg_at(window: &adw::ApplicationWindow, index: usize) -> Seg {
    ui::review_cut_segs(window)
        .into_iter()
        .nth(index)
        .unwrap_or_else(|| panic!("the cut lost clip #{index}"))
}

/// Total kept footage seconds — a trim of an END shortens this by exactly what was dropped.
fn total(segs: &[Seg]) -> f64 {
    segs.iter()
        .filter(|seg| seg.ins.is_empty() && seg.e > seg.s)
        .map(|seg| seg.e - seg.s)
        .sum()
}

/// Fire the strip's own left-button trim drag the way GTK does: press on the drawn border's pixel, move to the
/// target's pixel, release there. Pixels come from the strip's own geometry (`clip_boxes` at `strip_pps`), so
/// the hand grabs the pixel the paint drew.
///
/// The offsets are small ON PURPOSE. A `drag-update`/`drag-end` carries the offset from the press, and the strip
/// converts it with `x / strip_pps()` — it treats the number as an absolute second (that is how its own
/// throttled ticks work, one tick per mouse-move event). So a test that asks for 14 s of travel in ONE update
/// gets 14 × pps seconds asked for, which the clamp stops at the next clip's start + 1 s. Real hands send many
/// updates; this walks the border in [`STEP`] -second ticks, which is what a hand actually does.
fn drag_border(
    window: &adw::ApplicationWindow,
    index: usize,
    border: Border,
    to_seconds: f64,
) {
    let trimmer = ui::trim_gesture(window).expect("the Cut page wired a trim drag");
    let pps = deepened_pps(window);
    let boxes = cut_trim::clip_boxes(&ui::review_cut_segs(window), pps);
    let grab_px = if matches!(border, Border::End) {
        boxes[index].x + boxes[index].w
    } else {
        boxes[index].x
    };
    assert!(
        cut_trim::border_at(&boxes, grab_px) == Some((index, border)),
        "the press must land on clip {}'s {border:?} border, or this proves nothing",
        index + 1
    );
    let from_seconds = if matches!(border, Border::End) {
        boxes[index].x + boxes[index].w
    } else {
        boxes[index].x
    } / pps;
    // The drag's own geometry must be readable before it is fired: the grab pixel has to fall inside §S1's
    // `EDGE_GRAB_PX` reach of the border this test means, or the press would take a different clip (or none) and
    // prove nothing about the one named here.
    assert!(
        (from_seconds - if matches!(border, Border::End) { 60.0 } else { 40.0 }).abs() < 0.5,
        "clip {}'s {border:?} reads at {from_seconds} s at {pps} px/s; the strip is not painted where this \"
         test thinks it is",
        index + 1
    );
    trimmer.emit_by_name::<()>("drag-begin", &[&grab_px, &0.0f64]);
    settle();
    // The strip converts an update's number as an ABSOLUTE second (`x / strip_pps()`), so one tick can in
    // principle ask for the whole travel — but §S1's 90 ms scrub gate swallows every tick after the first, and a
    // synthetic burst lands all of them inside one millisecond, so only the FIRST would ever write. A real hand
    // sends its moves spread over time; this walks the border in [`STEP`]-second ticks with the gate waited out
    // between them, which is the same sequence of writes the handler makes.
    let target = naivepost::cut_trim::clamp_edge(
        &ui::review_cut_segs(window),
        index,
        border,
        to_seconds,
        0.0,
        // The wall the strip hands the rule (`window_trim_strip::TRACK_REC_END`, crate-private): a trim needs
        // some end rather than none, and 3600 s sits far past this fixture's tape, so the answer comes from the
        // clip and its neighbours alone.
        3600.0,
    );
    println!(
        "DRAG setup: pps {pps}, grabbed {border:?} at {from_seconds} s, aimed {to_seconds} s, clamp says {target}"
    );
    assert!(
        (target - to_seconds).abs() < 1e-9,
        "§S1's clamp moved this aim on its own ({to_seconds} -> {target}), so the test's target is wrong, \"
         not the app's behaviour"
    );
    let direction = if to_seconds >= from_seconds { STEP } else { -STEP };
    let mut here = from_seconds;
    while (here - to_seconds).abs() > STEP.abs() {
        here += direction;
        wait_past_scrub_gate();
        // The strip reads an update's number as an absolute second (`x / strip_pps()`), so the value handed to
        // the signal is that second times pps — NOT the offset from the press. That is how its own throttled
        // ticks are built, and it is what the release below does too; passing an offset here made every tick ask
        // for `from + to` seconds, which the clamp then stopped at the neighbour's start + 1 s.
        trimmer.emit_by_name::<()>("drag-update", &[&(here * pps), &0.0f64]);
        settle();
    }
    wait_past_scrub_gate();
    trimmer.emit_by_name::<()>("drag-update", &[&(to_seconds * pps), &0.0f64]);
    settle();
    trimmer.emit_by_name::<()>("drag-end", &[&(to_seconds * pps), &0.0f64]);
    settle();
    let landed = ui::review_cut_segs(window)
        .into_iter()
        .nth(index)
        .map(|seg| match border {
            Border::End => seg.e,
            Border::Start => seg.s,
        })
        .unwrap_or(f64::NAN);
    assert!(
        (landed - to_seconds).abs() < 0.01,
        "the strip left clip {}'s {border:?} at {landed}, not where the hand aimed ({to_seconds})",
        index + 1
    );
}

/// How far one simulated mouse-move tick takes the border — a whole second, so a short drag stays a handful of
/// events while each tick still asks for a second the strip is painted at.
const STEP: f64 = 1.0;

/// The snapshot recipe stages its trim by calling the same door with these numbers; a test that fires the real
/// gesture must aim at the same clip and the same second or it proves a different edit than the picture shows.
const SHOT_CLIP: usize = 1;
const SHOT_TO: f64 = 46.0;

/// The strip's pixels-per-second, read AFTER the toolbar's own + door has been walked: `cut_view::strip_pps`
/// is `live_pps` over the declared strip width, so it answers whatever zoom the page is really painted at —
/// which is the only number that makes a pixel target mean what the test aimed at. At the fit-only floor (a
/// 120 s take in a 480 px strip is ~3.8 px/s) one update's offset can ask for seconds the clamp then refuses,
/// and the drag proves nothing about where it was pointed.
fn deepened_pps(window: &adw::ApplicationWindow) -> f64 {
    for _ in 0..6 {
        ui::press_zoom(window, true);
    }
    settle();
    let pps = ui::cut_view::strip_pps();
    assert!(pps > 5.0, "the + ladder left the strip at {pps} px/s");
    pps
}

/// The tape every check films: 120 s, long enough that the strip's floor leaves real room to drag (a 50 s take
/// fits the 480 px strip at ~9.6 px/s and clamps any pull past the fit).
fn tape() -> Vec<naivepost::timeline::Recording> {
    vec![naivepost::timeline::Recording {
        base: "session-tape".to_string(),
        start: 0.0,
        end: 120.0,
    }]
}

/// Wait until wall-clock time has passed §S1's scrub throttle again, so the next emitted tick is one the strip
/// will actually write rather than swallow. The gate is `cut_trim::scrubs` on real milliseconds (90 ms), so the
/// condition asked for is "at least SCRUB_MS since this drag last wrote" — checked, waited for briefly, failed
/// with a deadline rather than guessed at. A test never waits a fixed number of turns here because the thing it
/// waits on is a clock, not a queue.
fn wait_past_scrub_gate() {
    let started = std::time::Instant::now();
    loop {
        if started.elapsed() >= std::time::Duration::from_millis(cut_trim::SCRUB_MS + 20) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

/// The staged shot: the recipe's own sentence, the trimmed edge it claims, and the painted row agreeing.
fn check_shot(app: &adw::Application) {
    let window = shot_window(app, "shot");
    let before = total(&ui::review_cut_segs(&window));
    assert_eq!(before, 40.0, "the fixture keeps two 20 s clips");
    let said = naivepost::snapshot_trim::stage(&window, &fixture_dir());
    let line = said.last().cloned().unwrap_or_default();
    println!("TRIM shot status: {line}");
    assert!(
        line.starts_with("clip ") && line.contains(" s)"),
        "the shot's status row is §F2.8's trim_status form, got {line:?}"
    );
    assert_eq!(
        status_text(&window),
        line,
        "and the painted status row says what the stage returned"
    );
    let second = seg_at(&window, 1);
    assert!(
        (second.s - 40.0).abs() < 1e-9,
        "the START stayed where it was; only the end moved: {second:?}"
    );
    assert!(
        second.e < 60.0 - 1.0 && second.e > second.s + 1.0,
        "clip 2's end was pulled in from 60 s and stayed above the 1 s floor: {second:?}"
    );
    let first = seg_at(&window, 0);
    assert!(
        (first.s - 10.0).abs() < 1e-9 && (first.e - 30.0).abs() < 1e-9,
        "clip 1 is untouched by a drag on clip 2: {first:?}"
    );
    let now = total(&ui::review_cut_segs(&window));
    assert!(
        (now - (before - 14.0)).abs() < 0.01,
        "the trim dropped exactly the tail it uncovered: {before} -> {now}"
    );
    // S1's other half: the picture landed on the edge, so the panel asked for the frame AT it.
    let asked = naivepost::ui::cut_picture::preview_asked_for_test();
    println!("TRIM shot preview asked: {asked:?}");
    assert!(asked.is_some(), "the panel asked for a frame after the trim");
    RAN_SHOT.store(true, Ordering::SeqCst);
    window.close();
}

/// The wire: the strip's own gesture, fired the way GTK does, trims the same edge and prints the same sentence.
fn check_real_drag(app: &adw::Application) {
    let window = shot_window(app, "drag");
    drag_border(&window, SHOT_CLIP, Border::End, SHOT_TO);
    let second = seg_at(&window, SHOT_CLIP);
    println!("DRAG left clip {} at {:?}", SHOT_CLIP + 1, (second.s, second.e));
    assert!(
        (second.e - SHOT_TO).abs() < 0.01,
        "the drag landed on the aimed second: {second:?}"
    );
    let said = status_text(&window);
    println!("DRAG status: {said}");
    assert!(
        said.starts_with("clip 2:") && said.contains(" s)"),
        "the drag printed the trim sentence on the status row, got {said:?}"
    );
    let asked = naivepost::ui::cut_picture::preview_asked_for_test()
        .expect("the page asks for a frame of the session it watches");
    // S1's "the picture lands on the edge" is a claim about a PLAYING preview: `scrub_picture` refuses when the
    // transport is not running and no child player exists (`cut_trim_press::scrub_picture`'s two guards), which
    // is why a headless snapshot cannot show it. What this can prove is the door the scrub aims at — the panel's
    // own ask, converted to the file's seconds by `timeline::local_at` — so the check reads that instead of
    // pretending a seek happened with nothing playing.
    assert_eq!(
        asked.0, "session-tape",
        "the panel is watching the session the trim trimmed"
    );
    RAN_DRAG.store(true, Ordering::SeqCst);
    window.close();
}

/// S3: a right-drag that takes a border trims it and never moves the red line.
fn check_line_never_moves(app: &adw::Application) {
    let window = shot_window(app, "line");
    ui::set_line_position(&window, naivepost::cut_line::LinePos { t: 25.0 });
    let mover = ui::move_gesture(&window).expect("the Cut page wired a move drag");
    let pps = deepened_pps(&window);
    let boxes = cut_trim::clip_boxes(&ui::review_cut_segs(&window), pps);
    // The press goes on the DRAWN end border (60 s here, not the fixture's 40 s start): §S1's "either button
    // drags that border" is answered from the press position, so pressing mid-clip would take the slide path
    // instead and this check would say nothing about the border rule.
    let grab_px = boxes[SHOT_CLIP].x + boxes[SHOT_CLIP].w;
    assert!(
        cut_trim::border_at(&boxes, grab_px) == Some((SHOT_CLIP, Border::End)),
        "the right press must land on clip {}'s end border", SHOT_CLIP + 1
    );
    mover.emit_by_name::<()>("drag-begin", &[&grab_px, &0.0f64]);
    settle();
    wait_past_scrub_gate();
    // ONE tick, and it is the release that lands the edge: GTK hands `drag-end` the OFFSET from the press, so
    // `-2 s` of travel from a border drawn at 60 s means 58 s. A second write would be a second gesture; §S1's
    // "one Undo per drag" is exactly why the update only scrubs and the end writes.
    let travel_px = -2.0 * pps;
    mover.emit_by_name::<()>("drag-update", &[&travel_px, &0.0f64]);
    settle();
    mover.emit_by_name::<()>("drag-end", &[&travel_px, &0.0f64]);
    settle();
    // The release lands the final edge. Note what GTK actually hands `drag-end`: the OFFSET from the press, not
    // an absolute second (the strip's own comment on `connect_drag_end` says as much) — so this is `-2 s` of
    // travel from a press drawn at 60 s, i.e. 58 s. Asking for an absolute here would ask for 58 + 60 seconds and
    // the clamp would stop it at the tape's end, which is the bug this line used to have.
    let travel_px = -2.0 * pps;
    mover.emit_by_name::<()>("drag-end", &[&travel_px, &0.0f64]);
    settle();
    let second = seg_at(&window, SHOT_CLIP);
    println!(
        "RIGHT DRAG left clip {} at {:?}, line at {}",
        SHOT_CLIP + 1,
        (second.s, second.e),
        ui::line_position(&window).t
    );
    assert!(
        (second.e - 58.0).abs() < 0.01,
        "a right press on a border trims it by exactly the travel (§S1's either button): {second:?}"
    );
    assert!(
        (ui::line_position(&window).t - 25.0).abs() < 1e-9,
        "§S3: a right click never moves the line"
    );
    RAN_LINE.store(true, Ordering::SeqCst);
    window.close();
}

fn round() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    let failures = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let log = failures.clone();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        let log = log.clone();
        app.connect_activate(move |app| {
            for (name, check) in [
                ("shot", check_shot as fn(&adw::Application)),
                ("drag", check_real_drag),
                ("line", check_line_never_moves),
            ] {
                if let Err(panic) =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| check(app)))
                {
                    let why = panic
                        .downcast_ref::<String>()
                        .cloned()
                        .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
                        .unwrap_or_else(|| "a non-string panic".to_string());
                    log.lock().unwrap().push(format!("{name}: {why}"));
                } else {
                    match name {
                        "shot" => RAN_SHOT.store(true, Ordering::SeqCst),
                        "drag" => RAN_DRAG.store(true, Ordering::SeqCst),
                        _ => RAN_LINE.store(true, Ordering::SeqCst),
                    }
                }
            }
            app.quit();
        });
        app.run_with_args::<String>(&[]);
    });
    let failed = failures.lock().unwrap().clone();
    assert!(failed.is_empty(), "trim-shot checks failed -> {}", failed.join(" // "));
}

#[test]
fn f2_8_s1_the_trim_shot_drags_a_border_and_prints_the_trim_sentence() {
    round();
    assert!(RAN_SHOT.load(Ordering::SeqCst), "the staged-shot check never ran");
}

#[test]
fn f2_8_s1_a_real_border_drag_trims_and_lands_the_picture_on_the_edge() {
    round();
    assert!(RAN_DRAG.load(Ordering::SeqCst), "the real-drag check never ran");
}

#[test]
fn f2_8_s3_a_right_drag_that_trims_never_moves_the_line() {
    round();
    assert!(RAN_LINE.load(Ordering::SeqCst), "the line-never-moves check never ran");
}
