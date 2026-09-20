//! The machine's own files, spec/01-project-and-files.md §7: `llm.conf` and its legacy
//! `settings.json`, the voices folder, prompts edited on this machine, and the watchdog
//! dumps. None of them live in a project, so nothing here goes through
//! [`crate::layout`] — every function is handed the directories it needs, and reading the
//! environment happens in one place ([`paths_from`]) so a test can pass temp folders and
//! never touch the real `$HOME`.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Where this machine's naivepost files live.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    /// `~/.config/naivepost` — the conf, the legacy settings, the prompts, the dumps.
    pub config_dir: PathBuf,
    /// `~/.local/share/naivepost` — the voice library when there is no other.
    pub data_dir: PathBuf,
}

/// The two folders, from the environment variables a caller read for us.
///
/// A relative `XDG_*` value is not a path and is ignored, as the prototype ignores it with
/// `filepath.IsAbs`: a config location that depends on the current directory would make the
/// program read a different machine's settings depending on where it was started from.
/// `None` means there is nowhere to keep anything — the prototype's "nowhere to write the
/// settings -- neither XDG_CONFIG_HOME nor HOME is set".
fn absolute(v: Option<&str>) -> Option<&str> {
    // A relative value is not a path; see [`paths_from`].
    v.filter(|v| Path::new(v).is_absolute())
}

pub fn paths_from(
    config_home: Option<&str>,
    home: Option<&str>,
    data_home: Option<&str>,
    xdg_data: Option<&str>,
) -> Option<Paths> {
    let config_root = absolute(config_home)
        .map(PathBuf::from)
        .or_else(|| absolute(home).map(|h| Path::new(h).join(".config")))?;
    let data_root = absolute(data_home)
        .map(PathBuf::from)
        .or_else(|| absolute(xdg_data).map(PathBuf::from))
        .or_else(|| absolute(home).map(|h| Path::new(h).join(".local/share")))?;
    Some(Paths {
        config_dir: config_root.join("naivepost"),
        data_dir: data_root.join("naivepost"),
    })
}

/// The two folders from this process's own environment — what a caller with no override of its
/// own reads. `main::settings_paths` is the same three variables; a widget that has to reach a
/// prompt file uses this rather than reaching into the binary's innards.
pub fn from_environment() -> Option<Paths> {
    let var = |name: &str| std::env::var(name).ok();
    paths_from(
        var("XDG_CONFIG_HOME").as_deref(),
        var("HOME").as_deref(),
        None,
        var("XDG_DATA_HOME").as_deref(),
    )
}

impl Paths {
    /// `llm.conf` — the one settings file, and the state that goes with it.
    pub fn conf_path(&self) -> PathBuf {
        self.config_dir.join("llm.conf")
    }

    /// The legacy `settings.json`: read once, never written (§7).
    pub fn settings_path(&self) -> PathBuf {
        self.config_dir.join("settings.json")
    }

    pub fn prompts_dir(&self) -> PathBuf {
        self.config_dir.join("prompts")
    }

    /// A prompt edited on this machine, which exists only while it differs from the
    /// shipped text (§7).
    pub fn prompt_file(&self, key: &str) -> PathBuf {
        self.prompts_dir().join(format!("{key}.txt"))
    }

    /// A watchdog dump beside the settings (§7): `hang-*.txt`, one per hang, so a second
    /// stall never overwrites the record of the first.
    pub fn hang_path(&self, stamp: &str) -> PathBuf {
        self.config_dir.join(hang_file(stamp))
    }

    /// The desktop entry, under `$XDG_DATA_HOME/applications` (§7).
    pub fn desktop_file(&self) -> PathBuf {
        self.data_dir.join("applications/ch.bocek.naivepost.desktop")
    }

    /// The MIME package declaring the project type (§7).
    pub fn mime_package(&self) -> PathBuf {
        self.data_dir.join("mime/packages/ch.bocek.naivepost.xml")
    }
}

/// How many requests a server may take at once. Every one defaults to 1 — §6 of
/// 03-shell.md spells that as "absent = 1" — and is written only when it says otherwise, so
/// a machine left on the defaults does not look different after every save.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Slots {
    pub llm: u32,
    pub asr: u32,
    pub diar: u32,
    pub align: u32,
    pub tts: u32,
    pub sep: u32,
    pub sd: u32,
}

