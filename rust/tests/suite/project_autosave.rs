//! §2's last sentence, on the program path: "the project is marshalled every 2 s, written only when
//! the bytes differ from the last write, and flushed on window close."
//!
//! The two halves are checked through the SAME entry points the program uses — `tick` is what
//! `project_autosave::start`'s timeout calls each interval, and the close flush is reached by
//! `window.close()`, the emission every widget round already makes. No test waits two seconds.

use crate::common;

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;

use common::{fixture_dir, settle, widget_in};
use naivepost::project::{self, Project};
use naivepost::project_autosave::{self as autosave, Tick};
use naivepost::{startup, ui};

/// Point the process at a throwaway root, the way every widget round does: the working copy every
/// flow writes into is `session.naivepost` beside the working root, so a tick that runs while cwd is
/// `rust/` would stamp the repository.
fn pin_root(tag: &str) -> PathBuf {
    let root =
        std::env::temp_dir().join(format!("naivepost-autosave-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("temp root");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");
    root
}

fn session_dir(root: &Path) -> PathBuf {
    startup::session_dir(root)
}

fn json_of(dir: &Path) -> Option<String> {
    std::fs::read_to_string(dir.join(project::PROJECT_FILE)).ok()
}

/// S5: the tick writes, and writes ONLY when the bytes differ.
///
/// The first tick after `install` must land: a freshly taken-on project has nothing on disk to
/// compare against, and a timer that skipped it would leave a session that never got its first write
/// unprotected until something was edited. The second must NOT: §2's "only when the bytes differ" is
/// the whole reason a 2 s tick is affordable, and it lives in `Autosave::last_written` — which is
/// exactly what a timer built from a fresh `Autosave` each tick would throw away.
#[test]
fn sec_01_project_and_files_2_naivepostjson_s5_a_tick_writes_then_writes_only_when_bytes_differ() {
    let root = pin_root("differ");
    let dir = session_dir(&root);
    let model = Project {
        policy: naivepost::project::Policy {
            language: naivepost::project::Field {
                value: "en".to_string(),
                origin: Default::default(),
                because: None,
            },
            ..Default::default()
        },
        ..Default::default()
    };
    let session: Rc<RefCell<Project>> = Rc::new(RefCell::new(model.clone()));
    let slot = autosave::slot_for_test(&session, &dir);

    assert_eq!(
        autosave::tick(&slot),
        Tick::Wrote,
        "the first tick after a project is taken on writes it"
    );
    let first = json_of(&dir).expect("the tick wrote naivepost.json");
    assert!(
        policy_language(&first) == "en",
        "what was written is the session, not a blank: {first}"
    );

    assert_eq!(
        autosave::tick(&slot),
        Tick::Unchanged,
        "an unchanged session costs no write (§2: only when the bytes differ)"
    );
    assert_eq!(
        json_of(&dir).as_deref(),
        Some(first.as_str()),
        "a refused tick leaves the file exactly as it was"
    );

    // Now the session changes the way a widget handler changes it, and the next tick owes it.
    session.borrow_mut().policy.language.value = "de".to_string();
    assert_eq!(
        autosave::tick(&slot),
        Tick::Wrote,
        "an edited session is written on the next tick"
    );
    assert!(
        json_of(&dir).is_some_and(|text| policy_language(&text) == "de"),
        "the second write carries the edit"
    );
    assert_eq!(
        autosave::tick(&slot),
        Tick::Unchanged,
        "and then settles again"
    );
}

/// S5 + F0.11 S1: a flow that wrote the session itself costs the next tick nothing.
///
/// `write_session_now` is the door a flow uses when it changed the session and wants it on disk now
/// (the rescan's pruned list). It writes through the newest slot's marker, so the timer finds the
/// bytes unchanged afterwards -- which is what keeps a second write from landing in the folder after
/// the flow already said it was done. With no window installed yet, the same door still writes: a
/// flow that has a folder has somewhere to put it.
#[test]
fn sec_01_project_and_files_2_naivepostjson_s5_a_flow_write_leaves_the_timer_nothing_to_do() {
    let root = pin_root("flow-write");
    let dir = session_dir(&root);
    let model = Project {
        policy: naivepost::project::Policy {
            language: naivepost::project::Field {
                value: "en".to_string(),
                origin: Default::default(),
                because: None,
            },
            ..Default::default()
        },
        ..Default::default()
    };
    let session: Rc<RefCell<Project>> = Rc::new(RefCell::new(model.clone()));
    // Installed, as a window installs it: the flow's door writes through the NEWEST slot, so a test of
    // that door has to put its session in one. `install` forgets the last write, which is what makes
    // the flow's own write the first thing that lands.
    autosave::install(Rc::clone(&session), dir.clone());
    let slot = autosave::newest_slot_for_test();

    assert!(
        autosave::write_session_now(&session.borrow(), &dir),
        "the flow's write lands"
    );
    let written = json_of(&dir).expect("naivepost.json written by the flow");
    assert!(
        policy_language(&written) == "en",
        "the flow wrote the session it was handed: {written}"
    );

    // The marker moved with the write, so the tick that follows owes nothing -- this is the whole point
    // of writing through the slot instead of beside it.
    assert_eq!(
        autosave::tick(&slot),
        Tick::Unchanged,
        "the timer does not repeat a write a flow already made"
    );

    // And the rule still works after a flow write: change the session, the next tick owes it again.
    session.borrow_mut().policy.language.value = "nl".to_string();
    assert_eq!(
        autosave::tick(&slot),
        Tick::Wrote,
        "a later edit is still written"
    );
}

/// S5: a tick with nothing installed writes nothing — a window that has not taken on a project has
/// no folder to write into, and inventing one is §1's business, not the timer's.
#[test]
fn sec_01_project_and_files_2_naivepostjson_s5_a_tick_with_no_project_installed_writes_nothing() {
    let root = pin_root("empty");
    let slot = Rc::new(RefCell::new(autosave::Ticker::default()));
    assert_eq!(autosave::tick(&slot), Tick::Nothing);
    assert!(
        json_of(&session_dir(&root)).is_none(),
        "nothing was created"
    );
}

/// S6: `window.close()` puts the session on disk, in the folder the working copy lives in, through
/// the close hook rather than a call a test makes directly.
///
/// The language is changed on the LIVE session handle — the one `build_window` published — after the
/// window exists, because that is the copy the close reads; changing the model handed to
/// `build_window` would only prove the argument was copied somewhere.
/// The one running GTK application this binary needs: the close check cannot build a window outside
/// a live main loop (GTK's own rule), so it runs inside `connect_activate` and records that it got
/// there — the arrangement `tests/suite/add_sources_widgets.rs` uses for the same reason.
static RAN_CLOSE: AtomicBool = AtomicBool::new(false);

#[test]
fn sec_01_project_and_files_2_naivepostjson_s6_closing_the_window_flushes_the_project() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(|app| {
            check_close_writes_the_session(app);
            RAN_CLOSE.store(true, Ordering::SeqCst);
            app.quit();
        });
        app.run_with_args::<String>(&[]);
    });
    assert!(
        RAN_CLOSE.load(Ordering::SeqCst),
        "the close check never ran"
    );
}

