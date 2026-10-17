//! F0.6 Open at start — the four doors driven through a REAL launch, not through `startup::decide`.
//!
//! `tests/open_at_start.rs` proves the precedence rule; this file proves the wire: that gio's two
//! handlers reach [`naivepost::ui::launch::open_at_start`], that the window ends up showing the project
//! each door names, and that the launch leaves its pair in `llm.conf` so the NEXT launch's S2 has
//! something to find (nothing wrote `PROJECT_<n>` at startup before this wiring, so S2 could never fire
//! on a real second start).
//!
//! One application for the whole binary and every scenario inside its single `activate` — the shape
//! `f0_5_bookkeeping_live.rs` uses, and the only one GTK allows here: `g_application_run` may be entered
//! once per process, and libadwaita refuses a window built on any thread other than the one that
//! entered the run. So the round runs once (the first `#[test]` to ask starts it) and records what each
//! launch showed into [`SLOTS`]; the four checks read their own slot back.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use adw::prelude::*;
use naivepost::project::{self, Project, Source};
use naivepost::settings::{self, Conf, Paths};
use naivepost::ui;
#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::temp_dir;

/// What one launch left behind, read back by the check that asked for it.
#[derive(Default, Clone)]
struct Slot {
    /// The sources the shown session holds — which project the window ended up on.
    sources: Vec<String>,
    /// The launch's own log, joined: it carries the `>>> opening …` line naming the door.
    log: String,
    /// `llm.conf`'s remembered pairs after that launch, exactly as the next launch would read them.
    projects: BTreeMap<String, String>,
    /// The conf rendered before and after, so "remembering changed nothing" can be pinned.
    conf_before: String,
    conf_after: String,
    /// What the launch folder held right after the launch call, before the loop turned again: the
    /// session's autosave writes it on its first beat, so only this moment shows what the launch wrote.
    root_after_launch: Vec<String>,
}

static SLOTS: std::sync::Mutex<[Slot; 4]> = std::sync::Mutex::new([
    Slot {
        sources: Vec::new(),
        log: String::new(),
        projects: BTreeMap::new(),
        conf_before: String::new(),
        conf_after: String::new(),
        root_after_launch: Vec::new(),
    },
    Slot {
        sources: Vec::new(),
        log: String::new(),
        projects: BTreeMap::new(),
        conf_before: String::new(),
        conf_after: String::new(),
        root_after_launch: Vec::new(),
    },
    Slot {
        sources: Vec::new(),
        log: String::new(),
        projects: BTreeMap::new(),
        conf_before: String::new(),
        conf_after: String::new(),
        root_after_launch: Vec::new(),
    },
    Slot {
        sources: Vec::new(),
        log: String::new(),
        projects: BTreeMap::new(),
        conf_before: String::new(),
        conf_after: String::new(),
        root_after_launch: Vec::new(),
    },
]);

fn slot(id: usize) -> Slot {
    SLOTS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()[id - 1]
        .clone()
}
fn record(id: usize, fill: impl FnOnce(&mut Slot)) {
    let mut slots = SLOTS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    fill(&mut slots[id - 1]);
}

/// A project folder holding ONE distinctive recording, so "which project is the window showing?" has an
/// unambiguous answer read off the live session rather than off a path string.
fn plant_project(dir: &Path, marker: &str) -> PathBuf {
    std::fs::create_dir_all(dir).expect("project folder");
    let project = Project {
        sources: vec![Source {
            path: format!("project:sources/{marker}.mkv"),
            footage: true,
            ..Default::default()
        }],
        ..Default::default()
    };
    project::save(&project, dir).expect("project written");
    // The recording itself has to be on disk: a launch that opens this project drops a source whose
    // file is missing (F0.9 S3), and these scenarios ask which PROJECT opened, not whether the
    // recording survived. `sources/` stays out of S4's untouched-root check, which looks at a blank root.
    let sources = dir.join("sources");
    std::fs::create_dir_all(&sources).expect("sources folder");
    std::fs::write(sources.join(format!("{marker}.mkv")), "video bytes").expect("recording planted");
    dir.to_path_buf()
}

/// A conf remembering one root → file pair, built from the default so the private `unknown` keys stay
/// whatever the current build writes.
fn remembering(root: &Path, file: &Path) -> Conf {
    let mut conf = Conf::default();
    conf.projects.insert(root.display().to_string(), file.display().to_string());
    conf
}

