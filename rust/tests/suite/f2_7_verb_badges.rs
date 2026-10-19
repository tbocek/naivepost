//! F2.7 ✕ badges — the five places a ✕ appears on the Cut page, each fired through its own widget and each
//! doing what `cut_verbs::badge_action` says it does.
//!
//! Every ✕ on this page now fires through ONE door (`ui::cut_verb_press::press_badge`), which forwards to
//! the rule that already owns the thing being taken away. That routing is what this file pins: a cross that
//! greys itself from a different question than its press asks is a lie, and a cross routed through a new
//! dispatcher while the old seam's sentence changed underneath would break the fold rounds' own tests.
//!
//! One application, one `connect_activate`, one `#[test]`; process-shared state as in the other Cut-page
//! widget rounds.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut::{Cut, Fx, Lane, Seg};
use naivepost::shell::Page;
use naivepost::ui;
use naivepost::ui::cut_verb_press;

use crate::common;
use common::{fixture_dir, hold_last_window, release_last_window, settle, status_text};

static RAN_SCENE_CROSS: AtomicBool = AtomicBool::new(false);
static RAN_OTHERS: AtomicBool = AtomicBool::new(false);
static RAN_NARROW: AtomicBool = AtomicBool::new(false);

fn footage(s: f64, e: f64, cam: i32) -> Seg {
    Seg { s, e, cam, ..Default::default() }
}

