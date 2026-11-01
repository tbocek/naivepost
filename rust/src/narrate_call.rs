//! F4.2 The narration call — the two messages this crate sends, assembled from parts that already own their
//! wording (`spec/07-narrate.md` F4.2 S1/S2), and the attempt loop that reads one answer back (S3/S4/S5).
//!
//! No file or socket: the reply arrives through a caller-supplied closure as in [`crate::describe`] and
//! [`crate::fix_transcripts`]. The wordings are arguments (from [`crate::settings::prompt_text`] with the
//! shipped text as fallback), so no builder here invents a load path for `spec/prompts/<key>.md`.

use crate::cut::{Fx, Seg};
use crate::cut_captions::is_ok;
use crate::layout::Tree;
use crate::narrate_pass;
use crate::narrate_reply;
use crate::narrate_run;
use crate::project::Project;
use crate::textfmt::SessionLine;

/// One narration call's two messages, as data, so a test can read each side and the request can be re-sent.
#[derive(Debug, Clone)]
pub struct Request {
    pub system: String,
    pub user: String,
}

/// §F4.2's message pair, pure: every string comes from a tested builder.
///
/// SYSTEM = the house rules cut to narrate's sections + the "narrate" prompt + the speech addendum when the
/// clips play what people said + the captions addendum for a captions-only voice + the precedence rule when
/// there is a User Context (§09 §8 puts that last). USER = [`crate::narrate_pass::user_message`]'s shape.
///
/// An empty piece adds nothing: no context means no precedence rule and no header over nothing.
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

    // Same join as `prompt_assembly::system_message`: house rules, then the job's wording with its addenda,
    // then the precedence rule only when there is a context to rank above.
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

/// The same pair filled from a live session folder, the only part that reads anything. A missing `cut.json`
/// or `session.tsv` yields an empty brief, not a failed run (F4.1 S1's refusals are asked before this).
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

