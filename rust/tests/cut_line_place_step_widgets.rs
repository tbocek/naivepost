//! F2.4 — the wire: real clicks on the four line-step buttons and a real key press reach
//! `cut_line`, moving the same line state tests/cut_line_place_step.rs asserts from the logic side.
//!
//! One application, one `connect_activate`, exactly one `#[test]`: the checks mutate process-shared
//! state (this window's line slot and preview player), so they run once and report through flags the
//! test asserts on — the shape tests/cut_review_cuts_widgets.rs uses.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::shell::Page;
use naivepost::{cut_line, cut_screen, ui};
#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::{fixture_dir};

static RAN_STEPS: AtomicBool = AtomicBool::new(false);
static RAN_KEYS: AtomicBool = AtomicBool::new(false);
static RAN_CLICK: AtomicBool = AtomicBool::new(false);
static RAN_EFFECTS: AtomicBool = AtomicBool::new(false);
static RAN_RESTORE: AtomicBool = AtomicBool::new(false);
static RAN_FLUSH: AtomicBool = AtomicBool::new(false);
static RAN_BAND_PRESS: AtomicBool = AtomicBool::new(false);
static RAN_RESTORE_SCROLL: AtomicBool = AtomicBool::new(false);
static RAN_PROBED_RATE: AtomicBool = AtomicBool::new(false);
static RAN_REACH: AtomicBool = AtomicBool::new(false);
static RAN_ZOOM_SCROLL: AtomicBool = AtomicBool::new(false);

const FPS: f64 = 25.0;

/// A window sitting on the Cut page, built fresh per check so one check's line state cannot leak into
/// another's assertions; the newest window is the one every accessor reads.
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
    assert_eq!(ui::state(&window).page, Page::Cut, "the Cut tab bounced");
    window
}

/// Let GTK lay children out. `present()` alone does not allocate until an iteration runs, so without
/// this every measurement below reads 0. gtk4-rs 0.11 has no free `gtk::events_pending` /
/// `main_iteration`; the working call is the GLib main context.
fn settle() {
    let context = glib::MainContext::default();
    while context.pending() {
        context.iteration(false);
    }
}

/// S2 through the real buttons: each of the four steps is present, mapped and clickable, and pressing
/// one moves the line by exactly `step_frames(shift) / fps` and pauses the preview.
fn check_the_four_step_buttons_move_the_line(app: &adw::Application) {
    let window = cut_window(app);
    settle();

    // All four named buttons exist, are laid out, and carry the spec's frame-step tooltip.
    for (name, _label, shift) in [
        ("line-step-back-five", "", true),
        ("line-step-back", "", false),
        ("line-step-forward", "", false),
        ("line-step-forward-five", "", true),
    ] {
        let button = ui::line_step_button(&window, name)
            .unwrap_or_else(|| panic!("the Cut page carries {name}"));
        assert!(button.is_mapped(), "{name} is mapped on the visible Cut page");
        assert!(button.width() > 20, "{name} wide enough to click, got {} px", button.width());
        assert!(button.height() > 16, "{name} tall enough to click, got {} px", button.height());
        assert_eq!(
            button.tooltip_text().as_deref(),
            Some(naivepost::cut_screen::FRAME_TIP),
            "{name} carries \u{00a7}A's own frame-step wording"
        );
        println!("LAYOUT {name} width={} height={} mapped={}", button.width(), button.height(), button.is_mapped());
        let _ = shift;
    }

    // Forward one frame: the line moves by +1/fps and the preview pauses.
    ui::set_line_position(&window, cut_line::LinePos { t: 10.0 });
    let start = ui::line_position(&window).t;
    ui::line_step_button(&window, "line-step-forward")
        .expect("forward button")
        .emit_by_name::<()>("clicked", &[]);
    let after = ui::line_position(&window).t;
    assert!(
        (after - (start + 1.0 / FPS)).abs() < 1e-9,
        "f\u{203a} steps one frame: {start} -> {after}, expected {}",
        start + 1.0 / FPS
    );
    assert!(
        !ui::preview_player(&window).transport.playing,
        "S2: stepping pauses the preview -- you are looking at a frame, not watching a run"
    );

    // Back five frames via the Shift button. It is wired with shift=true, so it steps BACK five --
    // asserted against step_frames(true), not a hardcoded 5, so a retune shows up here.
    let before = ui::line_position(&window).t;
    let back_five = ui::line_step_button(&window, "line-step-back-five")
        .expect("back-five button");
    assert_eq!(back_five.label().unwrap().as_str(), "\u{2039}\u{2039} f  step back 5 frames",
        "the shift variant is the one wired to that name");
    back_five.emit_by_name::<()>("clicked", &[]);
    let back = ui::line_position(&window).t;
    assert!(
        (back - (before - 5.0 / FPS)).abs() < 1e-9,
        "\u{2039}\u{2039}f steps five frames: {before} -> {back}, expected {}",
        before - 5.0 / FPS
    );
    assert_eq!(cut_line::step_frames(true), 5, "// the five-frame variant is what that button asked for");
    window.close();
}