impl Default for Slots {
    fn default() -> Self {
        Self { llm: 1, asr: 1, diar: 1, align: 1, tts: 1, sep: 1, sd: 1 }
    }
}

impl Slots {
    /// The seven keys, in the order they are written.
    pub const KEY_NAMES: [&'static str; 7] = [
        "LLM_SLOTS",
        "AUDIOCPP_ASR_SLOTS",
        "AUDIOCPP_DIAR_SLOTS",
        "AUDIOCPP_ALIGN_SLOTS",
        "AUDIOCPP_TTS_SLOTS",
        "AUDIOCPP_SEP_SLOTS",
        "SD_SLOTS",
    ];

    const KEYS: [(&'static str, fn(&Slots) -> u32); 7] = [
        ("LLM_SLOTS", |s| s.llm),
        ("AUDIOCPP_ASR_SLOTS", |s| s.asr),
        ("AUDIOCPP_DIAR_SLOTS", |s| s.diar),
        ("AUDIOCPP_ALIGN_SLOTS", |s| s.align),
        ("AUDIOCPP_TTS_SLOTS", |s| s.tts),
        ("AUDIOCPP_SEP_SLOTS", |s| s.sep),
        ("SD_SLOTS", |s| s.sd),
    ];
}

/// The remembered project list: a session folder and the project file last open in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectRef {
    /// `PROJECT_<n>_ROOT`.
    pub root: String,
    /// `PROJECT_<n>_FILE`.
    pub file: String,
}

/// The endpoints, the local tools and the remembered state, as `llm.conf` holds them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Conf {
    pub server: String,
    pub model: String,
    /// A credential: this is why the file is 0600.
    pub key: String,
    pub audio_server: String,
    pub audio_key: String,
    /// The voice library — the one folder read outside a project (§7).
    pub voices: String,
    pub asr_model: String,
    pub diar_model: String,
    pub tts_model: String,
    pub sep_model: String,
    pub align_model: String,
    pub ffmpeg: String,
    pub firefox: String,
    pub sd_server: String,
    pub sd_key: String,
    /// The `code:tag:name` list of 00-principles.md §6.
    pub subtitle_languages: String,
    /// Keys this build does not know, kept in file order so a read-modify-write cannot
    /// erase a setting a newer build put here.
    unknown: Vec<(String, String)>,
    pub slots: Slots,
    /// The project file last open in each session folder, keyed by folder because every
    /// path inside a project is relative to it. Ordered, so an unchanged save is
    /// byte-identical (§7).
    pub projects: BTreeMap<String, String>,
}

