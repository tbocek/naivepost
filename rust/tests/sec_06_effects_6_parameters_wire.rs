//! §06-effects#6-parameters-used, the wire: the ceiling `fx_record::gain_ceiling()` sets is read by
//! the form a hand actually types into. The logic side lives in `sec_06_effects_6_parameters.rs`
//! (`…_s3_max_gain_bounds_the_filter`); this proves the same number reaches the widget — 🔊 Volume's
//! Percent field tops out at the ceiling ×100, and an ask past it lands back on the ceiling rather
//! than becoming a gain nothing downstream would honour.
//!
//! One application, one `connect_activate`, exactly one `#[test]`: the checks mutate process state
//! (cwd, the last window), so they run in order inside a single GTK round like `volume_form_widgets`.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;

use naivepost::cut::Cut;
use naivepost::cut_select::Surface;
use naivepost::fx_record;
use naivepost::fx_volume;
use naivepost::ui;

#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::{click, cut_page, entry, release_last_window, settle, status_text};

static RAN_CEILING: AtomicBool = AtomicBool::new(false);

/// Mark a stretch on the selection band, which is what the volume press reads when it decides where
/// to put its record — the same seam a drag leaves behind.
fn mark_band(window: &adw::ApplicationWindow, from: f64, to: f64) {
    let band = ui::draw_selection(window, Surface::SelectionBand, None, from, to);
    assert!(band.is_some(), "the band was drawn");
    settle();
}

fn type_percent(window: &adw::ApplicationWindow, text: &str) {
    entry(window, "volume-field-percent").set_text(text);
    settle();
}

fn percent_field(window: &adw::ApplicationWindow) -> String {
    entry(window, "volume-field-percent").text().to_string()
}

fn ceiling_round(app: &adw::Application) {
    // --- the ceiling the s3 logic test reads is the ceiling the field enforces ------------------
    {
        let window = cut_page(app, &Cut::default());
        mark_band(&window, 4.0, 9.0);
        click(&window, "effect-item-volume");
        assert_eq!(
            percent_field(&window),
            format!("{:.0}", fx_volume::percent_of(fx_volume::DEFAULT_GAIN)),
            "the field opens at the default gain, not at the ceiling"
        );
        // The tooltip says where it tops out, in the same number the module computes.
        let tip = common::widget(&window, "volume-field-percent")
            .tooltip_text()
            .map(|t| t.to_string())
            .unwrap_or_default();
        assert!(
            tip.contains(&format!("{}", fx_volume::max_percent() as i64)),
            "the field's own help states the ceiling: {tip}"
        );

        // An ask PAST the ceiling: Apply must land on the ceiling, never above it.
        type_percent(&window, "2500");
        click(&window, "volume-apply-button");
        settle();
        let said = status_text(&window);
        assert!(
            said.contains(&format!("{:.0}%", fx_volume::max_percent())),
            "Apply says what it stored, at the ceiling: {said}"
        );
        let heard = fx_volume::heard_while_paused(&ui::review_cut_of(&window).fx, 6.0);
        assert!(
            (heard - fx_record::gain_ceiling()).abs() < 1e-9,
            "the paused preview hears exactly the ceiling, the same number s3 reads: {heard}"
        );
    }
    // --- and a legal ask under it is stored untouched -------------------------------------------
    {
        let window = cut_page(app, &Cut::default());
        mark_band(&window, 4.0, 9.0);
        click(&window, "effect-item-volume");
        type_percent(&window, "300");
        click(&window, "volume-apply-button");
        settle();
        let heard = fx_volume::heard_while_paused(&ui::review_cut_of(&window).fx, 6.0);
        assert!(
            (heard - 3.0).abs() < 1e-9,
            "×3 is under the ceiling and stays ×3: {heard}"
        );
    }
    release_last_window();
    RAN_CEILING.store(true, Ordering::SeqCst);
}

#[test]
fn sec_06_effects_6_parameters_used_s6_the_volume_form_reads_the_ceiling() {
    let root = std::env::temp_dir().join(format!("np-p6wire-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temp root created");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");

    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(ceiling_round);
        app.run();
    });

    assert!(
        RAN_CEILING.load(Ordering::SeqCst),
        "the ceiling-through-the-form check never ran"
    );
}
