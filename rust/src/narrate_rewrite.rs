//! P.policy.narrationRewrite — which clips the one narration call covers.
//! `spec/10-parameters.md` row `P.policy.narrationRewrite` (default **off**), read by F4.1 S4
//! ("Write a line for every clip without one, and for every clip when P.policy.narrationRewrite is
//! on"). The spec's own wording of the reason strings lives in `spec/07-narrate.md` F4.1 S3; this
//! module spells them out so the log cannot drift from it.
//!
//! This module owns the *decision* only: given the toggle, the cut's clips and the narration that
//! exists, which clips go into the call, and what the log says about why. Writing the lines is F4.2's
//! call and speaking them is S6's; neither is here, so the choice can be tested without a model.
//!
//! The silent list is never a filter here. A clip marked silent plays its own audio — that marker is the
//! user's decision, not the model's, and it survives whether or not this pass rewrites entries
//! ([`crate::audit_gaps::silent_survives_entry_rewrite`]). Nothing in this module mutates a
//! [`Narration`], so the survival is structural rather than a rule to remember.

use crate::narration::Narration;

/// `P.policy.narrationRewrite`, off: ▶ writes only the clips that have no line. Kept as a named
/// constant so the default is stated where the rule reads it; the project's own default
/// ([`crate::project::Policy::default`]) must agree with it, which `p_policy_narrationrewrite_s1` pins.
pub const REWRITE_DEFAULT: bool = false;

/// The clips this ▶ writes a line for.
///
/// Off (the default): only clips with no line — an existing line is left exactly as it is, which is
/// what makes off the safe default: nothing the user has edited disappears under a rewrite they did not
/// ask for. On: every clip, in order, because the user asked for the whole narration to be redone.
///
/// Order is preserved rather than sorted: the caller passes the cut's clips, already in play order.
pub fn clips_to_write(
    rewrite: bool,
    clips: &[(f64, f64)],
    narration: &Narration,
) -> Vec<(f64, f64)> {
    if rewrite {
        return clips.to_vec();
    }
    clips
        .iter()
        .copied()
        .filter(|(start, end)| !narration.has_line(*start, *end))
        .collect()
}

/// How many clips have no line — the count the ⚠ readout ([`crate::narrate_details::stale_mark`])
/// prints as "N unwritten clips", and what decides whether there is anything to write at all.
pub fn unwritten(clips: &[(f64, f64)], narration: &Narration) -> usize {
    clips
        .iter()
        .filter(|(start, end)| !narration.has_line(*start, *end))
        .count()
}

/// The `<why>` of F4.1 S3's log line, `>>> narrate: <why> — writing N clip(s), one LLM call, then
/// speaking them`. The reader is told why this run is happening, because a rewrite that came from a
/// toggle reads differently from one forced by a moved cut.
///
/// Precedence, decided here: the toggle wins over staleness. Someone who turned rewriting on asked for
/// it, so saying "clip 2 has no narration" would describe a side effect instead of the cause. Past
/// that, a narration with nothing in it is its own reason; lines sitting on video the cut dropped come
/// next, since they explain extra work the run has to undo; and the plain case names the first clip
/// with no line, which is what the user can act on.
pub fn why(
    rewrite: bool,
    clips_without_line: &[usize],
    total_clips: usize,
    entries: usize,
    lines_off_cut: usize,
) -> String {
    if rewrite {
        return "rewriting every line".to_string();
    }
    // Nothing written at all — including a project whose whole narration was deleted — is the head of
    // the flow, not a missing clip: there is no line anywhere for the cut to be behind.
    if entries == 0 || total_clips == 0 {
        return "there is no narration yet".to_string();
    }
    if lines_off_cut > 0 {
        return "the narration has lines for clips the cut no longer has".to_string();
    }
    match clips_without_line.first() {
        Some(index) => format!(
            "clip {} has no narration \u{2014} it is new, or the cut moved under it",
            index + 1
        ),
        // Every clip already has a line and none is off the cut: nothing forces this run, which is
        // exactly why the toggle exists. Said plainly rather than left as an empty reason.
        None => "every clip already has a line".to_string(),
    }
}

/// Which clips have no line, as their indexes into the cut (0-based). [`clips_to_write`] answers the
/// same question in bounds; this is the form the log needs, because it numbers clips for a person.
pub fn indexes_without_line(clips: &[(f64, f64)], narration: &Narration) -> Vec<usize> {
    clips
        .iter()
        .enumerate()
        .filter(|(_, (start, end))| !narration.has_line(*start, *end))
        .map(|(index, _)| index)
        .collect()
}

/// Whether this clip is marked silent — delegated whole to [`Narration::is_silent`] so there is one
/// answer. A silent clip is still written for when it has no line (it needs a caption even when nothing
/// is spoken); what silence guarantees is that no voice is laid on it, and that the marker outlives any
/// entry rewrite ([`crate::audit_gaps::silent_survives_entry_rewrite`]).
pub fn silent_kept(narration: &Narration, start: f64, end: f64) -> bool {
    narration.is_silent(start, end)
}

/// The whole log line, in the shape F4.1 S3 spells: `>>> narrate: <why> — writing N clip(s), one LLM
/// call, then speaking them`. Owns the frame so the wording of `<why>` cannot drift from it.
pub fn log_line(reason: &str, clips: usize) -> String {
    format!(
        ">>> narrate: {reason} \u{2014} writing {clips} clip(s), one LLM call, then speaking them"
    )
}
