// F2.10 — the wire: the camera rows on the Cut page are real widgets, and a click on each one reaches
// `cut_cam`'s / `cut_hear`'s rule rather than a copy of it. The logic tests (tests/suite/cut_cam_hearing.rs)
// prove the rules; this proves a press arrives at the named widget and lands in the same state.
//
// What is fired by name: `watch-row-<n>` (S3, watch a row — and S3's proof that the WATCH reaches the
// preview, read off `cut_picture::preview_asked_for_test` rather than a pixel), `scene-lens-<n>` (S1, pick which row a
// scene's picture comes from — the 🔍 belongs to the SCENE, not to a row header),
// `speaker-badge-<lane>` and `gutter-switch-<lane>` (S2, hear or not),
// and `play-cut-button`, which is what takes the preview back off a watched row.
//
// One application, one `connect_activate`, exactly one `#[test]`: these checks mutate process-shared
// state (this window's cut slot, its watch, its line position and the status line), so they run once in
// sequence and report through flags the test asserts on — the shape tests/suite/cut_copy_paste_lane_widgets.rs
// uses. A test binary gets exactly ONE `Application::run`, so every check lives inside that activate.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut::{self, Seg};
use naivepost::cut_hear as hear;
use naivepost::cut_line::LinePos;
use naivepost::shell::Page;
use naivepost::ui;

use crate::common;
use common::{fixture_dir, settle, status_text};

static RAN_ROWS: AtomicBool = AtomicBool::new(false);
static RAN_WATCH: AtomicBool = AtomicBool::new(false);
static RAN_LENS: AtomicBool = AtomicBool::new(false);
static RAN_SPEAKER: AtomicBool = AtomicBool::new(false);
static RAN_GUTTER: AtomicBool = AtomicBool::new(false);
static RAN_HELD: AtomicBool = AtomicBool::new(false);
static RAN_PREVIEW: AtomicBool = AtomicBool::new(false);
static RAN_HANDED_BACK: AtomicBool = AtomicBool::new(false);

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

/// This window's cut as a whole, read back through the same accessors the page draws from.
fn cut_of(window: &adw::ApplicationWindow) -> cut::Cut {
    let mut fresh = cut::Cut::default();
    fresh.segs = ui::review_cut_segs(window);
    fresh.lanes = ui::review_lanes(window);
    fresh
}

/// Let the main context run what the widget emissions queued.

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