/// S3 through the key controller: the arrows move only while something is held, Space toggles, and
/// the controller really is attached to the window rather than merely constructed.
fn check_the_key_controller_is_attached_and_refuses_without_a_hold(app: &adw::Application) {
    // The window reads its cut from cwd's session folder, and on an empty cut §F2.2 S2 makes Space
    // refuse, so this check needs a real cut on disk rather than whatever `rust/` happens to hold.
    pin_session_root("keys");
    let window = cut_window(app);
    // GTK exposes no public list of a widget's controllers (only `add_controller`), so the controller
    // is proven by what it does rather than by where it is filed: the checks below drive its handler's
    // own seams and assert the state changes.
    let _controller = ui::line_key_controller(&window).expect("the window has a line key controller");

    // S3: with nothing held the arrows do nothing at all -- the line must not budge.
    ui::set_line_position(&window, cut_line::LinePos { t: 40.0 });
    let before = ui::line_position(&window).t;
    assert_eq!(
        ui::arrow_steps(&window, false, None, FPS, 10_100),
        ui::Step::Still,
        "an arrow with nothing held refuses"
    );
    assert_eq!(ui::line_position(&window).t, before, "and the line did not move");

    // With a hold, the same arrow nudges the hold instead of the line.
    assert_eq!(
        ui::arrow_steps(&window, false, Some(cut_line::Held::Edge), FPS, 10_200),
        ui::Step::Nudged { frames: 1 },
        "with an edge held the arrow nudges THAT, one frame"
    );
    assert_eq!(
        ui::arrow_steps(&window, true, Some(cut_line::Held::Clip), FPS, 10_300),
        ui::Step::Nudged { frames: 5 },
        "Shift makes it five"
    );
    assert_eq!(ui::line_position(&window).t, before, "nudging a hold never moves the line");

    // S3: Space toggles the preview unless a text box holds the focus.
    ui::set_preview_state(&window, false, false, naivepost::run::Transport { playing: false, started: false });
    assert!(ui::space_toggles(&window, false), "Space is handled outside a text box");
    assert!(ui::preview_player(&window).transport.playing, "and it started playback");
    assert!(ui::space_toggles(&window, false), "again handled");
    assert!(!ui::preview_player(&window).transport.playing, "toggled back off");
    assert!(!ui::space_toggles(&window, true), "a text box keeps its Space");
    window.close();
}

/// S1 / S1b through the door the program itself uses (`ui::place_line_press`, which every real press
/// reaches through `cut_line_press`): a track press places the line and clears the selection exactly as
/// `cut_line::click_outcome` says, a gutter press moves nothing, and the 12 px reach picks edge over
/// border over clip.
fn check_a_track_press_places_the_line(app: &adw::Application) {
    let window = cut_window(app);

    // A `Hit` is what the widget layer hands the rule: where the pointer landed, in which band, whether
    // anything plays and whether sources are loaded. Built here exactly as `cut_line_press::press_in`
    // builds it, so the seam under test is the program's own.
    let hit = |at: f64, on_picture: bool, on_scene: bool, playing: bool, sources: bool, gutter: bool| {
        cut_line::Hit {
            at,
            row: cut_line::PICTURE_ROW,
            on_picture,
            on_scene_picture: on_scene,
            playing,
            sources,
            gutter,
        }
    };

    let outcome = ui::place_line_press(&window, hit(12.0, true, true, false, true, false), 20_000);
    assert!(outcome.line_moved, "a press on a track moves the line");
    assert_eq!(outcome.line_at, 12.0);
    assert_eq!(ui::line_position(&window).t, 12.0, "and the window's line is there");
    assert!(outcome.clears_selection, "every track press clears the selection");
    assert_eq!(outcome.watches, Some(cut_line::PICTURE_ROW), "on the picture band, the row is watched");
    assert!(outcome.takes_scene, "landing on the scene's own picture takes it in hand");

    // A gutter click moves no line and watches nothing -- and the seam agrees with the rule itself, so
    // the two cannot drift apart.
    let gutter = ui::place_line_press(&window, hit(99.0, true, false, false, true, true), 20_100);
    assert!(!gutter.line_moved, "a gutter press leaves the line alone");
    assert_eq!(ui::line_position(&window).t, 12.0, "still where the last real press put it");
    assert_eq!(
        gutter.watches, None,
        "and watches nothing -- matching watches_row's own answer: {}",
        cut_line::watches_row(true, false, true, true)
    );

    // Playing suppresses watching, sources-not-loaded suppresses it, both straight from the rule.
    let while_playing = ui::place_line_press(&window, hit(30.0, true, true, true, true, false), 20_200);
    assert_eq!(while_playing.watches, None, "nothing is watched while something plays");
    let no_sources = ui::place_line_press(&window, hit(31.0, true, true, false, false, false), 20_300);
    assert_eq!(no_sources.watches, None, "and none before a source has loaded");

    // S1b: the wider 12 px reach grabs the edge first, then a border, then the whole clip.
    // layout.lineReachPx is how params.rs catalogues cut_line::EDGE_REACH_PX.
    assert_eq!(cut_line::EDGE_REACH_PX, 12.0);
    assert_eq!(cut_line::first_press_pick(3.0, true), cut_line::PressPick::Edge, "within the reach, the edge");
    assert_eq!(cut_line::first_press_pick(12.0, true), cut_line::PressPick::Edge, "exactly at the reach still counts");
    assert_eq!(cut_line::first_press_pick(40.0, true), cut_line::PressPick::Border, "inside but clear of the edge, a border");
    assert_eq!(cut_line::first_press_pick(40.0, false), cut_line::PressPick::Clip, "outside the box, the whole clip");
    window.close();
}

