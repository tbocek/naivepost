//! §06-effects#5-cards-svg-inserts — THE WIRE for a card's own length. The rule lives in
//! [`naivepost::produce_cards::offer_seconds`] and is proven by `tests/produce_cards_bake.rs`; this file proves the
//! Cut page's real widgets reach it: a card whose animation ends at 2 s must open the form's Seconds entry at
//! `2.0`, not at P.policy.insertDefaultSeconds, because ffprobe reads an SVG as one frame and cannot answer this.
//!
//! Widgets fired by name: `insert-seconds` (the offered length), `insert-field-title` (a declared hole filled from
//! the path) and `insert-apply-button` (whose status line is compared with [`naivepost::cut_insert::status_line`] so
//! the drawn sentence and the logic's cannot drift apart).
//!
//! One application, one `connect_activate`, exactly one `#[test]`: these checks mutate process-shared state (this
//! window's cut slot, its line, the status line), so they run once in sequence and report through flags the test
//! asserts on at the end.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut::{self, Seg};
use naivepost::cut_cards as cards;
use naivepost::cut_insert as ins;
use naivepost::cut_screen;
use naivepost::shell::Page;
use naivepost::timeline::Recording;
use naivepost::ui;

#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::{fixture_dir, hold_last_window, release_last_window, settle, status_text, clip, tape, cut_button as button};

static RAN_LENGTH: AtomicBool = AtomicBool::new(false);
static RAN_FIELD: AtomicBool = AtomicBool::new(false);
static RAN_APPLY: AtomicBool = AtomicBool::new(false);
static RAN_ANSWER: AtomicBool = AtomicBool::new(false);