/// This machine's config home, pointed at a temp folder: `launch::settings_paths` reads
/// `$XDG_CONFIG_HOME`/`$XDG_DATA_HOME`, both process-wide, which is why the scenarios share one folder
/// and run one at a time inside the round.
fn config_home() -> Paths {
    let dir = std::env::temp_dir().join(format!("np-f06-conf-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("config home");
    std::env::set_var("XDG_CONFIG_HOME", &dir);
    std::env::set_var("XDG_DATA_HOME", &dir);
    settings::from_environment().expect("an absolute XDG_CONFIG_HOME is a config home")
}

/// Read `llm.conf` back the way the next launch will.
fn conf_on_disk(paths: &Paths) -> Conf {
    settings::read(paths).expect("the conf reads back")
}

/// The conf as it would be written again: compared before/after rather than the raw bytes, because a
/// first write also fills in every key the seed left at its default (§8), which is not what the
/// byte-identical rule of §7 is about.
fn rendered(paths: &Paths) -> String {
    settings::render(&conf_on_disk(paths))
}

/// One launch's worth of state: clear the log, pin the working folder, and take what the window shows
/// plus what the conf now remembers.
fn capture(id: usize, paths: &Paths) {
    let window = ui::window::window_logs();
    let _ = window;
    record(id, |slot| {
        slot.log = ui::window::window_logs().join("\n");
        slot.projects = conf_on_disk(paths).projects.clone();
        slot.conf_after = rendered(paths);
    });
}

/// Where each scenario launched and what its launch left in `llm.conf`. `Once` runs the round on ONE
/// thread and every check reads back its own row, so a plain mutex -- not a thread-local -- is what holds
/// it: a check may land on a different worker than the one that ran the round.
#[derive(Default)]
struct ScenarioRecord {
    root: Option<PathBuf>,
}

static ROOTS: std::sync::Mutex<[ScenarioRecord; 4]> = std::sync::Mutex::new([
    ScenarioRecord { root: None },
    ScenarioRecord { root: None },
    ScenarioRecord { root: None },
    ScenarioRecord { root: None },
]);

/// Remember which folder scenario `id` launched in.
fn note_root(id: usize, root: &Path) {
    ROOTS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())[id - 1]
        .root = Some(root.to_path_buf());
}

fn root_of(id: usize) -> PathBuf {
    ROOTS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())[id - 1]
        .root
        .clone()
        .unwrap_or_else(|| panic!("scenario S{id} recorded no root"))
}

/// The project file each door is expected to name in `llm.conf`: a `.naivepost` folder plus its
/// `naivepost.json`, which is what §7's pair stores (the FOLDER, not the file, is what S2 opens).
fn remembered_file(root: &Path, leaf: &str) -> String {
    root.join(leaf).join("naivepost.json").display().to_string()
}

/// Run the round: one application, one `run_with_args`, all four doors walked inside its handlers.
fn launch_round() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        // The scenarios run on THIS thread -- the one that enters `run`, which is the only thread GTK and
        // libadwaita allow a window to be built on. They are sequential, so the process-wide pieces
        // (`$PWD`, `$XDG_CONFIG_HOME`) can be re-pinned between them.
        let fired = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let runner = fired.clone();
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            // NON_UNIQUE keeps a test run off the session bus. `main` adds HANDLES_OPEN and gets its
            // folders through `connect_open`; here every scenario pins `$PWD` before it launches, so all
            // four doors answer inside one `activate` -- gio may only be entered once per process.
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(move |app| {
            // A plain launch with nothing handed lands here, and `main`'s handler runs the same call with
            // an empty list. S1 hands its files explicitly instead, exactly as gio's own `connect_open`
            // would after parsing argv. Only the first `activate` does the work; later ones are idle.
            if !runner.load(std::sync::atomic::Ordering::SeqCst) {
                return;
            }
            runner.store(false, std::sync::atomic::Ordering::SeqCst);
            scenario_handed(app);
            scenario_remembered(app);
            scenario_session(app);
            scenario_blank(app);
        });
        // The round ends on a timer rather than by closing windows: `run` returns when the last window
        // closes, and these stay open so each scenario still has a page to read off.
        let _ = glib::timeout_add_local(std::time::Duration::from_millis(50), {
            let app = app.clone();
            move || {
                app.quit();
                glib::ControlFlow::Break
            }
        });
        let _ = app.run_with_args::<String>(&[]);
    });
}

