//! Fix's request leg: the clean-transcript job that respells what ASR got wrong, over HTTP. §02-services.md §2: LLM, thinking OFF, "the same lines respelled; times and speakers unchanged, **enforced**".
//! [`crate::fix_transcripts`] holds the F1.8 rules; this answers its `ask` closure. One block is ONE chat call: system wording, then the User Context (script included, for name spellings) over the grounding and the block's rows, no tools. The same rows come back as [`Reply::Row`]s and `fix_block` refuses a moved count, time or speaker, which is the enforcement.
//! Test seam: point `LLM_SERVER` at a local fake.

use crate::exchanges::{Message, Part};
use crate::fix_transcripts::{self, Reply};
use crate::layout::Tree;
use crate::llm_request;
use crate::services::{self, Server};
use crate::settings::{self, Paths};
use crate::textfmt::{FrameEvent, Line};

/// The step name for cache entries and timed rows, same as `FIX_STEP`.
pub const STEP: &str = fix_transcripts::FIX_STEP;

/// The fix stage of the Prepare walk: one row per recording. A dial (`program: TEXT_TASK`), after describe in [`crate::run_stages::plan`] (§04 S6); a source with no transcript fails its row naming what is missing.
pub fn stage(project: &crate::project::Project) -> crate::run_stages::Stage {
    let tasks: Vec<crate::run_stages::Task> = project
        .sources
        .iter()
        .map(|source| crate::run_stages::Task {
            label: format!("fixing {}", crate::audio_task::source_lane(&source.path)),
            program: crate::run_stages::TEXT_TASK.to_string(),
            // What the door cannot read off the session: the recording kind (a video also owes `subtitles.srt`) and whether its words are narration.
            args: vec![
                source.path.clone(),
                if source.footage {
                    "video".to_string()
                } else {
                    "audio".to_string()
                },
                // "1" only for the narrator MICROPHONE of voice 1; a video with a voice number is film (`sources::is_narrator_mic`).
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
        // §F1.8's session halves bracket the per-source rows: S1's `offsets.tsv` first, S5-S7's merged timeline last.
        // §F1.13's word list goes between the fix rows and the merge row: the marking pass in the merge row must see this spelling.
        tasks: crate::fix_session::placement_rows(project)
            .into_iter()
            .chain(tasks)
            .chain(crate::word_list::row(project))
            .chain(crate::fix_session::merge_rows(project))
            .collect(),
    }
}

/// The row [`crate::run_stages::drive_walk`] hands here: fix one source's transcript end to end. [`stage_chunk`] is the same door with the environment-resolved parts handed in, for tests.
pub fn run_task(task: &crate::run_stages::Task, log: &mut impl FnMut(&str)) -> Result<(), String> {
    let paths = settings::from_environment()
        .ok_or("nowhere to keep the settings -- neither XDG_CONFIG_HOME nor HOME is set")?;
    let session = crate::startup::open_dir();
    let tree = Tree::new(&session)
        .map_err(|_| format!("{}: not a project folder to fix into", session.display()))?;
    let source = task
        .args
        .first()
        .cloned()
        .ok_or("the fix row names no source")?;
    let lane = crate::audio_task::source_lane(&source);
    let is_video = task.args.get(1).map(String::as_str) == Some("video");
    // Narrator words go to `commentary.fixed.tsv` (§F1.8 S4).
    let is_commentary = task.args.get(2).map(String::as_str) == Some("1");
    let conf = settings::read(&paths).unwrap_or_default();
    stage_chunk(&tree, &paths, &conf, &lane, is_video, is_commentary, log)?;
    Ok(())
}

/// One source's transcript through the fixer, one ask per block; returns blocks walked.
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
    // §F1.8 S2's grounding is ±5 s from EVERY other recording plus this source's events. This door fixes ONE source, so the list carries this lane's rows; the cross-source half is `crate::fix_session::session_fix`.
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
    let prompt = cache_prompt(paths, &user_context(tree));
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
        // §F1.8 S3: a VALID block is stored under its own rows so a re-run costs nothing; a block that failed every try is re-asked.
        let cached = fix_transcripts::cached_block(tree, &prompt, &block);
        let hit = cached.is_some();
        WIRE_FAILED.with(|failed| failed.set(false));
        let outcome = match cached {
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
        }
        // Filed under the block's input whatever the outcome (including ASR text kept after all tries). An answer just read back is not rewritten; a block no answer reached (⏹, server gone) is not filed, or its raw text would replay as the answer.
        if !hit && !WIRE_FAILED.with(std::cell::Cell::get) {
            if let Err(err) = fix_transcripts::store_block(tree, &prompt, &block, &outcome.lines) {
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

thread_local! {
    static WIRE_FAILED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

pub fn ask_for<'a>(
    tree: &'a Tree,
    paths: &'a Paths,
    conf: &'a settings::Conf,
) -> impl FnMut(&str) -> Result<Vec<Reply>, String> + 'a {
    move |message: &str| ask_once(tree, paths, conf, message)
}

/// One block's ask: system wording, then the User Context over the block's message, ONE chat call with no tools. Returns the model's rows ([`Reply::Row`]) or [`Reply::Unreadable`]. Every dial writes a timed `requests.tsv` row (§09 §10); a VALID answer is filed by `fix_transcripts::store_block`.
pub fn ask_once(
    tree: &Tree,
    paths: &Paths,
    conf: &settings::Conf,
    message: &str,
) -> Result<Vec<Reply>, String> {
    let system = fix_system(paths);
    let context = crate::project::load(tree.dir())
        .map(|project| project.context)
        .unwrap_or_default();
    let user = crate::prompt_assembly::user_message(
        &crate::prompt_assembly::without_script(&context),
        STEP,
        message,
    );
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
    // §2's row says thinking OFF: `mode_of` reads it from the prompt table so this cannot drift.
    let body = llm_request::body(
        STEP,
        &conf.model,
        &messages,
        crate::prompts::mode_of(STEP),
        None,
        true,
    )?
    .to_string();
    // A server that does not answer costs this block its fix, not the run; it is not filed, so the next run asks again.
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
            WIRE_FAILED.with(|failed| failed.set(true));
            return Ok(vec![Reply::Unreadable(format!(
                "the fixer could not be asked: {why}"
            ))]);
        }
    };
    let reply = llm_request::reply_from(&sent.content_type, &sent.body)
        .map_err(|why| format!("{STEP}: {why}"))?;
    let rows = fix_transcripts::answered_rows(reply.answer());
    if rows.is_empty() {
        let answer = reply.answer().trim();
        if !answer.is_empty() && !answer.contains('\n') && !answer.contains('\t') {
            return Ok(vec![Reply::Bare(answer.to_string())]);
        }
        return Ok(vec![Reply::Unreadable(
            fix_transcripts::NO_LINES.to_string(),
        )]);
    }
    Ok(rows.into_iter().map(Reply::Row).collect())
}

/// What a block is asked under besides its rows, as its cache key holds it: the fix system wording and the project's User Context. A change to either asks every block again.
pub fn cache_prompt(paths: &Paths, context: &str) -> String {
    format!(
        "{}\n\n{}",
        fix_system(paths),
        crate::prompt_assembly::without_script(context)
    )
}

fn user_context(tree: &Tree) -> String {
    crate::project::load(tree.dir())
        .map(|project| project.context)
        .unwrap_or_default()
}

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

/// The wording to ask with, through the settings read path so an edit wins (§04-prepare#6); the shipped wording when none is on disk.
fn wording(paths: &Paths, key: &str) -> String {
    let fallback = crate::prompts::shipped_text(key).unwrap_or_default();
    settings::prompt_text(paths, key, &fallback).unwrap_or(fallback)
}
