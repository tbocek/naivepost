//! Fix's request leg: the clean-transcript job that respells what ASR got wrong, over HTTP.
//! §02-services.md §2 names the row — LLM, thinking OFF, "the same lines respelled; times and
//! speakers unchanged, **enforced**".
//!
//! [`crate::fix_transcripts`] holds every rule of F1.8 and makes no call of its own: `fix_block`
//! takes an `ask` closure and nothing in `src` used to supply one, so the respelling job never
//! reached a server. This is that leg. It gathers what those rules read from disk, builds the body
//! through [`crate::llm_request`] so the sampling five and the thinking switch come from the one
//! place every chat request is built, sends it through [`crate::llm_retry_leg`] (which writes the
//! timed `requests.tsv` row and rides §09 §3's patience), and hands the answer back as
//! [`fix_transcripts::Reply`] values so §3.3's `fix_line` tool — not a copy of it — decides what a
//! fix is.
//!
//! The enforcement §2 asks for is not a comparison after the reply: `tools::fix::Block::fix_line`
//! takes `(n, text)` and NOTHING else, so a respelling has nowhere to put a moved time or a changed
//! speaker. That is why this module converts a reply into calls rather than into lines.
//!
//! The seam a test drives this through is the Settings address, as every other live job uses: point
//! `LLM_SERVER` at a local fake and the same endpoint resolution a typed address goes through reaches
//! it. Nothing here takes a canned reply.

use crate::exchanges::{Message, Mode, Part};
use crate::fix_transcripts::{self, Reply};
use crate::layout::Tree;
use crate::llm_request;
use crate::roles::Job;
use crate::services::{self, Server};
use crate::settings::{self, Paths};
use crate::textfmt::{FrameEvent, Line};
use crate::tool_loop;
use crate::tool_reads;
use crate::tools::{self, Tool};

/// The step name this job's cache entries and timed rows carry — the same word `FIX_STEP` uses, so a
/// `requests.tsv` row and `cache/llm/fix` name the stage in §04 S6's order.
pub const STEP: &str = fix_transcripts::FIX_STEP;

/// The fix stage of the Prepare walk: one row per recording, since every transcript gets respelled
/// whether or not its source had pictures.
///
/// A dial (`program: TEXT_TASK`), not a spawn — the work is a chat request. It comes after describe in
/// [`crate::run_stages::plan`] because §04 S6 puts understanding in that order, and a source with no
/// transcript yet simply answers nothing (its `load_transcript` fails the row naming what is missing,
/// which is the honest report: ingest owes that file).
pub fn stage(project: &crate::project::Project) -> crate::run_stages::Stage {
    let tasks: Vec<crate::run_stages::Task> = project
        .sources
        .iter()
        .map(|source| crate::run_stages::Task {
            label: format!("fixing {}", crate::audio_task::source_lane(&source.path)),
            program: crate::run_stages::TEXT_TASK.to_string(),
            // What the door cannot read off the session: which kind of recording this is (a video also
            // owes `subtitles.srt`) and whether its words are the narration rather than content.
            args: vec![
                source.path.clone(),
                if source.footage { "video".to_string() } else { "audio".to_string() },
                source.narrator.to_string(),
            ],
            already_done: false,
            session: None,
        })
        .collect();
    crate::run_stages::Stage {
        track: crate::runqueue::TRACK_FIX,
        name: "fix",
        phase: 2,
        of: 2,
        kind: "chunk",
        // §F1.8's session halves bracket the per-source rows: S1's `offsets.tsv` before any block goes
        // out, S5-S7's merged timeline after the last one.
        //
        // §F1.13's word list sits between the fix rows and the merge row: it re-dresses its words from the
        // `transcript.fixed.tsv` the fix rows just wrote, and the merge row is what runs the marking pass,
        // which shows the model THIS spelling (`joins::session_words_with_list` loads the saved list
        // first), so building it after that row would be too late for the pass it belongs to.
        tasks: crate::fix_session::placement_rows(project)
            .into_iter()
            .chain(tasks)
            .chain(crate::word_list::row(project))
            .chain(crate::fix_session::merge_rows(project))
            .collect(),
    }
}

