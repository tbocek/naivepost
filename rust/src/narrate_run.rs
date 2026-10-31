//! F4.1 ▶ Write and speak (`spec/07-narrate.md` F4.1) — the run's own decisions, with no window in them.

use crate::cut::NO_CUT_YET;
use crate::narrate_rewrite::{clips_to_write, log_line, why};
use crate::narrate_screen::CAPTIONS;
use crate::narration::{Entry, Narration};

/// S1's fourth refusal, word for word (§F4.1 S1; `spec/inventory/narrate.md` names the missing file as
/// `session.tsv`, whose accessor is [`crate::layout::Tree::session_tsv`]).
pub const NO_SESSION_TIMELINE: &str = "run Transcript first \u{2014} no session timeline";

/// S1: the first thing ▶ says when it will not run, or `None` when every gate is open.
pub fn refuse(busy: bool, has_cut: bool, has_session_timeline: bool) -> Option<String> {
    if busy {
        return Some(crate::run::PAUSING.to_string());
    }
    if !has_cut {
        return Some(NO_CUT_YET.to_string());
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
/// S3: how many jobs one ▶ makes.
pub const JOBS: usize = 2;
/// S3: the bar's words before the first clip closes — the wait has to be named or it reads as hung.
pub const STAGE_THINKING: &str = "thinking about it";

/// S3: the bar once clips start closing, `writing N/M clips` — N written so far, M asked for.
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

/// S6: the summary of the speaking pass.
pub fn spoken_log(spoken: usize, cached: usize) -> String {
    format!("    narrate: {spoken} line(s) spoken, {cached} already in the cache")
}

/// S7: narrated, spoken, ready to hear.
pub const DONE_SPOKEN: &str =
    "narration ready and spoken \u{2014} \u{25b6} the preview hears it in place";
/// S7: captions-only voice, so there was never anything to speak.
pub const DONE_CAPTIONS: &str = "narration ready \u{2014} captions only, nothing spoken";

/// S7: ⏹ ended the run inside `stage`.
pub fn stopped_status(stage: &str) -> String {
    format!("{stage} stopped")
}

/// S7: `stage` failed.
pub fn failed_status(stage: &str) -> String {
    format!("{stage} failed \u{2014} see log")
}

// --- What this ▶ is going to write (S3/S4) ------------------------------------------------------

/// The decision part of a run, made before any call: which clips get a line, and what the log says about
/// why.
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

/// Decide the run.
pub fn plan_run(rewrite: bool, clips: &[(f64, f64)], narration: &Narration, voice: &str) -> Plan {
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
        // all of them.
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
/// line with it off.
fn written_count(rewrite: bool, clips: &[(f64, f64)], narration: &Narration) -> usize {
    clips_to_write(rewrite, clips, narration).len()
}

pub fn lines_off_cut(clips: &[(f64, f64)], narration: &Narration) -> usize {
    narration
        .entries
        .iter()
        .filter(|entry| !clips.iter().any(|(s, e)| *s == entry.s && *e == entry.e))
        .count()
}

/// Whether `voice` is the picker's captions-only row.
pub fn voice_is_captions_only(voice: &str) -> bool {
    voice == CAPTIONS
}

// --- S6: whether one line is spoken ------------------------------------------------------------

/// What the speaking pass does with one line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Speak {
    SkippedBlank,
    SkippedCached,
    Synthesize,
}

/// S6's per-line checkpoint rule: skip blank text or a cached wav, otherwise synthesize.
pub fn speak_line(text: &str, cached_wav_exists: bool) -> Speak {
    if text.trim().is_empty() {
        return Speak::SkippedBlank;
    }
    if cached_wav_exists {
        return Speak::SkippedCached;
    }
    Speak::Synthesize
}

/// Count a speaking pass's outcome for [`spoken_log`]: how many were synthesized, how many were already
/// there.
pub fn tally(outcomes: &[Speak]) -> (usize, usize) {
    outcomes
        .iter()
        .fold((0, 0), |(spoken, cached), outcome| match outcome {
            Speak::Synthesize => (spoken + 1, cached),
            Speak::SkippedCached => (spoken, cached + 1),
            Speak::SkippedBlank => (spoken, cached),
        })
}

// --- S4: folding what the call answered into the record ---------------------------------------

/// One line the F4.2 call answered with.
#[derive(Debug, Clone, PartialEq)]
pub struct Written {
    pub start: f64,
    pub end: f64,
    pub at: f64,
    pub text: String,
    pub emotion: String,
}

/// Fold the written lines into the record and return the result.
pub fn fold_written(record: &Narration, written: &[Written]) -> Narration {
    let mut folded = record.clone();
    for line in written {
        folded
            .entries
            .retain(|entry| !(entry.s == line.start && entry.e == line.end));
        folded
            .silent
            .retain(|sil| !(sil.s == line.start && sil.e == line.end));
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

/// S6: decide every line of the record in one pass.
pub fn speak_pass(record: &[Entry], cached: impl Fn(&Entry) -> bool) -> Vec<Speak> {
    record
        .iter()
        .map(|entry| speak_line(&entry.text, cached(entry)))
        .collect()
}

/// Run the whole F4.1 flow over a prepared plan, with the model and the synth supplied by the caller.
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
    NothingWritten,
    CaptionsOnly { written: usize },
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
        let w = super::Written {
            start: 0.0,
            end: 1.0,
            at: 0.5,
            text: "x".into(),
            emotion: String::new(),
        };
        let folded = super::fold_written(&Narration::default(), &[w]);
        assert_eq!(folded.entries.len(), 1);
        assert_eq!(
            super::tally(&super::speak_pass(&folded.entries, |_| false)),
            (1, 0)
        );
        assert_eq!(
            super::tally(&super::speak_pass(&folded.entries, |_| true)),
            (0, 1)
        );
    }
}
