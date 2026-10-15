//! §F2.14 S2b, the live leg: the model mode's cut call, asked from the Cut page's ▶.
//!
//! `ui::cut_suggest_land::suggest_press` answered the model arm with nothing at all and opened no bar.
//! This is the ask: the words path's three inputs are read the same way, the model is shown the session
//! timeline it copies seconds off, its five tools are answered by [`crate::tools::cutpass::Plan`], and
//! whatever passed is walked back by [`crate::suggest::apply`] before anyone is told a cut exists.
//!
//! Three rules decide the shape of this module, and none of them is style:
//! - The model answers in **seconds of the session timeline**, the only clock the timeline it was
//!   shown is measured in (`02-services.md` §3.6: "session seconds copied off lines"). `Plan`'s snap
//!   points move every edge it accepts and `suggest::apply` moves them again, so a number a model
//!   typed never reaches a cut unchecked.
//! - The verdict is [`suggest::whole_cut_problems`], the same check `finish_cut` judges with. A finish
//!   the tool answered `ok` to is still re-checked here, because §S2 S6's "no valid cut after 3
//!   attempts" is about the CUT, not about whether the model stopped asking.
//! - **Nothing is written here.** §S3's landing is `cut_suggest_land::land`'s (one ↶, base moves);
//!   `run_model` proposes and returns. And no `cache/llm` entry: a reply the whole-cut check refused
//!   must not be served back on the retry, which is `captions_ask::ask_slot`'s rule and another item's.
//!
//! Every network leg is a caller-supplied closure, so no test dials a socket — the shape
//! `fix_call` and `narrate_tts::speak_line` already use.

use crate::cut_suggest;
use crate::cut_suggest_words_run;
use crate::cut_effect_decisions::Candidate;
use crate::exchanges::{Message, Mode, Part, ToolCall};
use crate::layout::Tree;
use crate::llm_request;
use crate::project::Project;
use crate::roles::Job;
use crate::services::{self, Server};
use crate::settings::{self, Paths};
use crate::suggest;
use crate::timeline;
use crate::tool_loop;
use crate::tools::{self, cutpass, Tool};

/// The step name this call's `requests.tsv` row and its log lines carry: the word the §S2 order gives
/// this stage, so the log and the request table name one thing.
pub const STEP: &str = "cut";
/// The shipped prompt this request is built from (`spec/prompts/cut.md`, through the settings copy).
pub const PROMPT_KEY: &str = "cut";
/// §S2b's first pass, counted from 1. The bound itself is `P.eng.llmAttempts` and is asked through
/// [`suggest::attempts_exhausted`], so the loop never repeats the number.
pub const FIRST_ATTEMPT: u32 = 1;
/// The spans a proposed segment must have a recording under: the filmed runs of this session's kept footage,
/// read straight off the session's recordings rather than off what the words path happened to keep. `runs_of`
/// trims a run to its words and drops a recording nobody spoke over, so reading only its spans would drop a
/// segment inside a real, wordless recording — footage that exists. §8's rule is about footage.
pub fn filmed_spans(inputs: &cut_suggest_words_run::Inputs) -> Vec<(f64, f64)> {
    let spoken: Vec<(f64, f64)> = inputs.runs.iter().map(|run| run.span).collect();
    let _ = inputs;
    let recordings: Vec<timeline::Recording> = spoken
        .iter()
        .map(|(start, end)| timeline::Recording {
            base: String::new(),
            start: *start,
            end: *end,
        })
        .collect();
    timeline::filmed_runs(&recordings)
}

