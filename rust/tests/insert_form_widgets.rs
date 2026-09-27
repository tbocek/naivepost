// F2.12 (Insert a card, still, video or sound) — THE WIRE. The rules live in `src/cut_insert.rs` and are
// proven by tests/cut_insert_card.rs; this file proves the Cut page's real widgets reach them. Every check
// fires a named widget the way GTK does (`emit_by_name("clicked")`) or drives the seam the chooser's own
// callback calls (`insert_chosen_with_length`), then asserts the SAME state the logic test asserts — the
// cut's own segments, the lane list, the plan behind the drawn form — rather than a painted pixel.
//
// Widgets fired by name: `insert-button` (S1 refusal, S6 Edit), `insert-mode-between` / `-over` / `-lane`,
// `insert-seconds`, `insert-sound-tick` (S4) and `insert-apply-button` (S5).
//
// One application, one `connect_activate`, exactly one `#[test]`: these checks mutate process-shared state
// (this window's cut slot, its line, its held clip and the status line), so they run once in sequence and
// report through flags the test asserts on at the end.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut::{self, Lane, Seg};
use naivepost::cut_insert as ins;
use naivepost::cut_screen;
use naivepost::shell::Page;
use naivepost::timeline::Recording;
use naivepost::ui;

static RAN_REFUSE: AtomicBool = AtomicBool::new(false);
static RAN_LIVE: AtomicBool = AtomicBool::new(false);
static RAN_FORM: AtomicBool = AtomicBool::new(false);
static RAN_SPLICE: AtomicBool = AtomicBool::new(false);
static RAN_OVER: AtomicBool = AtomicBool::new(false);
static RAN_EDIT: AtomicBool = AtomicBool::new(false);
static RAN_LANE_ONLY: AtomicBool = AtomicBool::new(false);

fn fixture_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/demo.naivepost")
}

/// A footage scene on camera 0.
fn clip(s: f64, e: f64) -> Seg {
    Seg { s, e, cam: 0, ..Default::default() }
}

/// A spliced card: `s == e` with a `dur`, which is what makes it an insert (`Seg::is_insert`).
fn card(at: f64, dur: f64, path: &str) -> Seg {
    Seg { s: at, e: at, dur, ins: path.to_string(), ..Default::default() }
}

thread_local! {
    /// The only strong handle this test holds on the window it last built. Kept in a slot so the NEXT check
    /// can drop it before building its own: a closed GTK window is not destroyed, its widget tree survives
    /// with every `insert-form` still parented to it, and a name lookup from a new window can reach those
    /// leftovers (`gtk_box_append: assertion 'gtk_widget_get_parent (child) == NULL' failed`).
    static LAST_WINDOW: std::cell::RefCell<Option<adw::ApplicationWindow>> =
        const { std::cell::RefCell::new(None) };
}

fn release_last_window() {
    LAST_WINDOW.with(|cell| *cell.borrow_mut() = None);
    settle();
}

/// Build a window already sitting on the Cut tab with the given tape and cut, the way the fold wire does.
fn cut_page(
    app: &adw::Application,
    tape: &[Recording],
    seeded: &cut::Cut,
) -> adw::ApplicationWindow {
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
        .emit_clicked();
    assert_eq!(ui::state(&window).page, Page::Cut);
    ui::set_session_recordings(tape);
    ui::seed_review_cut(&window, seeded);
    // Nothing left in hand from the previous check: a held card would make Insert read "Edit" where this
    // check means "Insert", and a held band would answer S1 where the check wants "no target".
    ui::set_held_clip(None);
    ui::clear_selection(&window);
    // Start every check with no line placed: `line_position` reads 0.0 whether or not a line exists, so the
    // page's own "has a target" flag is what S1's refusal turns on, and a leftover from the previous check
    // would make Insert answer live where this check wants the refusal.
    ui::note_place(false);
    settle();
    window
}