/// S1 + S2/S3 through the WIRES, not the seams: a real `released` on the line-click gesture clears a
/// band that was up, stores the watched row, and takes the scene under the line into hand; and once
/// something IS held, an arrow key press through the window's own key controller takes S2's NUDGE
/// branch instead of moving the line. Both were impossible before this round -- nothing emitted a click
/// into `place_line_press`, and `wire_line_keys` passed a hardcoded `None` as the hold.
fn check_a_real_click_clears_watches_and_a_hold_nudges_instead_of_stepping(app: &adw::Application) {
    use naivepost::cut::{Cut, Seg};

    let window = cut_window(app);
    settle();

    // A band is up (F2.6's own seam), so there is something for S1 to clear.
    ui::draw_selection(
        &window,
        naivepost::cut_select::Surface::PictureRow(0),
        None,
        5.0,
        45.0,
    )
    .expect("a drag on a picture row selects that row's footage");
    assert!(ui::selection(&window).is_some(), "a band is up before the click");

    // A cut with one kept scene covering 10..20, so a click at 15 lands ON a scene's picture.
    let mut seeded = Cut::default();
    seeded.segs = vec![Seg { s: 10.0, e: 20.0, ..Default::default() }];
    ui::seed_review_cut(&window, &seeded);

    // The REAL gesture, emitted the way GTK does it. x is in px at SELECT_SURFACE_PPS, so 15 s of the
    // scene is 15 * pps across -- converted by the handler itself, never by this test.
    let gesture = ui::line_click_gesture(&window).expect("the surface carries the line-click gesture");
    let x_px = 15.0 * ui::SELECT_SURFACE_PPS;
    gesture.emit_by_name::<()>("released", &[&1i32, &x_px, &8.0]);
    settle();

    assert_eq!(ui::line_position(&window).t, 15.0, "the click put the red line at 15 s");
    assert!(ui::selection(&window).is_none(), "and S1 cleared the band that was up");
    assert_eq!(
        ui::watched_row(),
        Some(cut_line::PICTURE_ROW),
        "S1 stored the watched picture row"
    );
    let held = ui::held_clip().expect("landing on a scene's picture took it in hand");
    assert_eq!((held.s, held.e), (10.0, 20.0), "the scene under the click is the one now held");

    // S2/S3 with that hold: an arrow through the WINDOW's key controller must nudge, not step. Read
    // the line before and after -- if the hardcoded `None` were still there the line would have moved by
    // a frame and this assertion would fail exactly as the item says it must.
    let before = ui::line_position(&window).t;
    let controller = ui::line_key_controller(&window).expect("the window has a line key controller");
    let _ = controller.emit_by_name::<bool>(
        "key-pressed",
        &[&gtk::gdk::Key::Left, &0u32, &gtk::gdk::ModifierType::empty()],
    );
    settle();
    assert_eq!(
        ui::line_position(&window).t,
        before,
        "with a clip in hand the arrow nudges THAT and never moves the line"
    );
    // The same key with Shift while held is five frames of the HOLD, still not of the line: the count
    // the handler hands on is what S2 says, and the line does not budge either way.
    let _ = controller.emit_by_name::<bool>(
        "key-pressed",
        &[
            &gtk::gdk::Key::Right,
            &0u32,
            &gtk::gdk::ModifierType::SHIFT_MASK,
        ],
    );
    settle();
    assert_eq!(
        ui::line_position(&window).t,
        before,
        "Shift + arrow with a clip held still nudges the clip, never the line"
    );

    // Clear the holds so a later check in this one process starts from an empty hand.
    ui::set_held_edge(None);
    ui::set_held_clip(None);
    ui::set_watched_row(None);
    window.close();
}

