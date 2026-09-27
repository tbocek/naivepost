// F2.10 — the wire: the camera rows on the Cut page are real widgets, and a click on each one reaches
// `cut_cam`'s / `cut_hear`'s rule rather than a copy of it. The logic tests (tests/cut_cam_hearing.rs)
// prove the rules; this proves a press arrives at the named widget and lands in the same state.
//
// What is fired by name: `watch-row-<n>` (S3, watch a row), `lens-badge-<n>` (S1, pick which row a
// scene's picture comes from), `speaker-badge-<lane>` and `gutter-switch-<lane>` (S2, hear or not),
// and `play-cut-button`, which is what takes the preview back off a watched row.
//
// One application, one `connect_activate`, exactly one `#[test]`: these checks mutate process-shared
// state (this window's cut slot, its watch, its line position and the status line), so they run once in
// sequence and report through flags the test asserts on — the shape tests/cut_copy_paste_lane_widgets.rs
// uses. A test binary gets exactly ONE `Application::run`, so every check lives inside that activate.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut::{self, Seg};
use naivepost::cut_hear as hear;
use naivepost::cut_line::LinePos;
use naivepost::shell::Page;
use naivepost::ui;

static RAN_ROWS: AtomicBool = AtomicBool::new(false);
static RAN_WATCH: AtomicBool = AtomicBool::new(false);
static RAN_LENS: AtomicBool = AtomicBool::new(false);
static RAN_SPEAKER: AtomicBool = AtomicBool::new(false);
static RAN_GUTTER: AtomicBool = AtomicBool::new(false);
static RAN_HANDED_BACK: AtomicBool = AtomicBool::new(false);

fn fixture_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/demo.naivepost")
}

/// Kept footage on camera `row`.
fn film(s: f64, e: f64, row: i32) -> Seg {
    Seg { s, e, cam: row, ..Default::default() }
}

/// The two-camera fixture of `spec/img/05-rows.png`: camera 0 for 0–37 and a second recording that
/// starts at 20, which is what puts them on two rows, plus one lane so there is a speaker badge and a
/// gutter switch to press. The lane is named `cam1` because that is the recording whose shift the name
/// plate carries (`shift["cam1"] = -19.0`, **2**'s "−19.00 s").
fn seeded_cut() -> cut::Cut {
    let mut fresh = cut::Cut::default();
    fresh.segs = vec![film(0.0, 37.0, 0), film(20.0, 37.0, 1)];
    fresh.lanes = vec![cut::Lane {
        name: "cam1".to_string(),
        src: "cam1".to_string(),
        at: 20.0,
        off: 0.0,
        dur: 17.0,
    }];
    fresh.shift.insert("cam1".to_string(), -19.0);
    fresh
}

fn status_text(window: &adw::ApplicationWindow) -> String {
    ui::find_status(window.upcast_ref())
        .expect("the shell has a status line")
        .text()
        .to_string()
}

/// This window's cut as a whole, read back through the same accessors the page draws from.
fn cut_of(window: &adw::ApplicationWindow) -> cut::Cut {
    let mut fresh = cut::Cut::default();
    fresh.segs = ui::review_cut_segs(window);
    fresh.lanes = ui::review_lanes(window);
    fresh
}

/// Let the main context run what the widget emissions queued.
fn settle() {
    let context = glib::MainContext::default();
    for _ in 0..64 {
        if !context.iteration(false) {
            break;
        }
    }
}

