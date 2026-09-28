//! F2.7 — the wire: the three verb buttons and the ⌦ key reach the window, and each press prints exactly
//! the sentence `cut_verbs` computes for the same state. The logic tests (tests/cut_verbs.rs) prove the
//! rules; this one proves a click arrives.
//!
//! One application, one `connect_activate`, exactly one `#[test]`: these checks mutate process-shared
//! state (this window's selection, cut and held-in-hand slots), so they run once and report through
//! flags the test asserts on — the shape tests/cut_select_widgets.rs uses.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut::{self, Seg};
use naivepost::cut_select::{self, Scope, Surface};
use naivepost::cut_verbs::{self, Outcome};
use naivepost::shell::Page;
use naivepost::ui;

#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::{fixture_dir, settle, status_text};

static RAN_BUTTONS: AtomicBool = AtomicBool::new(false);
static RAN_SOUND_GREY: AtomicBool = AtomicBool::new(false);
static RAN_DELETE_ORDER: AtomicBool = AtomicBool::new(false);

/// A window sitting on the Cut page with a footage cut seeded, so every verb has ground to act on.
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
    assert_eq!(ui::state(&window).page, Page::Cut);

    // A cut with two kept scenes on row 0: enough that Add has something to add over and Remove has
    // something to drop. Seeded through the same seam ▶✂✂ reads.
    let mut seeded = cut::Cut::default();
    seeded.segs = vec![seg(0.0, 30.0, 0), seg(30.0, 60.0, 0)];
    ui::seed_review_cut(&window, &seeded);
    window
}

fn seg(s: f64, e: f64, cam: i32) -> Seg {
    Seg {
        s,
        e,
        cam,
        ..Default::default()
    }
}

/// Let the main context run what the widget emissions queued, the way the other widget tests do.

fn button(window: &adw::ApplicationWindow, name: &str) -> gtk::Button {
    ui::line_step_button(window, name).unwrap_or_else(|| panic!("the Cut page carries {name}"))
}

/// The three buttons exist, a real click on ＋ Add prints exactly what the rule returns, and ✕ still
/// clears the band underneath them.
fn check_the_verb_buttons_reach_the_rule(app: &adw::Application) {
    let window = cut_window(app);

    let add_ = button(&window, "add-button");
    let split_ = button(&window, "split-button");
    let remove_ = button(&window, "remove-button");
    println!(
        "BUTTONS add={:?} split={:?} remove={:?}",
        add_.label(),
        split_.label(),
        remove_.label()
    );
    assert_eq!(add_.label().as_deref(), Some("\u{ff0b} Add"));
    assert_eq!(split_.label().as_deref(), Some("| Split"));
    assert_eq!(remove_.label().as_deref(), Some("\u{ff0d} Remove"));

    // A fresh window holds no band, so nothing that needs one is live; | Split is the exception because
    // with no band it splits at the red line instead of doing nothing.
    assert!(!add_.is_sensitive(), "no selection yet, so ＋ Add is greyed");
    assert!(!remove_.is_sensitive(), "no selection yet, so － Remove is greyed");
    assert!(
        split_.is_sensitive(),
        "| Split works with no selection: one border at the red line"
    );

    // Seed a FOOTAGE band through the F2.6 seam, exactly as a drag would leave it.
    let band = ui::draw_selection(&window, Surface::PictureRow(0), None, 10.0, 40.0)
        .expect("a drag on a picture row selects that row's footage");
    let seeded_segs = vec![seg(0.0, 30.0, 0), seg(30.0, 60.0, 0)];

    assert!(
        button(&window, "add-button").is_sensitive(),
        "a footage band makes ＋ Add live"
    );
    assert!(
        button(&window, "remove-button").is_sensitive(),
        "a footage band makes － Remove live"
    );

    // The real click, then compare against the rule called directly on the same state.
    add_.emit_by_name::<()>("clicked", &[]);
    settle();
    let expected = cut_verbs::add(Some(&band), &seeded_segs, &[]);
    let printed = status_text(&window);
    println!("ADD PRINTED {printed}");
    assert_eq!(
        printed,
        expected.status(),
        "the click reached cut_verbs::add and printed its own sentence"
    );
    match &expected {
        Outcome::Applied { status, .. } => assert!(
            status.contains("added"),
            "the applied sentence says what happened: {status}"
        ),
        Outcome::Refused(reason) => panic!("this band should be addable, got {reason}"),
    }

    // The ✕ still does its own job with the verbs alongside it: the band goes, nothing else.
    let cross = ui::clear_selection_button(&window).expect("the cross is there");
    cross.emit_by_name::<()>("clicked", &[]);
    settle();
    assert!(
        ui::selection(&window).is_none(),
        "the real cross cleared the band the verbs read"
    );
    assert!(
        !button(&window, "add-button").is_sensitive(),
        "with the band gone ＋ Add is greyed again"
    );

    window.close();
}

