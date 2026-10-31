//! The audio.cpp task door: the one place `POST /v1/tasks/run` is dialed.

use std::path::Path;

use serde_json::Value;

use crate::bodies::{self, ServerPath};
use crate::layout::Tree;
use crate::requests::{Turn, Word, WordsDoc};
use crate::server_leg;
use crate::services::{self, Kind, Need, Server};
use crate::transcribe::SAMPLE_RATE;

#[derive(Debug, Clone, Copy)]
pub struct Ask<'a> {
    pub step: &'a str,
    pub job: &'a str,
    pub task: &'a str,
}

fn answer(body: &Value) -> Value {
    if body.get("ok").and_then(Value::as_bool) == Some(false) {
        return body.clone();
    }
    body.get("result").cloned().unwrap_or_else(|| body.clone())
}

/// Speech to text's answer: the chunk's text, and its word times if the server sent them.
pub fn asr_answer(body: &Value) -> Result<WordsDoc, String> {
    let reply = answer(body);
    let text = ["text", "transcript"]
        .iter()
        .filter_map(|key| reply.get(*key).and_then(Value::as_str))
        .next();
    let words = match reply.get("words").or_else(|| reply.get("alignment")) {
        Some(_) => crate::align::parse_words(&reply)?,
        None => Vec::new(),
    };
    match text {
        Some(text) => Ok(WordsDoc {
            text: text.to_string(),
            words,
        }),
        None if !words.is_empty() => Ok(WordsDoc {
            text: joined(&words),
            words,
        }),
        None => Err(format!(
            "the audio.cpp server answered {body} -- no text and no word times to read it as"
        )),
    }
}

/// The text an ASR's word times spell, for a server that sends only times and tokens.
fn joined(words: &[Word]) -> String {
    words
        .iter()
        .map(|word| word.word.as_str())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Diarization's answer: speaker turns as samples.
pub fn diar_answer(body: &Value) -> Result<Vec<Turn>, String> {
    let reply = answer(body);
    let Some(list) = ["speaker_turns", "turns", "speakers", "diarization"]
        .iter()
        .filter_map(|key| reply.get(*key).and_then(Value::as_array))
        .next()
    else {
        if reply.get("timing").is_some() {
            return Ok(Vec::new());
        }
        return Err(format!(
            "the audio.cpp server answered {body} -- no speaker turns in it"
        ));
    };
    let mut turns = Vec::new();
    for entry in list {
        if let Some(turn) = turn(entry) {
            turns.push(turn);
        }
    }
    turns.sort_by_key(|turn| turn.start_sample);
    Ok(turns)
}

fn turn(entry: &Value) -> Option<Turn> {
    let start = sample(entry, &["start_sample", "start", "from"])?;
    let end = sample(entry, &["end_sample", "end", "to"])?;
    let speaker = entry
        .get("speaker_id")
        .or_else(|| entry.get("speaker"))
        .map(|value| match value {
            Value::Number(number) => number.as_u64().unwrap_or_default() as u32,
            Value::String(name) => {
                let digits = name.len() - name.trim_end_matches(|c: char| c.is_ascii_digit()).len();
                name[name.len() - digits..]
                    .parse::<u32>()
                    .unwrap_or_else(|_| {
                        name.bytes().fold(0u32, |hash, byte| {
                            hash.wrapping_mul(31).wrapping_add(byte as u32)
                        })
                    })
            }
            _ => 0,
        })
        .unwrap_or(0);
    (end > start).then_some(Turn {
        start_sample: start,
        end_sample: end,
        speaker_id: speaker,
    })
}

/// A time in samples, whichever unit the entry carried. Same rule [`crate::align`] applies to words:
/// the keys say their own unit, and a bare number too small to be samples at 16 kHz is seconds.
fn sample(entry: &Value, keys: &[&str]) -> Option<u64> {
    let rate = crate::transcribe::SAMPLE_RATE as f64;
    for key in keys {
        let Some(raw) = entry.get(*key).and_then(Value::as_f64) else {
            continue;
        };
        let seconds = if key.ends_with("_sample") {
            raw / rate
        } else if key.ends_with("_ms") {
            raw / 1000.0
        } else if raw < crate::align::PLAIN_AS_SAMPLES {
            raw
        } else {
            raw / rate
        };
        return Some((seconds * rate) as u64);
    }
    None
}

/// Voice separation's answer: the paths of the two stems on the server.
pub fn stem_paths(body: &Value) -> Result<(Stems, ServerPath, ServerPath), String> {
    let reply = answer(body);
    let path = |keys: &[&str]| {
        keys.iter()
            .filter_map(|key| reply.get(*key).and_then(Value::as_str))
            .next()
            .and_then(ServerPath::from_upload)
    };
    let voice = path(&["voice", "vocals", "stem_voice"])
        .ok_or_else(|| format!("the audio.cpp server answered {body} -- no voice stem path"))?;
    let rest = path(&["rest", "accompaniment", "other", "stem_rest"]).ok_or_else(|| {
        format!("the audio.cpp server answered {body} -- no accompaniment stem path")
    })?;
    Ok((Stems::Both, voice, rest))
}

/// Which half a separation reply names: both at once (the usual shape) or the one a chunk asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stems {
    Both,
    Voice,
    Rest,
}

