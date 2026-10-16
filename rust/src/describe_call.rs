//! Describe's request leg: the vision job that turns one chunk of frames into EVENT lines and a
//! running STATE, over HTTP. §02-services.md §2 names the row — LLM with vision, thinking OFF, one
//! EVENT per frame plus a STATE, four frames a call.
//!
//! [`crate::describe`] holds every rule of F1.7 and deliberately makes no call (“the model call is
//! not made here at all”). This is the other half: it gathers what those rules read from disk, builds
//! the body through [`crate::llm_request`] so the sampling five and the thinking switch come from the
//! one place every chat request is built, sends it through [`crate::llm_retry_leg`] (which writes the
//! timed `requests.tsv` row and rides §09 §3's patience), and hands the answer back to
//! [`crate::describe::apply_tool`] — so the §3.2 tool surface, not a copy of it, decides what an EVENT
//! is. Then the chunk's rows go to disk through [`crate::describe::write_chunk`].
//!
//! The seam a test drives this through is the Settings address, as every other live job uses: point
//! `LLM_SERVER` at a local fake and the same endpoint resolution a typed address goes through reaches
//! it. Nothing here takes a canned reply.

use crate::describe::{self, Speech, Stamp};
use crate::exchanges::{Message, Mode, Part};
use crate::layout::Tree;
use crate::llm_request;
use crate::roles::Job;
use crate::services::{self, Server};
use crate::settings::{self, Paths};
use crate::textfmt::FrameEvent;
use crate::tool_reads;
use crate::describe_scale;

use crate::tools::describe::Batch;
use crate::tools::Tool;

/// The step name this job's cache entries and timed rows carry: the same word `describe::STEP` uses,
/// so a `requests.tsv` row and the event log folder name the stage in §04 S6's order.
pub const STEP: &str = describe::STEP;

/// How many times one chunk may be asked before the row gives up on it. §3.2 gives describe no retry
/// budget of its own, so this takes the same count the fixer's blocks get (`P.machine.fixTries`): a
/// chunk whose frames the model would not answer is re-asked a couple of times and then reported,
/// which is cheaper than a rule no section wrote.
fn attempts() -> u32 {
    crate::fix_transcripts::attempts()
}

/// The describe stage of the Prepare walk: one row per footage source.
///
/// A dial (`program: VISION_TASK`), not a spawn — the work is a chat request. The frames have to be
/// on disk first, which is what the frames stage does, so this stage comes after it in
/// [`crate::run_stages::plan`] and a source with nothing extracted is left out here rather than
/// failing the run: §F1.7's own refusal is about exactly that case.
pub fn stage(project: &crate::project::Project) -> crate::run_stages::Stage {
    let tasks = project
        .sources
        .iter()
        .filter(|source| source.footage)
        .enumerate()
        .map(|(index, source)| crate::run_stages::Task {
            label: format!("describing {}", crate::audio_task::source_lane(&source.path)),
            program: crate::run_stages::VISION_TASK.to_string(),
            args: vec![source.path.clone(), index.to_string()],
            already_done: false,
            session: None,
        })
        .collect();
    crate::run_stages::Stage {
        track: crate::runqueue::TRACK_DESCRIBE,
        name: "describe",
        phase: 2,
        of: 2,
        kind: "chunk",
        tasks,
    }
}