/// A sound band greys exactly the three footage verbs and nothing else — the page draws the answer
/// `selection_verbs` gives, so the greying cannot drift from the rule.
fn check_a_sound_band_greys_the_three(app: &adw::Application) {
    let window = cut_window(app);
    let sound = ui::draw_selection(&window, Surface::Lane, Some("lecture"), 10.0, 40.0)
        .expect("a drag on a lane selects that recording's sound");
    assert_eq!(sound.scope, Scope::Sound { recording: "lecture".into() });

    let verbs = ui::selection_verbs(&window);
    assert!(
        !verbs.add && !verbs.split && !verbs.remove,
        "F2.6 S4: a sound selection kills all three footage verbs"
    );
    for name in ["add-button", "split-button", "remove-button"] {
        assert!(
            !button(&window, name).is_sensitive(),
            "{name} is greyed while the band is a recording's sound"
        );
    }
    // And the copy/insert half of the answer is untouched by that greying.
    assert!(
        verbs.copy,
        "⧉ Copy still lives on a sound band — only the footage verbs went"
    );

    window.close();
}

/// ⌦'s order through the seams: with an effect AND a clip AND a band all present, the effect is what the
/// status names, because the effect is what the hand last touched.
fn check_delete_takes_the_effect_first(app: &adw::Application) {
    let window = cut_window(app);
    ui::draw_selection(&window, Surface::PictureRow(0), None, 10.0, 40.0)
        .expect("a band is up");
    ui::set_held_clip(Some(seg(5.0, 9.0, 0)));
    ui::set_held_effect(Some(cut::Fx {
        kind: "zoom".into(),
        t: 12.0,
        ..Default::default()
    }));

    let outcome = ui::press_delete_key(&window);
    let printed = status_text(&window);
    println!("DELETE PRINTED {printed}");
    assert_eq!(
        printed,
        outcome.status(),
        "whatever ⌦ decided, the status line shows that decision and nothing else"
    );
    assert_eq!(
        printed,
        "removed the effect \u{2014} \u{21b6} Undo (Ctrl+Z) takes it back",
        "an effect outranks a held clip and a live band"
    );

    // With the effect put down, the held card answers next — the only door a spliced insert has.
    ui::set_held_effect(None);
    let second = ui::press_delete_key(&window);
    assert!(
        status_text(&window).starts_with("removed the card"),
        "next in line is the held card: {}",
        second.status()
    );

    // Nothing held at all: the Delete key controller is attached and claims the key.
    ui::set_held_clip(None);
    let controller = ui::delete_key_controller(&window).expect("⌦ has a key controller");
    assert!(
        controller
            .upcast_ref::<gtk::EventController>()
            .widget()
            .is_some(),
        "the ⌦ controller is attached to a widget, not floating unclaimed"
    );

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
            check_the_verb_buttons_reach_the_rule(app);
            RAN_BUTTONS.store(true, Ordering::SeqCst);
            check_a_sound_band_greys_the_three(app);
            RAN_SOUND_GREY.store(true, Ordering::SeqCst);
            check_delete_takes_the_effect_first(app);
            RAN_DELETE_ORDER.store(true, Ordering::SeqCst);
            app.quit();
        });
        app.run_with_args::<String>(&[]);
    });
}

#[test]
fn f2_7_s4_the_verb_buttons_and_delete_key_reach_the_window_through_real_widgets() {
    window_round();
    assert!(
        RAN_BUTTONS.load(Ordering::SeqCst),
        "the verb-button click check never ran"
    );
    assert!(
        RAN_SOUND_GREY.load(Ordering::SeqCst),
        "the sound-band greying check never ran"
    );
    assert!(
        RAN_DELETE_ORDER.load(Ordering::SeqCst),
        "the ⌦ order check never ran"
    );
}
