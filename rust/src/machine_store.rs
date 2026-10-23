//! The program's own write side for the machine files of spec/01-project-and-files.md §7.
//!
//! [`crate::settings`] owns the FORMATS — how `llm.conf` is quoted, rendered, read and mode-set,
//! where a hang dump goes, what the desktop entry says. What it never did was get CALLED: its
//! writer ran only in tests, so the `PROJECT_<n>` pairs §7 says the conf holds were never
//! persisted, the legacy `settings.json` recovery was never taken up, a watchdog give-up wrote no
//! dump, and nothing installed the desktop entry or told the MIME and desktop databases about it.
//! This module is that missing caller: every function here is a door a real code path walks through
//! (startup, Open a project, the Settings dialog, F6.3's stall watch), and every one of them ends
//! in a settings.rs writer rather than in a second copy of its rules.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::checks;
use crate::icons;
use crate::open_project;
use crate::settings::{self, Conf, Paths};

/// The launch's settings read, with §7's two one-time jobs done on the way.
///
/// Order is §6's precedence: a `<root>/llm.conf` still beside the videos is copied into the config
/// folder first (only while the config folder has none of its own), then the conf is read, and only
/// then does the legacy `settings.json` get asked — which it is ONLY when the conf has no
/// `PROJECT_*` keys at all (§7's "read once … on the launch after the merge"). When it does answer,
/// the conf is written back immediately: that write is what makes "once" true on disk rather than a
/// rule to remember, because the next launch finds `PROJECT_*` keys and stops asking.
pub fn open_and_recover(paths: &Paths, root: &Path) -> Result<Conf, String> {
    // A migration that cannot be done must not cost the launch, which is the same rule main.rs states
    // for an unreadable conf: the file stays for the person to look at.
    let _ = settings::migrate_legacy(paths, root);
    let mut conf = settings::read(paths)?;
    if settings::recover_projects(&mut conf, paths)? {
        settings::save(paths, &conf)?;
    }
    Ok(conf)
}

/// Record this opened project against its root and put the pair in `llm.conf` (§7's state).
///
/// Returns whether the file changed. The numbering and the byte-identical-when-unchanged property
/// come from [`settings::render`] sorting the pairs by folder, so nothing here sorts or numbers
/// anything; the map is the caller's (the window keeps one per launch) and is updated through
/// [`open_project::remember`], the same data-level door F0.9 S4 uses.
pub fn persist_project(
    paths: &Paths,
    projects: &mut BTreeMap<String, String>,
    root: &Path,
    file: &Path,
) -> Result<bool, String> {
    open_project::remember(projects, root, file);
    // Read-modify-write, so a key this build does not know survives the edit. With no conf readable
    // there is nothing to modify, and writing a default over an unreadable file would erase settings
    // nobody asked to lose — so the pair simply is not remembered this launch.
    let mut conf = settings::read(paths)?;
    conf.projects.clone_from(projects);
    settings::save(paths, &conf)
}

/// The dialog's pending write: what was typed, when the beat is due, and what the file said when the
/// dialog opened.
///
/// §5 gives the Settings dialog no Save button: every box is written 600 ms after the last keystroke
/// and again on close. [`checks::Draft`] already holds that beat ([`checks::CONF_SAVE_WAIT`]) and its
/// caches-invalidated-on-save rule; what it could not do was reach the file, so this struct pairs it
/// with the [`Paths`] and the base [`Conf`] and does the one write both doors go through.
pub struct Store {
    paths: Paths,
    base: Conf,
    draft: checks::Draft,
}

impl Store {
    /// Open the store over the conf as it stands right now — the values the boxes are seeded from.
    pub fn open(paths: &Paths) -> Result<Self, String> {
        Ok(Self {
            paths: paths.clone(),
            base: settings::read(paths)?,
            draft: checks::Draft::default(),
        })
    }

    /// The conf the boxes were seeded from, so the dialog can show what is on disk rather than blanks.
    pub fn value_for(&self, key: &str) -> Option<&str> {
        field_of(key).map(|get| get(&self.base))
    }

    /// The conf behind the boxes, which is what a spin seeds from (a slot count is a number, not a
    /// string field, so it cannot ride [`Self::value_for`]).
    pub fn base_conf(&self) -> Conf {
        self.base.clone()
    }

    /// A keystroke: the pending write is pushed back, so typing a whole address writes once.
    pub fn touched(&mut self, now: Duration) {
        self.draft.state.touched(now);
    }

