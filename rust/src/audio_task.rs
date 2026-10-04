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
    if body.get("ok").and_then(Value::as_bool) == Some(false) {
        return body.clone();
    }
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
                delta: None,
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
///
/// Like [`asr_ask`] it CUTS before it uploads: `align::ask_span` writes the window's scratch note and
/// hands over only the clip's name, so the plan that says which seconds this window is lives in that
/// note. Uploading a file nobody cut would ask the aligner about audio that was never sent.
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
            Ask { step: Task::Align.step(), job: clip, task: Task::Align.task() },
            model,
            &envelope,
        )?;
        parse_json(&body)
    }
}

/// One align window's cut arguments, read back from the `.plan` [`crate::align`] wrote beside it —
/// the same two-line file format as an ASR chunk's note, under `align/` instead of `asr/`. A missing
/// note is an error naming its path: the note is what proves this clip is the window being asked about.
fn align_plan(tree: &Option<Tree>, source: &str, clip: &str) -> Result<Vec<String>, String> {
    let dir = match tree {
        Some(tree) => tree.align_scratch(source),
        None => scratch_dir(),
    };
    let path = dir.join(format!("{clip}.plan"));
    let text = std::fs::read_to_string(&path).map_err(|why| format!("{}: {why}", path.display()))?;
    let plan: Vec<String> = text
        .lines()
        .next()
        .unwrap_or_default()
        .split_whitespace()
        .map(str::to_string)
        .collect();
    if plan.is_empty() {
        return Err(format!("{}: no cut arguments in the scratch note", path.display()));
    }
    Ok(plan)
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
        // The diarizer's windows live in `diar/`, not beside the ASR's chunks: `asr::run` clears `asr/`
        // when it succeeds, so a window left in there would outlive every stage's cleanup.
        let dir = match tree {
            Some(tree) => tree.diar_scratch(source),
            None => scratch_dir(),
        };
        write_scratch_at(&dir, &chunk, &plan, &note)?;
        let out = dir.join(&chunk);
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).map_err(|why| format!("{}: {why}", parent.display()))?;
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
///
/// `silences` are §F1.4 S2's quiet stretches, measured once by the caller over `voice16k.wav`: a cut that
/// lands inside one costs nothing, while one mid-sentence puts a word across two requests.
///
/// `log` is the dial's sink: the chunking flow has two things worth saying out loud (a halved ceiling, an
/// empty transcript) and they arrive in the outcome rather than in the words. Eight arguments because the
/// ask needs all of them to name one request — the tree, the endpoint's settings, the recording, its model
/// and language, its length and its silences — so the count stays rather than growing a struct no caller
/// fills any more honestly.
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
            tree.as_ref().expect("a session folder to write the chunks into"),
            source,
            duration,
            model,
            language,
            silences,
            asr_ask(tree, paths, source, model, language),
        )?;
        // §F1.4 S1 and S4 say things the user has to see — "no room for that much audio", "no speech
        // found" — and they live in the outcome, not in the words. Dropping them made a halved ceiling and
        // an empty transcript both look like a silent success, so they go to the door's log sink the way
        // `separate_run::run_task` hands over its own.
        for line in &outcome.logs {
            log(line);
        }
        // §F1.4 S3's per-chunk progress ("recognising speech i/n") is recorded the same way and goes to
        // the same sink: a run that cut four pieces has to say so four times, or the bar looks stuck.
        for line in &outcome.steps {
            log(line);
        }
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

/// The keys that name the speech half and everything else, however the model spelled them. A reply may
/// also answer one of them with an ARRAY of files (`{"rest": ["/u/a.wav", "/u/b.wav"]}`), which is a
/// rest made of more than one file rather than a mistake.
const VOICE_KEYS: [&str; 2] = ["voice", "vocals"];
const REST_KEYS: [&str; 4] = ["rest", "accompaniment", "nonvoice", "instrumental"];

/// What one chunk's separation reply gave back: which stem holds the speech (or `None` when the chunk
/// was silence, which is what makes S6's loudness rule meaningful), where each voice file lives on the
/// server, and where every other file does — in order, because a multi-stem answer's rest is their sum
/// and the order is the only thing that keeps them from being mixed twice.
#[derive(Debug)]
pub struct ChunkStems {
    pub voice: Option<usize>,
    pub voices: Vec<ServerPath>,
    pub rests: Vec<ServerPath>,
}

