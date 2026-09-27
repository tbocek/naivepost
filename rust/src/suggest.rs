//! F2.14 Suggest a cut — `spec/05-cut.md` F2.14.
//!
//! The whole flow as plain data: guards in, segments and sentences out. Nothing here opens a window,
//! writes a file or contacts a server, so every step is testable with handed-in values — the requests
//! themselves arrive with the runner rounds, the same way [`crate::describe`] and [`crate::retakes`]
//! settle their first steps off disk.
//!
//! What this module owns and what it only reads:
//! - [`crate::tools::cutpass`] answers the target arithmetic and the whole-cut checks; this module
//!   never re-decides a bound.
//! - [`crate::cut_effect_decisions`] holds the dead-air comparison and the weighted edge score; this
//!   module supplies the candidates and picks the winner per direction.
//! - [`crate::roles`] / [`crate::policy`] answer which job runs at all, so a context that rules a pass
//!   out means the pass is never called rather than called and told to return nothing.
//! - [`crate::cut_clamp`] clamps effects against the cut **as applied**, which is why the clamp happens
//!   here, after the walk-back moved every second an effect was chosen for.
//!
//! Persisting the result and making it the new base are the CALLER's job — one `ui::record_edit`, one
//! Undo (F2.13). This module writes nothing, twice over for the reason above and once more because a
//! flow that saves cannot be run twice in a test.

use crate::cut::{Cut, Seg, Fx};
use crate::cut_clamp;
use crate::cut_effect_decisions::{self as decisions, Candidate};
use crate::policy::{self, Pass};
use crate::project::{MarkingPass, Policy};
use crate::roles::{self, Job};
use crate::timeline;
use crate::tools::{self, cutpass};

// --- S1: the guards -------------------------------------------------------------------------------------
//
// Three questions in a fixed order, answered before anything is asked of a model. Each refusal is a
// sentence the page prints verbatim; none of them is recomputed here from another module's state —
// `busy` arrives from `run::RunBar::running.is_some()` and `hand_edits` from `hand_edit`'s mtime
// answer, both read by the caller. Duplicating either question here would let the guard and the thing it
// guards drift apart.

/// DECISION (the spec gives this branch only the word "refused"): a busy bar gets its own sentence,
/// because "refused" alone does not tell the person what to do about it.
pub const BUSY: &str = "a run is already going \u{2014} stop it before asking for a new cut";
/// S1, quoted from the flowchart.
pub const HAND_EDITS: &str = "you have hand edits \u{2014} press Revert first for a fresh suggestion";
/// S1, quoted from the flowchart.
pub const NO_TIMELINE: &str =
    "run Describe first \u{2014} the suggestion reads the session timeline, and there is none";
/// S2b, after the last correction round. `P.eng.llmAttempts`
pub const NO_VALID_CUT: &str = "no valid cut after 3 attempts";

/// Why ▶ did not ask for a cut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refuse {
    Busy,
    HandEdits,
    NoTimeline,
}

impl Refuse {
    /// The sentence this refusal says.
    pub fn said(&self) -> &'static str {
        match self {
            Refuse::Busy => BUSY,
            Refuse::HandEdits => HAND_EDITS,
            Refuse::NoTimeline => NO_TIMELINE,
        }
    }
}

/// S1: the first "no" wins, in the flowchart's order busy → hand edits → timeline.
///
/// Order matters as a fact about the machine, not about politeness: a run under way may be about to write
/// the very timeline whose absence is being reported, so answering the timeline question then would tell
/// the person to go and run Describe while their own ▶ is still filling it.
pub fn refuse(busy: bool, hand_edits: bool, has_timeline: bool) -> Option<Refuse> {
    if busy {
        return Some(Refuse::Busy);
    }
    if hand_edits {
        return Some(Refuse::HandEdits);
    }
    if !has_timeline {
        return Some(Refuse::NoTimeline);
    }
    None
}

// --- S2a: the words path (no model) ----------------------------------------------------------------------

