//! §03-shell.md §6 — the settings file as the PROGRAM's own layer, not only a format.
//!
//! `tests/suite/settings_file.rs` pins the FORMAT (every key round-trips, the legacy file is taken over
//! once, precedence, the environment below the dialog, `SUBTITLE_LANGUAGES`), and
//! `tests/suite/machine_files_write.rs` pins the dialog's 600 ms beat and its own close. What these four
//! assert is that the file exists on disk because the program wrote it: a click on the gear, a typed
//! box and a closed MAIN window leave `llm.conf` behind at 0600; a launch with nothing but a legacy
//! file beside the videos produces one; and the reader every request uses answers from that file
//! rather than from a built-in default.
//!
//! Cited settings keys: LLM_MODEL, AUDIOCPP_SERVER, LLM_SERVER, LLM_API_KEY.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use naivepost::checks;
use naivepost::machine_store;
use naivepost::settings::{self, Paths};
use naivepost::ui;
use naivepost::ui::settings as ui_settings;
use crate::common;
use common::{fixture_dir, settle};

/// The item's id, interpolated into every assertion message so a failure names the item.
const ITEM: &str = "sec_03_shell_6_the_settings_file";

/// One running GTK application for the two widget checks — GTK's main loop may only be started by the
/// thread that runs it, so both share one round and each records that it got that far.
fn window_round() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let root = pin_config_home();
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(|app| {
            check_main_window_close_lands_the_settings(app);
            RAN_CLOSE.store(true, Ordering::SeqCst);
            check_an_unchanged_setting_is_not_rewritten(app);
            RAN_IDENTICAL.store(true, Ordering::SeqCst);
            app.quit();
        });
        app.run_with_args::<String>(&[]);
        let _ = fs::remove_dir_all(&root);
    });
}

static RAN_CLOSE: AtomicBool = AtomicBool::new(false);
static RAN_IDENTICAL: AtomicBool = AtomicBool::new(false);

/// A config/data pair under the temp dir named for THIS binary and pid, so no run reads another's
/// leftovers and none writes the real `$HOME`. Pinned once for the round: the dialog reads the
/// environment when it opens, so the pointer has to be set before any window is built.
fn pin_config_home() -> PathBuf {
    let root = std::env::temp_dir().join(format!("np-s6-{}-gate", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("cfg")).expect("config root");
    fs::create_dir_all(root.join("data")).expect("data root");
    std::env::set_var("XDG_CONFIG_HOME", root.join("cfg"));
    std::env::set_var("XDG_DATA_HOME", root.join("data"));
    root
}

/// A throwaway config/data pair for the two non-widget checks, per label so they never share a folder.
struct Dirs {
    root: PathBuf,
    paths: Paths,
}

impl Dirs {
    fn new(label: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "np-s6-{label}-{}-{}",
            std::process::id(),
            LABELS.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&root);
        let paths = Paths {
            config_dir: root.join("cfg/naivepost"),
            data_dir: root.join("data/naivepost"),
        };
        Self { root, paths }
    }
}

static LABELS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// The dialog's answer provider: nothing is probed here, only saved.
fn echoing() -> ui_settings::Provider {
    std::rc::Rc::new(|_key, typed| Ok(format!("{typed} answered")))
}

fn mode(path: &Path) -> u32 {
    fs::metadata(path).expect("the file is there").permissions().mode() & 0o777
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_default()
}

/// Pump the main context until `done` says so or the wait runs out — the dialog's timer and a window's
/// close hook both run as the context turns, and `settle` is deliberately non-blocking.
fn pump_until(deadline: std::time::Duration, done: impl Fn() -> bool) {
    let until = std::time::Instant::now() + deadline;
    while std::time::Instant::now() < until && !done() {
        glib::MainContext::default().iteration(true);
    }
}