/// The stage's work for one footage source, from a tree and settings folder the caller resolved:
/// choose the frames F1.6 extracted, chunk them at four, and ask once per chunk. [`run_task`] is the
/// walk's door, which resolves those two from the environment and hands them here — the same split
/// `run_stages::dial_task` makes, so a test can drive the real stage without owning the process's cwd.
pub fn stage_chunk(
    tree: &Tree,
    paths: &Paths,
    conf: &settings::Conf,
    lane: &str,
    source_index: Option<usize>,
    log: &mut impl FnMut(&str),
) -> Result<usize, String> {
    let frames_on_disk = describe::frames_on_disk(&tree.frames_dir(lane));
    // S0: an every-frame extraction stops this source here, before a request is paid for frames whose
    // spacing nothing chose. Local, not fatal: the run carries on with the sources that can be
    // described (§Failure-is-specific), which is why this answers Ok(0) rather than Err.
    if describe::is_every_frame(&frames_on_disk, describe::GRID) {
        log(&describe::every_frame_refusal(lane));
        return Ok(0);
    }
    let changes = describe::read_changes(tree, lane);
    let duration = frames_on_disk.last().copied().unwrap_or(0.0) + describe::GRID;
    let project = crate::project::load(tree.dir()).ok();
    let freq = frame_interval(project.as_ref());
    // Where this source's stored frames sit on the session clock: the index `stage` put on the row, so
    // the offset comes from the same placement every other stage reads.
    let index = source_index.unwrap_or(0);
    // S1: each recording heard alongside the video, named with its offset, said before any work --
    // the words are what the descriptions will be built from.
    for (audio, offset) in describe_scale::alongside(project.as_ref(), index) {
        log(&describe::hearing_log(lane, &audio, offset));
    }
    // S1: the copies that get sent are made here, by ffmpeg, once each. The frames on disk stay at the
    // video's own size; only these 896-wide copies go on the wire.
    let start = describe_scale::clock_start(project.as_ref(), index);
    describe_scale::ensure(tree, conf, lane, start, &frames_on_disk, log).map_err(|why| format!("{lane}: {why}"))?;
    let picked = describe::sorted(&describe::pick_frames(&changes, duration, describe::GRID, freq));
    if picked.is_empty() {
        log(&format!(">>> [{lane}] no frames to describe -- the frames folder is empty"));
        return Ok(0);
    }
    let mut answered = 0usize;
    let mut from_cache = 0usize;
    for start in describe::chunk_plan(&picked, describe::FRAMES_PER_REQ) {
        let batch = &picked[start..picked.len().min(start + describe::FRAMES_PER_REQ)];
        // The chunk's own span, exactly as `describe::message` is told it: last frame plus one grid
        // step, so the rows it writes end where the request said its frames covered.
        let start_second = picked[start];
        let end = batch.last().copied().unwrap_or(start_second) + freq;
        let outcome = run_chunk(
            tree,
            paths,
            conf,
            lane,
            batch,
            end,
            freq,
            &user_context(tree),
            log,
        )?;
        answered += 1;
        if outcome.cached {
            from_cache += 1;
        }
    }
    let _ = &changes;
    log(&describe::complete_log_cached(lane, answered, from_cache));
    Ok(answered)
}

/// The row [`crate::run_stages::drive_walk`] hands here: describe one footage source end to end.
///
/// Frame choice, scaling and the interval are read by [`stage_chunk`]; this only resolves the two
/// environment pieces the walk's rows do not carry.
pub fn run_task(
    task: &crate::run_stages::Task,
    log: &mut impl FnMut(&str),
) -> Result<(), String> {
    let paths = settings::from_environment()
        .ok_or("nowhere to keep the settings -- neither XDG_CONFIG_HOME nor HOME is set")?;
    let session = session_dir();
    let tree = Tree::new(&session)
        .map_err(|_| format!("{}: not a project folder to describe into", session.display()))?;
    let source = task.args.first().cloned().ok_or("the describe row names no source")?;
    let lane = crate::audio_task::source_lane(&source);
    let index: usize = task.args.get(1).and_then(|text| text.parse().ok()).unwrap_or(0);
    let conf = settings::read(&paths).unwrap_or_default();
    stage_chunk(&tree, &paths, &conf, &lane, Some(index), log)?;
    Ok(())
}

/// Where the User Context comes from: the session's own notes, which §F1.7 S3 puts at the head of the
/// message. Read from `naivepost.json`; absent means no context rather than a made-up one, because an
/// invented context is a claim about the recording no person wrote.
fn user_context(tree: &Tree) -> String {
    crate::project::load(tree.dir())
        .map(|project| project.context.clone())
        .unwrap_or_default()
}

/// The frame interval this project asks for — `P.project.frameInterval`, which the project file holds
/// as `interval` (the number F1.6 extracted on and F1.7 stamps with). A session that cannot be read
/// falls back to the same default `Project::default()` carries, so the two never disagree.
fn frame_interval(project: Option<&crate::project::Project>) -> f64 {
    project
        .map(|project| project.interval)
        .unwrap_or_else(|| crate::project::Project::default().interval)
}

