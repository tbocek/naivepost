//! §03-shell.md F0.12 Add sources (from Prepare) — through the widgets. Prepare's "Add source files…"
//! decides nothing: it asks [`naivepost::add_sources`] and forwards the chooser's answer, so what
//! these checks assert is that a click either opened the chooser or was refused, never that anything
//! was copied (spec/00-principles.md §5).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use naivepost::add_sources;
use naivepost::ui;

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/demo.naivepost")
}

/// One running GTK application for this test binary — the same single-main-loop arrangement
/// `tests/rescan_widgets.rs` needs for the same reason: both checks run in one `connect_activate`,
/// and each records that it got that far, so a check that never happened is a failure rather than a
/// quiet pass. The `Once` makes the pair run once for the binary; GTK's main loop may only be started
/// by the thread that will run it, which is why both checks share one application.
fn window_round() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(|app| {
            check_offers_the_chooser(app);
            RAN_OFFER.store(true, Ordering::SeqCst);
            check_refused_during_a_run(app);
            RAN_REFUSED.store(true, Ordering::SeqCst);
            app.quit();
        });
        app.run_with_args::<String>(&[]);
    });
}

static RAN_OFFER: AtomicBool = AtomicBool::new(false);
static RAN_REFUSED: AtomicBool = AtomicBool::new(false);

/// S1 through the page: both widgets are there with §1's labels, and pressing the button opens the
/// chooser — which a headless test never answers — so nothing is added and the status stays quiet.
fn check_offers_the_chooser(app: &adw::Application) {
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();

    let button = ui::add_sources_button(&window).expect("Prepare has an Add source files button");
    assert_eq!(
        button.label().as_deref(),
        Some("Add source files\u{2026}"),
        "§1's wording for badge 9's own row"
    );
    let tick = ui::copy_into_project(&window).expect("the button comes with the copy tick");
    assert_eq!(tick.label().as_deref(), Some("copy into project"));

    button.emit_clicked();
    assert_eq!(ui::state(&window).status, "", "opening a dialog says nothing");
}

/// S0 through the widgets: ▶ starts a run (F0.2), and Add then says why it will not go ahead — no
/// chooser, because a copy is a run of its own.
fn check_refused_during_a_run(app: &adw::Application) {
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();

    ui::play_button(&window).expect("the run bar has a ▶").emit_clicked();
    // ▶ is one button that becomes ⏸ (F0.2): the press above started the run and had nothing to say,
    // so what this asserts is the Add button's own sentence, not the run bar's.
    ui::add_sources_button(&window)
        .expect("Prepare has an Add source files button")
        .emit_clicked();
    assert_eq!(ui::state(&window).status, add_sources::RUN_REFUSAL);
}

#[test]
fn f0_12_s1_the_add_button_and_the_copy_tick_are_on_prepare() {
    window_round();
    assert!(
        RAN_OFFER.load(Ordering::SeqCst),
        "the Add check never ran"
    );
}

/// S0 through the same widgets: refused during a run, with §1's sentence on the status line.
#[test]
fn f0_12_s0_adding_is_refused_while_a_run_is_on() {
    window_round();
    assert!(
        RAN_REFUSED.load(Ordering::SeqCst),
        "the refusal check never ran"
    );
}