/// The reply text the attempt loop watches, for §8's two "the reply that just arrived" rules: whether the
/// next call still asks for reasoning, and where the progress line sits. Filled by [`watch_reply`], which
/// wraps the caller's `ask`, so what is read is the bytes the dial handed back — never a scripted position.
pub const WATCH_IDLE: &str = "";
/// §S2 S7's `one rewrite, from the first answer`: the answer a refused attempt had already placed, and
/// what `correction` is wrapped around. `attempt`'s `complaint` is `None` on the first pass — the model
/// has not been told anything yet — and [`suggest::answer`]'s join after one, §S2 S6's `every fault at
/// once, worst first`, in `cutpass`'s words.
pub fn correction(problems: &[String]) -> String {
    format!(
        "That is not a valid cut: {}. Add, drop or move segments and call finish_cut again.",
        suggest::answer(problems)
    )
}
/// §05-cut#8: `the checks answer with every fault at once, worst first, joined by "; "`. `check` is the
/// whole-cut check's own list; `past_end` is a number the model wrote with its colons taken out, which is
/// faulted with the conversion shown on the model's OWN digits (`2804 … 1684`) because that is the only way
/// it can tell "too big" from "misread". With no such number the wording is unchanged — `cutpass` words a
/// count or a length, and §8 is about ordering and joining, not re-wording.
pub fn correction_from(check: &[String], past_end: Option<u32>) -> String {
    format!(
        "That is not a valid cut: {}. Add, drop or move segments and call finish_cut again.",
        crate::ui::cut_suggest_watch::faults(check, past_end)
    )
}

/// §S2b's wording for a call that could not be read at all: named job, named reason, per §02's rule
/// that a failure says which model it came from.
pub fn unreadable(why: &str) -> String {
    format!("{STEP}: {why}")
}

/// What a model press produced: the status line, the lines for `cut.log.json`, whether a cut landed,
/// the walked-back segments, and the full §S3 answer so the door can land it without re-deciding.
#[derive(Debug, Clone, PartialEq)]
pub struct Landed {
    pub said: String,
    pub log: Vec<String>,
    pub wrote: bool,
    pub segs: Vec<(f64, f64)>,
    pub applied: suggest::Applied,
}

impl Landed {
    fn refused(reason: &str) -> Self {
        Landed {
            said: reason.to_string(),
            log: Vec::new(),
            wrote: false,
            segs: Vec::new(),
            applied: suggest::Applied::default(),
        }
    }

    /// The status row's sentence for a landed cut. §S3 spells the pair `suggest::apply` returns and
    /// §S2b has no sentence of its own for a model cut that landed, so the log's own line is what the
    /// page says; inventing a third wording would be a third thing to keep in step.
    fn landed_log(log: &[String]) -> String {
        log.first()
            .map(|line| line.trim_start_matches(">>> ").to_string())
            .unwrap_or_default()
    }
}

/// The walk the answer is walked back against, from the words path's own three inputs: one reading of
/// the session, spelled once, so §S2a and §S2b cut on the same seconds.
///
/// The aligner's per-word times stay private to `cut_suggest_words_run` (`word_seconds`), so the word
/// edges here are the run edges that module already trusts — what `trim_run_to_words` yields — and each
/// silence contributes its midpoint, the two candidate kinds §S3's snap weights name.
pub fn walk_of(inputs: &cut_suggest_words_run::Inputs) -> suggest::Walk {
    let talked: Vec<(f64, f64)> = inputs.runs.iter().map(suggest::trim_run_to_words).collect();
    let mut snap_points: Vec<(f64, Candidate)> = Vec::new();
    for (start, end) in &talked {
        snap_points.push((*start, Candidate::WordEdge));
        snap_points.push((*end, Candidate::WordEdge));
    }
    for (from, to) in &inputs.silences {
        snap_points.push(((from + to) / 2.0, Candidate::SilenceMidpoint));
    }
    suggest::Walk {
        talked,
        silences: inputs.silences.clone(),
        snap_points,
        marks: inputs.marks.clone(),
    }
}

/// The footage end the model may not cut past: the last second any kept run reaches. `Plan` answers a
/// segment that goes further with §3.6's "past the end" fault, mm:ss reading included.
pub fn footage_end(inputs: &cut_suggest_words_run::Inputs) -> f64 {
    inputs
        .runs
        .iter()
        .map(|run| run.span.1)
        .fold(0.0_f64, f64::max)
}

/// The holes: every second up to `end` that no kept run films. `Plan::with_gaps` answers a segment
/// spending length in one as "no footage under an edge" rather than placing a piece of nothing.
pub fn gaps_of(inputs: &cut_suggest_words_run::Inputs, end: f64) -> Vec<(f64, f64)> {
    let mut filmed: Vec<(f64, f64)> = inputs.runs.iter().map(|run| run.span).collect();
    filmed.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let mut holes: Vec<(f64, f64)> = Vec::new();
    let mut cursor = 0.0;
    for (from, to) in filmed {
        if from > cursor {
            holes.push((cursor, from));
        }
        cursor = cursor.max(to);
    }
    if end > cursor {
        holes.push((cursor, end));
    }
    holes
}

