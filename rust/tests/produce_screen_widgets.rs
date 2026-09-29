//! §08-produce#1-screen — the wire: the Produce tab holds §A's fifteen widgets and the plain ones
//! answer, through the real widgets.
//!
//! `naivepost::produce_screen` holds every row, option, default and sentence (proven by
//! tests/produce_screen.rs); this file proves the page drew them rather than leaving a stub behind, and
//! that a press moves the held state and prints the rules' own sentence on the status line.
//!
//! One application, one `connect_activate`, exactly one `Application::run`: `g_application_run` refuses
//! a second claimant of the default main context, so all checks live inside that single activate and
//! each ends with the window dropped before the next is built. cwd is pinned to our own temp root BEFORE
//! the run because the page resolves the session through `startup::session_dir(current_dir())`; leaving
//! it at rust/ writes a stray `rust/session.naivepost/` into the repo. No sleeps: `settle()` pumps the
//! glib context instead.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::produce_screen as ps;
use naivepost::shell::Page;
use naivepost::ui;
#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::{fixture_dir, hold_last_window, release_last_window, settle, widget_in};

static RAN_PLATE: AtomicBool = AtomicBool::new(false);
static RAN_SETTINGS: AtomicBool = AtomicBool::new(false);
static RAN_FORCED: AtomicBool = AtomicBool::new(false);
static RAN_IMAGES: AtomicBool = AtomicBool::new(false);
static RAN_CHANGE: AtomicBool = AtomicBool::new(false);
static RAN_SETTINGS_WIRE: AtomicBool = AtomicBool::new(false);
static RAN_WORDS: AtomicBool = AtomicBool::new(false);

/// §A's fifteen, by the name a test finds each one by. The item id is spelled in every entry's comment
/// so the gate log shows which of the fifteen each check was about.
const WIDGETS: [&str; 15] = [
    "add-image-button",         // **1**  ＋ Add image…              sec_08_produce_1_screen S1
    "publish-images",          // **2**  the eight slots            sec_08_produce_1_screen S1
    "edit-instruction",        // **3**  Edit instruction           sec_08_produce_1_screen S1
    "negative-prompt",         // **4**  Negative prompt            sec_08_produce_1_screen S1
    "thumbnail-export",        // **5**  ⤓ export                   sec_08_produce_1_screen S1
    "thumbnail-redraw",        // **6**  ↻ redraw                   sec_08_produce_1_screen S1
    "thumbnail-picture",       // **7**  the picture + overlay editor  sec_08_produce_1_screen S1
    "title-entry",             // **8**  Title                      sec_08_produce_1_screen S1
    "title-suggest",           // **8b** ↻ suggest                  sec_08_produce_1_screen S1
    "description-box",         // **9**  YouTube description        sec_08_produce_1_screen S1
    "encoder-settings",       // **10** the thirteen rows in a scroller  sec_08_produce_1_screen S2
    "transcode-save",         // **11** ⤓ save video                sec_08_produce_1_screen S1
    "transcode-again",        // **12** ↻ encode again              sec_08_produce_1_screen S1
    "produce-inputs",         // **13** Inputs readout              sec_08_produce_1_screen S4
    "produce-outputs",        // **14** Outputs count + folder     sec_08_produce_1_screen S4
];

/// A widget somewhere in THIS window's tree, found from the window itself rather than from any global
/// slot: a closed window survives, so a tree-wide search can land on another window's copy.
/// `sec_08_produce_1_screen` uses this for every one of §A's fifteen widgets.
fn click(window: &adw::ApplicationWindow, name: &str) {
    ui::line_step_button(window, name)
        .unwrap_or_else(|| panic!("the Produce page drew no {name}"))
        .emit_by_name::<()>("clicked", &[]);
    settle();
}

/// The three tick controls §A names outright; they are rows but do not follow the `setting-<row>`
/// spelling, so the row counter accepts them by their own names.
const TICK_CONTROLS: [&str; 3] = ["tick-peak-rate-vfr", "tick-mono", "tick-blurred"];

/// The settings rows the page has DRAWN, read off the screen rather than from the page's hidden state:
/// a hidden row must count as absent, so what is under test is `refresh`, not whether a
/// `ProduceState::default()` agrees with itself. A name counts when it is a `setting-*` control or one
/// of the three tick controls; sorted, so a failure prints the list in a stable order.
fn drawn_setting_rows(window: &adw::ApplicationWindow) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    fn walk(node: &gtk::Widget, out: &mut Vec<String>) {
        let name: String = node.widget_name().to_string();
        if name.starts_with("setting-") || TICK_CONTROLS.contains(&name.as_str()) {
            out.push(name);
        }
        if let Some(child) = node.first_child() {
            let mut cursor = Some(child);
            while let Some(current) = cursor {
                walk(&current, out);
                cursor = current.next_sibling();
            }
        }
    }
    if let Some(content) = window.content() {
        walk(&content, &mut found);
    }
    found.sort();
    found
}

