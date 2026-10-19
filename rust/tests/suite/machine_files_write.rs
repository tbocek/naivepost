//! §01-project-and-files#7-machine-files — the WRITE side: the doors that make the machine's own
//! files get written by the program, not only by a test.
//!
//! [`naivepost::settings`] has always known how to write `llm.conf`, a hang dump, the desktop entry
//! and the MIME package; what it never had was a caller. These tests therefore assert on bytes, modes
//! and log lines produced by [`naivepost::machine_store`] through the same doors `main`, Open a
//! project, the Settings dialog and F6.3's stall watch walk through — never on a re-serialisation of
//! the format here in the test.
//!
//! Spec: spec/01-project-and-files.md §7 (and §5's settings dialog for the 600 ms beat), with the
//! modes of §1. Cited settings keys: PROJECT_<n>_ROOT, PROJECT_<n>_FILE, LLM_MODEL.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use adw::prelude::*;
use naivepost::checks;
use naivepost::icons::Databases;
use naivepost::machine_store;
use naivepost::settings::{self, Paths};
use naivepost::ui;
use naivepost::ui::settings as ui_settings;
use crate::common;
use common::{fixture_dir, settle};

/// A throwaway config/data pair standing in for `$XDG_CONFIG_HOME` and `$XDG_DATA_HOME`, shaped the
/// way `tests/suite/machine_files.rs` shapes its own so no test reads or writes the real `$HOME`.
struct Dirs {
    root: PathBuf,
    paths: Paths,
}

impl Dirs {
    fn new(label: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "np-machw-{label}-{pid}-{n}",
            pid = std::process::id(),
            n = COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&root);
        let paths = Paths {
            config_dir: root.join("cfg/naivepost"),
            data_dir: root.join("data/naivepost"),
        };
        Self { root, paths }
    }
}

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// One running GTK application for the widget check, in the shape `tests/suite/settings_widgets.rs` uses:
/// GTK's main loop may only be started by the thread that runs it, so the checks share one round.
fn window_round() {
    common::one_app_round(|app| {
        check_dialog_saves(app);
        RAN_DIALOG_SAVED.store(true, Ordering::SeqCst);
    });
}

/// The dialog's own answer provider: nothing is probed here, only saved.
fn echoing() -> ui_settings::Provider {
    std::rc::Rc::new(|_key, typed| Ok(format!("{typed} answered")))
}

/// Open the dialog over `window` answering from `provider` (same trick as `tests/suite/settings_widgets.rs`:
/// the gear's own unwired dialog stays below this one on the stack, and accessors take the last).
fn open_canned(window: &adw::ApplicationWindow, provider: ui_settings::Provider) {
    ui::open_settings(window, provider);
    assert!(ui::dialog_open(window), "the settings dialog is open");
}

fn mode(path: &Path) -> u32 {
    fs::metadata(path).expect("the file is there").permissions().mode() & 0o777
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_default()
}

/// S1: the launch persists what the legacy `settings.json` remembered, and asks about it only once.
#[test]
fn sec_01_project_and_files_7_machine_files_s1_the_launch_stores_the_recovered_projects_in_the_conf() {
    common::pin_private_cwd();
    let d = Dirs::new("recover");
    // Only the legacy file exists: this is the launch right after the merge into llm.conf.
    fs::create_dir_all(&d.paths.config_dir).unwrap();
    fs::write(
        d.paths.settings_path(),
        r#"{"projects":{"/sessions/lecture":"/sessions/lecture/naivepost.json"}}"#,
    )
    .unwrap();
    assert!(!d.paths.conf_path().exists(), "no conf yet");

    let conf = machine_store::open_and_recover(&d.paths, &d.root).expect("the launch reads its settings");
    assert_eq!(
        conf.projects.get("/sessions/lecture").map(String::as_str),
        Some("/sessions/lecture/naivepost.json"),
        "the recovered pair reached the store: {:?}",
        conf.projects
    );

    // §7's state lives in the conf now: 0600 in a 0700 folder, numbered by folder order.
    let conf_path = d.paths.conf_path();
    assert!(conf_path.exists(), "the recovery was written out");
    assert_eq!(mode(&conf_path), 0o600);
    // The folder's own mode is only forced when this call created it (§1: an existing
    // $XDG_CONFIG_HOME is not relabelled), so the check goes for the folder `save` made itself.
    let fresh = Dirs::new("recover-fresh");
    settings::save(&fresh.paths, &settings::Conf::default()).unwrap();
    assert_eq!(mode(&fresh.paths.config_dir), 0o700);
    let body = read(&conf_path);
    assert!(body.contains("PROJECT_1_ROOT=\"/sessions/lecture\""), "{body}");
    assert!(body.contains("PROJECT_1_FILE=\"/sessions/lecture/naivepost.json\""), "{body}");

    // The next launch does not ask again: same answer, and `settings::save` reports no change — the
    // bytes it would write are the bytes already there, which is §7's "read once" made true on disk.
    let again = machine_store::open_and_recover(&d.paths, &d.root).unwrap();
    assert_eq!(again.projects, conf.projects);
    assert!(
        !settings::save(&d.paths, &again).unwrap(),
        "a second launch had nothing new to write"
    );
}

