//! Fix — spec/02-services.md §3.3, one block of transcript lines at a time.

use serde_json::json;

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
    pub fn flag_line(&mut self, n: u32, why: &str) -> String {
        if !self.lines.iter().any(|line| line.n == n) {
            return crate::tools::error(&format!("line {n} is not in this block"));
        }
        self.flags.push((n, why.to_string()));
        crate::tools::ok(&json!({"line": n, "flagged": true, "why": why}))
    }

    pub fn flags(&self) -> &[(u32, String)] {
        &self.flags
    }

    /// The fixes named so far.
    pub fn fixes(&self) -> &[(u32, String)] {
        &self.fixed
    }

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