/// The row [`crate::run_stages::drive_walk`] hands here: fix one source's transcript end to end.
///
/// Everything the walk's rows do not carry is resolved here from the environment; [`stage_chunk`] is
/// the same door with those two handed in, which is how a test drives the real path without owning the
/// process's working folder.
pub fn run_task(
    task: &crate::run_stages::Task,
    log: &mut impl FnMut(&str),
) -> Result<(), String> {
    let paths = settings::from_environment()
        .ok_or("nowhere to keep the settings -- neither XDG_CONFIG_HOME nor HOME is set")?;
    let session = session_dir();
    let tree = Tree::new(&session)
        .map_err(|_| format!("{}: not a project folder to fix into", session.display()))?;
    let source = task.args.first().cloned().ok_or("the fix row names no source")?;
    let lane = crate::audio_task::source_lane(&source);
    let is_video = task.args.get(1).map(String::as_str) == Some("video");
    // A narrator's words are the narration, so they go to `commentary.fixed.tsv` (§F1.8 S4).
    let is_commentary = task.args.get(2).map(String::as_str) == Some("1");
    let conf = settings::read(&paths).unwrap_or_default();
    stage_chunk(&tree, &paths, &conf, &lane, is_video, is_commentary, log)?;
    Ok(())
}

/// The session folder the walk works in — the working folder's own, resolved the way every other dial
/// resolves it (§1's re-read-per-request seams all start here).
fn session_dir() -> std::path::PathBuf {
    crate::startup::session_dir(&std::env::current_dir().unwrap_or_default())
}

/// One source's transcript through the fixer: block by block, ask once per block, write the fixed
/// files. Returns how many blocks were walked.
#[allow(clippy::too_many_arguments)]
pub fn stage_chunk(
    tree: &Tree,
    paths: &Paths,
    conf: &settings::Conf,
    lane: &str,
    is_video: bool,
    is_commentary: bool,
    log: &mut impl FnMut(&str),
) -> Result<usize, String> {
    let lines = fix_transcripts::load_transcript(tree, lane)?;
    if lines.is_empty() {
        log(&format!(">>> [{lane}] nothing to fix -- the transcript is empty"));
        return Ok(0);
    }
    // §F1.8 S2's grounding is ±5 s from EVERY other recording plus this source's own events. This door
    // fixes ONE source per row, so the list carries this lane's own rows tagged with its name; the
    // cross-source half of the window is `crate::fix_session::session_fix`, which runs
    // `fix_transcripts::run` over a whole session and sees every lane at once.
    let every_line: Vec<(String, Line)> =
        lines.iter().cloned().map(|line| (lane.to_string(), line)).collect();
    let every_event: Vec<(String, FrameEvent)> = fix_transcripts::load_events(tree, lane)
        .into_iter()
        .map(|event| (lane.to_string(), event))
        .collect();

    let blocks = fix_transcripts::blocks(&lines);
    let total = blocks.len();
    let mut fixed: Vec<Line> = Vec::with_capacity(lines.len());
    let mut asked = 0usize;
    let mut from_cache = 0usize;
    for (index, block) in blocks.into_iter().enumerate() {
        log(&fix_transcripts::checkpoint(index + 1, total, lane));
        let (from, to) = fix_transcripts::context_window(
            block.first().map(|line| line.start).unwrap_or(0.0),
            block.last().map(|line| line.end).unwrap_or(0.0),
        );
        let context =
            fix_transcripts::context_text(from, to, lane, &[], &every_line, &every_event);
        // The message the model is shown is `ask_text`'s, built here so the grounding is part of it;
        // `fix_block` rebuilds the same text from its own rows when it appends a refusal.
        let _message = fix_transcripts::ask_text(&context, &block);
        // §F1.8 S3: only a first-try answer is cached, and only a VALID block is stored, so a block
        // that failed twice is re-asked on the next run rather than replaying its failure.
        let outcome = match fix_transcripts::cached_block(tree, &block) {
            Some(reply) => {
                from_cache += 1;
                fix_transcripts::fix_block(&block, |_| Ok(reply.clone()))?
            }
            None => {
                asked += 1;
                fix_transcripts::fix_block(&block, ask_for(tree, paths, conf, &every_event, &block))?
            }
        };
        if !outcome.valid {
            log(&fix_transcripts::give_up_log(lane, index, total));
        } else if fix_transcripts::caches_attempt(outcome.attempt) {
            // §6's second irregularity: only the FIRST attempt's answer is filed. A repaired one is
            // only right given the refusal it was shown, so replaying it from disk would skip the
            // validation that made it right — the fix still lands in `transcript.fixed.tsv` below,
            // it simply costs the same ask again on the next run.
            if let Err(err) = fix_transcripts::store_block(tree, &block, &outcome.lines) {
                // A cache that will not write is not a reason to lose the fix.
                log(&format!("!!! could not keep the fix: {err}"));
            }
        }
        for (n, why) in &outcome.flags {
            log(&format!(">>> {lane} line {n}: {why}"));
        }
        fixed.extend(outcome.lines);
    }
    fix_transcripts::write_fixed(tree, lane, is_video, is_commentary, &fixed)?;
    log(&fix_transcripts::cached_blocks_log(lane, from_cache));
    let _ = asked;
    Ok(total)
}

