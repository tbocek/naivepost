//! Describe's request leg: the vision job turning one chunk of frames into EVENT lines and a running STATE (§02-services.md §2).
//! [`crate::describe`] holds the F1.7 rules and makes no call; this gathers their inputs, builds the body via [`crate::llm_request`],
//! sends it through [`crate::llm_retry_leg`], hands the answer to [`crate::describe::apply_tool`] and writes rows via [`crate::describe::write_chunk`].
//!
//! Tests drive it through the Settings address: point `LLM_SERVER` at a local fake.

use crate::describe::{self, Speech, Stamp};
use crate::describe_scale;
use crate::exchanges::{Message, Part};
use crate::layout::Tree;
use crate::llm_request;
use crate::roles::Job;
use crate::services::{self, Server};
use crate::settings::{self, Paths};
use crate::textfmt::FrameEvent;
use crate::tool_reads;

use crate::tools::describe::{Batch, MissingReason};
use crate::tools::Tool;

/// The step name for cache entries and timed rows, same as `describe::STEP`.
pub const STEP: &str = describe::STEP;

/// Asks allowed per chunk: §3.2 gives describe no budget, so this takes the fixer's (`P.machine.fixTries`).
fn attempts() -> u32 {
    crate::fix_transcripts::attempts()
}

/// The describe stage of the Prepare walk: one row per footage source.
/// A dial (`program: VISION_TASK`), not a spawn. Comes after the frames stage in [`crate::run_stages::plan`]; a source with nothing extracted is left out (§F1.7).
pub fn stage(project: &crate::project::Project) -> crate::run_stages::Stage {
    let tasks = project
        .sources
        .iter()
        .filter(|source| source.footage)
        .enumerate()
        .map(|(index, source)| crate::run_stages::Task {
            label: format!(
                "describing {}",
                crate::audio_task::source_lane(&source.path)
            ),
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

/// The stage's work for one footage source: choose the frames F1.6 extracted, chunk them at four, ask once per chunk.
/// [`run_task`] resolves the tree and settings folder from the environment and hands them here, so tests avoid the process cwd.
pub fn stage_chunk(
    tree: &Tree,
    paths: &Paths,
    conf: &settings::Conf,
    lane: &str,
    source_index: Option<usize>,
    log: &mut impl FnMut(&str),
) -> Result<usize, String> {
    stage_chunk_reached(tree, paths, conf, lane, source_index, log, &mut |_, _| {})
}

/// How many requests describing the row `task` takes; the walk counts the stage's total from it.
pub fn planned_requests(task: &crate::run_stages::Task) -> usize {
    let Some(source) = task.args.first() else {
        return 0;
    };
    match Tree::new(session_dir()) {
        Ok(tree) => describe::planned_requests(&tree, &crate::audio_task::source_lane(source)),
        Err(_) => 0,
    }
}

/// [`stage_chunk`], reporting progress: `reached` gets chunks answered and chunks in all, at the start and after every answer (§03-shell §2).
pub fn stage_chunk_reached(
    tree: &Tree,
    paths: &Paths,
    conf: &settings::Conf,
    lane: &str,
    source_index: Option<usize>,
    log: &mut impl FnMut(&str),
    reached: &mut dyn FnMut(usize, usize),
) -> Result<usize, String> {
    let frames_on_disk = describe::frames_on_disk(&tree.frames_dir(lane));
    // S0: an every-frame extraction stops this source here, before paying for frames nothing spaced. Local, not fatal: Ok(0) (§Failure-is-specific).
    if describe::is_every_frame(&frames_on_disk, describe::GRID) {
        log(&describe::every_frame_refusal(lane));
        return Ok(0);
    }
    let changes = describe::read_changes(tree, lane);
    let project = crate::project::load(tree.dir()).ok();
    // F1.6 S6: a frame per scene change plus one per heartbeat; the changes are the text gate's when it ran, else the pixel pass's.
    let freq = crate::text_gate::HEARTBEAT_S;
    if crate::text_gate::changes_for_describe(tree, lane).is_some() {
        log(&format!(
            ">>> [{lane}] describing the text gate's {} scene change(s), one frame each and one every {freq} s between",
            changes.len(),
        ));
    }
    // The offset comes from the placement `stage` put on the row.
    let index = source_index.unwrap_or(0);
    // S1: recordings heard alongside the video, with offsets, said before any work.
    for (audio, offset) in describe_scale::alongside(project.as_ref(), index) {
        log(&describe::hearing_log(lane, &audio, offset));
    }
    // S1: the sent copies (896 wide) are made once by ffmpeg; frames on disk stay at the video's size.
    let start = describe_scale::clock_start(project.as_ref(), index);
    describe_scale::ensure(tree, conf, lane, start, &frames_on_disk, log)
        .map_err(|why| format!("{lane}: {why}"))?;
    let picked = describe::picked_for(tree, lane, &frames_on_disk);
    if picked.is_empty() {
        log(&format!(
            ">>> [{lane}] no frames to describe -- the frames folder is empty"
        ));
        return Ok(0);
    }
    let mut answered = 0usize;
    let mut from_cache = 0usize;
    // §02 §1: one request at a time per server; the rounds still let the retry loop find a chunk's stored reply and file a hit (§09 §10).
    let at_once = crate::llm_liveness::in_flight(true, 1);
    // `events.tsv` (the resume marker) and `state.txt` must go down in chunk order, so workers only ask and answers are applied here, oldest first.
    let context = user_context(tree);
    let plan = describe::chunk_plan(&picked, describe::FRAMES_PER_REQ);
    reached(0, plan.len());
    for round in plan.chunks(at_once.max(1)) {
        let dialled = std::thread::scope(|scope| {
            let mut handles = Vec::with_capacity(round.len());
            for start in round {
                let batch = &picked[*start..picked.len().min(*start + describe::FRAMES_PER_REQ)];
                // The chunk's span as `describe::message` is told it: last frame plus one grid step.
                let start_second = picked[*start];
                let end = batch.last().copied().unwrap_or(start_second) + freq;
                let batch = batch.to_vec();
                let context = context.clone();
                handles.push(scope.spawn(move || {
                    // Workers buffer their log lines (`log` lives on this thread); the main loop prints them in chunk order.
                    let mut said: Vec<String> = Vec::new();
                    let answer = run_chunk(
                        tree,
                        paths,
                        conf,
                        lane,
                        &batch,
                        end,
                        freq,
                        &context,
                        &mut |line| said.push(line.to_string()),
                    );
                    (said, answer)
                }));
            }
            // A dead worker is one chunk's failure (§Failure-is-specific); `scope` joins all before the files are written.
            handles
                .into_iter()
                .map(|handle| {
                    handle.join().unwrap_or_else(|_| {
                        (
                            Vec::new(),
                            Err(format!("{lane}: describe: a chunk worker died")),
                        )
                    })
                })
                .collect::<Vec<_>>()
        });
        for (said, answer) in dialled {
            for line in said {
                log(&line);
            }
            let outcome = answer?;
            answered += 1;
            if outcome.cached {
                from_cache += 1;
            }
            reached(answered, plan.len());
        }
    }
    let _ = &changes;
    log(&describe::complete_log_cached(lane, answered, from_cache));
    Ok(answered)
}

/// The row [`crate::run_stages::drive_walk`] hands here: describe one footage source end to end.
pub fn run_task(
    task: &crate::run_stages::Task,
    log: &mut impl FnMut(&str),
    reached: &mut dyn FnMut(usize, usize),
) -> Result<(), String> {
    let paths = settings::from_environment()
        .ok_or("nowhere to keep the settings -- neither XDG_CONFIG_HOME nor HOME is set")?;
    let session = session_dir();
    let tree = Tree::new(&session).map_err(|_| {
        format!(
            "{}: not a project folder to describe into",
            session.display()
        )
    })?;
    let source = task
        .args
        .first()
        .cloned()
        .ok_or("the describe row names no source")?;
    let lane = crate::audio_task::source_lane(&source);
    let index: usize = task
        .args
        .get(1)
        .and_then(|text| text.parse().ok())
        .unwrap_or(0);
    let conf = settings::read(&paths).unwrap_or_default();
    stage_chunk_reached(&tree, &paths, &conf, &lane, Some(index), log, reached)?;
    Ok(())
}

/// The session's own notes (§F1.7 S3), from `naivepost.json`; absent means no context, never an invented one.
fn user_context(tree: &Tree) -> String {
    crate::project::load(tree.dir())
        .map(|project| project.context.clone())
        .unwrap_or_default()
}

/// The working folder's session, resolved like every other dial (§1).
fn session_dir() -> std::path::PathBuf {
    crate::startup::open_dir()
}

/// The row a cache-served chunk hands [`crate::request_timing::record_cached`]. `started` is empty on purpose: the door stamps the read time.
/// `kind` and `job` match [`ask`]'s spellings; §6 keys the slot on the model id, so the model is this row's column.
fn cache_row(model: &str) -> crate::requests::Request {
    crate::requests::Request {
        started: String::new(),
        run: String::new(),
        step: STEP.to_string(),
        job: "describe".to_string(),
        service: crate::requests::Service::Llm,
        model: model.to_string(),
        kind: "POST".to_string(),
        ..Default::default()
    }
}

/// Where one chunk's answer sits in `cache/llm/describe/`: request digest plus model (§6 keys the slot on the model id).
struct StoredChunk<'a> {
    key: &'a str,
    model: &'a str,
}

/// A chunk's replayed answer: what was stored under `stored`, if anything, applied to the batch.
///
/// Files §10's `cache` line too, since [`describe::cached_reply`] bypasses `llm_cache::read`; the row goes down before the answer is applied.
fn replayed_chunk(
    tree: &Tree,
    stored: &StoredChunk<'_>,
    lane: &str,
    batch: &[f64],
    interval: f64,
    history: &[String],
) -> Option<Result<ChunkAnswer, String>> {
    let cached = describe::cached_reply(tree, stored.key)?;
    crate::request_timing::record_cached(tree, cache_row(stored.model));
    Some(
        apply_and_write(tree, lane, batch, interval, history, &cached).map(|missing| ChunkAnswer {
            cached: true,
            missing,
        }),
    )
}

/// A chunk already answered by an earlier run costs no request (§F1.7 S2): [`describe::starts_already_logged`] reads the chunk's START row.
/// `None` when it is still to ask.
///
/// Only a stored-text read is §10's cache hit: a chunk whose events survive but whose cache slot is gone is still owed no request,
/// and [`replayed_chunk`] files a line only for text it read back.
fn resume_answer(
    tree: &Tree,
    lane: &str,
    rows: &[describe::EventRow],
    batch: &[f64],
    interval: f64,
    history: &[String],
    stored: &StoredChunk<'_>,
) -> Option<Result<ChunkAnswer, String>> {
    if !describe::starts_already_logged(rows, batch, describe::FRAMES_PER_REQ).contains(&0) {
        return None;
    }
    Some(
        match replayed_chunk(tree, stored, lane, batch, interval, history) {
            Some(answered) => answered,
            // Nothing is re-asked: the work is on disk.
            None => Ok(ChunkAnswer {
                cached: true,
                missing: 0,
            }),
        },
    )
}

/// What one chunk came to.
pub struct ChunkAnswer {
    /// The reply came from `cache/llm/describe/` and cost no wire.
    pub cached: bool,
    /// Frames still without an EVENT after every attempt: empty is the normal answer.
    pub missing: usize,
}

/// One chunk of frames through the vision job: message, cache, dial, tool answers, and the two files written.
///
/// Retried up to [`attempts`] times while `finish` reports unanswered frames; the refusal goes into the next message.
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
        // `previous` walks the batch: with none, `is_new_scene` would mark every frame NEW SCENE.
        stamps.push(Stamp {
            at: *at,
            new_scene: describe::is_new_scene(*at, previous, &changes),
        });
        previous = Some(*at);
    }
    let message = describe::message(user_context, &state, start, end, &history, &speech, &stamps);
    let images = match read_images(tree, lane, batch) {
        Ok(images) => images,
        // An unscaled frame is the ingest step's debt, not a describe failure to retry (§Failure-is-specific).
        Err(reason) => return Err(format!("{lane}: {reason}")),
    };
    let key = describe::cache_key(
        &system_message(paths, user_context),
        &message,
        &images,
        &conf.model,
    );
    // §F1.7 S2's resume rule via [`describe::starts_already_logged`], one rung further: a chunk whose START is in the event log needs no request.
    let stored = StoredChunk {
        key: &key,
        model: &conf.model,
    };
    if let Some(answer) = resume_answer(tree, lane, &rows, batch, interval, &history, &stored) {
        // Said for both no-wire shapes (see [`resume_answer`]); §10's `cache` row is filed only where stored text was read back.
        log(&describe::cached_note(1));
        return answer;
    }
    let mut refusal: Option<String> = None;
    for attempt in 0..attempts() {
        if let Some(cached) = describe::cached_reply(tree, &key) {
            crate::request_timing::record_cached(tree, cache_row(&conf.model));
            let answered = apply_and_write(tree, lane, batch, interval, &history, &cached)?;
            log(&describe::cached_note(1));
            return Ok(ChunkAnswer {
                cached: true,
                missing: answered,
            });
        }
        let text = match ask(
            tree,
            paths,
            &conf.model.clone(),
            &message,
            &images,
            refusal.take(),
            user_context,
        ) {
            Ok(body) => body,
            Err(reason) => {
                // A wire failure is not a refusal: asking a missing server again spends an attempt on nothing.
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
        let calls = tool_calls_of(&reply);
        // §3.2's shape is tool calls; prose is read by the floor ([`describe::prose_calls`]). A reply whose calls record nothing (a bare
        // `finish`) is prose too: read for its calls alone it has no event for any frame (08-dex, 3 of 3 replays).
        let records = calls
            .iter()
            .any(|(name, _)| name == "record_event" || name == "set_state");
        if reply.tool_calls.is_empty() || (!records && !reply.content.trim().is_empty()) {
            let answered = apply_and_write(tree, lane, batch, interval, &history, &reply.content)?;
            describe::store_reply(tree, &key, &reply.content)?;
            return Ok(ChunkAnswer {
                cached: false,
                missing: answered,
            });
        }
        let calls: Vec<(String, serde_json::Value)> = describe::prose_calls(&reply.content, batch)
            .into_iter()
            .chain(calls)
            .collect();
        let mut held = batch_of(batch, interval, &history);
        // The leg's answer to a read this round asked for. It is written into the CALL list before storing, so a cached chunk replays the same reads.
        let mut answered: Vec<(String, String)> = Vec::new();
        for (name, args) in &calls {
            // §3.2's Reads row: four tools the job's own surface does not answer; without these the model gets silence and the retry blames it.
            // Answers are recorded beside the call and pushed to this stage's log.
            if let Some(answer) = read_answer(tree, lane, user_context, &rows, batch, name, args) {
                log(&format!(">>> {name} answered: {answer}"));
                answered.push((name.clone(), answer));
            }
            describe::apply_tool(&mut held, name, args);
        }
        // `finish` names the cold batch's first frame, but it is not a reason to fail: §04 §6 force-writes `Calm; same view.` there.
        let missing = held
            .finish()
            .iter()
            .filter(|gap| gap.reason != MissingReason::FirstFrameWithNoHistory)
            .count();
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
        return Ok(ChunkAnswer {
            cached: false,
            missing: answered,
        });
    }
    Err(format!(
        "{lane}: describe: the model answered {} attempts without an EVENT for every frame of the chunk starting {start:.1}s",
        attempts()
    ))
}

/// The system message §8 asks for: the house rules cut to this job's sections, the `describe` wording,
/// and the precedence rule when there is a User Context to outrank anything.
///
/// This does NOT change describe's §6 cache slot: [`describe::cache_key`] keys on the user message plus the frame bytes.
pub fn system_message(paths: &Paths, context: &str) -> String {
    crate::prompt_assembly::system_message(
        &wording(paths, "system"),
        "describe",
        &wording(paths, "describe"),
        context,
        false,
        false,
    )
}

/// The wording reader every ask uses: the shipped file, or the settings copy of it (§04-prepare#6).
fn wording(paths: &Paths, key: &str) -> String {
    let fallback = crate::join_ask::shipped_file(key).unwrap_or_default();
    settings::prompt_text(paths, key, &fallback).unwrap_or(fallback)
}

/// The body and the dial, as one call so the retry loop above only changes the message.
fn ask(
    tree: &Tree,
    paths: &Paths,
    model: &str,
    message: &str,
    images: &[Vec<u8>],
    refusal: Option<String>,
    context: &str,
) -> Result<(String, String), String> {
    let user = match refusal {
        Some(note) => format!("{message}\nThe previous answer was refused: {note}"),
        None => message.to_string(),
    };
    let mut parts = vec![Part::Text(user)];
    for image in images {
        parts.push(Part::Image(llm_request::data_url(image, "image/jpeg")));
    }
    let messages = vec![
        Message {
            role: "system".to_string(),
            parts: vec![Part::Text(system_message(paths, context))],
        },
        Message {
            role: "user".to_string(),
            parts,
        },
    ];
    let offered: Vec<Tool> = crate::tools::offered(Job::Describe);
    // §4: thinking OFF, read from [`crate::prompts::mode_of`] so it cannot drift from the catalogue.
    let body = llm_request::body(
        STEP,
        model,
        &messages,
        crate::prompts::mode_of(STEP),
        Some(&offered),
        true,
    )?
    .to_string();
    let endpoint = services::llm_endpoint(&settings::read(paths).unwrap_or_default());
    let base = crate::server_leg::base_row(Server::Llm, model, "POST", "", STEP, "describe");
    // §03-shell §7 wants the exchange page even when §F1.7 S4 later finds the reply in its cache; `chat` takes the tree by value.
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

/// Read the scaled copies to send. A missing copy is a hard error: sending three of four pictures would misdate every later event.
fn read_images(tree: &Tree, lane: &str, batch: &[f64]) -> Result<Vec<Vec<u8>>, String> {
    let dir = tree.llm_frames_dir(lane);
    let base = crate::describe_scale::name_base(tree, lane);
    batch
        .iter()
        .map(|at| {
            let name = describe::scaled_name(base, *at);
            let path = dir.join(&name);
            std::fs::read(&path).map_err(|_| {
                format!(
                    "{}: no scaled frame {name} to describe -- the scaling step did not write it",
                    dir.display()
                )
            })
        })
        .collect()
}

/// The three speech sections for the chunk's span, off `words.aligned.json` when there is one.
///
/// §3.2 windows ([`crate::tools::describe::CTX_WINDOW_SECONDS`] either side, [`crate::tools::describe::CTX_SEGS`] per side); `during` is uncapped.
/// All three sections are always present, `(none)` where empty; `describe::message`/`speech_block` do that.
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
    // Only a window calculator: `speech_around` is §3.2's rule for which words a span hears.
    let batch = Batch::new(from, (to - from).max(0.01), 2);
    let text_of = |line: &crate::tools::describe::Spoken| line.text.clone();
    let during: Vec<String> = batch
        .speech_around(from, to, &lines)
        .into_iter()
        .map(text_of)
        .collect();
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
    Speech {
        before,
        during,
        after,
    }
}

/// The batch the answer is recorded against: the frames THIS chunk holds, at their real seconds.
/// [`Batch::new`] spaces frames evenly, but a chunk may be short at the end of a take and the picked grid is uneven after a scene change;
/// this rebuilds the interval that makes `start + i * interval` land on each (the value `record_event` names).
fn batch_of(batch: &[f64], interval: f64, history: &[String]) -> Batch {
    let start = batch.first().copied().unwrap_or(0.0);
    let count = batch.len().max(1);
    // Over the whole batch: a one-frame final chunk spans `to` - `from`, not zero.
    let spread = batch.last().copied().unwrap_or(start) - start;
    let step = if count > 1 {
        spread / (count - 1) as f64
    } else {
        interval
    };
    let held = Batch::new(start, step, count);
    if history.is_empty() {
        held
    } else {
        held.with_history()
    }
}

/// The §3.2 answer's tool calls, as the message that carries them and as the pair this module stores in the cache.
///
/// The chat protocol has nowhere for per-call tool RESULTS, so a multi-call answer is re-asked with the refusal rather than fed back as a round (§3).
/// Storing the pair lets a cached answer take the same `apply_tool` path as a fresh one.
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
/// `get_events`, `get_frames` and `get_context` come from [`tool_reads::dispatch`] with THIS chunk's rows and frame seconds, never a fresh session copy.
/// `speech_around` is answered here: every-source speech is this leg's to gather.
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
    // `EventRow` has no end (§6's middle field is a read-side fold); a read needs the next row's start as the end.
    let events: Vec<FrameEvent> = rows
        .iter()
        .enumerate()
        .map(|(index, row)| FrameEvent {
            start: row.at,
            end: rows.get(index + 1).map(|next| next.at).unwrap_or(row.at),
            text: row.text.clone(),
        })
        .collect();
    let asked = crate::exchanges::ToolCall {
        name: name.to_string(),
        args: args.to_string(),
        result: String::new(),
    };
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

/// The live leg's `read_answer`, reachable from a test without a session on disk.
pub fn read_answer_for_test(
    tree: &Tree,
    lane: &str,
    user_context: &str,
    rows: &[describe::EventRow],
    batch: &[f64],
    name: &str,
    args: &serde_json::Value,
) -> Option<String> {
    read_answer(tree, lane, user_context, rows, batch, name, args)
}

/// The calls as the cached reply stores them, each with its answer; `apply_and_write` reads only `0`/`1`, so the extra field costs a replay nothing.
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

/// `speech_around` (§3.2): every line spoken in that stretch from EVERY recording, not the request brief's capped extract. A describe runs before
/// the fix stage merges `session.tsv`, so reading that file alone would answer "nothing".
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
fn apply_and_write(
    tree: &Tree,
    lane: &str,
    batch: &[f64],
    interval: f64,
    history: &[String],
    reply: &str,
) -> Result<usize, String> {
    // A stored reply is a call list or prose; prose is read by §3.2's floor against the frames as stamped.
    let calls: Vec<(String, serde_json::Value)> =
        match serde_json::from_str::<Vec<serde_json::Value>>(reply) {
            Ok(stored) => stored.iter().filter_map(stored_call).collect(),
            Err(_) => describe::prose_calls(reply, batch),
        };
    let mut held = batch_of(batch, interval, history);
    for (name, args) in &calls {
        describe::apply_tool(&mut held, name, args);
    }
    Ok(write_batch(tree, lane, batch, interval, history, &held))
}

/// One call of a stored reply: `{"0": name, "1": args, "result": …}` as [`with_results`] files it, or a bare `[name, args]` pair.
fn stored_call(entry: &serde_json::Value) -> Option<(String, serde_json::Value)> {
    let (name, args) = match entry {
        serde_json::Value::Array(pair) => (pair.first()?, pair.get(1)?),
        serde_json::Value::Object(_) => (entry.get("0")?, entry.get("1")?),
        _ => return None,
    };
    Some((name.as_str()?.to_string(), args.clone()))
}

/// §F1.7 S6: this chunk's rows appended and `state.txt` rewritten, after EVERY chunk.
///
/// An unmentioned frame reaches the file as `same`, true only after a row; a cold batch's first frame gets §6's `Calm; same view.` from
/// `describe::events_rows`, so the count below is a sanity bound on one row per frame.
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
        // An incomplete answer leaves both files alone: a half-written `events.tsv` would read as a resumed chunk.
        return missing;
    }
    // The STATE goes with the rows: a cleared state is itself an answer (§S6).
    if let Err(why) = describe::write_chunk(tree, lane, &rows, held.state()) {
        crate::ui::window::log_line(&format!(
            "!!! [{lane}] describe: could not write this chunk's rows -- {why}"
        ));
    }
    0
}
