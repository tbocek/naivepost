//! F4.2 The narration call — the two messages this crate sends, assembled from parts that already
//! own their wording (`spec/07-narrate.md` F4.2 S1/S2), and the attempt loop that reads one answer
//! back (S3/S4/S5).
//!
//! `narrate_pass` builds the job's text and its addenda but reads no files, and `prompt_assembly`
//! cuts the house rules to one job's sections but is never called with them: nothing until now held
//! the two ends together. This module is that join plus the loop over attempts. It writes no file and
//! opens no socket — the reply arrives through a caller-supplied closure exactly as in
//! [`crate::describe`] and [`crate::fix_transcripts`].
//!
//! Why the wording is an argument rather than something looked up: this build bakes no shipped prompt
//! text into the crate ([`crate::bench::SHIPPED_FOR_TESTS`] is empty and
//! [`crate::prompts::shipped_file`] answers only a file NAME that nothing here reads), so a builder
//! that went hunting for `prompts/` would invent a load path the app does not have. The door takes
//! the two wordings from [`crate::settings::prompt_text`] with an empty fallback, which is what the
//! settings box itself shows when this machine holds no edit.

use crate::cut::{Fx, Seg};
use crate::layout::Tree;
use crate::narrate_pass;
use crate::narrate_reply;
use crate::narrate_run;
use crate::project::Project;
use crate::textfmt::SessionLine;
use serde::Deserialize;

/// One narration call's two messages, as data: held apart so a test can read each side without a
/// server, and so the request can be logged or re-sent without rebuilding it.
#[derive(Debug, Clone)]
pub struct Request {
    pub system: String,
    pub user: String,
}

/// §F4.2's message pair, pure: every string comes from a builder that is itself tested, and no
/// wording is retyped here.
///
/// SYSTEM = the house rules cut to narrate's sections + the "narrate" prompt + the speech addendum
/// when the clips play what people said out loud + the captions addendum for a captions-only voice
/// + the precedence rule when there is a User Context to outrank anything (§09 §8 puts that last).
/// USER = the User Context block with the speech rule, then the clips brief — that whole shape is
/// [`crate::narrate_pass::user_message`]'s, unchanged.
///
/// An empty piece adds nothing: a project with no context gets no precedence rule and no header over
/// nothing, matching the rule both message builders already follow.
pub fn request(
    job_rules: &str,
    system_rules: &str,
    context: &str,
    segs: &[Seg],
    rows: &[SessionLine],
    fx: &[Fx],
    narrator: &str,
    captions_only: bool,
) -> Request {
    let speech = crate::narrate_pass::speech_heard(segs, rows, narrator);
    let job = crate::narrate_pass::system(job_rules, speech, captions_only);

    // Same pieces-join as `prompt_assembly::system_message`: house rules first (that module owns
    // which sections narrate gets), then the job's own wording with its addenda, then the precedence
    // rule only when there is a context for it to rank above.
    let mut pieces: Vec<String> = Vec::new();
    let house = crate::prompt_assembly::cut_system(system_rules, "narrate");
    if !house.trim().is_empty() {
        pieces.push(house);
    }
    if !job.trim().is_empty() {
        pieces.push(job);
    }
    if !context.trim().is_empty() {
        pieces.push(crate::prompt_assembly::precedence_rule());
    }

    Request {
        system: pieces.join("\n\n"),
        user: crate::narrate_pass::user_message(context, segs, rows, fx, narrator),
    }
}

/// The same pair filled from a live session folder. The only part of this module that reads anything,
/// kept separate so the assembly above stays testable with no display, no settings and no project on
/// disk. A missing `cut.json` or `session.tsv` yields an empty brief rather than a failed run: the
/// refusals in F4.1 S1 are asked before this is ever reached, so arriving here means both exist and
/// an unreadable one says so through the empty clips it produces.
pub fn request_from_tree(
    tree: &Tree,
    project: &Project,
    voice: &str,
    job_rules: &str,
    system_rules: &str,
) -> Request {
    let cut_ = crate::cut::load(tree).unwrap_or_default();
    let rows = crate::textfmt::read_session(&tree.session_tsv()).unwrap_or_default();
    let narrator = crate::sources::narrator_of(project, 1).unwrap_or("");
    request(
        job_rules,
        system_rules,
        &project.context,
        &cut_.segs,
        &rows,
        &cut_.fx,
        narrator,
        crate::narrate_run::voice_is_captions_only(voice),
    )
}

