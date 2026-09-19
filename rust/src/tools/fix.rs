//! Fix — spec/02-services.md §3.3, one block of transcript lines at a time.
//!
//! The model respells what ASR got wrong and touches nothing else. That is the whole design: times
//! and speakers are not arguments of `fix_line`, so there is no way to change them and no need to
//! check afterwards. The prototype enforced the same rule by comparison — and when one row came back
//! with a changed time, a different speaker or a lost tab it threw away **the whole block of 25
//! lines**, dropped reply lines with fewer than four tab fields before counting, then re-asked the
//! identical question once more without saying what was wrong. Here a bad row is one refusal naming
//! the row, and the other 24 stand.

use serde_json::json;

/// The cross-source grounding window of `get_lines`: how far either side of the block lines from
/// every other recording are read for. P.machine.fixContextSeconds
pub const FIX_CONTEXT_SECONDS: f64 = 5.0;

/// One line of a block, as ASR wrote it.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    /// The number the model is shown and sends back — its position in this block, 1-based.
    pub n: u32,
    /// Not an argument of `fix_line`: a fix cannot move a line in time.
    pub start: f64,
    /// Nor this.
    pub end: f64,
    /// Nor this. A wrong speaker is flagged, not fixed — see [`Block::flag_line`].
    pub speaker: String,
    pub text: String,
}

impl Line {
    /// The ASR text, which is what a line keeps unless `fix_line` names it.
    pub fn asr_text(&self) -> &str {
        &self.text
    }
}

/// A block of lines offered to one fix pass, and the fixes named so far.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Block {
    lines: Vec<Line>,
    /// The fixes that landed, keyed by line number. A line absent from here keeps its ASR text.
    fixed: Vec<(u32, String)>,
    /// What the model said was wrong but not its to change. §3.3: the channel the prototype lacks,
    /// where a wrong speaker or mis-timed row is "not yours to fix" with nowhere to say so.
    flags: Vec<(u32, String)>,
}

impl Block {
    /// A block of ASR lines, numbered as given — the numbers are the ones the model will send back.
    pub fn new(lines: Vec<Line>) -> Block {
        Block {
            lines,
            fixed: Vec::new(),
            flags: Vec::new(),
        }
    }

    /// The block as it was handed over, ASR text and all.
    pub fn lines(&self) -> &[Line] {
        &self.lines
    }

    /// `fix_line(n, text)` — ok, or an error when `n` is not in this block or the text has a tab.
    ///
    /// The tab rule is what the prototype enforced by discarding everything: a tab is the field
    /// separator of `transcript.txt`, so a respelling carrying one would silently split into two
    /// rows on the way to disk. Refusing it says where the trouble is instead.
    pub fn fix_line(&mut self, n: u32, text: &str) -> String {
        if !self.lines.iter().any(|line| line.n == n) {
            let (first, last) = match (self.lines.first(), self.lines.last()) {
                (Some(first), Some(last)) => (first.n, last.n),
                // No lines at all: nothing to name, and the model needs to hear that.
                _ => return crate::tools::error("this block has no lines to fix"),
            };
            return crate::tools::error(&format!(
                "line {n} is not in this block -- it runs {first}..={last}"
            ));
        }
        if text.contains('\t') {
            return crate::tools::error(&format!(
                "line {n} has a tab in it -- a tab separates the fields of transcript.txt, \
                 so it would be read as two lines"
            ));
        }
        match self.fixed.iter_mut().find(|(at, _)| *at == n) {
            Some(entry) => entry.1 = text.to_string(),
            None => self.fixed.push((n, text.to_string())),
        }
        crate::tools::ok(&json!({"line": n, "text": text}))
    }

    /// `flag_line(n, why)` — ok: the line stands and the note is logged.
    ///
    /// This is the only place a model can say "the speaker on this row is wrong" without changing
    /// the row, which is what §3.3 asks for. A flag costs nothing, so there is no reason for the
    /// model to stay quiet about something it can see and must not touch.
    pub fn flag_line(&mut self, n: u32, why: &str) -> String {
        if !self.lines.iter().any(|line| line.n == n) {
            return crate::tools::error(&format!("line {n} is not in this block"));
        }
        self.flags.push((n, why.to_string()));
        crate::tools::ok(&json!({"line": n, "flagged": true, "why": why}))
    }

    /// The notes the model left, in the order it left them. Shown with the block so a wrong speaker
    /// reaches a person rather than being dropped with the reply.
    pub fn flags(&self) -> &[(u32, String)] {
        &self.flags
    }

    /// The fixes named so far.
    pub fn fixes(&self) -> &[(u32, String)] {
        &self.fixed
    }

    /// The block with the fixes applied: a line no `fix_line` named keeps its ASR text, and every
    /// time and speaker is the one that was there. This is where §3.3's "enforced" lives — not in a
    /// comparison after the reply, but in the fact that the reply has nowhere to put them.
    pub fn apply(&self) -> Vec<Line> {
        self.lines
            .iter()
            .map(|line| {
                let mut fixed = line.clone();
                if let Some((_, text)) = self.fixed.iter().find(|(n, _)| *n == line.n) {
                    fixed.text = text.clone();
                }
                fixed
            })
            .collect()
    }

    /// `get_lines(from, to)` — the lines around the block from this and every other source.
    ///
    /// The window is ±[`FIX_CONTEXT_SECONDS`] of the asked range, and a line that merely overlaps it
    /// counts: a sentence starting three seconds before the window and running into it is exactly
    /// the grounding a respelling needs, and containment would drop it. That is also why the result
    /// can be longer than ±5 s of transcript — §3.3 says "beyond the brief's ±P.machine... window"
    /// on purpose.
    pub fn context_lines<'a>(
        &self,
        from: f64,
        to: f64,
        every_source: impl IntoIterator<Item = &'a Line>,
    ) -> Vec<&'a Line> {
        let (from, to) = if from <= to { (from, to) } else { (to, from) };
        let open = from - FIX_CONTEXT_SECONDS;
        let close = to + FIX_CONTEXT_SECONDS;
        every_source
            .into_iter()
            .filter(|line| line.start < close && line.end > open)
            .collect()
    }
}
