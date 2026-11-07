//! The model exchange log (spec/03-shell.md §7).

use std::fs;
use std::path::{Path, PathBuf};

use crate::layout::Tree;

/// The folder under the project these pages live in.
pub const LLM_DIR: &str = "llm";

/// Appended to a came-back line when the model stopped mid-answer.
pub const CUT_OFF: &str = "\u{2014} cut off at the model's token limit";

const PREVIEW_CHARS: usize = 110;

/// What the meta line says while the answer is still on its way (§7).
pub const REPLY_PENDING: &str = "reply pending";

const CUT_OFF_PLAIN: &str = "cut off at the model's token limit";

/// The stamp and first step name the page: `MMDD-HHMMSS-<first step>.html`.
pub fn page_name(stamp: &str, first_step: &str) -> String {
    format!("{stamp}-{first_step}.html")
}

/// Which of the model's two jobs a call belongs to (spec/05-cut.md and
/// spec/06-narrate.md both split thinking from executing).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Thinking,
    Execute,
}

impl Mode {
    fn label(self) -> &'static str {
        match self {
            Self::Thinking => "thinking",
            Self::Execute => "execute",
        }
    }
}

/// One piece of a message: words, or a picture sent as a data URL so the page
/// shows exactly what the model saw without keeping a second copy of frames.
#[derive(Debug, Clone)]
pub enum Part {
    Text(String),
    Image(String),
}

impl Part {
    fn text_len(&self) -> usize {
        match self {
            Self::Text(text) => text.len(),
            Self::Image(_) => 0,
        }
    }
}

/// One message as it went to the model.
#[derive(Debug, Clone)]
pub struct Message {
    pub role: String,
    pub parts: Vec<Part>,
}

impl Message {
    fn text_len(&self) -> usize {
        self.parts.iter().map(Part::text_len).sum()
    }
}

/// A tool the model asked for and whatever came back.
#[derive(Debug, Clone)]
pub struct ToolCall {
    pub name: String,
    pub args: String,
    pub result: String,
}

/// One exchange: what went out, what came back, how long it took.
#[derive(Debug, Clone)]
pub struct Call {
    pub step: String,
    pub model: String,
    pub mode: Mode,
    pub took_secs: u64,
    /// How much of `took_secs` went to reasoning rather than answering.
    pub thinking_secs: Option<u64>,
    pub messages: Vec<Message>,
    pub reply: String,
    pub reasoning: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    pub cut_off: bool,
    pub error: Option<String>,
}

impl Call {
    /// Bytes of text and number of images sent.
    pub fn sent(&self) -> (usize, usize) {
        let text = self.messages.iter().map(Message::text_len).sum();
        let images = self
            .messages
            .iter()
            .flat_map(|message| message.parts.iter())
            .filter(|part| matches!(part, Part::Image(_)))
            .count();
        (text, images)
    }
}

/// A byte count the way a person reads one.
pub fn size_of(bytes: usize) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = 1024.0 * 1024.0;
    let bytes = bytes as f64;
    if bytes < KB {
        format!("{bytes:.0} B")
    } else if bytes < MB {
        format!("{:.1} kB", bytes / KB)
    } else {
        format!("{:.1} MB", bytes / MB)
    }
}

/// A byte count the way §5's size line wants it: ALWAYS kilobytes, one decimal, whatever the size — `0.8 kB`,
/// `451.3 kB`, and even a megabyte of it as `1536.0 kB`.
pub fn kb_of(bytes: usize) -> String {
    format!("{:.1} kB", bytes as f64 / 1024.0)
}

/// A duration in the fewest units that still say how long it was.
pub fn duration_of(secs: u64) -> String {
    let (hours, rest) = (secs / 3600, secs % 3600);
    if hours > 0 {
        return format!("{hours}h{:02}m{:02}s", rest / 60, rest % 60);
    }
    if secs >= 60 {
        return format!("{}m{:02}s", secs / 60, secs % 60);
    }
    format!("{secs}s")
}