/// The envelope with the audio path in it: `{"model","request":{"audio",…}}`.
pub fn task_body(
    kind: Task,
    model: &str,
    audio: &ServerPath,
    language: &str,
    text: Option<&str>,
) -> Value {
    match kind {
        Task::Asr => bodies::asr_body(model, audio, language),
        Task::Diarize => bodies::diar_body(model, audio),
        Task::Align => bodies::align_body(model, audio, text.unwrap_or_default(), language),
    }
}

/// The three things `POST /v1/tasks/run` is asked to do. Names the `bodies` builder to use and the
/// task word the missing-model sentence names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Task {
    Asr,
    Diarize,
    Align,
}

impl Task {
    /// §1's task word for this kind, the one `Need::task` carries.
    pub fn task(self) -> &'static str {
        match self {
            Self::Asr => "asr",
            Self::Diarize => "diarize",
            Self::Align => "align",
        }
    }

    /// Which step asks for this kind, in the words §1's model-role table uses.
    pub fn step(self) -> &'static str {
        match self {
            Self::Asr => "Prepare",
            Self::Diarize => "Prepare",
            Self::Align => "Prepare",
        }
    }
}

/// Send one task envelope and bring back the reply body.
pub fn dial(
    tree: &Option<Tree>,
    paths: &crate::settings::Paths,
    ask: Ask<'_>,
    model: &str,
    envelope: &Value,
) -> Result<String, String> {
    let endpoint = services::endpoint_for(paths, Server::Audio, |name| std::env::var(name).ok());
    let envelope = envelope.to_string();
    let cancelled = crate::cancel_leg::cancel_check_now();
    // §1: one request at a time per server. Taken BEFORE the socket and released when the reply is in, so
    // the wait sits outside the measured call and lands in the row's own wait column instead of inflating its
    // time on the wire.
    let (slot, waited) = match crate::slot_gate::hold(&endpoint.url, ask.step, &cancelled) {
        Ok(got) => got,
        Err(crate::slot_gate::Wait::Cancelled) => {
            return Err(format!(
                "{}: cancelled while waiting for a {model} slot",
                ask.step
            ))
        }
    };
    // §4 holds the slot across the HTTP call only. `call_dialed` reads the reply inside its own call, so the
    // guard lives until it returns; dropping it here is the release.
    let _slot_hold = slot;
    let base = server_leg::base_row(
        Server::Audio,
        model,
        Kind::RunTask.method(),
        "",
        ask.step,
        ask.job,
    );
    // The wait goes in as milliseconds beside the request, not on `base`: `call_dialed` builds every timing
    // column itself and would drop a field it did not measure (see its `waited_ms`).
    let waited_ms = (waited * 1000.0).round() as i64;
    let reply = match tree {
        Some(tree) => {
            server_leg::call_dialed(
                tree,
                &base,
                &server_leg::Dial {
                    server: Server::Audio,
                    endpoint: &endpoint,
                    kind: Kind::RunTask,
                    job: None,
                    body: Some(envelope.as_bytes()),
                    filename: None,
                    cancelled: &cancelled,
                    liveness: None,
                    waited_ms,
                    delta: None,
                    budget: None,
                    attempt: 1,
                },
            )
            .1
        }
        None => server_leg::send(
            Server::Audio,
            &endpoint,
            Kind::RunTask,
            None,
            Some(&envelope),
            &cancelled,
        ),
    }?;
    // Given back before the answer is read through: §4's hold covers the call, not the caller's thinking.
    drop(_slot_hold);
    if reply.ok() {
        return Ok(reply.body);
    }
    if says_missing(&reply.status, &reply.body, model) {
        let catalogue = catalog(tree, paths, ask.step)?;
        return Err(services::missing_model(
            &endpoint.url,
            &catalogue,
            model,
            Need {
                task: ask.task,
                step: ask.step,
            },
        ));
    }
    Err(format!(
        "the audio.cpp server at {} answered {status} asking {model:?} to {task}: {why}",
        endpoint.url,
        status = reply.status,
        task = ask.task,
        why = trim(&reply.body)
    ))
}