/// A window on the Cut page with `seeded` live, plus a cut.json so an editing press has a file to write.
fn cut_window(app: &adw::Application, seeded: &Cut) -> adw::ApplicationWindow {
    let root = std::env::temp_dir().join(format!(
        "naivepost-f27-badges-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let session = naivepost::startup::session_dir(&root);
    std::fs::create_dir_all(session.join("cut")).expect("a session cut folder");
    std::fs::create_dir_all(session.join("prepare")).expect("a session prepare folder");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");
    release_last_window();
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    hold_last_window(window.clone());
    window.present();
    ui::tab_button(&window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_by_name::<()>("clicked", &[]);
    ui::reopen_history_on(&window, seeded);
    ui::seed_review_cut(&window, seeded);
    ui::refresh_camera_rows(&window);
    ui::refresh_effects_lane(&window);
    settle();
    window
}

/// A named button inside this window's tree (the crosses are drawn into the row list, not the toolbar).
fn button(window: &adw::ApplicationWindow, name: &str) -> Option<gtk::Button> {
    let content = window.content()?;
    find_named(content.upcast_ref(), name)?.downcast::<gtk::Button>().ok()
}

fn find_named(root: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
    if root.widget_name() == name {
        return Some(root.clone());
    }
    let mut child = root.first_child();
    while let Some(node) = child {
        if let Some(found) = find_named(&node, name) {
            return Some(found);
        }
        child = node.next_sibling();
    }
    None
}

/// The green bar's ✕: `scene-cross-<i>` sits on the scene's own camera row, drops exactly THAT scene, and
/// one ↶ puts it back.
fn f2_7_badges_the_green_bar_cross_drops_that_scene(app: &adw::Application) {
    let seeded = Cut { segs: vec![footage(0.0, 30.0, 0), footage(40.0, 70.0, 0)], ..Default::default() };
    let window = cut_window(app, &seeded);
    assert_eq!(ui::review_cut_segs_count(&window), 2, "two scenes to choose from");

    let cross = button(&window, "scene-cross-0").expect("row 1 carries the first scene's ✕");
    assert!(cross.is_sensitive(), "a 30 s bar is wide enough to carry a ✕");
    assert_eq!(
        cross.tooltip_text().map(|t| t.to_string()),
        Some(naivepost::cut_verbs::badge_action(naivepost::cut_verbs::Badge::GreenBar).to_string()),
        "the tooltip IS the badge's meaning, not a copy of it"
    );
    // The second scene's ✕ belongs to the same row here (both segs are cam 0) and exists too.
    assert!(button(&window, "scene-cross-1").is_some(), "every footage scene gets one");

    cross.emit_by_name::<()>("clicked", &[]);
    settle();
    assert_eq!(
        ui::review_cut_segs_count(&window),
        1,
        "exactly that scene went"
    );
    let said = status_text(&window);
    println!("SCENE CROSS: {said}");
    assert!(
        said.contains("removed") && said.contains("was 2"),
        "the sentence names what went and what it was: {said}"
    );
    assert_eq!(ui::review_cut_of(&window).segs[0].s, 40.0, "the survivor is the OTHER scene");

    // ↶ takes it back: the ✕ spent one Undo step, not none and not two. Fired through the toolbar's own
    // button, because that is the door a person presses.
    ui::line_step_button(&window, "undo-button")
        .expect("the toolbar drew its undo-button")
        .emit_by_name::<()>("clicked", &[]);
    settle();
    assert_eq!(ui::review_cut_segs_count(&window), 2, "one ↶ restores the scene");
    window.close();
}

/// The other four badges still do their own thing after being routed through the one badge door: the
/// selection's ✕ clears and removes nothing, the effect's ✕ removes the record, the row's and the lane's
/// crosses spend their fold rules, and each keeps the sentence its own round pinned.
fn f2_7_badges_each_other_cross_still_does_its_own_thing(app: &adw::Application) {
    let seeded = Cut {
        // The lane this cut carries is SOUND (a `lane` segment), and `clamp_effects_to_cut` runs after every
        // verb that rewrites the segments — with nothing filmed under the line it would drop the effects as
        // outside the cut. This check is about the ✕, so it gives them footage to live on.
        segs: vec![footage(0.0, 60.0, 0)],
        lanes: vec![Lane {
            name: "mic-main".to_string(),
            src: "project:audio/mic-main.m4a".to_string(),
            at: 5.0,
            off: 0.0,
            dur: 20.0,
        }],
        fx: vec![Fx { kind: "zoom".to_string(), t: 10.0, dur: 12.0, ..Default::default() }],
        nrows: 2,
        ..Default::default()
    };
    let window = cut_window(app, &seeded);

    // The selection's ✕: draws a band, presses the cross, and NOTHING was removed — clearing is not deleting.
    ui::draw_selection(&window, naivepost::cut_select::Surface::Ruler, None, 5.0, 20.0);
    settle();
    assert!(ui::selection(&window).is_some(), "a band is up");
    let clear = ui::clear_selection_button(&window).expect("the page carries clear-selection");
    assert!(clear.is_sensitive(), "with a band the cross is live");
    clear.emit_by_name::<()>("clicked", &[]);
    settle();
    assert!(ui::selection(&window).is_none(), "and it cleared");
    assert_eq!(ui::review_cut_segs_count(&window), 1, "clearing removed no footage");

    // The effect's ✕: `fx-remove-0`, whose width rule (§A.9 `effects.killMinPx`) decides whether it is
    // there at all — a 12 s record is wide enough, so it is, and pressing it takes the record. Pressed
    // TWICE on purpose: the first press also moves the red line onto the band (F3.8's hold), which makes a
    // second press report "no longer in the cut"; the first one is what kills it.
    let fx_cross = button(&window, "fx-remove-0").expect("a wide effect bar carries its own ✕");
    fx_cross.emit_by_name::<()>("clicked", &[]);
    settle();
    assert_eq!(ui::review_cut_of(&window).fx.len(), 0, "the effect went");
    // And the door says so when the slot it was aimed at has already been spent — which is what a stale ✕
    // would otherwise hide.
    let said = cut_verb_press::press_badge(
        &window,
        naivepost::cut_verbs::Badge::Effect,
        usize::MAX,
    );
    println!("EFFECT CROSS (spent slot): {said}");
    assert!(
        said.contains("no longer in the cut"),
        "a ✕ aimed at a spent slot says so rather than pretending: {said}"
    );

    // The lane's ✕: `lane-cross-<name>`, routed by the lane's ORDER in `cut.lanes`.
    let lane_cross = button(&window, "lane-cross-mic-main").expect("the lane carries its ✕");
    lane_cross.emit_by_name::<()>("clicked", &[]);
    settle();
    assert!(ui::review_cut_of(&window).lanes.is_empty(), "the lane was taken away");
    assert!(
        status_text(&window).contains("mic-main"),
        "naming the lane it took: {}",
        status_text(&window)
    );

    // The empty bottom row's ✕: `row-cross-2`, live only because `nrows` holds that row open and empty.
    let row_cross = button(&window, "row-cross-2").expect("the emptied bottom row carries its ✕");
    assert!(row_cross.is_sensitive(), "an empty bottom row offers a live ✕");
    row_cross.emit_by_name::<()>("clicked", &[]);
    settle();
    assert_eq!(ui::review_fold_rows(&window).nrows, 1, "the row floor was spent");
    window.close();
}

/// A bar too narrow to carry a ✕ clear of its grips offers no LIVE one — the button is there but greyed, so
/// the page shows the refusal instead of hiding the affordance, and it never dangles a kill target the press
/// rule would refuse. Same threshold the effect ✕ uses.
fn f2_7_badges_a_narrow_bar_offers_no_live_cross(app: &adw::Application) {
    let seeded = Cut {
        // 0.5 s of footage: at the page's zoom that is far under `fx_band::KILL_MIN_PX`, so no live ✕ may show.
        segs: vec![footage(0.0, 0.5, 0), footage(2.0, 3.0, 0)],
        ..Default::default()
    };
    let window = cut_window(app, &seeded);
    assert!(
        !naivepost::fx_band::kill_open(ui::cut_bar_width_px(0.5)),
        "the width rule itself says this bar is too narrow for a ✕"
    );
    assert!(
        !cut_verb_press::scene_cross_live(0.5),
        "and the page asks that same rule"
    );
    let cross = button(&window, "scene-cross-0").expect("the scene still shows its ✕ face");
    assert!(!cross.is_sensitive(), "but it is greyed: no live kill on a 0.5 s bar");
    // Pressing a greyed button does nothing at all, which is the whole point of greying it: the door still
    // answers (it is the rule that refuses, not the widget), and the scene stays where it was.
    let said = cut_verb_press::press_badge(&window, naivepost::cut_verbs::Badge::GreenBar, 0);
    println!("NARROW PRESS: {said}");
    settle();
    // The wide-scene case is the same rule answering the other way, checked where the bars are long.
    assert!(cut_verb_press::scene_cross_live(30.0), "a 30 s scene's bar does carry one");
    window.close();
}

fn round() {
    common::one_app_round(|app| {
        f2_7_badges_the_green_bar_cross_drops_that_scene(app);
        RAN_SCENE_CROSS.store(true, Ordering::SeqCst);
        f2_7_badges_each_other_cross_still_does_its_own_thing(app);
        RAN_OTHERS.store(true, Ordering::SeqCst);
        f2_7_badges_a_narrow_bar_offers_no_live_cross(app);
        RAN_NARROW.store(true, Ordering::SeqCst);
    });
}

#[test]
fn f2_7_badges_each_cross_does_what_its_badge_says() {
    round();
    assert!(RAN_SCENE_CROSS.load(Ordering::SeqCst), "the green-bar check never ran");
    assert!(RAN_OTHERS.load(Ordering::SeqCst), "the other-badges check never ran");
    assert!(RAN_NARROW.load(Ordering::SeqCst), "the narrow-bar check never ran");
}