/// §F4.2's attempt loop: ask, read the echoed bounds forward, refuse what does not hold up, ask again with
/// the reason, three times, then give up with nothing written.
///
/// Rejections go to `on_reject` ([`narrate_pass::rejected_log`], and [`narrate_pass::thinking_off_log`]
/// when this attempt switched thinking off), as this crate has no log sink of its own.
///
/// The cursor over `clips` only advances, so a reply that walks back matches nothing and is refused rather
/// than re-sorted (silent reordering speaks a narration out of order; [`narrate_reply::unmatched_entry_fault`]).
/// Silence has to be *said*: a clip nobody answered fails the attempt like an empty line, and a reply of
/// all-empty lines is a model refusing the job.
///
/// Owned elsewhere: the bound count [`narrate_pass::attempts`], the tolerance
/// [`narrate_reply::ENTRY_MATCH_TOLERANCE_SECONDS`], completeness [`narrate_pass::finish`], bar monotonicity
/// [`narrate_pass::progress_forward`]. Fitting (F4.3) is measured by the caller and handed to `finish` via
/// `unfitted`. No cache is consulted ([`narrate_pass::served_from_cache`] is false): the TTS cache is the cache (§F4.2).
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
    // Thinking starts on (§F4.2) and is switched off by the rule below; it never comes back on.
    let mut thinking = true;
    let mut best = 0u32;
    let mut last_problem = String::new();

    for attempt in 1..=narrate_pass::attempts() {
        let answer = reply(attempt, thinking)?;
        // `narrate_reply` accepts the wrapper document, a bare list and a single entry object; refusing two of
        // them would burn an attempt over a wrapper nobody asked for.
        let entries = match narrate_reply::entries_of(&answer) {
            Ok(entries) => entries,
            Err(problem) => {
                last_problem =
                    format!("the reply could not be read as narration entries: {problem}");
                reject(&mut on_reject, attempt, &last_problem, false, &mut thinking);
                continue;
            }
        };

        // Forward-only walk: `cursor` is the next clip an entry may land on.
        let mut written: Vec<narrate_run::Written> = Vec::new();
        let mut answered: Vec<u32> = Vec::new();
        let mut cursor = 0usize;
        let mut problem = String::new();
        for entry in &entries {
            let found = (cursor..clips.len()).find(|&i| {
                (entry.start - clips[i].s).abs() <= narrate_reply::ENTRY_MATCH_TOLERANCE_SECONDS
                    && (entry.end - clips[i].e).abs()
                        <= narrate_reply::ENTRY_MATCH_TOLERANCE_SECONDS
            });
            let Some(index) = found else {
                problem = narrate_reply::unmatched_entry_fault(entry.start, entry.end);
                break;
            };
            // Land the line, take the clip.
            cursor = index + 1;
            answered.push(index as u32 + 1);
            written.push(narrate_run::Written {
                start: clips[index].s,
                end: clips[index].e,
                at: entry.at,
                text: entry.text.clone(),
                emotion: entry.emotion.clone(),
            });
            // Streaming progress = written clips / clips, only forward.
            best = narrate_pass::progress_forward(best, written.len() as u32);
            progress(best, total);
        }
        if !problem.is_empty() {
            last_problem = problem;
            reject(
                &mut on_reject,
                attempt,
                &last_problem,
                !written.is_empty(),
                &mut thinking,
            );
            continue;
        }
        if written.iter().all(|line| line.text.trim().is_empty()) {
            last_problem = narrate_pass::no_line_fault();
            reject(&mut on_reject, attempt, &last_problem, false, &mut thinking);
            continue;
        }
        // A clip nobody answered, said the way `finish` says it.
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

/// §12 #22 / #27: the SAME attempt loop, but a round that answered through tools is matched by CLIP NUMBER
/// in ANY ORDER (a tool call names its clip outright; §12 #27's one policy). One clip skipped fails the whole
/// attempt (#22) and says which, through `finish`'s sentence; a clip number outside this edit's clips fails it too.
pub fn drive_by_clips<FReply, FProgress, FUnfitted, FReject>(
    clips: &[Seg],
    mut round: FReply,
    mut progress: FProgress,
    mut unfitted: FUnfitted,
    mut on_reject: FReject,
) -> Result<Vec<narrate_run::Written>, String>
where
    FReply: FnMut(
        u32,  /*attempt*/
        bool, /*thinking*/
    ) -> Result<crate::narrate_live::Round, String>,
    FProgress: FnMut(u32, u32),
    FUnfitted: FnMut(&[narrate_run::Written]) -> Vec<u32>,
    FReject: FnMut(&str),
{
    let total = clips.len() as u32;
    let mut thinking = true;
    let mut best = 0u32;
    let mut last_problem = String::new();

    for attempt in 1..=narrate_pass::attempts() {
        let said = round(attempt, thinking)?;
        // Prose instead of tool calls has no clip numbers to trust: hand it to `drive`, whose bound match carries the information.
        if !said.used_tools {
            return drive(
                clips,
                |_attempt, _thinking| Ok(said.answer.clone()),
                progress,
                unfitted,
                on_reject,
            );
        }
        let mut problem = String::new();
        // A second call for a clip already written is §12 #22's "thirty-nine good clips lost for the fortieth":
        // refused by name rather than silently keeping the first or last.
        let mut slots: Vec<Option<&crate::narrate_live::Picked>> = vec![None; clips.len()];
        for pick in &said.picks {
            let Some(index) = (1..=total).find(|n| *n == pick.clip) else {
                problem =
                    crate::cut_captions::clip_outside(pick.clip, 1, total).unwrap_or_else(|| {
                        format!("clip {} is not one of this edit's clips", pick.clip)
                    });
                break;
            };
            let slot = &mut slots[index as usize - 1];
            if slot.is_some() {
                problem = format!(
                    "clip {n} was written twice — one line per clip, rewrite it once",
                    n = index
                );
                break;
            }
            *slot = Some(pick);
        }
        if !problem.is_empty() {
            last_problem = problem;
            reject(&mut on_reject, attempt, &last_problem, false, &mut thinking);
            continue;
        }
        // Ordered by clip: the render lays lines out by position.
        let written: Vec<narrate_run::Written> = slots
            .iter()
            .enumerate()
            .filter_map(|(index, slot)| {
                let pick = (*slot)?;
                Some(narrate_run::Written {
                    start: clips[index].s,
                    end: clips[index].e,
                    at: pick.at,
                    text: pick.text.clone(),
                    emotion: pick.emotion.clone(),
                })
            })
            .collect();
        if written.iter().all(|line| line.text.trim().is_empty()) {
            last_problem = narrate_pass::no_line_fault();
            reject(&mut on_reject, attempt, &last_problem, false, &mut thinking);
            continue;
        }
        // Silence must be SAID: a clip with neither a line nor `leave_silent` fails the attempt, named.
        let missing: Vec<u32> = (1..=total)
            .filter(|n| slots[(*n - 1) as usize].is_none())
            .collect();
        if let Some(first) = missing.first() {
            last_problem = narrate_reply::clip_without_entry(*first - 1, clips);
            reject(&mut on_reject, attempt, &last_problem, true, &mut thinking);
            continue;
        }
        let answered: Vec<u32> = (1..=total).collect();
        best = narrate_pass::progress_forward(best, written.len() as u32);
        progress(best, total);
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

/// Report one rejected attempt and apply the thinking rule to the next one. `wrote_anything` is whether the
/// attempt placed any line: a call spent entirely thinking gets no reasoning budget again
/// ([`narrate_pass::thinking_after`]).
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
        assert!(got
            .system
            .ends_with(&crate::prompt_assembly::precedence_rule()));
    }
}
