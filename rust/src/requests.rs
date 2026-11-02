//! `requests.tsv` and the ASR sidecars of spec/01-project-and-files.md §6: the log of
//! every request sent outside, the four JSON documents a recording leaves behind, the
//! `meta.env` marker, and the frame-scanning pair `.frames` / `scenes.tsv`.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::layout;

/// A recording shorter than this is a start and a stop, not a take: it is written up as
/// silence with no server asked (§6).
pub const MIN_TAKE_SECONDS: f64 = 2.0;

/// The header of `requests.tsv`, §6's eighteen columns in order.
pub const HEADER: &str = concat!(
    "started\trun\tstep\tjob\tservice\tmodel\tkind\tsent_bytes\timages",
    "\treceived_bytes\ttokens_in\ttokens_out\twait_s\tfirst_byte_s\ton_wire_s",
    "\tthinking_s\toutcome\tattempt",
);

/// Which server a request went to.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Service {
    /// The default because the LLM is the server every step talks to.
    #[default]
    Llm,
    Audio,
    Image,
    Web,
}

impl Service {
    fn parse(name: &str) -> Option<Self> {
        match name {
            "llm" => Some(Self::Llm),
            "audio" => Some(Self::Audio),
            "image" => Some(Self::Image),
            "web" => Some(Self::Web),
            _ => None,
        }
    }
}

/// How a request ended.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Outcome {
    /// The default because a row that says nothing else ended well.
    #[default]
    Ok,
    Cache,
    Error(String),
    Stalled,
    Cancelled,
}

impl std::fmt::Display for Outcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ok => f.write_str("ok"),
            Self::Cache => f.write_str("cache"),
            Self::Error(reason) => write!(f, "error {reason}"),
            Self::Stalled => f.write_str("stalled"),
            Self::Cancelled => f.write_str("cancelled"),
        }
    }
}

impl FromStr for Outcome {
    type Err = String;

    /// An outcome this build cannot name is refused rather than read as `ok`: a row that
    /// quietly becomes a success is the one misreading this column cannot survive.
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text {
            "ok" => Ok(Self::Ok),
            "cache" => Ok(Self::Cache),
            "stalled" => Ok(Self::Stalled),
            "cancelled" => Ok(Self::Cancelled),
            rest => match rest.strip_prefix("error ") {
                Some(reason) if !reason.is_empty() => Ok(Self::Error(reason.to_string())),
                _ => Err(format!("{text:?} is not an outcome this build knows")),
            },
        }
    }
}

/// One request sent outside, timed.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Request {
    /// Local time with milliseconds, as [`format_started`] writes it.
    pub started: String,
    pub run: String,
    pub step: String,
    pub job: String,
    pub service: Service,
    pub model: String,
    /// What kind of call: `chat`, `asr`, `align`, `diarize`, `tts`, `separate`, `upload`,
    /// `image`, `search`, `read`.
    pub kind: String,
    pub sent_bytes: Option<i64>,
    pub images: Option<u32>,
    pub received_bytes: Option<i64>,
    pub tokens_in: Option<i64>,
    pub tokens_out: Option<i64>,
    /// Seconds in the queue before the request went out.
    pub wait_s: Option<f64>,
    pub first_byte_s: Option<f64>,
    /// Time on the wire — empty for a cache hit, which never had any.
    pub on_wire_s: Option<f64>,
    pub thinking_s: Option<f64>,
    pub outcome: Outcome,
    pub attempt: u32,
}

/// The row `request` is written as: eighteen tab-separated fields, newline-terminated.
pub fn write_line(request: &Request) -> String {
    let seconds = |v: Option<f64>| v.map(|v| format!("{v:.2}")).unwrap_or_default();
    let number = |v: i64| v.to_string();
    [
        request.started.clone(),
        request.run.clone(),
        request.step.clone(),
        request.job.clone(),
        serde_json::to_string(&request.service)
            .expect("a service name is a word")
            .trim_matches('"')
            .to_string(),
        request.model.clone(),
        request.kind.clone(),
        request.sent_bytes.map(number).unwrap_or_default(),
        request.images.map(|v| v.to_string()).unwrap_or_default(),
        request.received_bytes.map(number).unwrap_or_default(),
        request.tokens_in.map(number).unwrap_or_default(),
        request.tokens_out.map(number).unwrap_or_default(),
        seconds(request.wait_s),
        seconds(request.first_byte_s),
        seconds(request.on_wire_s),
        seconds(request.thinking_s),
        request.outcome.to_string(),
        request.attempt.to_string(),
    ]
    .join("\t")
        + "\n"
}