/// The request's user turn: §S2b's order — the person's context, the session length, the target block,
/// the timeline — delegated to [`suggest::message`] so the SESSION LENGTH line and the target block are
/// that module's wording and cannot drift from the words path's or from the spec's.
pub fn user_turn(user_context: &str, session_len: f64, target: f64, session_txt: &str) -> String {
    suggest::message(user_context, session_len, target, session_txt)
}

/// The system message, assembled the way every other ask does it: the shipped `system` and `cut`
/// wording, through the settings copy so an edit in Settings wins (§04-prepare#6). `None` when the
/// shipped text cannot be read — the press then names that instead of asking a model with no job.
pub fn system_message(user_context: &str, paths: Option<&Paths>) -> Option<String> {
    let job = wording(paths, PROMPT_KEY)?;
    let system = wording(paths, "system")?;
    Some(crate::prompt_assembly::system_message(
        &system,
        PROMPT_KEY,
        &job,
        user_context,
        false,
        false,
    ))
}

/// The wording reader every ask uses: the shipped file, or the settings copy of it. Spelled privately
/// here for the same reason `captions_ask` spells it: widening that module's helper would make one
/// flow's setting another flow's default.
fn wording(paths: Option<&Paths>, key: &str) -> Option<String> {
    let fallback = crate::join_ask::shipped_file(key)?;
    match paths {
        Some(paths) => settings::prompt_text(paths, key, &fallback).ok(),
        None => Some(fallback),
    }
}

/// One tool call, answered by the plan. The five names are `tools::Tool`'s and the arguments are
/// §3.6's: `add_segment` `start`/`end`/`why`, `remove_segment` `start`, `set_speed` `start`/`rate`,
/// `cut_status` and `finish_cut` with none.
///
/// A missing or non-numeric argument answers the tool's own error naming the argument that is absent.
/// Defaulting it to `0.0` would place a zero-length segment at the head of the session and teach the
/// model nothing, which is the opposite of what §3.6's result column is for.
pub fn answer_call(plan: &mut cutpass::Plan, call: &ToolCall) -> String {
    let args: serde_json::Value =
        serde_json::from_str(&call.args).unwrap_or_else(|_| serde_json::json!({}));
    let seconds = |name: &str| -> Option<f64> { args.get(name).and_then(|v| v.as_f64()) };
    match call.name.as_str() {
        "add_segment" => {
            let (Some(start), Some(end)) = (seconds("start"), seconds("end")) else {
                return tools::error(&missing("add_segment", &args, &["start", "end"]));
            };
            plan.add_segment(start, end, why_of(&args))
        }
        "remove_segment" => match seconds("start") {
            Some(start) => plan.remove_segment(start),
            None => tools::error(&missing("remove_segment", &args, &["start"])),
        },
        "set_speed" => {
            let (Some(start), Some(rate)) = (seconds("start"), seconds("rate")) else {
                return tools::error(&missing("set_speed", &args, &["start", "rate"]));
            };
            plan.set_speed(start, rate)
        }
        // Both read the plan rather than this module: `cut_status` is the reading `finish_cut` would
        // give at any time, and `finish_cut` is the whole-cut check §S2 S6 counts attempts against.
        "cut_status" => plan.status_json(),
        "finish_cut" => plan.finish_cut(),
        other => tools::error(&format!("no such tool: {other}")),
    }
}

/// The model's reason for keeping a stretch, kept beside the segment so a caller can show the choice
/// next to the cut (`cutpass::Segment::why`). Absent is answered with what is true, not a made-up reason.
fn why_of(args: &serde_json::Value) -> &str {
    args.get("why")
        .and_then(|v| v.as_str())
        .filter(|why| !why.trim().is_empty())
        .unwrap_or("no reason given")
}

/// The error for a call that named a tool and not its arguments: only what is actually absent is
/// listed, so a model that sent `start` is not told to send `start`.
fn missing(tool: &str, args: &serde_json::Value, want: &[&str]) -> String {
    let absent: Vec<&str> = want
        .iter()
        .copied()
        .filter(|name| args.get(*name).and_then(|v| v.as_f64()).is_none())
        .collect();
    format!("{tool} needs {} as seconds", absent.join(" and "))
}