/// S2: opening a project writes the pair; writing the same pair again changes no byte and no mtime.
#[test]
fn sec_01_project_and_files_7_machine_files_s2_opening_a_project_writes_the_pair_and_an_unchanged_save_is_byte_identical(
) {
    common::pin_private_cwd();
    let d = Dirs::new("persist");
    let mut projects = std::collections::BTreeMap::new();
    let root_b = Path::new("/sessions/bbb");
    let root_a = Path::new("/sessions/aaa");
    let file = |root: &Path| root.join("naivepost.json");

    machine_store::persist_project(&d.paths, &mut projects, root_b, &file(root_b)).unwrap();
    let first = read(&d.paths.conf_path());
    assert!(first.contains("PROJECT_1_ROOT=\"/sessions/bbb\""), "{first}");

    // The same project opened again: recorded, but the file is untouched — the sorted render makes an
    // unchanged save byte-identical, which is what §7 asks for.
    let changed = machine_store::persist_project(&d.paths, &mut projects, root_b, &file(root_b)).unwrap();
    assert!(!changed, "nothing to write");
    assert_eq!(read(&d.paths.conf_path()), first);

    // A second root lands as PROJECT_2, and the numbering follows the FOLDER, not the open order.
    machine_store::persist_project(&d.paths, &mut projects, root_a, &file(root_a)).unwrap();
    let both = read(&d.paths.conf_path());
    assert!(both.contains("PROJECT_1_ROOT=\"/sessions/aaa\""), "{both}");
    assert!(both.contains("PROJECT_2_ROOT=\"/sessions/bbb\""), "{both}");
    let back = settings::read(&d.paths).unwrap();
    assert_eq!(back.projects.len(), 2, "both pairs read back: {:?}", back.projects);
    assert_eq!(back.projects.get("/sessions/aaa").unwrap(), "/sessions/aaa/naivepost.json");
}

/// The beat itself, without a window: the same `checks::Draft` the dialog's timer drives, so the two
/// halves of S3 cannot disagree about when a write is owed.
#[test]
fn sec_01_project_and_files_7_machine_files_s3_the_beat_lands_only_once_it_has_elapsed() {
    common::pin_private_cwd();
    let d = Dirs::new("dialog");
    fs::create_dir_all(&d.paths.config_dir).unwrap();
    settings::save(&d.paths, &settings::Conf::default()).unwrap();
    let mut store = machine_store::Store::open(&d.paths).expect("the dialog opens over the conf");
    // The boxes are seeded from what is on disk, which is why a save that changes nothing says nothing.
    assert_eq!(store.value_for("llm"), Some(""));

    let typed = |value: &str| vec![("llm".to_string(), value.to_string())];
    let t0 = std::time::Duration::from_secs(1000);
    store.touched(t0);
    // The beat has not come round: typing an address must not rewrite a 0600 file per letter.
    assert!(store.flush(t0 + checks::CONF_SAVE_WAIT / 2, &typed("qwen3-7")).unwrap().is_none());
    assert!(!read(&d.paths.conf_path()).contains("qwen3-7"), "nothing written early");
    // The beat elapses: the file carries the new id, and §5's line names where it went.
    let line = store.flush(t0 + checks::CONF_SAVE_WAIT, &typed("qwen3-7")).unwrap();
    assert_eq!(line, Some(format!("settings saved to {}", d.paths.conf_path().display())));
    assert!(read(&d.paths.conf_path()).contains("LLM_MODEL=\"qwen3-7\""), "{}", read(&d.paths.conf_path()));
    assert_eq!(settings::read(&d.paths).unwrap().model, "qwen3-7", "and it reads back as a model");

    // A second keystroke with no beat left before closing still lands: losing the last thing typed is
    // the one failure this dialog must not have.
    store.touched(t0 + checks::CONF_SAVE_WAIT * 3);
    assert!(store.flush(t0 + checks::CONF_SAVE_WAIT * 3 + std::time::Duration::from_millis(200), &typed("llama3.2:3b")).unwrap().is_none());
    let closed = store.on_close(&typed("llama3.2:3b")).unwrap();
    assert!(closed.is_some(), "closing wrote the last value");
    assert_eq!(settings::read(&d.paths).unwrap().model, "llama3.2:3b");
    // Nothing owed now: closing again says nothing.
    assert!(store.on_close(&typed("llama3.2:3b")).unwrap().is_none());
}

