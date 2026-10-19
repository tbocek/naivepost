//! F0.6 Open at start — the launch's own door: which project a start opens, and what it leaves
//! remembered for the next one.
//!
//! Split out of `main.rs`, which is only allowed to build the application and hand these two calls to
//! gio: `connect_activate` runs [`open_at_start`] with nothing handed, `connect_open` runs it with the
//! files the desktop passed. Which of the four doors answers is [`crate::startup`]'s decision; this
//! module is the caller that makes the decision real on disk.

use std::path::{Path, PathBuf};

use adw::prelude::*;

use crate::{icons, machine_store, open_project, project, settings, startup, ui};

/// Open one folder the way a launch opens it: the project AND whatever the load had to say about it.
///
/// `project::load` alone answers the project and drops the report, which is how a legacy `out_dir`
/// could sit in a session opened at start without anyone being told (§8's [F0.9] bullet). A folder
/// that cannot be read still opens blank — the same behaviour this call replaced (`unwrap_or_default`)
/// keeps, because losing the session over an unreadable file is worse than opening empty.
///
/// F0.9 S3's dropped-source line belongs here too: a launch takes on a project exactly as the Open
/// button does, so it asks [`open_project::apply`] — the ONE door that decides which sources are gone —
/// and returns its lines after the load report. Going through `apply` rather than re-checking the files
/// here is what keeps the sentence spelled in one place and said once per missing file.
pub fn load_for_launch(path: &Path) -> (project::Project, Vec<String>) {
    let (mut project, mut report) =
        project::load_report(path).unwrap_or_else(|_| (project::Project::default(), Vec::new()));
    // Only a folder with a project file has sources to lose; a blank session has none and must not
    // gain a complaint about it.
    if path.join(project::PROJECT_FILE).is_file() {
        let kept = std::mem::take(&mut project.sources);
        project.sources = kept
            .iter()
            .filter(|source| {
                open_project::source_present(path, &source.path, &|file| file.exists())
            })
            .cloned()
            .collect();
        // One line per missing file, in the order the project listed them, and once each: a source
        // spelled twice would otherwise print the same complaint twice.
        let mut said: Vec<String> = Vec::new();
        for source in kept.iter().filter(|s| !project.sources.contains(s)) {
            if !said.contains(&source.path) {
                said.push(source.path.clone());
                report.push(open_project::dropped_line(&source.path));
            }
        }
    }
    (project, report)
}

/// The launch's icon search, in the shape `open_at_start` calls it: beside the binary, under the
/// app root (the prefix an installed build sits in — the exe's parent), then the working directory.
/// Split out so a test pins the SAME order the launch uses rather than retyping its argument shapes.
pub fn icons_for_launch(exe: &Path, cwd: &Path) -> Option<PathBuf> {
    let exe_dir = exe.parent().unwrap_or(Path::new(".")).to_path_buf();
    let app_root = exe_dir.parent().unwrap_or(Path::new(".")).to_path_buf();
    icons::found(&exe_dir, &app_root, cwd)
}