impl Conf {
    const KEYS: [(&'static str, fn(&Conf) -> &str); 15] = [
        ("LLM_SERVER", |c| &c.server),
        ("LLM_MODEL", |c| &c.model),
        ("LLM_API_KEY", |c| &c.key),
        ("AUDIOCPP_SERVER", |c| &c.audio_server),
        ("AUDIOCPP_API_KEY", |c| &c.audio_key),
        ("AUDIOCPP_VOICES", |c| &c.voices),
        ("AUDIOCPP_ASR_MODEL", |c| &c.asr_model),
        ("AUDIOCPP_DIAR_MODEL", |c| &c.diar_model),
        ("AUDIOCPP_TTS_MODEL", |c| &c.tts_model),
        ("AUDIOCPP_SEP_MODEL", |c| &c.sep_model),
        ("AUDIOCPP_ALIGN_MODEL", |c| &c.align_model),
        ("FFMPEG", |c| &c.ffmpeg),
        ("FIREFOX", |c| &c.firefox),
        ("SD_SERVER", |c| &c.sd_server),
        ("SD_API_KEY", |c| &c.sd_key),
    ];

    /// The remembered projects, ordered by folder — the order `PROJECT_<n>` is numbered in.
    pub fn projects(&self) -> Vec<ProjectRef> {
        remembered(&self.projects)
    }

    /// The remembered projects, as numbered pairs (§7).
    pub fn remembered(&self) -> String {
        if self.projects.is_empty() {
            return String::new();
        }
        let mut out = String::from(
            "\n# The project file last open in each session folder, and the folder it belongs\n\
             # to. Keyed by folder because every path inside a project is relative to it:\n\
             # opening last night's project from a different folder would resolve its\n\
             # sources against the wrong directory. Delete a pair to forget one.\n",
        );
        for (n, (root, file)) in self.projects.iter().enumerate() {
            out.push_str(&format!(
                "PROJECT_{}_ROOT={}\nPROJECT_{}_FILE={}\n",
                n + 1,
                quote(root),
                n + 1,
                quote(file)
            ));
        }
        out
    }
}

/// The whole file: bash-sourceable `KEY="value"` lines.
pub fn render(conf: &Conf) -> String {
    let mut out = String::from(
        "# Endpoints and local tools used by the pipeline (written by the settings dialog).\n\
         # Bash-sourceable -- keep this file chmod 600, the key is a credential.\n",
    );
    for (key, get) in Conf::KEYS {
        // A cleared box is written back as the shipped default for five keys only;
        // everything else goes out as typed, empty included (§8).
        let value = written_value(key, get(conf), shipped_default(key));
        out.push_str(&format!("{key}={}\n", quote(&value)));
    }
    // Always written: an empty list is a setting the user can clear on the dialog, and a key
    // that vanished would read as "never asked about".
    out.push_str(&format!(
        "SUBTITLE_LANGUAGES={}\n",
        quote(&conf.subtitle_languages)
    ));
    for (key, get) in Slots::KEYS {
        let value = get(&conf.slots);
        // "absent = 1" (03-shell.md §6): writing the default would make every default
        // machine look like it had been configured.
        if value != 1 {
            out.push_str(&format!("{key}={value}\n"));
        }
    }
    for (key, value) in &conf.unknown {
        out.push_str(&format!("{key}={}\n", quote(value)));
    }
    out.push_str(&conf.remembered());
    out
}

/// Quote one value for a double-quoted bash string: backslash, the quote itself, `$`, a
/// backtick, a carriage return and a newline — and nothing else.
///
/// Only those six, because the file has to survive two readers at once: `source` in a shell,
/// which expands `$VAR`, backticks and line breaks, and this module's own line-based read,
/// which must not see a value that ran over the end of its line. Everything else — spaces,
/// tabs, `#` mid-value, unicode — is already safe inside double quotes and quoting it would
/// make the file harder for a human to edit by hand.
pub fn quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '$' => out.push_str("\\$"),
            '`' => out.push_str("\\`"),
            '\r' => out.push_str("\\r"),
            '\n' => out.push_str("\\n"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The six sequences [`quote`] writes, back again — plus the pair of quotes around the whole
/// value, which is what makes the file bash-sourceable. A hand-written line may leave them
/// off (`KEY=value`), so they are stripped only when both are there.
fn unquote(value: &str) -> String {
    let inner = value
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .unwrap_or(value);
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('\\') => out.push('\\'),
            Some('"') => out.push('"'),
            Some('$') => out.push('$'),
            Some('`') => out.push('`'),
            Some('r') => out.push('\r'),
            Some('n') => out.push('\n'),
            // An escape this build does not know: keep both characters rather than
            // inventing a meaning for it.
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// Read the file's text. Comments (a line whose first non-space character is `#`) and blank
/// lines are skipped, `KEY="value"` and bare `KEY=value` both parse, and a key this build
/// does not know is kept so that reading and writing a file cannot erase a setting a newer
/// build put there.
pub fn parse(text: &str) -> Result<Conf, String> {
    let mut conf = Conf::default();
    // Projects arrive as numbered pairs, which are only whole once the whole file has been
    // seen: ROOT and FILE may be a line apart.
    let mut roots: BTreeMap<u32, String> = BTreeMap::new();
    let mut files: BTreeMap<u32, String> = BTreeMap::new();

    for (n, line) in text.lines().enumerate() {
        let line = line.trim_end_matches('\r');
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((key, value)) = trimmed.split_once('=') else {
            return Err(format!("line {}: {line:?} is not KEY=value", n + 1));
        };
        let key = key.trim();
        let value = unquote(value.trim());
        // The named settings first, then the slots, then the remembered projects. Each
        // arm takes the value by reference so only the unknown-key arm has to move it.
        if set_key(&mut conf, key, &value) {
            continue;
        }
        if key == "SUBTITLE_LANGUAGES" {
            conf.subtitle_languages = value;
            continue;
        }
        if Slots::KEYS.iter().any(|(k, _)| *k == key) {
            let parsed = value.parse::<u32>().map_err(|_| {
                format!("line {}: {key}={value:?} is not a number", n + 1)
            })?;
            set_slot(&mut conf.slots, key, parsed);
            continue;
        }
        if key.starts_with("PROJECT_") {
            let (number, which) = pair_key(key).ok_or_else(|| {
                format!("line {}: {key} is not PROJECT_<n>_ROOT or _FILE", n + 1)
            })?;
            match which {
                Which::Root => drop(roots.insert(number, value)),
                Which::File => drop(files.insert(number, value)),
            }
            continue;
        }
        // A setting from an older build: read and dropped (§8).
        if legacy_key(key) {
            continue;
        }
        // A setting from a newer build: carried through untouched.
        conf.unknown.push((key.to_string(), value));
    }

    // Numbered pairs by number; a ROOT with no FILE is still a remembered folder whose
    // last file is unknown, not a broken entry to throw away.
    for (number, root) in roots {
        conf.projects
            .insert(root, files.remove(&number).unwrap_or_default());
    }
    Ok(conf)
}

enum Which {
    Root,
    File,
}

/// `PROJECT_12_ROOT` to `(12, Root)`.
fn pair_key(key: &str) -> Option<(u32, Which)> {
    let rest = key.strip_prefix("PROJECT_")?;
    if let Some(number) = rest.strip_suffix("_ROOT") {
        return number.parse().ok().map(|n| (n, Which::Root));
    }
    let number = rest.strip_suffix("_FILE")?;
    Some((number.parse().ok()?, Which::File))
}

/// Assign one of the named settings. True when the key was one of ours.
fn set_key(conf: &mut Conf, key: &str, value: &str) -> bool {
    // The keys 03-shell.md §6 lists as belonging to this file. A key that is NOT here is a
    // newer build's setting and goes to `Conf::unknown` untouched.
    let field = match key {
        "LLM_SERVER" => &mut conf.server,
        "LLM_MODEL" => &mut conf.model,
        "LLM_API_KEY" => &mut conf.key,
        "AUDIOCPP_SERVER" => &mut conf.audio_server,
        "AUDIOCPP_API_KEY" => &mut conf.audio_key,
        "AUDIOCPP_VOICES" => &mut conf.voices,
        "AUDIOCPP_ASR_MODEL" => &mut conf.asr_model,
        "AUDIOCPP_DIAR_MODEL" => &mut conf.diar_model,
        "AUDIOCPP_TTS_MODEL" => &mut conf.tts_model,
        "AUDIOCPP_SEP_MODEL" => &mut conf.sep_model,
        "AUDIOCPP_ALIGN_MODEL" => &mut conf.align_model,
        "FFMPEG" => &mut conf.ffmpeg,
        "FIREFOX" => &mut conf.firefox,
        "SD_SERVER" => &mut conf.sd_server,
        "SD_API_KEY" => &mut conf.sd_key,
        _ => return false,
    };
    *field = value.to_string();
    true
}

/// Assign one of the slot counts by its key.
fn set_slot(slots: &mut Slots, key: &str, value: u32) {
    // The seven `*_SLOTS` keys; each absent from the file means 1 (03-shell.md §6), so a
    // slot is only ever written when it differs.
    let field = match key {
        "LLM_SLOTS" => &mut slots.llm,
        "AUDIOCPP_ASR_SLOTS" => &mut slots.asr,
        "AUDIOCPP_DIAR_SLOTS" => &mut slots.diar,
        "AUDIOCPP_ALIGN_SLOTS" => &mut slots.align,
        "AUDIOCPP_TTS_SLOTS" => &mut slots.tts,
        "AUDIOCPP_SEP_SLOTS" => &mut slots.sep,
        "SD_SLOTS" => &mut slots.sd,
        _ => return,
    };
    *field = value;
}

/// Read `llm.conf`. No file is no settings — a machine that has never opened the dialog.
pub fn read(paths: &Paths) -> Result<Conf, String> {
    let path = paths.conf_path();
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Conf::default()),
        Err(err) => return Err(format!("{}: {err}", path.display())),
    };
    parse(&text).map_err(|err| format!("{}: {err}", path.display()))
}

/// Write the whole file at 0600 in a directory at 0700, and say whether anything changed.
///
/// The bytes are compared first and an unchanged file is left alone: §7 asks for a save that
/// is byte-identical when nothing changed, and not touching the file also keeps its mtime
/// meaning "the settings were last edited".
pub fn save(paths: &Paths, conf: &Conf) -> Result<bool, String> {
    let path = paths.conf_path();
    let body = render(conf);
    if fs::read_to_string(&path).is_ok_and(|text| text == body) {
        return Ok(false);
    }
    write_mode(&paths.config_dir, 0o700)?;
    write_bytes(&path, body.as_bytes(), 0o600)?;
    Ok(true)
}

/// Where `llm.conf` used to be: beside the videos, under the folder the program was started from.
///
/// That placement was wrong twice over — the endpoints are the same for every session this machine
/// ever cuts, and a session folder gets copied, zipped and handed around, so the API key went with
/// it (03-shell.md §6's "legacy `<root>/llm.conf`").
pub fn legacy_conf_path(root: &Path) -> PathBuf {
    root.join("llm.conf")
}

/// Take over a pre-merge `llm.conf` still sitting beside the videos, once, and put it in the config
/// folder. Returns the legacy path when a move happened, so the caller can log which file stopped
/// being read.
///
/// Nothing happens while the config folder's own file exists: that one always wins, which is what
/// makes "migrated once" true rather than a rule that has to be remembered (§6's precedence). The
/// bytes are copied verbatim instead of parsed and re-rendered — a re-render would drop the comments
/// a user or an older build left in the file, and every key in it is one [`parse`] already reads.
/// Copied rather than moved: the old file is one line of documentation about a machine that has been
/// working for months, and deleting somebody's config to tidy up is not this program's call.
pub fn migrate_legacy(paths: &Paths, root: &Path) -> Result<Option<PathBuf>, String> {
    let path = paths.conf_path();
    if path.exists() {
        return Ok(None);
    }
    let legacy = legacy_conf_path(root);
    let bytes = match fs::read(&legacy) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(format!("{}: {err}", legacy.display())),
    };
    // The same modes as `save`: what arrives is the machine's endpoints and its key.
    write_mode(&paths.config_dir, 0o700)?;
    write_bytes(&path, &bytes, 0o600)?;
    Ok(Some(legacy))
}