/// Read one row.
pub fn parse_line(line: &str) -> Result<Request, String> {
    let row = line.trim_end_matches(['\n', '\r']);
    let f: Vec<&str> = row.split('\t').collect();
    if f.len() < 7 {
        return Err(format!("{row}: a request row starts with seven fields"));
    }
    let missing = |n: usize| f.len() > n;
    let opt_number = |n: usize| -> Result<Option<i64>, String> {
        if !missing(n) || f[n].is_empty() {
            return Ok(None);
        }
        f[n].parse::<i64>()
            .map(Some)
            .map_err(|_| format!("{row}: {:?} is not a count", f[n]))
    };
    let opt_seconds = |n: usize| -> Result<Option<f64>, String> {
        if !missing(n) || f[n].is_empty() {
            return Ok(None);
        }
        f[n].parse::<f64>()
            .map(Some)
            .map_err(|_| format!("{row}: {:?} is not a time", f[n]))
    };
    let attempt = if missing(17) && !f[17].is_empty() {
        f[17]
            .parse::<u32>()
            .map_err(|_| format!("{row}: {:?} is not an attempt", f[17]))?
    } else {
        // A row from before the column existed was the first try: that is what its lack of
        // an attempt means, and 0 would read as "no attempt at all".
        1
    };
    Ok(Request {
        started: f[0].to_string(),
        run: f[1].to_string(),
        step: f[2].to_string(),
        job: f[3].to_string(),
        service: Service::parse(f[4])
            .ok_or_else(|| format!("{row}: {:?} is not a service", f[4]))?,
        model: f[5].to_string(),
        kind: f[6].to_string(),
        sent_bytes: opt_number(7)?,
        images: match opt_number(8)? {
            Some(v) => Some(
                u32::try_from(v).map_err(|_| format!("{row}: {:?} is not an image count", f[8]))?,
            ),
            None => None,
        },
        received_bytes: opt_number(9)?,
        tokens_in: opt_number(10)?,
        tokens_out: opt_number(11)?,
        wait_s: opt_seconds(12)?,
        first_byte_s: opt_seconds(13)?,
        on_wire_s: opt_seconds(14)?,
        thinking_s: opt_seconds(15)?,
        outcome: if missing(16) && !f[16].is_empty() {
            f[16].parse()?
        } else {
            Outcome::Ok
        },
        attempt,
    })
}

/// Append one request's row, writing the header first when the file is new (§6: "appended
/// when it ends", "Kept across runs, never rewritten").
pub fn record(tree: &layout::Tree, request: &Request) -> Result<(), String> {
    let path = tree.requests_tsv();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| format!("{}: {err}", parent.display()))?;
    }
    // The header belongs to the file, not to a row.
    let needs_header = match fs::read_to_string(&path) {
        Ok(text) => text.lines().next().is_none_or(|line| line != HEADER),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => true,
        Err(err) => return Err(format!("{}: {err}", path.display())),
    };
    let mut file = OpenOptions::new()
        .append(true)
        .create(true)
        .open(&path)
        .map_err(|err| format!("{}: {err}", path.display()))?;
    if needs_header {
        let prefix = match fs::read_to_string(&path) {
            Ok(text) if !text.is_empty() && !text.ends_with('\n') => "\n",
            _ => "",
        };
        file.write_all(prefix.as_bytes())
            .and_then(|_| file.write_all(HEADER.as_bytes()))
            .and_then(|_| file.write_all(b"\n"))
            .map_err(|err| format!("{}: {err}", path.display()))?;
    }
    file.write_all(write_line(request).as_bytes())
        .map_err(|err| format!("{}: {err}", path.display()))?;
    // §1's 0644, set after the creation because a new file's mode is masked by the umask.
    fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o644))
        .map_err(|err| format!("{}: {err}", path.display()))
}

/// The log's rows, in the order they were appended.
pub fn read(tree: &layout::Tree) -> Result<Vec<Request>, String> {
    let path = tree.requests_tsv();
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(format!("{}: {err}", path.display())),
    };
    read_rows(&text, &path)
}

fn read_rows(text: &str, path: &Path) -> Result<Vec<Request>, String> {
    let mut out = Vec::new();
    for (n, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        if n == 0 && line == HEADER {
            continue;
        }
        out.push(parse_line(line).map_err(|err| format!("{}: {err}", path.display()))?);
    }
    Ok(out)
}