/// Which project this launch opens and shows (F0.6), decided by [`startup::decide`].
///
/// The root is the folder the program was started from, which is what F0.6's S2 keys its remembered
/// project by and where S3 looks for the working copy. A blank session loads as an empty project and
/// writes nothing; a folder that cannot be read opens blank too, for now — saying *why* on the status
/// line is F0.9 (Open a project), along with taking over a handed `naivepost.json` as its folder. New
/// Project is F0.8's.
pub fn open_at_start(app: &adw::Application, handed: &[PathBuf]) {
    let root = std::env::current_dir().unwrap_or_default();
    // The environment is read here and handed to settings.rs as strings, which is how that module
    // keeps its own tests off the real `$HOME`. No config folder at all means no settings.
    let paths = settings_paths();
    // §7's launch job, in one call: take over a legacy `<root>/llm.conf`, read the conf, and when it
    // holds no `PROJECT_*` pairs recover them from the legacy `settings.json` and write them back so
    // that file is never asked about again. A settings file that cannot be read must not cost the
    // user their session: the fallback opens the working copy like any other machine's.
    let conf = paths
        .as_ref()
        .and_then(|paths| machine_store::open_and_recover(paths, &root).ok())
        .unwrap_or_default();

    let opened = startup::decide(&root, &conf, handed);
    // Which door answered is the first thing worth recording: it answers "why did THIS open?", and
    // `startup::Opened` is a decision with no widget of its own to show it. It reaches the log through
    // the single `note_startup` at the end, because that sink only exists once a window was built.
    let door = format!(">>> opening {} ({})", opened.kind(), opened.path().display());
    // §8's [F0.9] bullet: a project with a legacy `out_dir` says so when it opens, and the launch is
    // one of the ways it opens — through `load_report`, which is where that line is produced.
    let (project, load_lines) = match &opened {
        // A blank session is no file at all, so there is nothing to read.
        startup::Opened::Blank { .. } => (project::Project::default(), Vec::new()),
        // The other three name a folder to open, and all three are opened the same way.
        named => load_for_launch(named.path()),
    };
    // "" is Prepare, the page a session always starts on (§1).
    ui::build_window(app, &project, "").present();
    // S2 has to have something to find next time: record what THIS launch opened against this root.
    let remembered = remember_launch(paths.as_ref(), &conf, &root, &opened);
    // §7's other launch job: register with the desktop unless this run is a build cache or a Flatpak,
    // then tell the MIME and desktop databases what changed. The lines reach the window's log through
    // `note_startup`, which holds them until the first window is built — there is no log before one.
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
    // §8's icons bullet: the tree travels with the app, so the launch looks for it beside the binary,
    // under the app root, then in the working directory — the order `icons_for_launch` fixes.
    // Finding nothing is not worth a line: an icon the theme cannot look up is invisible, not broken.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(icons) = icons_for_launch(&exe, &root) {
            startup_lines.push(format!(">>> icons from {}", icons.display()));
        }
        // The other half of the same bullet: the tree beside the binary is the APP icon's tree, and
        // every tab and control is drawn from a theme GTK finds through the XDG data dirs. When a
        // package carries its own theme it sits in the same place, so it is named here too — a broken
        // icon is otherwise invisible in the log and unexplainable afterwards.
        for theme in icons::bundled_themes(&exe, &root) {
            startup_lines.push(format!(">>> icon theme {}", theme.display()));
        }
    }
    // What the loaded project had to say (§8's legacy-`out_dir` line among them) reads as part of the
    // launch's own words, so it goes in before this module's last two lines.
    startup_lines.extend(load_lines);
    // The two lines this module owns -- which door answered, and whether the pair reached `llm.conf`
    // (§7's state) -- go in here, after the desktop's, so they read as the launch's own last words.
    startup_lines.insert(0, door);
    if remembered == Some(true) {
        startup_lines.push(format!(">>> remembered for next time: {}", opened.path().display()));
    }
    ui::window::note_startup(&startup_lines);
}

/// S2's other half: put this launch's project in `llm.conf` so the next launch's S2 can find it.
///
/// The key is the folder the program started in, which is exactly the key [`startup::last_project`]
/// reads with — deliberately NOT F0.9's per-opened-folder key (`ui::window::open_folder` remembers
/// those, and both doors write through the same [`machine_store::persist_project`]). The map starts
/// as the conf's own pairs so another root's memory survives this launch.
///
/// A blank session remembers nothing: S4 creates no file, and a path that does not exist would only
/// make the next S2 fall through anyway. A conf that cannot be read costs the memory, never the
/// or `None` when nothing was written at all; the caller logs that as the launch's own news.
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
    // The pair goes through the one door that writes `llm.conf`. Its answer about whether bytes changed
    // also says the write landed, which is what gets logged: a remembered launch is worth one line
    // (§7's state changing under the user's feet should be visible), and a failure is worth one too.
    match machine_store::persist_project(paths, &mut projects, root, &file) {
        Ok(changed) => Some(changed),
        Err(why) => {
            // The reason goes to the log (the window exists by now, so there is a sink) and back to the
            // caller as `None`, which is what keeps §1's named failure out of the "remembered" line.
            ui::window::note_startup(&[format!("!!! could not remember this project -- {why}")]);
            None
        }
    }
}

/// Where this machine keeps `llm.conf`, or `None` when there is nowhere to keep anything.
fn settings_paths() -> Option<settings::Paths> {
    let var = |name: &str| std::env::var(name).ok();
    // `data_home` has no environment variable of its own; settings.rs documents it as the caller's
    // override, so it stays `None` and `$XDG_DATA_HOME`/$HOME answer for the data folder.
    settings::paths_from(
        var("XDG_CONFIG_HOME").as_deref(),
        var("HOME").as_deref(),
        None,
        var("XDG_DATA_HOME").as_deref(),
    )
}
