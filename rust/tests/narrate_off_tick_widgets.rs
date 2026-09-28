//! F4.8 Narration off (spec/07-narrate.md) — the WIRE: a click on the real `narration-tick`
//! greys the three things `narrate_off::greyed(true)` names, writes its own flag into
//! `naivepost.json`, and leaves `narrate/narration.json` byte-for-byte as it was. Ticking it back
//! finds everything written (S5).
//!
//! `tests/narrate_off_page.rs` proves the rules headless; this file proves the widget reaches them.
//! Nothing here is driven through `ui::set_narration_off` — the box itself moves, and GTK emits
//! `toggled` from `set_active`, which is the same emission a user's click produces and the one
//! `narrate_page::wire()` listens for.

use std::sync::Once;

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::narrate_off;
use naivepost::narrate_screen;
use naivepost::narration::{self, Entry, Narration};
use naivepost::shell::Page;
use naivepost::ui;

#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::{fixture_dir, settle, status_text};

/// The three surfaces §1 says go grey, in the order `off_greys()` lists their names.
const GREYED_WIDGETS: [&str; 3] = ["narrate-lines", "narrate-preview", "voice-picker"];

fn widget_in(window: &adw::ApplicationWindow, name: &str) -> Option<gtk::Widget> {
    fn walk(node: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
        if node.widget_name() == name {
            return Some(node.clone());
        }
        if let Some(child) = node.first_child() {
            let mut cursor = Some(child);
            while let Some(current) = cursor {
                if let Some(found) = walk(&current, name) {
                    return Some(found);
                }
                cursor = current.next_sibling();
            }
        }
        None
    }
    let content = window.content()?;
    walk(&content, name)
}

/// A line to stage in the record, so "left exactly as it is" compares bytes that exist rather than an
/// absent file.
fn staged_entries() -> Vec<Entry> {
    vec![Entry {
        s: 0.0,
        e: 10.0,
        at: 2.0,
        text: "a line written before the tick went off".into(),
        ..Default::default()
    }]
}

static RUN: Once = Once::new();

#[test]
fn f4_8_s1_s2_s5_the_real_tick_greys_persists_and_leaves_the_record_alone() {
    // Pin cwd to our own temp root BEFORE anything builds: the page resolves the project through
    // `startup::session_dir(current_dir())`, and leaving it at rust/ writes a stray folder into the repo.
    let root = std::env::temp_dir().join(format!("np-narroff-wire-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("temp root created");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");

    RUN.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(run_body);
        app.run();
    });
}

fn run_body(app: &adw::Application) {
    let model = naivepost::project::load(&fixture_dir()).expect("the fixture loads");
    assert!(
        !model.no_narration,
        "the fixture must be a NARRATED project or the tick's resting state proves nothing"
    );
    let window = ui::build_window(app, &model, "Prepare");
    window.present();
    ui::tab_button(&window, Page::Narrate)
        .expect("the shell has a Narrate tab")
        .emit_by_name::<()>("clicked", &[]);
    settle();

    // Stage a record in THIS session's project folder, where the page's tree points.
    let session_root = naivepost::startup::session_dir(&std::env::current_dir().unwrap());
    let tree = naivepost::layout::Tree::new(&session_root).expect("the session folder is a project");
    let entries = staged_entries();
    narration::save(
        &Narration {
            entries: entries.clone(),
            silent: Vec::new(),
        },
        &tree,
    )
    .expect("record staged");
    let record_before = std::fs::read(tree.narration_json()).expect("record readable");

    let tick = widget_in(&window, "narration-tick")
        .and_then(|w| w.downcast::<gtk::CheckButton>().ok())
        .expect("the Narration tick is a check button");

    // (a) S1/S2: the widget's resting state comes from the project's flag, not from a literal.
    assert_eq!(
        tick.is_active(),
        narrate_off::tick_checked(false),
        "a narrated project opens with the tick CHECKED"
    );
    for name in GREYED_WIDGETS {
        assert!(
            widget_in(&window, name).unwrap().is_sensitive(),
            "{name} is live while the narration is on"
        );
    }

    // (b) Fire it through the widget. `set_active` moves the box AND emits `toggled`, the emission a
    // user click produces; `emit_by_name("toggled")` would run the handler without moving the box,
    // leaving the widget disagreeing with the state it just wrote.
    tick.set_active(false);
    settle();
    assert!(!tick.is_active(), "the box moved off");
    assert!(
        ui::read_state().narration_off,
        "the handler carried the box's state into the page"
    );

    // (c) The greys match `narrate_off::greyed(true)` field by field, and the tick stayed pressable.
    let greyed = narrate_off::greyed(true);
    assert!(
        greyed.lines && greyed.preview && greyed.voice,
        "all three of greyed()'s fields are set with narration off"
    );
    assert!(!greyed.tick, "the tick never greys -- it is the way back");
    for name in GREYED_WIDGETS {
        assert!(
            !widget_in(&window, name).unwrap().is_sensitive(),
            "{name} is insensitive with narration off, as greyed says"
        );
    }
    assert!(
        tick.is_sensitive(),
        "the tick stays live while all three others grey"
    );

    // (d) The status line the wire printed is the seam's own sentence, unchanged by going through GTK.
    assert_eq!(
        status_text(&window),
        format!(
            "narration off \u{2014} {} greyed",
            narrate_screen::off_greys().join(", ")
        ),
        "the widget's status equals the seam's wording"
    );

    // (e) S2: the record was never opened, while the flag DID land in the project file.
    assert!(
        !narrate_off::touches_the_record(),
        "the tick must never open the record"
    );
    assert_eq!(
        std::fs::read(tree.narration_json()).expect("still readable"),
        record_before,
        "narration.json left exactly as it was by the toggle"
    );
    let saved = std::fs::read_to_string(tree.dir().join("naivepost.json")).expect("project file reads");
    assert!(
        saved.contains("\"no_narration\": true"),
        "the tick wrote its own flag, so it survives a reload: {saved}"
    );
    assert_eq!(
        naivepost::project::load(tree.dir())
            .expect("the project reloads")
            .no_narration,
        true,
        "and a fresh read sees it"
    );

    // (f) S5: back on through the same widget, the surfaces return and the record still matches.
    tick.set_active(true);
    settle();
    assert!(tick.is_active(), "the box came back on");
    assert!(!ui::read_state().narration_off, "and the page came with it");
    for name in GREYED_WIDGETS {
        assert!(
            widget_in(&window, name).unwrap().is_sensitive(),
            "{name} is live again with the narration"
        );
    }
    assert_eq!(status_text(&window), "narration on", "and says so plainly");
    assert_eq!(
        std::fs::read(tree.narration_json()).expect("still readable"),
        record_before,
        "everything written is still there after the round trip"
    );
    assert_eq!(
        narration::load(&tree).unwrap().entries, entries,
        "the lines the tick passed over are the lines that were there"
    );

    window.close();
    settle();
}
