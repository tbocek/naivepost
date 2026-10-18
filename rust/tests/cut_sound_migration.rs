//! §3's last sentence about `cut.json`: "Legacy `sound` (whole-cut hearing) is migrated into per-scene
//! `quiet` on first save." The rule itself is `Cut::migrate_sound`; what this file proves is that the
//! ONE writer runs it — reading an old file changes nothing, saving one folds the whole-cut hearing into
//! the scenes and says so — including through a real fold press.

use std::path::PathBuf;

use naivepost::cut::{self};use naivepost::layout::Tree;

#[allow(dead_code)] // every test binary compiles this whole module
mod common;
use common::{fixture_dir, hold_last_window, release_last_window, settle, temp_dir};

/// A project folder holding the given `cut.json` text (or no file).
fn with_file(tag: &str, text: Option<&str>) -> (PathBuf, Tree) {
    let root = temp_dir(tag);
    let dir = root.join("demo.naivepost");
    std::fs::create_dir_all(dir.join("cut")).unwrap();
    if let Some(text) = text {
        std::fs::write(dir.join("cut/cut.json"), text).unwrap();
    }
    (root, Tree::new(&dir).unwrap())
}

fn read(t: &Tree) -> String {
    std::fs::read_to_string(t.cut_json()).unwrap()
}

/// The legacy file's text: a cut heard on `mic` end to end, written as bytes because only bytes can
/// hold an unmigrated file — `cut::save` now migrates on the way out (§3), which is what S1 proves.
const LEGACY: &str = r#"{"segs":[{"s":0,"e":10},{"s":10,"e":20,"quiet":["cam-2"]}],
 "lanes":[{"name":"mic","src":"project:sources/mic.mkv","at":0.0,"dur":120.0},
          {"name":"cam-2","src":"project:sources/cam.mkv","at":0.0,"dur":120.0}],
 "sound":"mic"}"#;

#[test]
fn sec_01_project_and_files_3_cutcutjson_s1_a_save_migrates_the_legacy_sound_key() {
    let (_root, tree) = with_file("sound-save", Some(LEGACY));
    let loaded = cut::load(&tree).expect("the legacy file reads");
    // Reading is lossless: `load` migrates the `fx` spellings only, so there IS something for a save
    // to migrate, which is what makes "on first save" a statement rather than a no-op.
    assert_eq!(loaded.sound.as_deref(), Some("mic"), "reading left the key alone");

    cut::save(&loaded, &tree).expect("the one writer");
    let text = read(&tree);
    assert!(!text.contains("\"sound\""), "the legacy key is gone for good: {text}");
    let after = cut::load(&tree).expect("and the file reads back");
    assert_eq!(after.segs[0].quiet, ["cam-2".to_string()], "the silent scene was set by the migration");
    assert_eq!(
        after.segs[1].quiet,
        ["cam-2".to_string()],
        "a scene that already said for itself was not overwritten"
    );
    assert_eq!(after.sound, None);

    // Nothing left to do: a second save writes the same bytes.
    let first = text.clone();
    cut::save(&after, &tree).expect("and saves again");
    assert_eq!(read(&tree), first, "a second save changes nothing");

    // And the caller's cut is untouched: the migration belongs to the file, not to the page that asked
    // for a write.
    assert_eq!(loaded.sound.as_deref(), Some("mic"), "the in-memory cut kept what it was handed");
}

#[test]
fn sec_01_project_and_files_3_cutcutjson_s2_the_migration_is_said_on_the_log() {
    let (_root, tree) = with_file("sound-log", Some(LEGACY));
    let before = naivepost::ui::window_logs().len();
    let loaded = cut::load(&tree).unwrap();
    cut::save(&loaded, &tree).unwrap();
    let lines = naivepost::ui::window_logs();
    assert_eq!(lines.len(), before + 1, "one line for one migration");
    assert!(
        lines.last().is_some_and(|line| line.contains(
            "this cut was heard on mic from end to end; that is now said scene by scene, \
             and every scene has been set to silence the other lanes"
        )),
        "{:?}",
        lines.last()
    );

    // A cut with nothing to migrate says nothing: the log is not a heartbeat.
    let quiet = cut::load(&tree).unwrap();
    cut::save(&quiet, &tree).unwrap();
    assert_eq!(naivepost::ui::window_logs().len(), before + 1, "no second note");
}

