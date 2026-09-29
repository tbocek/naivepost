// F2.9 — the wire: ⧉ Copy / ⧉ Paste at the red line / ⇲ Lane are real buttons on the Cut page, and each
// press reaches `cut_copy`'s rule rather than a copy of it. The logic tests (tests/cut_copy_paste_lane.rs)
// prove the rules; this proves a click arrives, and that the status line prints what the rule returned.
//
// One application, one `connect_activate`, exactly one `#[test]`: these checks mutate process-shared
// state (this window's cut slot, its lanes, the copy in hand and the status line), so they run once in
// sequence and report through flags the test asserts on — the shape tests/cut_trim_move_widgets.rs uses.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut::{self, Seg};
use naivepost::cut_copy as cp;
use naivepost::cut_select::{Surface, MIN_SCENE_SECONDS};
use naivepost::shell::Page;
use naivepost::ui;

#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::{fixture_dir, settle, status_text, clip};

static RAN_SHORT: AtomicBool = AtomicBool::new(false);
static RAN_TAKEN: AtomicBool = AtomicBool::new(false);
static RAN_PASTED: AtomicBool = AtomicBool::new(false);
static RAN_LANE: AtomicBool = AtomicBool::new(false);
static RAN_ESC: AtomicBool = AtomicBool::new(false);
static RAN_PASTE_LENGTHENS: AtomicBool = AtomicBool::new(false);

/// A footage scene on camera 0.
/// The Cut page with a three-clip cut seeded, so there is kept footage to copy from and to paste over.
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

    let mut seeded = cut::Cut::default();
    seeded.segs = vec![clip(0.0, 9.5), clip(10.0, 30.0), clip(40.0, 70.0)];
    ui::seed_review_cut(&window, &seeded);
    // No copy in hand at the start of every check: the thread-local outlives a window, so a leftover hand
    // would make the next check's greying wrong rather than today's answer.
    drop_copy();
    window
}

/// Let the main context run what the widget emissions queued.

/// This window's cut as a whole, so the length can be measured the same way the page measures it.
fn cut_of(window: &adw::ApplicationWindow) -> cut::Cut {
    let mut fresh = cut::Cut::default();
    fresh.segs = ui::review_cut_segs(window);
    fresh.lanes = ui::review_lanes(window);
    fresh
}

/// Put a band on the page the way a drag does — through `draw_selection`, so the check seeds the same
/// state the surface would have written rather than poking a thread-local.
fn seed_band(window: &adw::ApplicationWindow, from: f64, to: f64) {
    let band = ui::draw_selection(window, Surface::PictureRow(0), None, from, to);
    assert!(band.is_some(), "a picture-row drag must produce a footage band");
}

/// Empty the hand between checks without going through Esc (Esc has its own check below).
fn drop_copy() {
    // Esc is the app's own drop path; using it here keeps the reset honest rather than writing the
    // thread-local directly.
    let esc = COPY_ESC.with(|cell| cell.borrow().clone());
    if let Some(controller) = esc {
        press_key(&controller, gtk::gdk::Key::Escape);
        settle();
    }
    assert_eq!(ui::copy_hand(), None, "the reset left a copy in hand");
}

thread_local! {
    /// The Esc controller, fetched once per round: `copy_esc_controller` reads a thread-local, so this is
    /// the same handle the page attached.
    static COPY_ESC: std::cell::RefCell<Option<gtk::EventControllerKey>> =
        const { std::cell::RefCell::new(None) };
}

/// Send Escape through the controller the way the window would. The handler answers `Proceed` when no
/// copy is held and `Stop` when it dropped one; both are read by the caller where it matters.
fn press_key(controller: &gtk::EventControllerKey, key: gtk::gdk::Key) {
    // The handler answers with `glib::Propagation`, which has no FromValue for a returned Value, so the
    // signal is emitted for its side effect and the state below is what the check reads.
    let _ = controller.emit_by_name::<bool>(
        "key-pressed", &[&key, &0u32, &gtk::gdk::ModifierType::empty()],
    );
}

fn remember_esc(window: &adw::ApplicationWindow) {
    let controller = ui::copy_esc_controller(window).expect("Esc has a key controller");
    assert!(
        controller
            .upcast_ref::<gtk::EventController>()
            .widget()
            .is_some(),
        "the Esc controller is attached to a widget, not floating unclaimed"
    );
    COPY_ESC.with(|cell| *cell.borrow_mut() = Some(controller));
}

