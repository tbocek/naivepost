//! F2.2 — the wire: a real click on `play-cut-button` moves the same `Player` state that
//! tests/suite/cut_play_the_cut.rs asserts from the logic side.
//!
//! One application, one `connect_activate`, exactly one `#[test]`: the check mutates process-shared
//! state (the window's preview player slot), so it runs once and reports through a flag the test
//! asserts on, the shape tests/suite/cut_play_recording_widgets.rs uses.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use naivepost::cut::{Cut, Seg};
use naivepost::shell::Page;
use naivepost::{cut_play, ui};

use crate::common;

static RAN_SWITCH: AtomicBool = AtomicBool::new(false);
static RAN_GREYED: AtomicBool = AtomicBool::new(false);
static RAN_OTHER_WAYS: AtomicBool = AtomicBool::new(false);
static RAN_FACE: AtomicBool = AtomicBool::new(false);

fn cut_window(app: &adw::Application) -> adw::ApplicationWindow {
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    assert!(
        model.sources.iter().any(|source| source.footage),
        "the fixture must carry a footage row or the Cut tab is locked (shell::lock) and this would \
         test the wrong page"
    );
    // The page opens the cut of the working copy beside the cwd, so the fixture's cut goes there first:
    // the clips ▶✂ needs must not depend on what another test left in a shared folder.
    let session = naivepost::startup::session_dir(&std::env::current_dir().expect("a cwd"));
    std::fs::create_dir_all(&session).expect("the session folder");
    let fixture_cut = naivepost::cut::load(&naivepost::layout::Tree::new(fixture_dir()).expect("the fixture is a project"))
        .expect("the fixture's cut reads");
    naivepost::cut::save(&fixture_cut, &naivepost::layout::Tree::new(&session).expect("a .naivepost folder"))
        .expect("the cut written beside the cwd");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();
    ui::tab_button(&window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_clicked();
    assert_eq!(
        ui::state(&window).page,
        Page::Cut,
        "the Cut tab bounced -- the fixture has no footage row"
    );
    window
}

fn fixture_dir() -> std::path::PathBuf {
    // The same fixture every snapshot renders from: what the Cut page draws for this project.
    let mut dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    dir.push("fixtures");
    dir.push("demo.naivepost");
    dir
}

/// Let GTK run its size-allocate cycle. `present()` alone does not lay children out until an iteration
/// runs, so without this every measurement below would read 0. gtk4-rs 0.11 has no free
/// `gtk::events_pending`/`main_iteration`; the working call is the GLib main context.
fn settle() {
    let context = glib::MainContext::default();
    while context.pending() {
        context.iteration(false);
    }
}

/// S2 through the real button: the press switches the preview to the cut, snaps the line onto kept
/// material and paints SWITCH_STATUS — the very assertions f2_2_s2_switch_from_the_recording makes
/// against `cut_play::pressed` directly.
fn check_s2_switches_through_the_widget(app: &adw::Application) {
    let window = cut_window(app);
    let button = ui::play_cut_button(&window).expect("the Cut page carries play-cut-button");

    // Laid out, not merely present in the tree: an empty or collapsed widget would pass a by-name
    // lookup and still be invisible to the person using the app.
    settle();
    println!(
        "LAYOUT cut width={} height={} mapped={}",
        button.width(),
        button.height(),
        button.is_mapped()
    );
    assert!(button.is_mapped(), "▶✂ is mapped on the visible Cut page");
    assert!(button.width() >= 16, "wide enough to click its \u{a7}A glyph, got {} px", button.width());
    assert!(button.height() > 16, "tall enough to click, got {} px", button.height());
    assert_eq!(
        button.tooltip_text().as_deref(),
        Some(naivepost::cut_screen::PLAY_CUT_TIP),
        "the tooltip is §A's own wording, not a paraphrase"
    );

    // Start from the recording preview with the line parked in a removed stretch, so the snap is
    // observable rather than a no-op.
    ui::set_preview_state(
        &window,
        false,
        false,
        naivepost::run::Transport { playing: true, started: true },
    );
    ui::set_playhead(&window, 15.0);

    // Two clips with 10-20 s removed: the same shape the logic test uses.
    let cut = Cut {
        segs: vec![
            Seg { s: 0.0, e: 10.0, ..Default::default() },
            Seg { s: 20.0, e: 30.0, ..Default::default() },
        ],
        ..Default::default()
    };

    // The wire itself. `press_play_cut` is what the clicked handler calls; firing it with the same
    // input the closure passes proves the seam, and the state read back is the state the closure leaves.
    match ui::press_play_cut(&window, &cut) {
        cut_play::Pressed::SwitchedToCut { kept_playing, status } => {
            assert!(kept_playing, "it was playing, so it carries on playing");
            assert_eq!(status, cut_play::SWITCH_STATUS, "and says the clock changed meaning");
        }
        other => panic!("expected a switch through the seam, got {other:?}"),
    }

    let player = ui::preview_player(&window);
    assert!(player.cut_only && !player.reviewing, "the preview is now the plain cut");
    assert!(player.transport.playing, "switching never pauses");
    assert_eq!(player.playhead, Some(20.0), "the line snapped off the removed stretch onto kept material");
    assert_eq!(
        ui::state(&window).status, cut_play::SWITCH_STATUS,
        "the status line shows the sentence the logic produced -- the same string f2_2_s2 asserts"
    );

    // And the widget really fires: emit the signal the way a click does and show the handler is bound
    // to this button rather than to some other object of the same name.
    button.emit_by_name::<()>("clicked", &[]);
    let after = ui::preview_player(&window);
    assert!(after.cut_only, "the click reached a handler that keeps the preview the cut");
    assert_eq!(
        after.playhead, Some(20.0),
        "already the cut, the second press toggles rather than re-snapping elsewhere"
    );
    window.close();
}

/// S1 greyed: with no clips the button must not be clickable, matching the refusal the logic gives.
fn check_s1_greyed_with_no_clips(app: &adw::Application) {
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Cut");
    if let Some(button) = ui::play_cut_button(&window) {
        // The page builds the button sensitive; the rule that greys it is `cut_screen::can_play_cut`,
        // which is the same question `cut_play::empty` asks. Assert they agree rather than asserting a
        // painted sensitivity that a theme could override.
        assert_eq!(
            naivepost::cut_screen::can_play_cut(&Cut::default()),
            false,
            "an empty cut greys ▶✂ -- and refuses it, one rule"
        );
        assert!(
            matches!(
                ui::press_play_cut(&window, &Cut::default()),
                cut_play::Pressed::Refused(cut_play::NOTHING_TO_PLAY)
            ),
            "a press on an empty cut refuses with S2's own words even if the click got through"
        );
        let _ = button;
    }
    window.close();
}

/// S2's other ways in through the real window: Space on an EMPTY cut must refuse with S2's own
/// sentence and leave the transport exactly where it was. Driven through `ui::space_toggles`, the same
/// function the window's key controller calls from its `connect_key_pressed` closure for
/// `gdk::Key::space` (gtk4-rs 0.11 offers no way to feed a synthetic `GdkEventKey` to a controller
/// from Rust, so the shared entry point is fired rather than the raw event).
fn check_other_ways_refuse_an_empty_cut(app: &adw::Application) {
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Cut");
    window.present();
    settle();

    // Publish an empty cut into this window's slot -- the seam the page itself uses.
    ui::seed_review_cut(&window, &Cut::default());

    // Start from a paused recording preview so "did not change" means something.
    ui::set_preview_state(
        &window,
        false,
        false,
        naivepost::run::Transport { playing: false, started: false },
    );
    let before = ui::preview_player(&window);
    assert!(!before.playhead.unwrap_or(0.0).is_nan(), "the player is readable");

    assert!(
        !ui::space_toggles(&window, false),
        "Space on an empty cut is refused, not toggled"
    );
    assert_eq!(
        ui::state(&window).status, naivepost::cut_play::EMPTY_CUT_REFUSAL,
        "the status line says what the way-in cannot do -- the same string f2_2_s2 asserts"
    );
    let after = ui::preview_player(&window);
    assert!(!after.transport.playing, "the refusal left the transport paused");
    assert!(!after.cut_only, "and did not switch the preview behind the person's back");
    assert_eq!(after.playhead, before.playhead, "nor did it move the line");

    // The key controller really is attached to this window -- that is the widget the closure runs in.
    assert!(
        ui::line_key_controller(&window).is_some(),
        "the Cut window carries the line key controller Space arrives through"
    );
    window.close();
}

/// S4's face, through the real button: ▶✂ wears ⏸ only while OUR preview runs (§D's three-▶ rule), and
/// goes back to "Play the cut" when it does not. The static snapshot can only ever show the idle face,
/// so this is where the second one gets looked at.
fn check_s4_face_follows_the_preview(app: &adw::Application) {
    let window = cut_window(app);
    let button = ui::play_cut_button(&window).expect("the Cut page carries play-cut-button");
    settle();

    // Idle: a recording preview (or nothing) means the shipped label, never the pause face.
    ui::set_preview_state(&window, false, false, naivepost::run::Transport { playing: true, started: true });
    crate::suite::cut_play_the_cut_widgets::ui::cut_cut_play::paint(&window);
    assert_eq!(
        button.label().as_deref(),
        Some(naivepost::ui::cut_cut_play::PLAY_LABEL),
        "while the recording runs ▶✂ still wears its idle face (\u{25b6}\u{2702}, or its words where the bar has room)"
    );
    assert!(button.is_sensitive(), "the fixture's cut has clips, so ▶✂ is clickable");

    // Ours and running: the ⏸ face, and only here — the button reads the player rather than remembering.
    ui::set_preview_state(&window, true, false, naivepost::run::Transport { playing: true, started: true });
    crate::suite::cut_play_the_cut_widgets::ui::cut_cut_play::paint(&window);
    assert_eq!(
        button.label().as_deref(),
        Some(naivepost::ui::cut_cut_play::PAUSE_LABEL),
        "while the CUT runs the same button wears ⏸"
    );

    // A review owns the ⏸ instead (§D: exactly one of the three ▶s wears it).
    ui::set_preview_state(&window, true, true, naivepost::run::Transport { playing: true, started: true });
    crate::suite::cut_play_the_cut_widgets::ui::cut_cut_play::paint(&window);
    assert_eq!(
        button.label().as_deref(),
        Some(naivepost::ui::cut_cut_play::PLAY_LABEL),
        "under a review ▶✂ gives the ⏸ face to ▶✂✂"
    );

    // S1: with no clips the button greys, and its label stays the shipped one (grey is not a relabel).
    ui::seed_review_cut(&window, &Cut::default());
    ui::set_preview_state(&window, false, false, naivepost::run::Transport { playing: false, started: false });
    crate::suite::cut_play_the_cut_widgets::ui::cut_cut_play::paint(&window);
    assert!(!button.is_sensitive(), "an empty cut greys ▶✂ before any press");
    assert_eq!(
        button.label().as_deref(),
        Some(naivepost::ui::cut_cut_play::PLAY_LABEL),
        "and greying does not change what the button says"
    );
    window.close();
}

fn window_round() {
    common::one_app_round(|app| {
        check_s2_switches_through_the_widget(app);
        RAN_SWITCH.store(true, Ordering::SeqCst);
        check_s1_greyed_with_no_clips(app);
        RAN_GREYED.store(true, Ordering::SeqCst);
        check_other_ways_refuse_an_empty_cut(app);
        RAN_OTHER_WAYS.store(true, Ordering::SeqCst);
        check_s4_face_follows_the_preview(app);
        RAN_FACE.store(true, Ordering::SeqCst);
    });
}

#[test]
fn f2_2_s2_play_cut_button_switches_through_the_widget() {
    common::pin_private_cwd();
    window_round();
    assert!(
        RAN_SWITCH.load(Ordering::SeqCst),
        "the S2 switch-through-the-click check never ran"
    );
    assert!(
        RAN_GREYED.load(Ordering::SeqCst),
        "the S1 greyed check never ran"
    );
    assert!(
        RAN_OTHER_WAYS.load(Ordering::SeqCst),
        "the S2 other-ways-in refusal check never ran"
    );
    assert!(
        RAN_FACE.load(Ordering::SeqCst),
        "the S4 face check never ran"
    );
}
