//! The program's own write side for the machine files of spec/01-project-and-files.md §7.
//!
//! [`crate::settings`] owns the FORMATS; this is the caller. Every function is a door a real code path walks
//! through (startup, Open a project, the Settings dialog, F6.3's stall watch) and ends in a settings.rs
//! writer rather than a second copy of its rules.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::checks;
use crate::icons;
use crate::open_project;
use crate::settings::{self, Conf, Paths};

/// The launch's settings read, with §7's two one-time jobs done on the way.
///
/// Order is §6's precedence: a `<root>/llm.conf` beside the videos is copied into the config folder first
/// (only while that has none), then the conf is read, and only then is the legacy `settings.json` asked,
/// and ONLY when the conf has no `PROJECT_*` keys (§7's "read once"). When it answers, the conf is written
/// back immediately, so "once" is true on disk.
pub fn open_and_recover(paths: &Paths, root: &Path) -> Result<Conf, String> {
    // A migration that cannot be done must not cost the launch; the file stays for the person to look at.
    let _ = settings::migrate_legacy(paths, root);
    let mut conf = settings::read(paths)?;
    if settings::recover_projects(&mut conf, paths)? {
        settings::save(paths, &conf)?;
    }
    Ok(conf)
}

/// Record this opened project against its root and put the pair in `llm.conf` (§7's state).
///
/// Returns whether the file changed. Ordering and byte-identical-when-unchanged come from [`settings::render`]
/// sorting by folder; the map is the caller's, updated through [`open_project::remember`].
pub fn persist_project(
    paths: &Paths,
    projects: &mut BTreeMap<String, String>,
    root: &Path,
    file: &Path,
) -> Result<bool, String> {
    open_project::remember(projects, root, file);
    // Read-modify-write, so unknown keys survive. With no conf readable, writing a default over an unreadable
    // file would erase settings, so the pair is not remembered this launch.
    let mut conf = settings::read(paths)?;
    // The pair goes INTO the file's own list: replacing it with the caller's map (only what this launch opened)
    // erased the remembered project of every other folder.
    open_project::remember(&mut conf.projects, root, file);
    settings::save(paths, &conf)
}

/// The dialog's pending write: what was typed, when the beat is due, and what the file said at open.
///
/// §5 gives the Settings dialog no Save button: every box is written 600 ms after the last keystroke and on
/// close. [`checks::Draft`] holds that beat ([`checks::CONF_SAVE_WAIT`]); this pairs it with the [`Paths`]
/// and the base [`Conf`] and does the one write both doors go through.
pub struct Store {
    paths: Paths,
    base: Conf,
    draft: checks::Draft,
}

impl Store {
    /// Open the store over the conf as it stands now, the values the boxes are seeded from.
    pub fn open(paths: &Paths) -> Result<Self, String> {
        Ok(Self {
            paths: paths.clone(),
            base: settings::read(paths)?,
            draft: checks::Draft::default(),
        })
    }

    /// The conf the boxes were seeded from.
    pub fn value_for(&self, key: &str) -> Option<&str> {
        field_of(key).map(|get| get(&self.base))
    }

    /// The conf behind the boxes.
    pub fn base_conf(&self) -> Conf {
        self.base.clone()
    }

    /// Change the conf behind the boxes (a server entry picked in §5's dropdown: its settings, boxless ones
    /// included, become what the next write puts in `llm.conf`).
    pub fn adopt(&mut self, change: impl FnOnce(&mut Conf)) {
        change(&mut self.base);
    }

    /// A keystroke: the pending write is pushed back, so typing a whole address writes once.
    pub fn touched(&mut self, now: Duration) {
        self.draft.state.touched(now);
    }

    /// When the timer may fire, if a write is owed at all.
    pub fn due_at(&self) -> Option<Duration> {
        self.draft.state.due_at()
    }

