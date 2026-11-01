//! F2.14 Suggest a cut (`spec/05-cut.md` F2.14): the whole flow as plain data, guards in, segments and sentences out; no window, file or server.
//! Reads rather than owns: [`crate::tools::cutpass`] (target arithmetic, whole-cut checks), [`crate::cut_effect_decisions`] (dead-air comparison, edge score; this supplies candidates and picks per direction), [`crate::roles`] / [`crate::policy`] (which job runs at all), [`crate::cut_clamp`] (clamps effects against the cut **as applied**, hence after the walk-back).
//! Persisting is the CALLER's job: one `ui::record_edit`, one Undo (F2.13).

use crate::cut::{Cut, Fx, Seg};
use crate::cut_clamp;
use crate::cut_effect_decisions::{self as decisions, Candidate};
pub use crate::cut_suggest_passes::order as passes_switched_on;
use crate::decision_audit;
use crate::policy::{self, Pass};
use crate::project::{MarkingPass, Policy};
use crate::roles::{self, Job};
use crate::timeline;
use crate::tools::{self, cutpass};

// S1: three questions in a fixed order before anything is asked of a model; each refusal is a sentence printed verbatim. `busy` comes from `run::RunBar::running.is_some()` and `hand_edits` from `hand_edit`'s mtime answer, read by the caller and not recomputed here.

/// DECISION (the spec says only "refused"): a busy bar gets its own sentence saying what to do.
pub const BUSY: &str = "a run is already going \u{2014} stop it before asking for a new cut";
pub const HAND_EDITS: &str =
    "you have hand edits \u{2014} press Revert first for a fresh suggestion";
pub const NO_TIMELINE: &str =
    "run Describe first \u{2014} the suggestion reads the session timeline, and there is none";
/// S2b, after the last correction round. `P.eng.llmAttempts`
pub const NO_VALID_CUT: &str = "no valid cut after 3 attempts";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refuse {
    Busy,
    HandEdits,
    NoTimeline,
}

impl Refuse {
    pub fn said(&self) -> &'static str {
        match self {
            Refuse::Busy => BUSY,
            Refuse::HandEdits => HAND_EDITS,
            Refuse::NoTimeline => NO_TIMELINE,
        }
    }
}

/// S1: the first "no" wins in the flowchart's order busy → hand edits → timeline. A run under way may be about to write the timeline whose absence would be reported.
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

/// A silence longer than this inside a kept clip is cut out. `P.policy.deadAirMaxSeconds`
pub const DEAD_AIR_MAX_SECONDS: f64 = 8.0;
/// The beat left where dead air was taken out. `P.policy.deadAirKeepSeconds`
pub const DEAD_AIR_KEEP_SECONDS: f64 = 0.5;
/// How far past the first and last spoken word the trimmed span may reach, placed by the sound (S2a "within 0.4 s"). §6 gives no `P.` id: `suggest.trimReachSeconds`, a bare prefix like `machine.jpegQuality`.
pub const TRIM_REACH_SECONDS: f64 = 0.4;

/// One filmed run with the seconds that bookend its speech: the recorded span, and where the first word starts and the last ENDS.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SpokenRun {
    pub span: (f64, f64),
    pub first_spoken: f64,
    pub last_spoken: f64,
    /// The two edges as the sound placed them within [`TRIM_REACH_SECONDS`] (F1.11 `start_before` / `end_after`); `None` without an envelope, then the word pad stands in.
    pub placed: Option<(f64, f64)>,
    /// Every word of the run on the session clock, read by [`drop_wordless`].
    pub words: Vec<(f64, f64)>,
}

/// S2a: every filmed run trimmed to its words, placed by the sound, never outside the run (a reach across the run's end would count the next recording's seconds twice).
pub fn trim_run_to_words(run: &SpokenRun) -> (f64, f64) {
    let (start, end) = run.placed.unwrap_or((
        run.first_spoken - crate::edges::WORD_PAD,
        run.last_spoken + crate::edges::WORD_PAD,
    ));
    let start = start.max(run.span.0);
    (start, end.min(run.span.1).max(start))
}