/// S1: the 🔍 on the SCENE retakes that scene's picture from the next camera that filmed it, leaving its
/// seconds where they were, and the status reads the seam's own sentence. The button sits in the row list
/// beside the scene's ✕ (`scene-lens-0`), not on a row header — the question is about one scene.
fn check_the_lens_badge_moves_the_picture_to_that_row(app: &adw::Application) {
    let window = cut_window(app);
    // A hold from an earlier check would win over the line (S1's "the scene under the line, OR THE HELD
    // ONE"), so this check starts from no hold — otherwise it edits whatever a previous click picked up.
    ui::set_held_clip(None);
    let before = ui::review_cut_segs(&window);
    assert_eq!(before[0].cam, 0, "the scene under the line starts out shown from row 1");

    let lens = ui::line_step_button(&window, "scene-lens-0").expect("`scene-lens-0` exists");
    assert!(lens.is_sensitive(), "two cameras filmed those seconds, so there is somewhere to switch to");
    lens.emit_by_name::<()>("clicked", &[]);
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
    // The badges answer about the CHOSEN scene (S1's held one), so this check holds nothing: with no hold
    // the line at 25 s stands inside scene 0, which is the scene whose hearing is being asked about.
    ui::set_held_clip(None);
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

/// S2: a HELD scene is the scene the badges speak of — not wherever the red line happens to stand. The
/// spec's sentence names a time ("silent in the scene at 0:42"), so a badge that could only read the
/// line's own scene would describe a scene nobody picked; holding a later clip and pressing must silence
/// THAT clip and print its own seconds.
fn check_a_held_scene_is_the_one_the_badge_silences(app: &adw::Application) {
    let window = cut_window(app);
    // Hold the SECOND camera's scene (20-37). The red line stays where `cut_window` put it, at 25 s, so
    // the two candidates for "this scene" differ only while the hold is ignored.
    ui::set_held_clip(Some(film(20.0, 37.0, 1)));
    ui::refresh_camera_rows(&window);
    settle();
    let badge = ui::toggle_button(&window, "speaker-badge-cam1").expect("`speaker-badge-cam1` exists");
    assert!(badge.is_sensitive(), "a held footage scene is a scene that can be silenced");

    badge.set_active(true);
    settle();
    let segs = ui::review_cut_segs(&window);
    assert!(
        !segs[1].hears("cam1"),
        "the press silenced the HELD scene (index 1), not the line's: {:?}",
        segs.iter().map(|seg| seg.quiet.clone()).collect::<Vec<_>>()
    );
    assert!(
        segs[0].hears("cam1"),
        "and left the scene under the line alone: {:?}",
        segs[0].quiet
    );
    let printed = status_text(&window);
    assert!(printed.contains("silent in"), "and said so: {printed}");
    assert!(
        printed.contains("0:20"),
        "naming the HELD scene's own seconds, which is what makes the time true: {printed}"
    );
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
    // The click, fired the way the toolkit fires it. The "no longer watched" sentence is read from the
    // door that PRODUCED it (`hand_preview_back`, what ▶'s handler puts on the row) rather than off the
    // shared status line after a `settle()`: §2 lets any later line take that row -- the clock repaint
    // does exactly that once the preview is playing -- and reading the row back was the race this check
    // flaked on. Calling the door again AFTER the click would return None (the watch already released), so
    // the value asserted is the one the press itself printed, captured by reading the row at the moment
    // the release answers: the button's own handler is what calls the door, and its return is what went
    // on the row, so the seam below reads the row BEFORE anything else can speak over it.
    let before = status_text(&window);
    play.emit_by_name::<()>("clicked", &[]);
    let said = status_text(&window);
    settle();

    assert_eq!(ui::watched_row(), None, "\u{25b6} handed the preview back to the cut");
    assert_eq!(ui::camera_watch(&window).borrow().row, None, "and the watch holds no row");
    assert!(
        !plate_two.has_css_class("watched-row"),
        "so the dashed outline is off the row that was watched"
    );
    assert_ne!(said, before, "and the click put its own sentence on the row");
    assert!(
        said.contains("no longer watched") || said.contains("plays the cut"),
        "the press answered the click about the preview, not something else: {said:?}"
    );
    window.close();
}

/// S3: watching a row moves the PREVIEW, not only the outline and the sentence. The panel's frame is asked
/// for by `cut_picture::refresh`, which picks the watched row's recording and converts the line's session
/// second into a second of THAT FILE (`§B`'s `at(t)`), so the ask is what proves the picture followed the
/// click. Read through `preview_asked_for_test`, because a frame on disk would need Prepare's extraction — the ask
/// is the observable half that exists in a headless container.
fn check_a_row_watch_moves_the_preview_frame(app: &adw::Application) {
    let window = cut_window(app);
    ui::set_held_clip(None);
    ui::set_watched_row(None);
    naivepost::ui::cut_picture::refresh(&window);
    settle();
    let unwatched = naivepost::ui::cut_picture::preview_asked_for_test().expect("a page with rows asks for a frame");
    // Row 1 is the tape whose span starts at 0 s, so its file second equals the line's session second.
    assert!(
        (unwatched.1 - 25.0).abs() < 0.001,
        "with no watch the panel reads the first row at the line's own second: {unwatched:?}"
    );

    let plate_two = ui::line_step_button(&window, "watch-row-2").expect("`watch-row-2` exists");
    plate_two.emit_by_name::<()>("clicked", &[]);
    settle();
    let watched = naivepost::ui::cut_picture::preview_asked_for_test().expect("the watch still asks for a frame");
    assert_ne!(watched.0, unwatched.0, "the panel asks a DIFFERENT camera now: {watched:?}");
    // The second camera runs from 20 s of the session clock, so the same 25 s line is 5 s into ITS file;
    // asking the first row's file again would show a frame from the wrong part of the tape.
    // The second row's tape carries the hand correction (**2**'s "−19.00 s", `cut.shift["cam1"] = -19.0`),
    // so its FILE second is the line's session second MINUS that shift: 25 s on the clock is −14 s into
    // cam1's own file, which is exactly why a watch cannot ask every recording for the same number.
    assert!(
        (watched.1 - (-14.0)).abs() < 0.001,
        "and it converts the line's second through the watched recording's own start and shift: {watched:?}"
    );

    let play = ui::play_cut_button(&window).expect("the Cut page has ▶ Play the cut");
    play.emit_by_name::<()>("clicked", &[]);
    settle();
    let back = naivepost::ui::cut_picture::preview_asked_for_test().expect("and after the hand-back, still a frame");
    assert_eq!(back.0, unwatched.0, "▶ put the first row's picture back: {back:?}");
    assert!(
        (back.1 - unwatched.1).abs() < 0.001,
        "at the same second it was read at before the watch: {back:?}"
    );
    window.close();
}

fn window_round() {
    common::one_app_round(|app| {
        check_a_row_click_watches_it_and_says_what_the_cut_shows(app);
        RAN_ROWS.store(true, Ordering::SeqCst);
        check_the_lens_badge_moves_the_picture_to_that_row(app);
        RAN_WATCH.store(true, Ordering::SeqCst);
        check_the_speaker_badge_silences_this_scene(app);
        RAN_LENS.store(true, Ordering::SeqCst);
        check_a_held_scene_is_the_one_the_badge_silences(app);
        RAN_HELD.store(true, Ordering::SeqCst);
        check_the_gutter_switch_speaks_for_the_whole_cut(app);
        RAN_SPEAKER.store(true, Ordering::SeqCst);
        check_play_takes_the_preview_back(app);
        RAN_GUTTER.store(true, Ordering::SeqCst);
        check_a_row_watch_moves_the_preview_frame(app);
        RAN_PREVIEW.store(true, Ordering::SeqCst);
        RAN_HANDED_BACK.store(true, Ordering::SeqCst);
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
    assert!(RAN_WATCH.load(Ordering::SeqCst), "the lens check never ran");
    assert!(RAN_LENS.load(Ordering::SeqCst), "the speaker-badge check never ran");
    assert!(RAN_SPEAKER.load(Ordering::SeqCst), "the gutter-switch check never ran");
    assert!(RAN_HELD.load(Ordering::SeqCst), "the held-scene badge check never ran");
    assert!(
        RAN_PREVIEW.load(Ordering::SeqCst),
        "the preview-follows-the-watch check never ran"
    );
    assert!(RAN_GUTTER.load(Ordering::SeqCst), "the play-hands-back check never ran");
    assert!(RAN_HANDED_BACK.load(Ordering::SeqCst), "the hand-back assertion block never ran");
}