    /// The beat elapsed: write what is in the boxes now. `values` is every box's key and text, read by the
    /// caller. Answers the line §5 puts in the log when bytes changed, `None` otherwise.
    pub fn flush(
        &mut self,
        now: Duration,
        values: &[(String, String)],
    ) -> Result<Option<String>, String> {
        if !self.draft.save(now) {
            return Ok(None);
        }
        self.write(values)
    }

    /// Closing the window: write if a beat has not elapsed (losing the last thing typed is the one failure to avoid).
    pub fn on_close(&mut self, values: &[(String, String)]) -> Result<Option<String>, String> {
        if !self.draft.close() {
            return Ok(None);
        }
        self.write(values)
    }

    /// The one write both doors share: apply every recognised box onto a fresh copy of the base conf and hand
    /// it to settings' writer. Unrecognised keys (a row that stores nothing, like Vision) are skipped.
    fn write(&mut self, values: &[(String, String)]) -> Result<Option<String>, String> {
        let mut next = self.base.clone();
        for (key, value) in values {
            set_field(&mut next, key, value);
        }
        let changed = settings::save(&self.paths, &next)?;
        self.base = next;
        Ok(changed.then(|| format!("settings saved to {}", self.paths.conf_path().display())))
    }
}

/// The conf field a dialog box edits, read side. `None` for a box that stores nothing in the conf.
pub fn field_of(key: &str) -> Option<fn(&Conf) -> &str> {
    Some(match key {
        "server" => |c: &Conf| &c.server,
        "llm-key" => |c: &Conf| &c.key,
        "audio-key" => |c: &Conf| &c.audio_key,
        "sd-key" => |c: &Conf| &c.sd_key,
        "llm" => |c: &Conf| &c.model,
        "decision" => |c: &Conf| &c.decision_model,
        "ffmpeg" => |c: &Conf| &c.ffmpeg,
        "firefox" => |c: &Conf| &c.firefox,
        "audio" => |c: &Conf| &c.audio_server,
        "tts" => |c: &Conf| &c.tts_model,
        "asr" => |c: &Conf| &c.asr_model,
        "diar" => |c: &Conf| &c.diar_model,
        "sep" => |c: &Conf| &c.sep_model,
        "aligner" => |c: &Conf| &c.align_model,
        "sd" => |c: &Conf| &c.sd_server,
        _ => return None,
    })
}

/// Write one dialog box's value into its conf field. False when the key stores nothing.
pub fn set_field(conf: &mut Conf, key: &str, value: &str) -> bool {
    let slot: &mut String = match key {
        "server" => &mut conf.server,
        "llm-key" => &mut conf.key,
        "audio-key" => &mut conf.audio_key,
        "sd-key" => &mut conf.sd_key,
        "llm" => &mut conf.model,
        "decision" => &mut conf.decision_model,
        "ffmpeg" => &mut conf.ffmpeg,
        "firefox" => &mut conf.firefox,
        "audio" => &mut conf.audio_server,
        "tts" => &mut conf.tts_model,
        "asr" => &mut conf.asr_model,
        "diar" => &mut conf.diar_model,
        "sep" => &mut conf.sep_model,
        "aligner" => &mut conf.align_model,
        "sd" => &mut conf.sd_server,
        _ => return false,
    };
    *slot = value.to_string();
    true
}

/// What a box shows for `key`: the conf's own text, `None` for a box that stores nothing.
pub fn text_for(conf: &Conf, key: &str) -> Option<String> {
    field_of(key).map(|get| get(conf).to_string())
}

/// Write one watchdog dump (`hang-*.txt`, §7) naming the component that stopped answering.
///
/// `unresponsive` is the silence the stall watch measured, in seconds. Errors are swallowed to `None`
/// (principle 8). One dump per stall second: a stamp already on disk means another would bury the first.
pub fn note_hang(component: &str, unresponsive: Duration) -> Option<PathBuf> {
    let paths = settings::from_environment()?;
    let stamp = local_stamp();
    if paths.has_hang_dump(&stamp) {
        return None;
    }
    settings::write_hang_dump(&paths, component, std::process::id(), unresponsive, &stamp).ok()
}

