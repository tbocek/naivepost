//! F0.6 Open at start: which project a start opens and what it leaves remembered. Split from `main.rs`; [`crate::startup`] decides which door answered, this makes it real on disk.

use std::path::{Path, PathBuf};

use adw::prelude::*;

use crate::{icons, machine_store, open_project, project, settings, startup, ui, watchdog};

/// Open one folder the way a launch opens it: the project AND the load report (§8 [F0.9]). An unreadable folder still opens blank.
/// F0.9 S3's dropped-source lines come from [`open_project::apply`], the ONE door that decides which sources are gone, appended after the load report.
pub fn load_for_launch(path: &Path) -> (project::Project, Vec<String>) {
    let (mut project, mut report) =
        project::load_report(path).unwrap_or_else(|_| (project::Project::default(), Vec::new()));
    // Only a folder with a project file has sources to lose.
    if path.join(project::PROJECT_FILE).is_file() {
        let kept = std::mem::take(&mut project.sources);
        project.sources = kept
            .iter()
            .filter(|source| {
                open_project::source_present(path, &source.path, &|file| file.exists())
            })
            .cloned()
            .collect();
        // One line per missing file, once each.
        let mut said: Vec<String> = Vec::new();
        for source in kept.iter().filter(|s| !project.sources.contains(s)) {
            if !said.contains(&source.path) {
                said.push(source.path.clone());
                report.push(open_project::dropped_line(&source.path));
            }
        }
        // §4: a load that stripped a bad tag, asked of the same rule `apply` asks, is cleared here too and reported.
        crate::sources::clean_on_load(&mut project, &mut report);
        // Seating for a load whose OWN drop emptied slot 1, via the same second door `apply` opens.
    }
    (project, report)
}

/// The launch's icon search: beside the binary, under the app root, then the working directory. Split out so a test pins the same order.
pub fn icons_for_launch(exe: &Path, cwd: &Path) -> Option<PathBuf> {
    let exe_dir = exe.parent().unwrap_or(Path::new(".")).to_path_buf();
    let app_root = exe_dir.parent().unwrap_or(Path::new(".")).to_path_buf();
    icons::found(&exe_dir, &app_root, cwd)
}

/// Which project this launch opens (F0.6), decided by [`startup::decide`]. The root is the start folder, which S2 keys its remembered project by and S3 looks for the working copy in. A blank session loads empty and writes nothing; an unreadable folder opens blank (F0.9 will say why).
pub fn open_at_start(app: &adw::Application, handed: &[PathBuf]) {
    let root = std::env::current_dir().unwrap_or_default();
    // Environment is read here and handed to settings.rs as strings, keeping its tests off the real `$HOME`.
    let paths = settings_paths();
    // §1's watchdog: started once at launch; the GUI heartbeat is armed per window by `build_window`.
    watchdog::start_watch(paths.clone(), watchdog::SILENCE_LIMIT);
    // §7's launch job: take over a legacy `<root>/llm.conf`, and when it holds no `PROJECT_*` pairs recover them from legacy `settings.json`. An unreadable file must not cost the session.
    let conf = paths
        .as_ref()
        .and_then(|paths| machine_store::open_and_recover(paths, &root).ok())
        .unwrap_or_default();

    let opened = startup::decide(&root, &conf, handed);
    // Record which door answered; it reaches the log through `note_startup` once a window exists.
    let door = format!(
        ">>> opening {} ({})",
        opened.kind(),
        opened.path().display()
    );
    // §8 [F0.9]: a legacy `out_dir` is reported via `load_report`.
    let (project, load_lines) = match &opened {
        startup::Opened::Blank { .. } => (project::Project::default(), Vec::new()),
        named => load_for_launch(named.path()),
    };
    // "" is Prepare, the page a session starts on (§1). The decided folder becomes every step's folder via `build_window_at`.
    ui::window::build_window_at(app, &project, "", opened.path()).present();
    let remembered = remember_launch(paths.as_ref(), &conf, &root, &opened);
    // §7: register with the desktop unless this is a build cache or Flatpak; the lines wait in `note_startup` until a window exists.
    let mut startup_lines = Vec::new();
    if let (Some(paths), Some(exe)) = (paths.as_ref(), std::env::current_exe().ok()) {
        let mut databases = icons::Databases::new();
        let (log, commands) = machine_store::register_with_desktop(
            paths,
            &exe,
            machine_store::inside_flatpak(),
            machine_store::in_build_cache(&exe),
            &mut databases,
        );
        startup_lines.extend(log);
        startup_lines.extend(machine_store::run_refresh(&commands));
    }
    // §8 icons: look beside the binary, under the app root, then the working directory (`icons_for_launch`). Finding nothing is silent.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(icons) = icons_for_launch(&exe, &root) {
            startup_lines.push(format!(">>> icons from {}", icons.display()));
        }
        // The tree beside the binary is the APP icon tree; a package's own theme sits there too, so it is named for the log.
        for theme in icons::bundled_themes(&exe, &root) {
            startup_lines.push(format!(">>> icon theme {}", theme.display()));
        }
    }
    // The load report (§8 legacy `out_dir` among it) goes in before this module's last two lines.
    startup_lines.extend(load_lines);
    // Which door answered and whether the pair reached `llm.conf` (§7), after the desktop's lines.
    startup_lines.insert(0, door);
    if remembered == Some(true) {
        startup_lines.push(format!(
            ">>> remembered for next time: {}",
            opened.path().display()
        ));
    }
    ui::window::note_startup(&startup_lines);
}

/// S2's other half: put this launch's project in `llm.conf` for the next S2.
/// Keyed by the start folder, as [`startup::last_project`] reads (NOT F0.9's per-folder key; both write through [`machine_store::persist_project`]); starts from the conf's own pairs so another root's memory survives. A blank session remembers nothing. Returns `None` when nothing was written; the caller logs it.
fn remember_launch(
    paths: Option<&settings::Paths>,
    conf: &settings::Conf,
    root: &std::path::Path,
    opened: &startup::Opened,
) -> Option<bool> {
    let (Some(paths), Some(folder)) = (paths, opened.opened_folder()) else {
        return None;
    };
    let mut projects = conf.projects.clone();
    let file = open_project::project_file_in(&folder);
    // The write's answer also says it landed, which is logged (§7).
    match machine_store::persist_project(paths, &mut projects, root, &file) {
        Ok(changed) => Some(changed),
        Err(why) => {
            // The reason is logged and `None` returned, keeping §1's failure out of the "remembered" line.
            ui::window::note_startup(&[format!("!!! could not remember this project -- {why}")]);
            None
        }
    }
}

fn settings_paths() -> Option<settings::Paths> {
    let var = |name: &str| std::env::var(name).ok();
    // `data_home` has no env var of its own; it stays `None` so `$XDG_DATA_HOME`/$HOME answer.
    settings::paths_from(
        var("XDG_CONFIG_HOME").as_deref(),
        var("HOME").as_deref(),
        None,
        var("XDG_DATA_HOME").as_deref(),
    )
}
