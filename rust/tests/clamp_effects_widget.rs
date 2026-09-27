//! F3.12 — Clamp to the cut as applied: the WIRE. A press on － Remove applies a new segment list, and
//! the effects chosen against the OLD seconds must be held to the new ones in the same step, with the
//! count reaching the log and the sentence reaching the status line.
//!
//! `rust/tests/cut_clamped_to_cut.rs` proves the rules inside [`naivepost::cut_clamp`]; this file proves
//! the Cut page actually reaches them. Two doors are checked: the real widget (`remove-button` →
//! `press_remove` → `report_verb` → `ui::clamp_effects_to_cut`) and the seam called directly, so a
//! regression in either the wiring or the seam's own lines shows up here.
//!
//! One application, one `connect_activate`, exactly one `#[test]`: these checks mutate process-shared
//! state (this window's selection, cut and history), so they run once and report through flags the test
//! asserts on — the shape tests/speeds_pass_widgets.rs uses.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut::{Cut, Fx, Seg};
use naivepost::cut_clamp::{self, MIN_SURVIVING_SECONDS};
use naivepost::cut_select::Surface;
use naivepost::shell::Page;
use naivepost::ui;

static RAN_VERB_CLAMP: AtomicBool = AtomicBool::new(false);
static RAN_FLOOR: AtomicBool = AtomicBool::new(false);
static RAN_UNDO: AtomicBool = AtomicBool::new(false);
static RAN_NOOP: AtomicBool = AtomicBool::new(false);

fn fixture_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/demo.naivepost")
}

thread_local! {
    /// Strong handle on the window last built, so the next check drops it first: a closed GTK window is not
    /// destroyed and its names stay parented, which would send a lookup to the wrong tree.
    static LAST_WINDOW: std::cell::RefCell<Option<adw::ApplicationWindow>> =
        const { std::cell::RefCell::new(None) };
}