/// A silence longer than this inside a kept clip is cut out. `P.policy.deadAirMaxSeconds`
pub const DEAD_AIR_MAX_SECONDS: f64 = 8.0;
/// The beat left where dead air was taken out. `P.policy.deadAirKeepSeconds`
pub const DEAD_AIR_KEEP_SECONDS: f64 = 0.5;
/// How far past the first and last spoken word of a run the trimmed span may reach, placed by the sound.
/// `suggest.trimReachSeconds` — S2a's "placed by the sound within 0.4 s". §6 gives this no `P.` id, so
/// it takes a bare prefix like `machine.jpegQuality` did.
pub const TRIM_REACH_SECONDS: f64 = 0.4;

/// One filmed run with the two seconds that bookend its speech. The span is what was recorded; the
/// spoken pair is what the transcript says was said inside it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpokenRun {
    pub span: (f64, f64),
    pub first_spoken: f64,
    pub last_spoken: f64,
}

/// S2a: every filmed run trimmed to its words — just before the first to just after the last, and
/// never outside the run.
///
/// Never outside the run: the clamp is the whole of that rule. A reach that crossed the run's own end
/// would spend the next recording's seconds on this one's last syllable, and the next run starts where
/// this one stops, so the overlap would be silent footage counted twice.
pub fn trim_run_to_words(run: &SpokenRun) -> (f64, f64) {
    let start = (run.first_spoken - TRIM_REACH_SECONDS).max(run.span.0);
    let end = (run.last_spoken + TRIM_REACH_SECONDS).min(run.span.1);
    (start, end.max(start))
}

/// Subtract every mark from the kept spans. Returns the pieces left and how many marks actually took
/// seconds out — a mark sitting over footage that is already gone is not counted, because saying it went
/// would report a cut that did not happen.
pub fn remove_marks(segs: &[(f64, f64)], marks: &[(f64, f64)]) -> (Vec<(f64, f64)>, usize) {
    let mut out: Vec<(f64, f64)> = Vec::new();
    let mut taken = 0usize;
    for &(from, to) in segs {
        let mut pieces = vec![(from, to)];
        for &(m_from, m_to) in marks {
            let mut next: Vec<(f64, f64)> = Vec::new();
            let mut ate = false;
            for (p_from, p_to) in pieces {
                if !timeline::spans_overlap((m_from, m_to), (p_from, p_to)) {
                    next.push((p_from, p_to));
                    continue;
                }
                ate = true;
                // The mark splits this piece into whatever lies before it and whatever lies after.
                if m_from > p_from {
                    next.push((p_from, m_from));
                }
                if m_to < p_to {
                    next.push((m_to, p_to));
                }
            }
            pieces = next;
            if ate {
                taken += 1;
            }
        }
        out.extend(pieces.into_iter().filter(|(a, b)| *b > *a));
    }
    (out, taken)
}

/// Take the dead air out of the kept spans: only a silence strictly longer than `max` goes (exactly the
/// maximum is a deliberate beat, and the brief tells the model the number so it can keep one by asking),
/// and `keep` of the beat survives at the front of where the silence sat.
///
/// Returns the kept spans and the total seconds removed. A silence partly outside the kept footage costs
/// only the part inside it — you cannot cut what is not there.
pub fn remove_dead_air(
    segs: &[(f64, f64)],
    silences: &[(f64, f64)],
    max: f64,
    keep: f64,
) -> (Vec<(f64, f64)>, f64) {
    let mut out: Vec<(f64, f64)> = Vec::new();
    let mut removed = 0.0_f64;
    for &(from, to) in segs {
        let mut pieces = vec![(from, to)];
        for &(s_from, s_to) in silences {
            let long = decisions::dead_air_is_cut(s_to - s_from, max);
            let survives = decisions::dead_air_kept(s_to - s_from, max, keep);
            let mut next: Vec<(f64, f64)> = Vec::new();
            for (p_from, p_to) in pieces {
                let lo = s_from.max(p_from);
                let hi = s_to.min(p_to);
                if hi <= lo || !long {
                    next.push((p_from, p_to));
                    continue;
                }
                // Keep `survives` of the beat at the head of the silence; the rest is gone.
                let cut_from = (lo + survives).min(hi);
                removed += hi - cut_from;
                next.push((p_from, cut_from.min(p_to)));
                if cut_from < p_to {
                    next.push((hi.max(p_from), p_to));
                }
            }
            pieces = next;
        }
        out.extend(pieces.into_iter().filter(|(a, b)| *b > *a));
    }
    (out, removed)
}

