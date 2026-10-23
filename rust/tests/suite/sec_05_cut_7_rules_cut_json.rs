//! §05-cut#7-rules bullet 7 — `cut.json is written by ONE function`, from every edit: the WIRE.
//!
//! `rust/tests/suite/cut_rules.rs` proves the sentences inside [`naivepost::cut_rules`]; this file proves the file
//! on disk follows the page. Three checks:
//!
//! * **s7_a** — a － Remove through the real widget leaves a `cut.json` whose `segs` are the ones on screen,
//!   read back with [`naivepost::cut::load`] rather than trusted from memory.
//! * **s7_b** — a trim's three drag updates plus the release leave the RELEASED edge in the file: the mid-drag
//!   writes never reach disk (that is why §7 names one writer), the gesture end does.
//! * **s7_c** — [`naivepost::cut_save::exists`] answers false before the first edit and true after, which is the
//!   gate §7 says unlocks Narrate and Produce.
//!
//! Everything runs in ONE application round against a temp `.naivepost` folder pinned as the process working
//! directory, because `startup::session_dir(current_dir())` is where every write lands (§1's working copy).

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use gtk4::gio;
use naivepost::cut::{self, Cut, Seg};
use naivepost::cut_select::Surface;
use naivepost::{cut_save, layout, ui};

use crate::common;
use common::{cut_page, release_last_window, settle};

static RAN_REMOVE: AtomicBool = AtomicBool::new(false);
static RAN_TRIM: AtomicBool = AtomicBool::new(false);
static RAN_EXISTS: AtomicBool = AtomicBool::new(false);
static RAN_NOT_A_SEAM: AtomicBool = AtomicBool::new(false);

/// A footage scene from `s` to `e` on camera row `cam`.
fn seg(s: f64, e: f64) -> Seg {
    Seg {
        s,
        e,
        cam: 0,
        ..Default::default()
    }
}

/// Three kept runs of ten seconds each, so a Remove and a trim each have a neighbour to be measured against.
fn three_clips() -> Cut {
    Cut {
        segs: vec![seg(0.0, 10.0), seg(10.0, 20.0), seg(20.0, 30.0)],
        ..Default::default()
    }
}

/// The session folder every write in this run goes to: `session_dir(current_dir())`, §1's working copy.
fn tree() -> layout::Tree {
    let root = std::env::current_dir().expect("the process has a working directory");
    layout::Tree::new(naivepost::startup::session_dir(&root))
        .expect("the working copy is a .naivepost folder")
}

/// Remove the 20–30 clip with the REAL widget, the same door a click uses.
fn fire_remove_over(window: &adw::ApplicationWindow, from: f64, to: f64) {
    ui::draw_selection(window, Surface::PictureRow(0), None, from, to)
        .expect("a drag on a picture row selects that row's footage");
    let button: gtk::Button = ui::line_step_button(window, "remove-button")
        .expect("－ Remove is on the Cut toolbar (cut_verbs::BUTTONS)");
    button.emit_by_name::<()>("clicked", &[]);
    settle();
}