/// Fold what the window shows into the slot for the launch that just ran.
fn remember_sources(id: usize, app: &adw::Application, paths: &Paths) {
    let window = app
        .active_window()
        .or_else(|| app.windows().first().cloned())
        .expect("the launch presented a window")
        .downcast::<adw::ApplicationWindow>()
        .expect("an adwaita window");
    let sources = ui::session_sources(&window);
    record(id, |slot| slot.sources = sources);
    capture(id, paths);
    // The window stays open on purpose: closing it would end the round before the later scenarios run.
}

/// S1: the desktop hands two folders and only the FIRST opens — and the launch then remembers it, so
/// the next start's S2 finds it without anything being handed again.
fn scenario_handed(app: &adw::Application) {
    let root = temp_dir("f06-s1");
    std::fs::create_dir_all(&root).expect("root");
    let first = plant_project(&root.join("first.naivepost"), "first-recording");
    plant_project(&root.join("second.naivepost"), "second-recording");
    std::env::set_current_dir(&root).expect("cwd pinned");
    let paths = config_home();
    settings::save(&paths, &Conf::default()).expect("empty conf");
    record(1, |slot| slot.conf_before = rendered(&paths));
    note_root(1, &root);
    ui::window::clear_logs_for_test();
    // What gio would hand `connect_open` after parsing the command line: the folders, in the order given.
    let handed = vec![first, root.join("second.naivepost")];
    naivepost::ui::launch::open_at_start(app, &handed);
    remember_sources(1, app, &paths);
}

/// S2: what the last launch left in `llm.conf` opens this one, over a working copy that is also there.
fn scenario_remembered(app: &adw::Application) {
    let root = temp_dir("f06-s2");
    std::fs::create_dir_all(&root).expect("root");
    let jan = plant_project(&root.join("jan.naivepost"), "january-recording");
    // The working copy exists too: S2 must win over S3, not fall through to it.
    plant_project(&naivepost::startup::session_dir(&root), "working-copy");
    std::env::set_current_dir(&root).expect("cwd pinned");
    let paths = config_home();
    // §7 stores the FILE inside the `.naivepost` folder, which is what a byte-identical rewrite needs to
    // match: seeding the bare folder would make this launch "remember" it differently and rewrite the pair.
    settings::save(&paths, &remembering(&root, &jan.join("naivepost.json"))).expect("conf seeded");
    record(2, |slot| slot.conf_before = rendered(&paths));
    note_root(2, &root);
    ui::window::clear_logs_for_test();
    naivepost::ui::launch::open_at_start(app, &[]);
    remember_sources(2, app, &paths);
}

/// S3: nothing remembered for this root, but `<root>/session.naivepost` is there.
fn scenario_session(app: &adw::Application) {
    let root = temp_dir("f06-s3");
    std::fs::create_dir_all(&root).expect("root");
    // §8's icons bullet needs a tree to find. The launch looks beside the binary, under the app root
    // and in the working directory; this scenario pins the cwd to `root` two lines below, so an
    // `icons/` folder planted here is the one it answers with, and the check on slot 3 reads the
    // resulting log line back. Neither of the two folders above the test binary holds an `icons`
    // sibling, so nothing earlier can win the search.
    std::fs::create_dir_all(root.join("icons")).expect("icons tree");
    plant_project(&naivepost::startup::session_dir(&root), "the-working-copy");
    std::env::set_current_dir(&root).expect("cwd pinned");
    let paths = config_home();
    settings::save(&paths, &Conf::default()).expect("conf with no pairs");
    note_root(3, &root);
    ui::window::clear_logs_for_test();
    naivepost::ui::launch::open_at_start(app, &[]);
    remember_sources(3, app, &paths);
}

/// S4: a first launch in an empty folder — a blank session, and nothing written anywhere.
fn scenario_blank(app: &adw::Application) {
    let root = temp_dir("f06-s4");
    std::fs::create_dir_all(&root).expect("root");
    std::env::set_current_dir(&root).expect("cwd pinned");
    let paths = config_home();
    settings::save(&paths, &Conf::default()).expect("conf with no pairs");
    ui::window::clear_logs_for_test();
    naivepost::ui::launch::open_at_start(app, &[]);
    let left: Vec<String> = std::fs::read_dir(&root)
        .expect("root readable")
        .map(|entry| entry.expect("an entry").file_name().to_string_lossy().into_owned())
        .collect();
    record(4, |slot| slot.root_after_launch = left);
    remember_sources(4, app, &paths);
    record(4, |slot| slot.conf_before = slot.conf_after.clone());
    // Left for the check: S4 must not have created the session folder or touched the root.
    note_root(4, &root);
}