/// Build a window already sitting on the Cut tab with the given tape and cut, the way the insert wire does.
fn cut_page(app: &adw::Application, tape: &[Recording], seeded: &cut::Cut) -> adw::ApplicationWindow {
    release_last_window();
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    assert!(
        model.sources.iter().any(|source| source.footage),
        "the fixture must carry a footage row or the Cut tab is locked (shell::lock)"
    );
    let window = ui::build_window(app, &model, "Prepare");
    hold_last_window(window.clone());
    window.present();
    ui::tab_button(&window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_clicked();
    assert_eq!(ui::state(&window).page, Page::Cut);
    ui::set_session_recordings(tape);
    ui::seed_review_cut(&window, seeded);
    ui::set_held_clip(None);
    ui::clear_selection(&window);
    ui::note_place(false);
    settle();
    window
}

/// Find a named widget anywhere under `root` — the form puts its entries inside rows, so a flat scan of the
/// holder's own children would miss them. Every lookup starts at THIS window: a tree-wide search from another
/// window can hand back a stale copy, since a closed GTK window is not destroyed and its widgets stay parented.
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

fn child_entry(root: &impl AsRef<gtk::Widget>, name: &str) -> gtk::Entry {
    child_named(root, name)
        .and_then(|w| w.downcast::<gtk::Entry>().ok())
        .unwrap_or_else(|| panic!("the insert form drew no {name}"))
}

/// A card that animates for exactly 2 s and then holds: its last moving moment is 2.0 s, which is what the form
/// should open at. No probe is handed in, so the only place that number can come from is the document itself.
fn timed_card(name: &str) -> String {
    write_card_file(
        name,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1920\" height=\"1080\"\
         viewBox=\"0 0 1920 1080\"><rect width=\"1920\" height=\"1080\" fill=\"#0d1117\"/>\
         <text x=\"100\" y=\"200\" opacity=\"0\">Board<animate attributeName=\"opacity\" values=\"0;1\"\
         keyTimes=\"0;1\" dur=\"2s\" fill=\"freeze\"/></text></svg>",
    )
}

/// A card that declares one hole, so the form draws a field for it. Its path carries `?title=…`, which is where the
/// value comes from: `form_plan` reads the query off the path (§G: `what you type is kept on the end of the path`).
fn declared_card(name: &str) -> String {
    let file = write_card_file(
        name,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1920\" height=\"1080\">\
         <!-- Input: title | Title | what the board says -->\
         <text x=\"100\" y=\"200\">{{title}}</text>\
         <text x=\"100\" y=\"300\" opacity=\"0\">New<animate attributeName=\"opacity\" values=\"0;1\"\
         keyTimes=\"0;1\" dur=\"2s\" fill=\"freeze\"/></text></svg>",
    );
    format!("{file}?title=Best maps")
}

fn write_card_file(name: &str, doc: &str) -> String {
    let dir = std::env::temp_dir().join(format!("np-card-len-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("card dir");
    let path = dir.join(name);
    std::fs::write(&path, doc).expect("card written");
    path.to_string_lossy().to_string()
}

/// S3 + §5: the form opens at the CARD'S OWN seconds. Nothing was handed to the seam, so `2.0` can only have come
/// from reading the document — the fallback chain task 2 wired (`selection → file → default`).
fn the_form_opens_at_the_cards_own_seconds(app: &adw::Application) {
    let window = cut_page(app, &tape(), &cut::Cut { segs: vec![clip(0.0, 90.0)], ..Default::default() });
    ui::set_line_position(&window, naivepost::cut_line::LinePos { t: 20.0 });
    let path = timed_card("timed.svg");
    let said = ui::insert_chosen_with_length(&window, std::path::Path::new(&path), 20.0, None);
    settle();
    assert!(said.contains("timed.svg"), "{said}");
    let seconds = child_entry(&window, "insert-seconds").text().to_string();
    assert_eq!(seconds, "2.0", "the card's last moving moment, not the default: {said}");
    assert_ne!(seconds, format!("{:.1}", ins::DEFAULT_SECONDS), "P.policy.insertDefaultSeconds never reached this");
    window.close();
}

/// §5/S4: a card asked for with `?title=…` gets its declared hole's field, holding the path's value — the same
/// split the render applies when it fills the document.
fn the_paths_argument_lands_in_its_field(app: &adw::Application) {
    let window = cut_page(app, &tape(), &cut::Cut { segs: vec![clip(0.0, 90.0)], ..Default::default() });
    ui::set_line_position(&window, naivepost::cut_line::LinePos { t: 20.0 });
    let asked = declared_card("declared.svg");
    // The chooser hands back the FILE; the parameters ride on the end of its path (§G), which is what fills the
    // field — so the file is what this seam is given, and the query is what the plan reads off it.
    let file = asked.split('?').next().unwrap_or(&asked);
    ui::insert_chosen_with_length(&window, std::path::Path::new(file), 20.0, None);
    settle();
    assert!(
        child_named(&window, "insert-field-title").is_some(),
        "the card's declared `title` hole has its own entry"
    );
    // A card chosen fresh has nothing typed yet: the field asks rather than guessing.
    assert_eq!(child_entry(&window, "insert-field-title").text().as_str(), "");
    // And its own length still arrives: a declared hole did not cost the card its timing.
    assert_eq!(child_entry(&window, "insert-seconds").text().as_str(), "2.0");
    window.close();
}

/// S5: Apply writes the card and says §G's sentence — compared against [`ins::status_line`] called directly, so
/// the drawn line and the logic's line are the same string rather than two guesses.
fn apply_says_what_the_logic_would_say(app: &adw::Application) {
    let seeded = cut::Cut { segs: vec![clip(0.0, 90.0)], ..Default::default() };
    let was = cut_screen::cut_seconds(&seeded);
    let window = cut_page(app, &tape(), &seeded);
    ui::set_line_position(&window, naivepost::cut_line::LinePos { t: 20.0 });
    let path = timed_card("spliced.svg");
    ui::insert_chosen_with_length(&window, std::path::Path::new(&path), 20.0, None);
    settle();
    button(&window, "insert-apply-button").emit_by_name::<()>("clicked", &[]);
    settle();

    let after = cut_of(&window);
    let placed: &Seg = after.segs.iter().find(|seg| seg.is_insert()).expect("the card is in the cut");
    assert_eq!((placed.s, placed.e), (20.0, 20.0), "BETWEEN splices: s == e");
    assert_eq!(placed.dur, 2.0, "for the card's own two seconds");
    let now = cut_screen::cut_seconds(&after);
    let line = status_text(&window);
    // The same sentence `cut_insert::place` builds, called directly with the mode's own wording — BETWEEN's phrase is
    // written there rather than retyped here, so the two cannot drift.
    let how = "between the footage, which is cut open for it";
    let expected = ins::status_line(&path, 20.0, 2.0, how, now, was);
    assert_eq!(line, expected, "the widget's sentence is the logic's sentence");
    window.close();
}

/// §G (`what you type is kept on the end of the path`) + §5: what goes into `insert-field-title` lands ON THE
/// CLIP'S SOURCE, because that is the only place the producer reads a card's arguments from. A hole left blank
/// writes no argument at all — the hole keeps its own fallback rather than being filled with emptiness — and a
/// path that already carries one gains the new key without losing the old.
fn typed_answers_land_on_the_paths_arguments(app: &adw::Application) {
    let window = cut_page(app, &tape(), &cut::Cut { segs: vec![clip(0.0, 90.0)], ..Default::default() });
    ui::set_line_position(&window, naivepost::cut_line::LinePos { t: 20.0 });

    // (a) one hole, one answer: the stored source carries it, and the document that answers it has no hole left.
    // The chooser seam is handed the FILE (§G puts the parameters on the end of its path), so that is what this
    // seam gets; the field starts empty and the typed value is what lands on the cut's source.
    let plain = write_card_file(
        "answered.svg",
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1920\" height=\"1080\">\
         <!-- Input: title | Title | what the board says -->\
         <text x=\"100\" y=\"200\">{{title}}</text>\
         <text x=\"100\" y=\"300\" opacity=\"0\">New<animate attributeName=\"opacity\" values=\"0;1\" \
         keyTimes=\"0;1\" dur=\"2s\" fill=\"freeze\"/></text></svg>",
    );
    ui::insert_chosen_with_length(&window, std::path::Path::new(&plain), 20.0, Some(4.0));
    settle();
    field_entry(&window, "insert-field-title").set_text("Best maps");
    button(&window, "insert-apply-button").emit_by_name::<()>("clicked", &[]);
    settle();
    let placed = cut_of(&window)
        .segs
        .into_iter()
        .find(|seg| seg.is_insert())
        .expect("the answered card is in the cut");
    let args = cards::split_path(&placed.ins).args;
    assert_eq!(
        args,
        vec![("title".to_string(), "Best maps".to_string())],
        "the typed answer rides on the path the render will read: {:?}",
        placed.ins
    );
    let doc = std::fs::read_to_string(&plain).expect("the card file");
    let answered: Vec<String> = ins::answered_holes(&doc, &args)
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    assert_eq!(answered, vec!["title".to_string()], "that answer fills the hole it names");
    assert!(
        !cards::fill(&doc, &args).contains("{{title}}"),
        "the filled document has no hole left for the producer to leave unfilled"
    );
    window.close();

    // (b) nothing typed: no `?` at all, so the card is asked about as the bare file it is.
    let window = cut_page(app, &tape(), &cut::Cut { segs: vec![clip(0.0, 90.0)], ..Default::default() });
    ui::set_line_position(&window, naivepost::cut_line::LinePos { t: 20.0 });
    ui::insert_chosen_with_length(&window, std::path::Path::new(&plain), 20.0, Some(4.0));
    settle();
    button(&window, "insert-apply-button").emit_by_name::<()>("clicked", &[]);
    settle();
    let bare = cut_of(&window)
        .segs
        .into_iter()
        .find(|seg| seg.is_insert())
        .expect("the unanswered card is in the cut");
    assert_eq!(bare.ins, plain, "an empty field writes no argument: {}", bare.ins);
    window.close();

    // (c) a path that already carries an argument keeps it and gains the typed one, round-tripping split/join.
    let window = cut_page(app, &tape(), &cut::Cut { segs: vec![clip(0.0, 90.0)], ..Default::default() });
    ui::set_line_position(&window, naivepost::cut_line::LinePos { t: 20.0 });
    // A card declaring TWO holes, so one answer can join an argument the path already carried instead of
    // replacing it. The chooser hands back the file; §G's parameters ride on the end of its path.
    let two_holes = write_card_file(
        "written.svg",
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1920\" height=\"1080\">\
         <!-- Input: title | Title | what the board says -->\
         <!-- Input: who | Who | whose record it is -->\
         <text x=\"100\" y=\"200\">{{title}}</text>\
         <text x=\"100\" y=\"300\">{{who}}</text>\
         <text x=\"100\" y=\"400\" opacity=\"0\">New<animate attributeName=\"opacity\" values=\"0;1\" \
         keyTimes=\"0;1\" dur=\"2s\" fill=\"freeze\"/></text></svg>",
    );
    let written = format!("{two_holes}?keep=on");
    // (c) below asks `answer_path` about a source that already carries an argument, so nothing is placed here.
    window.close();
    // (c) a typed answer joins an argument the source already carried instead of replacing it, and the path
    // round-trips split/join so the render splits exactly what the form wrote. The seam stands in for the
    // chooser's FILE; `answer_path` is asked directly about a source that already carries `keep=on`, which is
    // the state a re-edited card's clip is in (§G keeps parameters on the end of the path).
    let joined = ins::answer_path(&written, &[("title".to_string(), "Ranks & records".to_string())]);
    let args = cards::split_path(&joined).args;
    assert_eq!(
        args.iter().filter(|(key, _)| key == "keep").count(),
        1,
        "the argument the path already carried survives exactly once: {joined}"
    );
    assert!(
        args.iter()
            .any(|(key, value)| key == "title" && value == "Ranks & records"),
        "and the typed one joins it: {joined}"
    );
    assert_eq!(
        cards::join_path(&cards::split_path(&joined)),
        joined,
        "the path round-trips, so the render splits what the form wrote"
    );
    window.close();
}

/// The insert form holder THIS window drew its form into — the VISIBLE one, not merely the first box named
/// `insert-form`. An unscoped name exists once per tab (`page_box` runs per tab) and a closed window's widgets
/// stay parented somewhere, so a tree-wide lookup can land on another tab's or another window's empty holder and
/// report "no such field" for a form that was drawn. The live holder is reached the way the form's own buttons
/// are: from a widget only the open form draws, up to the box that holds them.
fn live_holder(window: &adw::ApplicationWindow) -> gtk::Box {
    child_named(window, "insert-mode-between")
        .and_then(|radio| radio.parent())
        .and_then(|parent| parent.downcast::<gtk::Box>().ok())
        .expect("the open insert form drew its BETWEEN radio inside the form box")
}

/// A field entry of the open form, found under the holder that is drawing it.
fn field_entry(window: &adw::ApplicationWindow, name: &str) -> gtk::Entry {
    let holder = live_holder(window);
    child_named(&holder, name)
        .and_then(|w| w.downcast::<gtk::Entry>().ok())
        .unwrap_or_else(|| panic!("the open insert form drew no {name} under {:?}", all_names(holder.upcast_ref())))
}

/// Every widget name below `root`, so a miss says what WAS drawn rather than only what was not.
fn all_names(root: &gtk::Widget) -> Vec<String> {
    let mut out = vec![root.widget_name().to_string()];
    for child in root.observe_children().iter::<glib::Object>() {
        let Ok(child) = child else { continue };
        let Ok(widget) = child.downcast::<gtk::Widget>() else { continue };
        out.extend(all_names(&widget));
    }
    out
}

/// This window's cut as a whole, so its length is measured the way the page measures it.
fn cut_of(window: &adw::ApplicationWindow) -> cut::Cut {
    cut::Cut {
        segs: ui::review_cut_segs(window),
        lanes: ui::review_lanes(window),
        ..Default::default()
    }
}

fn window_round() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gtk::gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(|app| {
            the_form_opens_at_the_cards_own_seconds(app);
            RAN_LENGTH.store(true, Ordering::SeqCst);
            the_paths_argument_lands_in_its_field(app);
            RAN_FIELD.store(true, Ordering::SeqCst);
            apply_says_what_the_logic_would_say(app);
            RAN_APPLY.store(true, Ordering::SeqCst);
            typed_answers_land_on_the_paths_arguments(app);
            RAN_ANSWER.store(true, Ordering::SeqCst);
            app.quit();
        });
        app.run_with_args::<String>(&[]);
    });
}

#[test]
fn sec_06_effects_5_cards_svg_inserts_wire_a_cards_own_length_reaches_the_form_through_real_widgets() {
    // Pin cwd BEFORE anything builds: the presses resolve the project through `startup::session_dir(current_dir())`,
    // and leaving it at rust/ writes a stray `rust/session.naivepost/` into the repo.
    let root = std::env::temp_dir().join(format!("np-card-len-root-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temp root created");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");

    window_round();
    assert!(RAN_LENGTH.load(Ordering::SeqCst), "the offered-length check never ran");
    assert!(RAN_FIELD.load(Ordering::SeqCst), "the declared-field check never ran");
    assert!(RAN_APPLY.load(Ordering::SeqCst), "the Apply check never ran");
    assert!(
        RAN_ANSWER.load(Ordering::SeqCst),
        "the typed-answer-to-path check never ran"
    );
}
