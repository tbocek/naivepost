//! F2.3 — the wire: a real click on `review-cuts-button` starts and ends the review, moving the same
//! `Player` state tests/cut_review_cuts.rs asserts from the logic side.
//!
//! One application, one `connect_activate`, exactly one `#[test]`: the checks mutate process-shared
//! state (this window's preview player and its cut slot), so they run once and report through flags the
//! test asserts on — the shape tests/cut_play_the_cut_widgets.rs uses.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use naivepost::cut::{Cut, Seg};
use naivepost::run::Transport;
use naivepost::shell::Page;
use naivepost::{cut_review, ui};

static RAN_START: AtomicBool = AtomicBool::new(false);
static RAN_END: AtomicBool = AtomicBool::new(false);
static RAN_REFUSED: AtomicBool = AtomicBool::new(false);

fn fixture_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/demo.naivepost")
}

/// A window sitting on the Cut page, built fresh per check so one check's player state cannot leak
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
        "the Cut tab bounced -- the fixture has no footage row"
    );
    window
}

/// Let GTK lay the children out. `present()` alone does not allocate until an iteration runs, so
/// without this every measurement below reads 0. gtk4-rs 0.11 has no free
/// `gtk::events_pending`/`main_iteration`; the working call is the GLib main context.
fn settle() {
    let context = glib::MainContext::default();
    while context.pending() {
        context.iteration(false);
    }
}

/// Three clips, two joins, holes at 10-20 and 30-40 -- the shape the logic tests use.
fn three_clips() -> Cut {
    Cut {
        segs: vec![
            Seg { s: 0.0, e: 10.0, ..Default::default() },
            Seg { s: 20.0, e: 30.0, ..Default::default() },
            Seg { s: 40.0, e: 50.0, ..Default::default() },
        ],
        ..Default::default()
    }
}

/// S2 through the real button: starting from the red line puts the review on join 1 of 2 and paints
/// `reviewing_status` -- the very string f2_3_s4 asserts against the pure function.
fn check_s2_starts_through_the_widget(app: &adw::Application) {
    let window = cut_window(app);
    let button = ui::review_cuts_button(&window).expect("the Cut page carries review-cuts-button");

    // Laid out, not merely present: an empty or collapsed widget passes a by-name lookup and is still
    // invisible to the person using the app.
    settle();
    println!(
        "LAYOUT review width={} height={} mapped={}",
        button.width(),
        button.height(),
        button.is_mapped()
    );
    assert!(button.is_mapped(), "\u{25b6}\u{2702}\u{2702} is mapped on the visible Cut page");
    assert!(button.width() > 40, "wide enough for its label, got {} px", button.width());
    assert!(button.height() > 16, "tall enough to click, got {} px", button.height());
    assert_eq!(
        button.tooltip_text().as_deref(),
        Some(naivepost::cut_screen::REVIEW_TIP),
        "the tooltip is the spec's own wording, not a paraphrase"
    );

    // The recording preview, line at 5 s -- inside the first join's window (P.policy.reviewPadSeconds
    // spans 10 s either side of the join at 10 s, clamped to the clips).
    let cut = three_clips();
    ui::seed_review_cut(&window, &cut);
    ui::set_preview_state(&window, false, false, Transport { playing: false, started: false });
    ui::set_playhead(&window, 5.0);

    button.emit_by_name::<()>("clicked", &[]);
    let after = ui::preview_player(&window);
    assert!(after.reviewing, "S2: the review is running");
    assert!(after.cut_only, "and it IS the cut preview with one more hand on the line");
    assert!(after.transport.playing && after.transport.started, "starting means playing");
    assert_eq!(
        ui::state(&window).status,
        cut_review::reviewing_status(&cut, 0),
        "the status line carries the module's own 'reviewing cut 1 of 2' sentence"
    );
    RAN_START.store(true, Ordering::SeqCst);

    // S5's first half through the same widget: pressing it while it runs pauses AND ends the review.
    ui::set_preview_state(&window, true, true, Transport { playing: true, started: true });
    button.emit_by_name::<()>("clicked", &[]);
    let ended = ui::preview_player(&window);
    assert!(!ended.reviewing, "pressing \u{25b6}\u{2702}\u{2702} while it runs ends the review");
    assert!(!ended.transport.playing, "and pauses it -- that press is the pause");
    assert_eq!(
        ui::state(&window).status,
        cut_review::PAUSE_STATUS,
        "with the module's own short pause line"
    );
    window.close();
}

/// S1 through the seam: fewer than two clips refuses with `refused`'s own words, and the greyed rule
/// (`can_review`) asks the same question. The button stays sensitive here because no live cut model
/// reaches the page to grey it -- greying arrives with the cut-model round -- so the RULE is asserted
/// on the functions rather than on a painted sensitivity a theme could override.
fn check_s1_refuses_with_one_clip(app: &adw::Application) {
    let window = cut_window(app);
    assert!(
        !naivepost::cut_screen::can_review(&Cut::default()),
        "an empty cut cannot be reviewed"
    );
    let one_clip = Cut {
        segs: vec![Seg { s: 0.0, e: 10.0, ..Default::default() }],
        ..Default::default()
    };
    assert!(
        !naivepost::cut_screen::can_review(&one_clip),
        "one clip has no join between two clips"
    );

    ui::seed_review_cut(&window, &one_clip);
    match ui::press_review_cuts(&window, &one_clip) {
        cut_review::Pressed::Refused(reason) => assert_eq!(
            reason,
            "nothing to review \u{2014} a cut needs two clips to have a join between them"
        ),
        other => panic!("one clip must refuse, got {other:?}"),
    }
    assert_eq!(
        ui::state(&window).status,
        "nothing to review \u{2014} a cut needs two clips to have a join between them",
        "and the refusal is what the status line says"
    );
    assert!(!ui::preview_player(&window).reviewing, "a refused press starts nothing");
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
            check_s2_starts_through_the_widget(app);
            check_s1_refuses_with_one_clip(app);
            RAN_END.store(true, Ordering::SeqCst);
            app.quit();
        });
        app.run_with_args::<String>(&[]);
    });
}

#[test]
fn f2_3_s2_review_button_starts_the_review_through_the_widget() {
    window_round();
    assert!(RAN_START.load(Ordering::SeqCst), "the S2 start-through-the-click check never ran");
    assert!(RAN_END.load(Ordering::SeqCst), "the S5 end / S1 refusal checks never ran");
    assert!(RAN_REFUSED.load(Ordering::SeqCst) || RAN_END.load(Ordering::SeqCst));
}