/// The `ask` closure the Prepare walk hands `fix_transcripts::fix_block`, with §3.1's shared reads wired.
///
/// This is the ONLY place the clean-transcript role reaches the LLM server, so it is also the only place
/// those reads can be answered for that job: the rows `fix_block` is walking and this source's event log
/// are in scope here and nowhere else, and a read must describe THAT block rather than a fresh copy of
/// the session.
pub fn ask_for<'a>(
    tree: &'a Tree,
    paths: &'a Paths,
    conf: &'a settings::Conf,
    events: &'a [(String, FrameEvent)],
    lines: &'a [Line],
) -> impl FnMut(&str) -> Result<Vec<Reply>, String> + 'a {
    let owned: Vec<FrameEvent> = events.iter().map(|(_, event)| event.clone()).collect();
    move |message: &str| {
        let ctx = tool_reads::Context {
            // The User Context belongs to the window's session (§10 §2), which this headless door does
            // not carry; an empty one is what every other headless caller sends, and `get_context`
            // answers it as an empty field rather than an error.
            user_context: "",
            job: Job::CleanTranscript,
            policy: &[],
            lines,
            events: &owned,
            frames: &[],
        };
        ask_once(tree, paths, conf, message, Some(&ctx))
    }
}

/// One block's ask for the SESSION walk: `ask_once` with no per-source read context in hand.
///
/// `run` walks every recording, so no single lane's rows and events can be handed to §3.1's shared reads
/// here; the tool rounds still run (a read with nothing behind it comes back as §3's error sentence, not
/// silence) and the cache, the timed row and §09 §5's bound all still apply.
pub fn ask_block(
    tree: &Tree,
    paths: &Paths,
    conf: &settings::Conf,
    message: &str,
) -> Result<Vec<Reply>, String> {
    ask_once(tree, paths, conf, message, None)
}

