//! The model exchange log (spec/03-shell.md §7).
//!
//! One HTML page per run under `llm/`, named after the first step that ran, so
//! the file itself says which work produced it. Each call becomes a numbered
//! section; the app log gets two lines per call plus, once per run, the line
//! naming the page.
//!
//! Writing the page can never fail the call it records: every io problem comes
//! back as an already-formatted log line instead of an error the step has to
//! handle. Observability that can break the work is not observability.

use std::fs;
use std::path::{Path, PathBuf};

use crate::layout::Tree;

/// The folder under the project these pages live in.
pub const LLM_DIR: &str = "llm";

/// Appended to a came-back line when the model stopped mid-answer.
pub const CUT_OFF: &str = "\u{2014} cut off at the model's token limit";

/// A preview longer than this is truncated — it sits in a log header, not in
/// the page, where the whole reply already is.
const PREVIEW_CHARS: usize = 110;

/// The stamp and first step name the page: `MMDD-HHMMSS-<first step>.html`.
/// Later steps never rename it — the run keeps one page whoever started it.
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
    /// How much of `took_secs` went to reasoning rather than answering. `None` when
    /// the model does not separate them — §8 prints the cost only when it is known.
    pub thinking_secs: Option<u64>,
    pub messages: Vec<Message>,
    pub reply: String,
    pub reasoning: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    pub cut_off: bool,
    pub error: Option<String>,
}

