//! §06-effects#9-where-recordings-overlap through the WIRE: a real Cut page, the speed form's
//! own widgets, and the two sound/stop promises §9 makes the press say out loud.
//!
//! The logic tests in `sec_06_effects_9_where_recordings_overlap.rs` prove the rules; this one
//! proves the click reaches them — the recorded Fx carries the chosen sound answer AND the
//! status line holds the sentence the module spells, and a stop over a dropped second is
//! refused by the form rather than written to the lane.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;

use naivepost::cut::Cut;
use naivepost::cut_select::Surface;
use naivepost::cut_speed::{self as speed, SOUND_CHOICES};
use naivepost::effect_overlap;
use naivepost::ui;

#[allow(dead_code)] // every test binary compiles this whole module
mod common;
use common::{click, settle};

/// Choose a dropdown row the way the toolkit delivers a popover pick: `set_selected` fires the
/// signal a click inside the popover raises. Nothing here calls a rule directly.
fn choose(window: &adw::ApplicationWindow, name: &str, index: usize) {
    let drop = common::widget(window, name)
        .downcast::<gtk::DropDown>()
        .unwrap_or_else(|_| panic!("`{name}` is a DropDown"));
    drop.set_selected(index as u32);
    settle();
}

static RAN_OWN_NOTICE: AtomicBool = AtomicBool::new(false);
static RAN_REFUSED_STOP: AtomicBool = AtomicBool::new(false);

/// Mark a stretch on the selection ground, the seam a drag would use.
fn mark_band(window: &adw::ApplicationWindow, from: f64, to: f64) {
    ui::draw_selection(window, Surface::SelectionBand, None, from, to);
    settle();
}

fn round(app: &adw::Application) {
    // --- the `own` row: the record stores it AND the notice reaches the status line ---
    {
        let window = common::cut_page(app, &Cut::default());
        mark_band(&window, 10.0, 25.0);
        click(&window, "effect-item-speed");
        // The dropdown row that stores the own-clock answer, chosen the toolkit's way.
        let own_row = SOUND_CHOICES
            .iter()
            .position(|choice| speed::sound_stored(choice) == "own")
            .expect("the form offers the own-clock answer");
        choose(&window, "speed-field-sound", own_row);
        click(&window, "speed-apply-button");
        let cut_ = ui::review_cut_of(&window);
        assert_eq!(cut_.fx.len(), 1, "Apply placed exactly one record");
        assert_eq!(cut_.fx[0].snd, "own", "the sound answer the dropdown was left on");
        // The notice is its OWN line in the log: the placement sentence stays what §F3.3 fixes
        // and §9's promise is said beside it, not instead of it.
        let said = ui::window::window_logs();
        assert_eq!(
            said.last().map(String::as_str),
            Some(effect_overlap::says_render_only("own").expect("§9 spells the sentence")),
            "the preview says what it cannot do instead of doing it quietly: {said:?}"
        );
        RAN_OWN_NOTICE.store(true, Ordering::SeqCst);
        window.close();
    }

    // --- a stop over a second the cut dropped: refused at placement, nothing recorded ---
    {
        // One kept scene from 0 to 10 s: everything past it is a hole the cut does not keep.
        let seeded = Cut {
            segs: vec![naivepost::cut::Seg { s: 0.0, e: 10.0, ..Default::default() }],
            ..Default::default()
        };
        let window = common::cut_page(app, &seeded);
        mark_band(&window, 20.0, 24.0);
        click(&window, "effect-item-speed");
        choose(&window, "speed-field-rate", speed::rate_index(0.0));
        click(&window, "speed-apply-button");
        assert_eq!(
            ui::window::window_logs()
                .last()
                .map(String::as_str)
                .unwrap_or_default(),
            effect_overlap::no_frame(20.0),
            "the refusal names the moment and why"
        );
        assert!(
            ui::review_cut_of(&window).fx.is_empty(),
            "a refused stop wrote nothing to the lane"
        );
        RAN_REFUSED_STOP.store(true, Ordering::SeqCst);
        window.close();
    }
}

#[test]
fn sec_06_effects_9_where_recordings_overlap_s5_widgets_say_the_sound_and_stop_promises() {
    // GTK has to be initialised before any widget can exist; the checks run inside the app round.
    let _ = gtk::init();
    let root = naivepost::startup::session_dir(&std::env::temp_dir().join("np-sec06-9-widgets"));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("root");
    std::env::set_current_dir(&root).expect("cwd pinned to this root");
    let app = adw::Application::builder()
        .application_id(ui::APP_ID)
        .flags(gtk4::gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.connect_activate(round);
    app.run();
    assert!(RAN_OWN_NOTICE.load(Ordering::SeqCst), "the own-clock notice never ran");
    assert!(RAN_REFUSED_STOP.load(Ordering::SeqCst), "the refused stop never ran");
}