// --- S3/S4/S5: the attempt loop ---------------------------------------------------------------

/// One entry of the prototype's `{"entries":[{start,end,at,text,emotion}]}` reply shape (§F4.2).
///
/// A local struct rather than [`crate::narration::Entry`] because the two differ where it matters: an
/// `Entry` stores its bounds as the clip's own (copied verbatim from the cut) and is what a saved file
/// holds, while this one carries what the model *echoed*, which is exactly the thing being checked —
/// a match within [`narrate_reply::ENTRY_MATCH_TOLERANCE_SECONDS`] turns one into the other.
#[derive(Debug, Clone, Deserialize)]
struct EntryReply {
    start: f64,
    end: f64,
    #[serde(default)]
    at: f64,
    #[serde(default)]
    text: String,
    #[serde(default)]
    emotion: String,
}

#[derive(Debug, Deserialize)]
struct ReplyBody {
    entries: Vec<EntryReply>,
}

/// Whether a tool answer is a success rather than the `{}` error shape every refusal comes in — read as
/// JSON, because both of its answers are JSON and matching on a prefix would break the moment a
/// sentence starts like a key (the same test `cut_captions` uses for its own tools).
fn is_ok(answer: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(answer).is_ok_and(|body| body.get("error").is_none())
}

/// §F4.2's attempt loop: ask, read the echoed bounds forward, refuse what does not hold up, ask again
/// with the reason, three times, then give up with nothing written.
///
/// Rejections are handed to `on_reject` rather than printed: this crate has no log sink of its own (the
/// window owns `ui::log_line`), and a module that wrote to stderr would be invisible in the app and
/// unverifiable in a test. What arrives there is [`narrate_pass::rejected_log`]'s line, and
/// [`narrate_pass::thinking_off_log`]'s when thinking was switched off by this attempt.
///
/// The cursor over `clips` only ever advances, so a reply that walks back to a clip already passed
/// matches nothing and is refused rather than quietly re-sorted — silently reordering lines is how a
/// narration ends up spoken out of order ([`narrate_reply::unmatched_entry_fault`]). Silence has to
/// be *said*: a clip nobody answered fails the attempt just as an empty line does, and a reply whose
/// every line is empty is a model refusing the job, not exercising taste.
///
/// Nothing here is derived that another module already owns: the bound count is
/// [`narrate_pass::attempts`], the tolerance is [`narrate_reply::ENTRY_MATCH_TOLERANCE_SECONDS`], the
/// completeness check is [`narrate_pass::finish`], and the bar's monotonicity is
/// [`narrate_pass::progress_forward`]. Fitting (F4.3) is measured by the caller and handed to
/// `finish` through `unfitted`; this loop neither owns nor redoes that arithmetic.
///
/// No cache is consulted: [`narrate_pass::served_from_cache`] answers false and nothing here reads one,
/// for the reason §F4.2 gives — the TTS cache is the cache.
pub fn drive<FReply, FProgress, FUnfitted, FReject>(
    clips: &[Seg],
    mut reply: FReply,
    mut progress: FProgress,
    mut unfitted: FUnfitted,
    mut on_reject: FReject,
) -> Result<Vec<narrate_run::Written>, String>
where
    FReply: FnMut(u32 /*attempt*/, bool /*thinking*/) -> Result<String, String>,
    FProgress: FnMut(u32 /*written*/, u32 /*total*/),
    FUnfitted: FnMut(&[narrate_run::Written]) -> Vec<u32>,
    FReject: FnMut(&str),
{
    let total = clips.len() as u32;
    // Thinking starts on (§F4.2: "Thinking on") and is switched off by the rule below; it never
    // comes back on, because a model that answered once was not held back by the reasoning budget.
    let mut thinking = true;
    let mut best = 0u32;
    let mut last_problem = String::new();

    for attempt in 1..=narrate_pass::attempts() {
        let answer = reply(attempt, thinking)?;
        let body: ReplyBody = match serde_json::from_str(&answer) {
            Ok(parsed) => parsed,
            Err(problem) => {
                last_problem = format!("the reply was not the entries shape: {problem}");
                reject(&mut on_reject, attempt, &last_problem, false, &mut thinking);
                continue;
            }
        };

        // Forward-only walk: `cursor` is the next clip an entry may land on and never moves back.
        let mut written: Vec<narrate_run::Written> = Vec::new();
        let mut answered: Vec<u32> = Vec::new();
        let mut cursor = 0usize;
        let mut problem = String::new();
        for entry in &body.entries {
            let found = (cursor..clips.len()).find(|&i| {
                (entry.start - clips[i].s).abs() <= narrate_reply::ENTRY_MATCH_TOLERANCE_SECONDS
                    && (entry.end - clips[i].e).abs() <= narrate_reply::ENTRY_MATCH_TOLERANCE_SECONDS
            });
            let Some(index) = found else {
                problem = narrate_reply::unmatched_entry_fault(entry.start, entry.end);
                break;
            };
            // Land the line, take the clip: the next entry may only go forward from here.
            cursor = index + 1;
            answered.push(index as u32 + 1);
            written.push(narrate_run::Written {
                start: clips[index].s,
                end: clips[index].e,
                at: entry.at,
                text: entry.text.clone(),
                emotion: entry.emotion.clone(),
            });
            // Streaming progress = written clips / clips, and only ever forward.
            best = narrate_pass::progress_forward(best, written.len() as u32);
            progress(best, total);
        }
        if !problem.is_empty() {
            last_problem = problem;
            reject(&mut on_reject, attempt, &last_problem, !written.is_empty(), &mut thinking);
            continue;
        }
        if written.iter().all(|line| line.text.trim().is_empty()) {
            last_problem = narrate_reply::all_silent_fault();
            reject(&mut on_reject, attempt, &last_problem, false, &mut thinking);
            continue;
        }
        // A clip nobody answered, said the way `finish` says it: silence must be explicit.
        let missing: Vec<u32> = (1..=total).filter(|n| !answered.contains(n)).collect();
        if let Some(first) = missing.first() {
            last_problem = narrate_reply::clip_without_entry(*first - 1, clips);
            reject(&mut on_reject, attempt, &last_problem, true, &mut thinking);
            continue;
        }
        // Completeness and fit are `finish`'s answer, in its own words.
        let verdict = narrate_pass::finish(&answered, &unfitted(&written));
        if !is_ok(&verdict) {
            last_problem = verdict;
            reject(&mut on_reject, attempt, &last_problem, true, &mut thinking);
            continue;
        }
        return Ok(written);
    }
    Err(last_problem)
}