/// S1 refusal: nothing selected leaves ⧉ Copy greyed, and a half-second band refuses when clicked anyway.
fn check_a_short_or_absent_selection_never_reaches_the_hand(app: &adw::Application) {
    let window = cut_window(app);
    remember_esc(&window);

    let copy_ = ui::line_step_button(&window, "copy-button").expect("the Cut page carries copy-button");
    assert!(
        !copy_.is_sensitive(),
        "with no selection ⧉ Copy must be greyed, not live-and-empty"
    );

    // A 0.5 s band: too short by `MIN_SCENE_SECONDS`. Clicking a greyed button still routes nowhere, so
    // the band is drawn and the seam is called directly to prove the refusal itself.
    seed_band(&window, 12.0, 12.5);
    settle();
    assert!(
        ui::selection(&window).is_some(),
        "the short band was drawn before the refusal is asked for"
    );
    let said = ui::press_copy(&window);
    assert!(
        said.contains("under 1 s"),
        "the refusal says why: {said}"
    );
    assert_eq!(ui::copy_hand(), None, "a refused copy leaves nothing in hand");
    assert_eq!(
        said,
        format!(
            "the selection is {:.2} s \u{2014} under 1 s there is nothing worth copying",
            0.5
        ),
        "// P.policy.minSceneSeconds is the floor the sentence quotes"
    );
    assert!(ui::selection(&window).is_some(), "refusing copies did not clear the band either");
    let _ = MIN_SCENE_SECONDS;
    window.close();
}

/// S1 taken: a second or longer goes into hand, the status is the rule's own sentence, and the band lives.
fn check_a_long_selection_is_taken_in_hand_and_survives(app: &adw::Application) {
    let window = cut_window(app);
    remember_esc(&window);

    seed_band(&window, 12.0, 20.0);
    settle();
    let copy_ = ui::line_step_button(&window, "copy-button").expect("copy-button exists");
    assert!(copy_.is_sensitive(), "an 8 s band makes ⧉ Copy live");

    copy_.emit_clicked();
    settle();

    let hand = ui::copy_hand().expect("⧉ Copy put the seconds in hand");
    assert_eq!(hand.from, 12.0, "the hand starts where the band started");
    assert_eq!(hand.length, 8.0, "and holds the whole span");
    assert_eq!(
        status_text(&window),
        cp::copied_status(&hand),
        "the status line IS the rule's sentence, spelled by `copied_status`"
    );
    assert!(
        status_text(&window).contains("\u{29c9} Paste"),
        "the sentence points at the next button: {}",
        status_text(&window)
    );
    assert!(
        ui::selection(&window).is_some(),
        "taking a copy is reading, not editing: the band survives"
    );
    window.close();
}

/// S2 footage paste: spliced in at the line as a `copy:<seconds>` card, the cut gets longer, the hand is
/// consumed, and the printed sentence names both totals.
fn check_a_footage_paste_splices_at_the_line_and_consumes_the_copy(app: &adw::Application) {
    let window = cut_window(app);
    remember_esc(&window);

    seed_band(&window, 12.0, 20.0);
    settle();
    ui::line_step_button(&window, "copy-button")
        .expect("copy-button exists")
        .emit_clicked();
    settle();
    let hand = ui::copy_hand().expect("the copy is in hand");

    let before_segs = ui::review_cut_segs_count(&window);
    let before_len = naivepost::cut_screen::cut_seconds(&cut_of(&window));
    assert_eq!(before_segs, 3, "three clips were seeded");

    // The line inside kept footage, well clear of a border.
    ui::set_line_position(&window, naivepost::cut_line::LinePos { t: 35.0 });
    let paste_ = ui::line_step_button(&window, "paste-button")
        .expect("the Cut page carries paste-button");
    assert!(paste_.is_sensitive(), "a hand held makes ⧉ Paste live");

    paste_.emit_clicked();
    settle();

    let after = ui::review_cut_segs(&window);
    assert_eq!(after.len(), before_segs + 1, "the spliced insert joined the list");
    let spliced = after
        .iter()
        .find(|seg| seg.ins.contains("copy:"))
        .expect("the pasted segment is a `copy:<seconds>` insert");
    assert_eq!(spliced.s, 35.0, "it opens the cut at the line");
    assert_eq!(spliced.e, 35.0, "a spliced insert sits at one instant");
    assert_eq!(spliced.dur, 8.0, "and runs for the copied length");
    assert!(
        naivepost::cut_screen::cut_seconds(&cut_of(&window)) > before_len,
        "the video got longer: {before_len} -> {}",
        naivepost::cut_screen::cut_seconds(&cut_of(&window))
    );
    assert_eq!(ui::copy_hand(), None, "pasting consumes the copy: S2");
    let said = status_text(&window);
    assert!(said.starts_with("pasted "), "the sentence opens with the act: {said}");
    assert!(said.contains("was "), "and names what the cut used to be: {said}");
    assert!(
        !paste_.is_sensitive(),
        "with the copy gone ⧉ Paste greys again rather than lying about having something"
    );
    window.close();
}