/// S2a: a kept piece with no spoken word is not kept. A mark taking a recording's last words leaves a scrap behind them (08-dex: 11 of 15 segments under a second held no word). A word counts by the middle of what its run shows; runs that know no words keep everything.
pub fn drop_wordless(segs: &[(f64, f64)], runs: &[SpokenRun]) -> Vec<(f64, f64)> {
    let middles: Vec<f64> = runs
        .iter()
        .flat_map(|run| {
            run.words
                .iter()
                .map(|(from, to)| (from.max(run.span.0) + to.min(run.span.1)) / 2.0)
        })
        .collect();
    if middles.is_empty() {
        return segs.to_vec();
    }
    segs.iter()
        .filter(|(from, to)| middles.iter().any(|at| at >= from && at < to))
        .copied()
        .collect()
}

/// Subtract every mark from the kept spans; returns the pieces and how many marks took seconds out (a mark over footage already gone is not counted).
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

/// Take the dead air out of the kept spans: only a silence strictly longer than `max` goes (exactly `max` is a deliberate beat the brief lets the model keep), and `keep` of the beat survives at the front. Returns the spans and the seconds removed; a silence partly outside costs only the part inside.
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

pub fn coalesce(segs: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut sorted = segs.to_vec();
    sorted.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let mut out: Vec<(f64, f64)> = Vec::new();
    for (from, to) in sorted {
        match out.last_mut() {
            Some(last) if from <= last.1 => last.1 = last.1.max(to),
            _ => out.push((from, to)),
        }
    }
    out
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct WordsCut {
    pub segs: Vec<(f64, f64)>,
    pub marks_out: usize,
    pub silence_out: f64,
    pub status: String,
    pub log: String,
}

/// S2a: the whole words path: trim every run to its words, take the marks out, take the dead air out, coalesce. No model, server or cache.
pub fn words_cut(runs: &[SpokenRun], marks: &[(f64, f64)], silences: &[(f64, f64)]) -> WordsCut {
    let trimmed: Vec<(f64, f64)> = runs.iter().map(trim_run_to_words).collect();
    let (unmarked, marks_out) = remove_marks(&trimmed, marks);
    let (kept, silence_out) = remove_dead_air(
        &unmarked,
        silences,
        DEAD_AIR_MAX_SECONDS,
        DEAD_AIR_KEEP_SECONDS,
    );
    let segs = coalesce(&drop_wordless(&kept, runs));
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

/// Whether this cut aims at a length at all. `P.policy.targetLengthSeconds`; 0 means none.
pub fn has_target(target: f64) -> bool {
    target > cutpass::NO_TARGET
}

/// The target block of the cut message in §S2b's two forms (length or none); "no target" must not imply a ceiling.
pub fn target_block(target: f64) -> String {
    if !has_target(target) {
        return "NO TARGET LENGTH: keep what is worth keeping.".to_string();
    }
    let (low, high) = cutpass::footage_window(target);
    format!(
        "KEEP between {} and {} seconds of footage, in at most {} segments.",
        tools::mm_ss(low),
        tools::mm_ss(high),
        cutpass::max_segments(target)
    )
}

/// The dead-air line of the cut message: §12 entry 13 says the number must be STATED, so the model can keep a deliberate pause by saying so in its WHY. The rule stays in [`remove_dead_air`]. `max` is `P.policy.deadAirMaxSeconds`; no policy field exists (§10; home `suggest::DEAD_AIR_MAX_SECONDS`), so the caller passes the const.
pub fn dead_air_block(max: f64) -> String {
    format!(
        "SILENCES longer than {} seconds inside a kept clip are cut out afterwards, leaving a short \
         beat: if a longer pause is deliberate, say so in your WHY and it stays.",
        max as i64
    )
}

/// The cut request's own MATERIAL: session length, finished length, surviving silence, then the timeline. §8's User Context block and speech rule come first via [`crate::prompt_assembly::user_message`], so this takes no context. §9: with one camera and one recording this is the whole message; richer cuts use `suggest::message_over`.
pub fn message(session_len: f64, target: f64, dead_air_max: f64, session_txt: &str) -> String {
    message_over(session_len, target, dead_air_max, session_txt, &[], &[])
}

/// §9's last paragraph at the cut pass's door: the material with camera rows and heard lanes named above the length, target and dead-air lines ([`crate::effect_overlap::brief_lines`]). A single-camera session gets the byte-identical message. `rows` are the page's **1-based** camera numbers (`camera 1, camera 2`), counted by `crate::cut_cam::row_count`.
pub fn message_over(
    session_len: f64,
    target: f64,
    dead_air_max: f64,
    session_txt: &str,
    rows: &[String],
    lanes: &[String],
) -> String {
    let named: Vec<String> = crate::effect_overlap::brief_lines(rows, lanes)
        .into_iter()
        .collect();
    let naming = if named.is_empty() {
        String::new()
    } else {
        format!("{}\n\n", named.join("\n"))
    };
    format!(
        "{naming}SESSION LENGTH: {} seconds\n\n{}\n\n{}\n\nSESSION TIMELINE:\n{}",
        session_len as i64,
        target_block(target),
        dead_air_block(dead_air_max),
        session_txt
    )
}

/// Whether the model cut is asked for at all; under `CutMode::Words` no call is made.
pub fn model_cut_offered(policy_: &Policy, marking_pass: MarkingPass) -> bool {
    roles::job_applies(Job::ModelCut, marking_pass, policy::cut_mode_of(policy_))
}

/// The whole-cut checks as a problem list, worst first ([`cutpass::cut_problems`]'s order; the tool phrases the complaint).
pub fn whole_cut_problems(segs: &[(f64, f64)], target: f64) -> Vec<String> {
    let footage: f64 = segs.iter().map(|(a, b)| b - a).sum();
    let short_scenes: Vec<(f64, f64)> = segs
        .iter()
        .filter(|(a, b)| b - a < cutpass::MIN_SCENE_SECONDS)
        .map(|(a, b)| (*a, b - a))
        .collect();
    let status = cutpass::Status {
        footage,
        window: cutpass::footage_window(target),
        segments: segs.len(),
        min_segments: cutpass::min_segments(target),
        max_segments: cutpass::max_segments(target),
        marks_pending: 0.0,
        dead_air_pending: 0.0,
        short_scenes,
    };
    cutpass::cut_problems(&status)
}

pub fn answer(problems: &[String]) -> String {
    if problems.is_empty() {
        return "ok".to_string();
    }
    problems.join("; ")
}

/// A hole the model left closes when no wider than this AND somebody talked in it. `P.eng.seamMaxSeconds`
pub const SEAM_MAX_SECONDS: f64 = 1.5;

/// Everything the walk-back asks, handed in: whether someone talked in a span, silences, snap candidates, and what the marks take out, so tests use a hand-written table.
#[derive(Debug, Default, Clone)]
pub struct Walk {
    pub talked: Vec<(f64, f64)>,
    pub silences: Vec<(f64, f64)>,
    pub snap_points: Vec<(f64, Candidate)>,
    pub marks: Vec<(f64, f64)>,
}

/// `P.eng.talkPadSeconds` (spec/10-parameters.md): the prototype's `talking()` (`go/cut.go`) widens every speech span by this on both sides, as a cut a fifth of a second from a word is in the word. Used by [`hole_has_speech`]; not [`crate::align::PAD`], which is a different rule and number.
pub const TALK_PAD_SECONDS: f64 = 0.2;

/// S3: does anybody talk inside this gap? Both spans are widened by [`TALK_PAD_SECONDS`] first.
pub fn hole_has_speech(gap: (f64, f64), talked: &[(f64, f64)]) -> bool {
    talked.iter().any(|talked| {
        let padded = (talked.0 - TALK_PAD_SECONDS, talked.1 + TALK_PAD_SECONDS);
        timeline::spans_overlap(padded, gap)
    })
}

/// S3: close the small holes the model left, only where somebody talked; a quiet hole stays open (closing it would join scenes across a silence the speaker made).
pub fn close_holes(segs: &[(f64, f64)], talked: &[(f64, f64)], seam_max: f64) -> Vec<(f64, f64)> {
    let mut sorted = coalesce(&[]);
    sorted.extend(coalesced_sorted(segs));
    let mut out: Vec<(f64, f64)> = Vec::new();
    for (from, to) in sorted {
        match out.last_mut() {
            Some(last) => {
                let gap = (last.1, from);
                let small = gap.1 - gap.0 <= seam_max;
                let spoken = hole_has_speech(gap, talked);
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

/// `coalesce` without merging, so touching segments keep their own identity for `close_holes`.
fn coalesced_sorted(segs: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut sorted = segs.to_vec();
    sorted.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    sorted
}

/// S3: move one stamped edge onto the best candidate near it. The loop is here because [`decisions::choose_edge`] takes ONE `moves_outward` flag for all candidates. Outward (earlier for a start, later for an end) is preferred (§05) as it keeps more footage. No candidate inside the tolerance leaves the stamp.
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
        // A tie keeps the nearer point, the smaller move.
        let nearer = match best {
            Some((_best_point, best_score)) if best_score > score => continue,
            Some((best_point, best_score))
                if best_score == score && (best_point - stamp).abs() <= (point - stamp).abs() =>
            {
                continue;
            }
            _ => true,
        };
        let _ = nearer;
        best = Some((point, score));
    }
    best.map(|(point, _)| point).unwrap_or(stamp)
}

/// §S3's second line: `">>> …and N effect(s): the speeds, the captions and the decorations"`, printed by [`apply`] and [`crate::cut_suggest_passes`].
pub fn effects_line(count: usize) -> String {
    format!(">>> \u{2026}and {count} effect(s): the speeds, the captions and the decorations")
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Applied {
    pub segs: Vec<(f64, f64)>,
    pub kept_inserts: Vec<Seg>,
    pub effects: Vec<Fx>,
    pub log: Vec<String>,
}

/// The walk-back's one-sentence account (§S3, also on the status row via `cut_suggest_land`); [`walk_back_audit`] measures it.
fn walk_account(segs: usize, inserts: usize, total: f64) -> String {
    format!(
        "suggested {} segments, {} total",
        segs + inserts,
        tools::mm_ss(total)
    )
}

/// Whether this [`Walk`] may happen unseen (§1 fourth row). **Mechanical**: edge moves are bounded by [`decisions::SNAP_TOLERANCE_SECONDS`] (`P.policy.snapToleranceSeconds`) via [`snap_edge`]. **Explainable in one sentence**: [`walk_account`]. **Visible afterwards**: the materialisation (word edges, silence midpoints or marks) must be present. Not fed back to the model ([`decision_audit::Home::WalkBack`]).
pub fn walk_back_audit(walk: &Walk) -> Result<&'static str, String> {
    // An edge travels no further than the tolerance to reach a word edge.
    let mechanical = decisions::SNAP_TOLERANCE_SECONDS > 0.0;
    // The sentence exists: `apply`'s first log line is built unconditionally.
    let explainable = !walk_account(0, 0, 0.0).is_empty();
    // Nothing to snap to and nothing to remove: nothing machine-placed, so not visible afterwards.
    let visible = !walk.snap_points.is_empty() || !walk.marks.is_empty();
    if decision_audit::walk_back_admits(mechanical, explainable, visible) {
        return Ok(decision_audit::Home::WalkBack.label());
    }
    let mut failed: Vec<&str> = Vec::new();
    if !mechanical {
        failed.push("not bounded by the snap tolerance");
    }
    if !explainable {
        failed.push("has no one-sentence account of itself");
    }
    if !visible {
        failed.push("leaves nothing on screen afterwards");
    }
    Err(format!(
        "the walk-back cannot run: {} \u{2014} a change like that belongs at `finish`, where the model hears about it",
        failed.join(", ")
    ))
}

/// S3: the walk-back: close talked-in holes, snap both edges, take marks out, take dead air out, coalesce, clamp effects against what is kept. Hand-placed inserts come back untouched. Effects are replaced as a LIST via [`cut_clamp::clamp_to_cut`] (F3.12). §1's rule is enforced at this door by [`walk_back_audit`].
/// The allow is for the clamp probe inside: clippy points at the `let` above its suggestion span, so only the whole function can hold it.
#[allow(clippy::field_reassign_with_default)]
pub fn apply(model_segs: &[(f64, f64)], inserts: &[Seg], fx: &[Fx], walk: &Walk) -> Applied {
    // Measured before anything moves; refused, NOTHING is placed, so no half-walked cut lands.
    if let Err(why) = walk_back_audit(walk) {
        return Applied {
            segs: Vec::new(),
            kept_inserts: Vec::new(),
            effects: Vec::new(),
            log: vec![format!("!!! {why}")],
        };
    }
    let walked = walk_back_spans(model_segs, walk);
    let closed_len = walked.closed_len;
    let moved = walked.moved;
    let segs = walked.segs;
    let kept_inserts: Vec<Seg> = inserts.to_vec();

    // Clamp against the cut as applied (the scenes this walk just produced).
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
    let clamped = cut_clamp::clamp_to_cut(&probe);
    let effects = clamped.kept;

    let total: f64 = segs.iter().map(|(a, b)| b - a).sum::<f64>()
        + kept_inserts.iter().map(|seg| seg.dur).sum::<f64>();
    let mut log = vec![
        format!(
            ">>> {}",
            walk_account(segs.len(), kept_inserts.len(), total)
        ),
        walk_report(closed_len, moved),
        effects_line(effects.len()),
    ];
    log.extend(clamped.dropped_lines.iter().cloned());
    Applied {
        segs,
        kept_inserts,
        effects,
        log,
    }
}

/// What [`walk_back_spans`] counted, so [`apply`] and words-path callers can word it without redoing the arithmetic.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Walked {
    pub segs: Vec<(f64, f64)>,
    pub closed_len: usize,
    pub moved: usize,
    pub marks_out: usize,
    pub silence_out: f64,
}

/// §12 §4's fourth bullet as one function: hole closing, edge snapping, mark removal, dead-air removal, coalescing, in that order. Shared by [`apply`] (adds the audit gate and effect clamp) and [`crate::cut_suggest_words_run::run_words`], so words-mode replies get the same walk and [`walk_report`]. Edge moves are bounded ([`decisions::SNAP_TOLERANCE_SECONDS`]), holes close only where [`hole_has_speech`], a snap crossing its own segment keeps the stamp.
pub fn walk_back_spans(segs: &[(f64, f64)], walk: &Walk) -> Walked {
    let closed = close_holes(segs, &walk.talked, SEAM_MAX_SECONDS);
    let snapped: Vec<(f64, f64)> = closed
        .iter()
        .map(|(from, to)| {
            let start = snap_edge(&walk.snap_points, *from, true);
            let end = snap_edge(&walk.snap_points, *to, false);
            if end > start {
                (start, end)
            } else {
                // A snap crossing the segment would make negative footage; the stamp wins.
                (*from, *to)
            }
        })
        .collect();
    // One count per step: an edge landing where it started is not a move; a mark over gone footage is not a removal.
    let moved = snapped
        .iter()
        .zip(closed.iter())
        .filter(|((start, end), (from, to))| start != from || end != to)
        .count();
    let (unmarked, marks_out) = remove_marks(&snapped, &walk.marks);
    let (kept, silence_out) = remove_dead_air(
        &unmarked,
        &walk.silences,
        DEAD_AIR_MAX_SECONDS,
        DEAD_AIR_KEEP_SECONDS,
    );
    Walked {
        segs: coalesce(&kept),
        closed_len: closed.len(),
        moved,
        marks_out,
        silence_out,
    }
}

/// §4 "the page shows what it did": holes joined, edges moved, what was taken out; empty when nothing changed.
pub fn walk_report(closed_len: usize, moved: usize) -> String {
    format!("walk-back: {closed_len} stretch(es) after closing holes, {moved} edge(s) moved")
}