/// The first words of the answer itself.
pub fn preview(reply: &str) -> String {
    let tail = match reply.rfind("</think>") {
        Some(end) => &reply[end + "</think>".len()..],
        None => reply,
    };
    let collapsed = tail.split_whitespace().collect::<Vec<_>>().join(" ");
    let chars: Vec<char> = collapsed.chars().collect();
    if chars.len() > PREVIEW_CHARS {
        let head: String = chars[..PREVIEW_CHARS].iter().collect();
        return format!("{head}\u{2026}");
    }
    collapsed
}

/// What a call that answered puts in the log: how much came back and how long it
/// took, with the reasoning time when the model spent any — four minutes of thinking
/// behind two sentences is the fact worth naming (§8).
fn came_back(call: &Call) -> String {
    let mut line = format!(
        ">>> {}: {} came back in {}",
        call.step,
        size_of(call.reply.len()),
        duration_of(call.took_secs)
    );
    if let Some(thinking) = call.thinking_secs {
        line.push_str(&format!(", after {} of thinking", duration_of(thinking)));
    }
    if call.cut_off {
        line.push_str(&format!(" {CUT_OFF}"));
    }
    line
}

/// The two lines one call puts in the app log, plus the reply preview.
pub fn log_lines(call: &Call) -> Vec<String> {
    let (text, images) = call.sent();
    let mut lines = vec![format!(
        ">>> {}: {} of text and {images} image(s) went to the LLM",
        call.step,
        kb_of(text)
    )];

    if let Some(err) = &call.error {
        lines.push(format!(
            ">>> {}: the call failed after {}: {err}",
            call.step,
            duration_of(call.took_secs)
        ));
        return lines;
    }

    lines.push(came_back(call));

    if call.reply.trim().is_empty() && !call.tool_calls.is_empty() {
        // A reply made of tool calls alone has no text, and is an answer: "nothing at all" sent people looking for
        // a fault in a run that was working (09-admin's cut, 2026-11-07).
        lines.push(format!(
            ">>> {}: the model called {}",
            call.step,
            called_tools(&call.tool_calls)
        ));
    } else if call.reply.trim().is_empty() {
        lines.push(format!(
            ">>> {}: the model answered nothing at all",
            call.step
        ));
    } else {
        lines.push(format!(">>>   the reply begins: {}", preview(&call.reply)));
    }
    lines
}