/// The wire: the Cut page's gutter badge writes the file, so it is the press that must migrate.
#[test]
fn sec_01_project_and_files_3_cutcutjson_s3_a_fold_press_writes_the_migrated_cut() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        use adw::prelude::*;
        
        let app = adw::Application::builder()
            .application_id(naivepost::ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(|app| {
            press_folds_the_file(app);
            QUIT.with(|q| q.set(true));
            app.quit();
        });
        app.run_with_args::<String>(&[]);
    });
    assert!(QUIT.get(), "the press check never ran");
}

thread_local! {
    static QUIT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn press_folds_the_file(app: &adw::Application) {
    use adw::prelude::*;
    
    use naivepost::shell::Page;
    use naivepost::ui;

    // The presses resolve the project through `startup::session_dir(current_dir())`, so cwd is pinned
    // to our own root: it is both where the session lives and where `save_folds` writes.
    let root = temp_dir("sound-wire");
    std::fs::create_dir_all(&root).expect("temp root");
    std::env::set_current_dir(&root).expect("cwd pinned");
    let session = naivepost::startup::session_dir(&root);
    std::fs::create_dir_all(session.join("cut")).expect("cut folder");
    let tree = Tree::new(&session).expect("a .naivepost folder is a project");

    // The legacy file, in the session the page will save over: two kept clips out of a filmed 0–120
    // tape, heard on the desk mic from end to end. Planted as BYTES — `cut::save` would migrate it at
    // once, and this check is about what a press does to a file an old build left behind.
    std::fs::write(
        tree.cut_json(),
        r#"{"segs":[{"s":10,"e":30},{"s":40,"e":60}],
 "lanes":[{"name":"mic","src":"project:sources/mic.mkv","at":0.0,"dur":120.0}],
 "sound":"mic"}"#,
    )
    .expect("the legacy file is on disk");
    let seeded = cut::load(&tree).expect("and it reads as a legacy cut");
    assert_eq!(seeded.sound.as_deref(), Some("mic"));

    release_last_window();
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    hold_last_window(window.clone());
    window.present();
    ui::tab_button(&window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_clicked();
    ui::seed_review_cut(&window, &seeded);
    // The session's tape: a dropped stretch only exists inside a run longer than the clips kept in it,
    // so without this there are no gaps and the gutter badge has nothing to fold (same fixture
    // `tests/fold_rows_widgets.rs` builds).
    ui::set_session_recordings(&[naivepost::timeline::Recording {
        base: "session-tape".to_string(),
        start: 0.0,
        end: 120.0,
    }]);
    ui::refresh_fold_badges(&window);
    settle();

    let holder = ui::fold_badges_box(&window).expect("the Cut page drew its fold-badges holder");
    let all = find_button(holder.upcast_ref(), "fold-all-button").expect("the gutter badge is there");
    all.emit_by_name::<()>("clicked", &[]);
    settle();

    let text = read(&tree);
    assert!(text.contains("\"folds\""), "the press saved its fold: {text}");
    assert!(!text.contains("\"sound\""), "and the same write migrated the legacy key: {text}");
    let after = cut::load(&tree).expect("and it reads back");
    // The one lane this file names is `mic`, which is what the whole cut was heard on, so nothing else
    // is left to silence — the migration's job here is that the KEY went through the press's own write.
    assert!(after.sound.is_none(), "the loaded file holds no legacy key");
    assert!(
        common::last_log().is_some_and(|line| line.contains("this cut was heard on mic")),
        "and the log said it: {:?}",
        common::last_log()
    );
    window.close();
}

/// Walk a holder's subtree for a named button — the badges are packed inside the holder, not on it.
fn find_button(node: &gtk4::Widget, name: &str) -> Option<gtk4::Button> {
    use gtk4::prelude::*;
    if node.widget_name() == name {
        return node.clone().downcast::<gtk4::Button>().ok();
    }
    for child in node.observe_children().iter::<glib::Object>() {
        let Ok(child) = child else { continue };
        let Ok(widget) = child.downcast::<gtk4::Widget>() else { continue };
        if let Some(found) = find_button(&widget, name) {
            return Some(found);
        }
    }
    None
}