/// The plan one attempt drives: footage it cannot invent past, the window it is judged against, the
/// holes it may not cut from, the marks that will come out of it, and the edges it must land on.
pub fn new_plan(
    inputs: &cut_suggest_words_run::Inputs,
    walk: &suggest::Walk,
    end: f64,
    target: f64,
) -> cutpass::Plan {
    cutpass::Plan::new(end, target)
        .with_gaps(gaps_of(inputs, end), inputs.marks.clone())
        .with_snap_points(walk.snap_points.iter().map(|(point, _)| *point).collect())
}

/// The segments the model settled on, as plain pairs — the shape `suggest::apply` and
/// [`suggest::whole_cut_problems`] both take.
pub fn placed(plan: &cutpass::Plan) -> Vec<(f64, f64)> {
    plan.segments().iter().map(|s| (s.start, s.end)).collect()
}

/// Whether the plan is a cut, by the check `finish_cut` answers with. Nothing is re-derived here: the
/// segment count's floor and ceiling and the footage window belong to `cutpass`.
pub fn acceptable(plan: &cutpass::Plan, target: f64) -> bool {
    suggest::whole_cut_problems(&placed(plan), target).is_empty()
}

/// The number the model typed that fell past the end of the footage, in ITS OWN digits — the shape §8's
/// conversion is about. A model that stamps `[28:04]` and drops the colon writes `2804`; the tool refused it
/// against the footage end in seconds, so the digits are recovered from the refusal the model was given, and
/// a refusal that quoted no such number answers `None` rather than guessing one.
pub fn past_end_claim(problems: &[String], footage_end: f64) -> Option<u32> {
    let marker = " is past the end of the footage, which runs to ";
    for said in problems {
        let (claim, _) = said.split_once(marker)?;
        // The tool quotes its own `mm:ss`; the digits WITHOUT the colon are exactly what the model sent.
        let digits: String = claim.chars().filter(|c| c.is_ascii_digit()).collect();
        let claimed: u32 = digits.parse().ok()?;
        if f64::from(claimed) > footage_end {
            return Some(claimed);
        }
    }
    None
}

/// One attempt: the two messages, `tool_loop::run` over the offered tools, every call answered from
/// `plan`, and whether the plan it ended with is a cut. The loop's log lines come back in order — the
/// `>>>` round lines, §3's per-call notes, and §2's `!!!` line when the rounds ran out.
///
/// The loop's `finish` is not this function's verdict: `finished` says the model stopped asking, `ok`
/// says what it stopped with is a cut. §S2 S6 needs both, because a round that ran out and an answered
/// finish that failed the checks get different sentences.
#[allow(clippy::too_many_arguments)]
/// The conversation one attempt starts from: the two messages §S2b spells, plus the correction round
/// when there is one. §S2 S7's rewrite is a SECOND REQUEST with the complaint in it and the answer
/// already placed as its starting point — the same rule `captions_ask::ask_slot` exists for on the
/// caption batches: a retry must never be served the reply it was just refused for, so the cache is
/// kept out of this call entirely (`llm_cache::uses_cache(Job::ModelCut)` answers false). Spelled
/// separately so a test pins what the model is told, in order, with no socket in the way.
pub fn opening(system: &str, user: &str, complaint: Option<&str>) -> Vec<Message> {
    let mut messages = vec![
        Message {
            role: "system".to_string(),
            parts: vec![Part::Text(system.to_string())],
        },
        Message {
            role: "user".to_string(),
            parts: vec![Part::Text(user.to_string())],
        },
    ];
    if let Some(complaint) = complaint {
        messages.push(Message {
            role: "user".to_string(),
            parts: vec![Part::Text(complaint.to_string())],
        });
    }
    messages
}

#[allow(clippy::too_many_arguments)]
pub fn attempt_with<F>(
    system: &str,
    user: &str,
    complaint: Option<&str>,
    plan: &mut cutpass::Plan,
    target: f64,
    ask: &mut F,
) -> (bool, bool, Vec<String>)
where
    F: FnMut(&[Message], &[Tool]) -> Result<tool_loop::Turn, String>,
{
    attempt_with_tools(system, user, complaint, plan, target, &tools::offered(Job::ModelCut), ask)
}

