//! §05-cut#8 bullet 3 — what the Suggest call tells the person while it works, and what it says back when
//! the answer is not a cut.
//!
//! Every sentence and number here belongs to [`crate::cut_suggest`]; this module is the MISSING HALF — the
//! attempt loop that asks those questions. Until now the retry logs, the progress line, the drop count and
//! the worst-first answer existed as rules with no caller, so §8's bullet described strings rather than the
//! program. Three doors, one per clause:
//!
//! - [`Call::offer`] — which tools THIS attempt gets, and the one log line for taking the web tools away.
//! - [`Call::report`] — the streamed answer's progress: where the bar sits, what the status row says, and
//!   whether it is still pulsing because no segment has closed yet.
//! - [`Call::note_reply_text`] — whether this reply was all thinking and no answer, which is what switches
//!   thinking off for the next attempt.
//!
//! Why it is a struct rather than three free functions: all four clauses read and write the same four pieces
//! of per-attempt state (rejections so far, whether thinking is still being asked for, whether the web tools
//! are still being offered, and where the progress bar last stood), and a free function per clause would
//! have the loop thread that state by hand — the shape that lets one branch forget to update one of them.

use crate::cut_suggest;
/// §S2b's first attempt, counted from 1 — the same number the attempt loop starts at, restated here rather
/// than imported so this module keeps no dependency on the UI layer.
const FIRST_ATTEMPT: u32 = 1;
use crate::tool_loop::Turn;
use crate::tools::Tool;

/// The attempt state §8 reads, and the lines it produces on the way to changing it.
#[derive(Debug, Clone)]
pub struct Call {
    /// How many answers have been refused so far. §8's web-tool rule counts rejections, not attempts.
    rejections: u32,
    /// Whether this attempt should ask for reasoning. Starts at §2's table for the cut model.
    thinking: bool,
    /// Whether the web tools are still being offered.
    web: bool,
    /// Where the progress line last stood, in session seconds. §8: it never moves backwards.
    shown: f64,
}

impl Call {
    /// A first attempt: thinking per the role table, web tools offered, progress nowhere yet.
    pub fn new() -> Self {
        Self {
            rejections: 0,
            thinking: cut_suggest::thinking_after_empty_reply(false, 1),
            web: cut_suggest::offers_web(0),
            shown: 0.0,
        }
    }

    /// Whether THIS attempt asks for reasoning.
    pub fn thinking(&self) -> bool {
        self.thinking
    }

    /// The tools this attempt is offered: the job's set with the web pair removed once something has been
    /// rejected. Removing rather than re-deriving matters — the five cut tools stay offered whatever happens,
    /// because the model cannot build a cut without them.
    pub fn offer(&mut self) -> (Vec<Tool>, Option<String>) {
        let mut offered = crate::tools::offered(crate::roles::Job::ModelCut);
        if self.web && !cut_suggest::offers_web(self.rejections) {
            self.web = false;
            offered.retain(|tool| !crate::tools::is_web_tool(*tool));
            return (offered, Some(cut_suggest::WEB_TOOLS_WITHDRAWN_LOG.to_string()));
        }
        (offered, None)
    }

    /// The same rule read from the reply's raw text, which is what the program actually holds: everything the
    /// model wrote in the content field, plus every tool call it made, since a call IS an answer. Empty on
    /// both counts is what "spent the whole call thinking and wrote nothing" means. An empty reply is ALSO the
    /// first rejection §8 withdraws the web tools on, so this counts it; `note_rejected` only adds the ones
    /// whose reply did answer.
    pub fn note_reply_text(&mut self, text: &str) -> Option<String> {
        let all_reasoning = text.trim().is_empty();
        if all_reasoning {
            self.rejections += 1;
        }
        // The attempt number this rule is asked about is the count of attempts BEHIND it — the first, plus one
    // per rejection since — not the raw rejection count: after ONE rejection we are on attempt 2, and
    // `thinking_after_empty_reply` takes the attempt. Passing the bare count would ask attempt 1's question
    // again and never switch thinking off.
    let attempt = FIRST_ATTEMPT + self.rejections;
        let was = self.thinking;
        self.thinking = cut_suggest::thinking_after_empty_reply(all_reasoning, attempt);
        (was && !self.thinking).then(|| cut_suggest::THINKING_RETRY_LOG.to_string())
    }