/// Local time with milliseconds, `YYYY-MM-DD HH:MM:SS.mmm` — §6's `started`.
pub fn format_started(epoch_ms: i64, offset_secs: i32) -> String {
    let local = epoch_ms + i64::from(offset_secs) * 1000;
    let (days, millis) = (local.div_euclid(86_400_000), local.rem_euclid(86_400_000));
    let days = days + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_day =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_day + era * 400;
    let day_of_year = day_of_era - (365 * year_of_day + year_of_day / 4 - year_of_day / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = if month <= 2 { year + 1 } else { year };

    let (hours, rest) = ((millis / 3_600_000) as u32, millis % 3_600_000);
    let (minutes, rest) = ((rest / 60_000) as u32, rest % 60_000);
    format!(
        "{year:04}-{month:02}-{:02} {hours:02}:{minutes:02}:{:02}.{:03}",
        day,
        rest / 1000,
        rest % 1000
    )
}

// --- the ASR sidecars ---------------------------------------------------------

/// One word with its place in the recording, as the ASR server spells it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Word {
    pub word: String,
    pub start_sample: u64,
    pub end_sample: u64,
}

/// `words.json` — the ASR server's own document, kept as it came back.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WordsDoc {
    pub text: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub words: Vec<Word>,
}

/// `words.aligned.json` — the aligner's answer, and nothing but the words.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AlignedDoc {
    pub words: Vec<Word>,
}

/// One diarized turn in `turns.json`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Turn {
    pub start_sample: u64,
    pub end_sample: u64,
    pub speaker_id: u32,
}

/// One ASR request in `asrchunks.json`: the seconds it covered and exactly what came back
/// for them — the file that lets a chunk's words be checked against its audio without
/// asking the server again.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Chunk {
    pub s: f64,
    pub e: f64,
    pub text: String,
}

/// Write `words.json`.
pub fn write_words(tree: &layout::Tree, source: &str, doc: &WordsDoc) -> Result<(), String> {
    write_json(&tree.words_json(source), doc)
}

/// Read `words.json`.
pub fn read_words(tree: &layout::Tree, source: &str) -> Result<Option<WordsDoc>, String> {
    read_json(&tree.words_json(source))
}

/// Write `words.aligned.json`.
pub fn write_aligned(tree: &layout::Tree, source: &str, doc: &AlignedDoc) -> Result<(), String> {
    write_json(&tree.words_aligned_json(source), doc)
}

/// Read `words.aligned.json`.
pub fn read_aligned(tree: &layout::Tree, source: &str) -> Result<Option<AlignedDoc>, String> {
    read_json(&tree.words_aligned_json(source))
}

/// Write `turns.json`, a bare array.
pub fn write_turns(tree: &layout::Tree, source: &str, turns: &[Turn]) -> Result<(), String> {
    write_json(&tree.turns_json(source), &turns.to_vec())
}

/// Read `turns.json`.
pub fn read_turns(tree: &layout::Tree, source: &str) -> Result<Vec<Turn>, String> {
    Ok(read_json::<Vec<Turn>>(&tree.turns_json(source))?.unwrap_or_default())
}

/// Write `asrchunks.json`, a bare array.
pub fn write_chunks(tree: &layout::Tree, source: &str, chunks: &[Chunk]) -> Result<(), String> {
    write_json(&tree.asrchunks_json(source), &chunks.to_vec())
}

/// Read `asrchunks.json`.
pub fn read_chunks(tree: &layout::Tree, source: &str) -> Result<Vec<Chunk>, String> {
    Ok(read_json::<Vec<Chunk>>(&tree.asrchunks_json(source))?.unwrap_or_default())
}

fn write_json<T: Serialize + ?Sized>(path: &Path, value: &T) -> Result<(), String> {
    let text = serde_json::to_string(value).map_err(|err| format!("{path:?}: {err}"))?;
    write_raw(path, text.as_bytes())
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<Option<T>, String> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(format!("{}: {err}", path.display())),
    };
    serde_json::from_str(&text)
        .map(Some)
        .map_err(|err| format!("{}: {err}", path.display()))
}

/// Is this recording shorter than the bound, so that it is a start and a stop rather than a
/// take?
pub fn is_short(duration_secs: f64, min: f64) -> bool {
    duration_secs < min
}