/// The same attempt with the tool list handed in. §8's web-tool rule decides per attempt which tools go out,
/// so the list is the caller's to own — reading `tools::offered` inside would put the decision in two places
/// and let the loop log a withdrawal the request then ignored.
pub fn attempt_with_tools<F>(
    system: &str,
    user: &str,
    complaint: Option<&str>,
    plan: &mut cutpass::Plan,
    target: f64,
    offered: &[Tool],
    ask: &mut F,
) -> (bool, bool, Vec<String>)
where
    F: FnMut(&[Message], &[Tool]) -> Result<tool_loop::Turn, String>,
{
    let messages = opening(system, user, complaint);
    let run = tool_loop::run(
        STEP,
        messages,
        offered,
        |conversation, offered| ask(conversation, offered),
        |call| answer_call(plan, call),
    );
    let finished = matches!(run.outcome, tool_loop::Outcome::Finished { .. });
    (finished, acceptable(plan, target), run.logs)
}

/// One attempt with nothing to correct — the first pass, and the shape a test drives.
pub fn attempt<F>(
    system: &str,
    user: &str,
    plan: &mut cutpass::Plan,
    target: f64,
    ask: &mut F,
) -> (bool, bool, Vec<String>)
where
    F: FnMut(&[Message], &[Tool]) -> Result<tool_loop::Turn, String>,
{
    attempt_with(system, user, None, plan, target, ask)
}