/// S3: the Settings dialog has no Save button — §5 writes 600 ms after the last keystroke and again
/// on close. Driven through the REAL widgets: the named `entry-llm` box is what a person types into,
/// its own `changed` signal arms the beat, and the beat's timer lands the write.
#[test]
fn sec_01_project_and_files_7_machine_files_s3_the_settings_dialog_saves_what_was_typed_600_ms_after_the_last_keystroke_and_again_on_close(
) {
    common::pin_private_cwd();
    window_round();
    assert!(RAN_DIALOG_SAVED.load(Ordering::SeqCst), "the dialog-save check never ran");
}

static RAN_DIALOG_SAVED: AtomicBool = AtomicBool::new(false);

/// The widget half of S3, run inside the binary's one GTK round.
fn check_dialog_saves(app: &adw::Application) {
    // Everything the dialog writes goes under this temp root: the conf it saves to, and the folders it
    // reads at startup. Pinned before the dialog is built, because the store reads the environment when
    // the dialog opens.
    let root = std::env::temp_dir().join(format!("np-machw-dialog-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("cfg")).unwrap();
    fs::create_dir_all(root.join("data")).unwrap();
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", root.join("cfg"));
        std::env::set_var("XDG_DATA_HOME", root.join("data"));
    }
    let paths = settings::from_environment().expect("the pinned config folder");
    settings::save(&paths, &settings::Conf::default()).unwrap();

    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();
    open_canned(&window, echoing());
    settle();

    let entry = ui_settings::entry(&window, "llm").expect("the dialog carries entry-llm");
    // A keystroke arms the beat; nothing may be written before it elapses (no rewrite per letter).
    entry.set_text("qwen3-7");
    // GTK queues a signal emission rather than running it inline, so the handler that arms the beat has
    // to be given its turn before anything else is asserted.
    settle();
    assert!(
        !read(&paths.conf_path()).contains("qwen3-7"),
        "a save landed before the beat: {}",
        read(&paths.conf_path())
    );
    // Wait out CONF_SAVE_WAIT with the main context running, which is what fires the dialog's timer.
    // `settle` is non-blocking by design, so the wait is a bounded pump of real elapsed time.
    let deadline = std::time::Instant::now() + checks::CONF_SAVE_WAIT * 4;
    while std::time::Instant::now() < deadline {
        glib::MainContext::default().iteration(true);
        if read(&paths.conf_path()).contains("qwen3-7") {
            break;
        }
    }
    // The dialog resolves against the LAST one opened, so a second open leaves two on the stack; say
    // which file was written to, because a store built before the env was pinned points elsewhere.
    assert_eq!(
        paths.conf_path().display().to_string(),
        root.join("cfg/naivepost/llm.conf").display().to_string()
    );
    assert!(
        read(&paths.conf_path()).contains("LLM_MODEL=\"qwen3-7\""),
        "the beat wrote what was typed: {}",
        read(&paths.conf_path())
    );
    assert!(
        ui::dialog_log(&window).contains("settings saved to"),
        "the dialog said where it saved: {}",
        ui::dialog_log(&window)
    );

    // Closing lands the last keystroke even though no beat has elapsed since it.
    entry.set_text("llama3.2:3b");
    settle();
    let dialog = ui_settings::open_settings_window().expect("the dialog is open");
    dialog.close();
    // The close hook runs as the window is torn down; pump until the file says so or the wait runs out.
    let closed_at = std::time::Instant::now() + checks::CONF_SAVE_WAIT * 4;
    while settings::read(&paths).map(|c| c.model).unwrap_or_default() != "llama3.2:3b"
        && std::time::Instant::now() < closed_at
    {
        glib::MainContext::default().iteration(true);
    }
    assert_eq!(
        settings::read(&paths).unwrap().model,
        "llama3.2:3b",
        "closing wrote the last thing typed"
    );
}

/// S4: a stall writes one watchdog dump naming the component, at 0600 beside the conf; and nowhere
/// to put it costs nothing (principle 8).
#[test]
fn sec_01_project_and_files_7_machine_files_s4_a_give_up_writes_a_watchdog_dump_naming_the_component() {
    common::pin_private_cwd();
    let d = Dirs::new("hang");
    fs::create_dir_all(&d.paths.config_dir).unwrap();
    // The door reads the environment the way every leg does, so pin it at this temp folder.
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", d.root.join("cfg"));
        std::env::set_var("XDG_DATA_HOME", d.root.join("data"));
    }
    let path = machine_store::note_hang("cutting", std::time::Duration::from_secs(19))
        .expect("the dump landed beside the conf");
    assert_eq!(path.parent().unwrap(), d.paths.config_dir);
    let name = path.file_name().unwrap().to_string_lossy().into_owned();
    assert!(name.starts_with("hang-") && name.ends_with(".txt"), "{name}");
    assert_eq!(
        name,
        format!("hang-{}.txt", machine_store::local_stamp()),
        "one stamp per stall second"
    );
    assert_eq!(mode(&path), 0o600);
    let body = read(&path);
    assert!(body.contains("cutting"), "which component stalled:\n{body}");
    assert!(body.contains(&format!("pid {}", std::process::id())), "{body}");
    assert!(body.contains("19s"), "how long it was silent:\n{body}");

    // The same second again writes nothing: two dumps would bury the first.
    assert!(machine_store::note_hang("cutting", std::time::Duration::from_secs(19)).is_none());
    let dumps: Vec<String> = fs::read_dir(&d.paths.config_dir)
        .unwrap()
        .flatten()
        .filter_map(|e| e.file_name().to_str().map(str::to_string))
        .filter(|name| name.starts_with("hang-"))
        .collect();
    assert_eq!(dumps.len(), 1, "{dumps:?}");

    // And with no config folder anywhere, the give-up still succeeds: recording is observability.
    unsafe {
        std::env::remove_var("XDG_CONFIG_HOME");
        std::env::remove_var("HOME");
        std::env::remove_var("XDG_DATA_HOME");
    }
    assert!(machine_store::note_hang("narrating", std::time::Duration::from_secs(5)).is_none());
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", d.root.join("cfg"));
        std::env::set_var("XDG_DATA_HOME", d.root.join("data"));
    }
}