fn says_missing(status: &u16, body: &str, model: &str) -> bool {
    if *status == 404 {
        return true;
    }
    let lower = body.to_lowercase();
    !model.trim().is_empty()
        && lower.contains(&model.to_lowercase())
        && [
            "not found",
            "unknown model",
            "not loaded",
            "not served",
            "no such model",
        ]
        .iter()
        .any(|phrase| lower.contains(phrase))
}

/// The server's own model list, for the message that names what it does serve.
pub fn catalog(
    tree: &Option<Tree>,
    paths: &crate::settings::Paths,
    step: &str,
) -> Result<Vec<services::AudioModel>, String> {
    let endpoint = services::endpoint_for(paths, Server::Audio, |name| std::env::var(name).ok());
    let cancelled = crate::cancel_leg::cancel_check_now();
    let base = server_leg::base_row(Server::Audio, "", Kind::ListModels.method(), "", step, "");
    let reply = match tree {
        Some(tree) => {
            server_leg::call(
                tree,
                &base,
                Server::Audio,
                &endpoint,
                Kind::ListModels,
                None,
                None,
                &cancelled,
            )
            .1
        }
        None => server_leg::send(
            Server::Audio,
            &endpoint,
            Kind::ListModels,
            None,
            None,
            &cancelled,
        ),
    }?;
    if !reply.ok() {
        return Err(format!(
            "the audio.cpp server at {} answered {status} to its model list",
            endpoint.url,
            status = reply.status
        ));
    }
    Ok(services::parse_models(&reply.body))
}

/// The `ask` [`crate::asr::run`] wants: one chunk's file name in, that chunk's text and word times out.
pub fn asr_ask<'a>(
    tree: &'a Option<Tree>,
    paths: &'a crate::settings::Paths,
    source: &'a str,
    model: &'a str,
    language: &'a str,
) -> impl FnMut(&str) -> Result<crate::asr::Answer, String> + 'a {
    move |file: &str| {
        let (plan, _) = read_scratch(tree, source, file)?;
        // The chunk goes where its note is: `asr::make_scratch` writes `asr/cNN.wav.plan`, so cutting to
        // any other folder splits a failed chunk from the note that explains it (§F1.4 S2).
        let out = scratch_folder(tree, |tree| tree.asr_scratch(source)).join(file);
        if let Some(dir) = out.parent() {
            std::fs::create_dir_all(dir).map_err(|why| format!("{}: {why}", dir.display()))?;
        }
        // The recording this chunk comes from is the decoded pcm the stage wrote first (§F1.3 S1); naming it
        // is the door's job because `cut_plan` gives only the seek arguments.
        let input = tree
            .as_ref()
            .map(|tree| tree.voice16k_wav(source))
            .unwrap_or_else(|| std::path::PathBuf::from(source));
        cut_from(paths, &input, &plan, &out)?;
        let bytes = std::fs::read(&out).map_err(|why| format!("{}: {why}", out.display()))?;
        let audio = upload_bytes(tree, paths, file, &bytes)?;
        let envelope = task_body(Task::Asr, model, &audio, language, None);
        let body = dial(
            tree,
            paths,
            Ask {
                step: Task::Asr.step(),
                job: file,
                task: Task::Asr.task(),
            },
            model,
            &envelope,
        )?;
        let parsed: Value = parse_json(&body)?;
        let doc = asr_answer(&parsed)?;
        Ok(crate::asr::Answer {
            text: doc.text,
            words: doc.words,
        })
    }
}