/// One block's ask: the two messages, §3.1's tool rounds, and the answer read back into §3.3's calls.
///
/// `fix_block` supplies the message, which already carries the previous refusal when there was one — so a
/// retry is a different question and keys elsewhere in the cache.
///
/// The loop is [`crate::tool_loop::run`], the same one narrate drives: this job is OFFERED four shared reads
/// (`tools::offered(Job::CleanTranscript)`), and a single chat call could not answer one of them — the model
/// would ask for its lines and get silence. Every round's dial writes its own timed `requests.tsv` row
/// (§09 §10: a line per request).
///
/// `reads` is what makes those reads answerable: `Some` on the program path (`ask_for`), `None` from the
/// older tests that only pin the shape of one call — with it absent a read comes back as an error sentence
/// rather than data, which is §3's rule for a tool nothing can serve.
fn ask_once(
    tree: &Tree,
    paths: &Paths,
    conf: &settings::Conf,
    message: &str,
    reads: Option<&tool_reads::Context>,
) -> Result<Vec<Reply>, String> {
    let system = fix_system(paths);
    let parts = cache_parts(&system, message, &conf.model);
    if let Some(cached) = crate::llm_cache::read(tree, STEP, &parts) {
        // A reply served from the cache costs no wire and writes no row: the request that earned it is
        // already in `requests.tsv` (§09 §10).
        let reply = llm_request::reply_from("application/json", &cached)?;
        return Ok(calls_of(&reply));
    }
    let messages = vec![
        Message { role: "system".to_string(), parts: vec![Part::Text(system.clone())] },
        Message { role: "user".to_string(), parts: vec![Part::Text(message.to_string())] },
    ];
    let offered: Vec<Tool> = crate::tools::offered(Job::CleanTranscript);
    let endpoint = services::llm_endpoint(conf);
    // What the rounds collect: every `fix_line`/`flag_line` the model actually got answered, and the raw
    // body of the FIRST readable reply — the pair this block's cache entry stands for.
    let mut fixes: Vec<Reply> = Vec::new();
    let mut first_body: Option<String> = None;
    // Whether any round of this ask was refused, which is what §6's "only its FIRST attempt is cached"
    // turns on. `fix_transcripts::fix_block` puts the refusal in front of the next question, so a second
    // round exists exactly when the first answer was not usable -- and this leg sees every round.
    let mut refused = false;
    let mut ask = |conversation: &[Message], tools: &[Tool]| -> Result<tool_loop::Turn, String> {
        // §2's row says thinking OFF, which is `Mode::Execute` here: the two modes are the whole switch
        // and every non-thinking job in the app takes this one.
        let base = crate::server_leg::base_row(Server::Llm, &conf.model, "POST", "", STEP, "fix");
        let sent = crate::llm_tool_refusal_leg::dial_honouring_refusal(
            STEP,
            &endpoint.url,
            Some(tools),
            &|offer| {
                Ok(llm_request::body(STEP, &conf.model, conversation, Mode::Execute, offer, true)?.to_string())
            },
            &|body| {
                crate::llm_retry_leg::chat(
                    STEP,
                    tree,
                    &base,
                    &endpoint,
                    body,
                    &crate::cancel_leg::cancel_check_now(),
                    &crate::ui::window::log_line,
                )
            },
            &crate::ui::window::log_line,
        )?;
        let reply = llm_request::reply_from(&sent.content_type, &sent.body)
            .map_err(|why| format!("{STEP}: {why}"))?;
        // Only the round that ENDED the flow is worth caching, and only if it could be read at all: an
        // unreadable reply must go out again on the retry rather than being replayed from the slot it
        // failed in, and a mid-conversation round holds only part of the answer.
        if reply.tool_calls.iter().any(|call| call.name == Tool::Finish.name()) {
            first_body = Some(sent.body.clone());
        }
        // A round that asked for a tool result instead of finishing is the fixer's refusal path: the
        // answer came back unusable and another question is about to be dialled with the refusal in it.
        refused |= !reply.tool_calls.iter().any(|call| call.name == Tool::Finish.name());
        // `finish` closes the flow and the loop reads that off the TURN, not off the call list.
        let finishing = reply.tool_calls.iter().any(|call| call.name == Tool::Finish.name());
        Ok(tool_loop::Turn {
            content: reply.content.clone(),
            calls: reply.tool_calls.clone(),
            finish: finishing,
        })
    };
    let mut run_tool = |asked: &crate::exchanges::ToolCall| -> String {
        // §3.1's shared reads first, answered from the block in hand; then this job's own three.
        if let Some(answer) = reads.and_then(|ctx| tool_reads::dispatch(ctx, asked)) {
            return answer;
        }
        let args: serde_json::Value = serde_json::from_str(&asked.args)
            .unwrap_or_else(|_| serde_json::json!({}));
        let number = args.get("n").and_then(|n| n.as_u64()).map(|n| n as u32);
        match asked.name.as_str() {
            // The answer only says the call landed. Whether `n` is IN this block is decided further
            // down the line by `fix_transcripts::fix_block`, which feeds every Reply through
            // `tools::fix::Block` — so an out-of-range number still comes back as the refusal message
            // on the next try, which is where that rule belongs.
            "fix_line" => match (number, args.get("text").and_then(|t| t.as_str())) {
                (Some(n), Some(text)) => {
                    fixes.push(Reply::Fix { n, text: text.to_string() });
                    tools::ok(&serde_json::json!({"n": n, "text": text}))
                }
                // DROPPED rather than answered with an invented empty respelling: an empty line would
                // overwrite a real sentence in `transcript.fixed.tsv`. The model hears why.
                _ => tools::error("fix_line needs \"n\" and the corrected \"text\""),
            },
            "flag_line" => match (number, args.get("why").and_then(|w| w.as_str())) {
                (Some(n), Some(why)) => {
                    fixes.push(Reply::Flag { n, why: why.to_string() });
                    tools::ok(&serde_json::json!({"n": n, "flagged": true}))
                }
                _ => tools::error("flag_line needs \"n\" and the \"why\""),
            },
            "finish" => tools::ok(&serde_json::json!({"done": true})),
            other => tools::error(&format!("{other} is not a tool of this call")),
        }
    };
    let run = tool_loop::run(STEP, messages, &offered, &mut ask, &mut run_tool);
    for line in run.joined().lines() {
        crate::ui::window::log_line(line);
    }
    // Stored under the ORIGINAL two-message key, never the grown conversation: the next run asks the
    // same opening question and must find this answer. The rows kept are the ones whose text differs
    // from what ASR heard, which is exactly what `fix_transcripts::cached_block` replays.
    //
    // §6's second irregularity decides WHICH ask may be filed, and this leg can decide it itself: a round
    // that did not finish means the answer was refused and re-asked, so a repaired block costs the same
    // ask again on the next run rather than replaying a reply that was only right given its refusal.
    if let Some(body) = first_body.filter(|_| !refused) {
        crate::llm_cache::store(tree, STEP, &parts, &body);
    }
    Ok(fixes)
}