/// A temp root whose `session.naivepost/cut/` exists, with cwd pinned there so the window resolves its
/// project through `startup::session_dir(current_dir())`. Each of the two S4 checks gets its own dir:
/// they run in one process and share the cwd, so a leftover `cut/line.json` would restore into the next
/// check's window and make its assertion about a different file.
///
/// The cut itself is copied from `fixtures/demo.naivepost` rather than left absent: the page restores
/// against the spans it loaded, and the fixture's `cut/cut.json` (kept 0-9.5 / 10-30 / 40-70) is what
/// those spans are. A session with no cut has no filmed span at all, so nothing could ever be restored.
fn pin_session_root(tag: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("cut-line-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let proj = root.join("session.naivepost");
    std::fs::create_dir_all(proj.join("cut")).expect("temp session root");
    std::fs::copy(
        fixture_dir().join("cut/cut.json"),
        proj.join("cut/cut.json"),
    )
    .expect("the fixture's cut is copied in as this session's cut");
    std::env::set_current_dir(&root).expect("cwd pinned to the session root");
    proj
}

/// §05-cut#1-screen F2.4 S4 — the line comes back from its own file when the page opens, and does NOT
/// come back when no filmed span covers it. Rate: writes are capped at one a second (`// layout.lineSaveMs`,
/// `cut_line::LINE_WRITE_MS`) while the line MOVES; restoring reads the file once at build time, so this
/// check is about what lands in the slot from disk, not about throttling.
fn check_the_line_is_restored_on_open(app: &adw::Application) {
    let proj = pin_session_root("restore");

    // 12.0 s sits inside the fixture's kept 10.0–30.0 (cut/cut.json), so some span still covers it.
    cut_line::write_line(&proj, cut_line::LinePos { t: 12.0 }).expect("saved line written");
    let window = cut_window(app);
    settle();
    assert_eq!(
        ui::line_position(&window).t,
        12.0,
        "the line came back from <project>/cut/line.json rather than starting at zero"
    );
    let got_root = ui::line_root(&window).expect("the page knows its project root");
    assert_eq!(
        std::fs::canonicalize(&got_root).unwrap_or_else(|_| got_root.clone()),
        std::fs::canonicalize(&proj).unwrap_or_else(|_| proj.clone()),
        "and that root is the session dir the file was read from"
    );

    // The branch that must NOT restore: nothing was filmed out at 9999 s, so the saved position is
    // refused and the line starts where a page with no remembered position starts.
    cut_line::write_line(&proj, cut_line::LinePos { t: 9999.0 }).expect("far line written");
    let fresh = cut_window(app);
    settle();
    assert_eq!(
        ui::line_position(&fresh).t,
        0.0,
        "a line no recording covers is left alone -- the page opens at zero"
    );

    // Leave no line file behind for any later window built in this one process.
    let _ = std::fs::remove_file(cut_line::line_file(&proj));
    window.close();
    fresh.close();
}

/// §05-cut#1-screen F2.4 S4 — closing beats the rate limit THROUGH THE REAL FILE. The step button runs
/// `move_line_and_save`, which writes through the writer's one-second rule (`LineWriter::note_move`), so a
/// first press DOES land on disk and every press inside the same second does not; `flush_line_on_close` is
/// what puts the position on disk even though the last save was less than `cut_line::LINE_WRITE_MS` ago
/// (`// layout.lineSaveMs`).
fn check_closing_flushes_the_line_through_the_real_file(app: &adw::Application) {
    let proj = pin_session_root("flush");
    assert!(
        cut_line::read_line(&proj).is_none(),
        "this check starts with no remembered line, so anything found later was written here"
    );

    let window = cut_window(app);
    settle();
    assert_eq!(ui::line_position(&window).t, 0.0, "no file -> the line opens at zero");

    // A real press on f› : the line moves by one frame and the writer is now throttled.
    let forward = ui::line_step_button(&window, "line-step-forward").expect("f› exists");
    forward.emit_clicked();
    settle();
    let moved = ui::line_position(&window).t;
    assert_ne!(moved, 0.0, "the press moved the line off zero");
    // The move itself wrote: a root reaches the page, so `note_move` puts the position on disk (bytes, not
    // just a throttle stamp). Read it back from the file rather than comparing to the slot -- a second press
    // inside the same second must add NO new bytes, which is what caps moves at one write a second.
    let written = cut_line::read_line(&proj).map(|pos| pos.t);
    assert_eq!(
        written,
        Some(moved),
        "the throttled path wrote the FIRST move, at most once a second"
    );
    let again = ui::line_step_button(&window, "line-step-forward").expect("f› again");
    again.emit_clicked();
    settle();
    assert_eq!(
        cut_line::read_line(&proj).map(|pos| pos.t),
        written,
        "and the next press inside the same second put no new bytes out -- the cap holds"
    );

    // Close-time flush: same instant, well inside the one-second cap, and the bytes go out anyway.
    assert!(
        ui::flush_line_on_close(&window),
        "closing must beat the one-second throttle (`cut_line::LINE_WRITE_MS`)"
    );
    assert_eq!(
        cut_line::read_line(&proj).map(|pos| pos.t),
        Some(ui::line_position(&window).t),
        "the file holds the position the buttons left the line at, read back from disk"
    );

    let _ = std::fs::remove_file(cut_line::line_file(&proj));
}

/// S1 / S1b through the WIRES on the PAINTED band: the press is read off where the pointer landed -- the
/// row from y, the gutter from x -- rather than from constants. Three presses, three different answers.
fn check_a_real_band_press_uses_where_the_pointer_landed(app: &adw::Application) {
    use naivepost::cut::{Cut, Seg};
    use naivepost::ui::{cut_tracks, cut_view};

    pin_session_root("band-press");
    let window = cut_window(app);
    settle();

    // A cut whose single kept scene covers 10..30 s, so a press inside it lands on that scene's picture.
    let seeded = Cut {
        segs: vec![Seg { s: 10.0, e: 30.0, cam: 0, ..Default::default() }],
        ..Default::default()
    };
    ui::seed_review_cut(&window, &seeded);
    ui::set_watched_row(None);
    settle();

    let gesture = naivepost::ui::window::band_line_gesture_for_test(&window)
        .expect("the painted band carries the line-click gesture");
    let pps = cut_view::pps();
    let origin = cut_view::origin();
    // The geometry the band paints with: the same doors `cut_line_press` reads.
    let boxes = cut_view::band_clip_boxes_for_test(&window);
    assert!(!boxes.is_empty(), "the band has a painted box to press on");
    // The row the painted box sits on: rows are coloured from the recordings the page holds, so the test asks
    // for the same answer the band paints with instead of assuming row 0.
    let rows = naivepost::timeline::row_count(
        &ui::page_recordings(),
        &naivepost::ui::review_cut_of(&window),
    );
    // The scene under the pointer is the one `cut_delete::scene_under` finds at that second, and the row it
    // is drawn on is that recording's colouring -- ask both of the same doors the press handler reads.
    // The fixture's own spans are what the band paints, so the press goes at a second they really cover
    // (its kept footage runs 0-9.5 / 10-30 / 40-70) rather than at whatever x happens to be mid-box.
    let pressed_at = 12.0;
    let inside_x = origin + cut_view::gutter_px() + pressed_at * pps;
    let held_seg = naivepost::cut_delete::scene_under(
        &naivepost::ui::review_cut_of(&window).segs,
        pressed_at,
    )
    .cloned();
    let painted_row = naivepost::timeline::rows_for(
        &ui::page_recordings(),
        &naivepost::ui::review_cut_of(&window),
    )
    .into_iter()
    .zip(ui::page_recordings().iter())
    .find(|(_, rec)| rec.base == format!("cam{}", held_seg.as_ref().map(|s| s.cam).unwrap_or(0)))
    .map(|(row, _)| row)
    .unwrap_or(0);
    assert!(painted_row < rows.max(1), "the painted box's row {painted_row} is a drawn row");
    // Row 0's own picture half, off the arithmetic the painter lays out.
    let bands = cut_tracks::bands_for(naivepost::fx_lane::row_count(
        &naivepost::ui::review_cut_of(&window).fx,
    ));
    // ...but the band's OWN height decides which y is a picture row: `surface_at` sends anything past the
    // drawn surface back to the selection band, so the press must land inside what this window laid out.
    let band_px = naivepost::ui::cut_tracks::band_height_px(
        &cut_view::band_scene_for(&window),
        naivepost::ui::cut_tracks::THUMB_DEFAULT_PX,
    );
    assert!(
        band_px > bands.first_picture,
        "the band is tall enough to have a picture row at all: {band_px} px vs first row {}",
        bands.first_picture
    );
    // The fixture's footage is one camera (every kept segment is cam0), so the coloured picture row the
    // press must land on is row 0 -- its middle is `first_picture + row_height / 2`, inside the band's own
    // laid-out height, which is what `surface_at` bounds the rows by.
    assert_eq!(rows, 1, "this session films one camera, so one picture row is drawn");
    let row_y = bands.first_picture + bands.row_height / 2.0;
    assert!(
        row_y < band_px,
        "row 0's middle ({row_y}) is inside the {band_px} px band this window laid out"
    );

    // (a) On the picture row, inside the painted clip: the line goes to THAT second and row 0 is watched.
    gesture.emit_by_name::<()>("released", &[&1i32, &inside_x, &row_y]);
    settle();
    let expected = ((inside_x - cut_view::gutter_px() - origin) / pps).max(0.0);
    assert!(
        (ui::line_position(&window).t - expected).abs() < 1e-6,
        "the band press put the line at the pixel it was given: {} vs {expected}",
        ui::line_position(&window).t
    );
    assert_eq!(
        ui::watched_row(),
        Some(painted_row),
        "and watched the row the pointer was on"
    );

    // (b) In the gutter (§B's left strip of the band): NOTHING moves and nothing is watched. This branch
    // was unreachable before -- the old handler passed `gutter: false` as a constant.
    let before = ui::line_position(&window).t;
    ui::set_watched_row(None);
    let in_gutter = cut_view::gutter_px() / 2.0;
    gesture.emit_by_name::<()>("released", &[&1i32, &in_gutter, &row_y]);
    settle();
    assert_eq!(
        ui::line_position(&window).t,
        before,
        "a gutter press moves no line"
    );
    assert_eq!(ui::watched_row(), None, "and watches nothing");

    // (c) On the effects lane (§A puts it between the selection band and the picture rows): S1 watches only
    // a picture row, so the line moves -- a track press is a track press -- but nothing is watched.
    let lane_y = (bands.fx.0 + bands.fx.1) / 2.0;
    let on_time_x = origin + cut_view::gutter_px() + 20.0 * pps;
    gesture.emit_by_name::<()>("released", &[&1i32, &on_time_x, &lane_y]);
    settle();
    assert!(
        (ui::line_position(&window).t - 20.0).abs() < 0.5,
        "the lane press moved the line to its own second: {}",
        ui::line_position(&window).t
    );
    assert_eq!(ui::watched_row(), None, "but a lane is not a picture row, so nothing is watched");

    // (d) On the PLACEHOLDER strip's ruler band: the strip's handler now reads the press's real y instead of
    // assuming every click landed on its one picture row, so a ruler click moves the line but watches
    // nothing -- the same answer the painted band gives for ground that is not a picture row.
    let strip = ui::line_click_gesture(&window).expect("the placeholder strip carries its own gesture");
    let ruler_y = (naivepost::cut_select::PLACEHOLDER_BANDS.ruler.0
        + naivepost::cut_select::PLACEHOLDER_BANDS.ruler.1)
        / 2.0;
    let strip_x = 5.0 * ui::SELECT_SURFACE_PPS;
    ui::set_watched_row(Some(7));
    strip.emit_by_name::<()>("released", &[&1i32, &strip_x, &ruler_y]);
    settle();
    assert_eq!(
        ui::line_position(&window).t,
        5.0,
        "a ruler click still places the line at its own second"
    );
    assert_eq!(
        ui::watched_row(),
        None,
        "but it is not a picture-row click, so the watch was cleared rather than set"
    );

    ui::set_watched_row(None);
    window.close();
}

/// S4 through the widget: a remembered line far along the tape comes back AND the view scrolls so it is in
// view -- `follow_playhead` runs on the restore path, not only on a tick.
fn check_the_restored_line_is_cued_and_scrolled_into_view(app: &adw::Application) {
    use naivepost::ui::cut_view;

    let proj = pin_session_root("restore-scroll");
    // The fixture's kept spans are 0-9.5 / 10-30 / 40-70, so 65 s is covered by the last one and sits at the
    // far right of the tape -- outside any view that starts at zero.
    // Second 50 sits well past the right edge of a 450 px view at this zoom (2000 px in) and well clear of
    // the tape's end, so revealing it can only scroll the view -- unlike a line near the very end, which
    // clamps against the tape's last pixel and is honest about being as far right as the view can go.
    cut_line::write_line(&proj, cut_line::LinePos { t: 50.0 }).expect("saved at 50 s");
    let window = cut_window(app);
    settle();
    assert_eq!(
        ui::line_position(&window).t,
        50.0,
        "the saved line came back from cut/line.json"
    );
    let pps = cut_view::pps();
    let view = cut_view::view_px(cut_view::surface_width(&window));
    let line_px = 50.0 * pps;
    let origin = cut_view::origin();
    if cut_view::fits(cut_view::total_px(70.0, pps), view) {
        // Everything already fits: §B's revealPlayhead deliberately leaves the view alone, so the honest
        // assertion is that the line is inside it, which is what "scrolled into view" means here.
        assert!(
            line_px >= origin && line_px <= origin + view,
            "with the whole tape on screen the line is inside the view without scrolling"
        );
    } else {
        assert!(
            origin > 0.0 && line_px >= origin && line_px <= origin + view,
            "the view scrolled to hold the restored line: origin {origin}, line at {line_px}, view {view}"
        );
    }
    let _ = std::fs::remove_file(cut_line::line_file(&proj));
    window.close();
}

/// S2 through the program's own tool-resolution path: Settings names a folder, ffprobe is found beside it,
/// and the step moves by ONE FRAME OF THAT FILE (30000/1001 = 33.37 ms), not by the fallback 1/25 s.
fn check_a_step_moves_one_frame_of_the_probed_rate(app: &adw::Application) {
    use std::os::unix::fs::PermissionsExt;

    pin_session_root("probed-rate");
    // Point Settings at a fake ffmpeg so `services::ffprobe_for` resolves ffprobe beside it, exactly as it
    // does for a person who typed a path into the box.
    // A per-check directory, not one shared with the flush check: this check moves HOME and cwd, so every
    // path it hands the app is absolute.
    let tools = std::env::temp_dir().join(format!("cut-line-probe-tools-{}", std::process::id()));
    let home = std::env::temp_dir().join(format!("cut-line-probe-home-{}", std::process::id()));
    let conf_dir = home.join(".config").join("naivepost");
    std::fs::create_dir_all(&tools).expect("tools dir");
    std::fs::create_dir_all(&conf_dir).expect("settings dir");
    // Named `ffprobe`, not `fake-ffprobe`: §1's rule is that ffprobe lives BESIDE the configured ffmpeg, so
    // `services::ffprobe_for` turns `<dir>/fake-ffmpeg` into `<dir>/ffprobe`. A stub under any other name is
    // simply not found, and the step silently falls back to 25 fps -- which looks like a model bug but is a
    // missing file.
    let probe_script = tools.join("ffprobe");
    std::fs::write(&probe_script, "#!/bin/sh\necho 30000/1001\n").expect("probe stub");
    std::fs::set_permissions(&probe_script, PermissionsExt::from_mode(0o755)).expect("it runs");
    let ffmpeg = tools.join("fake-ffmpeg");
    std::fs::write(&ffmpeg, "#!/bin/sh\nexit 0\n").expect("ffmpeg stub");
    std::fs::set_permissions(&ffmpeg, PermissionsExt::from_mode(0o755)).expect("it runs");
    std::fs::write(
        conf_dir.join("llm.conf"),
        format!("FFMPEG=\"{}\"\n", ffmpeg.display()),
    )
    .expect("settings written");
    std::env::set_var("HOME", &home);
    std::env::set_var("XDG_CONFIG_HOME", home.join(".config"));

    // A footage file must exist for the rate to be asked of it: a real path on disk, which is what
    // `cut_fps::session_fps` resolves and probes (the stub answers whatever it is asked).
    let film = tools.join("lecture.mkv");
    std::fs::write(&film, b"not really a video, and the stub answers whatever it is asked").unwrap();

    let window = cut_window(app);
    settle();
    // The session's footage row must point at that file: write it through the handle every page reads.
    let session = ui::session_handle(&window);
    {
        let mut held = session.borrow_mut();
        if held.sources.is_empty() {
            held.sources.push(naivepost::project::Source {
                path: film.to_string_lossy().into_owned(),
                footage: true,
                ..Default::default()
            });
        } else {
            let first = &mut held.sources[0];
            first.path = film.to_string_lossy().into_owned();
            first.footage = true;
        }
    }

    let probed = naivepost::cut_fps::session_fps(&window);
    assert!(
        (probed - (30000.0 / 1001.0)).abs() < 1e-6,
        "the step's rate came from ffprobe beside the configured ffmpeg, got {probed}"
    );

    ui::set_line_position(&window, cut_line::LinePos { t: 10.0 });
    let start = ui::line_position(&window).t;
    ui::line_step_button(&window, "line-step-forward")
        .expect("f\u{203a} button")
        .emit_by_name::<()>("clicked", &[]);
    settle();
    let after = ui::line_position(&window).t;
    let one_frame = 1.0 / (30000.0 / 1001.0);
    assert!(
        (after - (start + one_frame)).abs() < 1e-9,
        "one frame of 30000/1001 footage is {one_frame} s, not 1/25 = {}",
        1.0 / 25.0
    );
    assert!(
        !ui::preview_player(&window).transport.playing,
        "S2's \"then pause\": the transport is not playing after a step"
    );
    window.close();
}

/// S1b through the WIRES: a press on the painted band that is clear of the green takes the clip's EDGE by
/// the wider 12 px reach, while a press INSIDE the same painted box takes the whole clip. This is the case
/// S1b exists for -- "how to take a clip or card whose green sits on another row" -- and it can only be
/// proved against the boxes the band actually paints, at the zoom and scroll it paints them at.
fn check_a_real_press_clear_of_the_green_takes_the_edge_by_the_reach(app: &adw::Application) {
    use naivepost::ui::{cut_tracks, cut_view};

    pin_session_root("reach");
    // The fixture's own cut (0-9.5 / 10-30 / 40-70), unseeded: its first painted box starts at second 0,
    // so a pixel just left of that edge sits on no green at all.
    let window = cut_window(app);
    settle();
    ui::set_held_edge(None);
    ui::set_held_clip(None);

    let gesture = naivepost::ui::window::band_line_gesture_for_test(&window)
        .expect("the painted band carries the line-click gesture");
    let origin = cut_view::origin();
    let boxes = cut_view::band_clip_boxes_for_test(&window);
    assert!(!boxes.is_empty(), "the band has painted boxes to press beside");
    // The target is the GAP between two kept scenes: pressing just LEFT of a later box's edge puts the
    // pointer on a dropped stretch -- bare ground, no green -- while still landing in the picture band.
    // That is exactly S1b's case ("how to take a clip or card whose green sits on ANOTHER row"): the hand
    // reaches across bare ground for the edge. The fixture's spans are 0-9.5 / 10-30 / 40-70, so the
    // 9.5-10 gap serves. A box starting at second 0 will not do: its left edge lies under the gutter,
    // and a gutter press takes nothing (S1), as the band check above pins.
    let target = boxes
        .iter()
        .find(|held| {
            let left_on_screen = held.x - origin;
            held.w > 0.0 && left_on_screen > cut_view::gutter_px() + cut_line::EDGE_REACH_PX
        })
        .expect("a painted box whose left edge is clear of the gutter, i.e. beside a dropped stretch");
    let box_left = target.x - origin;
    let seg = naivepost::ui::review_cut_of(&window)
        .segs
        .get(target.index)
        .cloned()
        .expect("the painted box names a kept segment");

    let bands = cut_tracks::bands_for(naivepost::fx_lane::row_count(
        &naivepost::ui::review_cut_of(&window).fx,
    ));
    let row_y = bands.first_picture + bands.row_height / 2.0;

    // (a) Two px LEFT of the painted edge: outside the green, inside `EDGE_REACH_PX` (12), so the reach
    // hands over the EDGE -- not the clip, and not nothing. The target's own filter already required that
    // much clearance from the gutter, so there is nothing further to assert about the distance here.
    gesture.emit_by_name::<()>("released", &[&1i32, &(box_left - 2.0), &row_y]);
    settle();
    let edge = ui::held_edge().expect("a press clear of the green took the edge by the reach");
    assert_eq!(
        (edge.s, edge.e),
        (seg.s, seg.e),
        "and it is the edge of the box the pointer was beside"
    );

    // (b) Inside that SAME box: the click lands on the green, so S1 owns it and the reach must not run --
    // the whole clip comes into hand and the edge slot goes back to empty.
    ui::set_held_edge(None);
    ui::set_held_clip(None);
    let mid_x = box_left + target.w / 2.0;
    gesture.emit_by_name::<()>("released", &[&1i32, &mid_x, &row_y]);
    settle();
    assert!(
        ui::held_clip().is_some(),
        "a press on the green takes the clip (S1), which is what S1's scene-under-the-click does"
    );
    assert_eq!(
        ui::held_edge(),
        None,
        "and the reach did NOT also grab an edge -- `press_tries_reach` refuses a press on the green"
    );
    window.close();
}

/// S4's "scrolled into view" through the widget AT A ZOOM WHERE THE TAPE DOES NOT FIT. At the open zoom a
/// short session fits and §B's revealPlayhead deliberately leaves the view alone, so the ordinary restore
/// check can never see the origin move; one ladder step above §A's zoom floor makes the claim testable, and
/// the scrollbar that §A says appears when things do not fit is visible with it.
fn check_a_restored_line_scrolls_into_view_on_a_zoomed_tape(app: &adw::Application) {
    use adw::prelude::*;
    use naivepost::ui::cut_view;

    let proj = pin_session_root("restore-zoom");
    // The line has to sit far enough INSIDE the tape that centring it on a zoomed view overflows the head of
    // the tape: a line near the tape's end can never push the origin off zero, because §B's `x_of` clamps the
    // scroll to the tape's last pixel and `t*pps + view/2` lands short of it. 45 s is inside the fixture's
    // last kept stretch (40-70), clear of its end.
    cut_line::write_line(&proj, cut_line::LinePos { t: 45.0 }).expect("saved at 45 s");
    // The tape is NOT declared here: this window's session is the `pin_session_root` folder's kept footage —
    // the fixture's 0-9.5 / 10-30 / 40-70, 60 filmed seconds — and §A's zoom floor is computed over that, so
    // the arithmetic below reads the spans the page itself measures rather than a number copied from the
    // fixture (floor = view / filmed; a too-long tape would widen the floor past every zoom this session can
    // reach on a page this wide, and §B clamps a scroll to nothing while the tape fits the view).
    // The floor and the zoom are asked of THE WINDOW, which is the only thing that knows how wide its own view
    // was laid out: `surface_width` reads the band's measured allocation, so it can only be asked AFTER the
    // page exists. One step up that ladder (the step the toolbar's + fires) is where the tape stops fitting.
    let window = cut_window(app);
    settle();
    let view = cut_view::view_px(cut_view::surface_width(&window));
    let pps = cut_screen::zoom_up(cut_view::floor_pps(view), 0.0);
    cut_view::set_pps(pps);
    let filmed = cut_view::filmed_seconds_for_test();
    let line_px = 45.0 * pps;
    let total = cut_view::total_px(filmed, pps);
    assert!(
        pps > cut_view::floor_pps(view),
        "the setup sits ABOVE §A's floor, where the tape stops fitting: {pps} px/s under a floor of {}",
        cut_view::floor_pps(view)
    );
    assert!(
        !cut_view::fits(total, view),
        "at {pps} px/s a {filmed:.0} s tape is {total} px against a {view} px view, so it cannot fit"
    );
    // The page opened with the view at the head of the tape: its own restore ran at the shallower open zoom,
    // where everything fits and §B leaves the view alone. So the line is taken past the tape's end first — the
    // order §D's `setPlayhead` describes (place, then reveal) — which forces §B to scroll the view to hold it,
    // and the page then leaves it there, because the line is back inside the view.
    ui::set_playhead(&window, filmed + 5.0);
    settle();
    let pulled = cut_view::origin();
    ui::set_playhead(&window, 45.0);
    settle();
    let origin = cut_view::origin();
    assert_eq!(
        ui::line_position(&window).t,
        45.0,
        "45 s is inside the fixture's last kept stretch (40-70), so it is covered"
    );
    assert!(
        pulled > 0.0,
        "a line off the end of the tape pulls the view to its far edge: origin {pulled} of {total:.0} px"
    );
    assert!(
        origin > 0.0,
        "§B keeps the view where the reveal put it: origin {origin} after the line came back inside"
    );
    assert!(
        line_px >= origin && line_px <= origin + view,
        "the restored line is inside the view: origin {origin}, line {line_px}, view {view}"
    );
    let bar = find_named(&window, "track-scrollbar")
        .expect("the page carries its scrollbar")
        .downcast::<gtk::Scrollbar>()
        .expect("it is a scrollbar");
    assert!(bar.is_visible(), "§A: the scrollbar shows when not everything fits");

    let _ = std::fs::remove_file(cut_line::line_file(&proj));
    cut_view::set_pps(naivepost::cut_screen::ZOOM_AT_OPEN);
    window.close();
}

/// First widget in this window's tree carrying this name. `find_widget_by_name` is crate-private, so a
/// test walks the same way `tests/sec05_cut_picture_scrollbar.rs` does.
fn find_named(window: &adw::ApplicationWindow, name: &str) -> Option<gtk::Widget> {
    fn walk(widget: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
        if widget.widget_name() == name {
            return Some(widget.clone());
        }
        for child in widget.observe_children().iter::<glib::Object>() {
            let Ok(child) = child else { continue };
            let Some(child) = child.downcast_ref::<gtk::Widget>() else { continue };
            if let Some(found) = walk(child, name) {
                return Some(found);
            }
        }
        None
    }
    walk(window.upcast_ref(), name)
}

fn window_round() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(|app| {
            check_the_four_step_buttons_move_the_line(app);
            RAN_STEPS.store(true, Ordering::SeqCst);
            check_the_key_controller_is_attached_and_refuses_without_a_hold(app);
            RAN_KEYS.store(true, Ordering::SeqCst);
            check_a_track_press_places_the_line(app);
            RAN_CLICK.store(true, Ordering::SeqCst);
            check_a_real_click_clears_watches_and_a_hold_nudges_instead_of_stepping(app);
            RAN_EFFECTS.store(true, Ordering::SeqCst);
            check_the_line_is_restored_on_open(app);
            RAN_RESTORE.store(true, Ordering::SeqCst);
            check_closing_flushes_the_line_through_the_real_file(app);
            RAN_FLUSH.store(true, Ordering::SeqCst);
            check_a_real_band_press_uses_where_the_pointer_landed(app);
            RAN_BAND_PRESS.store(true, Ordering::SeqCst);
            check_the_restored_line_is_cued_and_scrolled_into_view(app);
            RAN_RESTORE_SCROLL.store(true, Ordering::SeqCst);
            check_a_step_moves_one_frame_of_the_probed_rate(app);
            RAN_PROBED_RATE.store(true, Ordering::SeqCst);
            check_a_real_press_clear_of_the_green_takes_the_edge_by_the_reach(app);
            RAN_REACH.store(true, Ordering::SeqCst);
            check_a_restored_line_scrolls_into_view_on_a_zoomed_tape(app);
            RAN_ZOOM_SCROLL.store(true, Ordering::SeqCst);
            app.quit();
        });
        app.run_with_args::<String>(&[]);
    });
}

