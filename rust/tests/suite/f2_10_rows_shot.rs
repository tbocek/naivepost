//! F2.10's picture, pinned by state instead of by eye.
//!
//! `spec/img/05-rows.png` shows TWO recordings at the same time, one row each: the first camera's plate
//! naming its file (badge **1**), the second's carrying its shift correction `−19.00 s` (**2**), the overlap
//! both filmed (**3**), the watched row outlined (**4**), the preview showing that watched row (**5**), one
//! kept scene taken from camera 1 (**6**) and status "watching camera 2 — the cut shows camera 1 here; ▶
//! plays the cut" (**7**). The completion check that reopened this item found the shot's plates reading `cam0`
//! and `row 2` with an empty preview, so `snapshot_rows::stage` is the fix and this file is what keeps it
//! fixed: every check calls the same `stage` the recipe calls and asserts on what the page then says — the
//! plate labels it drew, the class it put on the row box, the frame it asked frames for, and the sentences its
//! own doors printed.
//!
//! One application, one `connect_activate`, one `#[test]` per check (`tests/suite/f2_8_shot_state.rs`'s shape): a
//! second `adw::Application` in a process is refused by GIO, and a panic inside `connect_activate` escapes
//! into GTK's C loop and aborts without a message, so each check runs under `catch_unwind` inside the single
//! activation and its failures are asserted outside it. Each check gets its own temp folder — the stage writes
//! a lane file and a frame into the tree the cwd resolves to, and the page's slots are process-wide, so two
//! fixtures in one binary would race.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut_select::Surface;
use naivepost::shell::Page;
use naivepost::snapshot_rows;
use naivepost::ui;

use crate::common;
use common::{fixture_dir, hold_last_window, release_last_window, settle, status_text};

static RAN_PLATES: AtomicBool = AtomicBool::new(false);
static RAN_WATCH: AtomicBool = AtomicBool::new(false);
static RAN_LENS: AtomicBool = AtomicBool::new(false);
static RAN_HEARING: AtomicBool = AtomicBool::new(false);
static RAN_ADD: AtomicBool = AtomicBool::new(false);
static RAN_COVERS: AtomicBool = AtomicBool::new(false);
static RAN_BENCH: AtomicBool = AtomicBool::new(false);