/// Read one chunk's stems out of a tasks-run reply, however the model spelled them.
///
/// Three shapes are accepted, all of them real answers a local server gives:
/// * `{"voice": ..., "rest": ...}` — the two-way answer [`stem_paths`] already reads;
/// * `{"stems": [{"name": "vocals", "path": ...}, ...]}` — a RoFormer's named set;
/// * a flat object whose values are paths (`{"drums": ..., "bass": ..., "other": ..., "vocals": ...}`)
///   — HTDemucs, four stems, no names field.
///
/// A name counts as the voice when it contains `voice` or `vocal`; everything else is rest. Silence is
/// `voice: null`/absent-with-no-voices, and a reply that carries NO paths at all is an error naming the
/// reply, never an empty answer: a chunk that came back with nothing in it is not a chunk that heard
/// nothing (the same rule [`asr_answer`] applies to its text).
pub fn chunk_stems(body: &Value) -> Result<ChunkStems, String> {
    let reply = answer(body);
    let mut voices: Vec<ServerPath> = Vec::new();
    let mut rests: Vec<ServerPath> = Vec::new();
    let mut voice_index: Option<usize> = None;
    // An explicit failure is reported before anything is parsed: `answer` keeps the whole body for
    // exactly this case, so the sentence the server wrote is what reaches the log.
    if reply.get("ok").and_then(Value::as_bool) == Some(false) {
        let why = reply.get("error").and_then(Value::as_str).unwrap_or("no reason given");
        return Err(format!("the audio.cpp server refused the separation: {why}"));
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
        // Flat object: pairs in the reply's own order, keys kept for the voice test. The named loop
        // below is what fills the lists from a `"stems"` array; a bare `"voice"`/`"rest"` pair goes
        // through `push_paths`, which also accepts an ARRAY of files for one half.
        if let Some(map) = reply.as_object() {
            for (key, value) in map {
                // Only a key that MEANS one of the halves is read as such; an HTDemucs object's `other`
                // is an instrument name on the same footing as `drums` and `bass`, so it stays in the
                // named loop below rather than being claimed here.
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
        // A reply with no paths is either silence or a shape nobody recognised, and only one of those is
        // normal: a reply that looks like a mistake is shown, not swallowed.
        let looks_silent = reply.get("voice").is_some()
            || reply.get("silence").and_then(Value::as_bool) == Some(true);
        if !looks_silent {
            return Err(format!(
                "the audio.cpp server answered {body} -- no stem paths in it"
            ));
        }
        return Ok(ChunkStems { voice: None, voices, rests });
    }
    Ok(ChunkStems { voice: voice_index, voices, rests })
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
                delta: None,
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
    write_scratch_at(&dir, file, plan, body)
}

/// The note itself, wherever the calling stage keeps its scratch. Each stage names its own folder
/// (`asr/`, `align/`, `diar/`) because a stage's cleanup deletes exactly that folder — a window written
/// into another stage's scratch is a file nobody ever removes.
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

/// The same rule for a door built without a session handle: name the folder the stage keeps its scratch
/// in, and the cut lands there anyway. `transcribe_source` holds the tree but hands its closures an
/// `Option<Tree>` clone, so without this the ASR's chunks would fall outside every stage's folder and
/// outside the session — invisible to the cleanup that is supposed to keep them.
fn scratch_folder(tree: &Option<Tree>, fallback: impl FnOnce(&Tree) -> std::path::PathBuf) -> std::path::PathBuf {
    match tree {
        Some(tree) => fallback(tree),
        None => scratch_dir(),
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
    log: &mut impl FnMut(&str),
) -> Result<crate::transcribe::Passage, String> {
    let conf = crate::settings::read(paths).unwrap_or_default();
    let asr_model = services::asr_model(&conf).to_string();
    let diar_model = services::diar_model(&conf).to_string();
    // §F1.5 S1: a configured aligner the box does not declare is NOT replaced and NOT fatal. Cut points
    // come off the waveform either way, so the id is left in the box, the reason is said, and the pass
    // goes on with the ASR's own times. `align::not_served` already spells that sentence, so the `Err`
    // is logged rather than raised; the url comes from the settings, not a `""`, or the line would say
    // which server refuses without naming it.
    let endpoint = services::endpoint_for(paths, Server::Audio, |name| std::env::var(name).ok());
    let models = catalog(&Some(tree.clone()), paths, "align")?;
    let aligners = match crate::align::aligners(&conf.align_model, &endpoint.url, &models) {
        Ok(ids) => ids,
        Err(refusal) => {
            log(&refusal);
            Vec::new()
        }
    };
    // align's own sentences are collected in the align closure and appended to `Passage.logs` at the
    // end, so they stay in the aligner's own order instead of being printed before it ran.
    let align_lines: std::rc::Rc<std::cell::RefCell<Vec<String>>> = Default::default();
    // §F1.4 S2/§F1.5 S2: the quiet stretches of THIS recording, measured once and handed to both
    // chunking steps so one definition of "quiet" moves both the ASR's cuts and the aligner's windows.
    // A detector that is not there or fails outright is not a failure: no silences only means every cut
    // lands on the clock, which is what `asr::cut_points` says an empty list means.
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
    // S4 is only knowable after S3 wrote `words.json`: the aligner places THIS pass's words, so the
    // decision arrives as a closure over the words the pass hands back rather than as a value decided
    // before anything was transcribed. `align::run` writes `words.aligned.json` itself; this only says
    // whether it answered, and `Failed` keeps the server's own sentence (§Failure-is-specific).
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
        transcribe_asr(&held, paths, source, &asr_model, language, duration, &silences, log),
        diar_ask(&held, paths, source, &diar_model, duration),
    )?;
    // `speech_stage` prints `passage.logs` in order after this returns, so appending here puts the
    // aligner's own sentences (S1's picker line, S6's bare-seconds warning) in the run's log without
    // the align closure ever touching the sink: it returns one `Align`, and `&mut log` belongs to the
    // ASR closure it shares the call's borrow scope with.
    passage.logs.extend(align_lines.borrow().iter().cloned());
    Ok(passage)
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
