//! §03-shell.md §1: the log's level comes off the line and shows as a colour, not as a prefix.
//!
//! §1's log bullet is explicit: "each line is the message alone. New: no `>>> `, `!!! ` or four-space
//! prefix — the level shows by colour (failure red, detail dimmed, progress plain). Prototype: lines
//! start `>>> `, `!!! ` or four spaces." So the prototype's markers survive as the LEVEL of a line —
//! they are what every flow already writes (§7 quotes them verbatim, and the sinks print them) — but the
//! shell's log view draws the message without them and paints the level instead.
//!
//! DECISION recorded here because §1 and §7 pull in opposite directions: §7 still spells the app-log
//! lines as `">>> <step>: …"`, and dozens of tests read the sink (`window_logs()`) in that wording.
//! So the MARKER STAYS in the stored line (the sink, stderr, the exchange page: nothing changed there)
//! and only what the log VIEW draws loses it. One painter, [`paint_line`] in `log_link`, is what applies
//! this, so the two readings cannot drift.
//!
//! Split out of the paint path because it is a rule, not a widget: which level, and which body, is
//! answerable with no window at all.

use adw::prelude::*;
use gtk4 as gtk;

/// The four levels §1 names. `Plain` is a line that carries no marker at all — the shell's own
/// sentences ("line restored to 00:11", "opened ‹folder›" before its marker is taken off) read the same
/// as a progress line, which is what "progress plain" leaves them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// `>>> ` — the run deciding or reporting.
    Progress,
    /// `!!! ` — a failure or a warning; the only level given a colour of its own beyond dimming.
    Failure,
    /// Four leading spaces — a detail: the command a subprocess ran, a path with its size.
    Detail,
    /// No marker.
    Plain,
}

impl Level {
    /// The buffer tag a line of this level is drawn with, or `None` for the plain case: a progress line
    /// and an unmarked line take the log view's own foreground, which is what keeps the log readable in
    /// a dark theme rather than painted with a hardcoded grey.
    pub fn tag(self) -> Option<&'static str> {
        match self {
            Self::Failure => Some(FAILURE_TAG),
            Self::Detail => Some(DETAIL_TAG),
            Self::Progress | Self::Plain => None,
        }
    }
}

/// The failure colour, taken from the prototype's own log palette (`!!! ` is the failure marker) and
/// from the inventory's `.test-bad` (#c01c28), so the log's red is the app's red.
pub const FAILURE_COLOUR: &str = "#c01c28";
/// How far a detail is dimmed, as a foreground mixed 45 % towards the log's own background colour: the
/// value §1's "detail dimmed" means here, spelled as a colour because this GTK build has no
/// `fade-alpha` property on `GtkTextTag` (asked for it, GTK answered "Can't find property").
pub const DETAIL_COLOUR: &str = "#6f6f6f";

/// The buffer tag names. `log_link::paint_line` applies them; a test reads them by the same names.
pub const FAILURE_TAG: &str = "log-failure";
pub const DETAIL_TAG: &str = "log-detail";

/// The progress marker, and the failure marker: the first four characters of a line, as §7 spells them.
const PROGRESS_MARK: &str = ">>> ";
const FAILURE_MARK: &str = "!!! ";
/// The detail indent: four spaces, §1's "four-space prefix".
const DETAIL_MARK: &str = "    ";

/// Which level a logged line carries, read off its marker.
pub fn level_of(line: &str) -> Level {
    if line.starts_with(PROGRESS_MARK) {
        Level::Progress
    } else if line.starts_with(FAILURE_MARK) {
        Level::Failure
    } else if line.starts_with(DETAIL_MARK) {
        Level::Detail
    } else {
        Level::Plain
    }
}

/// Its marker is its first four characters and nothing more — §1 names the prefixes as `>>> `, `!!! ` and
/// "four-space", three markers of exactly four characters each — so the message keeps whatever spacing
/// follows them (§7's own link line is written as `">>>   this run's exchanges…"` and its message keeps
/// that indent). A marker line with nothing after the marker answers the empty string rather than the
/// marker.
pub fn body_of(line: &str) -> &str {
    match level_of(line) {
        Level::Progress => &line[PROGRESS_MARK.len()..],
        Level::Failure => &line[FAILURE_MARK.len()..],
        Level::Detail => line.trim_start(),
        Level::Plain => line,
    }
}

/// Make the buffer carry the two level tags. Idempotent, and called before a tag is ever applied so a
/// line is never drawn plain merely because nobody installed them.
pub fn install_tags(buffer: &gtk::TextBuffer) {
    if buffer.tag_table().lookup(FAILURE_TAG).is_none() {
        buffer.create_tag(Some(FAILURE_TAG), &[("foreground", &FAILURE_COLOUR)]);
    }
    if buffer.tag_table().lookup(DETAIL_TAG).is_none() {
        buffer.create_tag(Some(DETAIL_TAG), &[("foreground", &DETAIL_COLOUR)]);
    }
}

/// Draw one line's MESSAGE into `buffer` and tag it with its level. Returns nothing: the caller decides
/// what else the line gets (the exchange-page link tag rides on the same range in `log_link`).
pub fn paint(buffer: &gtk::TextBuffer, line: &str) {
    install_tags(buffer);
    let level = level_of(line);
    let start = buffer.end_iter().offset();
    buffer.insert(&mut buffer.end_iter(), body_of(line));
    buffer.insert(&mut buffer.end_iter(), "\n");
    if let Some(tag) = level.tag() {
        let from = buffer.iter_at_offset(start);
        // The tag covers every buffer line this message occupies, which is one: a log line never
        // contains a newline of its own, the view wraps it visually without adding buffer lines.
        let to = buffer
            .iter_at_line(from.line() + 1)
            .unwrap_or_else(|| buffer.end_iter());
        buffer.apply_tag_by_name(tag, &from, &to);
    }
}