/// A Cut-page window in its own temp folder, so the stage has a real session tree to write its lane file and
/// its one extracted frame into — the same shape the F2.7/F2.8 shot tests use for the same reason.
fn shot_window(app: &adw::Application, slug: &str) -> adw::ApplicationWindow {
    let root = std::env::temp_dir().join(format!(
        "naivepost-f210-rows-{}-{slug}",
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
    common::size_to_minimum(&window);
    ui::tab_button(&window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_by_name::<()>("clicked", &[]);
    settle();
    // …and sized again now that the Cut page is the visible tab: headless an unallocated window leaves the
    // page's paned on its clamp with everything below the handle unmapped, and `stage` below then measures an
    // unmapped page. The shot's own staging moves this divider itself.
    common::size_to_minimum(&window);
    window
}

/// The named widget, whatever its type — the row boxes are `gtk::Box`, not buttons, so a downcast-only
/// accessor is too narrow. Same walk the page's own finders use.
fn find_named_panic(window: &adw::ApplicationWindow, name: &str) -> gtk::Widget {
    fn walk(root: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
        if root.widget_name() == name {
            return Some(root.clone());
        }
        for child in root.observe_children().iter::<glib::Object>() {
            let Ok(child) = child else { continue };
            let Ok(widget) = child.downcast::<gtk::Widget>() else { continue };
            if let Some(found) = walk(&widget, name) {
                return Some(found);
            }
        }
        None
    }
    walk(window.upcast_ref(), name).unwrap_or_else(|| panic!("no widget named `{name}` on the page"))
}

/// The two recordings of the figure, spelled the way `snapshot_rows::recordings` spells them: camera 1 for
/// its 37 s, and camera 2 sitting on the clock from 0:20 -- its stamp being 39 s after camera 1's, less the
/// 19 s correction. Its FILE begins at −19 s of the session clock, which is what the preview's ask below
/// reports, and its tape runs the same 37 s from where it sits.
fn recordings() -> Vec<naivepost::timeline::Recording> {
    vec![
        naivepost::timeline::Recording {
            base: snapshot_rows::CAM1.to_string(),
            start: 0.0,
            end: 37.0,
        },
        naivepost::timeline::Recording {
            base: snapshot_rows::CAM2.to_string(),
            start: 20.0,
            end: 20.0 - snapshot_rows::CAM2_SHIFT,
        },
    ]
}

/// A row's name plate label, read off the widget the page built.
fn plate(window: &adw::ApplicationWindow, name: &str) -> String {
    ui::line_step_button(window, name)
        .unwrap_or_else(|| panic!("the row list drew no `{name}` plate"))
        .label()
        .unwrap_or_default()
        .to_string()
}

/// The bases a row list names, for an assertion message that says what it saw.
fn bases(recordings: &[naivepost::timeline::Recording]) -> Vec<&str> {
    recordings.iter().map(|rec| rec.base.as_str()).collect()
}

/// The scene under the line in the live cut — read through the same door the painter reads.
fn scene_at_line(window: &adw::ApplicationWindow) -> naivepost::cut::Seg {
    let at = ui::line_position(window).t;
    ui::review_cut_segs(window)
        .into_iter()
        .find(|seg| seg.ins.is_empty() && seg.s <= at && at < seg.e)
        .unwrap_or_else(|| panic!("no kept scene under the line at {at}"))
}

/// Badges **1**, **2** and **7**: the two rows each carry their OWN recording's name, the second spelling the
/// shift correction, and neither falls back to the row number the shot used to show.
fn check_the_two_plates_name_their_own_recordings(app: &adw::Application) {
    let window = shot_window(app, "plates");
    let said = snapshot_rows::stage(&window, &fixture_dir());
    println!("05-rows stage said: {}", said.join(" | "));
    settle();
    let first = plate(&window, "watch-row-1");
    let second = plate(&window, "watch-row-2");
    println!("plates: [{first}] [{second}]");
    assert_eq!(
        first, snapshot_rows::CAM1,
        "badge 1: the first row's plate names the recording it speaks for"
    );
    assert!(
        second.starts_with(snapshot_rows::CAM2),
        "badge 2: the second row's plate names ITS OWN recording, not a row number: {second}"
    );
    assert!(
        second.ends_with("-19.00 s"),
        "badge 2: and spells the shift correction signed and to two decimals: {second}"
    );
    assert!(
        !first.contains("19.00"),
        "the unshifted camera's plate carries no correction: {first}"
    );
    // **7**, and it came from the stage rather than from a label written here.
    let status = status_text(&window);
    assert!(
        status.starts_with("watching camera 2"),
        "badge 7: the status row carries the spec's sentence: {status}"
    );
    assert!(
        said.iter().any(|line| line.starts_with("watching camera 2")),
        "and the recipe prints the same sentence it painted"
    );
    // The two rows really are two rows: the stage's two recordings, laid out on different rows by the
    // greedy colouring, and each named by its own base rather than by a row number.
    let placed = naivepost::timeline::rows_for(&recordings(), &ui::review_cut_of(&window));
    assert_ne!(placed[0], placed[1], "the overlap put the two cameras on different rows");
    assert_eq!(
        bases(&recordings()),
        vec![snapshot_rows::CAM1, snapshot_rows::CAM2],
        "and each row speaks for its own recording"
    );
    RAN_PLATES.store(true, Ordering::SeqCst);
    window.close();
}

/// The wire behind **4**: a click on the OTHER row's plate moves the dashed outline and hands the preview
/// back to that row. Said ONCE means the sentence does not repeat itself when the click was the scene's own
/// camera.
fn check_clicking_the_other_plate_moves_the_outline(app: &adw::Application) {
    let window = shot_window(app, "watch");
    snapshot_rows::stage(&window, &fixture_dir());
    settle();
    assert!(
        find_named_panic(&window, "camera-row-2").has_css_class("watched-row"),
        "**4**: the staged shot has row 2 outlined"
    );
    assert!(
        !find_named_panic(&window, "camera-row-1").has_css_class("watched-row"),
        "and only that row"
    );
    // The preview followed the watch: the frame the panel asked for came from camera 2's file (§B's `at(t)`
    // converts the line's session second into THAT file's own clock — 25 s of the session is 4 s into a
    // recording that sits at 0:20).
    let asked = ui::cut_picture::preview_asked_for_test();
    println!("preview asked: {asked:?}");
    let (base, at) = asked.expect("the panel asked frames for a recording");
    assert_eq!(base, snapshot_rows::CAM2, "badge 5: the preview asked the WATCHED row's camera");
    // `timeline::local_at` answers in the FILE's own clock, shift included: 25 − 20 + (−19) = −14, i.e. the
    // line stands 14 s BEFORE camera 2's tape begins (its stamp puts it 19 s earlier and only 5 s of that
    // correction was undone by the difference in stamps). The ask and the frame looked up are computed in one
    // place, so what matters here is which row asked -- and that it was camera 2's, not the cut's camera 1.
    assert!(
        (at - (25.0 - 20.0 + snapshot_rows::CAM2_SHIFT)).abs() < 1e-6,
        "and the second asked is that file's own, shift included: {at}"
    );

    // Now the click on row 1 -- the row the CUT shows -- and the page's rule decides what the status says.
    // `press_watch_row` paints a sentence only when `cut_cam::click_row` handed one back, so reading the
    // status after it shows which way the rule goes: a watch that resolves the disagreement (the preview is
    // now looking at the same row as the cut) owes nothing, and nothing must be painted.
    ui::line_step_button(&window, "watch-row-1")
        .expect("watch-row-1")
        .emit_by_name::<()>("clicked", &[]);
    settle();
    assert_eq!(ui::watched_row(), Some(0), "the click watched row 1");
    assert!(
        find_named_panic(&window, "camera-row-1").has_css_class("watched-row"),
        "and the dashed outline moved onto it"
    );
    assert!(
        !find_named_panic(&window, "camera-row-2").has_css_class("watched-row"),
        "off the row it was on"
    );
    assert!(
        !ui::camera_watch(&window).borrow().said,
        "the new watch had nothing to report, so it is not marked as having spoken"
    );
    // The ONCE in "status says ONCE", read off the door rather than the label: watching the row the cut
    // already shows is not a disagreement, so `cut_cam::watch_status` answers `None` and `press_watch_row`
    // paints nothing (`src/ui/cut_cameras.rs`: the status row is written only when a sentence came back).
    // No press here clears the status row and none is asked to -- the spec's word is ONCE, not CLEARED.
    let said_again = ui::press_watch_row(&window, 0);
    settle();
    assert!(
        said_again.is_none(),
        "a click onto the cut's own row owes no sentence at all, got {said_again:?}"
    );
    assert!(
        !ui::camera_watch(&window).borrow().said,
        "and the watch is not marked as having spoken, so it cannot repeat itself later either"
    );
    // The other half of S3: the play button hands the preview back to the cut. That is `hand_preview_back`'s
    // door, and it is the only way the watch is given up -- clicking the cut's own row keeps watching that row,
    // which is what the paragraph above just proved.
    let back_said = ui::hand_preview_back(&window);
    settle();
    println!("hand-back said: {back_said:?}");
    assert!(back_said.is_some(), "the play button hands a watched row back to the cut");
    assert_eq!(
        ui::camera_watch(&window).borrow().row,
        None,
        "and the watch is gone, so the preview is the cut's again"
    );
    // Badge 5's other half, read where it is actually observable: the snapshot bench names a base for its own
    // fixture folder (`set_preview_camera_base`) and that name OUTLIVES the watch, so the frame ask cannot show
    // the hand-back. What CAN is which row the panel answers from once nothing is watched -- `preview_row`
    // falls back to the scene's own camera, camera 1, which is exactly "the preview is the cut's again".
    assert_eq!(
        ui::preview_row(&window),
        0,
        "with no form open and no row watched, the preview stands on the cut's own row"
    );
    RAN_WATCH.store(true, Ordering::SeqCst);
    window.close();
}

/// S1 on the real badge: the 🔍 retakes the scene from the row that filmed the overlap, and names the row by
/// what lies on it.
fn check_the_lens_badge_moves_the_picture_to_the_other_row(app: &adw::Application) {
    let window = shot_window(app, "lens");
    snapshot_rows::stage(&window, &fixture_dir());
    settle();
    let before = scene_at_line(&window).cam;
    assert_eq!(before, 0, "badge 6: the one kept scene starts on camera 1's row");
    ui::line_step_button(&window, "scene-lens-0")
        .expect("the scene drew its lens badge")
        .emit_by_name::<()>("clicked", &[]);
    settle();
    let after = scene_at_line(&window).cam;
    assert_eq!(after, 1, "the press moved the scene's picture to the row that filmed the overlap");
    let status = status_text(&window);
    println!("lens status: {status}");
    assert!(
        status.contains("is shown from") && status.contains(snapshot_rows::CAM2),
        "and the status names the row by the recording on it: {status}"
    );
    RAN_LENS.store(true, Ordering::SeqCst);
    window.close();
}

/// S2 on the real badges: a speaker badge silences the microphone IN THIS SCENE, the gutter switch is the
/// whole cut, and the two are different questions.
fn check_a_speaker_badge_silences_one_scene_and_the_switch_the_cut(app: &adw::Application) {
    let window = shot_window(app, "hearing");
    snapshot_rows::stage(&window, &fixture_dir());
    settle();
    let scene = ui::review_cut_of(&window);
    assert!(
        naivepost::cut_hear::lane_is_heard_anywhere(&scene, &[snapshot_rows::MIC]),
        "a fresh cut hears the microphone everywhere"
    );
    ui::line_step_button(&window, &format!("speaker-badge-{}", snapshot_rows::MIC))
        .expect("the row drew the lane's speaker badge")
        .emit_by_name::<()>("clicked", &[]);
    settle();
    let hushed = ui::review_cut_of(&window);
    let index = naivepost::cut_hear::scene_at(&hushed.segs, ui::line_position(&window).t)
        .expect("the line stands in the staged scene");
    assert!(
        !hushed.segs[index].hears(snapshot_rows::MIC),
        "S2: the scene under the line no longer hears the microphone"
    );
    let status = status_text(&window);
    println!("speaker status: {status}");
    assert!(
        status.starts_with(snapshot_rows::MIC) && status.contains("is silent in the scene at"),
        "and the status says which lane and which scene: {status}"
    );
    // The gutter switch: the same lane for every scene of the cut. One scene exists here, so the proof is
    // that the switch's OWN answer flipped, which is `cut_hear::lane_is_heard_anywhere`'s reading.
    ui::line_step_button(&window, &format!("gutter-switch-{}", snapshot_rows::MIC))
        .expect("the row drew the lane's gutter switch")
        .emit_by_name::<()>("clicked", &[]);
    settle();
    let switched = ui::review_cut_of(&window);
    // The switch's face is `lane_is_heard_anywhere`'s reading, so pressing it can never contradict what the
    // switch claimed -- which is the wire here. The scene the badge silenced stays silenced either way.
    assert!(
        naivepost::cut_hear::lane_is_heard_anywhere(&switched, &[snapshot_rows::MIC])
            != naivepost::cut_hear::lane_is_heard_anywhere(&hushed, &[snapshot_rows::MIC]),
        "the gutter switch answers for the whole cut, so its own reading flipped: {:?}",
        switched.segs.iter().map(|seg| seg.quiet.clone()).collect::<Vec<_>>()
    );
    RAN_HEARING.store(true, Ordering::SeqCst);
    window.close();
}

/// The spec's own sentence for this figure ("in the staged shot, ＋ Add over the same seconds on camera 2
/// added nothing"): a band laid over footage the cut already keeps earns no new scene.
fn check_add_over_the_same_seconds_on_the_other_camera_adds_nothing(app: &adw::Application) {
    let window = shot_window(app, "add");
    snapshot_rows::stage(&window, &fixture_dir());
    settle();
    let before = ui::review_cut_segs_count(&window);
    // 20–37 is exactly badge 3's overlap: camera 2's ground, already kept from camera 1.
    ui::draw_selection(&window, Surface::PictureRow(1), None, 20.0, 37.0);
    let outcome = ui::press_add(&window);
    let status = status_text(&window);
    println!("add over the overlap said: {status}");
    let after = ui::review_cut_segs_count(&window);
    match outcome {
        naivepost::cut_verbs::Outcome::Refused(_) => {
            assert_eq!(after, before, "a refusal changed nothing: {before} -> {after}")
        }
        // The other answer the rule gives: the seconds were TAKEN OFF the other camera and handed to camera 2
        // (cut_verbs::add's steal). The kept footage is then the same 0-37 stretch either way -- what may not
        // happen is a second, overlapping piece of footage, which is what "clips never overlap" forbids.
        naivepost::cut_verbs::Outcome::Applied { .. } => {
            let footage: Vec<(f64, f64)> = ui::review_cut_segs(&window)
                .iter()
                .filter(|seg| seg.ins.is_empty() && seg.e > seg.s)
                .map(|seg| (seg.s, seg.e))
                .collect();
            let total: f64 = footage.iter().map(|(s, e)| e - s).sum();
            assert!(
                (total - 37.0).abs() < 1e-9,
                "the kept footage still covers exactly 37 s -- the overlap was taken off the camera that \n                 kept it and handed to the one it was drawn on, so nothing new entered the cut: {footage:?}"
            );
        }
    }
    RAN_ADD.store(true, Ordering::SeqCst);
    window.close();
}

/// The shot's own frame must REACH both camera plates together — `spec/img/05-rows.png` is one picture of both
/// rows, so a shot that cannot bring row 2 into the same frame as row 1 does not show F2.10 at all. Measured on
/// the page rather than in the PNG: the window clamps to ~790 px of plate while the rows sit further down a page
/// ~1015 px tall, so the file can never contain them; what the spec asks for is one reachable frame with **1**
/// above **2**, which is a fact about the widgets.
fn check_the_shot_covers_both_camera_rows(app: &adw::Application) {
    let window = shot_window(app, "covers");
    snapshot_rows::stage(&window, &fixture_dir());
    // `present()` only queues the map, so headless nothing has allocated the page when `stage` sized the panes:
    // without this drain every widget answers 0x0 and the frame measures an unmapped window.
    settle();
    let high = window.height();
    println!("shot frame {}x{high}", window.width());
    assert!(high > 400, "the window never mapped: {high} px tall");
    // The shot's frame is the window at its own height; `write_png` snapshots that box, so what has to be true
    // for F2.10's picture to exist at all is that BOTH plates are inside a band of that height -- reachable
    // together by scrolling the page, which is how the user sees them and how the recipe's own scroll reaches
    // them. Measured: row 2 sits ~73 px below the bottom of an unscrolled 730 px plate.
    let tops = snapshot_rows::row_tops(&window);
    let (first, second) = (tops[0], tops[1]);
    println!("plate tops {first}/{second} in a {high} px frame");
    assert!(
        snapshot_rows::shot_covers(&window, first, high),
        "both camera plates fit one frame of the window's height ({high} px): tops at {first}/{second}"
    );
    assert_eq!(
        snapshot_rows::rows_in_order(&window),
        Some(true),
        "badge **1** sits above badge **2**: camera 1's row is the top row"
    );
    // Not vacuous: a band too short to hold both rows must fail, or the gate proves nothing.
    assert!(
        !snapshot_rows::shot_covers(&window, first, second - first),
        "a band that stops above row 2 must not pass"
    );
    RAN_COVERS.store(true, Ordering::SeqCst);
    window.close();
}

/// F2.10's picture only exists if the shell keeps its honest height: a bare `gtk::TextView` in Prepare's
/// column reports its whole prompt as the page minimum (measured 2536 px), which climbs into the view stack
/// and pushes every other tab's content above the top of the window — measured before this fix, the camera
/// rows sat at y=-1188/-1150 in an 890 px frame, painted nowhere. Two facts pin that: the bench text view is
/// inside a scroller (so the document cannot set the minimum), and after staging both camera plates lie
/// inside the pixels the PNG captures.
fn check_the_bench_stays_scrolled_and_the_rows_are_painted(app: &adw::Application) {
    let window = shot_window(app, "painted");
    snapshot_rows::stage(&window, &fixture_dir());
    settle();
    let bench = find_named_panic(&window, "bench-text");
    assert!(
        bench
            .parent()
            .and_then(|p| p.downcast::<gtk::ScrolledWindow>().ok())
            .is_some(),
        "F2.10 needs the shell to keep its height: `bench-text` must sit inside a ScrolledWindow so the \
         prompt's own height cannot become the window's minimum (it is a {:?})",
        bench.parent().map(|p| p.widget_name().to_string())
    );
    let high = window.height();
    assert!(high > 400, "the window never mapped: {high} px tall");
    let tops = snapshot_rows::row_tops(&window);
    for (label, top) in [("row 1", tops[0]), ("row 2", tops[1])] {
        assert!(
            top >= 0 && top < high,
            "{label} is inside the painted {high} px frame: its top is at {top}"
        );
    }
    RAN_BENCH.store(true, Ordering::SeqCst);
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
                ("plates", check_the_two_plates_name_their_own_recordings as fn(&adw::Application)),
                ("watch", check_clicking_the_other_plate_moves_the_outline),
                ("lens", check_the_lens_badge_moves_the_picture_to_the_other_row),
                ("hearing", check_a_speaker_badge_silences_one_scene_and_the_switch_the_cut),
                ("add", check_add_over_the_same_seconds_on_the_other_camera_adds_nothing),
                ("covers", check_the_shot_covers_both_camera_rows),
                ("bench", check_the_bench_stays_scrolled_and_the_rows_are_painted),
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
                        "plates" => RAN_PLATES.store(true, Ordering::SeqCst),
                        "watch" => RAN_WATCH.store(true, Ordering::SeqCst),
                        "lens" => RAN_LENS.store(true, Ordering::SeqCst),
                        "hearing" => RAN_HEARING.store(true, Ordering::SeqCst),
                        "covers" => RAN_COVERS.store(true, Ordering::SeqCst),
                        "bench" => { let _ = (); }
                        _ => RAN_ADD.store(true, Ordering::SeqCst),
                    }
                }
            }
            app.quit();
        });
        app.run_with_args::<String>(&[]);
    });
    let failed = failures.lock().unwrap().clone();
    assert!(failed.is_empty(), "05-rows shot checks failed -> {}", failed.join(" // "));
}

