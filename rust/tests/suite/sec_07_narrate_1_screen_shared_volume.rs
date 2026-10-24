//! §07-narrate#1-screen (**6**) over the WIRE: the Narrate page's volume slider is a second face of the
//! app's ONE preview volume (F2.5 S6, `spec/inventory/narrate.md` §A "shared with every preview").
//!
//! Every check fires the real widget (`set_value`, which emits `value-changed` exactly as a drag does)
//! and reads back `ui::preview_volume` — the same slot the Cut page's slider writes and every preview
//! multiplies by. The first check fails without this item's fix: a Narrate window had no volume slot at
//! all, so its drag was written into a throwaway and lost.
//!
//! One application, one `connect_activate`, exactly one `#[test]`: these checks mutate process-shared
//! state (this thread's preview-volume slot), so they run once as stages and report through flags — the
//! shape tests/suite/sec_07_narrate_1_screen_widgets.rs uses.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;

use naivepost::cut_hear;
use naivepost::ui;

use crate::common;
use common::{fixture_dir, hold_last_window, release_last_window, settle};

/// `ui::find_widget_by_name` is crate-private, so the test walks the tree itself — the shape
/// tests/suite/sec_06_effects_2_text_staging.rs uses.
fn find(root: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
    if root.widget_name() == name {
        return Some(root.clone());
    }
    for child in root.observe_children().iter::<glib::Object>() {
        let Ok(child) = child else { continue };
        let Ok(widget) = child.downcast::<gtk::Widget>() else {
            continue;
        };
        if let Some(found) = find(&widget, name) {
            return Some(found);
        }
    }
    None
}

static RAN_SHARED: AtomicBool = AtomicBool::new(false);
static RAN_BOTH_PAGES: AtomicBool = AtomicBool::new(false);
static RAN_SCALE: AtomicBool = AtomicBool::new(false);

/// A session folder of our own, so nothing these checks write lands in a real project.
fn scratch_root(tag: &str) -> std::path::PathBuf {
    let root = naivepost::startup::session_dir(&std::env::temp_dir().join(format!(
        "np-sec07-volume-{}-{}",
        tag,
        std::process::id()
    )));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("scratch session folder");
    std::env::set_current_dir(&root).expect("cwd pinned to this root");
    root
}

fn base_state() -> ui::NarrateState {
    ui::NarrateState {
        segs: vec![
            naivepost::cut::Seg {
                s: 0.0,
                e: 30.0,
                ..Default::default()
            },
            naivepost::cut::Seg {
                s: 30.0,
                e: 60.0,
                ..Default::default()
            },
        ],
        voice: "narrator1".into(),
        session: 5.0,
        cut_at: 5.0,
        length: 60.0,
        clips: 2,
        has_cut: true,
        has_timeline: true,
        narrators: 1,
        voice_files: vec![],
        recording_covers_clip: true,
        ..Default::default()
    }
}

/// A fresh window on the Narrate page — built the way the shell builds it, because the volume slider is
/// wired by `narrate_page::wire`, which only runs on a real window.
fn narrate_page(app: &adw::Application, state: ui::NarrateState) -> adw::ApplicationWindow {
    release_last_window();
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    hold_last_window(window.clone());
    window.set_default_size(1280, 800);
    window.present();
    ui::tab_button(&window, naivepost::shell::Page::Narrate)
        .expect("the shell has a Narrate tab")
        .emit_by_name::<()>("clicked", &[]);
    settle();
    ui::set_state(state);
    ui::refresh(&window);
    settle();
    window
}

/// This page's volume slider, by its stable name — never a value written straight to the seam.
fn narrate_scale(window: &adw::ApplicationWindow) -> gtk::Scale {
    find(window.upcast_ref(), "narrate-volume")
        .expect("§1 item 6 narrate-volume")
        .downcast()
        .expect("the volume control is a scale")
}

/// **6** + F2.5 S6: a drag of the Narrate slider moves the one number every preview reads.
fn check_the_slider_writes_the_shared_number(app: &adw::Application) {
    let _ = scratch_root("shared");
    let window = narrate_page(app, base_state());
    let scale = narrate_scale(&window);
    assert!(scale.is_mapped(), "the slider is drawn on the visible page");
    // Untouched means audible, and it is the SHARED default rather than a page-private number.
    assert_eq!(
        ui::preview_volume(&window),
        cut_hear::VOLUME_DEFAULT,
        "an untouched Narrate page reads the shared default"
    );
    // The drag proper: setting the widget's value fires `value-changed` for real.
    scale.set_value(40.0);
    settle();
    assert!(
        (ui::preview_volume(&window) - 0.4).abs() < 1e-9,
        "40 on this slider lands as the shared gain 0.4 — not 40, not 0.004, got {}",
        ui::preview_volume(&window)
    );
    // And the page's own preview leg asks the same holder, so an audition cannot sound louder than the
    // preview it belongs to (§C.4: sound equals the render's).
    scale.set_value(75.0);
    settle();
    assert!(
        (ui::preview_volume(&window) - 0.75).abs() < 1e-9,
        "a second move follows: the handler is not a one-shot"
    );
    RAN_SHARED.store(true, Ordering::SeqCst);
}