/// §S2b then §S3, over everything the page holds: the attempts, then the walk-back of the one answer
/// that passed. `ask` is the network leg — nothing else here reaches a socket, and nothing here writes
/// a file.
///
/// A plan that `finish_cut` accepted still goes through [`suggest::apply`], which is §S3 in one call:
/// talked-in holes closed, both edges snapped, marks and dead air out, coalesced, effects clamped
/// against the footage that survived. Hand-placed inserts go in untouched — §S3 keeps them — so they
/// are read off the cut on screen rather than proposed.
pub fn run_model<F>(
    tree: &Tree,
    project: &Project,
    cut: &crate::cut::Cut,
    target: f64,
    paths: Option<&Paths>,
    mut ask: F,
) -> Landed
where
    F: FnMut(&[Message], &[Tool]) -> Result<tool_loop::Turn, String>,
{
    let system = match system_message(&project.context, paths) {
        Some(text) => text,
        None => return Landed::refused(&unreadable("no prompt to ask with")),
    };
    let inputs = cut_suggest_words_run::inputs(tree, project, cut);
    let end = footage_end(&inputs);
    if !suggest::has_target(target) && end <= cutpass::NO_TARGET {
        // §S1's timeline question is answered before the press reaches here; this is the case where
        // there IS a timeline and nothing filmed under it. Nothing to propose, in S1's words.
        return Landed::refused(suggest::NO_TIMELINE);
    }
    let walk = walk_of(&inputs);
    let user = user_turn(&project.context, end, target, &read_session_txt(tree));
    let inserts: Vec<crate::cut::Seg> = cut
        .segs
        .iter()
        .filter(|seg| seg.is_insert())
        .cloned()
        .collect();
    let mut logs: Vec<String> = Vec::new();
    let mut previous: Option<Vec<String>> = None;
    // §8's attempt state — thinking, web tools, and where the progress line last stood — carried as one thing
    // so no branch can update part of it and forget the rest.
    let mut call = crate::ui::cut_suggest_watch::Call::new();
    // §2's firefox setting, read once for the whole run: it decides whether the web pair is offered at all, and
    // the reason is said once per step however many attempts and rounds follow. An unset box means "find one on
    // PATH", which is `web_browser::firefox_path`'s rule — so a machine with a browser still gets both tools.
    let firefox = paths
        .and_then(|paths| settings::read(paths).ok())
        .map(|conf| conf.firefox)
        .unwrap_or_default();
    if !crate::llm_web_offer::availability(&firefox).0 {
        call.with_web(false);
    }
    // The replies this attempt received, refreshed by [`watch_reply`] on every dial. §8's progress and thinking
    // rules are both about the reply that just arrived, so they read this and nothing else.
    let last_reply = std::cell::RefCell::new(Vec::new());
    // §S2b's "streaming progress counts segments": the replies are collected by the dial (below), and the
    // progress each one earns is reported after the round, where this loop still owns `call` — the same `Call`
    // answers §8's thinking rule there, and a closure borrowing it for the whole attempt would not allow it.
    let mut watch = watch_reply(&mut ask, &last_reply);
    // What every second of this session was filmed by: §8 drops a segment with no recording at either end
    // BEFORE its length is judged, because a stretch nobody filmed has no length to argue about.
    let filmed = filmed_spans(&inputs);
    // The bound is `P.eng.llmAttempts`, asked through `attempts_exhausted` so the number lives once.
    // Every attempt gets a fresh `Plan`: a correction round repairs the model's answer, not the tool's
    // arithmetic, and a plan carried over would let a refused segment survive into the next answer.
    for number in FIRST_ATTEMPT.. {
        // §S2 S6's complaint goes back to the model as its own correction request: every fault at once,
        // worst first, in `cutpass`'s words, and with §8's past-the-end conversion when the model's own
        // digits are recoverable from the refusal it got. The first pass has none to send.
        let complaint = previous
            .as_ref()
            .map(|problems| correction_from(problems, past_end_claim(problems, end)))
            .unwrap_or_default();
        let complaint = (!complaint.is_empty()).then_some(complaint.as_str());
        if suggest::attempts_exhausted(number) {
            break;
        }
        // §8: the web tools come off after the FIRST rejection, and the page is told they did.
        let (offered, withdrawn) = call.offer();
        logs.extend(withdrawn);
        // §2: and a machine with no browser was told why, once, in §2's own words.
        if let Some(line) = crate::llm_web_offer::unavailable_log(STEP, &firefox) {
            logs.push(line);
        }
        let mut plan = new_plan(&inputs, &walk, end, target);
        last_reply.borrow_mut().clear();
        let (_finished, ok, round_logs) =
            attempt_with_tools(&system, &user, complaint, &mut plan, target, &offered, &mut watch);
        // The progress this attempt's replies earned, reported now that the loop owns `call` again.
        // `Call::report` answers the bar's sentence, its fraction and whether it still pulses; `watch` is the
        // page's door, and no door means no page to tell (a logic test), which is the same answer.
        for reply in last_reply.borrow().iter() {
            let progress = call.report(reply, end);
            crate::ui::cut_suggest_progress::watch(&progress);
        }
        logs.extend(round_logs);
        // §8: an answer that was all reasoning and no answer gets its next call asked WITHOUT thinking, and
        // the log says why the question changed.
        logs.extend(retry_note(&mut call, &last_reply));
        if ok {
            // §8's drop runs BEFORE the length is judged, so the count the walk-back is built from is the one
            // the page ends up with: a segment with no recording at either end names a stretch of session that
            // was never filmed, and nothing downstream should be sized around it.
            let dropped = cut_suggest::drop_unfilmed(placed(&plan), &filmed);
            if dropped.1 > 0 {
                logs.push(cut_suggest::dropped_log(number, dropped.1));
            }
            let applied = suggest::apply(&dropped.0, &inserts, &cut.fx, &walk);
            let said = Landed::landed_log(&applied.log);
            // §S3's two log lines go out with the round lines, in the order the flowchart prints them:
            // the rounds that produced the answer, then what the walk-back made of it.
            let mut log = std::mem::take(&mut logs);
            log.extend(applied.log.iter().cloned());
            return Landed {
                said,
                log,
                wrote: true,
                segs: applied.segs.clone(),
                applied,
            };
        }
        // A refused answer counts against BOTH of §8's rules: the check below is the correction's source, and
        // the web tools are withdrawn on the first one. An EMPTY reply was already charged as the first
        // rejection when the thinking rule read it, so charging it twice would move the withdrawal one attempt
        // early and make the web-tool rule depend on which of two readers looked first.
        if call.answered(&replies_text(&last_reply)) {
            call.note_rejected();
        }
        previous = Some(suggest::whole_cut_problems(&placed(&plan), target));
    }
    logs.push(format!(">>> {STEP}: {}", suggest::NO_VALID_CUT));
    Landed {
        said: suggest::NO_VALID_CUT.to_string(),
        log: logs,
        wrote: false,
        segs: Vec::new(),
        applied: suggest::Applied::default(),
    }
}