fn check_close_writes_the_session(app: &adw::Application) {
    let root = pin_root("close");
    let dir = session_dir(&root);
    let mut model = project::load(&fixture_dir()).expect("fixture loads");
    model.policy.language.value = "en".to_string();

    let window = ui::build_window(app, &model, "Prepare");
    // Read before the loop turns: the 2 s autosave writes on its own beat, so only this moment shows
    // that the build itself saved nothing.
    assert!(
        json_of(&dir).is_none(),
        "nothing has written the working copy yet: the flush, not the build, is what saves"
    );
    window.present();
    settle();
    assert!(
        widget_in(&window, "save-button").is_some(),
        "the window is the built shell, so its close hook is the real one"
    );

    // An edit the live session holds and the model the window was built with does not: the User Context.
    ui::window::set_session_context(&window, "recorded in German");
    settle();

    window.close();
    settle();
    let text = json_of(&dir).expect("closing wrote naivepost.json");
    assert!(
        text.contains("recorded in German"),
        "the flush wrote the LIVE session, not the model the window was built with: {text}"
    );
}

/// S6: the close is a flush, not another tick. A session whose bytes already match the file is
/// written again on the way out — §2 names the close as the safety net, so nothing about the
/// rate-limit rule may skip it.
#[test]
fn sec_01_project_and_files_2_naivepostjson_s6_the_close_flush_is_not_skippable() {
    let root = pin_root("flush");
    let dir = session_dir(&root);
    std::fs::create_dir_all(&dir).expect("session folder");
    let model = Project {
        policy: naivepost::project::Policy {
            language: naivepost::project::Field {
                value: "pt".to_string(),
                origin: Default::default(),
                because: None,
            },
            ..Default::default()
        },
        ..Default::default()
    };
    project::save(&model, &dir).expect("a file that already matches the session");

    // A `Ticker` whose `last_written` IS the file's bytes would answer `Unchanged` and write nothing;
    // the flush goes around it, and still says the file landed.
    let session: Rc<RefCell<Project>> = Rc::new(RefCell::new(model.clone()));
    let slot = autosave::slot_for_test(&session, &dir);
    assert_eq!(
        autosave::tick(&slot),
        Tick::Wrote,
        "install forgets the last write"
    );
    assert_eq!(
        autosave::tick(&slot),
        Tick::Unchanged,
        "so the rule itself is the one being tested"
    );
    assert!(
        autosave::flush_on_close(&model, &dir),
        "the close writes whatever the tick would have refused"
    );
}

/// S5's interval is §2's 2 s, and it is one number the timer and a test both read.
#[test]
fn sec_01_project_and_files_2_naivepostjson_s5_the_interval_is_two_seconds() {
    assert_eq!(
        project::AUTOSAVE_INTERVAL,
        std::time::Duration::from_secs(2),
        "P.window's autosave 2 s (10 §3)"
    );
}

/// The policy's language as the written `naivepost.json` holds it (`policy.language.value`).
fn policy_language(text: &str) -> String {
    serde_json::from_str::<serde_json::Value>(text).unwrap_or_default()["policy"]["language"]
        ["value"]
        .as_str()
        .unwrap_or_default()
        .to_string()
}
