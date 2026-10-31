use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Where this machine's naivepost files live.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    pub config_dir: PathBuf,
    /// `~/.local/share/naivepost` — the voice library when there is no other.
    pub data_dir: PathBuf,
}

/// The two folders, from the environment variables a caller read for us.
fn absolute(v: Option<&str>) -> Option<&str> {
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
        self.data_dir
            .join("applications/ch.bocek.naivepost.desktop")
    }

    /// Whether a dump with this stamp is already there. §7 names the file `hang-*.txt`, and one
    /// stall must be one file: two dumps in the same second would bury the first.
    pub fn has_hang_dump(&self, stamp: &str) -> bool {
        self.hang_path(stamp).exists()
    }

    /// The MIME package declaring the project type (§7).
    pub fn mime_package(&self) -> PathBuf {
        self.data_dir.join("mime/packages/ch.bocek.naivepost.xml")
    }
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
    /// The model that answers Prepare's small yes/no questions about frames (§F1.6 S6, the text gate): a fast
    /// vision model on the same server. Empty = the main model answers them ([`Conf::decider`]).
    pub decision_model: String,
    /// The `code:tag:name` list of 00-principles.md §6.
    pub subtitle_languages: String,
    /// Keys this build does not know, kept in file order so a read-modify-write cannot
    /// erase a setting a newer build put here.
    unknown: Vec<(String, String)>,
    pub projects: BTreeMap<String, String>,
}

impl Conf {
    /// The model that judges the text gate (§F1.6 S6): the decision model when one is set, else the main one,
    /// which describes the same frames anyway. The gate always runs, so there is no "no gate" to fall to.
    pub fn decider(&self) -> String {
        let decision = self.decision_model.trim();
        if decision.is_empty() {
            self.model.trim().to_string()
        } else {
            decision.to_string()
        }
    }
}

impl Conf {
    const KEYS: [(&'static str, fn(&Conf) -> &str); 16] = [
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
        ("DECISION_MODEL", |c| &c.decision_model),
    ];

    pub fn key_names() -> Vec<&'static str> {
        Self::KEYS.iter().map(|(key, _)| *key).collect()
    }

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
    for (key, value) in &conf.unknown {
        out.push_str(&format!("{key}={}\n", quote(value)));
    }
    out.push_str(&conf.remembered());
    out
}

/// Quote one value for a double-quoted bash string: backslash, the quote itself, `$`, a
/// backtick, a carriage return and a newline — and nothing else.
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
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

pub fn parse(text: &str) -> Result<Conf, String> {
    let mut conf = Conf::default();
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
        if set_key(&mut conf, key, &value) {
            continue;
        }
        if key == "SUBTITLE_LANGUAGES" {
            conf.subtitle_languages = value;
            continue;
        }
        if key.starts_with("PROJECT_") {
            let (number, which) = pair_key(key)
                .ok_or_else(|| format!("line {}: {key} is not PROJECT_<n>_ROOT or _FILE", n + 1))?;
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
        conf.unknown.push((key.to_string(), value));
    }

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
pub fn set_key(conf: &mut Conf, key: &str, value: &str) -> bool {
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
        "DECISION_MODEL" => &mut conf.decision_model,
        _ => return false,
    };
    *field = value.to_string();
    true
}

/// One named setting's value (`LLM_SERVER`, `AUDIOCPP_API_KEY`, ...), `None` for a name this file does not hold.
pub fn value_named<'a>(conf: &'a Conf, key: &str) -> Option<&'a str> {
    Conf::KEYS
        .iter()
        .find(|(name, _)| *name == key)
        .map(|(_, get)| get(conf))
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

pub fn legacy_conf_path(root: &Path) -> PathBuf {
    root.join("llm.conf")
}

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
    write_mode(&paths.config_dir, 0o700)?;
    write_bytes(&path, &bytes, 0o600)?;
    Ok(Some(legacy))
}

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

pub const IGNORED_KEYS: [&str; 4] = [
    "AUDIOCPP_MODELS",
    "SD_MODEL",
    "AUDIOCPP_LANGUAGE",
    "PROMPT_CUT",
];

pub const LEGACY_MODELS: &str = "AUDIOCPP_MODELS";

/// Whether this key belongs to an older build: one of the four named ones, or any
/// `PROMPT_*` — the per-step prompt overrides §7 keeps as files instead.
fn legacy_key(key: &str) -> bool {
    IGNORED_KEYS.contains(&key) || key.starts_with("PROMPT_") || RETIRED_SLOT_KEYS.contains(&key)
}