/// The page's state for the row and forcing checks, spelled from `produce_screen::defaults()` rather
/// than hand-written here: the defaults come from the rules module, so this test cannot drift from what
/// the page shows. The clip count and duration are the page's own Inputs readout, which the cut flow
/// publishes.
fn published() -> ui::produce_page::ProduceState {
    let of = |row: &str| {
        ps::defaults()
            .iter()
            .find(|(name, _)| *name == row)
            .map(|(_, value)| value.to_string())
            .unwrap_or_default()
    };
    ui::produce_page::ProduceState {
        container: of("Container"),
        codec: of("Codec"),
        subtitles: of("Subtitles"),
        preset: of("Preset"),
        resolution: of("Resolution"),
        frame_rate: of("Frame rate"),
        audio: of("Audio"),
        clips: 7,
        seconds: 254.0,
        title: "a talk about pipes".to_string(),
        ..Default::default()
    }
}

/// Build a window sitting on the Produce tab, dropping the previous check's window first.
fn produce_page(app: &adw::Application) -> adw::ApplicationWindow {
    release_last_window();
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, Page::Produce.label());
    hold_last_window(window.clone());
    window.present();
    settle();
    window
}

/// §08-produce#1-screen — the tab is the page: fifteen widgets, the thirteen settings rows (twelve
/// while narration is off), webm forcing its codec and dropping the subtitle track, and the images row
/// answering an add.
fn produce_round(_app: &adw::Application) {
    // --- (a) the plate draws §A's fifteen ------------------------------------------
    let window = produce_page(_app);
    for name in WIDGETS {
        assert!(
            widget_in(&window, name).is_some(),
            "§A's widget {name} is missing from the Produce tab"
        );
    }
    // The picture is a plate with the page's own ground, so it is not a hole in the layout.
    let picture = widget_in(&window, "thumbnail-picture").expect("the thumbnail plate exists");
    assert!(picture.has_css_class("produce-plate"), "and wears the plate class");
    // §A: the written column has a 360 px minimum so the paned cannot starve it.
    let right = picture
        .parent()
        .and_then(|left| left.parent())
        .expect("the paned holds the two columns");
    assert_eq!(right.widget_name(), "GtkPaned", "the split is a paned");
    RAN_PLATE.store(true, Ordering::SeqCst);
    window.close();
    settle();

    // --- (b) the thirteen rows, twelve with narration off -------------------------
    let window = produce_page(_app);
    ui::produce_page::set_state(published());
    ui::produce_page::refresh(&window);
    let rows_with_narration = drawn_setting_rows(&window);
    assert!(
        rows_with_narration.contains(&"setting-game-audio".to_string()),
        "with narration on, the Game audio row is on the page"
    );
    assert_eq!(
        rows_with_narration.len(),
        13,
        "§A's thirteen rows, each found by its control name: {rows_with_narration:?}"
    );
    // Narration off drops the game-volume row and nothing else (§F4.8: the value keeps, so the row
    // returns where it was left).
    let mut off = published();
    off.narration_off = true;
    off.game_volume = 0.44;
    ui::produce_page::set_state(off);
    ui::produce_page::refresh(&window);
    let rows_off = drawn_setting_rows(&window);
    assert_eq!(rows_off.len(), 12, "narration off leaves twelve rows: {rows_off:?}");
    assert!(
        !rows_off.contains(&"setting-game-audio".to_string()),
        "and the row that is gone is the game-volume one"
    );
    // The value is untouched by hiding it, so un-hiding restores the slider where it was left.
    let held = ui::produce_page::read_state();
    assert!(
        (held.game_volume - 0.44).abs() < 1e-9,
        "hiding the row does not zero the number: it reads {}",
        held.game_volume
    );
    RAN_SETTINGS.store(true, Ordering::SeqCst);
    window.close();
    settle();

    // --- (c) webm forces vp9 and drops the subtitle track ------------------------
    let window = produce_page(_app);
    ui::produce_page::set_state(published());
    ui::produce_page::refresh(&window);
    let said = ui::produce_page::set_setting(&window, "Container", "webm");
    let after = ui::produce_page::read_state();
    assert_eq!(after.container, "webm", "the container took the choice");
    assert_eq!(
        after.codec, "vp9",
        "§A: webm forces vp9 -- the stated choice, not the silent one"
    );
    assert_eq!(
        after.subtitles,
        ps::SUBTITLE_CHOICES[2],
        "and a webm file carries no subtitle track, so \"track in file\" became none"
    );
    assert!(
        said.contains("forces"),
        "the forcing is SAID, not applied quietly: {said}"
    );
    // The widget shows the same answer the state holds: no divergence after a forcing rule.
    let codec_picker = widget_in(&window, "setting-codec")
        .and_then(|w| w.downcast::<gtk::DropDown>().ok())
        .expect("the codec row is a dropdown");
    let shown = ps::CODECS[codec_picker.selected() as usize].to_string();
    assert_eq!(shown, after.codec, "the picker shows what the state says");
    RAN_FORCED.store(true, Ordering::SeqCst);
    window.close();
    settle();

    // --- (d) the images row answers an add ----------------------------------------
    let window = produce_page(_app);
    // Empty state first: §A prints two sentences where the slots go, so the tab reads as empty rather
    // than broken.
    let mut state = published();
    state.frames = Vec::new();
    ui::produce_page::set_state(state);
    ui::produce_page::refresh(&window);
    assert!(
        widget_in(&window, "publish-images-empty").is_some(),
        "§A: an empty images row prints its note where the slots go"
    );
    click(&window, "add-image-button");
    let added = ui::produce_page::read_state();
    assert_eq!(
        added.frames.len(),
        1,
        "the page's own Add button put one slot on the row"
    );
    assert!(
        widget_in(&window, "image-slot-0").is_some(),
        "and the row drew it as slot 0"
    );
    assert!(
        widget_in(&window, "image-set-thumbnail-0").is_some(),
        "the base slot offers Set Thumbnail"
    );
    assert!(
        widget_in(&window, "image-make-base-0").is_none(),
        "and does not offer Make base on the picture that already is the base"
    );
    RAN_IMAGES.store(true, Ordering::SeqCst);
    window.close();
    settle();

    // --- (e) the Change… door: the slot keeps its place, only its picture moves -------------------
    // sec_08_produce_1_screen S1 — §1 item 2's Change… door on a live button.
    let window = produce_page(_app);
    let mut one = published();
    one.frames = vec!["project:produce/publish/keep-me.jpg".to_string()];
    ui::produce_page::set_state(one);
    ui::produce_page::refresh(&window);
    click(&window, "image-change-0");
    let after_change = ui::produce_page::read_state();
    assert_eq!(
        after_change.frames.len(),
        1,
        "Change… never adds or drops a slot -- it replaces one in place"
    );
    assert_ne!(
        after_change.frames[0], "project:produce/publish/keep-me.jpg",
        "and the picture in that slot is no longer the one that was there"
    );
    assert!(
        common::status_text(&window).contains("image changed"),
        "the page says which thing happened: {}",
        common::status_text(&window)
    );
    RAN_CHANGE.store(true, Ordering::SeqCst);
    window.close();
    settle();

    // --- (f) the settings wire: the rule answers the WIDGET, not a call from this test -----------
    // sec_08_produce_1_screen S2 — §1 items 11–14 reached through notify::selected / toggled.
    let window = produce_page(_app);
    ui::produce_page::set_state(published());
    ui::produce_page::refresh(&window);
    let container_picker = widget_in(&window, "setting-container")
        .and_then(|w| w.downcast::<gtk::DropDown>().ok())
        .expect("the Container row is a dropdown");
    // webm, chosen the way a person chooses it: by moving the picker, not by calling `set_setting`.
    container_picker.set_selected(2);
    settle();
    let forced = ui::produce_page::read_state();
    assert_eq!(
        forced.codec, "vp9",
        "webm's forcing arrived through notify::selected, with no set_setting call in this test"
    );
    assert_eq!(
        forced.subtitles, ps::SUBTITLE_CHOICES[2],
        "and the subtitle track went with it"
    );
    // A tick goes through `set_toggle` the same way.
    let blurred = widget_in(&window, "tick-blurred")
        .and_then(|w| w.downcast::<gtk::CheckButton>().ok())
        .expect("the Frame edges tick exists");
    blurred.set_active(false);
    settle();
    assert!(
        !ui::produce_page::read_state().blurred_edges,
        "unticking Frame edges reaches the state through the toggle signal"
    );
    // And the guard: put the value back IN THE STATE and refresh. The programmatic `set_active(true)`
    // emits its own toggled signal; had that reached `set_toggle` it would have written the value the
    // repaint just set, which is harmless here but proves nothing -- what matters is that the handler
    // does NOT answer the repaint as an edit at all. Read both sides after the refresh.
    let mut back = ui::produce_page::read_state();
    back.blurred_edges = true;
    ui::produce_page::set_state(back);
    ui::produce_page::refresh(&window);
    settle();
    assert!(
        ui::produce_page::read_state().blurred_edges,
        "the held value survives its own repaint"
    );
    assert!(
        widget_in(&window, "tick-blurred")
            .and_then(|w| w.downcast::<gtk::CheckButton>().ok())
            .map(|t| t.is_active())
            .unwrap_or(false),
        "and the widget shows it"
    );
    RAN_SETTINGS_WIRE.store(true, Ordering::SeqCst);
    window.close();
    settle();

    // --- (g) the ✎ chip and its dialog ---------------------------------------------------------
    // sec_08_produce_1_screen S1 item 7 — the thumbnail text-overlay editor's chip and dialog.
    let window = produce_page(_app);
    let mut marked = published();
    marked.texts = vec![naivepost::project::TextMark {
        cx: 0.5,
        cy: 0.25,
        wf: 1.0,
        hf: 0.4,
        text: "a title band".to_string(),
    }];
    ui::produce_page::set_state(marked);
    ui::produce_page::refresh(&window);
    assert!(
        widget_in(&window, "words-chip-0").is_some(),
        "§1 item 7: every word box on the picture gets its ✎ chip"
    );
    click(&window, "words-chip-0");
    // The dialog is its own top-level window rather than a child of the page, so it is found through the
    // module that opened it instead of by walking this window's content tree.
    let dialog = naivepost::ui::produce_words::open_dialog().expect("the chip opens the reword dialog");
    assert!(dialog.widget_name() == "words-dialog", "and it is the named dialog");
    let entry = dialog
        .extra_child()
        .and_then(|c| c.downcast::<gtk::Entry>().ok())
        .expect("the dialog has its entry");
    assert_eq!(
        entry.text().as_str(),
        "a title band",
        "opened PRE-FILLED: a reword dialog that starts blank loses the words it was opened to change"
    );
    entry.set_text("rewritten band");
    dialog.response("save");
    settle();
    assert_eq!(
        ui::produce_page::read_state().texts[0].text, "rewritten band",
        "Save went through press_words_reword"
    );
    // Re-open and remove. The dialog must come up filled with the NEW words, not the ones first seeded.
    click(&window, "words-chip-0");
    let again = naivepost::ui::produce_words::open_dialog().expect("the chip opens the dialog again");
    assert_eq!(
        again
            .extra_child()
            .and_then(|c| c.downcast::<gtk::Entry>().ok())
            .map(|e| e.text().to_string())
            .unwrap_or_default(),
        "rewritten band",
        "the second opening reads the live mark"
    );
    again.response("remove");
    settle();
    assert!(
        ui::produce_page::read_state().texts.is_empty(),
        "Remove went through press_words_remove"
    );
    assert!(
        widget_in(&window, "words-chip-0").is_none(),
        "and the chip is gone with it -- a leftover chip would claim a box that no longer exists"
    );
    RAN_WORDS.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

/// The setting rows the page actually drew, by their widget names.
#[allow(dead_code)]
fn screen_rows(window: &adw::ApplicationWindow) -> Vec<String> {
    let mut found = Vec::new();
    fn walk(node: &gtk::Widget, out: &mut Vec<String>) {
        let name = node.widget_name();
        if name.starts_with("setting-") {
            out.push(name.to_string());
        }
        if let Some(child) = node.first_child() {
            let mut cursor = Some(child);
            while let Some(current) = cursor {
                walk(&current, out);
                cursor = current.next_sibling();
            }
        }
    }
    if let Some(content) = window.content() {
        walk(&content, &mut found);
    }
    found
}

#[test]
fn sec_08_produce_1_screen_s1_s4_plate_rows_forcing_and_images_wire_the_named_widgets() {
    // Pin cwd to our own temp root BEFORE anything builds.
    let root = std::env::temp_dir().join(format!("np-produce-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temp root created");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");

    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(produce_round);
        app.run();
    });

    assert!(RAN_PLATE.load(Ordering::SeqCst), "the fifteen-widget check never ran");
    assert!(RAN_SETTINGS.load(Ordering::SeqCst), "the rows check never ran");
    assert!(RAN_FORCED.load(Ordering::SeqCst), "the forcing check never ran");
    assert!(RAN_IMAGES.load(Ordering::SeqCst), "the images-row check never ran");
    assert!(RAN_CHANGE.load(Ordering::SeqCst), "the Change… door check never ran");
    assert!(
        RAN_SETTINGS_WIRE.load(Ordering::SeqCst),
        "the settings-signal check never ran"
    );
    assert!(RAN_WORDS.load(Ordering::SeqCst), "the word-chip / dialog check never ran");
}