fn release_last_window() {
    LAST_WINDOW.with(|cell| {
        if let Some(old) = cell.borrow_mut().take() {
            old.close();
        }
    });
    settle();
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

fn status_text(window: &adw::ApplicationWindow) -> String {
    ui::find_status(window.upcast_ref())
        .expect("the shell has a status line")
        .text()
        .to_string()
}

/// Build a window sitting on the Cut tab with the given cut seeded, dropping the previous check's window first.
fn cut_page(app: &adw::Application, seeded: &Cut) -> adw::ApplicationWindow {
    release_last_window();
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    assert!(
        model.sources.iter().any(|source| source.footage),
        "the fixture must carry a footage row or the Cut tab is locked (shell::lock)"
    );
    let window = ui::build_window(app, &model, "Prepare");
    LAST_WINDOW.with(|cell| *cell.borrow_mut() = Some(window.clone()));
    window.present();
    ui::tab_button(&window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_by_name::<()>("clicked", &[]);
    ui::reopen_history_on(&window, seeded);
    ui::seed_review_cut(&window, seeded);
    ui::refresh_effects_lane(&window);
    settle();
    window
}

/// A footage scene from `s` to `e` on camera row `cam`.
fn seg(s: f64, e: f64, cam: i32) -> Seg {
    Seg {
        s,
        e,
        cam,
        ..Default::default()
    }
}

/// An effect of `kind` starting at `t` and lasting `dur` (fades included in `dur`, as §1 defines it).
fn fx(kind: &str, t: f64, dur: f64) -> Fx {
    Fx {
        kind: kind.into(),
        t,
        dur,
        ..Default::default()
    }
}

fn approx(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

/// Three kept runs of ten seconds each: 0–10, 10–20, 20–30, so every second the clamp writes down is
/// checkable by hand.
fn three_clips() -> Cut {
    Cut {
        segs: vec![seg(0.0, 10.0, 0), seg(10.0, 20.0, 0), seg(20.0, 30.0, 0)],
        ..Default::default()
    }
}

/// The same three clips carrying one effect doomed by dropping 20–30 (it spans 25–35, all of it over the
/// footage about to go) and one that sits safe inside 0–10.
fn clips_with_a_doomed_zoom() -> Cut {
    let mut cut_ = three_clips();
    cut_.fx = vec![fx("zoom", 25.0, 10.0), fx("zoom", 2.0, 5.0)];
    cut_
}

/// Remove the third clip with the REAL widget, so the clamp runs on the apply path rather than as a call.
fn fire_remove_over(window: &adw::ApplicationWindow, from: f64, to: f64) {
    ui::draw_selection(window, Surface::PictureRow(0), None, from, to)
        .expect("a drag on a picture row selects that row's footage");
    let button: gtk::Button = ui::line_step_button(window, "remove-button")
        .expect("－ Remove is on the Cut toolbar (cut_verbs::BUTTONS)");
    button.emit_by_name::<()>("clicked", &[]);
    settle();
}

// --- the round ----------------------------------------------------------------------------------------

fn clamp_round(app: &adw::Application) {
    // --- (1) the real door clamps: dropping a clip takes the effect that pointed at it --------------
    let window = cut_page(app, &clips_with_a_doomed_zoom());
    fire_remove_over(&window, 20.0, 30.0);

    let after = ui::review_cut_of(&window);
    assert_eq!(
        after.segs.len(),
        2,
        "－ Remove took the 20–30 clip out of the cut: {:?}",
        after.segs
    );
    let starts: Vec<f64> = after.fx.iter().map(|effect| effect.t).collect();
    assert_eq!(
        starts,
        vec![2.0],
        "the zoom spanning 25–35 went with the footage it pointed at; the one inside 0–10 stayed"
    );
    assert!(
        ui::window_logs()
            .iter()
            .any(|line| *line == cut_clamp::log_line(1)),
        "S5's line reached the log verbatim; logs were {:?}",
        ui::window_logs()
    );
    // The clamp writes its own sentence last: the user reads what the cut refused to keep, not only that
    // ten seconds went away.
    assert_eq!(
        status_text(&window),
        "effects clamped to the cut \u{2014} 1 dropped",
        "the status line carries the clamp's line after the verb's"
    );
    RAN_VERB_CLAMP.store(true, Ordering::SeqCst);
    window.close();
    settle();

    // --- (2) the floor: a 0.4 s survivor is dropped, a 2.5 s one is trimmed and kept ---------------
    // P.eng.effectMinSurvivingSeconds == cut_clamp::MIN_SURVIVING_SECONDS (1.0 s). It is a floor on the
    // SURVIVOR: 0.4 s left of a band is not the effect that was meant, 2.5 s of it still is.
    assert!(
        approx(MIN_SURVIVING_SECONDS, 1.0),
        "the floor named here is 1.0 s of surviving band"
    );
    let mut cut_ = Cut::default();
    cut_.segs = vec![seg(0.0, 10.0, 0)];
    cut_.fx = vec![fx("zoom", 9.6, 0.5), fx("zoom", 7.5, 3.0)];
    let window = cut_page(app, &cut_);
    let said = ui::clamp_effects_to_cut(&window);
    assert_eq!(
        said,
        "effects clamped to the cut \u{2014} 1 dropped",
        "the under-floor band is the one that went"
    );
    let kept = ui::review_cut_of(&window).fx;
    assert_eq!(kept.len(), 1, "only the 2.5 s survivor is left: {kept:?}");
    assert!(approx(kept[0].t, 7.5), "the survivor starts where it was placed");
    assert!(
        approx(kept[0].dur, 2.5),
        "trimmed to the footage the cut keeps: dur was {}",
        kept[0].dur
    );
    assert!(kept[0].dur >= MIN_SURVIVING_SECONDS);
    assert!(
        ui::window_logs()
            .iter()
            .any(|line| *line == cut_clamp::log_line(1)),
        "the seam logged S5's line for the dropped band"
    );
    RAN_FLOOR.store(true, Ordering::SeqCst);
    window.close();
    settle();

    // --- (3) one ↶ takes the clamp back -----------------------------------------------------------
    // The verb and the clamp EACH call `record_edit`, so the clamp is its own undo step: one step back is
    // the state the verb left — new segments, the old effect list still in place. Had the clamp called
    // `publish_cut` alone, the dropped effect would be unreachable.
    let window = cut_page(app, &clips_with_a_doomed_zoom());
    fire_remove_over(&window, 20.0, 30.0);
    assert_eq!(ui::review_cut_of(&window).fx.len(), 1, "clamped first");
    let back = ui::press_undo(&window);
    settle();
    println!("UNDO SAID {back}");
    let restored = ui::review_cut_of(&window);
    assert_eq!(
        restored.fx.len(),
        2,
        "one ↶ put the pre-clamp effect list back: {:?}",
        restored.fx
    );
    assert!(
        restored.fx.iter().any(|effect| approx(effect.t, 25.0)),
        "the dropped zoom is back where it was placed"
    );
    RAN_UNDO.store(true, Ordering::SeqCst);
    window.close();
    settle();

    // --- (4) a no-op writes nothing ---------------------------------------------------------------
    // No dropped, no trimmed: the seam returns its own line and leaves the cut byte-identical, with no
    // S5 line at all — a no-op must not consume an Undo step or log a zero count.
    let mut cut_ = Cut::default();
    cut_.segs = vec![seg(0.0, 10.0, 0)];
    cut_.fx = vec![fx("zoom", 2.0, 5.0)];
    let window = cut_page(app, &cut_);
    let said = ui::clamp_effects_to_cut(&window);
    assert_eq!(
        said,
        "nothing to clamp \u{2014} every effect sits inside the footage the cut keeps"
    );
    assert_eq!(
        ui::review_cut_of(&window).fx,
        cut_.fx,
        "the effect list is exactly what was seeded"
    );
    assert!(
        !ui::window_logs()
            .iter()
            .any(|line| line.starts_with(">>> 0 effect(")),
        "S5 logs only a count above zero; logs were {:?}",
        ui::window_logs()
    );
    RAN_NOOP.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

#[test]
fn f3_12_widget_the_cut_page_reaches_the_clamp() {
    // Pin cwd to our own temp root BEFORE anything builds: the press resolves the project through
    // `startup::session_dir(current_dir())`, and leaving it at rust/ writes a stray
    // `rust/session.naivepost/` into the repo.
    let root = std::env::temp_dir().join(format!("np-f312-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temp root created");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");

    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(clamp_round);
        app.run();
    });

    assert!(
        RAN_VERB_CLAMP.load(Ordering::SeqCst),
        "the real Remove press never reached the clamp"
    );
    assert!(
        RAN_FLOOR.load(Ordering::SeqCst),
        "the surviving-seconds floor never ran"
    );
    assert!(
        RAN_UNDO.load(Ordering::SeqCst),
        "the undo of the clamp never ran"
    );
    assert!(
        RAN_NOOP.load(Ordering::SeqCst),
        "the no-op branch never ran"
    );
}
