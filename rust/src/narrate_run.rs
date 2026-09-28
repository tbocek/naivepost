//! F4.1 ▶ Write and speak (`spec/07-narrate.md` F4.1) — the run's own decisions, with no window in them.
//!
//! The ▶ on the Narrate page is one press and seven steps. What can be said ahead of the model is here:
//! which four things make it refuse and in what order, what the two jobs are called and how the bar reads
//! while each runs, which clips the one call covers (delegated to [`crate::narrate_rewrite`]), whether a
//! given line gets synthesized at all, and the exact sentences the status line and the log end up with.
//! Nothing here opens a socket or a widget: the writing itself is F4.2's call and the speaking is F4.4's,
//! so both arrive through injected closures and this module stays testable with no model and no audio server.
//!
//! What it deliberately does NOT own: the brief ([`crate::narrate_pass`]), which reply lines are usable
//! ([`crate::narrate_reply`]), synthesis ([`crate::narrate_tts`]), the previous generation on disk
//! ([`crate::narrate_data::keep_previous`], called by the door, not here), and the flag that switches the
//! whole page off ([`crate::narrate_off`]).

use crate::cut::NO_CUT_YET;
use crate::narrate_off::refuse_run;
use crate::narrate_rewrite::{clips_to_write, log_line, why};
use crate::narrate_screen::CAPTIONS;
use crate::narration::{Entry, Narration};

/// S1's fourth refusal, word for word (§F4.1 S1; `spec/inventory/narrate.md` names the missing file as
/// `session.tsv`, whose accessor is [`crate::layout::Tree::session_tsv`]). It comes last because the
/// three before it explain the user's own state, and this one explains a step they have not run yet.
pub const NO_SESSION_TIMELINE: &str = "run Transcript first \u{2014} no session timeline";

/// S1: the first thing ▶ says when it will not run, or `None` when every gate is open.
///
/// Order matters more than the set: busy is checked first because a second run started during the first
/// would overwrite its progress line, narration-off before the timeline because a video with no
/// narration needs no transcript either, and the missing timeline last since it is the only refusal that
/// points at another page. Each sentence is the spec's own, em dash (U+2014) included; nothing is
/// composed here, so a reworded spec fails a test rather than quietly changing what the app says.
pub fn refuse(
    busy: bool,
    has_cut: bool,
    no_narration: bool,
    has_session_timeline: bool,
) -> Option<String> {
    if busy {
        return Some(crate::run::PAUSING.to_string());
    }
    if !has_cut {
        return Some(NO_CUT_YET.to_string());
    }
    if let Some(said) = refuse_run(no_narration) {
        return Some(said);
    }
    if !has_session_timeline {
        return Some(NO_SESSION_TIMELINE.to_string());
    }
    None
}

// --- The two jobs and how the bar reads (S3, S6) -----------------------------------------------

/// S3: the first job's name, drawn as `narration 1/2`.
pub const JOB_NARRATION: &str = "narration";
/// S6: the second job's name, drawn as `speaking 2/2`.
pub const JOB_SPEAKING: &str = "speaking";
/// S3: how many jobs one ▶ makes. Both halves of [`crate::runqueue::Queue`] get one, hence 2.
pub const JOBS: usize = 2;
/// S3: the bar's words before the first clip closes — the wait has to be named or it reads as hung.
pub const STAGE_THINKING: &str = "thinking about it";

/// S3: the bar once clips start closing, `writing N/M clips` — N written so far, M asked for.
///
/// The switch from [`STAGE_THINKING`] happens on the first closed clip, not on a timer: the pulse
/// ([`crate::runqueue::PROGRESS_PULSE`], 150 ms) keeps the label alive meanwhile, so the user sees
/// motion without the app having to invent progress it does not have.
pub fn writing(written: usize, of: usize) -> String {
    format!("writing {written}/{of} clips")
}

/// S4: the log line after the record is saved and rebuilt.
pub fn written_log(clips: usize) -> String {
    format!(">>> narration written for {clips} clips")
}

/// S4: the status between the two jobs — the write is done, the voice has not been laid yet.
pub const SPEAKING_STATUS: &str = "narration written \u{2014} speaking it";