/// The tools one reply called, by name, a repeat counted: `add_segment ×3, finish_cut`.
fn called_tools(calls: &[ToolCall]) -> String {
    let mut named: Vec<(&str, usize)> = Vec::new();
    for call in calls {
        match named
            .iter_mut()
            .find(|(name, _)| *name == call.name.as_str())
        {
            Some((_, count)) => *count += 1,
            None => named.push((call.name.as_str(), 1)),
        }
    }
    named
        .iter()
        .map(|(name, count)| {
            if *count > 1 {
                format!("{name} \u{d7}{count}")
            } else {
                name.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// The one line per run naming the page.
pub fn link_line(page_rel: &str) -> String {
    format!(">>>   this run's exchanges, images included: {page_rel}")
}

/// What a tool result says about itself.
pub fn result_outcome(result: &str) -> Option<&'static str> {
    if result.trim().is_empty() {
        return Some("answered nothing");
    }
    let refused = serde_json::from_str::<serde_json::Value>(result)
        .ok()
        .and_then(|value| {
            value
                .get("error")
                .and_then(serde_json::Value::as_str)
                .filter(|reason| !reason.trim().is_empty())
                .map(|_| ())
        })
        .is_some();
    refused.then_some("refused")
}

fn esc(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// The page's `<head>`, written once before any section exists.
pub fn page_head(name: &str) -> String {
    format!(
        "<!doctype html>\n<html><head><meta charset=\"utf-8\">\n\
         <title>{name} -- Naivepost run</title>\n<style>{CSS}</style></head><body>\n",
        name = esc(name)
    )
}

const CSS: &str = "body{font:14px/1.5 monospace;margin:1em}\
h1{font-size:1.15em;margin:1.2em 0 .2em}\
h2{margin:1.4em 0 .2em}\
.meta{color:#666}\
.msg{border-left:2px solid #ccc;padding-left:.6em;margin:.4em 0}\
.role{font-weight:bold}\
pre{white-space:pre-wrap;margin:.2em 0}\
img{max-width:480px;display:block;margin:.3em 0}\
.note{color:#a00}";

/// The meta line under a section's heading: model, mode, and either the pending marker or the
/// outcome (§7).
pub fn meta_line(call: &Call, answered: bool) -> String {
    let tail = if !answered {
        REPLY_PENDING.to_string()
    } else if let Some(err) = &call.error {
        err.clone()
    } else if call.cut_off {
        CUT_OFF_PLAIN.to_string()
    } else if call.reply.trim().is_empty() && call.tool_calls.is_empty() {
        "answered nothing at all".to_string()
    } else {
        duration_of(call.took_secs)
    };
    format!(
        "{} \u{b7} {} \u{b7} {}",
        esc(&call.model),
        call.mode.label(),
        esc(&tail)
    )
}

/// The word the page puts in front of one tool block.
fn tool_word(tool: &ToolCall, index: usize) -> String {
    if tool.name.is_empty() {
        return esc(&format!("result #{}", index + 1));
    }
    esc(&tool.name)
}

/// One call as a numbered section of the page, in its finished form.
pub fn section_html(seq: usize, call: &Call) -> String {
    section_in_state(seq, call, true)
}

/// The same section before the reply has come back: the messages are already sent, so they belong on
/// disk now, but there is no reply, reasoning, tool note or cut-off note to show yet.
pub fn section_html_pending(seq: usize, call: &Call) -> String {
    section_in_state(seq, call, false)
}

/// The section renderer behind both forms.
fn section_in_state(seq: usize, call: &Call, answered: bool) -> String {
    let mut html = format!(
        "<section><h1>{seq}. {step}</h1>\n<p class=\"meta\">{meta}</p>\n",
        step = esc(&call.step),
        meta = meta_line(call, answered)
    );

    for message in &call.messages {
        html.push_str(&format!(
            "<div class=\"msg\"><h2 class=\"role\">{}:</h2>",
            esc(&message.role)
        ));
        for part in &message.parts {
            match part {
                Part::Text(text) => html.push_str(&format!("\n<pre>{}</pre>", esc(text))),
                Part::Image(url) => html.push_str(&format!("\n<img src=\"{}\">", esc(url))),
            }
        }
        html.push_str("</div>\n");
    }

    if !answered {
        return html;
    }

    if let Some(reasoning) = &call.reasoning {
        html.push_str(&format!(
            "<details><summary>reasoning</summary><pre>{}</pre></details>\n",
            esc(reasoning)
        ));
    }

    for (index, tool) in call.tool_calls.iter().enumerate() {
        html.push_str(&format!(
            "<div class=\"tool\"><span class=\"role\">{}</span>(<pre>{}</pre>)\n<pre>{}</pre>{}</div>\n",
            tool_word(tool, index),
            esc(&tool.args),
            esc(&tool.result),
            // §7's notes cover cut-off and empty answers; this is the same idea one level down, where a
            // tool refused.
            match result_outcome(&tool.result) {
                Some(outcome) => format!("<span class=\"note\">{outcome}</span>"),
                None => String::new(),
            }
        ));
    }

    if let Some(err) = &call.error {
        html.push_str(&format!("<p class=\"note\">{}</p>\n</section>\n", esc(err)));
        return html;
    }

    if call.reply.trim().is_empty() && call.tool_calls.is_empty() {
        html.push_str("<p class=\"note\">the model answered nothing at all</p>\n");
    } else if !call.reply.trim().is_empty() {
        html.push_str(&format!("<pre>{}</pre>\n", esc(&call.reply)));
    }
    // §7's wording for the page: each tool call is noted on the reply as `[tool call name(args)]`.
    for tool in &call.tool_calls {
        // [tool name(args) → outcome] — the same verdict the block itself carries, so the two can never disagree.
        let line = crate::exchange_leg::call_line(tool);
        html.push_str(&format!("<p>{}</p>\n", esc(&line)));
    }
    if call.cut_off {
        html.push_str("<p class=\"note\">cut off at the model's token limit</p>\n");
    }
    html.push_str("</section>\n");
    html
}

/// One run's page: sections in call order, written when the request goes out, again
/// as the reply streams, and whole at the end.
pub struct Page {
    name: String,
    dir: PathBuf,
    seq: usize,
    sections: Vec<String>,
    open: Option<usize>,
    streamed_raw: Option<String>,
}

impl Page {
    /// Name the page for this run.
    pub fn start(tree: &Tree, stamp: &str, first_step: &str) -> Page {
        Page {
            name: page_name(stamp, first_step),
            dir: tree.llm_dir(),
            seq: 0,
            sections: Vec::new(),
            open: None,
            streamed_raw: None,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn path(&self) -> PathBuf {
        self.dir.join(&self.name)
    }

    /// The page as the log names it.
    pub fn rel(&self) -> String {
        format!("{LLM_DIR}/{}", self.name)
    }

    /// File one call and return what the app log should show for it: its own
    /// lines, then the page link the first time round only.
    pub fn record(&mut self, call: &Call) -> Vec<String> {
        let mut lines = self.begin(call);
        lines.extend(self.complete(call));
        lines
    }

    /// The request half: filed as a section and on disk before the model has said a
    /// word.
    pub fn begin(&mut self, call: &Call) -> Vec<String> {
        self.seq += 1;
        self.sections.push(section_html_pending(self.seq, call));
        self.open = Some(self.seq - 1);
        self.write();

        let (text, images) = call.sent();
        let mut lines = vec![format!(
            ">>> {}: {} of text and {images} image(s) went to the LLM",
            call.step,
            kb_of(text)
        )];
        if self.seq == 1 {
            lines.push(link_line(&self.rel()));
        }
        lines
    }

    /// Part of the reply arrived: append it to the open section and put the page back
    /// on disk.
    pub fn stream(&mut self, text: &str) -> Option<String> {
        let open = self.open?;
        if text.is_empty() {
            return None;
        }
        let stored = std::mem::take(&mut self.sections[open]);
        // Everything up to the marker is the section as it was written; the reply itself is carried
        // in `streamed_raw`, never read back out of the escaped HTML.
        let head = match stored.find(STREAM_MARK) {
            Some(at) => stored[..at].to_string(),
            None => stored.clone(),
        };
        let streamed = format!("{}{text}", self.streamed_raw.take().unwrap_or_default());
        // Whatever came after the open `<pre>` stays after it, so a later section cannot be swallowed.
        let tail = match stored.rfind(CLOSE_PRE) {
            Some(at) => stored[at + CLOSE_PRE.len()..].to_string(),
            None => String::new(),
        };
        self.streamed_raw = Some(streamed.clone());
        // The meta line changes with the first fragment: `reply pending` was true while nothing had arrived,
        // and a page that still says it after words landed tells the reader the opposite of what is on disk.
        let head = mark_answered(head);
        self.sections[open] = format!("{head}{STREAM_PRE}{}{CLOSE_PRE}\n{tail}", esc(&streamed));
        self.write();
        Some(streamed)
    }

    /// The call is over: rewrite its section whole from the finished call — the
    /// streamed fragment is replaced, not repeated — and return the verdict lines.
    pub fn complete(&mut self, call: &Call) -> Vec<String> {
        if let Some(open) = self.open.take() {
            self.streamed_raw = None;
            self.sections[open] = section_html(open + 1, call);
            self.write();
        }
        log_lines(call)
    }

    /// Write the page.
    pub fn flush(&mut self) -> Option<String> {
        if self.sections.is_empty() {
            return None;
        }
        write_page(&self.dir, &self.name, &self.sections).err()
    }

    /// Rewrite the page.
    fn write(&mut self) {
        let _ = write_page(&self.dir, &self.name, &self.sections);
    }
}

/// Which page a call belongs to, and when that page ends.
#[derive(Default)]
pub struct RunLog {
    page: Option<Page>,
}

impl RunLog {
    /// No run open yet: nothing recorded, nothing on disk.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether a page is currently open for this run.
    pub fn is_open(&self) -> bool {
        self.page.is_some()
    }

    /// The open page's project-relative name — what the log line points at.
    pub fn page_rel(&self) -> Option<String> {
        self.page.as_ref().map(Page::rel)
    }

    /// File one call's REQUEST half before its answer exists, so a page opened mid-call shows the request
    /// with `reply pending` and then grows.
    pub fn begin(&mut self, tree: &Tree, stamp: &str, step: &str, call: &Call) -> Vec<String> {
        if self.page.is_none() {
            self.page = Some(Page::start(tree, stamp, step));
        }
        self.page.as_mut().expect("just opened above").begin(call)
    }

    /// Part of the reply arrived: grow the open section and put the page back on disk.
    pub fn stream(&mut self, text: &str) -> Option<String> {
        self.page.as_mut()?.stream(text)
    }

    /// The call is over: rewrite its open section whole from the finished call and hand back the verdict
    /// lines.
    pub fn complete(&mut self, tree: &Tree, stamp: &str, step: &str, call: &Call) -> Vec<String> {
        if self.page.is_none() {
            self.page = Some(Page::start(tree, stamp, step));
        }
        self.page
            .as_mut()
            .expect("just opened above")
            .complete(call)
    }

    /// Record one call, opening the run's page under `stamp`/`step` if none is open.
    pub fn record(&mut self, tree: &Tree, stamp: &str, step: &str, call: &Call) -> Vec<String> {
        if self.page.is_none() {
            self.page = Some(Page::start(tree, stamp, step));
        }
        let page = self.page.as_mut().expect("just opened above");
        page.record(call)
    }

    /// A queue reset ends the run: close the page so the next call starts a fresh one, and hand
    /// back the closed page's relative name so the caller can still point at what just finished.
    pub fn reset(&mut self) -> Option<String> {
        self.page.take().map(|page| page.rel())
    }
}

/// Marks where a call's streamed reply begins inside its own section, so `stream` can
/// grow it and `complete` replace it wholesale.
const STREAM_MARK: &str = "<!--streamed:";

/// What the streamed block opens with: the reply grows inside a `<pre>` of its own, so a reader
/// watching the file during a call sees the words arriving in the shape they will end in (§7:
const STREAM_PRE: &str = "<pre><!--streamed:";

/// What closes that `<pre>`.
const CLOSE_PRE: &str = concat!("</", "pre>");

fn mark_answered(section: String) -> String {
    section.replace(REPLY_PENDING, "arriving")
}

/// The whole page in one write: a half-written file would look like an exchange
/// that never happened.
fn write_page(dir: &Path, name: &str, sections: &[String]) -> Result<(), String> {
    let mut body = page_head(name);
    for section in sections {
        body.push_str(section);
    }
    body.push_str("</body></html>\n");

    fs::create_dir_all(dir).map_err(|err| format!("could not keep the exchange: {err}"))?;
    fs::write(dir.join(name), body).map_err(|err| format!("could not keep the exchange: {err}"))
}