/// S5: the desktop entry and the MIME package are installed once, the databases are told about them,
/// and neither Flatpak nor a build cache gets either file.
#[test]
fn sec_01_project_and_files_7_machine_files_s5_the_desktop_entry_and_mime_package_are_installed_and_the_databases_told(
) {
    common::pin_private_cwd();
    let d = Dirs::new("desktop");
    // An installed launcher is any executable NOT under a `target/` folder (this test's own binary is,
    // which §7 counts as a build cache), and /usr/bin is read-only here, so the copy lives in this
    // test's own temp root.
    let installed = d.root.join("prefix/bin/naivepost");
    fs::create_dir_all(installed.parent().unwrap()).unwrap();
    fs::write(&installed, "#!/bin/sh\n").unwrap();
    fs::set_permissions(&installed, PermissionsExt::from_mode(0o755)).unwrap();
    let mut databases = Databases::new();
    let (log, commands) =
        machine_store::register_with_desktop(&d.paths, &installed, false, false, &mut databases);

    let entry = d.paths.desktop_file();
    let mime = d.paths.mime_package();
    assert!(entry.exists(), "the desktop entry was installed");
    assert!(mime.exists(), "the MIME package was installed");
    assert_eq!(mode(&entry), 0o644);
    assert_eq!(mode(&mime), 0o644);
    let entry_body = read(&entry);
    assert!(entry_body.contains(&format!("Exec={}", installed.display())), "{entry_body}");
    assert!(entry_body.contains("MimeType=application/x-naivepost-project;"), "{entry_body}");
    assert!(read(&mime).contains("<glob pattern=\"*.naivepost\"/>"), "the project suffix is declared");
    assert!(log.iter().any(|line| line.contains("told the desktop about")), "{log:?}");
    // Both databases, each pointed at the tree that holds what changed.
    let spelled: Vec<String> = commands.iter().map(|c| c.join(" ")).collect();
    assert!(
        spelled.iter().any(|c| c.starts_with("update-mime-database ")),
        "the MIME database is re-read: {spelled:?}"
    );
    assert!(
        spelled.iter().any(|c| c.starts_with("update-desktop-database ")),
        "the desktop database is re-read: {spelled:?}"
    );

    // A second launch writes neither file again, so the databases are not re-told about nothing.
    let before = (read(&entry), read(&mime));
    let (log2, commands2) =
        machine_store::register_with_desktop(&d.paths, &installed, false, false, &mut Databases::new());
    assert_eq!((read(&entry), read(&mime)), before);
    assert!(commands2.is_empty(), "nothing new for the databases: {commands2:?}");
    assert!(log2.is_empty(), "and nothing said about it: {log2:?}");

    // Inside a Flatpak the manifest exports both; a file written from inside would point into the
    // sandbox. Same for a run out of a build cache, whose path stops existing on `cargo clean`.
    let flatpak = Dirs::new("flatpak");
    let (_, commands) =
        machine_store::register_with_desktop(&flatpak.paths, &installed, true, false, &mut Databases::new());
    assert!(!flatpak.paths.desktop_file().exists(), "a Flatpak exports both from its manifest");
    assert!(!flatpak.paths.mime_package().exists());
    assert!(commands.is_empty(), "and tells nobody: {commands:?}");
    let cache = Dirs::new("cache");
    let cached = cache.root.join("target/debug/naivepost");
    fs::create_dir_all(cached.parent().unwrap()).unwrap();
    fs::write(&cached, "#!/bin/sh\n").unwrap();
    fs::set_permissions(&cached, PermissionsExt::from_mode(0o755)).unwrap();
    let (_log, commands) = machine_store::register_with_desktop(
        &cache.paths,
        &cached,
        false,
        machine_store::in_build_cache(&cached),
        &mut Databases::new(),
    );
    assert!(commands.is_empty(), "a build-cache launch installs nothing: {commands:?}");
    assert!(!cache.paths.desktop_file().exists(), "not even the file");
    assert!(!cache.paths.mime_package().exists());
    assert!(machine_store::in_build_cache(&cached), "that IS a build cache");
    assert!(!machine_store::in_build_cache(&installed));
}

/// S6: a missing refresh tool is one log line, not a failed launch.
#[test]
fn sec_01_project_and_files_7_machine_files_s6_running_the_refresh_survives_a_missing_database_tool() {
    common::pin_private_cwd();
    // This container has update-mime-database and NOT update-desktop-database, so both branches are
    // live here: one command starts, the other cannot be found.
    let log = machine_store::run_refresh(&[
        vec!["update-mime-database".to_string(), "/tmp".to_string()],
        vec!["naivepost-no-such-database-tool".to_string(), "/tmp".to_string()],
    ]);
    assert_eq!(log.len(), 1, "only the missing one is said about: {log:?}");
    assert!(log[0].contains("naivepost-no-such-database-tool"), "{log:?}");
    assert!(log[0].contains("failed"), "{log:?}");
    // An empty command list says nothing rather than panicking.
    assert!(machine_store::run_refresh(&[]).is_empty());
}