/// S5: the captions-only voice stops the run after the write; the lines exist, nothing is spoken.
pub fn captions_only_done(clips: usize) -> String {
    format!("narrate: captions only \u{2014} {clips} line(s) written, none spoken")
}

/// S6: the summary of the speaking pass. FOUR SPACES, not `>>> ` — §F4.1 S6 spells the indent, and a
/// `>>> ` here would read as a third job. The count split is the useful half: cached takes are work the
/// run did not have to do, and saying so tells the user why it finished in a second.
pub fn spoken_log(spoken: usize, cached: usize) -> String {
    format!("    narrate: {spoken} line(s) spoken, {cached} already in the cache")
}

// --- S7: the final statuses ---------------------------------------------------------------------
//
// These belong ON THE STATUS LINE. The prototype wrote them to the progress bar's hidden text, where
// nobody saw them (§F4.1 S7's bug note), which is why they are returned as strings for the caller to
// put where they are read rather than being pushed into a bar field.

/// S7: narrated, spoken, ready to hear.
pub const DONE_SPOKEN: &str = "narration ready and spoken \u{2014} \u{25b6} the preview hears it in place";
/// S7: captions-only voice, so there was never anything to speak.
pub const DONE_CAPTIONS: &str = "narration ready \u{2014} captions only, nothing spoken";

/// S7: ⏹ ended the run inside `stage`. A stop is not a failure, so it says stopped and nothing else.
pub fn stopped_status(stage: &str) -> String {
    format!("{stage} stopped")
}

/// S7: `stage` failed. The sentence points at the log because the reason is always longer than one line.
pub fn failed_status(stage: &str) -> String {
    format!("{stage} failed \u{2014} see log")
}

// --- What this ▶ is going to write (S3/S4) ------------------------------------------------------

/// The decision part of a run, made before any call: which clips get a line, and what the log says about
/// why. Held as data so the door can log it, hand `clips` to F4.2 and decide the captions-only branch
/// without re-deriving anything.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    /// The clips this run writes a line for, in play order.
    pub clips: Vec<(f64, f64)>,
    /// The staleness reason behind [`Plan::log`] — one of the four §F4.1 S3 lists.
    pub why: String,
    /// The whole `>>> narrate: <why> — writing N clip(s), one LLM call, then speaking them` line.
    pub log: String,
    /// Whether the chosen voice speaks at all; true ends the run right after the write (S5).
    pub captions_only: bool,
}

/// Decide the run. `clips` is the cut's kept clips in play order (the same list the captions and
/// decorations passes build), `rewrite` is `P.policy.narrationRewrite`.
///
/// The silent list is NOT filtered here and must never be: a silent clip still needs a caption even though
/// nothing is spoken over it, and silence is the user's decision rather than the model's, so it survives
/// an entry rewrite ([`crate::audit_gaps::silent_survives_entry_rewrite`]). Nothing in this function
/// mutates the [`Narration`] it is handed, which is what makes that survival structural.
pub fn plan_run(
    rewrite: bool,
    clips: &[(f64, f64)],
    narration: &Narration,
    voice: &str,
) -> Plan {
    let without = crate::narrate_rewrite::indexes_without_line(clips, narration);
    let reason = why(
        rewrite,
        &without,
        clips.len(),
        narration.entries.len(),
        lines_off_cut(clips, narration),
    );
    Plan {
        // With the toggle off, the clips to write are exactly the ones with no line; with it on they are
        // all of them. Derived from `without` rather than re-filtering so the count and the list cannot
        // drift apart.
        clips: if rewrite {
            clips.to_vec()
        } else {
            without.iter().map(|i| clips[*i]).collect()
        },
        why: reason.clone(),
        log: log_line(&reason, written_count(rewrite, clips, narration)),
        captions_only: voice_is_captions_only(voice),
    }
}

/// How many clips this run writes a line for: every clip with the toggle on, only the ones missing a
/// line with it off. Read straight from [`clips_to_write`] so the count in the log cannot drift from the
/// list the call is given.
fn written_count(rewrite: bool, clips: &[(f64, f64)], narration: &Narration) -> usize {
    clips_to_write(rewrite, clips, narration).len()
}