/// The `ask` [`crate::align::run`] wants: model, clip and text in, the aligner's word list out.
pub fn align_ask<'a>(
    tree: &'a Option<Tree>,
    paths: &'a crate::settings::Paths,
    source: &'a str,
    language: &'a str,
) -> impl FnMut(&str, &str, &str) -> Result<Value, String> + 'a {
    move |model: &str, clip: &str, text: &str| {
        let plan = align_plan(tree, source, clip)?;
        let out = match tree {
            Some(tree) => tree.align_scratch(source).join(clip),
            None => scratch_dir().join(clip),
        };
        if let Some(dir) = out.parent() {
            std::fs::create_dir_all(dir).map_err(|why| format!("{}: {why}", dir.display()))?;
        }
        let input = tree
            .as_ref()
            .map(|tree| tree.voice16k_wav(source))
            .unwrap_or_else(|| std::path::PathBuf::from(source));
        cut_from(paths, &input, &plan, &out)?;
        let bytes = std::fs::read(&out).map_err(|why| format!("{}: {why}", out.display()))?;
        let audio = upload_bytes(tree, paths, clip, &bytes)?;
        let envelope = task_body(Task::Align, model, &audio, language, Some(text));
        let body = dial(
            tree,
            paths,
            Ask {
                step: Task::Align.step(),
                job: clip,
                task: Task::Align.task(),
            },
            model,
            &envelope,
        )?;
        parse_json(&body)
    }
}

fn align_plan(tree: &Option<Tree>, source: &str, clip: &str) -> Result<Vec<String>, String> {
    let dir = match tree {
        Some(tree) => tree.align_scratch(source),
        None => scratch_dir(),
    };
    let path = dir.join(format!("{clip}.plan"));
    let text =
        std::fs::read_to_string(&path).map_err(|why| format!("{}: {why}", path.display()))?;
    let plan: Vec<String> = text
        .lines()
        .next()
        .unwrap_or_default()
        .split_whitespace()
        .map(str::to_string)
        .collect();
    if plan.is_empty() {
        return Err(format!(
            "{}: no cut arguments in the scratch note",
            path.display()
        ));
    }
    Ok(plan)
}

/// The diarization scan [`crate::transcribe::run`] wants: nothing in but this source's turns out.
pub fn diar_scan<'a>(
    tree: &'a Option<Tree>,
    paths: &'a crate::settings::Paths,
    source: &'a str,
    model: &'a str,
    duration: f64,
    said: &'a std::cell::RefCell<Vec<String>>,
) -> impl FnOnce() -> Result<Vec<Turn>, String> + 'a {
    move || {
        let rung = crate::diar_scan::run(
            duration,
            model,
            source,
            |windows: &[(f64, f64)]| {
                let one = diar_window_ask(tree, paths, source, model);
                windows.iter().map(|span| one(span.0, span.1)).collect()
            },
            |(from, to), anchor: &crate::diarize::Anchor| {
                placement_ask(tree, paths, source, model, from, to, anchor)
            },
            |line| said.borrow_mut().push(line.to_string()),
        )?;
        Ok(rung.turns)
    }
}

pub fn placement_ask(
    tree: &Option<Tree>,
    paths: &crate::settings::Paths,
    source: &str,
    model: &str,
    from: f64,
    to: f64,
    anchor: &crate::diarize::Anchor,
) -> Result<Vec<Turn>, String> {
    let mut pieces: Vec<(f64, f64)> = anchor
        .slots
        .iter()
        .map(|slot| (slot.from, slot.to))
        .collect();
    pieces.push((from, to));
    let chunk = format!("p{:04}.wav", (from * 10.0) as i64);
    diar_file_ask(tree, paths, source, model, &chunk, &joined_plan(&pieces))
}

/// ffmpeg's arguments for one file made of `pieces` of the input, in order: the input split once per piece,
/// each piece trimmed to its seconds and restarted at zero, and the pieces concatenated with nothing between.
fn joined_plan(pieces: &[(f64, f64)]) -> Vec<String> {
    let labels: String = (0..pieces.len()).map(|i| format!("[s{i}]")).collect();
    let mut graph = format!("[0:a]asplit={}{labels}", pieces.len());
    for (i, (from, to)) in pieces.iter().enumerate() {
        graph.push_str(&format!(
            ";[s{i}]atrim=start={from}:end={to},asetpts=PTS-STARTPTS[p{i}]"
        ));
    }
    let parts: String = (0..pieces.len()).map(|i| format!("[p{i}]")).collect();
    graph.push_str(&format!(";{parts}concat=n={}:v=0:a=1[out]", pieces.len()));
    vec![
        "-filter_complex".into(),
        graph,
        "-map".into(),
        "[out]".into(),
    ]
}