/// The session folder the walk works in — the working folder's own, resolved the way every other dial
/// resolves it (§1's re-read-per-request seams all start here). Named for what it returns rather than
/// for a settings folder: `run_task` needs the SESSION to open the tree in, and only then the paths.
fn session_dir() -> std::path::PathBuf {
    crate::startup::session_dir(&std::env::current_dir().unwrap_or_default())
}

/// A chunk already answered by an earlier run costs no request: §F1.7 S2's resume rule, read through
/// [`describe::starts_already_logged`] so the rule stays the one F1.7 wrote (the chunk's START second
/// is the row whose presence proves the whole chunk was answered). Returns `None` when it is still to
/// ask, and the cached answer when it is not.
fn resume_answer(
    tree: &Tree,
    lane: &str,
    rows: &[describe::EventRow],
    batch: &[f64],
    interval: f64,
    history: &[String],
    key: &str,
) -> Option<Result<ChunkAnswer, String>> {
    if !describe::starts_already_logged(rows, batch, describe::FRAMES_PER_REQ).contains(&0) {
        return None;
    }
    match describe::cached_reply(tree, key) {
        Some(cached) => Some(
            apply_and_write(tree, lane, batch, interval, history, &cached)
                .map(|missing| ChunkAnswer { cached: true, missing }),
        ),
        // The rows say the chunk was answered but the cache has nothing for it (an old run before the
        // cache existed, or a cleared cache). Nothing is re-asked: the work is on disk, and paying a
        // request to reproduce it would be the resume rule failing in the other direction.
        None => Some(Ok(ChunkAnswer { cached: true, missing: 0 })),
    }
}

/// What one chunk came to.
pub struct ChunkAnswer {
    /// The reply came from `cache/llm/describe/` and cost no wire.
    pub cached: bool,
    /// Frames still without an EVENT after every attempt: empty is the normal answer.
    pub missing: usize,
}

