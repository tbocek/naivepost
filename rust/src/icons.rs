//! Finding the icons, and telling the desktop they changed
//! (§03-shell#8-details-confirmed-against-the-code-verification-pass).
//!
//! The icon tree travels with the app — installed beside the binary, unpacked under
//! the app root, or simply here in a development checkout — so it is searched in
//! that order rather than configured. When the desktop entry or the MIME package is
//! (re)written the databases behind them have to be refreshed; that is a subprocess
//! and §8 wants it off the launch path, so [`Databases::refresh`] returns the
//! commands instead of running them: the caller spawns them in the background, and a
//! returned command list is testable without spawning anything.

use std::fs;
use std::path::{Path, PathBuf};

/// The folder holding the icon theme tree, wherever the app itself was found.
pub const ICONS_DIR: &str = "icons";

/// Where to look for [`ICONS_DIR`], in the order that wins: beside the binary (an
/// installed or AppImage build), under the app root (a prefix install), then the
/// working directory (a checkout run from `rust/`).
pub fn search_paths(exe_dir: &Path, app_root: &Path, cwd: &Path) -> Vec<PathBuf> {
    [exe_dir, app_root, cwd]
        .into_iter()
        .map(|dir| dir.join(ICONS_DIR))
        .collect()
}

/// The first icon tree that is actually there. `None` means the desktop entry names
/// an icon by id and the theme will simply not find it — a missing icon is not worth
/// failing a launch over.
pub fn found(exe_dir: &Path, app_root: &Path, cwd: &Path) -> Option<PathBuf> {
    search_paths(exe_dir, app_root, cwd)
        .into_iter()
        .find(|path| path.is_dir())
}

/// The themes in one `…/icons` folder — the same folder [`found`] answers, read one level down.
///
/// A theme is a folder with an `index.theme`, which is what GTK's own search requires; a stray
/// folder (or the `hicolor` tree, which holds the app icon rather than the interface's symbols)
/// is not a theme and is not returned.
pub fn themes(icons_dir: &Path) -> Vec<PathBuf> {
    let mut themes: Vec<PathBuf> = icons_dir
        .read_dir()
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|theme| theme.join("index.theme").is_file())
                .collect()
        })
        .unwrap_or_default();
    themes.sort();
    themes
}

/// The icon themes carried by the same build as `exe`, in [`search_paths`] order.
///
/// This is where the interface's symbols come from in a package: an AppImage puts its theme beside
/// the binary (see `release.sh`), so a missing symbolic icon is visible from the executable path
/// alone, with no display and no `$XDG_DATA_DIRS` to consult.
pub fn bundled_themes(exe: &Path, cwd: &Path) -> Vec<PathBuf> {
    let exe_dir = exe.parent().unwrap_or(Path::new(".")).to_path_buf();
    let app_root = exe_dir.parent().unwrap_or(Path::new(".")).to_path_buf();
    search_paths(&exe_dir, &app_root, cwd)
        .iter()
        .flat_map(|dir| themes(dir))
        .collect()
}

/// The `-symbolic` names the interface asks the theme for, spelled once so a missing
/// one is a list a test can walk rather than a string buried in a widget builder.
///
/// `gui/icons` ships the app icon only: everything else — the four tabs, the source
/// row's four symbols, the transport — is looked up in the desktop's theme, so an
/// icon set that cannot resolve this list draws the broken-icon placeholder in every
/// button. That is the difference between a checkout on a desktop and a package on a
/// bare machine, and the reason the packages carry a theme (see `release.sh`).
///
/// Kept in step with the code on purpose: a name added to a widget but not to this list
/// is a name nothing checks, and `just test icon_theme_symbols` is what keeps the two
/// together.
pub const NAMED_SYMBOLIC_ICONS: &[&str] = &[
    "view-list-symbolic",
    "edit-cut-symbolic",
    "audio-input-microphone-symbolic",
    "audio-speakers-symbolic",
    "media-record-symbolic",
    "applications-multimedia-symbolic",
    "camera-video-symbolic",
    "dialog-warning-symbolic",
    "document-new-symbolic",
    "document-open-symbolic",
    "document-save-symbolic",
    "emblem-system-symbolic",
    "folder-symbolic",
    "help-about-symbolic",
    "list-add-symbolic",
    "list-remove-symbolic",
    "media-playback-pause-symbolic",
    "media-playback-start-symbolic",
    "media-playback-stop-symbolic",
    "user-trash-symbolic",
    "view-refresh-symbolic",
];

