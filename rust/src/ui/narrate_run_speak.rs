//! F4.1 S6 — the speaking half of ▶ Write and speak, over the real wire.
//!
//! [`crate::narrate_run`] decides WHICH lines a run speaks (blank is a deliberate silence, a take
//! already in `narrate/tts/` costs nothing) but dials no socket: that split is what makes its rules
//! testable with no server. This module is the other half — it takes those decisions and actually
//! asks audio.cpp for each take, through [`crate::speak_leg::speak`], the same door the row's ▶
//! (`narrate_page::press_line_speak`) and the preview's walk (`narrate_preview_leg`) already use.
//! Without it a run could report "N line(s) spoken" having spent nothing, which is exactly what S6
//! must not be able to say.
//!
//! The per-line leg arrives as a closure the caller owns, the convention this crate uses wherever a
//! network leg is involved ([`crate::narrate_tts::speak_line`], [`crate::produce_translate::Ask`]):
//! the program calls [`speak_lines`] handing it [`dial_line`] and gets the real wire; a test pointed
//! at a loopback fake gets the same code path with an address it controls. Nothing here
//! draws a widget or starts a thread.

use crate::layout::Tree;
use crate::narrate_run::{self, Speak};
use crate::narration::{self, Entry};
use crate::narrate_tts;

/// What a speaking pass ended as: every line's verdict, the two counts the run logs, and whether ⏹ cut
/// the pass short — which is what makes §S7's `<stage> stopped` reachable from the speaking half.
#[derive(Debug, Clone, PartialEq)]
pub struct SpeakRun {
    pub lines: Vec<Line>,
    pub spoken: usize,
    pub cached: usize,
    /// A stop was raised before this line, so the pass ended early and the run says "speaking stopped".
    pub stopped: bool,
}

/// One line's verdict, so a refusal can be said with the step and reason the leg already named.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Line {
    /// A take arrived: the file it landed in, which is also the proof the server was asked.
    Spoken(std::path::PathBuf),
    /// The line's take was already on disk — the run spends nothing and says so as cached.
    Cached,
    /// A deliberate silence (no words): neither spoken nor cached, so `[`tally`]` counts neither.
    Blank,
    /// The engine refused. The sentence names the model and the reason (spec/00-principles).
    Refused(String),
}

/// Speak every line of a written narration and report the run's two counts.
///
/// `cached` answers whether one entry's take is already in `narrate/tts/`; the door passes the real
/// file check ([`crate::narration::key_for`] + [`crate::narration::tts_file`] under
/// [`crate::layout::Tree::tts_wav`]) because a warm project must really report zero work rather
/// than a remembered answer. `speak` performs F4.4 for one line and answers the take's path or the
/// refusal's sentence.
///
/// A refusal does not stop the run: §Failure-is-specific wants the bad line named where it failed
/// and the rest spoken anyway, so the caller logs each sentence and the tally still closes the job.
pub fn speak_lines<F>(
    entries: &[Entry],
    cached: impl Fn(&Entry) -> bool,
    mut speak: F,
) -> SpeakRun
where
    F: FnMut(&Entry) -> Result<std::path::PathBuf, String>,
{
    let decisions = narrate_run::speak_pass("", entries, cached);
    let mut lines = Vec::with_capacity(entries.len());
    let mut stopped = false;
    for (entry, decision) in entries.iter().zip(decisions) {
        // ⏹ between lines: the cancel switch is live (§F0.3), so a stop pressed while this pass walks
        // the record ends it HERE instead of spending one more synthesis on a run nobody wants.
        if crate::cancel_leg::cancelled() {
            stopped = true;
            break;
        }
        let line = match decision {
            Speak::SkippedBlank => Line::Blank,
            Speak::SkippedCached => Line::Cached,
            Speak::Synthesize => match speak(entry) {
                Ok(wav) => Line::Spoken(wav),
                Err(why) => Line::Refused(why),
            },
        };
        lines.push(line);
    }
    let spoken = lines
        .iter()
        .filter(|line| matches!(line, Line::Spoken(_)))
        .count();
    let cached_count = lines.iter().filter(|line| **line == Line::Cached).count();
    SpeakRun { lines, spoken, cached: cached_count, stopped }
}

/// The live leg for one line: the whole of F4.4 over the real wire, with the key, the seed and the
/// project's language derived from the entry exactly as the row's ▶ derives them
/// (`narrate_page::press_line_speak`), so a run and a single-row audition cannot disagree about
/// which take a line owns. Answers the take's path, or the refusal's sentence.
pub fn dial_line(tree: &Tree, voice: &str, language: &str, entry: &Entry) -> Result<std::path::PathBuf, String> {
    let key = narration::key_for(entry, Some(voice));
    let seed = narration::tts_seed(&key);
    match crate::speak_leg::speak(tree, &entry.text, &entry.emotion, seed, &key, language) {
        narrate_tts::Outcome::Take(wav) => Ok(wav),
        narrate_tts::Outcome::Refused(why) => Err(why),
    }
}

/// ⏹'s summary of a speaking pass cut short: the same `N line(s) spoken, M already in the cache`
/// sentence a finished pass logs, so a stop still says what it left behind — "stopped after 3 of 12"
/// and "stopped before anything" are different facts for the person reading the log.
pub fn stopped_speaking_log(run: &SpeakRun) -> String {
    narrate_run::spoken_log(run.spoken, run.cached)
}

/// Whether a line ended in a refusal, for the door's log loop.
pub fn refused(line: &Line) -> Option<&str> {
    match line {
        Line::Refused(why) => Some(why),
        _ => None,
    }
}
