//! §03-shell.md §1: the log's level comes off the line and shows as a colour, not as a prefix.

use adw::prelude::*;
use gtk4 as gtk;

/// The four levels §1 names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Progress,
    Failure,
    Detail,
    Plain,
}

impl Level {
    /// The buffer tag a line of this level is drawn with, or `None` for the plain case: a progress
    /// line and an unmarked line take the log view's own foreground, which is what keeps the log
    /// readable in a dark theme rather than painted with a hardcoded grey.
    pub fn tag(self) -> Option<&'static str> {
        match self {
            Self::Failure => Some(FAILURE_TAG),
            Self::Detail => Some(DETAIL_TAG),
            Self::Progress | Self::Plain => None,
        }
    }
}

pub const FAILURE_COLOUR: &str = "#c01c28";
/// How far a detail is dimmed, as a foreground mixed 45 % towards the log's own background colour:
/// the value §1's "detail dimmed" means here, spelled as a colour because this GTK build has no
/// `fade-alpha` property on `GtkTextTag` (asked for it, GTK answered "Can't find property").
pub const DETAIL_COLOUR: &str = "#6f6f6f";

pub const FAILURE_TAG: &str = "log-failure";
pub const DETAIL_TAG: &str = "log-detail";

/// The progress marker, and the failure marker: the first four characters of a line, as §7 spells
/// them.
const PROGRESS_MARK: &str = ">>> ";
const FAILURE_MARK: &str = "!!! ";
/// The detail indent: four spaces, §1's "four-space prefix".
const DETAIL_MARK: &str = "    ";

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

/// Its marker is its first four characters and nothing more — §1 names the prefixes as `>>> `, `!!!
pub fn body_of(line: &str) -> &str {
    match level_of(line) {
        Level::Progress => &line[PROGRESS_MARK.len()..],
        Level::Failure => &line[FAILURE_MARK.len()..],
        Level::Detail => line.trim_start(),
        Level::Plain => line,
    }
}

/// Make the buffer carry the two level tags. Idempotent, and called before a tag is ever applied so
/// a line is never drawn plain merely because nobody installed them.
pub fn install_tags(buffer: &gtk::TextBuffer) {
    if buffer.tag_table().lookup(FAILURE_TAG).is_none() {
        buffer.create_tag(Some(FAILURE_TAG), &[("foreground", &FAILURE_COLOUR)]);
    }
    if buffer.tag_table().lookup(DETAIL_TAG).is_none() {
        buffer.create_tag(Some(DETAIL_TAG), &[("foreground", &DETAIL_COLOUR)]);
    }
}

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