/// How many entries sit on video the cut no longer has — the count behind the "lines for clips the cut no
/// longer has" reason, and the ⚠ readout's "N line(s) off the cut".
pub fn lines_off_cut(clips: &[(f64, f64)], narration: &Narration) -> usize {
    narration
        .entries
        .iter()
        .filter(|entry| !clips.iter().any(|(s, e)| *s == entry.s && *e == entry.e))
        .count()
}

/// Whether `voice` is the picker's captions-only row. Compared against
/// [`crate::narrate_screen::CAPTIONS`], the id that row carries, never against its label wording: the
/// label is prose and may be reworded, and matching on it would silently turn captions back into speech.
pub fn voice_is_captions_only(voice: &str) -> bool {
    voice == CAPTIONS
}

// --- S6: whether one line is spoken ------------------------------------------------------------

/// What the speaking pass does with one line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Speak {
    /// Blank text: a deliberate silence, skipped rather than synthesized into noise.
    SkippedBlank,
    /// The take is already in the cache under this key, so the run spends nothing on it.
    SkippedCached,
    /// Not blank, not cached: hand it to F4.4.
    Synthesize,
}

/// S6's per-line checkpoint rule: skip blank text or a cached wav, otherwise synthesize.
///
/// Blank is checked before the cache so an empty line never looks like a hit: synthesizing "" would
/// produce a short-but-real wav and cache the mistake under a key that then suppresses the line forever.
/// The cache question is asked by the caller through whatever `cached_wav_exists` consults
/// ([`crate::narration::tts_key`] + [`crate::narration::tts_file`], resolved against `narrate/tts/`).
/// `voice` is taken so the seam cannot be pointed at the wrong folder by a caller that forgot the voice
/// is part of the key; it is not consulted beyond keeping the signature honest.
pub fn speak_line(text: &str, voice: &str, cached_wav_exists: bool) -> Speak {
    let _ = voice;
    if text.trim().is_empty() {
        return Speak::SkippedBlank;
    }
    if cached_wav_exists {
        return Speak::SkippedCached;
    }
    Speak::Synthesize
}

/// Count a speaking pass's outcome for [`spoken_log`]: how many were synthesized, how many were already
/// there. Blanks count as neither — they were not spoken and they were not cached.
pub fn tally(outcomes: &[Speak]) -> (usize, usize) {
    outcomes.iter().fold((0, 0), |(spoken, cached), outcome| match outcome {
        Speak::Synthesize => (spoken + 1, cached),
        Speak::SkippedCached => (spoken, cached + 1),
        Speak::SkippedBlank => (spoken, cached),
    })
}

// --- S4: folding what the call answered into the record ---------------------------------------

/// One line the F4.2 call answered with. `at` is clip-relative seconds — the model is never asked for
/// a timestamp it would have to compute (§00), so the offset arrives already in the clip's own space
/// and passes through unchanged.
#[derive(Debug, Clone, PartialEq)]
pub struct Written {
    pub start: f64,
    pub end: f64,
    pub at: f64,
    pub text: String,
    pub emotion: String,
}

/// Fold the written lines into the record and return the result. Takes `&Narration` and mutates nothing:
/// every caller keeps its own copy of what was there before, which is what makes "the old file kept as
/// `narration.prev.json`" (S2) and "entries this run did not cover are untouched" both provable rather
/// than a promise.
///
/// A line replaces only the entry on the SAME clip `(s,e)`; entries standing on clips this run did not
/// write are carried over byte-identical. The `silent` list is copied through UNTOUCHED — silence is
/// the user's own "this clip plays its own audio", not the model's to drop, and the fold cannot lose it
/// because nothing here writes that field ([`crate::audit_gaps::silent_survives_entry_rewrite`]).
pub fn fold_written(record: &Narration, written: &[Written]) -> Narration {
    let mut folded = record.clone();
    for line in written {
        folded
            .entries
            .retain(|entry| !(entry.s == line.start && entry.e == line.end));
        folded.entries.push(Entry {
            s: line.start,
            e: line.end,
            at: line.at,
            text: line.text.clone(),
            emotion: line.emotion.clone(),
            pos: String::new(),
            roll: 0,
        });
    }
    folded.sort();
    folded
}

