//! §F2.14 S2b, the live leg: the model mode's cut call, asked from the Cut page's ▶.

use crate::cut_effect_decisions::Candidate;
use crate::cut_suggest;
use crate::cut_suggest_words_run;
use crate::exchanges::{Message, Part, ToolCall};
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

pub const WATCH_IDLE: &str = "";
pub fn correction(problems: &[String]) -> String {
    format!(
        "That is not a valid cut: {}. Add, drop or move segments and call finish_cut again.",
        suggest::answer(problems)
    )
}
fn read_answer(plan: &cutpass::Plan, call: &ToolCall) -> Option<String> {
    match call.name.as_str() {
        // The transcript this cut was proposed from is already in the brief the plan was asked with; a
        // second copy is not what these two reads are for, and §3's rule is that a problem comes back as
        // a sentence the model can act on rather than as silence it has to guess at.
        "get_lines" | "get_events" => Some(crate::tools::error(&format!(
            "{} is not answered for this job: the lines and events this cut was asked about are already \
             in the brief above",
            call.name
        ))),
        "get_frames" | "get_context" => {
            let frames = frames_of_footage(plan.footage_end());
            let ctx = crate::tool_reads::Context {
                user_context: "",
                job: crate::roles::Job::ModelCut,
                policy: &[("targetLengthSeconds", plan.target().to_string())],
                lines: &[],
                events: &[],
                frames: &frames,
            };
            crate::tool_reads::dispatch(&ctx, call)
        }
        _ => None,
    }
}

/// How many frames one `get_frames` answer may name: enough to aim a segment, few enough that a read
/// cannot blow the request cap every chat call is checked against (`llm_budget`).
const FRAMES_ANSWER_MAX: usize = 200;

/// The seconds a cut may be aimed at, one per `P.project.frameInterval` out to the end of the footage.
fn frames_of_footage(footage_end: f64) -> Vec<f64> {
    let interval = crate::prepare::MODEL_FRAME_STEP;
    let mut at = 0.0;
    let mut out = Vec::new();
    while at < footage_end && out.len() < FRAMES_ANSWER_MAX {
        out.push(at);
        at += interval;
    }
    out
}

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

    fn landed_log(log: &[String]) -> String {
        log.first()
            .map(|line| line.trim_start_matches(">>> ").to_string())
            .unwrap_or_default()
    }
}

/// The walk the answer is walked back against, from the words path's own three inputs: one reading of
/// the session, spelled once, so §S2a and §S2b cut on the same seconds.
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

pub fn user_turn(user_context: &str, session_len: f64, target: f64, session_txt: &str) -> String {
    user_turn_named(user_context, session_len, target, session_txt, &[], &[])
}

pub fn user_turn_named(
    user_context: &str,
    session_len: f64,
    target: f64,
    session_txt: &str,
    rows: &[String],
    lanes: &[String],
) -> String {
    crate::prompt_assembly::user_message(
        user_context,
        "cut",
        &suggest::message_over(
            session_len,
            target,
            suggest::DEAD_AIR_MAX_SECONDS,
            session_txt,
            rows,
            lanes,
        ),
    )
}

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

fn wording(paths: Option<&Paths>, key: &str) -> Option<String> {
    let fallback = crate::join_ask::shipped_file(key)?;
    match paths {
        Some(paths) => settings::prompt_text(paths, key, &fallback).ok(),
        None => Some(fallback),
    }
}