/// Which of [`NAMED_SYMBOLIC_ICONS`] the themes in `icons_dir` cannot resolve, in list order.
///
/// The rule GTK follows: an icon lives in a theme's `index.theme` folders — for a symbolic icon,
/// in `symbolic/` at some size, either loose or under `apps/`. A name with no file anywhere is what
/// makes GTK draw the broken-icon picture, so this answers the packaging question ("does the tree
/// the build ships carry every name the app asks for?") without a display.
///
/// Named `_for_test` because the program never asks it: GTK resolves these names itself when a widget
/// draws, and the tree a package ships is checked by the release script running the AppImage.
pub fn missing_in_for_test(icons_dir: &Path) -> Vec<&'static str> {
    let themes = themes(icons_dir);
    NAMED_SYMBOLIC_ICONS
        .iter()
        .copied()
        .filter(|name| {
            !themes.iter().any(|theme| {
                icon_dirs(theme)
                    .into_iter()
                    .flat_map(|dir| {
                        [
                            dir.join(format!("{name}.svg")),
                            dir.join("apps").join(format!("{name}.svg")),
                        ]
                    })
                    .any(|file| file.is_file())
            })
        })
        .collect()
}

/// The folders inside one theme where an icon may sit, per its own `index.theme` when it can be read.
///
/// Only `Directories:` is parsed, which is enough: it is a flat list of folders relative to the
/// theme. A theme whose file cannot be read falls back to its own subfolders, since the answer only
/// decides whether a name counts as present.
fn icon_dirs(theme: &Path) -> Vec<PathBuf> {
    let listed: Vec<PathBuf> = fs::read_to_string(theme.join("index.theme"))
        .map(|text| {
            text.lines()
                .find_map(|line| line.strip_prefix("Directories="))
                .unwrap_or("")
                .split(';')
                .filter(|part| !part.is_empty())
                .map(|part| theme.join(part))
                .collect()
        })
        .unwrap_or_default();
    if !listed.is_empty() {
        return listed;
    }
    theme
        .read_dir()
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| path.is_dir())
                .collect()
        })
        .unwrap_or_default()
}
/// The desktop's databases, and whether this launch has already explained itself.
#[derive(Debug, Clone, Default)]
pub struct Databases {
    /// One line per launch naming the folder written into: the same path is logged
    /// again on every save otherwise, and a log that repeats itself stops being read.
    pub logged: bool,
}

impl Databases {
    pub fn new() -> Self {
        Self::default()
    }

    /// What to run after (re)writing the desktop entry or the MIME package, and what
    /// to put in the log — one line naming the folder written into, once across calls.
    /// Nothing is logged when neither file changed: there is nothing new for the
    /// databases to pick up.
    pub fn refresh(
        &mut self,
        wrote_desktop_entry: bool,
        wrote_mime_package: bool,
        path: &Path,
        log: &mut Vec<String>,
    ) -> Vec<Vec<String>> {
        let mut commands = Vec::new();
        for (wrote, tool) in [
            (wrote_mime_package, "update-mime-database"),
            (wrote_desktop_entry, "update-desktop-database"),
        ] {
            if !wrote {
                continue;
            }
            commands.push(vec![tool.to_string(), path.display().to_string()]);
            if !self.logged {
                log.push(format!(">>> told the desktop about {}", path.display()));
                self.logged = true;
            }
        }
        commands
    }
}