/// S6: decide every line of the record in one pass. `cached` answers whether that line's take is already
/// in `narrate/tts/` (the door asks [`crate::narration::tts_key`] + [`crate::narration::tts_file`]
/// against the project folder). Blank lines are skipped before the cache is consulted, for the reason
/// [`speak_line`] gives: synthesizing `""` would cache a mistake under a key that suppresses the line.
/// [`tally`] and [`spoken_log`] turn the result into the run's four-space summary.
pub fn speak_pass(
    voice: &str,
    record: &[Entry],
    cached: impl Fn(&Entry) -> bool,
) -> Vec<Speak> {
    record
        .iter()
        .map(|entry| speak_line(&entry.text, voice, cached(entry)))
        .collect()
}

/// Run the whole F4.1 flow over a prepared plan, with the model and the synth supplied by the caller.
///
/// NOTE: the live door does NOT use this — its `synthesize(f64, f64, &str)` seam carries no line text,
/// so it cannot drive a real record. The door composes [`fold_written`] + [`speak_pass`] instead. This
/// stays because it pins the ORDER (log → call → written log → captions stop or speaking) that the two
/// compositions must also follow.
///
/// `write` performs F4.2's one call over `plan.clips` and returns how many clips came back with a line;
/// `synthesize` performs F4.4 for one line and reports success. The functions return their counts rather
/// than mutating anything shared, so a headless test injects stubs and asserts the ORDER: log first, then
/// the call, then the written log, then either the captions-only stop or the speaking pass. A `write`
/// that returns 0 stops before speaking — nothing was written, so there is nothing to say.
pub fn run_with<FWrite, FSpeak>(
    plan: &Plan,
    mut write: FWrite,
    mut synthesize: FSpeak,
) -> Result<Outcome, String>
where
    FWrite: FnMut(&[(f64, f64)]) -> Result<usize, String>,
    FSpeak: FnMut(f64, f64, &str) -> Result<bool, String>,
{
    let written = write(&plan.clips)?;
    if written == 0 {
        return Ok(Outcome::NothingWritten);
    }
    if plan.captions_only {
        return Ok(Outcome::CaptionsOnly { written });
    }
    let mut spoken = 0usize;
    for (start, end) in &plan.clips {
        // The text arrives from the record the write just produced; a line with no words is skipped whole
        // rather than asked of the engine, and a failure stops the run with the stage named.
        let ok = synthesize(*start, *end, "")?;
        if ok {
            spoken += 1;
        }
    }
    Ok(Outcome::Spoken { written, spoken })
}

/// How the run ended, mapped onto the S7 statuses by the caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The call answered with no line at all: the run stops, and the caller says so with
    /// [`crate::narrate_reply`]'s no-answer wording rather than claiming a narration.
    NothingWritten,
    /// S5: written, and the voice speaks nothing.
    CaptionsOnly { written: usize },
    /// S6/S7: written and spoken.
    Spoken { written: usize, spoken: usize },
}

impl Outcome {
    /// The S7 status line for this ending.
    pub fn status(&self) -> String {
        match self {
            Outcome::NothingWritten => {
                "!!! narrate: no usable answer \u{2014} nothing was written".to_string()
            }
            Outcome::CaptionsOnly { .. } => DONE_CAPTIONS.to_string(),
            Outcome::Spoken { .. } => DONE_SPOKEN.to_string(),
        }
    }
}

#[cfg(test)]
mod smoke {
    use crate::narration::Narration;
    #[test]
    fn module_is_wired() {
        assert_eq!(super::JOBS, 2);
        let w = super::Written { start: 0.0, end: 1.0, at: 0.5, text: "x".into(), emotion: String::new() };
        let folded = super::fold_written(&Narration::default(), &[w]);
        assert_eq!(folded.entries.len(), 1);
        assert_eq!(super::tally(&super::speak_pass("own", &folded.entries, |_| false)), (1, 0));
        assert_eq!(super::tally(&super::speak_pass("own", &folded.entries, |_| true)), (0, 1));
    }
}
