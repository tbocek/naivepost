// §05-cut#2 §2 — "base = the last suggestion or what the page opened with".
//
// Two halves, as every other Cut-page flow file splits them:
// * the LOGIC half drives `History::suggested` and `History::revert` directly and pins what each keeps;
// * the WIRE half presses the Cut page's real Revert button (`revert-button`, `emit_by_name("clicked")`)
//   after the suggestion was landed through the program's own door
//   (`ui::cut_suggest_land::land`, the seam `build_window` installs over the page's edit doors), and then
//   asserts the same state the logic half asserts: the segments on screen, the sentence Revert printed.
//
// One application, one `connect_activate`, one `#[test]`: these checks mutate process-shared state (the
// window's cut slot, its history, the status line), so they run once in sequence and report through flags.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use naivepost::cut::{self, Cut, History, Seg};
use naivepost::shell::Page;
use naivepost::suggest::Applied;
use naivepost::timeline::Recording;
use naivepost::ui;

use crate::common;
use common::{fixture_dir, hold_last_window, release_last_window, settle, status_text};

static RAN_LOGIC: AtomicBool = AtomicBool::new(false);
static RAN_WIRE: AtomicBool = AtomicBool::new(false);

/// Two kept scenes: what a suggestion proposes.
fn suggested_cut() -> Cut {
    Cut {
        segs: vec![
            Seg { s: 0.0, e: 30.0, ..Default::default() },
            Seg { s: 40.0, e: 60.0, ..Default::default() },
        ],
        ..Default::default()
    }
}

/// The walk-back's answer in the shape the program's door takes: the same two scenes, no cards, no effects.
fn answer() -> Applied {
    Applied {
        segs: vec![(0.0, 30.0), (40.0, 60.0)],
        kept_inserts: Vec::new(),
        effects: Vec::new(),
        log: Vec::new(),
    }
}

/// S1+S2 logic: a suggestion BECOMES the base, so Revert goes back to the suggestion and drops only what a
/// hand did afterwards — never the model's work, and never the cut from before the model spoke.
fn check_the_suggestion_is_the_base() {
    // The page opened on one long take: that was the base before anything happened.
    let opened = Cut {
        segs: vec![Seg { s: 0.0, e: 100.0, ..Default::default() }],
        ..Default::default()
    };
    let mut history = History::open(&opened);
    assert!(history.base_is_the_screen(&opened), "the page opens on its own cut");

    // A hand edit before the suggestion: the model's answer lands, and the base moves WITH it (§2).
    let landed = suggested_cut();
    history.note_change(&landed);
    history.suggested(&landed);
    assert!(
        history.base_is_the_screen(&landed),
        "after a suggestion the base is the suggestion, not what the page opened with"
    );
    assert!(
        !history.base_is_the_screen(&opened),
        "the cut from before the model spoke is no longer the base"
    );

    // A hand edit AFTER the suggestion: one clip trimmed by hand.
    let mut after_hand = suggested_cut();
    after_hand.segs[1].e = 55.0;
    history.note_change(&after_hand);
    assert!(!history.base_is_the_screen(&after_hand), "the hand edit is not the base");

    // Revert: the hand edit disappears, the suggestion stands.
    let back = history.revert();
    assert_eq!(back.segs, landed.segs, "Revert lands exactly the suggested scenes");
    assert_ne!(back.segs, opened.segs, "and not the pre-suggestion take");
    assert_ne!(back.segs, after_hand.segs, "and not the hand-trimmed tail");
    RAN_LOGIC.store(true, Ordering::SeqCst);
}

/// A tape long enough that the Cut tab unlocks and the page has something to cut.
fn tape() -> Vec<Recording> {
    vec![Recording { base: "session-tape".into(), start: 0.0, end: 120.0 }]
}

fn cut_page(app: &adw::Application, seeded: &Cut) -> adw::ApplicationWindow {
    release_last_window();
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    hold_last_window(window.clone());
    window.present();
    ui::tab_button(&window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_clicked();
    assert_eq!(ui::state(&window).page, Page::Cut);
    ui::set_session_recordings(&tape());
    ui::reopen_history_on(&window, seeded);
    ui::seed_review_cut(&window, seeded);
    settle();
    window
}

/// A suggestion's answer landed as a cut, with a hand trim of its second clip on top: the state a Revert
/// is asked about in the wire check.
fn hand_trimmed() -> Cut {
    let mut hand = suggested_cut();
    hand.segs[1].e = 55.0;
    hand
}

/// S2 wire: land a suggestion through the program's door, then press the page's Revert. The suggested
/// scenes must still be on screen afterwards — that is the whole of §2's sentence, checked through a click.
///
/// The landing goes through `ui::cut_suggest_land::land`, which is the seam `build_window` installs over
/// the page's own read/write doors: the same path the §8 suggest press walks, driven here with the answer
/// the walk-back rule produces, because a test cannot dial a model it is not pointed at.
fn check_revert_after_a_suggestion_keeps_the_suggestion(app: &adw::Application) {
    let opened = Cut {
        segs: vec![Seg { s: 0.0, e: 100.0, ..Default::default() }],
        ..Default::default()
    };
    let window = cut_page(app, &opened);

    ui::cut_suggest_land::land(&window, &answer());
    settle();
    assert_eq!(
        ui::review_cut_segs(&window).len(),
        2,
        "the landed suggestion is what the page shows"
    );
    // And the page knows it: Revert went live, because a step was recorded as well as a base moved.
    let revert = ui::line_step_button(&window, "revert-button").expect("the toolbar carries revert-button");
    assert!(revert.is_sensitive(), "Revert is live after a suggestion");

    // A hand edit on top of it, through the same door every hand edit uses (a trim of the second clip).
    let hand = hand_trimmed();
    ui::seed_review_cut(&window, &hand);

    revert.emit_by_name::<()>("clicked", &[]);
    settle();

    let on_screen = ui::review_cut_segs(&window);
    assert_eq!(
        on_screen,
        suggested_cut().segs,
        "Revert kept the suggested scenes and dropped only the hand edit"
    );
    // The sentence is the history's own (§A item 20), built by `press_revert` from the two counts, so the
    // check asks the rule for it rather than re-typing a string here.
    assert_eq!(
        status_text(&window),
        cut::reverted(suggested_cut().segs.len(), hand.segs.len()),
        "and says what went away with the line the rule owns"
    );
    RAN_WIRE.store(true, Ordering::SeqCst);
}

fn round() {
    common::one_app_round(|app| {
        check_the_suggestion_is_the_base();
        println!("CHECK the page's Revert after a landed suggestion");
        check_revert_after_a_suggestion_keeps_the_suggestion(app);
    });
}

#[test]
fn sec_05_cut_2_model_summary_full_schema_in_01_project_and_filesmd_3_s1_a_suggestion_becomes_what_revert_goes_back_to() {
    common::pin_private_cwd();
    round();
    assert!(RAN_LOGIC.load(Ordering::SeqCst), "the base-moves-with-a-suggestion check never ran");
}

#[test]
fn sec_05_cut_2_model_summary_full_schema_in_01_project_and_filesmd_3_s2_a_revert_after_a_suggestion_keeps_the_suggested_work_and_drops_only_hand_edits() {
    common::pin_private_cwd();
    round();
    assert!(RAN_WIRE.load(Ordering::SeqCst), "the Revert-button check never ran");
}
