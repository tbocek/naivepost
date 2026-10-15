// F2.9 S2 — the paste line is PLACEABLE. "⧉ Paste at the red line" means where the person is looking, and
// the completion check found the door could not be aimed: `paste_line` answered with any player object's
// `playhead`, which a PAUSED preview leaves standing at whatever second it was parked on, so a click on the
// band afterwards moved the stored line and the paste ignored it. The rule now: a transport that is actually
// PLAYING answers with its playhead; anything else falls through to the line the last press placed.
//
// One application, one `connect_activate`, one `#[test]`, results through flags — the shape
// tests/cut_copy_paste_lane_widgets.rs uses, because these checks share this window's cut slot, its copy in
// hand and its status line.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use naivepost::cut::{self, Seg};
use naivepost::run::Transport;
use naivepost::shell::Page;
use naivepost::ui;
use naivepost::ui::cut_view;

#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::{fixture_dir, settle, status_text};

static RAN_PARKED: AtomicBool = AtomicBool::new(false);
static RAN_PLAYING: AtomicBool = AtomicBool::new(false);

fn clip(s: f64, e: f64) -> Seg {
    Seg { s, e, cam: 0, ..Default::default() }
}

/// The Cut page with footage seeded, plus a preview that has been STARTED AND PARKED at `parked_at` — the
/// state that used to hijack the paste: `playhead: Some(parked_at)`, `transport.playing == false`.
fn cut_window(app: &adw::Application) -> adw::ApplicationWindow {
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();
    ui::tab_button(&window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_clicked();
    assert_eq!(ui::state(&window).page, Page::Cut);
    let seeded = cut::Cut {
        segs: vec![clip(0.0, 9.5), clip(10.0, 30.0), clip(40.0, 70.0)],
        ..Default::default()
    };
    ui::seed_review_cut(&window, &seeded);
    settle();
    window
}

/// Park the preview at `at`: started, not playing, playhead left there. Read back so the test cannot be
/// asserting against a player that never got set.
fn park(window: &adw::ApplicationWindow, at: f64) {
    ui::set_playhead(window, at);
    ui::set_preview_state(
        window,
        false,
        false,
        Transport { playing: false, started: true },
    );
    let player = ui::preview_player(window);
    assert_eq!(player.playhead, Some(at), "the preview holds the parked second");
    assert!(!player.transport.playing, "and it is NOT playing — that is the stale case");
    settle();
}

/// Copy twelve seconds of footage into hand, so a paste has something to place.
fn take_footage_hand(window: &adw::ApplicationWindow) {
    ui::draw_selection(window, naivepost::cut_select::Surface::PictureRow(0), None, 12.0, 24.0);
    let said = ui::press_copy(window);
    assert!(said.starts_with("copied "), "⧉ Copy took the band: {said}");
    assert!(ui::copy_hand().is_some(), "the seconds are in hand");
}

/// A parked preview must NOT decide where the paste goes: a real press on the painted band moves the line,
/// and ⧉ Paste reports THAT second in its `at m:ss`.
fn s2_a_band_click_moves_where_the_paste_goes_while_the_preview_is_parked() {
    let app = adw::Application::builder()
        .application_id("org.gtk.naivepost.test.f29pasteline.parked")
        .build();
    let done = std::cell::RefCell::new(None);
    let out = done.clone();
    app.connect_activate(move |app| {
        let window = cut_window(app);
        // The stale number: parked at 5.0, nowhere near where the press will land.
        park(&window, 5.0);
        take_footage_hand(&window);

        // Press the band through its own gesture, at a second the parked playhead does not name. Same x/y
        // arithmetic tests/cut_line_place_step_widgets.rs uses: gutter + scroll + seconds * pps, y inside row
        // 0's picture half off the bands the painter lays out.
        let gesture = naivepost::ui::window::band_line_gesture_for_test(&window)
            .expect("the painted band carries the line-click gesture");
        let pps = cut_view::pps();
        let origin = cut_view::origin();
        let pressed_at = 44.0;
        let x = origin + cut_view::gutter_px() + pressed_at * pps;
        let bands = naivepost::ui::cut_tracks::bands_for(naivepost::fx_lane::row_count(
            &ui::review_cut_of(&window).fx,
        ));
        let y = bands.first_picture + bands.row_height / 2.0;
        gesture.emit_by_name::<()>("released", &[&1i32, &x, &y]);
        settle();

        // (a) The press really moved the stored line, and the parked playhead is still sitting at 5.0 — the
        // two disagree on purpose, which is the whole case.
        let placed = ui::line_position(&window).t;
        assert!(
            (placed - pressed_at).abs() < 0.2,
            "the band press put the line at {} s (asked for {pressed_at})",
            placed
        );
        assert_eq!(
            ui::preview_player(&window).playhead,
            Some(5.0),
            "the parked preview still holds its old second — nothing re-sought it"
        );

        // (b) ⧉ Paste answers the CLICKED second, not the parked one.
        let said = ui::press_paste(&window);
        println!("f2_9 parked-preview paste: {said} (line {placed}, parked 5.0)");
        // `tools::mm_ss` spells it zero-padded on both halves, which is what the sentence embeds.
        let mm_ss = format!("{:02}:{:02}", (placed as i64) / 60, (placed as i64) % 60);
        assert!(
            said.contains(&format!("at {mm_ss} ")),
            "the paste says where the CLICK put the line ({mm_ss}), not where the paused preview sits: {said}"
        );
        assert!(
            !said.contains("at 00:05 "),
            "and it must not answer the stale playhead: {said}"
        );
        assert!(ui::copy_hand().is_none(), "a paste consumes the copy");
        // The wire: press the page's own ⧉ Paste BUTTON with a hand in hand and the status row carries what
        // the door returned (the direct call above proves the rule; this proves the click reaches it).
        ui::draw_selection(
            &window,
            naivepost::cut_select::Surface::PictureRow(0),
            None,
            12.0,
            24.0,
        );
        assert!(ui::press_copy(&window).starts_with("copied "), "back in hand for the button press");
        let paste_button = ui::line_step_button(&window, "paste-button")
            .expect("the Cut page carries paste-button");
        paste_button.emit_by_name::<()>("clicked", &[]);
        settle();
        let shown = status_text(&window);
        assert!(
            shown.starts_with("pasted ") && shown.contains(&format!("at {mm_ss} ")),
            "the button's own press prints the sentence on the status row, aimed at the clicked line: {shown:?}"
        );
        assert!(ui::copy_hand().is_none(), "and that paste consumed the copy too");
        RAN_PARKED.store(true, Ordering::SeqCst);
        out.borrow_mut().replace(app.clone());
        app.quit();
    });
    app.run_with_args::<String>(&[]);
    assert!(RAN_PARKED.load(Ordering::SeqCst), "the parked-preview check ran");
}

/// The other side: while the transport IS playing, its playhead is what the eye is watching, so the paste
/// follows the running preview rather than an older click.
fn s2_a_playing_preview_leads_the_paste() {
    let app = adw::Application::builder()
        .application_id("org.gtk.naivepost.test.f29pasteline.playing")
        .build();
    let done = std::cell::RefCell::new(None);
    let out = done.clone();
    app.connect_activate(move |app| {
        let window = cut_window(app);
        // A click first, so the stored line is somewhere ELSE and the two answers can be told apart.
        ui::set_line_position(&window, naivepost::cut_line::LinePos { t: 44.0 });
        // Then the preview runs, standing at 12.0.
        ui::set_playhead(&window, 12.0);
        ui::set_preview_state(&window, false, false, Transport { playing: true, started: true });
        assert!(ui::preview_player(&window).transport.playing, "the preview is playing");
        settle();
        take_footage_hand(&window);
        let said = ui::press_paste(&window);
        println!("f2_9 playing-preview paste: {said} (stored line 44.0, playhead 12.0)");
        assert!(
            said.contains("at 00:12 "),
            "a running preview leads the paste to the second on screen: {said}"
        );
        assert!(!said.contains("at 00:44 "), "and not to the older click: {said}");
        RAN_PLAYING.store(true, Ordering::SeqCst);
        out.borrow_mut().replace(app.clone());
        app.quit();
    });
    app.run_with_args::<String>(&[]);
    assert!(RAN_PLAYING.load(Ordering::SeqCst), "the playing-preview check ran");
}

#[test]
fn f2_9_s2_the_paste_lands_where_the_page_is_looking() {
    common::pin_private_cwd();
    s2_a_band_click_moves_where_the_paste_goes_while_the_preview_is_parked();
    s2_a_playing_preview_leads_the_paste();
}