/// S6: the wire. A person clicks the gear, types an id, and closes the MAIN WINDOW — not the dialog,
/// which is a separate top level whose own close hook therefore never runs. §5's "on close" and §6's
/// precedence "dialog box → this file" both hang on that path.
fn check_main_window_close_lands_the_settings(app: &adw::Application) {
    let paths = settings::from_environment().expect("{ITEM}: the pinned config folder resolves");
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();

    // The gear is how anyone reaches the dialog at all, so the click goes through it.
    let gear = ui::settings_button(&window).expect("{ITEM}: the header bar has a Settings button");
    gear.emit_clicked();
    assert!(
        ui::dialog_open(&window),
        "{ITEM} S6: clicking the gear opens the dialog"
    );
    // The gear's dialog is wired to the live provider; a canned one above it keeps the write identical
    // without dialing a server (the accessors resolve against the LAST dialog opened).
    ui::open_settings(&window, echoing());
    settle();

    let entry = ui_settings::entry(&window, "llm").expect("{ITEM}: the dialog carries entry-llm");
    entry.set_text("qwen3.8:14b");
    settle();
    assert!(
        !read(&paths.conf_path()).contains("qwen3.8:14b"),
        "{ITEM} S6: nothing may be written before the beat elapses: {}",
        read(&paths.conf_path())
    );

    // Close the MAIN window with the dialog still open and its write still owed.
    let before = ui::window_logs().len();
    window.close();
    pump_until(checks::CONF_SAVE_WAIT * 6, || {
        settings::read(&paths).map(|conf| conf.model).unwrap_or_default() == "qwen3.8:14b"
    });

    assert!(
        paths.conf_path().is_file(),
        "{ITEM} S6: closing the main window left no {} behind",
        paths.conf_path().display()
    );
    assert_eq!(
        settings::read(&paths).expect("{ITEM} S6: the conf reads back").model,
        "qwen3.8:14b",
        "{ITEM} S6: the typed setting was lost when the main window closed"
    );
    assert_eq!(mode(&paths.conf_path()), 0o600, "{ITEM} S6: the key stays a secret");
    assert_eq!(
        mode(&paths.config_dir),
        0o700,
        "{ITEM} S6: the folder holding it is private too"
    );
    let logged = ui::window_logs()[before..].join("\n");
    assert!(
        logged.contains("settings saved to"),
        "{ITEM} S6: the main log said where the settings went, got {logged:?}"
    );
}

/// S6 again: "written whole" must not mean "rewritten always". Closing a dialog that changed nothing
/// leaves the bytes alone, which is what keeps the file's mtime meaning "last edited".
fn check_an_unchanged_setting_is_not_rewritten(app: &adw::Application) {
    let paths = settings::from_environment().expect("{ITEM}: the pinned config folder resolves");
    let body = read(&paths.conf_path());
    assert!(
        body.contains("LLM_MODEL=\"qwen3.8:14b\""),
        "{ITEM} S6: the previous check's write should still be the file: {body:?}"
    );
    let stamp = fs::metadata(paths.conf_path()).expect("the conf is there").modified().unwrap();

    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();
    ui::open_settings(&window, echoing());
    settle();
    // Press nothing, change nothing, close it.
    let dialog = ui_settings::open_settings_window().expect("{ITEM}: the dialog is open");
    dialog.close();
    pump_until(std::time::Duration::from_millis(200), || false);

    assert_eq!(
        read(&paths.conf_path()),
        body,
        "{ITEM} S6: a close that changed nothing rewrote the file"
    );
    assert_eq!(
        fs::metadata(paths.conf_path()).unwrap().modified().unwrap(),
        stamp,
        "{ITEM} S6: an unchanged save touched the file anyway"
    );
    let _ = window;
}

#[test]
fn sec_03_shell_6_the_settings_file_s6_a_setting_typed_into_the_dialog_is_on_disk_after_the_main_window_closes() {
    common::pin_private_cwd();
    window_round();
    assert!(
        RAN_CLOSE.load(Ordering::SeqCst),
        "{ITEM} S6: the main-window-close check never ran"
    );
}

#[test]
fn sec_03_shell_6_the_settings_file_s6_an_unchanged_setting_is_not_rewritten() {
    common::pin_private_cwd();
    window_round();
    assert!(
        RAN_IDENTICAL.load(Ordering::SeqCst),
        "{ITEM} S6: the byte-identical check never ran"
    );
}