/// The diarization turns [`crate::transcribe::run`] wants: nothing in but this source's turns out.
pub fn diar_window_ask<'a>(
    tree: &'a Option<Tree>,
    paths: &'a crate::settings::Paths,
    source: &'a str,
    model: &'a str,
) -> impl Fn(f64, f64) -> Result<Vec<Turn>, String> + 'a {
    move |from: f64, to: f64| {
        let chunk = format!("d{:04}.wav", (from * 10.0) as i64);
        let turns = diar_file_ask(
            tree,
            paths,
            source,
            model,
            &chunk,
            &crate::asr::cut_plan(from, to),
        )?;
        // A window's answer is in that window's own seconds; the ladder's arithmetic is all on the
        // recording's clock, so every turn is moved back before anyone looks at it.
        Ok(turns
            .into_iter()
            .map(|turn| Turn {
                start_sample: turn.start_sample + (from * SAMPLE_RATE as f64) as u64,
                end_sample: turn.end_sample + (from * SAMPLE_RATE as f64) as u64,
                speaker_id: turn.speaker_id,
            })
            .collect())
    }
}

/// One diarization request: `chunk` cut from this source's `voice16k.wav` with ffmpeg's `plan`, uploaded, and
/// asked about. The turns are on the cut file's own clock; the caller says where that file sits.
fn diar_file_ask(
    tree: &Option<Tree>,
    paths: &crate::settings::Paths,
    source: &str,
    model: &str,
    chunk: &str,
    plan: &[String],
) -> Result<Vec<Turn>, String> {
    let note = serde_json::json!({ "audio": chunk, "language": "" });
    // The diarizer's windows live in `diar/`, not beside the ASR's chunks: `asr::run` clears `asr/`
    // when it succeeds, so a window left in there would outlive every stage's cleanup.
    let dir = match tree {
        Some(tree) => tree.diar_scratch(source),
        None => scratch_dir(),
    };
    write_scratch_at(&dir, chunk, plan, &note)?;
    let out = dir.join(chunk);
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent).map_err(|why| format!("{}: {why}", parent.display()))?;
    }
    let input = tree
        .as_ref()
        .map(|tree| tree.voice16k_wav(source))
        .unwrap_or_else(|| std::path::PathBuf::from(source));
    cut_from(paths, &input, plan, &out)?;
    let bytes = std::fs::read(&out).map_err(|why| format!("{}: {why}", out.display()))?;
    let audio = upload_bytes(tree, paths, chunk, &bytes)?;
    let envelope = task_body(Task::Diarize, model, &audio, "", None);
    let body = dial(
        tree,
        paths,
        Ask {
            step: Task::Diarize.step(),
            job: chunk,
            task: Task::Diarize.task(),
        },
        model,
        &envelope,
    )?;
    diar_answer(&parse_json(&body)?)
}

/// The `asr` [`crate::transcribe::run`] wants: this source's `words.json` document out.
#[allow(clippy::too_many_arguments)]
pub fn transcribe_asr<'a, L>(
    tree: &'a Option<Tree>,
    paths: &'a crate::settings::Paths,
    source: &'a str,
    model: &'a str,
    language: &'a str,
    duration: f64,
    silences: &'a [crate::asr::Silence],
    log: &'a mut L,
) -> impl FnOnce() -> Result<WordsDoc, String> + 'a
where
    L: FnMut(&str),
{
    move || {
        let outcome = crate::asr::run(
            tree.as_ref()
                .expect("a session folder to write the chunks into"),
            source,
            duration,
            model,
            language,
            silences,
            asr_ask(tree, paths, source, model, language),
        )?;
        for line in &outcome.logs {
            log(line);
        }
        // §F1.4 S3's per-chunk progress ("recognising speech i/n") is recorded the same way and goes to
        // the same sink: a run that cut four pieces has to say so four times, or the bar looks stuck.
        for line in &outcome.steps {
            log(line);
        }
        Ok(WordsDoc {
            text: outcome.text,
            words: outcome.words,
        })
    }
}