/// Write up a short recording as silence — no server asked (§6): `transcript.txt` is a lone
/// newline, `words.json` says `{"text":""}`, `turns.json` is an empty array, and there is no
/// `asrchunks.json` because no ASR request was ever made.
pub fn write_silence(tree: &layout::Tree, source: &str) -> Result<(), String> {
    write_raw(&tree.transcript_txt(source), b"\n")?;
    // Only `text`, exactly as §6 spells it: an empty `words` array would be a second way to
    // say that nothing was heard, and the two could disagree.
    write_raw(&tree.words_json(source), br#"{"text":""}"#)?;
    write_raw(&tree.turns_json(source), b"[]")?;
    Ok(())
}

// --- meta.env, .frames, scenes.tsv --------------------------------------------

/// Write `meta.env` as `KEY=VALUE` lines.
pub fn write_meta(tree: &layout::Tree, pairs: &[(String, String)]) -> Result<(), String> {
    let mut out = String::new();
    for (key, value) in pairs {
        out.push_str(&format!("{key}={value}\n"));
    }
    write_raw(&tree.meta_env(), out.as_bytes())
}

/// Write `.frames`: `<grid>|<scene threshold>`.
pub fn write_frames_marker(
    tree: &layout::Tree,
    source: &str,
    grid: f64,
    scene_threshold: f64,
) -> Result<(), String> {
    write_raw(
        &tree.frames_marker(source),
        format!("{grid}|{scene_threshold}\n").as_bytes(),
    )
}

/// Read `.frames`.
pub fn read_frames_marker(tree: &layout::Tree, source: &str) -> Result<Option<(f64, f64)>, String> {
    let path = tree.frames_marker(source);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(format!("{}: {err}", path.display())),
    };
    let (grid, threshold) = text
        .trim()
        .split_once('|')
        .ok_or_else(|| format!("{}: expected <grid>|<scene threshold>", path.display()))?;
    let parse = |v: &str| {
        v.parse::<f64>()
            .map_err(|_| format!("{}: {v:?} is not a number", path.display()))
    };
    Ok(Some((parse(grid)?, parse(threshold)?)))
}

/// One scene change: when, and how strongly it scored.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SceneChange {
    pub time: f64,
    pub score: f64,
}

/// Write `scenes.tsv`: `time\tscore` per scene change, times with two decimals like every
/// other time in these files.
pub fn write_scenes(
    tree: &layout::Tree,
    source: &str,
    scenes: &[SceneChange],
) -> Result<(), String> {
    let mut out = String::new();
    for scene in scenes {
        out.push_str(&format!("{:.2}\t{}\n", scene.time, scene.score));
    }
    write_raw(&tree.scenes_tsv(source), out.as_bytes())
}

/// Read `scenes.tsv`.
pub fn read_scenes(tree: &layout::Tree, source: &str) -> Result<Vec<SceneChange>, String> {
    let path = tree.scenes_tsv(source);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(format!("{}: {err}", path.display())),
    };
    let mut out = Vec::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let (time, score) = line
            .split_once('\t')
            .ok_or_else(|| format!("{}: {line:?} is not time\\tscore", path.display()))?;
        let parse = |v: &str| {
            v.parse::<f64>()
                .map_err(|_| format!("{}: {v:?} is not a number", path.display()))
        };
        out.push(SceneChange {
            time: parse(time)?,
            score: parse(score)?,
        });
    }
    Ok(out)
}

/// Write a file WHOLE at §1's 0644, making its directory.
fn write_raw(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| format!("{}: {err}", parent.display()))?;
    }
    let part = part_beside(path);
    let wrote = File::create(&part)
        .and_then(|mut file| {
            file.write_all(bytes)?;
            // The bytes are on the disk before the name exists; a rename after this point cannot fail
            // into half-written content.
            file.flush()
        })
        .and_then(|()| {
            fs::set_permissions(&part, std::os::unix::fs::PermissionsExt::from_mode(0o644))
        });
    if let Err(err) = wrote {
        let _ = fs::remove_file(&part);
        return Err(format!("{}: {err}", path.display()));
    }
    match fs::rename(&part, path) {
        Ok(()) => Ok(()),
        Err(err) => {
            let _ = fs::remove_file(&part);
            Err(format!("{}: {err}", path.display()))
        }
    }
}

/// `<name>.part` beside `path` — §1's in-flight spelling, applied to every file Prepare writes.
fn part_beside(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    path.with_file_name(format!("{name}.part"))
}
