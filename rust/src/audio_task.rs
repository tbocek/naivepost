//! The audio.cpp task door: the one place `POST /v1/tasks/run` is dialed.
//!
//! §02-services.md §1 lists the endpoint next to the four things audio.cpp serves — speech to text,
//! alignment, diarization, voice separation — and §5 confirms the envelope as `{"model","request"}`,
//! which [`crate::bodies`] builds. Before this module nothing in the program ever sent it: the flow
//! modules ([`crate::asr`], [`crate::align`], [`crate::transcribe`], [`crate::separate`]) each take an
//! `ask`/`join` closure "the real ▶ hands over" (their own words) and nothing did. Every function here
//! is therefore a closure supplier: it hands the flow the ask it was written to be given, and the ask
//! dials the real server through [`crate::server_leg`], so a test points the Settings box at a
//! loopback fake exactly as the shipped legs do.
//!
//! What this module owns, because no other module can:
//! - **The endpoint, re-read per request** (§1: "the settings file is re-read per request (each URL,
//!   key and model id)"), through [`crate::services::endpoint_for`] so audio.cpp's env precedence
//!   lives in one place.
//! - **The missing-model sentence.** A run that asks for a model the server does not serve gets
//!   [`crate::services::missing_model`], which carries §1's compiled-in install hint. Until now that
//!   wording only reached a log from the Settings dialog's Test buttons.
//! - **The reply shapes** below.

use std::path::Path;

use serde_json::Value;

use crate::bodies::{self, ServerPath};
use crate::layout::Tree;
use crate::requests::{Turn, Word, WordsDoc};
use crate::transcribe::SAMPLE_RATE;
use crate::server_leg;
use crate::services::{self, Kind, Need, Server};

/// What one dial needs to know beyond the body: who is asking (the step's word, which lands in the
/// `requests.tsv` step column), which job of it (`asr/c03.wav`, so a failed chunk is comparable
/// against its audio), and what §1 task the request names — the same word [`Need::task`] takes, so
/// the missing-model sentence says "Prepare needs `asr` there".
#[derive(Debug, Clone, Copy)]
pub struct Ask<'a> {
    pub step: &'a str,
    pub job: &'a str,
    pub task: &'a str,
}

/// The reply shape §5's three task kinds answer with, as read here. The spec gives the envelope and
/// the audio path but not the body of a task reply, so this is a decision written down: the reply is
/// read leniently, at the top level or under `result`, and a shape that is not one of these is an
/// `Err` naming the server and showing the reply rather than a guess — an aligner already does this
/// ([`crate::align::parse_words`]) and a silent empty answer is how a wrong guess gets filed as work.
fn answer(body: &Value) -> Value {
    body.get("result").cloned().unwrap_or_else(|| body.clone())
}

/// Speech to text's answer: the chunk's text, and its word times if the server sent them.
///
/// Accepted spellings: `text | transcript` for the words heard; `words | alignment` for the times,
/// each entry a word in [`crate::align::parse_words`]'s senses. A text-only reply is fine — that is
/// what a model that only transcribes sends — but a reply with neither is reported, because a chunk
/// that came back with nothing in it is not a chunk that heard nothing.
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
        Some(text) => Ok(WordsDoc { text: text.to_string(), words }),
        None if !words.is_empty() => Ok(WordsDoc { text: joined(&words), words }),
        None => Err(format!(
            "the audio.cpp server answered {body} -- no text and no word times to read it as"
        )),
    }
}

/// The text an ASR's word times spell, for a server that sends only times and tokens.
fn joined(words: &[Word]) -> String {
    words.iter().map(|word| word.word.as_str()).collect::<Vec<_>>().join(" ")
}

/// Diarization's answer: speaker turns as samples.
///
/// `turns | speakers | diarization` at either level, each entry read with [`turn`] and then sorted by
/// start, because placement anchors on order and a server that answers out of order would otherwise
/// place against a scrambled timeline. An empty list is an honest answer: nobody spoke.
pub fn diar_answer(body: &Value) -> Result<Vec<Turn>, String> {
    let reply = answer(body);
    let list = ["turns", "speakers", "diarization"]
        .iter()
        .filter_map(|key| reply.get(*key).and_then(Value::as_array))
        .next()
        .ok_or_else(|| format!("the audio.cpp server answered {body} -- no speaker turns in it"))?;
    let mut turns = Vec::new();
    for entry in list {
        if let Some(turn) = turn(entry) {
            turns.push(turn);
        }
    }
    turns.sort_by_key(|turn| turn.start_sample);
    Ok(turns)
}