/// One chunk of frames through the vision job: message, cache, dial, tool answers, and the two files
/// written for exactly what was answered.
///
/// Retried up to [`attempts`] times when `finish` reports frames still unanswered — the refusal text
/// goes into the next attempt's message, the same way the fixer re-asks, because the identical
/// question earns the identical answer.
/// One chunk's frames through the vision job: choose, cache, dial, answer through the §3.2 surface,
/// write the two files.
///
/// `stage_chunk` walks this once per chunk of four frames; each retry below is one more pass over the
/// same batch (a chunk the model left incomplete is re-asked with the refusal in front of it).
#[allow(clippy::too_many_arguments)]
fn run_chunk(
    tree: &Tree,
    paths: &Paths,
    conf: &settings::Conf,
    lane: &str,
    batch: &[f64],
    batch_end: f64,
    interval: f64,
    user_context: &str,
    log: &mut impl FnMut(&str),
) -> Result<ChunkAnswer, String> {
    let changes = describe::read_changes(tree, lane);
    let start = *batch.first().ok_or("an empty batch describes nothing")?;
    let end = batch_end;
    let rows = describe::read_events(tree, lane);
    let history = describe::seed_history(&rows);
    let state = describe::seed_state(describe::read_state(tree, lane).as_deref());
    let speech = speech_in(tree, lane, start, end);
    let mut stamps: Vec<Stamp> = Vec::new();
    let mut previous: Option<f64> = None;
    for at in batch {
        // `previous` walks the batch rather than staying `None`: `is_new_scene` answers "is a scene
        // change between this frame and the one before it", and asking with no previous for every
        // frame would mark the whole batch NEW SCENE.
        stamps.push(Stamp { at: *at, new_scene: describe::is_new_scene(*at, previous, &changes) });
        previous = Some(*at);
    }
    let message = describe::message(user_context, &state, start, end, &history, &speech, &stamps);
    let images = match read_images(tree, lane, batch) {
        Ok(images) => images,
        // A frame that was never scaled is not a describe failure to retry: the ingest step owes it,
        // and saying which picture is missing is the honest answer (§Failure-is-specific).
        Err(reason) => return Err(format!("{lane}: {reason}")),
    };
    let key = describe::cache_key(&message, &images);
    if let Some(answer) = resume_answer(tree, lane, &rows, batch, interval, &history, &key) {
        log(&describe::cached_note(1));
        return answer;
    }
    let mut refusal: Option<String> = None;
    for attempt in 0..attempts() {
        if let Some(cached) = describe::cached_reply(tree, &key) {
            let answered = apply_and_write(tree, lane, batch, interval, &history, &cached)?;
            log(&describe::cached_note(1));
            return Ok(ChunkAnswer { cached: true, missing: answered });
        }
        let text = match ask(tree, paths, &conf.model.clone(), &message, &images, refusal.take()) {
            Ok(body) => body,
            Err(reason) => {
                // A wire failure is not a refusal: asking a server that is not there again answers the
                // same way and spends an attempt on nothing (§Failure-is-specific).
                return Err(format!("{lane}: describe: {reason}"));
            }
        };
        let _ = attempt;
        let reply = match llm_request::reply_from(&text.0, &text.1) {
            Ok(reply) => reply,
            Err(reason) => {
                refusal = Some(reason);
                continue;
            }
        };
        if reply.tool_calls.is_empty() {
            // §3.2's shape is tool calls; a prose reply means the model answered around the surface.
            // The §3.2 lenient parse is what reads it, so an ordinary answer still counts.
            let answered = apply_and_write(tree, lane, batch, interval, &history, &reply.content)?;
            describe::store_reply(tree, &key, &reply.content)?;
            return Ok(ChunkAnswer { cached: false, missing: answered });
        }
        let calls = tool_calls_of(&reply);
        let mut held = batch_of(batch, interval, &history);
        // The answer the leg gave to a read this round asked for, if it asked for one. `run_chunk` keeps its
        // reply as the CALL list rather than the raw envelope, so the answer is written back into that list
        // before it is stored -- which is also what makes a cached chunk replay the same reads it did live.
        let mut answered: Vec<(String, String)> = Vec::new();
        for (name, args) in &calls {
            // §3.2's Reads row offers this job four things its own tool surface does not answer. Answering
            // them is what makes a read round possible: without it the model asks for its events or its
            // frames and gets silence, and the retry below then tells it about frames it was never given the
            // chance to use. Each answer is recorded beside the call it answers (below, in the cached reply)
            // and pushed to THIS stage's log, so the run and the exchange page both show what was handed back.
            if let Some(answer) = read_answer(tree, lane, user_context, &rows, batch, name, args) {
                log(&format!(">>> {name} answered: {answer}"));
                answered.push((name.clone(), answer));
            }
            describe::apply_tool(&mut held, name, args);
        }
        let missing = held.finish().len();
        if missing > 0 {
            refusal = Some(format!(
                "{missing} frame(s) of this batch have no EVENT yet -- record_event each one by its FRAME number"
            ));
            continue;
        }
        let text = serde_json::to_string(&with_results(&calls, &answered))
            .map_err(|why| format!("tool answer: {why}"))?;
        let answered = write_batch(tree, lane, batch, interval, &history, &held);
        describe::store_reply(tree, &key, &text)?;
        return Ok(ChunkAnswer { cached: false, missing: answered });
    }
    Err(format!(
        "{lane}: describe: the model answered {} attempts without an EVENT for every frame of the chunk starting {start:.1}s",
        attempts()
    ))
}