/// S3 lane: the copy gets a row of its own and NOTHING is cut — no segment appears.
fn check_a_lane_gives_the_copy_its_own_row_without_cutting(app: &adw::Application) {
    let window = cut_window(app);
    remember_esc(&window);

    seed_band(&window, 12.0, 20.0);
    settle();
    ui::line_step_button(&window, "copy-button")
        .expect("copy-button exists")
        .emit_clicked();
    settle();
    let hand = ui::copy_hand().expect("the copy is in hand");

    let before_lanes = ui::review_lanes(&window).len();
    let before_segs = ui::review_cut_segs_count(&window);
    // Inside a filmed run, so `lane_start` accepts the second.
    ui::set_line_position(&window, naivepost::cut_line::LinePos { t: 15.0 });
    let lane_ = ui::line_step_button(&window, "lane-button").expect("the Cut page carries lane-button");
    assert!(lane_.is_sensitive(), "a hand held makes ⇲ Lane live");

    lane_.emit_clicked();
    settle();

    let lanes = ui::review_lanes(&window);
    assert_eq!(lanes.len(), before_lanes + 1, "one row arrived");
    let fresh = lanes.last().expect("the new lane");
    assert_eq!(fresh.at, 15.0, "the row starts at the line");
    assert_eq!(fresh.dur, hand.length, "and runs for the copied length");
    assert_eq!(
        ui::review_cut_segs_count(&window),
        before_segs,
        "nothing was cut to the new row — ＋ Add does that later"
    );
    assert_eq!(
        status_text(&window),
        cp::lane_status(hand.length, hand.from, &fresh.name, 15.0),
        "the status line IS the lane's own sentence"
    );
    // A second copy of the same thing cannot share the name: `Lane::name` keys `Cut::rows`.
    ui::line_step_button(&window, "copy-button")
        .expect("copy-button exists")
        .emit_clicked();
    settle();
    ui::set_line_position(&window, naivepost::cut_line::LinePos { t: 16.0 });
    lane_.emit_clicked();
    settle();
    let names: Vec<String> = ui::review_lanes(&window).iter().map(|l| l.name.clone()).collect();
    assert_eq!(names.len(), 2, "a second row arrived");
    assert_ne!(names[0], names[1], "and it does not reuse the first row's name: {names:?}");
    window.close();
}

/// S2's other half: Esc drops the copy, and only that key.
fn check_esc_drops_the_copy(app: &adw::Application) {
    let window = cut_window(app);
    remember_esc(&window);

    seed_band(&window, 12.0, 20.0);
    settle();
    ui::line_step_button(&window, "copy-button")
        .expect("copy-button exists")
        .emit_clicked();
    settle();
    assert!(ui::copy_hand().is_some(), "a copy is in hand before the key");

    let controller = COPY_ESC
        .with(|cell| cell.borrow().clone())
        .expect("the Esc controller was fetched");
    press_key(&controller, gtk::gdk::Key::Escape);
    settle();
    assert_eq!(ui::copy_hand(), None, "Esc dropped the copy");
    assert_eq!(status_text(&window), cp::DROPPED, "and said so plainly");
    assert!(
        !ui::line_step_button(&window, "paste-button")
            .expect("paste-button exists")
            .is_sensitive(),
        "⧉ Paste greyed back out with nothing in hand"
    );

    // Any other key passes through untouched: typing in an entry must keep working.
    seed_band(&window, 30.0, 40.0);
    settle();
    ui::line_step_button(&window, "copy-button")
        .expect("copy-button exists")
        .emit_clicked();
    settle();
    press_key(&controller, gtk::gdk::Key::a);
    settle();
    assert!(ui::copy_hand().is_some(), "and the copy survived it");
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
            check_a_short_or_absent_selection_never_reaches_the_hand(app);
            RAN_SHORT.store(true, Ordering::SeqCst);
            check_a_long_selection_is_taken_in_hand_and_survives(app);
            RAN_TAKEN.store(true, Ordering::SeqCst);
            check_a_footage_paste_splices_at_the_line_and_consumes_the_copy(app);
            RAN_PASTED.store(true, Ordering::SeqCst);
            check_a_lane_gives_the_copy_its_own_row_without_cutting(app);
            RAN_LANE.store(true, Ordering::SeqCst);
            check_esc_drops_the_copy(app);
            RAN_ESC.store(true, Ordering::SeqCst);
            check_copy_then_paste_lengthens_through_the_real_buttons(app);
            RAN_PASTE_LENGTHENS.store(true, Ordering::SeqCst);
            app.quit();
        });
        app.run_with_args::<String>(&[]);
    });
}