/// One turn, its edges as samples (seconds and milliseconds also read: the shape varies per server)
/// and its speaker as a number or a name hashed to one, since a diarizer that labels voices by name
/// still has to answer in [`Turn`]'s terms.
fn turn(entry: &Value) -> Option<Turn> {
    let start = sample(entry, &["start_sample", "start", "from"])?;
    let end = sample(entry, &["end_sample", "end", "to"])?;
    let speaker = entry
        .get("speaker_id")
        .or_else(|| entry.get("speaker"))
        .map(|value| match value {
            Value::Number(number) => number.as_u64().unwrap_or_default() as u32,
            Value::String(name) => name.bytes().fold(0u32, |hash, byte| hash.wrapping_mul(31).wrapping_add(byte as u32)),
            _ => 0,
        })
        .unwrap_or(0);
    (end > start).then_some(Turn { start_sample: start, end_sample: end, speaker_id: speaker })
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
///
/// A stems task answers with file paths, not bytes — every audio.cpp task works inside the server's
/// own folders (that is why [`ServerPath`] exists) — so this returns server paths to be downloaded
/// and joined by the caller's ffmpeg.
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
    let rest = path(&["rest", "accompaniment", "other", "stem_rest"])
        .ok_or_else(|| format!("the audio.cpp server answered {body} -- no accompaniment stem path"))?;
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
///
/// The path a task names is always one an upload handed back, never a local path — audio.cpp reads
/// inside its own container, so a local path is a file-not-found minutes later. Hence the
/// [`ServerPath`] type rather than a `&str` here.
pub fn task_body(kind: Task, model: &str, audio: &ServerPath, language: &str, text: Option<&str>) -> Value {
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
///
/// `paths` is explicit rather than read from the environment inside so the caller's own resolved
/// settings folder is what is dialled (the module's documented style, and it keeps a test off the
/// real `$HOME`); `tree` is `None` when no project is open, which drops the timed row exactly as the
/// settings probes do — a task run without a project has nowhere to write `requests.tsv`.
///
/// Failure is specific: a non-200 names the model, the status and the server's own sentence; a reply
/// saying the model is not there costs one extra `GET /v1/models` and comes back as
/// [`services::missing_model`], which is where §1's install hint lives.
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
    // §1: at most this model's Settings slot count of requests on the wire at once. Taken BEFORE the
    // socket and released when the reply is in, so the wait sits outside the measured call and lands in
    // the row's own wait column instead of inflating its time on the wire.
    let conf = crate::settings::read(paths).unwrap_or_default();
    let slots = crate::slot_gate::slots_for_task(&conf, ask.task);
    let (_slot, waited) = crate::slot_gate::hold(&endpoint.url, model, slots, ask.step);
    let base = server_leg::base_row(Server::Audio, model, Kind::RunTask.method(), "", ask.step, ask.job);
    // The wait goes in as milliseconds beside the request, not on `base`: `call_dialed` builds every timing
    // column itself and would drop a field it did not measure (see its `waited_ms`).
    let waited_ms = (waited * 1000.0).round() as i64;
    let reply = match tree {
        Some(tree) => server_leg::call_dialed(
            tree,
            &base,
            &server_leg::Dial {
                server: Server::Audio,
                endpoint: &endpoint,
                kind: Kind::RunTask,
                job: None,
                body: Some(&envelope),
                filename: None,
                cancelled: &cancelled,
                liveness: None,
                waited_ms,
            },
        )
        .1,
        None => server_leg::send(Server::Audio, &endpoint, Kind::RunTask, None, Some(&envelope), &cancelled),
    }?;
    if reply.ok() {
        return Ok(reply.body);
    }
    if says_missing(&reply.status, &reply.body, model) {
        let catalogue = catalog(tree, paths, ask.step)?;
        return Err(services::missing_model(
            &endpoint.url,
            &catalogue,
            model,
            Need { task: ask.task, step: ask.step },
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

/// Whether a failure is the server saying it does not have this model. Both ways it says so: a 404,
/// and a 4xx/5xx whose body names the wanted id beside a word that means "not here". A 500 that
/// does not mention the model is an ordinary failure and stays one.
fn says_missing(status: &u16, body: &str, model: &str) -> bool {
    if *status == 404 {
        return true;
    }
    let lower = body.to_lowercase();
    !model.trim().is_empty()
        && lower.contains(&model.to_lowercase())
        && ["not found", "unknown model", "not loaded", "not served", "no such model"]
            .iter()
            .any(|phrase| lower.contains(phrase))
}

/// The server's own model list, for the message that names what it does serve.
pub fn catalog(tree: &Option<Tree>, paths: &crate::settings::Paths, step: &str) -> Result<Vec<services::AudioModel>, String> {
    let endpoint = services::endpoint_for(paths, Server::Audio, |name| std::env::var(name).ok());
    let cancelled = crate::cancel_leg::cancel_check_now();
    let base = server_leg::base_row(Server::Audio, "", Kind::ListModels.method(), "", step, "");
    let reply = match tree {
        Some(tree) => server_leg::call(tree, &base, Server::Audio, &endpoint, Kind::ListModels, None, None, &cancelled).1,
        None => server_leg::send(Server::Audio, &endpoint, Kind::ListModels, None, None, &cancelled),
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
/// This is the half `asr::run`'s own doc names — "the real ▶ hands over ffmpeg's cut, the upload and the
/// ASR request; a test hands over a closure" — so it does all three:
///
/// - **The cut.** `run_at` writes `asr/cNN.wav.plan` immediately before it asks, and that note's first line
///   is `cut_plan`'s arguments (`-ss <from> -t <span>`). Reading the span back from the flow's own scratch is
///   what lets an ask know which chunk it is answering about: `asr::run` passes a file name and nothing else.
/// - **The upload.** A task names a file inside the server's folders, so a local path is no answer at all;
///   the bytes go up as raw under the chunk's own name, which is `Kind::Upload`'s contract (§1).
/// - **The request**, answered through [`asr_answer`] like any other ASR reply.
pub fn asr_ask<'a>(
    tree: &'a Option<Tree>,
    paths: &'a crate::settings::Paths,
    source: &'a str,
    model: &'a str,
    language: &'a str,
) -> impl FnMut(&str) -> Result<crate::asr::Answer, String> + 'a {
    move |file: &str| {
        let (plan, _) = read_scratch(tree, source, file)?;
        let out = scratch_out(tree, source, file);
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
            Ask { step: Task::Asr.step(), job: file, task: Task::Asr.task() },
            model,
            &envelope,
        )?;
        let parsed: Value = parse_json(&body)?;
        let doc = asr_answer(&parsed)?;
        Ok(crate::asr::Answer { text: doc.text, words: doc.words })
    }
}

/// The `ask` [`crate::align::run`] wants: model, clip and text in, the aligner's word list out.
///
/// The model arrives from the aligner picker (§1: an aligner is chosen by task, not by the box), so
/// unlike [`asr_ask`] this one takes the id per call and dials whichever the picker named.
pub fn align_ask<'a>(
    tree: &'a Option<Tree>,
    paths: &'a crate::settings::Paths,
    language: &'a str,
) -> impl FnMut(&str, &str, &str) -> Result<Value, String> + 'a {
    move |model: &str, clip: &str, text: &str| {
        let audio = upload_chunk(tree, paths, clip)?;
        let envelope = task_body(Task::Align, model, &audio, language, Some(text));
        let body = dial(
            tree,
            paths,
            Ask { step: Task::Align.step(), job: clip, task: Task::Align.task() },
            model,
            &envelope,
        )?;
        parse_json(&body)
    }
}

/// The diarization turns [`crate::transcribe::run`] wants: nothing in but this source's turns out.
///
/// The window is not chosen here — [`crate::diar_scan::run`] walks §2's 90 → 45 → 25 s ladder and asks once
/// per window on the recording, which is why this closure takes the span it is being asked about rather than
/// assuming the whole file. One request per window is also what makes an answer mean anything: a server sent
/// two windows at once has no reason to call the same person `SPEAKER_01` twice.
pub fn diar_window_ask<'a>(
    tree: &'a Option<Tree>,
    paths: &'a crate::settings::Paths,
    source: &'a str,
    model: &'a str,
) -> impl Fn(f64, f64) -> Result<Vec<Turn>, String> + 'a {
    move |from: f64, to: f64| {
        let chunk = format!("d{:04}.wav", (from * 10.0) as i64);
        let plan = crate::asr::cut_plan(from, to);
        // The scratch note records the request the way the door spells every other one, so a failed window
        // is comparable against its audio without asking again — the same file `asr_ask` reads back.
        // The path in it is the chunk's own name: nothing has been uploaded yet at this point.
        let note = serde_json::json!({ "audio": chunk, "language": "" });
        write_scratch(tree, source, &chunk, &plan, &note)?;
        let out = scratch_out(tree, source, &chunk);
        if let Some(dir) = out.parent() {
            std::fs::create_dir_all(dir).map_err(|why| format!("{}: {why}", dir.display()))?;
        }
        let input = tree
            .as_ref()
            .map(|tree| tree.voice16k_wav(source))
            .unwrap_or_else(|| std::path::PathBuf::from(source));
        cut_from(paths, &input, &plan, &out)?;
        let bytes = std::fs::read(&out).map_err(|why| format!("{}: {why}", out.display()))?;
        let audio = upload_bytes(tree, paths, &chunk, &bytes)?;
        let envelope = task_body(Task::Diarize, model, &audio, "", None);
        let body = dial(
            tree,
            paths,
            Ask { step: Task::Diarize.step(), job: &chunk, task: Task::Diarize.task() },
            model,
            &envelope,
        )?;
        // A window's answer is in that window's own seconds; the ladder's arithmetic is all on the
        // recording's clock, so every turn is moved back before anyone looks at it.
        Ok(diar_answer(&parse_json(&body)?)?
            .into_iter()
            .map(|turn| Turn {
                start_sample: turn.start_sample + (from * SAMPLE_RATE as f64) as u64,
                end_sample: turn.end_sample + (from * SAMPLE_RATE as f64) as u64,
                speaker_id: turn.speaker_id,
            })
            .collect())
    }
}

