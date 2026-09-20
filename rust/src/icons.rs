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
