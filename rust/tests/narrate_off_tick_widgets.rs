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

/// One `adw::Application` and one main loop for the whole binary, as every widget test here has to be:
/// a second `run()` in a second test aborts with "cannot acquire the default main context", so both
/// scenarios ride this single activation, each building its own window and closing it before the next.
static RUN: Once = Once::new();

/// Set by each block at its own last assertion, so a scenario that never got there is a failure rather
/// than a silent pass next to the other test's success.
static STOP_RAN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static TICK_RAN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// ONE `adw::Application`, one main loop, both scenarios walked inside it. libtest runs these two tests
/// on separate threads of one process, and `Once::call_once` is what joins them: whichever thread gets
/// there first runs the loop, and the other blocks in `call_once` until that loop has finished — so after
/// either call returns, BOTH blocks have run and their flags can be read. A second
/// `adw::Application::run` in the same process aborts on the already-acquired default main context, which
/// is why neither test may start its own.
fn shared_loop() {
    RUN.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(run_body);
        // Our own argv: handing cargo-test's flags to libgio aborts before activate runs.
        app.run_with_args(&["naivepost"]);
    });
}

#[test]
fn f4_8_s1_a_running_preview_stops_with_the_narration() {
    shared_loop();
    assert!(
        STOP_RAN.load(std::sync::atomic::Ordering::SeqCst),
        "the running-preview block never ran"
    );
}

#[test]
fn f4_8_s1_s2_s5_the_real_tick_greys_persists_and_leaves_the_record_alone() {
    shared_loop();
    assert!(
        TICK_RAN.load(std::sync::atomic::Ordering::SeqCst),
        "the tick round-trip block never ran"
    );
}

fn run_body(app: &adw::Application) {
    stop_body(app);
    tick_round_trip(app);
    app.quit();
}

/// Greying a panel whose sound is still coming out of the speakers would leave the app playing a
/// narration the tick says there is none, so the tick's door stops the players as well as setting
/// sensitivity. Nothing here spawns a child: with no preview in the leg's slot `stop_running` answers
/// cleanly, which keeps the assertion about the PAGE's flag rather than about a process.
fn stop_body(app: &adw::Application) {
    // Its own project folder, written BEFORE the window is built: the page resolves its session through
    // `startup::session_dir(current_dir())`, and the other scenario reuses this process's cwd.
    let root = std::env::temp_dir().join(format!("np-narroff-stop-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("temp root created");
    std::env::set_current_dir(&root).expect("cwd pinned to the stop root");
    naivepost::project::save(
        &naivepost::project::Project { no_narration: false, ..Default::default() },
        &root.join("demo.naivepost"),
    )
    .expect("a narrated project to start from");
    let model = naivepost::project::load(&fixture_dir()).expect("the fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();
    ui::tab_button(&window, Page::Narrate)
        .expect("the shell has a Narrate tab")
        .emit_by_name::<()>("clicked", &[]);
    settle();

    // Stage the state the transport leaves behind while a preview runs: the same published copy the
    // page reads, not a poke at a private field. The narration is ON here and goes off below, so the
    // greys the earlier block left on these widgets are cleared before the check starts.
    let mut running = ui::read_state();
    running.playing = true;
    running.narration_off = false;
    ui::set_state(running);
    ui::refresh(&window);
    settle();
    assert!(ui::read_state().playing, "the preview is running before the tick moves");
    for name in GREYED_WIDGETS {
        assert!(
            widget_in(&window, name).unwrap().is_sensitive(),
            "{name} is live again with the narration back on"
        );
    }

    let session_root = naivepost::startup::session_dir(&std::env::current_dir().unwrap());
    let tree = naivepost::layout::Tree::new(&session_root).expect("the session folder is a project");
    narration::save(
        &Narration { entries: staged_entries(), silent: Vec::new() },
        &tree,
    )
    .expect("record staged");
    let record_before = std::fs::read(tree.narration_json()).expect("record readable");

    let tick = widget_in(&window, "narration-tick")
        .and_then(|w| w.downcast::<gtk::CheckButton>().ok())
        .expect("the Narration tick is a check button");
    // The emission a click produces: `set_active` moves the box AND fires `toggled`. The page's state is
    // process-wide, so the round-trip block may have left the tick OFF; bring it back on through the same
    // door before moving it off again, or this press would be a no-op.
    if !tick.is_active() {
        tick.set_active(true);
        settle();
    }
    // The project file now says narrated (this scenario's own root), so the seeded record and the flag
    // agree — which is what the status sentence below is about.
    tick.set_active(false);
    settle();

    assert!(!ui::read_state().playing, "the running preview stopped with the narration");
    assert!(ui::read_state().narration_off, "and the flag went off with it");
    assert!(tick.is_sensitive(), "the tick stayed pressable \u{2014} it is the way back");
    assert_eq!(
        status_text(&window),
        format!("narration off \u{2014} {} greyed", narrate_screen::off_greys().join(", ")),
        "stopping the preview did not change what the tick says"
    );
    assert_eq!(
        std::fs::read(tree.narration_json()).expect("still readable"),
        record_before,
        "stopping the players still never opens the record"
    );

    window.close();
    settle();
    // Publish the OFF state this block ends in: the page keeps its own copy process-wide, and the next
    // scenario builds a fresh window over it.
    ui::set_state({
        let mut after = ui::read_state();
        after.narration_off = true;
        after.playing = false;
        after
    });
    STOP_RAN.store(true, std::sync::atomic::Ordering::SeqCst);
}

fn tick_round_trip(app: &adw::Application) {
    // The cwd and the project folder are pinned BEFORE the window is built: the page resolves its session
    // through `startup::session_dir(current_dir())`, and the scenario that ran first left the cwd on a
    // folder it had already deleted (with `no_narration: true` written into that project file).
    let root = std::env::temp_dir().join(format!("np-narroff-wire-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("temp root created");
    std::env::set_current_dir(&root).expect("cwd pinned to the round-trip root");
    let model = naivepost::project::load(&fixture_dir()).expect("the fixture loads");
    naivepost::project::save(&model, &root.join("demo.naivepost"))
        .expect("the narrated fixture project, in this scenario's own folder");
    // The previous scenario left the published state with the narration off; this block starts from an ON
    // project, so it puts the page back to what a narrated session opens with before building the window.
    ui::set_state({
        let mut fresh = ui::read_state();
        fresh.narration_off = false;
        fresh
    });
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

    ui::set_narration_off(&window, false);
    settle();
    // (a) S1/S2: the widget's resting state comes from the project's flag, not from a literal. The
    // running-preview block may have left the published state with the narration OFF, so this block puts
    // it back on through the page's own door and clears the status line before checking what an ON page
    // looks like — (d) below reads that line, and it must see only what (b)'s click prints.
    assert_eq!(
        tick.is_active(),
        narrate_off::tick_checked(false),
        "the tick is CHECKED with the narration on"
    );
    // (d) reads this line, so it must show only what (b)'s click prints: clear it before firing.
    ui::find_status(window.upcast_ref()).expect("the shell has a status line").set_text("");
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
    assert!(
        naivepost::project::load(tree.dir())
            .expect("the project reloads")
            .no_narration,
        "and a fresh read sees it"
    );

    // (f) S5: back on through the same widget, the surfaces return and the record still matches. The box
    // is already checked from the reset in (a), so `set_active` would emit nothing; move it off first so
    // this is a real emission of the handler, exactly as a user's second click would be.
    tick.set_active(false);
    settle();
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
    TICK_RAN.store(true, std::sync::atomic::Ordering::SeqCst);
}