/// Merge touching and overlapping spans into one list, sorted.
pub fn coalesce(segs: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut sorted = segs.to_vec();
    sorted.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let mut out: Vec<(f64, f64)> = Vec::new();
    for (from, to) in sorted {
        match out.last_mut() {
            // Touching counts: two pieces meeting at the same second are one stretch of video.
            Some(last) if from <= last.1 => last.1 = last.1.max(to),
            _ => out.push((from, to)),
        }
    }
    out
}

/// What the words path produced.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WordsCut {
    pub segs: Vec<(f64, f64)>,
    pub marks_out: usize,
    pub silence_out: f64,
    pub status: String,
    pub log: String,
}

/// S2a: the whole words path — trim every run to its words, take the marked stretches out, take the
/// dead air out, coalesce. No model is asked, so this answers with no server and no cache.
pub fn words_cut(runs: &[SpokenRun], marks: &[(f64, f64)], silences: &[(f64, f64)]) -> WordsCut {
    let trimmed: Vec<(f64, f64)> = runs.iter().map(trim_run_to_words).collect();
    let (unmarked, marks_out) = remove_marks(&trimmed, marks);
    let (kept, silence_out) =
        remove_dead_air(&unmarked, silences, DEAD_AIR_MAX_SECONDS, DEAD_AIR_KEEP_SECONDS);
    let segs = coalesce(&kept);
    let total: f64 = segs.iter().map(|(a, b)| b - a).sum();
    WordsCut {
        status: format!("cut by the words: {} segments", segs.len()),
        log: format!(
            ">>> cut by the words: {} stretch(es) taken out, {} of silence, {} segments, {} total",
            marks_out,
            tools::mm_ss(silence_out),
            segs.len(),
            tools::mm_ss(total)
        ),
        segs,
        marks_out,
        silence_out,
    }
}

// --- S2b: the model path ---------------------------------------------------------------------------------

/// Whether this cut aims at a length at all. `P.policy.targetLengthSeconds` — 0 means none, and a
/// length of zero is not a very short video.
pub fn has_target(target: f64) -> bool {
    target > cutpass::NO_TARGET
}

/// The footage the finished cut accepts, from the target. Delegates the whole arithmetic.
pub fn target_window(target: f64) -> (f64, f64) {
    cutpass::footage_window(target)
}

/// The target block of the cut message: the two forms §S2b spells, one for a cut that aims at a length
/// and one for a cut that does not. A model told "no target" must not infer a ceiling from nothing.
pub fn target_block(target: f64) -> String {
    if !has_target(target) {
        return "NO TARGET LENGTH: keep what is worth keeping.".to_string();
    }
    let (low, high) = target_window(target);
    format!(
        "KEEP between {} and {} seconds of footage, in at most {} segments.",
        cutpass::clock(low),
        cutpass::clock(high),
        cutpass::max_segments(target)
    )
}

/// The cut request's own order: the person's context first (it is what the cut is for), then how long the
/// session is, then what the finished length must be, then the timeline it has to read.
pub fn message(context: &str, session_len: f64, target: f64, session_txt: &str) -> String {
    format!(
        "{context}\n\nSESSION LENGTH: {} seconds\n\n{}\n\nSESSION TIMELINE:\n{}",
        session_len as i64,
        target_block(target),
        session_txt
    )
}

/// Whether the model cut is asked for at all. Under `CutMode::Words` this answers false and no call is
/// made — the timeline comes from the marked text instead.
pub fn model_cut_offered(policy_: &Policy, marking_pass: MarkingPass) -> bool {
    roles::job_applies(Job::ModelCut, marking_pass, policy::cut_mode_of(policy_))
}