/// The wrapper that keeps §8's three "the reply that just arrived" rules honest: the attempt loop reads the
/// text the dial ACTUALLY handed back, so neither the thinking rule nor the progress numbers can be scripted.
/// The request is streamed (`live_ask` passes `streaming: true`), but the reply this loop reads is still one
/// snapshot per call — the delta pump in [`crate::llm_stream_leg`] belongs to another item, so a call's whole
/// answer arrives at once — and the loop needs EVERY call of an attempt, not the last one: a model that asked
/// for three segments and then
/// called `finish_cut` wrote its answer in the first reply and said nothing in the second. So the running text
/// is a log of every reply this attempt received, and "wrote nothing" means not one reply wrote anything.
pub fn watch_reply<'a, F>(
    ask: &'a mut F,
    seen: &'a std::cell::RefCell<Vec<String>>,
) -> impl FnMut(&[Message], &[Tool]) -> Result<tool_loop::Turn, String> + 'a
where
    F: FnMut(&[Message], &[Tool]) -> Result<tool_loop::Turn, String>,
{
    move |conversation, offered| {
        let turn = ask(conversation, offered)?;
        // Record the reply for §8's thinking and web-tool rules, and for the progress report the loop makes
        // from it once it owns `call` again (§S2b's "streaming progress counts segments").
        seen.borrow_mut()
            .push(crate::ui::cut_suggest_watch::reply_text(&turn));
        Ok(turn)
    }
}

/// The replies one attempt received, as one text: empty only when every reply in it was empty, which is what
/// §8's "the model spent the whole call thinking and wrote nothing" reads.
fn replies_text(replies: &std::cell::RefCell<Vec<String>>) -> String {
    let mut text = String::new();
    for reply in replies.borrow().iter() {
        text.push_str(reply);
    }
    text
}

/// §8's thinking rule, read off the reply that just arrived: `Some` is the one line saying the next call asks
/// without reasoning. Split from the loop because the loop is already reading four rules at once.
fn retry_note(
    call: &mut crate::ui::cut_suggest_watch::Call,
    replies: &std::cell::RefCell<Vec<String>>,
) -> Option<String> {
    call.note_reply_text(&replies_text(replies))
}

/// `session.txt` — the timeline the model copies its seconds off. An unreadable file goes into the
/// request as an empty timeline, which answers itself; guessing a timeline is not an option.
pub fn read_session_txt(tree: &Tree) -> String {
    std::fs::read_to_string(tree.session_txt()).unwrap_or_default()
}

/// The network half of the ask, spelled once: the same timed, retried chat call every LLM job makes,
/// with this step's row and the window's log. Answers the reply's content type and body, so a test can
/// substitute its own and no test dials a socket.
pub fn dial(
    tree: &Tree,
    model: &str,
    endpoint: &services::Endpoint,
    body: &str,
) -> Result<(String, String), String> {
    let base = crate::server_leg::base_row(Server::Llm, model, "POST", "", STEP, "cut");
    let sent = crate::llm_retry_leg::chat(
        STEP,
        tree,
        &base,
        endpoint,
        body,
        &crate::cancel_leg::cancel_check_now(),
        &crate::ui::window::log_line,
    )?;
    Ok((sent.content_type, sent.body))
}

/// The live ask leg: body → [`dial`] → turn. §S2b's row says thinking ON, which is `Mode::Thinking`;
/// the web tools come from `tools::offered`, which already carries them for this job (§S2b's "dropped
/// after a first rejection" is `cut_suggest::offers_web`'s rule and another item's).
pub fn live_ask<'a>(
    tree: &'a Tree,
    conf: &'a settings::Conf,
) -> impl FnMut(&[Message], &[Tool]) -> Result<tool_loop::Turn, String> + 'a {
    let endpoint = services::llm_endpoint(conf);
    let model = conf.model.clone();
    move |conversation: &[Message], offered: &[Tool]| {
        // §2 S5: the cut's own ask honours a refusal of the `tools` field like every other tool flow, so one
        // refusing server costs a log line and a bare resend rather than the whole step.
        let (content_type, reply) = crate::llm_tool_refusal_leg::dial_honouring_refusal(
            STEP,
            &endpoint.url,
            Some(offered),
            &|offer| {
                Ok(llm_request::body(&model, conversation, Mode::Thinking, offer, true)?.to_string())
            },
            &|body| dial(tree, &model, &endpoint, body),
            &crate::ui::window::log_line,
        )?;
        let parsed = llm_request::reply_from(&content_type, &reply).map_err(|why| unreadable(&why))?;
        let finishing = parsed
            .tool_calls
            .iter()
            .any(|call| call.name == Tool::FinishCut.name());
        Ok(tool_loop::Turn {
            content: parsed.content.clone(),
            calls: parsed.tool_calls.clone(),
            finish: finishing,
        })
    }
}

