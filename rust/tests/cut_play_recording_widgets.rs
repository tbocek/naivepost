//! F2.1 — Play the recording (▶) through the widgets.
//!
//! The Cut page's ▶ decides nothing: it calls [`naivepost::ui::press_play_recording`], which forwards
//! to [`naivepost::preview::press_recording`] and paints what comes back (spec/00-principles.md §5).
//! `tests/cut_play_recording.rs` proves the rules; this proves the wire — that a real click on a real
//! named button moves the same `Player` state and puts the same sentence on the status line.
//!
//! One application, one `connect_activate`, exactly one `#[test]`: the checks mutate process-shared
//! thread-locals (the preview player and the window log), so they must not run beside anything else in
//! this binary. Same arrangement as `tests/add_sources_widgets.rs`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::preview::{self, Press};
use naivepost::run::Transport;
use naivepost::shell::Page;
use naivepost::ui;

static RAN_WIDGET: AtomicBool = AtomicBool::new(false);
static RAN_LAYOUT: AtomicBool = AtomicBool::new(false);
static RAN_S2: AtomicBool = AtomicBool::new(false);
static RAN_S1: AtomicBool = AtomicBool::new(false);
static RAN_REFUSAL: AtomicBool = AtomicBool::new(false);

fn window_round() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(|app| {
            check_button_exists_with_the_specs_tooltip(app);
            RAN_WIDGET.store(true, Ordering::SeqCst);
            check_the_button_is_laid_out_on_the_page(app);
            RAN_LAYOUT.store(true, Ordering::SeqCst);
            check_s2_plays_from_the_red_line_through_the_click(app);
            RAN_S2.store(true, Ordering::SeqCst);
            check_s1_switches_back_to_the_recording_through_the_click(app);
            RAN_S1.store(true, Ordering::SeqCst);
            check_nothing_filmed_refuses_through_the_click(app);
            RAN_REFUSAL.store(true, Ordering::SeqCst);
            app.quit();
        });
        app.run_with_args::<String>(&[]);
    });
}

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/demo.naivepost")
}

/// A window sitting on the Cut page. Built fresh per check so one check's player state cannot leak
/// into another's assertions; the newest window is the one every accessor reads.
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
        "the Cut tab bounced -- S2's lock means the fixture has no footage row"
    );
    window
}

/// The Cut page carries its own ▶ (§F2.1), labelled and tooled with the spec's own words.
fn check_button_exists_with_the_specs_tooltip(app: &adw::Application) {
    let window = cut_window(app);
    let button = ui::play_recording_button(&window)
        .expect("the Cut page has a play-the-recording button");
    let tip = button.tooltip_text().unwrap_or_default();
    assert!(
        tip.contains("cuts and all"),
        "the tooltip says what §F2.1 S2 says: {tip:?}"
    );
    assert!(
        button.label().as_deref().is_some_and(|label| label.contains("Play the recording")),
        "the button says what it plays: {:?}",
        button.label()
    );
    window.close();
}

/// The button is not just in the tree — it is laid out and mapped where the page shows it. Measured
/// rather than eyeballed: a widget that exists but collapses to 0x0, or never gets allocated because
/// its page was left unmapped, would pass a `find_widget_by_name` check and still be invisible to the
/// person using the app. These are the numbers "nothing is empty, overlapping or cut off" asks for.
fn check_the_button_is_laid_out_on_the_page(app: &adw::Application) {
    let window = cut_window(app);
    // Let GTK run its size-allocate cycle: `present()` alone does not lay children out until an
    // iteration runs, so without this every measurement below would read 0.
    let context = glib::MainContext::default();
    while context.pending() {
        context.iteration(false);
    }
    let button = ui::play_recording_button(&window).expect("the Cut page's ▶");
    println!(
        "LAYOUT width={} height={} mapped={}",
        button.width(),
        button.height(),
        button.is_mapped()
    );
    assert!(button.is_mapped(), "the button is mapped on the visible Cut page");
    assert!(
        button.width() > 40,
        "wide enough for \"▶ Play the recording\", got {} px",
        button.width()
    );
    assert!(
        button.height() > 16,
        "tall enough to click, got {} px",
        button.height()
    );
    window.close();
}