/// The projects remembered by the legacy `settings.json`, which §7 keeps readable and never
/// writable. `None` when the file is not there.
pub fn legacy_projects(paths: &Paths) -> Result<Option<BTreeMap<String, String>>, String> {
    let path = paths.settings_path();
    #[derive(serde::Deserialize)]
    struct Legacy {
        #[serde(default)]
        projects: BTreeMap<String, String>,
    }
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(format!("{}: {err}", path.display())),
    };
    let legacy: Legacy =
        serde_json::from_str(&text).map_err(|err| format!("{}: {err}", path.display()))?;
    Ok(Some(legacy.projects))
}

/// Take the remembered projects from `settings.json` — but only once, and only when the conf
/// has nothing to say about projects (§7: "read once … on the launch after the merge"). The
/// legacy file is not written by anything in here.
pub fn recover_projects(conf: &mut Conf, paths: &Paths) -> Result<bool, String> {
    if !conf.projects.is_empty() {
        return Ok(false);
    }
    let Some(projects) = legacy_projects(paths)? else {
        return Ok(false);
    };
    if projects.is_empty() {
        return Ok(false);
    }
    conf.projects = projects;
    Ok(true)
}

/// Keys an older build wrote that this one reads and then drops (§8). They parse
/// without an error so a hand-edited conf still loads, and they are NOT kept in
/// [`Conf::unknown`] — that list exists for a newer build's settings, and echoing a
/// dead key back would make it permanent. `AUDIOCPP_MODELS` is the exception with
/// a use: its `voices/` subfolder is the voices folder when `AUDIOCPP_VOICES` is
/// absent, which [`read`] records as [`LEGACY_MODELS`].
pub const IGNORED_KEYS: [&str; 4] = [
    "AUDIOCPP_MODELS",
    "SD_MODEL",
    "AUDIOCPP_LANGUAGE",
    "PROMPT_CUT",
];