pub fn separation_body(model: &str, audio: &ServerPath, chunk: &str) -> Value {
    serde_json::json!({
        "model": model,
        "request": { "audio": audio.as_str(), "chunk": chunk },
    })
}

const VOICE_KEYS: [&str; 2] = ["voice", "vocals"];
const REST_KEYS: [&str; 4] = ["rest", "accompaniment", "nonvoice", "instrumental"];

#[derive(Debug)]
pub struct ChunkStems {
    pub voice: Option<usize>,
    pub voices: Vec<ServerPath>,
    pub rests: Vec<ServerPath>,
}

/// Read one chunk's stems out of a tasks-run reply, however the model spelled them.
pub fn chunk_stems(body: &Value) -> Result<ChunkStems, String> {
    let reply = answer(body);
    let mut voices: Vec<ServerPath> = Vec::new();
    let mut rests: Vec<ServerPath> = Vec::new();
    let mut voice_index: Option<usize> = None;
    // An explicit failure is reported before anything is parsed: `answer` keeps the whole body for
    // exactly this case, so the sentence the server wrote is what reaches the log.
    if reply.get("ok").and_then(Value::as_bool) == Some(false) {
        let why = reply
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("no reason given");
        return Err(format!(
            "the audio.cpp server refused the separation: {why}"
        ));
    }
    let mut named: Vec<(String, ServerPath)> = Vec::new();
    if let Some(list) = reply.get("stems").and_then(Value::as_array) {
        for entry in list {
            let name = ["name", "stem", "track"]
                .iter()
                .filter_map(|key| entry.get(*key).and_then(Value::as_str))
                .next()
                .unwrap_or("")
                .to_ascii_lowercase();
            let path = ["path", "audio", "file", "url"]
                .iter()
                .filter_map(|key| entry.get(*key).and_then(Value::as_str))
                .next()
                .and_then(ServerPath::from_upload);
            if let Some(path) = path {
                named.push((name, path));
            }
        }
    } else {
        if let Some(map) = reply.as_object() {
            for (key, value) in map {
                if VOICE_KEYS.contains(&key.as_str()) {
                    push_paths(value, &mut voices);
                    continue;
                }
                if REST_KEYS.contains(&key.as_str()) {
                    push_paths(value, &mut rests);
                    continue;
                }
                if let Some(path) = value.as_str().and_then(ServerPath::from_upload) {
                    named.push((key.to_ascii_lowercase(), path));
                }
            }
        }
    }
    for (name, path) in named {
        let speech = name.contains("voice") || name.contains("vocal");
        if speech {
            if voice_index.is_none() {
                voice_index = Some(voices.len());
            }
            voices.push(path);
        } else {
            rests.push(path);
        }
    }
    if voice_index.is_none() {
        voice_index = voices.first().map(|_| 0);
    }
    if voices.is_empty() && rests.is_empty() {
        let looks_silent = reply.get("voice").is_some()
            || reply.get("silence").and_then(Value::as_bool) == Some(true);
        if !looks_silent {
            return Err(format!(
                "the audio.cpp server answered {body} -- no stem paths in it"
            ));
        }
        return Ok(ChunkStems {
            voice: None,
            voices,
            rests,
        });
    }
    Ok(ChunkStems {
        voice: voice_index,
        voices,
        rests,
    })
}

/// One JSON value's server paths: a single string or an array of them, ignoring anything else.
fn push_paths(value: &Value, into: &mut Vec<ServerPath>) {
    match value {
        Value::String(text) => {
            if let Some(path) = ServerPath::from_upload(text) {
                into.push(path);
            }
        }
        Value::Array(items) => {
            for item in items {
                push_paths(item, into);
            }
        }
        _ => {}
    }
}