fn round(app: &adw::Application) {
    // --- (a) a Remove's file matches the screen ---------------------------------------------------------
    {
        let window = cut_page(app, &three_clips());
        fire_remove_over(&window, 20.0, 30.0);

        let on_screen = ui::review_cut_of(&window);
        assert_eq!(
            on_screen.segs.len(),
            2,
            "－ Remove took the 20–30 clip out of the cut on screen: {:?}",
            on_screen.segs
        );
        let on_disk = cut::load(&tree()).expect("the Remove wrote a cut.json");
        assert_eq!(
            on_disk.segs, on_screen.segs,
            "the file holds the SAME segments the page shows — a write that lags the screen is an edit lost \
             on reopen: disk={:?} screen={:?}",
            on_disk.segs, on_screen.segs
        );
        // And `exists` says the file is there now, from the same resolution the writer used.
        assert!(
            cut_save::exists(),
            "the gate is the FILE: an edit that wrote nothing would leave Narrate and Produce greyed forever"
        );
        RAN_REMOVE.store(true, Ordering::Relaxed);
        release_last_window();
    }

    // --- (b) a trim writes its released edge, not a mid-drag one --------------------------------------
    {
        // SAME session as (a): the file is REMOVED rather than sought in a second temp folder, because
        // `cut_page` loads its project from the working directory and moving cwd mid-round makes the next window
        // build against a project with no footage (which aborts inside `app.run()`).
        std::fs::remove_file(tree().cut_json()).expect("(a) left a cut.json to remove");
        let window = cut_page(app, &three_clips());
        // Plant a file by hand so "the file did not change" is a checkable fact mid-drag: §7's rule is that a
        // tick never reaches disk, which only shows up against something already there to leave alone.
        crate::suite::sec_05_cut_7_rules_cut_json::cut_save::write_cut(&three_clips())
            .expect("a baseline cut.json planted through the one writer");
        let planted = std::fs::read(tree().cut_json()).expect("the planted file is readable");
        // The seam pair the strip's own gesture calls: `drag-begin` runs `begin_move`, which opens §7's write
        // gate; each update runs the same door `drag-update` runs; `end_move` is what `drag-end` runs, and it
        // closes the gate and writes. Driving the seams (rather than emitting GTK signals, which cannot arm the
        // trim hand without the strip's real allocation) keeps this deterministic.
        naivepost::ui::cut_trim_press::begin_move();
        assert!(
            cut_save::gesture_in_flight(),
            "`begin_move` is what the strip's drag-begin calls, and opening §7's write gate is its job"
        );
        for to in [9.0, 8.0, 7.0] {
            let landed = ui::press_trim_border(&window, 0, naivepost::cut_trim::Border::End, to);
            let edge = ui::review_cut_of(&window).segs[0].e;
            // The file must be EXACTLY what it was before the tick — byte for byte — which is a stronger and
            // more honest statement than "no file": §7's rule is that a mid-drag update never reaches disk.
            let bytes = std::fs::read(tree().cut_json()).unwrap_or_default();
            assert_eq!(
                bytes, planted,
                "§7's `cut.json is written by one function`: a mid-drag tick moves the page (edge={edge}) but \
                 leaves the file exactly as it found it"
            );
            assert!(
                (landed - to).abs() < 1e-6 || (edge - to).abs() < 1e-6,
                "the tick reports where it landed: asked {to}, landed {landed}, on screen {edge}"
            );
        }
        naivepost::ui::cut_trim_press::end_move(&window);
        settle();
        let on_disk = cut::load(&tree()).expect("the release wrote cut.json");
        assert!(
                !cut_save::gesture_in_flight(),
                "the release closed §7's write gate — an open gate would leave every later edit unsaved"
            );
        assert!(
            on_disk.segs != three_clips().segs,
            "the release DID write: the file moved off what it held before the drag — {:?}",
            on_disk.segs[0]
        );
        assert!(
                on_disk.segs[0].e <= 9.5,
                "the FILE holds an edge the drag produced ({:?}), not the untouched value it started at",
                on_disk.segs[0]
            );
        RAN_TRIM.store(true, Ordering::Relaxed);
        release_last_window();
    }

    // --- (c) the gate: false before an edit, true after ---------------------------------------------------
    {
        // A cut already on disk is overwritten by the edit; what §7 gates on is the file BEING there, so this
        // check asks the question at the moment the file is absent — planted empty of cuts is not possible (no
        // tree without a folder), so the absence is taken from a folder that has never been written: the
        // session's own `cut/` is removed here, which is safe because nothing but this check reads it.
        let cut_dir = tree()
            .cut_json()
            .parent()
            .expect("cut/ has a parent")
            .to_path_buf();
        std::fs::remove_dir_all(&cut_dir).expect("the cut folder is this run's to remove");
        let untouched = cut_save::exists();
        let window = cut_page(app, &three_clips());
        fire_remove_over(&window, 10.0, 20.0);
        let after = cut_save::exists();
        assert!(
                !untouched && after,
                "Narrate and Produce grey from the file being there: false in a session nobody has cut yet, \
                 true as soon as one edit has been written (untouched={untouched} after={after})"
            );
        RAN_EXISTS.store(true, Ordering::Relaxed);
    }

    // --- (d) the writer resolves the session itself; it is not a seam a check fills -------------------
    {
        let before_cwd = std::env::current_dir().expect("cwd readable");
        crate::suite::sec_05_cut_7_rules_cut_json::cut_save::write_cut(&three_clips())
            .expect("the one writer wrote");
        let written_then_exists = tree().cut_json().is_file() && cut_save::exists();
        std::fs::remove_dir_all(tree().cut_json().parent().expect("cut/ has a parent"))
            .expect("cut/ removed");
        let gone = cut_save::exists();
        let after_cwd = std::env::current_dir().expect("cwd readable again");
        assert!(
                written_then_exists && !gone,
                "§7's writer answers from the program's OWN resolution: write_cut put the file where `exists` \
                 looks ({written_then_exists}) and taking the folder away makes the gate say no again ({gone})"
            );
        assert_eq!(
            before_cwd, after_cwd,
            "neither call moved the working directory — nothing was handed in from outside"
        );
        RAN_NOT_A_SEAM.store(true, Ordering::Relaxed);
    }
}

/// The test's own name carries §7 bullet 7 and its `s7` case; the checks inside are labelled (a)-(d).
#[test]
fn sec_05_cut_7_rules_s7_cut_json_written_by_one_function() {
    // Pin cwd BEFORE anything builds: `startup::session_dir(current_dir())` is where every write lands, and
    // leaving it at rust/ writes a stray `rust/session.naivepost/` into the repo.
    let root = std::env::temp_dir().join(format!("np-cut-json-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temp root created");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");

    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(|app| {
            round(app);
            // The round must END the loop it ran in: `app.run()` otherwise waits for a window that was already
            // released, and the test hangs (this file's earlier 10-minute stall).
            app.quit();
        });
        app.run_with_args::<String>(&[]);
    });
    settle();

    assert!(
        RAN_REMOVE.load(Ordering::Relaxed)
            && RAN_TRIM.load(Ordering::Relaxed)
            && RAN_EXISTS.load(Ordering::Relaxed)
            && RAN_NOT_A_SEAM.load(Ordering::Relaxed),
        "all four checks run in one round: remove={:?} trim={:?} exists={:?} not_a_seam={:?}",
        RAN_REMOVE.load(Ordering::Relaxed),
        RAN_TRIM.load(Ordering::Relaxed),
        RAN_EXISTS.load(Ordering::Relaxed),
        RAN_NOT_A_SEAM.load(Ordering::Relaxed)
    );
}