impl Call {
    /// Bytes of text and number of images sent. An image never counts as text:
    /// "1.8 MB of text" would be a lie about a request carrying two frames.
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

/// The first words of the answer itself. Everything up to the last `</think>`
/// is the model talking to itself, and quoting that would make every preview
/// open the same way; the whole of it is in the page.
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
///
/// The `>>> ` prefixes are kept because §7 quotes these lines literally; §2's
/// rule that the level shows by colour is the log view's rendering job, not
/// something the message text carries.
pub fn log_lines(call: &Call) -> Vec<String> {
    let (text, images) = call.sent();
    let mut lines = vec![format!(
        ">>> {}: {} of text and {images} image(s) went to the LLM",
        call.step,
        size_of(text)
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

    if call.reply.trim().is_empty() {
        lines.push(format!(">>> {}: the model answered nothing at all", call.step));
    } else {
        lines.push(format!(">>>   the reply begins: {}", preview(&call.reply)));
    }
    lines
}

/// The one line per run naming the page. Relative, so it still means the same
/// thing after Save as (F0.10) moves the folder it points into.
pub fn link_line(page_rel: &str) -> String {
    format!(">>>   this run's exchanges, images included: {page_rel}")
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
h2{margin:1.4em 0 .2em}\
.meta{color:#666}\
.msg{border-left:2px solid #ccc;padding-left:.6em;margin:.4em 0}\
.role{font-weight:bold}\
pre{white-space:pre-wrap;margin:.2em 0}\
img{max-width:480px;display:block;margin:.3em 0}\
.note{color:#a00}";

/// One call as a numbered section of the page.
pub fn section_html(seq: usize, call: &Call) -> String {
    let mut html = format!(
        "<section><h2>{seq}. {step}</h2>\n<p class=\"meta\">{model} \u{b7} {mode} \u{b7} {took}</p>\n",
        step = esc(&call.step),
        model = esc(&call.model),
        mode = call.mode.label(),
        took = duration_of(call.took_secs)
    );

    for message in &call.messages {
        html.push_str(&format!("<div class=\"msg\"><span class=\"role\">{}:</span>", esc(&message.role)));
        for part in &message.parts {
            match part {
                Part::Text(text) => html.push_str(&format!("\n<pre>{}</pre>", esc(text))),
                Part::Image(url) => html.push_str(&format!("\n<img src=\"{}\">", esc(url))),
            }
        }
        html.push_str("</div>\n");
    }

    if let Some(reasoning) = &call.reasoning {
        html.push_str(&format!(
            "<details><summary>reasoning</summary><pre>{}</pre></details>\n",
            esc(reasoning)
        ));
    }

    for tool in &call.tool_calls {
        html.push_str(&format!(
            "<div class=\"tool\"><span class=\"role\">{}</span>(<pre>{}</pre>)\n<pre>{}</pre></div>\n",
            esc(&tool.name),
            esc(&tool.args),
            esc(&tool.result)
        ));
    }

    if let Some(err) = &call.error {
        html.push_str(&format!("<p class=\"note\">{}</p>\n</section>\n", esc(err)));
        return html;
    }

    if call.reply.trim().is_empty() {
        html.push_str("<p class=\"note\">the model answered nothing at all</p>\n");
    } else {
        html.push_str(&format!("<pre>{}</pre>\n", esc(&call.reply)));
    }
    if call.cut_off {
        // The dash belongs to the log line's prose; here it reads as a note.
        html.push_str("<p class=\"note\">cut off at the model's token limit</p>\n");
    }
    html.push_str("</section>\n");
    html
}

/// One run's page: sections in call order, written when the request goes out, again
/// as the reply streams, and whole at the end. Writing early is the point — a run
/// killed mid-call still leaves the prompt it was stuck on.
pub struct Page {
    name: String,
    dir: PathBuf,
    seq: usize,
    sections: Vec<String>,
    /// The section of the call currently open, so `stream` can grow it in place.
    open: Option<usize>,
}

impl Page {
    /// Name the page for this run. Nothing is created yet — a run that never
    /// asks the model anything leaves no empty page behind.
    pub fn start(tree: &Tree, stamp: &str, first_step: &str) -> Page {
        Page {
            name: page_name(stamp, first_step),
            dir: tree.llm_dir(),
            seq: 0,
            sections: Vec::new(),
            open: None,
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
    /// word. Returns what went out, plus the page link — once per run, on this first
    /// call of the run (§8).
    pub fn begin(&mut self, call: &Call) -> Vec<String> {
        self.seq += 1;
        self.sections.push(section_html(self.seq, call));
        self.open = Some(self.seq - 1);
        self.write();

        let (text, images) = call.sent();
        let mut lines = vec![format!(
            ">>> {}: {} of text and {images} image(s) went to the LLM",
            call.step,
            size_of(text)
        )];
        if self.seq == 1 {
            lines.push(link_line(&self.rel()));
        }
        lines
    }

    /// Part of the reply arrived: append it to the open section and put the page back
    /// on disk. `None` once the call is complete, or with nothing to add — a recorder
    /// that is not recording is valid and inert.
    pub fn stream(&mut self, text: &str) -> Option<String> {
        let open = self.open?;
        if text.is_empty() {
            return None;
        }
        let section = &self.sections[open];
        let head = match section.find(STREAM_MARK) {
            Some(at) => &section[..at],
            None => section.as_str(),
        };
        let streamed = match section.rfind(STREAM_MARK) {
            Some(at) => format!("{}{}", &section[at + STREAM_MARK.len()..], text),
            None => text.to_string(),
        };
        self.sections[open] = format!("{head}{STREAM_MARK}{streamed}");
        self.write();
        Some(streamed)
    }

    /// The call is over: rewrite its section whole from the finished call — the
    /// streamed fragment is replaced, not repeated — and return the verdict lines.
    pub fn complete(&mut self, call: &Call) -> Vec<String> {
        if let Some(open) = self.open.take() {
            self.sections[open] = section_html(open + 1, call);
            self.write();
        }
        log_lines(call)
    }

    /// Write the page. `None` when there is nothing to write or it worked;
    /// `Some(line)` holding an already-worded failure when it did not, because
    /// a failed exchange page must never fail the step that made it.
    pub fn flush(&mut self) -> Option<String> {
        if self.sections.is_empty() {
            return None;
        }
        write_page(&self.dir, &self.name, &self.sections).err()
    }

    /// Rewrite the page. A failure is dropped here on purpose: §8's "recording never
    /// fails the call" means a half-written page is acceptable and an error handed to
    /// the step is not. The final [`Page::flush`] is what reports it.
    fn write(&mut self) {
        let _ = write_page(&self.dir, &self.name, &self.sections);
    }
}

/// Marks where a call's streamed reply begins inside its own section, so `stream` can
/// grow it and `complete` replace it wholesale. An HTML comment: invisible in the page
/// and never part of the model's text.
const STREAM_MARK: &str = "<!--streamed:";

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
