//! Fix's request leg: the clean-transcript job that respells what ASR got wrong, over HTTP.
//! §02-services.md §2 names the row: LLM, thinking OFF, "the same lines respelled; times and
//! speakers unchanged, **enforced**".
//!
//! [`crate::fix_transcripts`] holds every rule of F1.8 and makes no call of its own: `fix_block` takes an
//! `ask` closure, and this is the leg that answers it. One block is ONE chat call, as the prototype asked it
//! (`fixRows`): the system wording, then the User Context (the session note, a script included: the
//! spellings of names it holds are the point) over the grounding and the block's rows, and no tools. The
//! answer is the same rows with only the text changed, handed back as [`Reply::Row`]s for `fix_block` to hold
//! against its block: a count, a time or a speaker that moved refuses it, which is the enforcement §2 asks
//! for. The first rewrite offered tools instead (`fix_line`, `flag_line`, reads with nothing behind them) and
//! sent no User Context: the model read a context that was empty and answered nothing, and the blocks kept
//! the ASR's lower case ("my name is thomas bocek") in the subtitles.
//!
//! The seam a test drives this through is the Settings address, as every other live job uses: point
//! `LLM_SERVER` at a local fake and the same endpoint resolution a typed address goes through reaches
//! it. Nothing here takes a canned reply.

use crate::exchanges::{Message, Part};
use crate::fix_transcripts::{self, Reply};
use crate::layout::Tree;
use crate::llm_request;
use crate::services::{self, Server};
use crate::settings::{self, Paths};
use crate::textfmt::{FrameEvent, Line};

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
                if source.footage {
                    "video".to_string()
                } else {
                    "audio".to_string()
                },
                // "1" only for the narrator MICROPHONE of voice 1: a video with a voice number is film, its
                // words are content (`sources::is_narrator_mic`).
                if crate::sources::is_narrator_mic(source) && source.narrator == 1 {
                    "1".to_string()
                } else {
                    "0".to_string()
                },
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
pub fn run_task(task: &crate::run_stages::Task, log: &mut impl FnMut(&str)) -> Result<(), String> {
    let paths = settings::from_environment()
        .ok_or("nowhere to keep the settings -- neither XDG_CONFIG_HOME nor HOME is set")?;
    let session = session_dir();
    let tree = Tree::new(&session)
        .map_err(|_| format!("{}: not a project folder to fix into", session.display()))?;
    let source = task
        .args
        .first()
        .cloned()
        .ok_or("the fix row names no source")?;
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
    crate::startup::open_dir()
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
        log(&format!(
            ">>> [{lane}] nothing to fix -- the transcript is empty"
        ));
        return Ok(0);
    }
    // §F1.8 S2's grounding is ±5 s from EVERY other recording plus this source's own events. This door
    // fixes ONE source per row, so the list carries this lane's own rows tagged with its name; the
    // cross-source half of the window is `crate::fix_session::session_fix`, which runs
    // `fix_transcripts::run` over a whole session and sees every lane at once.
    let every_line: Vec<(String, Line)> = lines
        .iter()
        .cloned()
        .map(|line| (lane.to_string(), line))
        .collect();
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
        let context = fix_transcripts::context_text(from, to, lane, &[], &every_line, &every_event);
        // §F1.8 S3: only a first-try answer is cached, and only a VALID block is stored, so a block
        // that failed twice is re-asked on the next run rather than replaying its failure.
        let outcome = match fix_transcripts::cached_block(tree, &block) {
            Some(reply) => {
                from_cache += 1;
                fix_transcripts::fix_block(&block, &context, |_| Ok(reply.clone()))?
            }
            None => {
                asked += 1;
                fix_transcripts::fix_block(&block, &context, ask_for(tree, paths, conf))?
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

/// The `ask` closure the Prepare walk hands `fix_transcripts::fix_block`: one block, one chat call.
pub fn ask_for<'a>(
    tree: &'a Tree,
    paths: &'a Paths,
    conf: &'a settings::Conf,
) -> impl FnMut(&str) -> Result<Vec<Reply>, String> + 'a {
    move |message: &str| ask_once(tree, paths, conf, message)
}

/// One block's ask for the SESSION walk: the same single call.
pub fn ask_block(
    tree: &Tree,
    paths: &Paths,
    conf: &settings::Conf,
    message: &str,
) -> Result<Vec<Reply>, String> {
    ask_once(tree, paths, conf, message)
}

/// One block's ask: the system wording, then the User Context over the block's message, in ONE chat call with
/// no tools (the prototype's `fixRows`). The answer comes back as the rows the model wrote ([`Reply::Row`]), or
/// as [`Reply::Unreadable`] when it holds none, for `fix_block` to judge against the block. Every dial writes
/// its own timed `requests.tsv` row (§09 §10); the block's VALID answer is filed by `fix_transcripts::store_block`,
/// which is the cache a re-run reads.
fn ask_once(
    tree: &Tree,
    paths: &Paths,
    conf: &settings::Conf,
    message: &str,
) -> Result<Vec<Reply>, String> {
    let system = fix_system(paths);
    // The session note goes first, as the prototype's `ctxBlockFor("fix")`: a script in it is where the
    // spellings of names come from ("Use the script's spelling for names"), and the job says so.
    let context = crate::project::load(tree.dir())
        .map(|project| project.context)
        .unwrap_or_default();
    let user = crate::prompt_assembly::user_message(&context, STEP, message);
    let messages = vec![
        Message {
            role: "system".to_string(),
            parts: vec![Part::Text(system)],
        },
        Message {
            role: "user".to_string(),
            parts: vec![Part::Text(user)],
        },
    ];
    let endpoint = services::llm_endpoint(conf);
    let base = crate::server_leg::base_row(Server::Llm, &conf.model, "POST", "", STEP, "fix");
    // §2's row says thinking OFF: `mode_of` reads it from the prompt table so this cannot drift (§10 §4).
    let body = llm_request::body(
        STEP,
        &conf.model,
        &messages,
        crate::prompts::mode_of(STEP),
        None,
        true,
    )?
    .to_string();
    // A server that does not answer costs this block its fix, not the run: the block keeps the ASR's lines
    // after its one retry and is not filed, so the next run asks it again.
    let sent = match crate::llm_retry_leg::chat(
        STEP,
        tree,
        &base,
        &endpoint,
        &body,
        &crate::cancel_leg::cancel_check_now(),
        &crate::ui::window::log_line,
    ) {
        Ok(sent) => sent,
        Err(why) => {
            return Ok(vec![Reply::Unreadable(format!(
                "the fixer could not be asked: {why}"
            ))])
        }
    };
    let reply = llm_request::reply_from(&sent.content_type, &sent.body)
        .map_err(|why| format!("{STEP}: {why}"))?;
    let rows = fix_transcripts::answered_rows(reply.answer());
    if rows.is_empty() {
        return Ok(vec![Reply::Unreadable(
            "the answer held no lines -- answer with the lines asked, start, end, speaker and text \
             separated by tabs"
                .to_string(),
        )]);
    }
    Ok(rows.into_iter().map(Reply::Row).collect())
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