/// Upload one file's bytes and read back the server path, through the same door as everything else:
/// the timed row, §1's bearer rule, and the refusal that names the start-up flag.
pub fn upload_bytes(
    tree: &Option<Tree>,
    paths: &crate::settings::Paths,
    name: &str,
    bytes: &[u8],
) -> Result<ServerPath, String> {
    let endpoint = services::endpoint_for(paths, Server::Audio, |env| std::env::var(env).ok());
    let cancelled = crate::cancel_leg::cancel_check_now();
    let payload = bytes;
    let base = server_leg::base_row(Server::Audio, "", Kind::Upload.method(), "", "upload", name);
    let reply = match tree {
        Some(tree) => {
            server_leg::call_dialed(
                tree,
                &base,
                &server_leg::Dial {
                    server: Server::Audio,
                    endpoint: &endpoint,
                    kind: Kind::Upload,
                    job: None,
                    body: Some(payload),
                    filename: Some(name),
                    cancelled: &cancelled,
                    liveness: None,
                    waited_ms: 0,
                    delta: None,
                    budget: None,
                    attempt: 1,
                },
            )
            .1
        }
        None => server_leg::send_named(
            Server::Audio,
            &endpoint,
            Kind::Upload,
            None,
            Some(payload),
            Some(name),
            &cancelled,
        ),
    }?;
    if let Some(refusal) = services::upload_refused(reply.status, &reply.body) {
        return Err(refusal);
    }
    if !reply.ok() {
        return Err(format!(
            "uploading {name} to {} answered {}: {}",
            endpoint.url,
            reply.status,
            trim(&reply.body)
        ));
    }
    let parsed = parse_json(&reply.body)?;
    let path = services::upload_path(&parsed)?;
    ServerPath::from_upload(&path)
        .ok_or_else(|| format!("the upload named {path:?}, which is not a server path"))
}

/// The scratch folder this door's chunks and uploads live in, under the session when one is open.
fn scratch_dir() -> std::path::PathBuf {
    std::env::current_dir().unwrap_or_default().join("scratch")
}

pub fn write_scratch(
    tree: &Option<Tree>,
    source: &str,
    file: &str,
    plan: &[String],
    body: &serde_json::Value,
) -> Result<(), String> {
    let dir = match tree {
        Some(tree) => tree.asr_scratch(source),
        None => scratch_dir(),
    };
    write_scratch_at(&dir, file, plan, body)
}

fn write_scratch_at(
    dir: &Path,
    file: &str,
    plan: &[String],
    body: &serde_json::Value,
) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|why| format!("{}: {why}", dir.display()))?;
    let text = format!("{}\n{}\n", plan.join(" "), body);
    std::fs::write(dir.join(format!("{file}.plan")), text)
        .map_err(|why| format!("{}: {why}", dir.join(file).display()))
}

fn read_scratch(
    tree: &Option<Tree>,
    source: &str,
    file: &str,
) -> Result<(Vec<String>, String), String> {
    let dir = match tree {
        Some(tree) => tree.asr_scratch(source),
        None => scratch_dir(),
    };
    let path = dir.join(format!("{file}.plan"));
    let text =
        std::fs::read_to_string(&path).map_err(|why| format!("{}: {why}", path.display()))?;
    let mut lines = text.lines();
    let plan: Vec<String> = lines
        .next()
        .unwrap_or_default()
        .split_whitespace()
        .map(str::to_string)
        .collect();
    if plan.is_empty() {
        return Err(format!(
            "{}: no cut arguments in the scratch note",
            path.display()
        ));
    }
    Ok((plan, lines.next().unwrap_or_default().to_string()))
}

fn scratch_folder(
    tree: &Option<Tree>,
    fallback: impl FnOnce(&Tree) -> std::path::PathBuf,
) -> std::path::PathBuf {
    match tree {
        Some(tree) => fallback(tree),
        None => scratch_dir(),
    }
}

fn cut_from(
    paths: &crate::settings::Paths,
    input: &Path,
    plan: &[String],
    out: &Path,
) -> Result<(), String> {
    let mut args = vec!["-i".to_string(), input.to_string_lossy().into_owned()];
    args.extend(plan.iter().cloned());
    args.push("-y".to_string());
    args.push(out.to_string_lossy().into_owned());
    let shown: Vec<&str> = args.iter().map(String::as_str).collect();
    // The tool is the one Settings names, exactly as `run_stages::dial_task` resolves ffprobe through
    // `services::ffprobe_for`: an empty box means the plain name off PATH (§ "What it needs").
    let program = ffmpeg_program(paths);
    crate::subprocess::run(&program, &shown, |_| ())?;
    out.exists()
        .then_some(())
        .ok_or_else(|| format!("{program} produced no chunk file at {}", out.display()))
}