/// The session filmed 0–120 as one take.
fn tape() -> Vec<Recording> {
    vec![Recording { base: "session-tape".to_string(), start: 0.0, end: 120.0 }]
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

/// This window's cut as a whole, so its length is measured the same way the page measures it.
fn cut_of(window: &adw::ApplicationWindow) -> cut::Cut {
    let mut fresh = cut::Cut::default();
    fresh.segs = ui::review_cut_segs(window);
    fresh.lanes = ui::review_lanes(window);
    fresh
}

fn button(window: &adw::ApplicationWindow, name: &str) -> gtk::Button {
    ui::line_step_button(window, name).unwrap_or_else(|| panic!("the Cut page drew no {name}"))
}

/// Find a widget THIS window drew, by name — the same walk the page's own accessors use. A tree-wide search
/// from another window can hand back a stale copy, so every lookup here starts at this window.
/// The insert form holder THIS window built, reached through the parent chain of a widget we know it drew.
/// A tree-wide name lookup can land on another window's `insert-form` (a closed GTK window is not
/// destroyed), which is why every form-widget lookup here starts at the holder we just filled.
fn form_holder(window: &adw::ApplicationWindow) -> gtk::Box {
    let between = widget_named(window, "insert-mode-between")
        .expect("the insert form drew its BETWEEN radio");
    between
        .parent()
        .and_then(|p| p.downcast::<gtk::Box>().ok())
        .expect("the radios sit inside the insert form box")
}

fn widget_named(window: &adw::ApplicationWindow, name: &str) -> Option<gtk::Widget> {
    fn walk(root: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
        if root.widget_name() == name {
            return Some(root.clone());
        }
        for child in root.observe_children().iter::<glib::Object>() {
            let Ok(child) = child else { continue };
            let Ok(widget) = child.downcast::<gtk::Widget>() else { continue };
            if let Some(found) = walk(&widget, name) {
                return Some(found);
            }
        }
        None
    }
    walk(window.upcast_ref(), name)
}

fn child_names(holder: &gtk::Box) -> Vec<String> {
    holder
        .observe_children()
        .iter::<glib::Object>()
        .flatten()
        .filter_map(|c| c.downcast::<gtk::Widget>().ok())
        .map(|w| w.widget_name().to_string())
        .collect()
}

fn child_check(holder: &gtk::Box, name: &str) -> gtk::CheckButton {
    child_named(holder, name)
        .and_then(|w| w.downcast::<gtk::CheckButton>().ok())
        .unwrap_or_else(|| panic!("the insert form drew no {name} in {:?}", child_names(holder)))
}

fn child_entry(holder: &gtk::Box, name: &str) -> gtk::Entry {
    child_named(holder, name)
        .and_then(|w| w.downcast::<gtk::Entry>().ok())
        .unwrap_or_else(|| panic!("the insert form drew no {name} in {:?}", child_names(holder)))
}

/// Find a named widget anywhere under `root` — the form puts its entries inside rows, so a flat scan of
/// the holder's own children would miss them.
fn child_named(root: &impl AsRef<gtk::Widget>, name: &str) -> Option<gtk::Widget> {
    let root = root.as_ref();
    if root.widget_name() == name {
        return Some(root.clone());
    }
    for child in root.observe_children().iter::<glib::Object>() {
        let Ok(child) = child else { continue };
        let Ok(widget) = child.downcast::<gtk::Widget>() else { continue };
        if let Some(found) = child_named(&widget, name) {
            return Some(found);
        }
    }
    None
}

fn radio(window: &adw::ApplicationWindow, name: &str) -> gtk::CheckButton {
    maybe_radio(window, name).unwrap_or_else(|| panic!("the insert form drew no {name}"))
}

fn maybe_radio(window: &adw::ApplicationWindow, name: &str) -> Option<gtk::CheckButton> {
    widget_named(window, name)?.downcast::<gtk::CheckButton>().ok()
}

/// S1: with neither a red line nor a selection, a real click on ⧉ Insert refuses with §G's own sentence
/// and writes nothing into the cut.
fn f2_12_s1_a_click_with_no_target_refuses(app: &adw::Application) {
    let window = cut_page(app, &tape(), &cut::Cut::default());
    // No line placed, no seconds marked: the page opened with nowhere for an insert to go.
    assert_eq!(ui::selection(&window), None, "no band on a fresh page");
    assert_eq!(
        ui::line_position(&window).t,
        0.0,
        "and no target was claimed, so Insert has nothing to place against"
    );

    let insert = button(&window, "insert-button");
    insert.emit_by_name::<()>("clicked", &[]);
    settle();
    assert_eq!(
        status_text(&window),
        ins::NO_LINE_YET,
        "the refusal must be §G's sentence, verbatim"
    );
    assert!(
        ui::review_cut_segs(&window).is_empty(),
        "a refused insert writes no segment"
    );
    assert!(ui::insert_open(&window).is_none(), "and opens no form");
    window.close();
}

/// S1: the greying pair. A line alone lights Insert; without a line AND without a selection it stays greyed
/// — the state the previous round left permanently true.
fn f2_12_s1_a_line_or_a_selection_makes_insert_live(app: &adw::Application) {
    let window = cut_page(app, &tape(), &cut::Cut::default());
    // Greyed while there is no target at all.
    assert!(
        !button(&window, "insert-button").is_sensitive(),
        "with no line and no selection Insert must be greyed"
    );

    // A red line alone answers "where": Insert lights up.
    ui::set_line_position(&window, naivepost::cut_line::LinePos { t: 30.0 });
    ui::refresh_insert_button(&window);
    settle();
    assert!(
        button(&window, "insert-button").is_sensitive(),
        "a placed line gives Insert somewhere to go"
    );

    // And a band answers it too, with its own start.
    ui::set_line_position(&window, naivepost::cut_line::LinePos { t: 0.0 });
    ui::draw_selection(
        &window,
        naivepost::cut_select::Surface::PictureRow(0),
        None,
        40.0,
        46.0,
    );
    settle();
    assert!(
        button(&window, "insert-button").is_sensitive(),
        "a selection gives it somewhere too"
    );
    assert_eq!(
        ui::selection(&window).map(|band| band.start),
        Some(40.0),
        "the band the button reads is the one that was drawn"
    );
    window.close();
}

/// S4: the form the chooser's seam draws. Three modes with §G's labels, LANE offered for video only, the
/// Seconds entry open at the plan's length, and the sound tick present only when there is a sound to answer
/// for — greyed while LANE is chosen.
///
/// Every lookup here is scoped to the form box this window drew (`form_holder`), because a name search from
/// the window can land on another window's `insert-form`: a closed GTK window is not destroyed and its
/// widgets stay parented somewhere.
fn f2_12_s4_the_form_offers_modes_seconds_and_the_sound_tick(app: &adw::Application) {
    let window = cut_page(app, &tape(), &cut::Cut { segs: vec![clip(0.0, 90.0)], ..Default::default() });
    ui::set_line_position(&window, naivepost::cut_line::LinePos { t: 20.0 });

    // --- a video: all three modes are on offer ---
    let video_path = write_video("sting.mp4");
    let said = ui::insert_chosen_with_length(&window, std::path::Path::new(&video_path), 20.0, Some(7.5));
    settle();
    assert!(said.contains("sting.mp4"), "{said}");
    assert!(said.contains("7.5 s"), "the plan's length is said back: {said}");

    let holder = form_holder(&window);
    let between = child_check(&holder, "insert-mode-between");
    let over = child_check(&holder, "insert-mode-over");
    let lane = child_check(&holder, "insert-mode-lane");
    assert_eq!(between.label().unwrap_or_default().to_string(), ins::BETWEEN_LABEL, "BETWEEN wears §G's label");
    assert_eq!(over.label().unwrap_or_default().to_string(), ins::OVER_LABEL, "OVER likewise");
    assert_eq!(lane.label().unwrap_or_default().to_string(), ins::LANE_LABEL, "and LANE, offered because it is video");
    assert!(between.is_active(), "no selection means splice first (S3)");
    assert!(!over.is_active() && !lane.is_active(), "one mode at a time");
    assert_eq!(
        child_entry(&holder, "insert-seconds").text().as_str(),
        "7.5",
        "S3: the file's own length opens the Seconds entry"
    );

    // --- a still card: LANE is NOT offered, so the radio is simply absent ---
    let card_path = write_card("still-card.svg");
    let said = ui::insert_chosen_with_length(&window, std::path::Path::new(&card_path), 20.0, Some(3.0));
    settle();
    assert!(said.contains("still-card.svg"), "{said}");
    let holder = form_holder(&window);
    assert!(
        child_named(&holder, "insert-mode-lane").is_none(),
        "a still cannot have a row of its own, so no LANE radio is drawn"
    );
    // The card's declared field got its own named entry (§G: one entry per declared card field).
    assert!(
        child_named(&holder, "insert-field-title").is_some(),
        "the card's `title` hole has its own entry: {:?}",
        child_names(&holder)
    );
    assert_eq!(
        child_entry(&holder, "insert-seconds").text().as_str(),
        "3.0",
        "and the entry opened at the length handed in"
    );
    window.close();
}

/// A tiny declared card on disk, so the SVG door has something to read.
/// A video file on disk with a sound track declared, so the chooser's answer has something real behind it.
fn write_video(name: &str) -> String {
    let dir = std::env::temp_dir().join("np-insert-cards");
    std::fs::create_dir_all(&dir).expect("video dir");
    let path = dir.join(name);
    // The bytes are irrelevant to `cut_insert::kind` (extension only), but writing one keeps the seam
    // honest: this is a file the chooser could actually have returned.
    std::fs::write(&path, b"not really a video, only a name").expect("video written");
    path.to_string_lossy().to_string()
}

fn write_card(name: &str) -> String {
    let dir = std::env::temp_dir().join("np-insert-cards");
    std::fs::create_dir_all(&dir).expect("card dir");
    let path = dir.join(name);
    std::fs::write(
        &path,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1920\" height=\"1080\">\
         <!-- Input: title | Title | what the board says -->\
         <text x=\"100\" y=\"200\">{{title}}</text></svg>",
    )
    .expect("card written");
    path.to_string_lossy().to_string()
}

/// S5: Apply on BETWEEN splices — `s == e` with a `dur`, the video longer by the card's own seconds, and
/// the status line carrying §G's whole sentence including the undo clause.
fn f2_12_s5_apply_between_splices_a_card(app: &adw::Application) {
    let seeded = cut::Cut { segs: vec![clip(0.0, 90.0)], ..Default::default() };
    let was = cut_screen::cut_seconds(&seeded);
    let window = cut_page(app, &tape(), &seeded);
    ui::set_line_position(&window, naivepost::cut_line::LinePos { t: 20.0 });
    let path = write_card("splice-card.svg");
    ui::insert_chosen_with_length(&window, std::path::Path::new(&path), 20.0, Some(4.0));
    settle();
    radio(&window, "insert-mode-between").set_active(true);
    settle();

    button(&window, "insert-apply-button").emit_by_name::<()>("clicked", &[]);
    settle();

    let after = cut_of(&window);
    let placed = after
        .segs
        .iter()
        .find(|seg| seg.is_insert())
        .expect("a spliced card is in the cut");
    assert_eq!(placed.s, placed.e, "spliced means s == e");
    assert_eq!(placed.dur, 4.0, "and carries its own dur");
    assert_eq!(placed.s, 20.0, "at the second the page read");
    let now = cut_screen::cut_seconds(&after);
    assert!(
        (now - (was + 4.0)).abs() < 1e-9,
        "the video grew by the card alone: was {was}, now {now}"
    );
    let line = status_text(&window);
    assert!(line.contains("inserted at 00:20 for 4.0 s"), "{line}");
    assert!(line.contains("the cut is now 01:34 (was 01:30)"), "{line}");
    assert!(line.contains("\u{21b6} Undo takes it back"), "{line}");
    assert!(ui::insert_open(&window).is_none(), "Apply closes the form");
    window.close();
}

/// S5: Apply on OVER takes exactly those seconds — the card spans them and carries NO `dur`, and the footage
/// under it is gone from the cut.
fn f2_12_s5_apply_over_replaces_those_seconds(app: &adw::Application) {
    let seeded = cut::Cut { segs: vec![clip(0.0, 90.0)], ..Default::default() };
    let window = cut_page(app, &tape(), &seeded);
    ui::set_line_position(&window, naivepost::cut_line::LinePos { t: 20.0 });
    let path = write_card("over-card.svg");
    ui::insert_chosen_with_length(&window, std::path::Path::new(&path), 20.0, Some(5.0));
    settle();
    radio(&window, "insert-mode-over").set_active(true);
    settle();

    button(&window, "insert-apply-button").emit_by_name::<()>("clicked", &[]);
    settle();

    let after = cut_of(&window);
    let placed = after
        .segs
        .iter()
        .find(|seg| seg.is_overwrite_insert())
        .expect("an overwriting card is in the cut");
    assert_eq!((placed.s, placed.e), (20.0, 25.0), "it runs for the seconds it took");
    assert_eq!(placed.dur, 0.0, "and carries no dur of its own");
    assert!(
        !after.segs.iter().any(|seg| seg.ins.is_empty() && seg.s < 25.0 && seg.e > 20.0),
        "no kept footage is left under it: {:?}",
        after.segs
    );
    // The video did NOT grow: the card fills the hole rather than adding to it.
    let now = cut_screen::cut_seconds(&after);
    assert!((now - 90.0).abs() < 1e-9, "over costs no extra seconds: {now}");
    assert!(status_text(&window).contains("inserted at 00:20 for 5.0 s"));
    window.close();
}

/// S6: holding a card turns Insert into Edit, and a held card that is no longer in the cut answers
/// `that card is no longer in the cut`.
fn f2_12_s6_a_held_card_makes_insert_edit(app: &adw::Application) {
    let path = write_card("held-card.svg");
    let seeded = cut::Cut {
        segs: vec![clip(0.0, 90.0), card(30.0, 4.0, &path)],
        ..Default::default()
    };
    let window = cut_page(app, &tape(), &seeded);
    // Hold the card that IS in the cut: the button changes word.
    ui::set_held_clip(Some(card(30.0, 4.0, &path)));
    ui::refresh_insert_button(&window);
    settle();
    assert_eq!(
        button(&window, "insert-button").label().unwrap_or_default().to_string(),
        ins::edit_verb(true),
        "a held card makes the button say Edit"
    );
    assert_eq!(ins::edit_verb(true), "Edit", "and the word is Edit, not Insert");

    // Hold a card the cut does not contain: pressing says so plainly rather than opening a stale form.
    ui::set_held_clip(Some(card(77.0, 4.0, "assets/gone.svg")));
    ui::refresh_insert_button(&window);
    settle();
    button(&window, "insert-button").emit_by_name::<()>("clicked", &[]);
    settle();
    assert_eq!(status_text(&window), ins::GONE, "{}", status_text(&window));
    window.close();
}

/// S5/S4: LANE adds a row and cuts nothing — no new segment, one more lane, and the lane carries the file.
fn f2_12_s5_lane_adds_a_row_and_cuts_nothing(app: &adw::Application) {
    let seeded = cut::Cut { segs: vec![clip(0.0, 90.0)], ..Default::default() };
    let window = cut_page(app, &tape(), &seeded);
    ui::set_line_position(&window, naivepost::cut_line::LinePos { t: 20.0 });
    let path = write_card("lane-video.mp4");
    ui::insert_chosen_with_length(&window, std::path::Path::new(&path), 20.0, Some(6.0));
    settle();
    radio(&window, "insert-mode-lane").set_active(true);
    settle();

    button(&window, "insert-apply-button").emit_by_name::<()>("clicked", &[]);
    settle();

    let after = cut_of(&window);
    assert_eq!(after.segs.len(), 1, "a lane cuts nothing: {:?}", after.segs);
    let lanes: Vec<&Lane> = after.lanes.iter().collect();
    assert_eq!(lanes.len(), 1, "one row was added: {lanes:?}");
    assert!(lanes[0].src.contains("lane-video.mp4"), "{}", lanes[0].src);
    assert_eq!(lanes[0].at, 20.0, "starting where the page read");
    assert_eq!(lanes[0].dur, 6.0, "for the file's own seconds");
    assert!(status_text(&window).contains("on a lane of its own"), "{}", status_text(&window));
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
            f2_12_s1_a_click_with_no_target_refuses(app);
            RAN_REFUSE.store(true, Ordering::SeqCst);
            f2_12_s1_a_line_or_a_selection_makes_insert_live(app);
            RAN_LIVE.store(true, Ordering::SeqCst);
            f2_12_s4_the_form_offers_modes_seconds_and_the_sound_tick(app);
            RAN_FORM.store(true, Ordering::SeqCst);
            f2_12_s5_apply_between_splices_a_card(app);
            RAN_SPLICE.store(true, Ordering::SeqCst);
            f2_12_s5_apply_over_replaces_those_seconds(app);
            RAN_OVER.store(true, Ordering::SeqCst);
            f2_12_s6_a_held_card_makes_insert_edit(app);
            RAN_EDIT.store(true, Ordering::SeqCst);
            f2_12_s5_lane_adds_a_row_and_cuts_nothing(app);
            RAN_LANE_ONLY.store(true, Ordering::SeqCst);
            app.quit();
        });
        app.run_with_args::<String>(&[]);
    });
}

#[test]
fn f2_12_wire_the_insert_button_and_form_reach_the_rules_through_real_widgets() {
    // Pin cwd to our own temp root BEFORE anything builds: the presses resolve the project through
    // `startup::session_dir(current_dir())`, and leaving it at rust/ writes a stray
    // `rust/session.naivepost/` into the repo.
    let root = std::env::temp_dir().join(format!("np-insert-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temp root created");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");

    window_round();
    assert!(RAN_REFUSE.load(Ordering::SeqCst), "the no-target refusal check never ran");
    assert!(RAN_LIVE.load(Ordering::SeqCst), "the sensitivity check never ran");
    assert!(RAN_FORM.load(Ordering::SeqCst), "the form check never ran");
    assert!(RAN_SPLICE.load(Ordering::SeqCst), "the splice check never ran");
    assert!(RAN_OVER.load(Ordering::SeqCst), "the overwrite check never ran");
    assert!(RAN_EDIT.load(Ordering::SeqCst), "the held-card Edit check never ran");
    assert!(RAN_LANE_ONLY.load(Ordering::SeqCst), "the lane check never ran");
}