/// The body and the dial, as one call so the retry loop above only changes the message.
fn ask(
    tree: &Tree,
    paths: &Paths,
    model: &str,
    message: &str,
    images: &[Vec<u8>],
    refusal: Option<String>,
) -> Result<(String, String), String> {
    let user = match refusal {
        Some(note) => format!("{message}\nThe previous answer was refused: {note}"),
        None => message.to_string(),
    };
    let mut parts = vec![Part::Text(user)];
    for image in images {
        parts.push(Part::Image(llm_request::data_url(image, "image/jpeg")));
    }
    let messages = vec![Message { role: "user".to_string(), parts }];
    let offered: Vec<Tool> = crate::tools::offered(Job::Describe);
    // §2's row says thinking OFF, which is `Mode::Execute` here: the two modes are the whole switch
    // and every non-thinking job in the app takes this one.
    let body = llm_request::body(STEP, model, &messages, Mode::Execute, Some(&offered), true)?.to_string();
    let endpoint = services::llm_endpoint(
        &settings::read(paths).unwrap_or_default(),
    );
    let base = crate::server_leg::base_row(Server::Llm, model, "POST", "", STEP, "describe");
    // §03-shell §7 wants the exchange page even when §F1.7 S4 later finds the reply in its own cache.
    // `chat` takes the tree by value, so the stage hands its own clone to the leg; the page for this
    // chunk is filed under the run's stamp, which the leg owns.
    let reply = crate::llm_retry_leg::chat(
        STEP,
        tree,
        &base,
        &endpoint,
        &body,
        &crate::cancel_leg::cancel_check_now(),
        &crate::ui::window::log_line,
    )?;
    Ok((reply.content_type, reply.body))
}

/// Read the scaled copies to send. A missing copy is a hard error naming the frame: sending three of
/// four pictures and stamping four would misdate every event after the gap.
fn read_images(tree: &Tree, lane: &str, batch: &[f64]) -> Result<Vec<Vec<u8>>, String> {
    let dir = tree.llm_frames_dir(lane);
    batch
        .iter()
        .map(|at| {
            let name = describe::scaled_name(*at);
            let path = dir.join(&name);
            std::fs::read(&path).map_err(|_| {
                format!("{}: no scaled frame {name} to describe -- the scaling step did not write it", dir.display())
            })
        })
        .collect()
}

/// The three speech sections for the chunk's span, off `words.aligned.json` when there is one.
///
/// The windows are the §3.2 ones ([`crate::tools::describe::CTX_WINDOW_SECONDS`] either side,
/// [`crate::tools::describe::CTX_SEGS`] per side) and `during` is uncapped: the model is told which
/// words its frames sit on top of and what led up to them. §3.2 says all three sections are always
/// present, with `(none)` where a side is empty, and `describe::message`/`speech_block` do that.
fn speech_in(tree: &Tree, lane: &str, from: f64, to: f64) -> Speech {
    let words = crate::requests::read_words(tree, lane)
        .ok()
        .flatten()
        .unwrap_or_default();
    let rate = crate::transcribe::SAMPLE_RATE as f64;
    let lines: Vec<crate::tools::describe::Spoken> = words
        .words
        .iter()
        .map(|word| crate::tools::describe::Spoken {
            source: lane.to_string(),
            start: word.start_sample as f64 / rate,
            end: word.end_sample as f64 / rate,
            text: word.word.clone(),
        })
        .collect();
    let window = crate::tools::describe::CTX_WINDOW_SECONDS;
    // The batch here is only a window calculator: `speech_around` is §3.2's rule for which words a span
    // hears, and re-deriving that span here would be a second copy of it.
    let batch = Batch::new(from, (to - from).max(0.01), 2);
    let text_of = |line: &crate::tools::describe::Spoken| line.text.clone();
    let during: Vec<String> = batch.speech_around(from, to, &lines).into_iter().map(text_of).collect();
    let before: Vec<String> = batch
        .speech_around(from - window, from, &lines)
        .into_iter()
        .rev()
        .take(crate::tools::describe::CTX_SEGS)
        .rev()
        .map(text_of)
        .collect();
    let after: Vec<String> = batch
        .speech_around(to, to + window, &lines)
        .into_iter()
        .take(crate::tools::describe::CTX_SEGS)
        .map(text_of)
        .collect();
    Speech { before, during, after }
}