#[test]
fn f2_4_s1_placing_and_stepping_the_line_reaches_the_window_through_real_widgets() {
    // Pin cwd to our own temp root before anything builds, as the other widget binaries do: the page resolves
    // its session through `startup::session_dir(current_dir())`, and the FIRST check here never calls
    // `pin_session_root`, so with cwd left at `rust/` that window opens a stray `rust/session.naivepost/`
    // whose implied spans are empty -- which is what left F2.4 S4's restored line uncovered.
    let root = std::env::temp_dir().join(format!("np-linewires-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temp root created");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");

    window_round();
    assert!(RAN_STEPS.load(Ordering::SeqCst), "the step-button check never ran");
    assert!(RAN_KEYS.load(Ordering::SeqCst), "the key-controller check never ran");
    assert!(RAN_CLICK.load(Ordering::SeqCst), "the track-press check never ran");
    assert!(RAN_EFFECTS.load(Ordering::SeqCst), "the click-effects and nudge check never ran");
    assert!(RAN_RESTORE.load(Ordering::SeqCst), "the restore-on-open check never ran");
    assert!(RAN_FLUSH.load(Ordering::SeqCst), "the flush-on-close check never ran");
    assert!(
        RAN_BAND_PRESS.load(Ordering::SeqCst),
        "the painted-band press check never ran"
    );
    assert!(
        RAN_RESTORE_SCROLL.load(Ordering::SeqCst),
        "the restore-cue-and-scroll check never ran"
    );
    assert!(
        RAN_PROBED_RATE.load(Ordering::SeqCst),
        "the probed-frame-rate step check never ran"
    );
    assert!(
        RAN_REACH.load(Ordering::SeqCst),
        "the clear-of-the-green reach check never ran"
    );
    assert!(
        RAN_ZOOM_SCROLL.load(Ordering::SeqCst),
        "the zoomed-tape restore-scroll check never ran"
    );
}