/// Which of the three after-the-cut passes run. An empty answer is the whole point: a context that says
/// "no captions, no speed changes, no effects" means those jobs are NEVER CALLED, not called and told
/// to return nothing — a call that returns nothing still costs a model, still shows on the bar, and
/// still could hand back something the policy ruled out.
pub fn passes_switched_on(policy_: &Policy) -> Vec<Pass> {
    [Pass::Captions, Pass::Speeds, Pass::Decorations]
        .into_iter()
        .filter(|pass| policy::pass_runs(policy_, *pass))
        .collect()
}

/// Whether attempt `attempt` was the last one allowed. `P.eng.llmAttempts`
pub fn attempts_exhausted(attempt: u32) -> bool {
    roles::cut_attempts_exhausted(attempt)
}

/// The whole-cut checks as a problem list, worst first — the ordering is [`cutpass::cut_problems`]'s own
/// and is not touched here, because the tool that rejected the answer is what phrases the complaint.
pub fn whole_cut_problems(segs: &[(f64, f64)], target: f64) -> Vec<String> {
    let footage: f64 = segs.iter().map(|(a, b)| b - a).sum();
    let short_scenes: Vec<(f64, f64)> = segs
        .iter()
        .filter(|(a, b)| b - a < cutpass::MIN_SCENE_SECONDS)
        .map(|(a, b)| (*a, b - a))
        .collect();
    let status = cutpass::Status {
        footage,
        window: target_window(target),
        segments: segs.len(),
        min_segments: cutpass::min_segments(target),
        max_segments: cutpass::max_segments(target),
        marks_pending: 0.0,
        dead_air_pending: 0.0,
        short_scenes,
    };
    cutpass::cut_problems(&status)
}

/// What the check answers: `ok`, or every fault at once joined the way the tool joins them.
pub fn answer(problems: &[String]) -> String {
    if problems.is_empty() {
        return "ok".to_string();
    }
    problems.join("; ")
}

// --- S3: apply, the walk-back ------------------------------------------------------------------------------

/// A hole the model left closes when it is no wider than this AND somebody talked in it.
/// `P.eng.seamMaxSeconds`
pub const SEAM_MAX_SECONDS: f64 = 1.5;

/// Everything the walk-back asks about, handed in rather than measured: whether someone talked in a span,
/// where the silences are, where an edge may snap to, and what the marks take out. Wave data and the
/// aligner stay out of this module so the walk can be run with a hand-written table.
#[derive(Debug, Default, Clone)]
pub struct Walk {
    pub talked: Vec<(f64, f64)>,
    pub silences: Vec<(f64, f64)>,
    pub snap_points: Vec<(f64, Candidate)>,
    pub marks: Vec<(f64, f64)>,
}

/// S3: close the small holes the model left, and only where somebody talked.
///
/// A quiet hole is left open on purpose: closing it would join two scenes across a silence the speaker
/// made, and the seam would read as a jump cut in the middle of a thought.
pub fn close_holes(segs: &[(f64, f64)], talked: &[(f64, f64)], seam_max: f64) -> Vec<(f64, f64)> {
    let mut sorted = coalesce(&[]);
    sorted.extend(coalesced_sorted(segs));
    let mut out: Vec<(f64, f64)> = Vec::new();
    for (from, to) in sorted {
        match out.last_mut() {
            Some(last) => {
                let gap = (last.1, from);
                let small = gap.1 - gap.0 <= seam_max;
                let spoken = talked.iter().any(|t| timeline::spans_overlap(*t, gap));
                if small && spoken {
                    last.1 = to.max(last.1);
                } else {
                    out.push((from, to));
                }
            }
            None => out.push((from, to)),
        }
    }
    out
}

/// `coalesce` without merging, so `close_holes` sees each segment on its own: two segments already
/// touching have no hole between them to decide about, and merging them first would hide that.
fn coalesced_sorted(segs: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut sorted = segs.to_vec();
    sorted.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    sorted
}