/// The per-model request counts an earlier build wrote (`LLM_SLOTS` and the rest). Requests now go one at a
/// time per server (02-services.md §1), so these are read and dropped.
pub const RETIRED_SLOT_KEYS: [&str; 7] = [
    "LLM_SLOTS",
    "AUDIOCPP_ASR_SLOTS",
    "AUDIOCPP_DIAR_SLOTS",
    "AUDIOCPP_ALIGN_SLOTS",
    "AUDIOCPP_TTS_SLOTS",
    "AUDIOCPP_SEP_SLOTS",
    "SD_SLOTS",
];

pub fn remembered(projects: &BTreeMap<String, String>) -> Vec<ProjectRef> {
    projects
        .iter()
        .filter(|(root, file)| !root.is_empty() && !file.is_empty())
        .map(|(root, file)| ProjectRef {
            root: root.clone(),
            file: file.clone(),
        })
        .collect()
}

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
        _ => crate::settings::DEFAULT_VOICES,
    }
}

/// The five cleared boxes' shipped default, named here so the write path and §7's
/// reader agree on what an empty value means.
pub const DEFAULT_VOICES: &str = "/mnt/models/audiocpp/voices";

pub fn voices_folder(
    conf_value: &str,
    models_dir: &str,
    flatpak: bool,
    data_dir: &Path,
) -> PathBuf {
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

/// Create the voices folder at 0700 and leave an existing one alone. Nothing reads it
/// through a settings box (§7), so this is the only place its mode is set.
pub fn ensure_voices_folder(folder: &Path) -> Result<(), String> {
    write_mode(folder, 0o700)
}

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

/// The prompt to send: the open project's profile section for the step, else the shipped wording.
pub fn prompt_text(paths: &Paths, key: &str, shipped: &str) -> Result<String, String> {
    Ok(crate::profile::active_section(paths, key).unwrap_or_else(|| shipped.to_string()))
}

pub const LEGACY_PROMPT_NAMES: [&str; 2] = ["General.txt", "Default.txt"];

pub fn legacy_prompt_file(paths: &Paths, key: &str, name: &str) -> PathBuf {
    paths.prompts_dir().join(key).join(name)
}

pub fn stored_prompt(paths: &Paths, key: &str) -> Result<Option<String>, String> {
    let candidates = std::iter::once(paths.prompt_file(key)).chain(
        LEGACY_PROMPT_NAMES
            .iter()
            .map(|name| legacy_prompt_file(paths, key, name)),
    );
    for path in candidates {
        match fs::read_to_string(&path) {
            Ok(text) => {
                let text = text.trim();
                if !text.is_empty() {
                    return Ok(Some(text.to_string()));
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(format!("{}: {err}", path.display())),
        }
    }
    Ok(None)
}

/// Drop an override so the shipped text applies again — the User Dialog's reset button.
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

pub fn hang_stamp(month: u32, day: u32, hour: u32, minute: u32, second: u32) -> String {
    format!("{month:02}{day:02}-{hour:02}{minute:02}{second:02}")
}

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

/// Write one watchdog dump that CARRIES THE STACKS — §1's GUI hang watchdog, as opposed to
/// [`write_hang_dump`], which names a stalled server-side component and has no stack of its own.
pub fn write_stack_dump(paths: &Paths, body: &str, stamp: &str) -> Result<PathBuf, String> {
    write_mode(&paths.config_dir, 0o700)?;
    let path = paths.hang_path(stamp);
    write_bytes(&path, body.as_bytes(), 0o600)?;
    Ok(path)
}

/// How long a component had been unresponsive, printed the way Go prints a duration.
pub(crate) fn seconds_for(wait: std::time::Duration) -> String {
    let ms = wait.as_millis();
    if ms.is_multiple_of(1000) {
        format!("{}s", ms / 1000)
    } else {
        format!("{:.1}s", ms as f64 / 1000.0)
    }
}

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

pub fn audio_url(
    dialog: Option<&str>,
    tts_url_env: Option<&str>,
    conf: Option<&str>,
    default: &str,
) -> String {
    resolve(dialog, tts_url_env, conf, default).to_string()
}

pub fn wants_desktop_file(exe: &Path, flatpak: bool, build_cache: bool) -> bool {
    if flatpak || build_cache {
        return false;
    }
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

pub fn install_mime_package(paths: &Paths) -> Result<(), String> {
    write_bytes(&paths.mime_package(), mime_package().as_bytes(), 0o644)
}

pub(crate) fn write_mode(dir: &Path, mode: u32) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
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