#[test]
fn f2_10_s1_the_shot_covers_both_camera_rows() {
    round();
    assert!(RAN_COVERS.load(Ordering::SeqCst), "the shot-coverage check never ran");
}

#[test]
fn f2_10_s3_a_seeded_second_camera_keeps_its_own_base_on_its_plate() {
    round();
    assert!(
        RAN_PLATES.load(Ordering::SeqCst),
        "the two-plates check never ran"
    );
}

#[test]
fn f2_10_s4_clicking_the_other_plate_moves_the_outline_and_the_preview() {
    round();
    assert!(RAN_WATCH.load(Ordering::SeqCst), "the watch-wire check never ran");
}

#[test]
fn f2_10_s1_the_lens_badge_names_the_row_the_scene_is_shown_from() {
    round();
    assert!(RAN_LENS.load(Ordering::SeqCst), "the lens check never ran");
}

#[test]
fn f2_10_s2_a_speaker_badge_silences_one_scene_and_the_gutter_switch_the_whole_cut() {
    round();
    assert!(RAN_HEARING.load(Ordering::SeqCst), "the hearing check never ran");
}

#[test]
fn f2_10_s5_the_bench_stays_scrolled_and_both_rows_are_painted() {
    round();
    assert!(
        RAN_BENCH.load(Ordering::SeqCst),
        "the bench-scroller / painted-rows check never ran"
    );
}

#[test]
fn f2_10_s3_add_over_the_same_seconds_on_the_other_camera_adds_nothing() {
    round();
    assert!(RAN_ADD.load(Ordering::SeqCst), "the Add-over-the-overlap check never ran");
}