/// S7: a launch finds no conf of its own but a legacy one beside the videos, and takes it over — once.
/// This is the precedence layer being CREATED by starting the program, not by a test calling `save`.
#[test]
fn sec_03_shell_6_the_settings_file_s7_a_launch_with_no_conf_but_a_legacy_one_writes_the_file() {
    common::pin_private_cwd();
    let d = Dirs::new("legacy-launch");
    fs::create_dir_all(&d.root).unwrap();
    fs::write(
        settings::legacy_conf_path(&d.root),
        "LLM_SERVER=\"http://old-box:8731/v1\"\nLLM_API_KEY=\"sk-old\"\n",
    )
    .unwrap();
    assert!(
        !d.paths.conf_path().exists(),
        "{ITEM} S7: this is the launch right after the merge"
    );

    let conf = machine_store::open_and_recover(&d.paths, &d.root).expect("{ITEM} S7: the launch reads its settings");
    assert_eq!(
        conf.server, "http://old-box:8731/v1",
        "{ITEM} S7: the legacy endpoint did not reach the store"
    );
    assert!(
        d.paths.conf_path().is_file(),
        "{ITEM} S7: no {} after a launch that had a legacy file",
        d.paths.conf_path().display()
    );
    let body = read(&d.paths.conf_path());
    assert!(
        body.contains("LLM_SERVER=\"http://old-box:8731/v1\""),
        "{ITEM} S7: the migrated file does not carry the endpoint: {body:?}"
    );
    assert_eq!(mode(&d.paths.conf_path()), 0o600, "{ITEM} S7: a copied key is still 0600");
    assert_eq!(mode(&d.paths.config_dir), 0o700, "{ITEM} S7: and its folder 0700");

    // "Migrated once": a second launch reads the config folder's own file and leaves it untouched.
    let stamp = fs::metadata(d.paths.conf_path()).unwrap().modified().unwrap();
    let again = machine_store::open_and_recover(&d.paths, &d.root).expect("{ITEM} S7: the second launch reads too");
    assert_eq!(again.server, conf.server, "{ITEM} S7: the file answers the second launch");
    assert_eq!(
        fs::metadata(d.paths.conf_path()).unwrap().modified().unwrap(),
        stamp,
        "{ITEM} S7: a second launch re-migrated the legacy file"
    );
}

/// S7: the file is the layer under the dialog box, read back through the door every request actually
/// uses — not through a copy of the chain in this test.
#[test]
fn sec_03_shell_6_the_settings_file_s7_the_file_is_the_layer_under_the_dialog_box() {
    common::pin_private_cwd();
    let d = Dirs::new("precedence");
    let mut conf = settings::Conf::default();
    conf.audio_server = "http://cpp-box:8765".to_string();
    settings::save(&d.paths, &conf).expect("{ITEM} S7: setup writes the file");

    // No environment at all: the file's own value wins over the shipped loopback.
    let no_env = |_: &str| None;
    let endpoint = naivepost::services::endpoint_for(&d.paths, naivepost::services::Server::Audio, no_env);
    assert_eq!(
        endpoint.url, "http://cpp-box:8765",
        "{ITEM} S7: the reader ignored the file and took a default"
    );

    // Clear the box (§5 has no Save: an empty box IS the edit) and the default answers again.
    let mut cleared = settings::Conf::default();
    cleared.audio_server = String::new();
    settings::save(&d.paths, &cleared).expect("{ITEM} S7: setup writes the cleared file");
    let defaulted = naivepost::services::endpoint_for(&d.paths, naivepost::services::Server::Audio, no_env);
    assert_eq!(
        defaulted.url,
        naivepost::services::audio_default(),
        "{ITEM} S7: a cleared box fell through to something other than the shipped default"
    );

    // And the environment sits BELOW the box but above the default (§6's own sentence).
    let env = |name: &str| (name == "NAIVEPOST_TTS_URL").then(|| "http://env-box:9000".to_string());
    let from_env = naivepost::services::endpoint_for(&d.paths, naivepost::services::Server::Audio, env);
    assert_eq!(from_env.url, "http://env-box:9000", "{ITEM} S7: the env variable was skipped");
    settings::save(&d.paths, &conf).expect("{ITEM} S7: setup writes the box value back");
    let from_box = naivepost::services::endpoint_for(&d.paths, naivepost::services::Server::Audio, env);
    assert_eq!(
        from_box.url, "http://cpp-box:8765",
        "{ITEM} S7: a filled box must outrank the environment"
    );
}
