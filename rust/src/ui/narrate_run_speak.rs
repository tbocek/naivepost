//! F4.1 S6 — the speaking half of ▶ Write and speak, over the real wire.

use crate::layout::Tree;
use crate::narrate_run::{self, Speak};
use crate::narrate_tts;
use crate::narration::{self, Entry};

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
pub fn speak_lines<F>(entries: &[Entry], cached: impl Fn(&Entry) -> bool, mut speak: F) -> SpeakRun
where
    F: FnMut(&Entry) -> Result<std::path::PathBuf, String>,
{
    let decisions = narrate_run::speak_pass(entries, cached);
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
    SpeakRun {
        lines,
        spoken,
        cached: cached_count,
        stopped,
    }
}

pub fn dial_line(
    tree: &Tree,
    voice: &str,
    language: &str,
    entry: &Entry,
) -> Result<std::path::PathBuf, String> {
    let key = narration::key_for(entry, Some(voice));
    let seed = narration::tts_seed(&key);
    match crate::speak_leg::speak(tree, &entry.text, &entry.emotion, seed, &key, language) {
        narrate_tts::Outcome::Take(wav) => Ok(wav),
        narrate_tts::Outcome::Refused(why) => Err(why),
    }
}

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
