//! §05-cut#8 bullet 3 — what the Suggest call tells the person while it works, and what it says back when
//! the answer is not a cut.

use crate::cut_suggest;
/// §S2b's first attempt, counted from 1 — the same number the attempt loop starts at, restated here rather
/// than imported so this module keeps no dependency on the UI layer.
const FIRST_ATTEMPT: u32 = 1;
use crate::tool_loop::Turn;
use crate::tools::Tool;

/// The attempt state §8 reads, and the lines it produces on the way to changing it.
#[derive(Debug, Clone)]
pub struct Call {
    rejections: u32,
    /// Whether this attempt should ask for reasoning. Starts at §2's table for the cut model.
    thinking: bool,
    /// Whether the web tools are still being offered.
    web: bool,
    /// Whether the web pair was TAKEN away this run, as opposed to never offered (§2's no-browser case).
    withdrawn: bool,
    /// Whether an earlier attempt of this run was asked WITH thinking: that is what makes switching it off a
    /// change worth announcing rather than a fact the model never had a chance to notice.
    asked_thinking_before: bool,
    announced: bool,
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
            withdrawn: false,
            announced: false,
            asked_thinking_before: cut_suggest::thinking_after_empty_reply(false, 1),
            shown: 0.0,
        }
    }

    /// Whether THIS attempt asks for reasoning.
    pub fn thinking(&self) -> bool {
        self.thinking
    }

    pub fn with_web(&mut self, web: bool) {
        self.web = web;
    }

    pub fn offer(&mut self) -> (Vec<Tool>, Option<String>) {
        let mut offered = crate::tools::offered(crate::roles::Job::ModelCut);
        if !self.web {
            // No browser on this machine (§2, via [`Call::with_web`]): keep the web rows out silently.
            offered.retain(|tool| !crate::tools::is_web_tool(*tool));
        }
        if self.web && !cut_suggest::offers_web(self.rejections) {
            self.web = false;
            self.withdrawn = true;
            self.announced = false;
            offered.retain(|tool| !crate::tools::is_web_tool(*tool));
            return (
                offered,
                Some(cut_suggest::WEB_TOOLS_WITHDRAWN_LOG.to_string()),
            );
        }
        (offered, None)
    }

    pub fn note_reply_text(&mut self, text: &str) -> Option<String> {
        let all_reasoning = text.trim().is_empty();
        if all_reasoning {
            self.rejections += 1;
        }
        let attempt = FIRST_ATTEMPT + self.rejections;
        let was = self.thinking;
        self.thinking = cut_suggest::thinking_after_empty_reply(all_reasoning, attempt);
        if self.thinking {
            self.asked_thinking_before = true;
        }
        self.announced = false;
        (was && !self.thinking).then(|| cut_suggest::THINKING_RETRY_LOG.to_string())
    }

    pub fn note_rejected(&mut self) {
        self.rejections += 1;
    }

    /// Whether the last reply answered at all, which is what the loop tests before charging a rejection.
    pub fn answered(&self, text: &str) -> bool {
        !text.trim().is_empty()
    }

    pub fn note_to_model(&mut self) -> Option<String> {
        let mut notes: Vec<&str> = Vec::new();
        // Only a real withdrawal counts: tools never offered (no browser, §2) were nothing the model had.
        if self.withdrawn && !self.announced {
            notes.push("the web browsing tools are no longer available to you");
        }
        if self.asked_thinking_before && !self.thinking {
            notes.push(
                "this call has no thinking -- write the answer itself in the content of your reply",
            );
        }
        if notes.is_empty() {
            return None;
        }
        self.announced = true;
        Some(notes.join("; "))
    }

    /// The progress the reply allows, placed exactly as §8 requires.
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
        Progress {
            line,
            fraction,
            pulsing: cut_suggest::pulses(self.shown, closed),
        }
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

pub fn faults(check: &[String], past_end: Option<u32>) -> String {
    let mut faults: Vec<cut_suggest::Fault> = Vec::new();
    if let Some(claimed) = past_end {
        faults.push(cut_suggest::Fault::PastEnd(claimed));
    }
    faults.extend(
        check
            .iter()
            .map(|said| cut_suggest::Fault::Quoted(said.clone())),
    );
    cut_suggest::answer(&faults)
}
