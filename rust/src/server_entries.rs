//! §03-shell.md §5 and §6: named server entries for the Settings dialog's Writing, Audio and Drawing sections.
//!
//! A person with two audio.cpp machines (one busy, one free) switches between them by name instead of retyping an
//! address, a key and five model ids. Each section keeps its entries and which one is active in
//! `<config>/servers.json`; `llm.conf` stays the one file every flow reads, so picking an entry writes its values
//! there and no flow knows entries exist. The entries are stored under `llm.conf`'s own key names.
//!
//! The file holds API keys, so it is written whole through a `.part` and a rename, at 0600, and nothing here
//! puts a value into a log line.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::settings::{self, Conf, Paths};

/// The three sections that have servers to switch between. Cutting's local tools have no entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Writing,
    Audio,
    Drawing,
}

impl Section {
    /// Every section with entries, in the dialog's order.
    pub const ALL: [Section; 3] = [Section::Writing, Section::Audio, Section::Drawing];

    /// The section's short name: the dialog's widget names and `servers.json`'s object keys.
    pub fn key(self) -> &'static str {
        match self {
            Section::Writing => "writing",
            Section::Audio => "audio",
            Section::Drawing => "drawing",
        }
    }

    /// The `llm.conf` settings one entry of this section holds.
    pub fn conf_keys(self) -> &'static [&'static str] {
        match self {
            Section::Writing => &["LLM_SERVER", "LLM_API_KEY", "LLM_MODEL", "DECISION_MODEL"],
            Section::Audio => &[
                "AUDIOCPP_SERVER",
                "AUDIOCPP_API_KEY",
                "AUDIOCPP_VOICES",
                "AUDIOCPP_TTS_MODEL",
                "AUDIOCPP_ASR_MODEL",
                "AUDIOCPP_DIAR_MODEL",
                "AUDIOCPP_SEP_MODEL",
                "AUDIOCPP_ALIGN_MODEL",
            ],
            Section::Drawing => &["SD_SERVER", "SD_API_KEY"],
        }
    }

    /// The setting that names this section's server, which a migrated entry is named after.
    fn server_key(self) -> &'static str {
        self.conf_keys()[0]
    }

    /// The section a dialog box belongs to, if it belongs to one with entries.
    pub fn of_box(box_key: &str) -> Option<Section> {
        Section::ALL.into_iter().find(|section| {
            section
                .conf_keys()
                .iter()
                .any(|key| box_of(key) == Some(box_key))
        })
    }
}

/// The dialog box that edits a setting (`entry-<box>`), `None` for one with no box (the voices folder).
pub fn box_of(conf_key: &str) -> Option<&'static str> {
    Some(match conf_key {
        "LLM_SERVER" => "server",
        "LLM_API_KEY" => "llm-key",
        "LLM_MODEL" => "llm",
        "DECISION_MODEL" => "decision",
        "AUDIOCPP_SERVER" => "audio",
        "AUDIOCPP_API_KEY" => "audio-key",
        "AUDIOCPP_TTS_MODEL" => "tts",
        "AUDIOCPP_ASR_MODEL" => "asr",
        "AUDIOCPP_DIAR_MODEL" => "diar",
        "AUDIOCPP_SEP_MODEL" => "sep",
        "AUDIOCPP_ALIGN_MODEL" => "aligner",
        "SD_SERVER" => "sd",
        "SD_API_KEY" => "sd-key",
        _ => return None,
    })
}

/// The setting a dialog box edits, the way round [`box_of`] goes.
pub fn conf_key_of_box(box_key: &str) -> Option<&'static str> {
    Section::ALL
        .into_iter()
        .flat_map(|section| section.conf_keys().iter().copied())
        .find(|key| box_of(key) == Some(box_key))
}

/// One entry: its settings, by `llm.conf` name.
pub type Entry = BTreeMap<String, String>;

/// One section's entries and which one is in use.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Group {
    pub active: String,
    pub entries: BTreeMap<String, Entry>,
}

/// `servers.json`: one group per section.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Servers {
    pub writing: Group,
    pub audio: Group,
    pub drawing: Group,
}

impl Servers {
    pub fn group(&self, section: Section) -> &Group {
        match section {
            Section::Writing => &self.writing,
            Section::Audio => &self.audio,
            Section::Drawing => &self.drawing,
        }
    }

    pub fn group_mut(&mut self, section: Section) -> &mut Group {
        match section {
            Section::Writing => &mut self.writing,
            Section::Audio => &mut self.audio,
            Section::Drawing => &mut self.drawing,
        }
    }
}

/// Where the entries live: beside `llm.conf`.
pub fn path(paths: &Paths) -> PathBuf {
    paths.config_dir.join("servers.json")
}

/// A section's settings as `conf` holds them now: the values a new or migrated entry starts from.
pub fn values_from(section: Section, conf: &Conf) -> Entry {
    section
        .conf_keys()
        .iter()
        .map(|key| {
            (
                key.to_string(),
                settings::value_named(conf, key)
                    .unwrap_or_default()
                    .to_string(),
            )
        })
        .collect()
}

/// Put an entry's settings into `conf`; a setting the entry lacks is emptied, so a pick never mixes two servers.
pub fn apply(section: Section, entry: &Entry, conf: &mut Conf) {
    for key in section.conf_keys() {
        let value = entry.get(*key).map(String::as_str).unwrap_or_default();
        settings::set_named(conf, key, value);
    }
}