    /// The answer an attempt produced that was not a cut: the count the web-tool rule is measured by. Only
    /// counted when the reply DID answer — an empty reply was already counted by [`Self::note_reply_text`],
    /// which owns the FIRST rejection §8 withdraws on.
    pub fn note_rejected(&mut self) {
        self.rejections += 1;
    }

    /// Whether the last reply answered at all, which is what the loop tests before charging a rejection.
    pub fn answered(&self, text: &str) -> bool {
        !text.trim().is_empty()
    }

    /// The progress the reply allows, placed exactly as §8 requires.
    ///
    /// The scanner reads the reply text the dial actually handed back — the request is streamed
    /// (`live_ask` passes `streaming: true`) but the delta pump in [`crate::llm_stream_leg`] belongs to
    /// another item, so a call hands over its whole answer at once — and these are numbers computed from real
    /// reply bytes, never a scripted position. Objects closed inside the text's last `"segments": [` is what a
    /// partial answer can say, and the last closed
    /// object's `end` is the only position inside it; with neither, the bar pulses under the thinking sentence
    /// rather than claiming a place it does not have.
    pub fn report(&mut self, text: &str, total: f64) -> Progress {
        let (closed, last_end) = cut_suggest::streamed(text, "segments").unwrap_or((0, None));
        let at = last_end.unwrap_or(self.shown);
        let line = if closed > 0 {
            cut_suggest::progress(closed, at, total)
        } else {
            cut_suggest::THINKING_STATUS.to_string()
        };
        self.shown = cut_suggest::place_progress(at, self.shown);
        let fraction = cut_suggest::progress_fraction(closed, self.shown, total);
        Progress { line, fraction, pulsing: cut_suggest::pulses(self.shown, closed) }
    }
}

impl Default for Call {
    fn default() -> Self {
        Self::new()
    }
}

/// One reply's whole text in the shape the rules read it: the content field, then a line per tool call, so a
/// reply that placed segments counts as having written something and a reply that only thought does not.
pub fn reply_text(turn: &Turn) -> String {
    let mut text = turn.content.clone();
    for call in &turn.calls {
        text.push('\n');
        text.push_str(&call.name);
        text.push(' ');
        text.push_str(&call.args);
    }
    text
}

/// One tick's worth of progress for the page to paint: the status sentence, the bar's fraction, and whether
/// it is pulsing because nothing has closed yet.
#[derive(Debug, Clone, PartialEq)]
pub struct Progress {
    pub line: String,
    pub fraction: f64,
    pub pulsing: bool,
}

/// The count-and-length faults the whole-cut check answered with, in §8's shape: every fault at once, worst
/// first, joined by `"; "`. §8's own conversion fault — a stamp the model wrote with its colons taken out —
/// is handed in alongside them, so the model sees `2804 is not a second: a stamp [28:04] is mm*60+ss, 1684`
/// rather than being told a number is "too big". The check's strings pass through untouched: they are
/// `cutpass`'s own wording, and a model that gets a fault it did not write learns nothing.
pub fn faults(check: &[String], past_end: Option<u32>) -> String {
    let mut faults: Vec<cut_suggest::Fault> = Vec::new();
    if let Some(claimed) = past_end {
        faults.push(cut_suggest::Fault::PastEnd(claimed));
    }
    faults.extend(check.iter().map(|said| cut_suggest::Fault::Quoted(said.clone())));
    cut_suggest::answer(&faults)
}