/// Where the legacy `AUDIOCPP_MODELS` root is parked while a conf is being read, so
/// a caller can hand it to [`voices_folder`] when `AUDIOCPP_VOICES` stayed empty.
pub const LEGACY_MODELS: &str = "AUDIOCPP_MODELS";

/// Whether this key belongs to an older build: one of the four named ones, or any
/// `PROMPT_*` — the per-step prompt overrides §7 keeps as files instead.
fn legacy_key(key: &str) -> bool {
    IGNORED_KEYS.contains(&key) || key.starts_with("PROMPT_")
}

/// Whether this key belongs to an older build, so its value can be dropped (§8).
pub fn is_legacy_key(key: &str) -> bool {
    legacy_key(key)
}


/// root, is not a project at all — the prototype keeps only the pairs where both are set.
pub fn remembered(projects: &BTreeMap<String, String>) -> Vec<ProjectRef> {
    // Sorted by folder, which is what makes an unchanged save byte-identical and the
    // `PROJECT_<n>` numbering stable between launches.
    projects
        .iter()
        .filter(|(root, file)| !root.is_empty() && !file.is_empty())
        .map(|(root, file)| ProjectRef { root: root.clone(), file: file.clone() })
        .collect()
}

/// The voice library: `AUDIOCPP_VOICES` wins, else the data folder inside Flatpak, else the
/// dev box's `/mnt/models/audiocpp/voices` (§7). This is the one folder read outside a
/// project and it has no settings box; the Flatpak rule exists because the dev-box path is
/// simply not in the sandbox.
/// The boxes that go back to the shipped default when they are cleared, rather
/// than to the empty string (§03-shell#8-details-confirmed-against-the-code-verification-pass).
/// The five are names a server answers or a folder it reads: an empty one is not a
/// usable value, unlike a URL, a key or a tool path, which are written as typed —
/// including empty, because clearing `FFMPEG` is a real answer.
pub const DEFAULTED_WHEN_CLEARED: [&str; 5] = [
    "AUDIOCPP_VOICES",
    "AUDIOCPP_ASR_MODEL",
    "AUDIOCPP_DIAR_MODEL",
    "AUDIOCPP_TTS_MODEL",
    "AUDIOCPP_SEP_MODEL",
];