#[test]
fn f2_9_s1_s2_s3_copy_paste_and_lane_reach_the_window_through_real_widgets() {
    window_round();
    assert!(RAN_SHORT.load(Ordering::SeqCst), "the short-selection check never ran");
    assert!(RAN_TAKEN.load(Ordering::SeqCst), "the taken-in-hand check never ran");
    assert!(RAN_PASTED.load(Ordering::SeqCst), "the footage-paste check never ran");
    assert!(RAN_LANE.load(Ordering::SeqCst), "the lane check never ran");
    assert!(RAN_ESC.load(Ordering::SeqCst), "the Esc-drops-the-copy check never ran");
    assert!(
        RAN_PASTE_LENGTHENS.load(Ordering::SeqCst),
        "the copy-then-paste-lengthens check never ran"
    );
}

/// F2.9 S1+S2 through the REAL buttons: `copy-button` then `paste-button` on a footage band of >= 1 s
/// splices a `copy:<seconds>` card at the red line, the cut gets longer, the copy is consumed, and the
/// buttons' sensitivity follows `refresh_copy_buttons`' rule -- Copy live only with a band at least
/// `cut_select::MIN_SCENE_SECONDS` long (// P.policy.minSceneSeconds), Paste/Lane only while a hand is held.
fn check_copy_then_paste_lengthens_through_the_real_buttons(app: &adw::Application) {
    let window = cut_window(app);
    drop_copy();
    // A 6 s footage band: comfortably over the 1 s floor (// P.policy.minSceneSeconds = 1.0).
    seed_band(&window, 20.0, 26.0);
    settle();

    let copy_ = ui::line_step_button(&window, "copy-button").expect("copy-button exists");
    let paste_ = ui::line_step_button(&window, "paste-button").expect("paste-button exists");
    assert!(copy_.is_sensitive(), "a 6 s band makes \u{29c9} Copy live");
    assert!(
        !paste_.is_sensitive(),
        "with nothing in hand \u{29c9} Paste is greyed -- that is `refresh_copy_buttons`' rule"
    );

    copy_.emit_clicked();
    settle();
    let hand = ui::copy_hand().expect("the click put the seconds in hand");
    assert_eq!((hand.from, hand.length), (20.0, 6.0), "the hand holds the whole band");
    assert!(paste_.is_sensitive(), "holding a copy makes \u{29c9} Paste live");
    assert!(
        ui::selection(&window).is_some(),
        "S1: taking a copy leaves the selection on the band"
    );

    // Aim it at the red line and press the real Paste button.
    ui::set_line_position(&window, naivepost::cut_line::LinePos { t: 40.0 });
    let before_len = naivepost::cut_screen::cut_seconds(&cut_of(&window));
    let before_segs = ui::review_cut_segs_count(&window);
    paste_.emit_clicked();
    settle();

    let after_segs = ui::review_cut_segs_count(&window);
    assert_eq!(after_segs, before_segs + 1, "the spliced insert joined the cut");
    let card = ui::review_cut_segs(&window)
        .into_iter()
        .find(|seg| seg.ins.contains("copy:"))
        .expect("the pasted segment is a `copy:<seconds>` insert");
    assert_eq!(card.s, 40.0, "it opens the cut at the line");
    assert_eq!(card.dur, 6.0, "and runs for the copied length");
    assert!(
        naivepost::cut_screen::cut_seconds(&cut_of(&window)) > before_len,
        "the video got longer: {before_len} -> {}",
        naivepost::cut_screen::cut_seconds(&cut_of(&window))
    );
    // S2: pasting CONSUMES the copy -- and the buttons go grey again on their own.
    assert!(ui::copy_hand().is_none(), "a successful paste consumed the copy");
    assert!(
        !paste_.is_sensitive(),
        "Paste greyed again once nothing is in hand"
    );
    let printed = status_text(&window);
    assert!(
        printed.contains("pasted"),
        "the status line carries the paste sentence: {printed}"
    );
    window.close();
}