/// The `diarize` [`crate::transcribe::run`] wants: nothing in but this source's turns out.
pub fn diar_ask<'a>(
    tree: &'a Option<Tree>,
    paths: &'a crate::settings::Paths,
    source: &'a str,
    model: &'a str,
    duration: f64,
) -> impl FnOnce() -> Result<Vec<Turn>, String> + 'a {
    move || {
        // §2's ladder, walked: the first window size whose plan fits serves, and a rung that is too small
        // for its own anchor moves on instead of failing the step.
        let rung = crate::diar_scan::run(duration, model, |windows| {
            let one = diar_window_ask(tree, paths, source, model);
            windows.iter().map(|(from, to)| one(*from, *to)).collect()
        })?;
        Ok(rung.turns)
    }
}

/// The `asr` [`crate::transcribe::run`] wants: this source's `words.json` document out.
///
/// The recording is chunked, not uploaded whole: `crate::asr::run` decides the pieces (60 s on the qwen3
/// family, 300 s elsewhere, halved on an out-of-memory answer — §F1.3 S1) and hands each piece's file name
/// to [`asr_ask`], which cuts, uploads and dials it. One request for a two-hour take would be a request for
/// a two-hour decode on the server.
pub fn transcribe_asr<'a>(
    tree: &'a Option<Tree>,
    paths: &'a crate::settings::Paths,
    source: &'a str,
    model: &'a str,
    language: &'a str,
    duration: f64,
) -> impl FnOnce() -> Result<WordsDoc, String> + 'a {
    move || {
        let outcome = crate::asr::run(
            tree.as_ref().expect("a session folder to write the chunks into"),
            source,
            duration,
            model,
            language,
            &[],
            asr_ask(tree, paths, source, model, language),
        )?;
        Ok(WordsDoc { text: outcome.text, words: outcome.words })
    }
}