/// What one settings box writes. `shipped_default` is what this build means by an
/// empty box for that key; every other key passes its own text straight through.
pub fn written_value(key: &str, typed: &str, shipped_default: &str) -> String {
    if typed.is_empty() && DEFAULTED_WHEN_CLEARED.contains(&key) {
        return shipped_default.to_string();
    }
    typed.to_string()
}

/// The shipped default behind one settings key, so the write path can name what an
/// empty box means without a second list beside it.
fn shipped_default(key: &str) -> &'static str {
    match key {
        "AUDIOCPP_ASR_MODEL" => crate::services::ASR_MODEL,
        "AUDIOCPP_DIAR_MODEL" => crate::services::DIAR_MODEL,
        "AUDIOCPP_TTS_MODEL" => crate::services::TTS_MODEL,
        "AUDIOCPP_SEP_MODEL" => crate::services::SEP_MODEL,
        // The folder every other build of this stack mounts.
        _ => crate::settings::DEFAULT_VOICES,
    }
}

/// The five cleared boxes' shipped default, named here so the write path and §7's
/// reader agree on what an empty value means.
pub const DEFAULT_VOICES: &str = "/mnt/models/audiocpp/voices";

/// The voice library: `AUDIOCPP_VOICES`, else the `voices/` subfolder of the legacy
/// `AUDIOCPP_MODELS` root (§8: that key is read and ignored, except for exactly
/// this), else the platform's own folder.
pub fn voices_folder(conf_value: &str, models_dir: &str, flatpak: bool, data_dir: &Path) -> PathBuf {
    // An override that is not absolute is not a folder: it would resolve against whatever
    // the process happens to be started from. The legacy root is trusted the same way —
    // `voices/` under a relative path would move with the process too.
    if !conf_value.is_empty() && Path::new(conf_value).is_absolute() {
        return PathBuf::from(conf_value);
    }
    if !models_dir.is_empty() && Path::new(models_dir).is_absolute() {
        return Path::new(models_dir).join("voices");
    }
    if flatpak {
        return data_dir.join("voices");
    }
    PathBuf::from(DEFAULT_VOICES)
}

/// The legacy `AUDIOCPP_MODELS` spelling of the voices folder: its `voices/`
/// subfolder, which is what the prototype implied (§8). Only an absolute root
/// counts, for the same reason an override does — a relative one moves with the
/// process. Anything else answers empty, meaning "no legacy folder".
pub fn voices_under_models(models_dir: &str) -> String {
    if models_dir.is_empty() || !Path::new(models_dir).is_absolute() {
        return String::new();
    }
    format!("{}/voices", models_dir.trim_end_matches('/'))
}

/// Create the voices folder at 0700 and leave an existing one alone. Nothing reads it
/// through a settings box (§7), so this is the only place its mode is set.
pub fn ensure_voices_folder(folder: &Path) -> Result<(), String> {
    write_mode(folder, 0o700)
}