pub fn answer_call(plan: &mut cutpass::Plan, call: &ToolCall) -> String {
    if let Some(answer) = read_answer(plan, call) {
        return answer;
    }
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

pub fn past_end_claim(problems: &[String], footage_end: f64) -> Option<u32> {
    let marker = " is past the end of the footage, which runs to ";
    for said in problems {
        let (claim, _) = said.split_once(marker)?;
        let digits: String = claim.chars().filter(|c| c.is_ascii_digit()).collect();
        let claimed: u32 = digits.parse().ok()?;
        if f64::from(claimed) > footage_end {
            return Some(claimed);
        }
    }
    None
}

#[allow(clippy::too_many_arguments)]
pub fn opening(
    system: &str,
    user: &str,
    complaint: Option<&str>,
    note: Option<&str>,
) -> Vec<Message> {
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
    if let Some(note) = note {
        messages.push(Message {
            role: "user".to_string(),
            parts: vec![Part::Text(note.to_string())],
        });
    }
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
    note: Option<&str>,
    plan: &mut cutpass::Plan,
    target: f64,
    ask: &mut F,
) -> (bool, bool, Vec<String>)
where
    F: FnMut(&[Message], &[Tool]) -> Result<tool_loop::Turn, String>,
{
    attempt_with_tools(
        system,
        user,
        complaint,
        note,
        plan,
        target,
        &tools::offered(Job::ModelCut),
        ask,
    )
}

#[allow(clippy::too_many_arguments)] // one argument per thing an attempt is asked with; folding them hides it
pub fn attempt_with_tools<F>(
    system: &str,
    user: &str,
    complaint: Option<&str>,
    note: Option<&str>,
    plan: &mut cutpass::Plan,
    target: f64,
    offered: &[Tool],
    ask: &mut F,
) -> (bool, bool, Vec<String>)
where
    F: FnMut(&[Message], &[Tool]) -> Result<tool_loop::Turn, String>,
{
    let messages = opening(system, user, complaint, note);
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
    attempt_with(system, user, None, None, plan, target, ask)
}

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
    let mut rows: Vec<String> = cut
        .segs
        .iter()
        .filter(|scene| scene.ins.is_empty())
        .map(|scene| (crate::cut_cam::scene_row(scene) + 1).to_string())
        .collect();
    rows.sort();
    rows.dedup();
    let lanes: Vec<String> = cut.lanes.iter().map(|lane| lane.name.clone()).collect();
    let user = user_turn_named(
        &project.context,
        end,
        target,
        &read_session_txt(tree),
        &rows,
        &lanes,
    );
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
        // §12: and the model is TOLD which of the two changed, beside the complaint about its last answer.
        // Read AFTER `offer`, which owns the withdrawal and marks the sentence as owed.
        let note = call.note_to_model();
        let (finished, ok, round_logs) = attempt_with_tools(
            &system,
            &user,
            complaint,
            note.as_deref(),
            &mut plan,
            target,
            &offered,
            &mut watch,
        );
        for reply in last_reply.borrow().iter() {
            let progress = call.report(reply, end);
            crate::ui::cut_suggest_progress::watch(&progress);
        }
        logs.extend(round_logs);
        // §8: an answer that was all reasoning and no answer gets its next call asked WITHOUT thinking, and
        // the log says why the question changed.
        logs.extend(retry_note(&mut call, &last_reply));
        if finished && ok {
            // §8's drop runs BEFORE the length is judged, so the count the walk-back is built from is the one
            // the page ends up with: a segment with no recording at either end names a stretch of session that
            // was never filmed, and nothing downstream should be sized around it.
            let dropped = cut_suggest::drop_unfilmed(placed(&plan), &filmed);
            if dropped.1 > 0 {
                logs.push(cut_suggest::dropped_log(number, dropped.1));
            }
            // Nothing filmed in the whole answer: landing it would empty the cut.
            if dropped.0.is_empty() {
                continue;
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
                // §4 gives the cut thinking ON; read from the prompt table (§10 §4) rather than named here.
                Ok(llm_request::body(
                    STEP,
                    &model,
                    conversation,
                    crate::prompts::mode_of(STEP),
                    offer,
                    true,
                )?
                .to_string())
            },
            &|body| dial(tree, &model, &endpoint, body),
            &crate::ui::window::log_line,
        )?;
        let parsed =
            llm_request::reply_from(&content_type, &reply).map_err(|why| unreadable(&why))?;
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

pub(crate) struct Press {
    paths: Option<Paths>,
    project: Project,
    cut: crate::cut::Cut,
}

/// Why a press cannot ask, and whose sentence it is.
pub(crate) enum Blocked {
    NoSession,
    Settings(String),
}

pub(crate) fn press_parts(window: &adw::ApplicationWindow) -> Result<(Tree, Press), Blocked> {
    let Some(tree) = session() else {
        return Err(Blocked::NoSession);
    };
    let paths = settings::from_environment();
    if let Some(Err(reason)) = paths.as_ref().map(settings::read) {
        return Err(Blocked::Settings(reason));
    }
    Ok((
        tree,
        Press {
            paths,
            project: crate::ui::window::session_handle(window).borrow().clone(),
            cut: crate::ui::window::newest_review_cut(),
        },
    ))
}

pub(crate) fn blocked(why: Blocked) -> Landed {
    match why {
        Blocked::NoSession => Landed::refused(suggest::NO_TIMELINE),
        Blocked::Settings(reason) => Landed::refused(&unreadable(&format!(
            "settings could not be read: {reason}"
        ))),
    }
}

/// The window's door for `suggest_press`'s model arm: read the settings this press dials, ask, walk
/// back, answer.
pub fn press(window: &adw::ApplicationWindow, busy: bool) -> Landed {
    let _ = busy;
    let (tree, parts) = match press_parts(window) {
        Ok(got) => got,
        Err(why) => return blocked(why),
    };
    let conf = match parts.paths.as_ref().map(settings::read) {
        Some(Ok(conf)) => conf,
        _ => settings::Conf::default(),
    };
    let mut ask = live_ask(&tree, &conf);
    finish(&tree, parts, &mut ask)
}

/// The ask half of [`press`], with what [`press_parts`] read off the window: what Suggest's worker runs
/// (§03-shell §1). The run's `llm/` page is this thread's, so it is opened and closed around the ask.
pub(crate) fn ask_on_worker(tree: Tree, parts: Press) -> Landed {
    crate::exchange_leg::note_run_started();
    let conf = match parts.paths.as_ref().map(settings::read) {
        Some(Ok(conf)) => conf,
        _ => settings::Conf::default(),
    };
    let mut ask = live_ask(&tree, &conf);
    let landed = finish(&tree, parts, &mut ask);
    crate::exchange_leg::note_run_ended();
    landed
}

/// `P.policy.targetLengthSeconds` — the length this cut aims at, seconds; §10's default is 0 = none.
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