/// The name a migrated entry gets: its server's host ("ai.jos.li"), or "default" for an empty server box.
pub fn host_name(server: &str) -> String {
    let trimmed = server.trim();
    if trimmed.is_empty() {
        return "default".to_string();
    }
    let without_scheme = trimmed.split_once("://").map_or(trimmed, |(_, rest)| rest);
    let host = without_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or(without_scheme);
    let host = host.rsplit_once('@').map_or(host, |(_, host)| host);
    // The host without its port: "alpha.lan:8080" is named "alpha.lan", "[::1]:9001" is "[::1]".
    let host = match host.rsplit_once(':') {
        Some((name, port))
            if !port.is_empty()
                && port.bytes().all(|b| b.is_ascii_digit())
                && (!name.contains(':') || name.ends_with(']')) =>
        {
            name
        }
        _ => host,
    };
    if host.is_empty() {
        "default".to_string()
    } else {
        host.to_string()
    }
}

/// One entry per section from what `llm.conf` says now: the first time the dialog opens, and the fallback when
/// `servers.json` cannot be read.
pub fn migrate(conf: &Conf) -> Servers {
    let mut servers = Servers::default();
    for section in Section::ALL {
        let values = values_from(section, conf);
        let name = host_name(
            values
                .get(section.server_key())
                .map(String::as_str)
                .unwrap_or(""),
        );
        let group = servers.group_mut(section);
        group.entries.insert(name.clone(), values);
        group.active = name;
    }
    servers
}

/// What reading `servers.json` came to: the entries, and a line to say when something was not as expected.
pub struct Loaded {
    pub servers: Servers,
    pub said: Option<String>,
}

/// Read the entries. No file: one entry per section from `conf` (and nothing to say). A file that does not parse
/// is kept as `servers.json.bad`, said once, and the dialog carries on from `conf`; it is never overwritten
/// silently. A group with no entries or an active name it does not hold is mended the same way.
pub fn load(paths: &Paths, conf: &Conf) -> Loaded {
    let file = path(paths);
    let text = match std::fs::read_to_string(&file) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Loaded {
                servers: migrate(conf),
                said: None,
            }
        }
        Err(err) => {
            return Loaded {
                servers: migrate(conf),
                said: Some(format!(
                    "!!! {} could not be read ({err}) -- the entries come from llm.conf for now",
                    file.display()
                )),
            }
        }
    };
    match serde_json::from_str::<Servers>(&text) {
        Ok(mut servers) => {
            let fresh = migrate(conf);
            for section in Section::ALL {
                let group = servers.group_mut(section);
                if group.entries.is_empty() {
                    *group = fresh.group(section).clone();
                } else if !group.entries.contains_key(&group.active) {
                    group.active = group.entries.keys().next().cloned().unwrap_or_default();
                }
            }
            Loaded {
                servers,
                said: None,
            }
        }
        Err(why) => {
            let bad = bad_path(paths);
            let kept = std::fs::rename(&file, &bad).is_ok();
            Loaded {
                servers: migrate(conf),
                said: Some(if kept {
                    format!(
                        "!!! {} could not be read ({why}) -- kept as {}; the entries come from llm.conf",
                        file.display(),
                        bad.display()
                    )
                } else {
                    format!(
                        "!!! {} could not be read ({why}) -- the entries come from llm.conf for now",
                        file.display()
                    )
                }),
            }
        }
    }
}

/// Where an unreadable `servers.json` is put aside.
pub fn bad_path(paths: &Paths) -> PathBuf {
    paths.config_dir.join("servers.json.bad")
}

/// Write the entries whole: a `.part` at 0600, renamed over the file, so a crash never leaves half a file and the
/// keys inside are never readable by another user, not even for a moment.
pub fn save(paths: &Paths, servers: &Servers) -> Result<(), String> {
    let file = path(paths);
    let body = serde_json::to_string_pretty(servers).map_err(|why| why.to_string())?;
    // The folder holds the keys too: made 0700 when this write is the one that creates it, as llm.conf's is.
    settings::write_mode(&paths.config_dir, 0o700)?;
    let part = file.with_extension("json.part");
    write_private(&part, body.as_bytes())?;
    std::fs::rename(&part, &file).map_err(|err| format!("{}: {err}", file.display()))
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut out = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .map_err(|err| format!("{}: {err}", path.display()))?;
    out.write_all(bytes)
        .map_err(|err| format!("{}: {err}", path.display()))?;
    // A `.part` left from an older run keeps its old mode through `open`: say 0600 again.
    std::fs::set_permissions(path, std::os::unix::fs::PermissionsExt::from_mode(0o600))
        .map_err(|err| format!("{}: {err}", path.display()))
}

/// ＋ New: a new entry named `name` holding `values`, made the active one. Refused in words for an empty name or
/// one the section already has.
pub fn create(group: &mut Group, name: &str, values: Entry) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("give the entry a name".to_string());
    }
    if group.entries.contains_key(name) {
        return Err(format!("there is already an entry called {name}"));
    }
    group.entries.insert(name.to_string(), values);
    group.active = name.to_string();
    Ok(())
}

/// 🗑: delete `name` and make the next entry active (the one after it by name, else the one before). Refused for
/// the last entry: a section always has one.
pub fn delete(group: &mut Group, name: &str) -> Result<String, String> {
    if group.entries.len() <= 1 {
        return Err("the last entry cannot be deleted -- a section always has one".to_string());
    }
    if !group.entries.contains_key(name) {
        return Err(format!("there is no entry called {name}"));
    }
    let names: Vec<String> = group.entries.keys().cloned().collect();
    let at = names.iter().position(|known| known == name).unwrap_or(0);
    let next = names
        .get(at + 1)
        .or_else(|| at.checked_sub(1).and_then(|before| names.get(before)))
        .cloned()
        .unwrap_or_default();
    group.entries.remove(name);
    group.active = next.clone();
    Ok(next)
}