/// F2.5 S6 (`one preview volume, shared by every preview`): the two faces of the setting hold ONE
/// number, in both directions. The shell builds all five pages into one window, so this window carries
/// both sliders over one slot, and a drag on either face is the number the other shows.
fn check_both_pages_show_the_one_volume(app: &adw::Application) {
    let _ = scratch_root("both");
    let narrate = narrate_page(app, base_state());
    // The Cut page's own door for its own slider — the same widget F2.5's test drags.
    let cut_scale = ui::preview_volume_scale(&narrate).expect("the shell carries preview-volume");
    assert!(
        (cut_scale.value() - 100.0).abs() < 1e-9,
        "untouched, both faces sit at the shared default, got {}",
        cut_scale.value()
    );
    // Direction 1: a drag here lands on the one number every preview reads, and on the Cut page's face
    // of it at this window's next repaint.
    narrate_scale(&narrate).set_value(40.0);
    settle();
    assert!(
        (ui::preview_volume(&narrate) - 0.4).abs() < 1e-9,
        "40 on this page's slider is the shared gain 0.4, got {}",
        ui::preview_volume(&narrate)
    );
    ui::refresh(&narrate);
    settle();
    assert!(
        (cut_scale.value() - 40.0).abs() < 1e-9,
        "the Cut page's slider shows the number the Narrate slider set, got {}",
        cut_scale.value()
    );
    // A fresh Cut page then OPENS at that number: reading a newly built window is what makes "shared" a
    // claim about the holder rather than about two widgets painted next to each other.
    // Direction 2: the other way round — a Cut-page drag of 25 is the shared number, and this page's own
    // widget shows it at its next repaint (§A's "shared with every preview": one setting, wherever it is
    // shown).
    cut_scale.set_value(25.0);
    settle();
    assert!(
        (ui::preview_volume(&narrate) - 0.25).abs() < 1e-9,
        "the number the Cut page moved is the number every preview reads, got {}",
        ui::preview_volume(&narrate)
    );
    ui::refresh(&narrate);
    settle();
    assert!(
        (narrate_scale(&narrate).value() - 25.0).abs() < 1e-9,
        "a repaint mirrors the shared number onto this page's slider, got {}",
        narrate_scale(&narrate).value()
    );
    RAN_BOTH_PAGES.store(true, Ordering::SeqCst);
}

/// §A's row for **6**: "shared preview volume 0–100" with the shared tooltip, so the two faces of the
/// one setting cannot be spelled differently.
fn check_the_scale_is_the_shared_scale(app: &adw::Application) {
    let _ = scratch_root("scale");
    let window = narrate_page(app, base_state());
    let scale = narrate_scale(&window);
    assert_eq!(
        (scale.adjustment().lower(), scale.adjustment().upper()),
        (0.0, 100.0),
        "0..100, the scale the shared property is shown on"
    );
    assert!(
        (scale.adjustment().step_increment() - 1.0).abs() < 1e-9,
        "step 1 — what a slider reads as, while the property under it is 0..1"
    );
    assert_eq!(
        scale.tooltip_text().map(|t| t.to_string()),
        Some(cut_hear::VOLUME_TIP.to_string()),
        "the tooltip is the shared sentence, verbatim, so it cannot drift from the module's"
    );
    RAN_SCALE.store(true, Ordering::SeqCst);
}

fn round(app: &adw::Application) {
    check_the_slider_writes_the_shared_number(app);
    check_both_pages_show_the_one_volume(app);
    check_the_scale_is_the_shared_scale(app);
    release_last_window();
}

#[test]
fn sec_07_narrate_1_s6_the_narrate_volume_slider_is_the_shared_one() {
    let _ = gtk::init();
    let app = adw::Application::builder()
        .application_id(ui::APP_ID)
        .flags(gtk4::gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.connect_activate(round);
    app.run_with_args::<&str>(&[]);
    assert!(
        RAN_SHARED.load(Ordering::SeqCst),
        "the shared-number check ran"
    );
    assert!(
        RAN_BOTH_PAGES.load(Ordering::SeqCst),
        "the both-pages check ran"
    );
    assert!(
        RAN_SCALE.load(Ordering::SeqCst),
        "the scale-shape check ran"
    );
}