/// A widget by its stable name, whatever its type -- used where the accessor's downcast is too narrow (the
/// row boxes are `gtk::Box`, not buttons). Same walk the page's own finders use.
fn find_named(window: &adw::ApplicationWindow, name: &str) -> gtk::Widget {
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

/// A Cut-page window with the two-camera cut seeded and the red line inside the overlap (t = 25), so the
/// scene under the line is the one shown from camera 0 while camera 1 filmed the same seconds.
fn cut_window(app: &adw::Application) -> adw::ApplicationWindow {
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    assert!(
        model.sources.iter().any(|source| source.footage),
        "the fixture must carry a footage row or the Cut tab is locked (shell::lock) and this would \
         test the wrong page"
    );
    let window = ui::build_window(app, &model, "Prepare");
    window.present();
    ui::tab_button(&window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_clicked();
    assert_eq!(ui::state(&window).page, Page::Cut);
    ui::seed_review_cut(&window, &seeded_cut());
    // The line sits at 25 s: inside BOTH recordings' span, and inside the scene whose picture comes
    // from camera 0 — which is what makes S3's "the cut shows camera 1 here" sentence owed.
    ui::set_line_position(&window, LinePos { t: 25.0 });
    // Nothing watched before the first check: the thread-local outlives a window, so a leftover watch
    // would make the next check's outline wrong rather than today's answer.
    ui::set_watched_row(None);
    ui::refresh_camera_rows(&window);
    settle();
    window
}

/// S3 + S1: the row list is really drawn, and clicking a row's name plate watches it, moves the dashed
/// outline onto that row alone, and prints the **7** sentence.
fn check_a_row_click_watches_it_and_says_what_the_cut_shows(app: &adw::Application) {
    let window = cut_window(app);

    let rows = ui::camera_rows_box(&window).expect("the Cut page drew `camera-rows`");
    assert!(rows.is_mapped(), "`camera-rows` is on the visible page");
    let plate_one = ui::line_step_button(&window, "watch-row-1").expect("`watch-row-1` exists");
    assert!(plate_one.is_mapped(), "the first row's name plate is realized");
    // The plate spells a signed shift on the row that HAS one: `cut_cam::name_plate` writes `{:+.2} s`,
    // so row 2's plate carries "−19.00 s" (**2** in `spec/img/05-rows.png`). Row 1 is unshifted and
    // its plate is the bare base name -- which is why the check reads the shifted row, not any row.
    let plate_two = ui::line_step_button(&window, "watch-row-2").expect("`watch-row-2` exists");
    let plate_label = plate_two.label().unwrap_or_default().to_string();
    assert!(
        plate_label.contains("-19.00 s"),
        "the second row's plate spells the signed shift correction: {plate_label}"
    );
    assert_eq!(ui::watched_row(), None, "a fresh window watches no row");
    assert!(
        !plate_one.has_css_class("watched-row"),
        "and no row carries the dashed outline yet"
    );

    plate_two.emit_by_name::<()>("clicked", &[]);
    settle();
    assert_eq!(ui::watched_row(), Some(1), "the click watched row 2");
    assert_eq!(
        ui::camera_watch(&window).borrow().row,
        Some(1),
        "and the window's own watch agrees"
    );
    // The class lives on the ROW (`camera-row-<n>`), not on its name plate: `refresh_camera_rows` puts it
    // there and the plate only carries the label, so the outline is read off the holder.
    let row_two = find_named(&window, "camera-row-2");
    let row_one = find_named(&window, "camera-row-1");
    assert!(row_two.has_css_class("watched-row"), "**4** the watched row's box is outlined");
    assert!(!row_one.has_css_class("watched-row"), "and only that row's box");
    // **7**: the line at 25 s lies in a kept scene shown from camera 1(row 0), so watching row 2 says so.
    let printed = status_text(&window);
    assert!(
        printed.starts_with("watching camera 2"),
        "the status carries the spec's sentence: {printed}"
    );
    assert!(printed.contains("plays the cut"), "and points at ▶ for the way back: {printed}");
    window.close();
}

/// S1: the lens badge on the second row retakes the scene's picture from that row, leaving its seconds
/// where they were, and the status reads the seam's own sentence.
fn check_the_lens_badge_moves_the_picture_to_that_row(app: &adw::Application) {
    let window = cut_window(app);
    let before = ui::review_cut_segs(&window);
    assert_eq!(before[0].cam, 0, "the scene under the line starts out shown from row 1");

    let lens = ui::toggle_button(&window, "lens-badge-2").expect("`lens-badge-2` exists");
    assert!(lens.is_sensitive(), "with footage under the line the lens question has an answer");
    lens.emit_by_name::<()>("toggled", &[]);
    settle();

    let after = ui::review_cut_segs(&window);
    assert_eq!(after[0].cam, 1, "the scene is shown from row 2 now");
    assert_eq!((after[0].s, after[0].e), (before[0].s, before[0].e), "only the row moved, never the seconds");
    let printed = status_text(&window);
    assert!(
        printed.contains("is shown from") && printed.contains("now"),
        "the status reads S1's wording: {printed}"
    );
    window.close();
}

/// S2: the speaker badge silences the lane in THIS scene and lights as silent; pressing again un-silences
/// it, both read back off the window's own cut.
fn check_the_speaker_badge_silences_this_scene(app: &adw::Application) {
    let window = cut_window(app);
    let badge = ui::toggle_button(&window, "speaker-badge-cam1").expect("`speaker-badge-cam1` exists");
    assert!(badge.is_sensitive(), "a footage scene can be silenced");
    assert!(!badge.is_active(), "at first the scene hears the lane, so the badge is not struck through");

    badge.set_active(true);
    settle();
    let segs = ui::review_cut_segs(&window);
    assert!(
        !segs[0].hears("cam1"),
        "the press put the lane in this scene's quiet list: {:?}",
        segs[0].quiet
    );
    let printed = status_text(&window);
    assert!(printed.contains("silent in"), "and said so: {printed}");

    badge.set_active(false);
    settle();
    let segs = ui::review_cut_segs(&window);
    assert!(segs[0].hears("cam1"), "pressing again took the lane back: {:?}", segs[0].quiet);
    assert!(status_text(&window).contains("heard in"), "and says heard now");
    window.close();
}

/// S2: the gutter switch speaks for the whole cut — after one press EVERY kept scene lists the lane, which
/// is the pair `refresh_camera_rows` reads to show the switch OFF.
fn check_the_gutter_switch_speaks_for_the_whole_cut(app: &adw::Application) {
    let window = cut_window(app);
    let switch = ui::toggle_button(&window, "gutter-switch-cam1").expect("`gutter-switch-cam1` exists");
    assert!(switch.is_active(), "everything is heard at first, so the switch stands ON");

    switch.set_active(false);
    settle();
    let cut_now = cut_of(&window);
    assert!(
        hear::all_silent(&cut_now, &["cam1"]),
        "every kept scene now lists the lane: {:?}",
        cut_now.segs.iter().map(|seg| seg.quiet.clone()).collect::<Vec<_>>()
    );
    assert!(
        !hear::lane_is_heard_anywhere(&cut_now, &["cam1"]),
        "and nothing hears it anywhere -- which is why the face goes OFF"
    );
    assert!(status_text(&window).contains("off for the whole cut"));
    window.close();
}

/// S3: ▶✂ takes the preview back off the watched row — both slots cleared, and the status says the cut
/// is playing rather than the row.
fn check_play_takes_the_preview_back(app: &adw::Application) {
    let window = cut_window(app);
    let plate_two = ui::line_step_button(&window, "watch-row-2").expect("`watch-row-2` exists");
    plate_two.emit_by_name::<()>("clicked", &[]);
    settle();
    assert_eq!(ui::watched_row(), Some(1), "a row is watched before ▶");

    let play = ui::play_cut_button(&window).expect("the Cut page has ▶ Play the cut");
    play.emit_by_name::<()>("clicked", &[]);
    settle();

    assert_eq!(ui::watched_row(), None, "\u{25b6} handed the preview back to the cut");
    assert_eq!(ui::camera_watch(&window).borrow().row, None, "and the watch holds no row");
    assert!(
        !plate_two.has_css_class("watched-row"),
        "so the dashed outline is off the row that was watched"
    );
    assert!(
        status_text(&window).contains("no longer watched"),
        "and the status says so: {}",
        status_text(&window)
    );
    window.close();
}

fn window_round() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(|app| {
            check_a_row_click_watches_it_and_says_what_the_cut_shows(app);
            RAN_ROWS.store(true, Ordering::SeqCst);
            check_the_lens_badge_moves_the_picture_to_that_row(app);
            RAN_WATCH.store(true, Ordering::SeqCst);
            check_the_speaker_badge_silences_this_scene(app);
            RAN_LENS.store(true, Ordering::SeqCst);
            check_the_gutter_switch_speaks_for_the_whole_cut(app);
            RAN_SPEAKER.store(true, Ordering::SeqCst);
            check_play_takes_the_preview_back(app);
            RAN_GUTTER.store(true, Ordering::SeqCst);
            RAN_HANDED_BACK.store(true, Ordering::SeqCst);
            app.quit();
        });
        app.run_with_args::<String>(&[]);
    });
}

#[test]
fn f2_10_wire_the_camera_row_widgets_reach_the_rules_through_real_clicks() {
    // Pin cwd to our own temp root before anything builds: the press resolves the project through
    // `startup::session_dir(current_dir())`, and leaving cwd at `rust/` writes a stray
    // `rust/session.naivepost/` into the repo.
    let root = std::env::temp_dir().join(format!("np-camrows-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temp root created");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");

    window_round();
    assert!(RAN_ROWS.load(Ordering::SeqCst), "the row-click check never ran");
    assert!(RAN_WATCH.load(Ordering::SeqCst), "the lens-badge check never ran");
    assert!(RAN_LENS.load(Ordering::SeqCst), "the speaker-badge check never ran");
    assert!(RAN_SPEAKER.load(Ordering::SeqCst), "the gutter-switch check never ran");
    assert!(RAN_GUTTER.load(Ordering::SeqCst), "the play-hands-back check never ran");
    assert!(RAN_HANDED_BACK.load(Ordering::SeqCst), "the hand-back assertion block never ran");
}