pub use crate::ui::cut_suggest_land::session;

/// What [`press`] reads off the window before anything is asked. `None` answers §S1's
/// own sentence for the case §S1 could not see, and an unreadable settings file answers its reason
/// rather than a dial to a guessed address (§02-services#1: settings are re-read per request).
struct Press {
    paths: Option<Paths>,
    project: Project,
    cut: crate::cut::Cut,
}

/// Why a press cannot ask, and whose sentence it is.
enum Blocked {
    NoSession,
    Settings(String),
}

fn press_parts(window: &adw::ApplicationWindow) -> Result<(Tree, Press), Blocked> {
    let Some(tree) = session() else {
        return Err(Blocked::NoSession);
    };
    let paths = settings::from_environment();
    if let Some(Err(reason)) = paths.as_ref().map(settings::read) {
        return Err(Blocked::Settings(reason));
    }
    Ok((tree, Press {
        paths,
        project: crate::ui::window::session_handle(window).borrow().clone(),
        cut: crate::ui::window::newest_review_cut(),
    }))
}

fn blocked(why: Blocked) -> Landed {
    match why {
        Blocked::NoSession => Landed::refused(suggest::NO_TIMELINE),
        Blocked::Settings(reason) => {
            Landed::refused(&unreadable(&format!("settings could not be read: {reason}")))
        }
    }
}

/// The window's door for `suggest_press`'s model arm: read the settings this press dials, ask, walk
/// back, answer.
///
/// `busy` is taken and unused on purpose: the press that refused a busy bar never reaches here — §S1's
/// order (busy → hand edits → timeline → the word gate) is answered upstream in
/// `cut_suggest_land::refuse_suggest_run` and is not re-decided here. The cut is landed by the caller,
/// which owns the one-↶ write.
pub fn press(window: &adw::ApplicationWindow, busy: bool) -> Landed {
    let _ = busy;
    let (tree, parts) = match press_parts(window) {
        Ok(got) => got,
        Err(why) => return blocked(why),
    };
    // `press_parts` already proved the file reads; reading it twice is what honours a change made
    // between the check and the dial, which is the whole point of reading settings per request. No
    // settings folder at all is no settings: the compiled-in loopback default, as every other ask.
    let conf = match parts.paths.as_ref().map(settings::read) {
        Some(Ok(conf)) => conf,
        _ => settings::Conf::default(),
    };
    let mut ask = live_ask(&tree, &conf);
    finish(&tree, parts, &mut ask)
}

/// `P.policy.targetLengthSeconds` — the length this cut aims at, seconds; §10's default is 0 = none.
///
/// Read on the press path rather than hardcoded at the call: the day F0.7 derives a number from the User
/// Context it writes the policy field and every `suggest::target_block`/length-gate answer on this page moves
/// with it, with nothing rewired. F0.7's derivation is another item's round.
pub fn target_length_seconds(policy: &crate::project::Policy) -> f64 {
    policy.target_length_seconds.value.max(0.0)
}

/// The last half of the press: one model call over what the page holds, then every `>>>` line to the
/// log so the flowchart's story is in the file the log window shows.
fn finish<F>(tree: &Tree, parts: Press, ask: &mut F) -> Landed
where
    F: FnMut(&[Message], &[Tool]) -> Result<tool_loop::Turn, String>,
{
    let landed = run_model(
        tree,
        &parts.project,
        &parts.cut,
        // The target comes from the session's own policy through [`target_length_seconds`], not from a literal
        // here: `suggest::target_block` then answers "NO TARGET LENGTH: keep what is worth keeping." and the
        // whole-cut check drops its length complaint, which is what a policy with no derived length means.
        target_length_seconds(&parts.project.policy),
        parts.paths.as_ref(),
        ask,
    );
    for line in &landed.log {
        crate::ui::window::log_line(line);
    }
    landed
}