    /// When the timer may fire, if a write is owed at all.
    pub fn due_at(&self) -> Option<Duration> {
        self.draft.state.due_at()
    }

    /// The beat elapsed: write what is in the boxes now. `values` is every box's key and current
    /// text, read by the caller out of its widgets — the dialog owns the widgets, this owns the file.
    /// Answers the line §5 puts in the log when bytes actually changed, `None` when they did not.
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

    /// Closing the window: write if a beat has not elapsed, because losing the last thing typed is
    /// the one failure this dialog must not have.
    pub fn on_close(&mut self, values: &[(String, String)]) -> Result<Option<String>, String> {
        if !self.draft.close() {
            return Ok(None);
        }
        self.write(values)
    }

    /// The one write both doors share: apply every recognised box onto a fresh copy of the base conf
    /// and hand the whole store to settings' writer, which quotes it, compares the bytes and sets the
    /// modes. Unrecognised keys (a row that stores nothing, like Vision) are skipped, not refused.
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

/// The conf field a dialog box edits, read side. `None` for a box that stores nothing in the conf —
/// Vision tests the model above it rather than holding its own id.
pub fn field_of(key: &str) -> Option<fn(&Conf) -> &str> {
    Some(match key {
        "server" => |c: &Conf| &c.server,
        "llm-key" => |c: &Conf| &c.key,
        "llm" => |c: &Conf| &c.model,
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
    // A spin's value travels under `<model key>-slots`; clamping here is what makes 99 typed into a
    // spin land on disk as 16 rather than as a queue the server has to absorb.
    if let Some(field) = slot_field(&mut conf.slots, key) {
        let asked = value.trim().parse::<u32>().unwrap_or(SLOT_MIN);
        *field = asked.clamp(SLOT_MIN, SLOT_MAX);
        return true;
    }
    let slot: &mut String = match key {
        "server" => &mut conf.server,
        "llm-key" => &mut conf.key,
        "llm" => &mut conf.model,
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

/// The span §5 gives a Slots spin: 1 to 16, default 1.
pub const SLOT_MIN: u32 = 1;
pub const SLOT_MAX: u32 = 16;

/// The seven model boxes that carry a slot count, as their dialog key.
pub const SLOT_KEYS: [&str; 7] = ["llm", "tts", "asr", "diar", "sep", "aligner", "sd"];

/// The store name of one model box's slot count. A spin shares its row's key with the text box beside
/// it, so the two are told apart here rather than by renaming either widget.
pub fn slot_key(model_key: &str) -> String {
    format!("{model_key}-slots")
}

/// Which `Slots` field a store name writes, when the name is one of the seven.
fn slot_field<'s>(slots: &'s mut settings::Slots, name: &str) -> Option<&'s mut u32> {
    Some(match name {
        "llm-slots" => &mut slots.llm,
        "tts-slots" => &mut slots.tts,
        "asr-slots" => &mut slots.asr,
        "diar-slots" => &mut slots.diar,
        "sep-slots" => &mut slots.sep,
        // §5 lists the forced aligner among the model boxes; its count is `Slots::align`.
        "aligner-slots" => &mut slots.align,
        "sd-slots" => &mut slots.sd,
        _ => return None,
    })
}

/// The count a model box's spin shows for this conf. Absent from the file means 1 (§6: "absent = 1"),
/// which is why this answers for every slot key and never `None`.
pub fn slot_of_key(conf: &Conf, model_key: &str) -> u32 {
    // Read through the write side's own table, so a key added there cannot be missed here; the clamp in
    // `set_field` means whatever it writes is already inside §5's 1..=16.
    let name = slot_key(model_key);
    let mut slots = settings::Slots::default();
    match slot_field(&mut slots, &name) {
        Some(field) => *field = conf_slot_text(conf, &name),
        None => return SLOT_MIN,
    }
    slot_field(&mut slots, &name).copied().unwrap_or(SLOT_MIN)
}

/// The number `conf` carries for one slot key — `read` has already parsed the `*_SLOTS` pair into
/// `Slots`, so the seed side only has to pick the right field out of it.
fn conf_slot_text(conf: &Conf, name: &str) -> u32 {
    match name {
        "llm-slots" => conf.slots.llm,
        "tts-slots" => conf.slots.tts,
        "asr-slots" => conf.slots.asr,
        "diar-slots" => conf.slots.diar,
        "sep-slots" => conf.slots.sep,
        "aligner-slots" => conf.slots.align,
        "sd-slots" => conf.slots.sd,
        _ => SLOT_MIN,
    }
}

/// What a box shows for `key`: the conf's own text, or a slot count when the key NAMES A SPIN.
///
/// One model row carries two controls under one dialog key — the id box and its Slots spin — so the
/// store tells them apart by the STORE-side name (`slot_key`'s `<model>-slots`) and never by the plain
/// key: a plain `tts` must stay the TTS model id, or every model box would show its slot count.
pub fn text_for(conf: &Conf, key: &str) -> Option<String> {
    if let Some(model_key) = key.strip_suffix("-slots") {
        if SLOT_KEYS.contains(&model_key) {
            return Some(slot_of_key(conf, model_key).to_string());
        }
    }
    field_of(key).map(|get| get(conf).to_string())
}

/// Write one watchdog dump (`hang-*.txt`, §7) naming the component that stopped answering.
///
/// `unresponsive` is the silence the stall watch measured, in seconds. Errors are swallowed to
/// `None`: principle 8 — a failure to record is a log line at most, never a failed step, and the
/// give-up that called this has already said its piece. One dump per stall second: a stamp already
/// on disk means another dump would only bury the first, so nothing more is written.
pub fn note_hang(component: &str, unresponsive: Duration) -> Option<PathBuf> {
    let paths = settings::from_environment()?;
    let stamp = local_stamp();
    if paths.has_hang_dump(&stamp) {
        return None;
    }
    settings::write_hang_dump(&paths, component, std::process::id(), unresponsive, &stamp).ok()
}

/// Today's local date and time as `MMDD-HHMMSS`, the stamp [`settings::hang_stamp`] wants — month
/// first so the dumps sort in the order they happened. Local, not UTC: a dump is read by whoever was
/// watching the clock when the app stalled.
pub fn local_stamp() -> String {
    // Seconds since the epoch, turned into the LOCAL calendar by glib (std has no local offset).
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let Ok(local) = glib::DateTime::from_unix_local(now.as_secs() as i64) else {
        // No local zone to speak of: a dump still gets written, stamped from the epoch.
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

/// Whether this launch runs inside a Flatpak, whose manifest exports both files itself (§7): a
/// desktop entry written from inside the sandbox would name a path that only exists in there.
pub fn inside_flatpak() -> bool {
    std::env::var("FLATPAK_ID").is_ok() || Path::new("/.flatpak-info").exists()
}

/// Whether `exe` is being run out of a build cache rather than installed. `cargo run` and
/// `just build` leave the binary under a `target/` directory, and an entry pointing there would
/// offer the app until somebody types `cargo clean`.
pub fn in_build_cache(exe: &Path) -> bool {
    exe.components().any(|part| part.as_os_str() == "target")
}

/// Install the desktop entry and the MIME package when this launch may (§7), and say what the
/// desktop's databases now need re-running.
///
/// Returns the log lines plus the commands, in the shape [`icons::Databases::refresh`] expects:
/// neither file counts as "written" when its bytes already match, which is what keeps a launch from
/// re-telling the databases the same thing every time the app opens. Deciding whether to install at
/// all is [`settings::wants_desktop_file`]'s rule; the two cases it cannot see from here — running
/// inside a Flatpak, and running out of a build cache — are read by [`inside_flatpak`] and
/// [`in_build_cache`] and passed in, because a test must be able to prove both refuse without the
/// machine they run on having to be a Flatpak.
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
    // The folder each database is told about is its own: `applications` for the entry, `mime` for
    // the package. `refresh` logs one line for the first, so it is handed the mime folder (the one
    // whose type decides whether a file manager offers naivepost at all).
    let commands = databases.refresh(
        wrote_entry,
        wrote_mime,
        &paths.data_dir.join("mime"),
        &mut log,
    );
    (log, commands)
}

/// Run the desktop's refresh tools in the background (§7: the app must start whether or not they
/// exist) and answer a log line for each one that could not be started.
///
/// Both are `freedesktop-utils`/`desktop-file-utils` binaries; a minimal system has one and not the
/// other, and a missing helper is worth one line, not a failed launch. Their output is nobody's
/// input, so they are spawned with piped stdio and left — the databases are small and the tools exit
/// on their own.
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

/// Whether a file needs writing: absent, unreadable, or different bytes. Unreadable counts as needing
/// a write — a file we cannot compare is one the desktop cannot use either.
fn differs(path: &Path, body: &str) -> bool {
    std::fs::read_to_string(path)
        .map(|text| text != body)
        .unwrap_or(true)
}