/// Keep a prompt edited on this machine — but only while it differs from the shipped text
/// (§7), so returning to the default deletes the override rather than leaving a copy of it
/// behind that would shadow a future change to the shipped wording.
pub fn write_prompt(paths: &Paths, key: &str, text: &str, shipped: &str) -> Result<bool, String> {
    if text == shipped {
        return Ok(false);
    }
    write_mode(&paths.prompts_dir(), 0o700)?;
    write_bytes(&paths.prompt_file(key), text.as_bytes(), 0o600)?;
    Ok(true)
}

/// The edited text, or `None` meaning "use the shipped text".
pub fn read_prompt(paths: &Paths, key: &str) -> Result<Option<String>, String> {
    let path = paths.prompt_file(key);
    match fs::read_to_string(&path) {
        Ok(text) => Ok(Some(text)),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(format!("{}: {err}", path.display())),
    }
}

/// A prompt key is a file name under `prompts/`, so it may not climb out of that folder.
pub fn prompt_file_ok(key: &str) -> Result<(), String> {
    if key.is_empty()
        || key.contains('/')
        || key.contains('\\')
        || key == "."
        || key == ".."
        || Path::new(key).is_absolute()
    {
        return Err(format!("{key:?} is not a prompt key"));
    }
    Ok(())
}

/// The prompt to send: this machine's edited text when there is one, else the shipped one.
pub fn prompt_text(paths: &Paths, key: &str, shipped: &str) -> Result<String, String> {
    Ok(read_prompt(paths, key)?.unwrap_or_else(|| shipped.to_string()))
}

/// Drop an override so the shipped text applies again — the User Dialog's reset button.
/// False when there was nothing to drop.
pub fn clear_prompt(paths: &Paths, key: &str) -> Result<bool, String> {
    match fs::remove_file(paths.prompt_file(key)) {
        Ok(()) => Ok(true),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(err) => Err(format!("{}: {err}", paths.prompt_file(key).display())),
    }
}

/// A dump's file name (§7: `hang-*.txt`).
pub fn hang_file(stamp: &str) -> String {
    format!("hang-{stamp}.txt")
}

/// The stamp of one hang, in the prototype's Go layout `0102-150405` — month first so the
/// dumps sort in the order they happened.
pub fn hang_stamp(month: u32, day: u32, hour: u32, minute: u32, second: u32) -> String {
    format!("{month:02}{day:02}-{hour:02}{minute:02}{second:02}")
}

/// Write one watchdog dump and return its path. §7 names the file `hang-*.txt`; what it has
/// to say is which component stopped answering, its PID and how long it had been unresponsive
/// — the prototype wrote "the GTK thread has not answered for 3.2s" plus every stack, and a
/// dump that cannot name the stalled process is no use at 3 a.m.
pub fn write_hang_dump(
    paths: &Paths,
    component: &str,
    pid: u32,
    unresponsive: std::time::Duration,
    stamp: &str,
) -> Result<PathBuf, String> {
    let body = format!(
        "naivepost: {component} has not answered for {} (pid {pid}), stamp {stamp}\n\n",
        seconds_for(unresponsive)
    );
    // 0600 beside the settings file (§1's exception list).
    write_mode(&paths.config_dir, 0o700)?;
    let path = paths.hang_path(stamp);
    write_bytes(&path, body.as_bytes(), 0o600)?;
    Ok(path)
}

/// How long a component had been unresponsive, printed the way Go prints a duration.
fn seconds_for(wait: std::time::Duration) -> String {
    let ms = wait.as_millis();
    if ms % 1000 == 0 {
        format!("{}s", ms / 1000)
    } else {
        format!("{:.1}s", ms as f64 / 1000.0)
    }
}

/// Where a setting comes from (03-shell.md §6): the dialog box, then this file, then the
/// legacy `<root>/llm.conf` migrated once, then the built-in default. An empty answer is not
/// an answer — a cleared box means "back to the default", not "the empty string".
pub fn resolve<'a>(
    dialog: Option<&'a str>,
    conf: Option<&'a str>,
    legacy: Option<&'a str>,
    default: &'a str,
) -> &'a str {
    [dialog, conf, legacy]
        .into_iter()
        .flatten()
        .find(|v| !v.is_empty())
        .unwrap_or(default)
}

