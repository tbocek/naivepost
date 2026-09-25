//! §03-shell.md F0.4 "I'm feeling lucky" — through the widget. The button decides nothing: it
//! forwards its click to [`naivepost::lucky::Chain`] and draws what comes back, so what these checks
//! assert is that the forwarding happened, not what a press means (spec/00-principles.md §5).

use std::path::{Path, PathBuf};

use adw::prelude::*;
use naivepost::lucky;
use naivepost::ui;

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/demo.naivepost")
}

/// One running GTK application for this test binary, taken in turn by the checks below — the same
/// single-main-loop arrangement tests/press_stop_widgets.rs needs (`g_application_run` refuses a
/// second claimant of the default main context). Both scenarios run inside one `activate`, because
/// the gears' meaning depends on whether ▶ already has a run going.
fn window_round() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(|app| {
            refuses_while_a_run_is_going(app);
            starts_and_logs_the_order_when_idle(app);
            RAN.store(true, std::sync::atomic::Ordering::SeqCst);
            app.quit();
        });
        app.run_with_args::<String>(&[]);
    });
}

static RAN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// S1 through the real widget: with ▶ holding a run, pressing the gears is refused and the status line
/// carries the refusal naming ⏹ rather than starting anything.
fn refuses_while_a_run_is_going(app: &adw::Application) {
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();

    let play = ui::play_button(&window).expect("play button");
    let gears = ui::lucky_button(&window).expect("F0.4 puts the gears in the run bar");

    // The button is labelled, not an anonymous icon.
    assert_eq!(
        gears.label().as_deref(),
        Some(lucky::LUCKY_LABEL),
        "the control reads \"I'm feeling lucky\" as §03 names it"
    );

    // Nothing going yet, so the gears are live.
    assert!(gears.is_sensitive(), "an idle bar leaves the gears pressable");

    // Start a run with ▶ so the bar is busy.
    play.emit_clicked();
    assert!(
        ui::state(&window).page == naivepost::shell::Page::Prepare,
        "▶ started Prepare's step"
    );

    gears.emit_clicked();
    assert_eq!(
        ui::state(&window).status,
        lucky::BUSY_REFUSAL,
        "S1: the gears refuse while ▶ has a run, and say which button clears it"
    );
    window.close();
}

/// S2 + S3 through the real widget: pressing the gears with nothing running logs the full intended
/// order before any step has run, and walks every page in order.
fn starts_and_logs_the_order_when_idle(app: &adw::Application) {
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();

    let before = ui::window_logs().len();
    let gears = ui::lucky_button(&window).expect("the gears exist");
    gears.emit_clicked();

    let logged: Vec<String> = ui::window_logs()[before..].to_vec();
    assert!(
        !logged.is_empty(),
        "a press with nothing running must log something, got {logged:?}"
    );
    assert_eq!(
        logged[0],
        ">>> run: Prepare \u{2192} Cut \u{2192} Narrate \u{2192} Produce",
        "S3: the opening line lists every step in page order, first"
    );
    // Each step then announces itself as the chain reaches it.
    assert!(logged.contains(&">>> run: Prepare".to_string()), "{logged:?}");
    assert!(logged.contains(&">>> run: Cut".to_string()), "{logged:?}");
    assert!(logged.contains(&">>> run: Narrate".to_string()), "{logged:?}");
    assert!(logged.contains(&">>> run: Produce".to_string()), "{logged:?}");
    // And the press ends with a sentence mirrored on the status line, not silence.
    let last = logged.last().expect("an end line");
    assert!(
        last.contains("done") || last.contains("left undone"),
        "S6: the chain closes with an end line, got {last}"
    );
    assert!(
        !ui::state(&window).status.is_empty(),
        "the status line mirrors the end sentence rather than going blank"
    );
    // The gears are handed back once the chain is over.
    assert!(gears.is_sensitive(), "after the chain the gears are pressable again");
    window.close();
}

#[test]
fn f0_4_pressing_the_lucky_button_goes_through_the_chain() {
    window_round();
    assert!(
        RAN.load(std::sync::atomic::Ordering::SeqCst),
        "the gears checks never ran"
    );
}