/// Which ffmpeg this session cuts with: the configured path when the box holds one, the bare name
/// otherwise. Read per request like every other settings value, so a re-typed box takes effect at once.
pub fn ffmpeg_program(paths: &crate::settings::Paths) -> String {
    let configured = crate::settings::read(paths).unwrap_or_default().ffmpeg;
    let trimmed = configured.trim();
    if trimmed.is_empty() {
        return crate::run_stages::FFMPEG.to_string();
    }
    trimmed.to_string()
}

pub fn transcribe_source(
    tree: &Tree,
    paths: &crate::settings::Paths,
    source: &str,
    duration: f64,
    language: &str,
    log: &mut impl FnMut(&str),
) -> Result<crate::transcribe::Passage, String> {
    let conf = crate::settings::read(paths).unwrap_or_default();
    let asr_model = services::asr_model(&conf).to_string();
    let diar_model = services::diar_model(&conf).to_string();
    let endpoint = services::endpoint_for(paths, Server::Audio, |name| std::env::var(name).ok());
    let models = catalog(&Some(tree.clone()), paths, "align")?;
    let aligners = match crate::align::aligners(&conf.align_model, &endpoint.url, &models) {
        Ok(ids) => ids,
        Err(refusal) => {
            log(&refusal);
            Vec::new()
        }
    };
    // align's and the diarizer's own sentences are collected in their closures and appended to
    // `Passage.logs` at the end, so they stay in their own order instead of being printed before they ran.
    let align_lines: std::rc::Rc<std::cell::RefCell<Vec<String>>> = Default::default();
    let diar_lines: std::rc::Rc<std::cell::RefCell<Vec<String>>> = Default::default();
    // §F1.4 S2/§F1.5 S2: the quiet stretches of THIS recording, measured once and handed to both
    // chunking steps so one definition of "quiet" moves both the ASR's cuts and the aligner's windows.
    let silences = crate::separate_media::spans(
        &crate::separate_media::program(&conf.ffmpeg),
        &tree.voice16k_wav(source),
    )
    .unwrap_or_default();
    let held = Some(tree.clone());
    // Borrowed, not moved: the aligner's closure and the ASR's both need them, and `transcribe::run`
    // calls the ASR first, so whichever were moved would leave the other holding nothing.
    let silences_ref: &[crate::asr::Silence] = &silences;
    let held_ref = &held;
    let align = |doc: &crate::requests::WordsDoc| match aligners.first() {
        None => crate::transcribe::Align::None,
        Some(_) => {
            let chunks = crate::requests::read_chunks(tree, source).unwrap_or_default();
            match crate::align::run(
                tree,
                source,
                duration,
                language,
                silences_ref,
                &chunks,
                &doc.words,
                &doc.text,
                &aligners,
                align_ask(held_ref, paths, source, language),
            ) {
                Ok(outcome) => {
                    let mut held_lines = align_lines.borrow_mut();
                    held_lines.extend(outcome.logs.iter().cloned());
                    held_lines.extend(outcome.steps.iter().cloned());
                    crate::transcribe::Align::Done
                }
                Err(why) => crate::transcribe::Align::Failed(why),
            }
        }
    };
    let mut passage = crate::transcribe::run(
        tree,
        source,
        duration,
        &asr_model,
        align,
        transcribe_asr(
            &held, paths, source, &asr_model, language, duration, &silences, log,
        ),
        diar_scan(&held, paths, source, &diar_model, duration, &diar_lines),
    )?;
    passage.logs.extend(align_lines.borrow().iter().cloned());
    passage.logs.extend(diar_lines.borrow().iter().cloned());
    Ok(passage)
}

pub fn source_lane(path: &str) -> String {
    let base = path.rsplit('/').next().unwrap_or(path);
    base.split_once('.')
        .map(|(stem, _)| stem.to_string())
        .unwrap_or_else(|| base.to_string())
}

/// The server path a task is asked about, from the file name the flow module chose.
fn parse_json(body: &str) -> Result<Value, String> {
    serde_json::from_str(body).map_err(|_| {
        format!(
            "the audio.cpp server answered something that is not JSON: {}",
            trim(body)
        )
    })
}

/// Enough of a server's sentence to put in a log line.
fn trim(body: &str) -> String {
    let flat = body.trim().replace('\n', " ");
    if flat.len() > 200 {
        format!("{}…", &flat[..200])
    } else {
        flat
    }
}