/// The envelope a separation request carries. §5 lists no body for it, so this is a decision written
/// down: the same envelope every task run takes (`{"model","request"}`) with the recording's server
/// path and the chunk the split is asked for, since a stems task answers with stem paths for the audio
/// it was given and nothing else.
///
/// Public because the run path ([`crate::separate_run`]) puts a decoded recording on the server before
/// it asks: the thing a task may name is always an upload's answer, so the split needs the upload and
/// its envelope from the same door, and naming it in one place keeps the two halves of the request
/// describing the same bytes.
pub fn separation_body(model: &str, audio: &ServerPath, chunk: &str) -> Value {
    serde_json::json!({
        "model": model,
        "request": { "audio": audio.as_str(), "chunk": chunk },
    })
}

/// Put one chunk on the server and bring back the path its tasks may name. The bytes come from the
/// scratch folder the flow already wrote the file into ([`crate::layout::Tree::asr_scratch`] and
/// [`crate::layout::Tree::align_scratch`]), so the upload and the envelope describe the same bytes
/// the scratch plan names.
fn upload_chunk(
    tree: &Option<Tree>,
    paths: &crate::settings::Paths,
    file: &str,
) -> Result<ServerPath, String> {
    let name = file.rsplit('/').next().unwrap_or(file);
    let local = scratch_file(name);
    let bytes = std::fs::read(&local).map_err(|why| format!("{}: {why}", local.display()))?;
    upload_bytes(tree, paths, name, &bytes)
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
    let payload = String::from_utf8_lossy(bytes).into_owned();
    let base = server_leg::base_row(Server::Audio, "", Kind::Upload.method(), "", "upload", name);
    let reply = match tree {
        Some(tree) => server_leg::call_dialed(
            tree,
            &base,
            &server_leg::Dial {
                server: Server::Audio,
                endpoint: &endpoint,
                kind: Kind::Upload,
                job: None,
                body: Some(&payload),
                filename: Some(name),
                cancelled: &cancelled,
                liveness: None,
                waited_ms: 0,
            },
        )
        .1,
        None => server_leg::send_named(
            Server::Audio,
            &endpoint,
            Kind::Upload,
            None,
            Some(&payload),
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
    ServerPath::from_upload(&path).ok_or_else(|| format!("the upload named {path:?}, which is not a server path"))
}

/// The scratch folder this door's chunks and uploads live in, under the session when one is open.
fn scratch_dir() -> std::path::PathBuf {
    std::env::current_dir().unwrap_or_default().join("scratch")
}

/// The scratch note the flow modules keep (`plan` plus the body, so a failed chunk is comparable
/// against its audio without asking again) — the same file `asr::make_scratch` writes, written here
/// because this door is the half that actually runs.
///
/// Public for the one test that drives [`asr_ask`] on its own: in the program the note is always written
/// by `asr::run_at` immediately before it asks, and a test that calls the ask directly has to stand in for
/// that writer. It is not a seam the program substitutes — the cut, upload and dial are the real ones.
pub fn write_scratch_for_test(
    tree: &Option<Tree>,
    source: &str,
    file: &str,
    plan: &[String],
    body: &serde_json::Value,
) -> Result<(), String> {
    write_scratch(tree, source, file, plan, body)
}

fn write_scratch(
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
    std::fs::create_dir_all(&dir).map_err(|why| format!("{}: {why}", dir.display()))?;
    let text = format!("{}\n{}\n", plan.join(" "), body);
    std::fs::write(dir.join(format!("{file}.plan")), text)
        .map_err(|why| format!("{}: {why}", dir.join(file).display()))
}

/// Read one chunk's scratch note back: ffmpeg's cut arguments on the first line, the request body on the
/// second — the file format [`write_scratch`] and `asr::make_scratch` both write. The door needs the plan
/// because that is the only place a chunk's span is recorded for whoever runs the cut.
fn read_scratch(tree: &Option<Tree>, source: &str, file: &str) -> Result<(Vec<String>, String), String> {
    let dir = match tree {
        Some(tree) => tree.asr_scratch(source),
        None => scratch_dir(),
    };
    let path = dir.join(format!("{file}.plan"));
    let text = std::fs::read_to_string(&path).map_err(|why| format!("{}: {why}", path.display()))?;
    let mut lines = text.lines();
    let plan: Vec<String> = lines
        .next()
        .unwrap_or_default()
        .split_whitespace()
        .map(str::to_string)
        .collect();
    if plan.is_empty() {
        return Err(format!("{}: no cut arguments in the scratch note", path.display()));
    }
    Ok((plan, lines.next().unwrap_or_default().to_string()))
}

/// Where one chunk's cut audio goes: beside its `.plan`, under the session when one is open.
fn scratch_out(tree: &Option<Tree>, source: &str, file: &str) -> std::path::PathBuf {
    match tree {
        Some(tree) => tree.asr_scratch(source).join(file),
        None => scratch_dir().join(file),
    }
}

/// ffmpeg's cut of one chunk from a named input, through the same spawn path the run's stages use so a
/// failure names the tool and its tail. `Err` when the output is not there afterwards: a decode that
/// produced nothing is not an upload, and naming the file is the useful half of that sentence.
fn cut_from(paths: &crate::settings::Paths, input: &Path, plan: &[String], out: &Path) -> Result<(), String> {
    // `-ss` goes AFTER the input on purpose: the door cuts `voice16k.wav`, which §F1.3 S1 decoded to pcm,
    // and a seek before the input is exact only for formats with an index. Here it must be exact — a chunk
    // whose start is a keyframe away would have its word times shifted by the difference, and `asr::shift`
    // trusts the span the scratch note recorded.
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

/// Prepare's speech stage for one recording: `crate::transcribe::run`, with every closure it asks for
/// supplied from real dials — ASR, diarization, and the aligner chosen by task off the server's own
/// list (§1: “an aligner is chosen by task, not id”). This is what a ▶ Prepare press reaches for each
/// recording row; before it the stage logged its intent and asked no server.
pub fn transcribe_source(
    tree: &Tree,
    paths: &crate::settings::Paths,
    source: &str,
    duration: f64,
    language: &str,
) -> Result<crate::transcribe::Passage, String> {
    let _ = tree.voice16k_wav(source);
    let conf = crate::settings::read(paths).unwrap_or_default();
    let asr_model = services::asr_model(&conf).to_string();
    let diar_model = services::diar_model(&conf).to_string();
    let aligners = crate::align::aligners(&conf.align_model, "", &catalog(&Some(tree.clone()), paths, "align")?)?;
    // An aligner served is an alignment step attempted through the dial; whether it answered is this
    // door's to say, so the pass runs first and its outcome decides `Done` or `Failed` — `Failed`
    // keeps the server's own sentence, which is what S4 logs or stops on (§Failure-is-specific).
    let align = match aligners.first() {
        None => crate::transcribe::Align::None,
        Some(_) => {
            let held = Some(tree.clone());
            let doc = crate::requests::read_words(tree, source)?.unwrap_or_default();
            match crate::align::run(
                tree,
                source,
                duration,
                language,
                &[],
                &[],
                &doc.words,
                &doc.text,
                &aligners,
                align_ask(&held, paths, language),
            ) {
                Ok(_) => crate::transcribe::Align::Done,
                Err(why) => crate::transcribe::Align::Failed(why),
            }
        }
    };
    let held = Some(tree.clone());
    crate::transcribe::run(
        tree,
        source,
        duration,
        &asr_model,
        align,
        transcribe_asr(&held, paths, source, &asr_model, language, duration),
        diar_ask(&held, paths, source, &diar_model, duration),
    )
}

/// Where a recording row's `source` word comes from: its file name minus the extension, the same
/// rule `prepare_run::lane` and `frames.rs` use, so the folders this door writes are the ones every
/// later step reads.
pub fn source_lane(path: &str) -> String {
    let base = path.rsplit('/').next().unwrap_or(path);
    base.split_once('.')
        .map(|(stem, _)| stem.to_string())
        .unwrap_or_else(|| base.to_string())
}

/// The file a flow's scratch holds, wherever the caller's cwd puts the session. Searched in the two
/// scratch folders rather than guessed at: an ASR chunk and an align window never share a name
/// (`c03.wav` against `w000001500.wav`), so the first hit is the right one.
fn scratch_file(name: &str) -> std::path::PathBuf {
    let cwd = std::env::current_dir().unwrap_or_default();
    ["asr", "align"]
        .iter()
        .map(|folder| cwd.join("scratch").join(folder).join(name))
        .chain(std::iter::once(scratch_dir().join(name)))
        .find(|path| path.exists())
        .unwrap_or_else(|| cwd.join(name))
}

/// The server path a task is asked about, from the file name the flow module chose.
///
/// A reply body that is not JSON is a protocol disagreement, said as one with the reply quoted.
fn parse_json(body: &str) -> Result<Value, String> {
    serde_json::from_str(body).map_err(|_| format!("the audio.cpp server answered something that is not JSON: {}", trim(body)))
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