/// The batch the answer is recorded against: the frames THIS chunk holds, at their real seconds.
///
/// [`Batch::new`] computes its own evenly-spaced seconds because a caller that only knows “n frames
/// from t, d apart” is what §3.2's tool surface was written against. A real batch is not always that:
/// a chunk may be short at the end of the take, and its frames are the picked grid, whose spacing is
/// uneven wherever a scene change moved a step. So the seconds the stamps were built from are what the
/// batch is asked about, and this rebuilds the interval that makes `start + i * interval` land on each
/// of them — the value `record_event` names in its answer.
fn batch_of(batch: &[f64], interval: f64, history: &[String]) -> Batch {
    let start = batch.first().copied().unwrap_or(0.0);
    let count = batch.len().max(1);
    // Over the whole batch rather than the first step: a short final chunk is one frame, and a
    // one-frame batch's span is what the request said it covered (`to` - `from`), not zero.
    let spread = batch.last().copied().unwrap_or(start) - start;
    let step = if count > 1 { spread / (count - 1) as f64 } else { interval };
    let held = Batch::new(start, step, count);
    if history.is_empty() {
        held
    } else {
        held.with_history()
    }
}

/// The §3.2 answer's tool calls, as the message that carries them and as the pair this module stores
/// in the cache.
///
/// A chat reply already arrives as structured `tool_calls` (no XML, no fence), so no parsing of prose
/// is needed to get them; what is NOT in the chat protocol is anywhere to put per-call tool RESULTS,
/// so a multi-call answer cannot be fed back as a second round the way §3's loop describes one, and
/// the retry instead re-asks the batch with the refusal in the user message — which is what
/// `finish`'s answer ("these frames have no EVENT") is for. Storing the pair rather than the raw
/// reply is what lets a cached answer go through the same `apply_tool` path as a fresh one.
fn tool_calls_of(reply: &llm_request::Reply) -> Vec<(String, serde_json::Value)> {
    reply
        .tool_calls
        .iter()
        .map(|call| {
            (
                call.name.clone(),
                serde_json::from_str(&call.args).unwrap_or_else(|_| serde_json::json!({})),
            )
        })
        .collect()
}

/// Answer one of §3.2's four reads, or `None` when the call belongs to `describe::apply_tool`.
///
/// `get_events`, `get_frames` and `get_context` come from [`tool_reads::dispatch`] with THIS chunk's own
/// event rows and frame seconds — never a fresh copy of the session, because a read must describe the
/// batch the pictures are from. `speech_around` is answered here rather than in the shared layer because
/// every-source speech is this leg's to gather.
fn read_answer(
    tree: &Tree,
    lane: &str,
    user_context: &str,
    rows: &[describe::EventRow],
    batch: &[f64],
    name: &str,
    args: &serde_json::Value,
) -> Option<String> {
    if name == "speech_around" {
        return Some(speech_answer(tree, lane, args));
    }
    // `EventRow` is describe's resume shape (`at` + text, no end: §6's middle field is a read-side fold).
    // A read has to say how long a reading held, which is the next row's start.
    let events: Vec<FrameEvent> = rows
        .iter()
        .enumerate()
        .map(|(index, row)| FrameEvent {
            start: row.at,
            end: rows.get(index + 1).map(|next| next.at).unwrap_or(row.at),
            text: row.text.clone(),
        })
        .collect();
    let asked = crate::exchanges::ToolCall { name: name.to_string(), args: args.to_string(), result: String::new() };
    let ctx = tool_reads::Context {
        user_context,
        job: Job::Describe,
        policy: &[],
        lines: &[],
        events: &events,
        frames: batch,
    };
    tool_reads::dispatch(&ctx, &asked)
}

/// The calls as the chunk's cached reply stores them, each with the answer it was given. `apply_and_write`
/// reads the pair at `0`/`1` and skips anything else, so the third field costs a replay nothing and leaves a
/// readable record of what the model was handed.
fn with_results(
    calls: &[(String, serde_json::Value)],
    answered: &[(String, String)],
) -> Vec<serde_json::Value> {
    calls
        .iter()
        .map(|(name, args)| {
            let answer = answered
                .iter()
                .find(|(asked, _)| asked == name)
                .map(|(_, answer)| answer.clone());
            serde_json::json!({ "0": name, "1": args, "result": answer })
        })
        .collect()
}