/// The audio server's URL. `NAIVEPOST_TTS_URL` sits below the dialog and above the file, and
/// `AUDIOCPP_SERVER` is the file's own key (03-shell.md §6), so both are consulted before the
/// built-in default. Nothing here validates the URL: it is handed to a client that reports a
/// bad one far better than this function could.
pub fn audio_url(
    dialog: Option<&str>,
    tts_url_env: Option<&str>,
    conf: Option<&str>,
    default: &str,
) -> String {
    resolve(dialog, tts_url_env, conf, default).to_string()
}

/// Whether this launch may register itself with the desktop (§7). Two cases say no: inside a
/// Flatpak, whose manifest already exports both files and where a file written from within
/// would point into the sandbox; and a run out of a build cache, which has no installed
/// launcher for the entry to name.
pub fn wants_desktop_file(exe: &Path, flatpak: bool, build_cache: bool) -> bool {
    if flatpak || build_cache {
        return false;
    }
    // The entry's `Exec` is an absolute path, so there has to be a real binary there.
    exe.is_file()
}

/// Quote one value for a desktop entry's key: a space or a quote in an install path would
/// otherwise end the token.
fn desktop_quote(value: &str) -> String {
    if value.contains([' ', '"', '\\']) {
        format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        value.to_string()
    }
}

/// The desktop entry, pointing at the real launcher (§7). `MimeType` names the project type
/// so a file manager offers naivepost for it.
pub fn desktop_file(exe: &Path) -> String {
    let exec = desktop_quote(&exe.display().to_string());
    format!(
        "[Desktop Entry]\nType=Application\nName=NaivePost\nGenericName=Video editing assistant\n\
         Comment=Edit a recorded session: prepare, cut, narrate, produce.\n\
         Exec={exec}\nIcon=ch.bocek.naivepost\nTerminal=false\nCategories=AudioVideo;Video;\n\
         MimeType=application/x-naivepost-project;\nStartupWMClass=naivepost\n"
    )
}

/// The MIME package: one type, whose glob is the project folder's own suffix (§7).
pub fn mime_package() -> String {
    String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <mime-info xmlns=\"http://www.freedesktop.org/standards/shared-mime-info\">\n  \
         <mime-type type=\"application/x-naivepost-project\">\n    \
         <comment>NaivePost project</comment>\n    \
         <glob pattern=\"*.naivepost\"/>\n  </mime-type>\n</mime-info>\n",
    )
}

/// Write the desktop entry at 0644 (§1's ordinary file mode). The caller decides *whether* —
/// see [`wants_desktop_file`].
pub fn install_desktop_file(paths: &Paths, exe: &Path) -> Result<(), String> {
    write_bytes(&paths.desktop_file(), desktop_file(exe).as_bytes(), 0o644)
}

/// Write the MIME package at 0644. After either file changes the caller runs
/// `update-mime-database` / `update-desktop-database` in the background (03-shell.md §7);
/// that is a process, so it happens above here.
pub fn install_mime_package(paths: &Paths) -> Result<(), String> {
    write_bytes(&paths.mime_package(), mime_package().as_bytes(), 0o644)
}

/// Create a directory and force its mode — a created directory's mode is masked by the
/// umask, which is why it cannot simply be trusted (§1's rule, applied to the machine's own
/// folders because this file holds credentials).
fn write_mode(dir: &Path, mode: u32) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    // Created here? Then its mode is masked by the umask and has to be forced. A directory
    // that already existed is left alone — relabelling somebody else's folder (an existing
    // $XDG_CONFIG_HOME) is not this function's business, and a throwaway test root under
    // /tmp would otherwise come back 0700 for no reason.
    let fresh = fs::metadata(dir).is_err();
    fs::create_dir_all(dir).map_err(|err| format!("{}: {err}", dir.display()))?;
    if fresh {
        fs::set_permissions(dir, PermissionsExt::from_mode(mode))
            .map_err(|err| format!("{}: {err}", dir.display()))?;
    }
    Ok(())
}

fn write_bytes(path: &Path, bytes: &[u8], mode: u32) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| format!("{}: {err}", parent.display()))?;
    }
    fs::write(path, bytes).map_err(|err| format!("{}: {err}", path.display()))?;
    fs::set_permissions(path, std::os::unix::fs::PermissionsExt::from_mode(mode))
        .map_err(|err| format!("{}: {err}", path.display()))
}
