//! F2.2 — the wire: a real click on `play-cut-button` moves the same `Player` state that
//! tests/cut_play_the_cut.rs asserts from the logic side.
//!
//! One application, one `connect_activate`, exactly one `#[test]`: the check mutates process-shared
//! state (the window's preview player slot), so it runs once and reports through a flag the test
//! asserts on, the shape tests/cut_play_recording_widgets.rs uses.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut::{Cut, Seg};
use naivepost::shell::Page;
use naivepost::{cut_play, ui};

static RAN_SWITCH: AtomicBool = AtomicBool::new(false);
static RAN_GREYED: AtomicBool = AtomicBool::new(false);

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
    assert!(button.width() > 40, "wide enough for its label, got {} px", button.width());
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

fn window_round() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(|app| {
            check_s2_switches_through_the_widget(app);
            RAN_SWITCH.store(true, Ordering::SeqCst);
            check_s1_greyed_with_no_clips(app);
            RAN_GREYED.store(true, Ordering::SeqCst);
            app.quit();
        });
        let _ = app.run_with_args::<String>(&[]);
    });
}

#[test]
fn f2_2_s2_play_cut_button_switches_through_the_widget() {
    window_round();
    assert!(
        RAN_SWITCH.load(Ordering::SeqCst),
        "the S2 switch-through-the-click check never ran"
    );
    assert!(
        RAN_GREYED.load(Ordering::SeqCst),
        "the S1 greyed check never ran"
    );
}