/// `speech_around` (§3.2): every line spoken in that stretch, from EVERY recording — not the two-per-side
/// extract the request's own brief carries (capped by `describe::CTX_SEGS`, quoting only what the message
/// builder picked). So this gathers from every source the project lists: a describe runs before the fix
/// stage merges `session.tsv`, so reading that file alone would answer "nothing" during the very walk
/// whose brief is short.
fn speech_answer(tree: &Tree, lane: &str, args: &serde_json::Value) -> String {
    let number = |key: &str| -> Option<f64> {
        match args.get(key)? {
            serde_json::Value::Number(n) => n.as_f64(),
            serde_json::Value::String(text) => text.trim().parse().ok(),
            _ => None,
        }
    };
    let (Some(from), Some(to)) = (number("from"), number("to")) else {
        return crate::tools::error("speech_around needs \"from\" and \"to\" session seconds");
    };
    let (from, to) = if from <= to { (from, to) } else { (to, from) };
    let rate = crate::transcribe::SAMPLE_RATE as f64;
    let project = crate::project::load(tree.dir()).unwrap_or_default();
    let mut hits: Vec<(f64, f64, String, String)> = Vec::new();
    for source in &project.sources {
        let other = crate::audio_task::source_lane(&source.path);
        let Some(words) = crate::requests::read_words(tree, &other).ok().flatten() else {
            continue;
        };
        for word in words.words {
            let (start, end) = (
                word.start_sample as f64 / rate,
                word.end_sample as f64 / rate,
            );
            if start < to && end > from {
                hits.push((start, end, other.clone(), word.word.clone()));
            }
        }
    }
    hits.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.2.cmp(&b.2)));
    let rows: Vec<String> = hits
        .iter()
        .map(|(start, end, source, text)| {
            format!(
                "{}\t{source}\t{text}",
                crate::fix_transcripts::stamp_span(*start, *end)
            )
        })
        .collect();
    if rows.is_empty() {
        // "nobody spoke there" said out loud: an empty list cannot tell that from "no words file yet".
        crate::ui::window::log_line(&format!(
            ">>> {lane}: speech_around found nothing between {from:.1}s and {to:.1}s"
        ));
    }
    crate::tools::ok(&serde_json::json!({"from": from, "to": to, "lines": rows}))
}

/// Feed one reply through the §3.2 tool surface and write what it answered. Returns the frames still
/// missing (0 when the answer was complete).
///
/// A cached reply is stored in the tool-call spelling `run_chunk` writes it in, so a hit and a fresh
/// answer take the one path through `apply_tool` rather than two.
fn apply_and_write(
    tree: &Tree,
    lane: &str,
    batch: &[f64],
    interval: f64,
    history: &[String],
    reply: &str,
) -> Result<usize, String> {
    let calls: Vec<(String, serde_json::Value)> = serde_json::from_str(reply).unwrap_or_default();
    let mut held = batch_of(batch, interval, history);
    for (name, args) in &calls {
        describe::apply_tool(&mut held, name, args);
    }
    Ok(write_batch(tree, lane, batch, interval, history, &held))
}

/// §F1.7 S6: this chunk's rows appended and `state.txt` rewritten, after EVERY chunk.
/// §F1.7 S6: this chunk's rows appended and `state.txt` rewritten, after EVERY chunk.
///
/// Writing happens only once `Batch::finish` has reported nothing missing, so every frame of the chunk
/// has an EVENT of its own: `describe::events_rows` answers an unmentioned frame with the word `same`,
/// which is a true statement only when a row precedes it (its own doc explains the first-frame-with-
/// no-history case). The count below is a sanity bound on that precondition, not a branch of its own.
fn write_batch(
    tree: &Tree,
    lane: &str,
    batch: &[f64],
    interval: f64,
    history: &[String],
    held: &Batch,
) -> usize {
    let rows = describe::events_rows(
        batch,
        held.events(),
        !history.is_empty(),
        batch.last().copied().unwrap_or(0.0) + interval,
    );
    let missing = batch.len().saturating_sub(rows.len());
    if missing > 0 {
        // An incomplete answer leaves both files alone: a half-written `events.tsv` would read back as
        // a chunk that was answered, which is the resume rule failing silently.
        return missing;
    }
    // The STATE goes with the rows: a cleared state is itself an answer (§S6).
    let _ = describe::write_chunk(tree, lane, &rows, held.state());
    0
}