/// S1: the desktop hands two folders and only the FIRST opens — and the launch then remembers it, so
/// the next start's S2 finds it without anything being handed again.
#[test]
fn f0_6_s1_a_handed_folder_opens_and_is_remembered() {
    launch_round();
    let got = slot(1);
    assert_eq!(
        got.sources,
        vec!["project:sources/first-recording.mkv".to_string()],
        "only the first handed folder opens (log: {})",
        got.log
    );
    assert!(
        got.log.contains("handed by the desktop"),
        "the launch says which door answered: {}",
        got.log
    );
    // §7's state: the pair lands in `llm.conf`, keyed by the folder the launch started in — the same key
    // S2 reads with. That write is what makes a real second launch able to answer S2 at all.
    let root = root_of(1);
    let expected = remembered_file(&root, "first.naivepost");
    assert_eq!(
        got.projects.get(&root.display().to_string()),
        Some(&expected),
        "the launch remembered what it opened: {:?}",
        got.projects
    );
    assert_eq!(got.projects.len(), 1, "and nothing else: {:?}", got.projects);
}

/// S2: what the LAST launch left in `llm.conf` opens this one, over a working copy that is also sitting
/// there — and re-opening it changes no byte.
#[test]
fn f0_6_s2_the_project_remembered_by_the_last_launch_opens() {
    launch_round();
    let got = slot(2);
    assert_eq!(
        got.sources,
        vec!["project:sources/january-recording.mkv".to_string()],
        "S2 opens what was remembered, not the session beside the root (log: {})",
        got.log
    );
    assert!(
        got.log.contains("remembered for this root"),
        "the launch says which door answered: {}",
        got.log
    );
    assert_eq!(
        got.conf_before, got.conf_after,
        "re-opening the remembered project rewrote nothing (§7's byte-identical save)"
    );
}

/// S3: nothing remembered for this root, but the working copy beside it is there — that is the session.
#[test]
fn f0_6_s3_the_session_beside_the_root_opens() {
    launch_round();
    let got = slot(3);
    assert_eq!(
        got.sources,
        vec!["project:sources/the-working-copy.mkv".to_string()],
        "S3 opens the working copy beside the root (log: {})",
        got.log
    );
    assert!(
        got.log.contains("the session beside the root"),
        "the launch says which door answered: {}",
        got.log
    );
    // §8's icons bullet, read back from a real launch: the tree found beside the working directory is
    // named once, so an icon the theme cannot look up is at least traceable to a folder that was searched.
    assert!(
        got.log.contains("icons from"),
        "the launch names the icon tree it found: {}",
        got.log
    );
    // The working copy is a real folder, so it is worth remembering for next time too.
    let root = root_of(3);
    let expected = remembered_file(&root, "session.naivepost");
    assert_eq!(
        got.projects.get(&root.display().to_string()),
        Some(&expected),
        "the session the launch opened is remembered: {:?}",
        got.projects
    );
}

/// S4: a first launch in an empty folder — a blank session, no sources, and NOTHING written: neither
/// the session folder nor a remembered pair for a path that does not exist.
#[test]
fn f0_6_s4_nothing_to_open_is_a_blank_session_and_writes_nothing() {
    launch_round();
    let got = slot(4);
    assert!(got.sources.is_empty(), "a blank session shows no sources (log: {})", got.log);
    assert!(
        got.log.contains("a blank session"),
        "the launch says which door answered: {}",
        got.log
    );
    assert!(got.projects.is_empty(), "nothing was remembered for a path that does not exist: {:?}", got.projects);
    assert_eq!(got.conf_before, got.conf_after, "and the conf kept its bytes");
    // S4 is a decision, not a file: the launch itself writes nothing, neither the session folder nor
    // anything else in the folder it happened to start from. The session appears when it is first
    // saved, which the autosave does on its beat, so the folder is read right after the launch.
    assert!(
        got.root_after_launch.is_empty(),
        "the launch wrote into the folder it started from: {:?}",
        got.root_after_launch
    );
}