/// S2 through the click: play from the red line, then pause without losing what ⏹ owns.
fn check_s2_plays_from_the_red_line_through_the_click(app: &adw::Application) {
    let window = cut_window(app);
    ui::set_playhead(&window, 12.5);
    let button = ui::play_recording_button(&window).expect("the Cut page's ▶");

    button.emit_clicked();
    let playing = ui::preview_player(&window);
    assert!(playing.transport.playing, "the click started playback");
    assert!(playing.transport.started, "and the preview is now the bar's business");
    assert_eq!(
        playing.playhead,
        Some(12.5),
        "S2 plays FROM the red line -- the line did not jump to 0"
    );

    button.emit_clicked();
    let paused = ui::preview_player(&window);
    assert!(!paused.transport.playing, "the second click paused it");
    assert!(
        paused.transport.started,
        "`started` survives a pause: ⏹ still has something to end (run.rs)"
    );
    assert_eq!(paused.playhead, Some(12.5), "pausing leaves the line where it was");
    window.close();
}

/// S1 through the click: leaving either ✂ preview switches to the recording and says so, and does not
/// stop what was already running.
fn check_s1_switches_back_to_the_recording_through_the_click(app: &adw::Application) {
    // From the ✂ cut preview.
    let window = cut_window(app);
    ui::set_playhead(&window, 30.0);
    ui::set_preview_state(&window, true, false, Transport::default());
    ui::play_recording_button(&window).expect("▶").emit_clicked();

    let after = ui::preview_player(&window);
    assert!(!after.cut_only, "the preview is the recording now");
    assert_eq!(
        ui::state(&window).status,
        preview::RECORDING_STATUS,
        "the clock changed meaning and the status line says exactly that"
    );
    assert_eq!(after.playhead, Some(30.0), "switching costs the red line nothing");
    window.close();

    // From a running review: "already playing: carry on" -- the switch must not pause it.
    let reviewing = cut_window(app);
    ui::set_playhead(&reviewing, 30.0);
    // The review is already running when ▶ is pressed -- §F2.1 S1's "already playing: carry on".
    ui::set_preview_state(
        &reviewing,
        false,
        true,
        Transport { playing: true, started: true },
    );
    ui::play_recording_button(&reviewing)
        .expect("▶")
        .emit_clicked();

    let carried = ui::preview_player(&reviewing);
    assert!(!carried.reviewing, "the review was left behind");
    assert!(
        carried.transport.playing,
        "'already playing: carry on' -- pressing ▶ must not stop the thing its own status names"
    );
    assert_eq!(
        ui::state(&reviewing).status,
        preview::RECORDING_STATUS,
        "the same sentence from the review as from the ✂ preview"
    );
    reviewing.close();
}

/// Nothing filmed and no red line: the press refuses, changes nothing, and names why.
fn check_nothing_filmed_refuses_through_the_click(app: &adw::Application) {
    let window = cut_window(app);
    // The button's own callback passes `&[]` (no Recording model reaches it yet -- see
    // `wire_play_recording`), so clicking is already the empty-runs case; the seam is driven with the
    // same empty list to read the `Press` value back and pin which branch answered.
    let refused = ui::press_play_recording(&window, &[]);
    assert_eq!(refused, Press::NoFootage, "the refusal came from the seam, not from the test");
    let idle = ui::preview_player(&window);
    assert!(!idle.transport.playing, "nothing started");
    assert_eq!(idle.playhead, None, "no line was invented to play from");
    let status = ui::state(&window).status;
    assert!(
        status.contains("no recording"),
        "the status line names the reason: {status:?}"
    );
    window.close();
}

#[test]
fn f2_1_s2_the_recording_button_plays_through_the_widget() {
    window_round();
    assert!(RAN_WIDGET.load(Ordering::SeqCst), "the widget-exists check never ran");
    assert!(
        RAN_LAYOUT.load(Ordering::SeqCst),
        "the laid-out-on-the-page check never ran"
    );
    assert!(RAN_S2.load(Ordering::SeqCst), "the S2 click check never ran");
    assert!(RAN_S1.load(Ordering::SeqCst), "the S1 click check never ran");
    assert!(
        RAN_REFUSAL.load(Ordering::SeqCst),
        "the no-footage click check never ran"
    );
    // S3's fold opening is NOT covered here on purpose: folds are lane state (`[f64; 2]` spans owned by
    // the Cut page's model, F2.11's round) and no fold list is reachable from the window -- the
    // button's callback holds only a Player. `preview::walk_fold` and its "unfolded m:ss -- ▶ ran
    // into it" sentence are proven in tests/cut_play_recording.rs against the logic, and wiring them
    // needs the page's fold model, which is that round's work rather than a stub here.
}