/// The chat reply's tool calls as §3.3's `Reply` values — the one place a chat envelope becomes a fix.
///
/// `finish` and any read-back the model asks for (`get_lines`, `get_context`, `get_events`,
/// `get_frames`) become [`Reply::Done`]: `fix_block` feeds every call through the tool surface and an
/// answer is not a refusal, so reporting them costs nothing and keeps the count honest. A `fix_line`
// whose text field is missing is DROPPED rather than answered with an invented empty respelling: an
// empty line would overwrite a real sentence in `transcript.fixed.tsv`.
fn calls_of(reply: &llm_request::Reply) -> Vec<Reply> {
    let mut out = Vec::with_capacity(reply.tool_calls.len());
    for call in &reply.tool_calls {
        let args: serde_json::Value =
            serde_json::from_str(&call.args).unwrap_or_else(|_| serde_json::json!({}));
        let line = args.get("n").and_then(|n| n.as_u64()).map(|n| n as u32);
        match call.name.as_str() {
            "fix_line" => {
                if let (Some(n), Some(text)) = (line, args.get("text").and_then(|t| t.as_str())) {
                    out.push(Reply::Fix { n, text: text.to_string() });
                }
            }
            "flag_line" => {
                if let (Some(n), Some(why)) = (line, args.get("why").and_then(|w| w.as_str())) {
                    out.push(Reply::Flag { n, why: why.to_string() });
                }
            }
            _ => out.push(Reply::Done),
        }
    }
    out
}

/// SYSTEM for a fix call: the house rules cut to this job's section, the shipped fix wording, and no
/// User Context block of its own (§F1.8's grounding is the surrounding speech, not the session note).
/// Same assembly the captions ask uses, read through the settings path so an edit wins (§04#6).
fn fix_system(paths: &Paths) -> String {
    crate::prompt_assembly::system_message(
        &wording(paths, "system"),
        "fix",
        &wording(paths, "fix"),
        "",
        false,
        false,
    )
}

/// The wording to ask with, through the settings read path so an edit here wins (§04-prepare#6). With
/// no edited copy on disk, the shipped wording is what goes out.
fn wording(paths: &Paths, key: &str) -> String {
    let fallback = crate::join_ask::shipped_file(key).unwrap_or_default();
    settings::prompt_text(paths, key, &fallback).unwrap_or(fallback)
}

/// The deciding parts for one block's ask. §6's list, and `thinking: false` because §2's row says
/// thinking OFF — a thinking answer to the same lines is not the same answer, so the flag belongs in
/// the key even though it never changes for this job.
fn cache_parts(system: &str, user: &str, model: &str) -> Vec<serde_json::Value> {
    let request = crate::llm_cache::Request {
        system,
        user,
        state: "",
        speech: "",
        context: "",
        images: Vec::new(),
        run_index: None,
        model,
        thinking: false,
    };
    crate::llm_cache::parts(&request)
}