/// S3: move one stamped edge onto the best candidate near it.
///
/// The loop is here rather than in [`decisions::choose_edge`] because that helper takes ONE
/// `moves_outward` flag for every candidate, which cannot know whether a given point lies outward from
/// this stamp. Outward means away from the middle of the kept stretch — earlier for a start edge, later
/// for an end edge — and §05 prefers it because it keeps more of what was filmed. No candidate inside
/// the tolerance leaves the stamp exactly where it was.
pub fn snap_edge(candidates: &[(f64, Candidate)], stamp: f64, outward_is_earlier: bool) -> f64 {
    let mut best: Option<(f64, f64)> = None;
    for &(point, kind) in candidates {
        let outward = if outward_is_earlier {
            point < stamp
        } else {
            point > stamp
        };
        let Some(score) = decisions::edge_score(
            kind,
            point,
            stamp,
            decisions::SNAP_TOLERANCE_SECONDS,
            outward,
        ) else {
            continue;
        };
        // A tie keeps the nearer point, which is the smaller move: snapping further than needed for the
        // same weight would look arbitrary on the band.
        let nearer = match best {
            Some((best_point, best_score)) if best_score > score => continue,
            Some((best_point, best_score)) if best_score == score && (best_point - stamp).abs() <= (point - stamp).abs() => {
                continue;
            }
            _ => true,
        };
        let _ = nearer;
        best = Some((point, score));
    }
    best.map(|(point, _)| point).unwrap_or(stamp)
}

/// What the walk-back produced.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Applied {
    pub segs: Vec<(f64, f64)>,
    pub kept_inserts: Vec<Seg>,
    pub effects: Vec<Fx>,
    pub log: Vec<String>,
}

/// S3: the walk-back — close the talked-in holes, snap both edges of every segment, take the marks out,
/// take the dead air out, coalesce, and clamp the effects against what is now kept.
///
/// Hand-placed inserts come back untouched: they were put there by a person, not proposed, so no step of
/// the walk has a claim on them. Effects are replaced as a LIST via [`cut_clamp::clamp_to_cut`], against
/// the cut as it will actually play — F3.12's reason for existing, since every step above moved the
/// seconds an effect was chosen for.
pub fn apply(model_segs: &[(f64, f64)], inserts: &[Seg], fx: &[Fx], walk: &Walk) -> Applied {
    let closed = close_holes(model_segs, &walk.talked, SEAM_MAX_SECONDS);
    let snapped: Vec<(f64, f64)> = closed
        .iter()
        .map(|(from, to)| {
            let start = snap_edge(&walk.snap_points, *from, true);
            let end = snap_edge(&walk.snap_points, *to, false);
            if end > start {
                (start, end)
            } else {
                // A snap that crossed the segment would make negative footage; the stamp wins, because
                // the model's own numbers are the last thing known to be ordered.
                (*from, *to)
            }
        })
        .collect();
    let (unmarked, _) = remove_marks(&snapped, &walk.marks);
    let (kept, _) = remove_dead_air(
        &unmarked,
        &walk.silences,
        DEAD_AIR_MAX_SECONDS,
        DEAD_AIR_KEEP_SECONDS,
    );
    let segs = coalesce(&kept);

    let kept_inserts: Vec<Seg> = inserts.to_vec();

    // Clamp against the cut as applied: the scenes below are the ones this walk just produced.
    let mut probe = Cut::default();
    probe.segs = segs
        .iter()
        .map(|(a, b)| Seg {
            s: *a,
            e: *b,
            ..Default::default()
        })
        .collect();
    probe.fx = fx.to_vec();
    let effects = cut_clamp::clamp_to_cut(&probe).kept;

    let total: f64 = segs.iter().map(|(a, b)| b - a).sum::<f64>()
        + kept_inserts.iter().map(|seg| seg.dur).sum::<f64>();
    let log = vec![
        format!(
            ">>> suggested {} segments, {} total",
            segs.len() + kept_inserts.len(),
            tools::mm_ss(total)
        ),
        format!(
            ">>> \u{2026}and {} effect(s): the speeds, the captions and the decorations",
            effects.len()
        ),
    ];
    Applied {
        segs,
        kept_inserts,
        effects,
        log,
    }
}