/// Report one rejected attempt and apply the thinking rule to the next one. `wrote_anything` is whether
/// this attempt placed any line at all: a call spent entirely thinking gets no reasoning budget again
/// ([`narrate_pass::thinking_after`]), and the second log line says so only when that switch happened.
fn reject<FReject>(
    on_reject: &mut FReject,
    attempt: u32,
    problem: &str,
    wrote_anything: bool,
    thinking: &mut bool,
) where
    FReject: FnMut(&str),
{
    on_reject(&narrate_pass::rejected_log(attempt, problem));
    let still = narrate_pass::thinking_after(*thinking, wrote_anything);
    if *thinking && !still {
        on_reject(&narrate_pass::thinking_off_log());
    }
    *thinking = still;
}

#[cfg(test)]
mod smoke {
    use super::*;

    #[test]
    fn a_request_carries_both_messages() {
        let got = request("JOB", "HOUSE", "", &[], &[], &[], "", false);
        // The house rules lead and the job's own wording follows them, per §09 §8.
        assert_eq!(got.system, "HOUSE\n\nJOB");
        assert!(got.user.contains(crate::narrate_pass::CLIPS_HEADER));
    }

    #[test]
    fn an_empty_house_prompt_leaves_the_job_alone_and_no_context_adds_no_precedence_rule() {
        let got = request("JOB", "", "", &[], &[], &[], "", false);
        assert_eq!(got.system, "JOB");
    }

    #[test]
    fn a_context_outranks_the_job_so_the_precedence_rule_closes_the_system_message() {
        let got = request("JOB", "", "a game session", &[], &[], &[], "", false);
        assert!(got.system.ends_with(&crate::prompt_assembly::precedence_rule()));
    }
}