/// Today's local date and time as `MMDD-HHMMSS`, the stamp [`settings::hang_stamp`] wants. Month first so
/// dumps sort in order; local, not UTC, since a dump is read by whoever watched the clock.
pub fn local_stamp() -> String {
    // Seconds since the epoch, turned into the LOCAL calendar by glib (std has no local offset).
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let Ok(local) = glib::DateTime::from_unix_local(now.as_secs() as i64) else {
        // No local zone: a dump still gets written, stamped from the epoch.
        return settings::hang_stamp(1, 1, 0, 0, 0);
    };
    settings::hang_stamp(
        local.month() as u32,
        local.day_of_month() as u32,
        local.hour() as u32,
        local.minute() as u32,
        local.second() as u32,
    )
}

/// Whether this launch runs inside a Flatpak, whose manifest exports both files itself (§7).
/// A desktop entry written from inside the sandbox would name a path that only exists there.
pub fn inside_flatpak() -> bool {
    std::env::var("FLATPAK_ID").is_ok() || Path::new("/.flatpak-info").exists()
}

/// Whether `exe` is being run out of a build cache (`target/`), where an entry would offer the app until `cargo clean`.
pub fn in_build_cache(exe: &Path) -> bool {
    exe.components().any(|part| part.as_os_str() == "target")
}

/// Install the desktop entry and the MIME package when this launch may (§7), and say what the desktop's
/// databases now need re-running.
///
/// Returns the log lines plus the commands, in the shape [`icons::Databases::refresh`] expects; a file whose
/// bytes already match is not "written", so a launch does not re-tell the databases. Whether to install is
/// [`settings::wants_desktop_file`]'s rule; Flatpak and build-cache are passed in ([`inside_flatpak`],
/// [`in_build_cache`]) so a test can prove both refuse.
pub fn register_with_desktop(
    paths: &Paths,
    exe: &Path,
    flatpak: bool,
    build_cache: bool,
    databases: &mut icons::Databases,
) -> (Vec<String>, Vec<Vec<String>>) {
    let wanted = settings::wants_desktop_file(exe, flatpak, build_cache);
    let wrote_entry = wanted && differs(&paths.desktop_file(), &settings::desktop_file(exe));
    if wrote_entry {
        let _ = settings::install_desktop_file(paths, exe);
    }
    let wrote_mime = wanted && differs(&paths.mime_package(), &settings::mime_package());
    if wrote_mime {
        let _ = settings::install_mime_package(paths);
    }
    let mut log = Vec::new();
    // `refresh` logs one line for the first folder, so it is handed the mime folder (its type decides whether
    // a file manager offers naivepost at all).
    let commands = databases.refresh(
        wrote_entry,
        wrote_mime,
        &paths.data_dir.join("mime"),
        &mut log,
    );
    (log, commands)
}

/// Run the desktop's refresh tools in the background (§7: the app must start whether or not they exist)
/// and answer a log line for each one that could not be started.
///
/// A minimal system may lack one of the two binaries; that is a line, not a failed launch. Their output is
/// nobody's input, so they are spawned with piped stdio and left to exit.
pub fn run_refresh(commands: &[Vec<String>]) -> Vec<String> {
    let mut log = Vec::new();
    for command in commands {
        let Some((program, rest)) = command.split_first() else {
            continue;
        };
        let args: Vec<&str> = rest.iter().map(String::as_str).collect();
        if let Err(err) = crate::subprocess::spawn_piped(program, &args) {
            log.push(format!(
                "{} failed: {err}",
                crate::subprocess::shown(program, &args)
            ));
        }
    }
    log
}

/// Whether a file needs writing: absent, unreadable (we cannot compare it, the desktop cannot use it), or different.
fn differs(path: &Path, body: &str) -> bool {
    std::fs::read_to_string(path)
        .map(|text| text != body)
        .unwrap_or(true)
}
