//! The row-oriented text formats of spec/01-project-and-files.md §6: the transcripts,
//! the merged timeline, the event log, the marks and the finished words.

use std::fs;
use std::path::Path;

/// One line of `transcript.tsv` / `transcript.fixed.tsv`: `start`, `end`, speaker, text.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Line {
    pub start: f64,
    pub end: f64,
    pub speaker: String,
    pub text: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SessionLine {
    pub start: f64,
    pub end: f64,
    pub source: String,
    pub who: String,
    pub text: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct FrameEvent {
    pub start: f64,
    pub end: f64,
    pub text: String,
}

/// One mark of `retakes.tsv`: a stretch said twice.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Retake {
    /// The abandoned words.
    pub s: f64,
    pub e: f64,
    /// Where the attempt that was kept begins; 0 for none.
    pub again: f64,
    pub to: f64,
    pub text: String,
    pub whole: String,
}

/// `%.2f`, which is what every time in these files is written with.
fn secs(v: f64) -> String {
    format!("{v:.2}")
}

fn read_to_string(path: &Path) -> Result<Option<String>, String> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(format!("{}: {err}", path.display())),
    }
}

pub(crate) fn write_rows(path: &Path, text: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| format!("{parent:?}: {err}"))?;
    }
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let part = path.with_file_name(format!("{name}.part"));
    let wrote = fs::write(&part, text).and_then(|()| {
        fs::set_permissions(&part, std::os::unix::fs::PermissionsExt::from_mode(0o644))
    });
    if let Err(err) = wrote {
        let _ = fs::remove_file(&part);
        return Err(format!("{}: {err}", path.display()));
    }
    match fs::rename(&part, path) {
        Ok(()) => Ok(()),
        Err(err) => {
            let _ = fs::remove_file(&part);
            Err(format!("{}: {err}", path.display()))
        }
    }
}

pub(crate) fn write_rows_or_none(path: &Path, text: &str) -> Result<(), String> {
    if !text.trim().is_empty() {
        return write_rows(path, text);
    }
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(format!("{}: {err}", path.display())),
    }
}

/// One row per line of a TSV, blank lines skipped.
fn rows(text: &str) -> impl Iterator<Item = &str> {
    text.lines().filter(|line| !line.trim().is_empty())
}

pub fn write_lines(lines: &[Line], path: &Path) -> Result<(), String> {
    let mut out = String::new();
    for line in lines {
        out.push_str(&format!(
            "{}\t{}\t{}\t{}\n",
            secs(line.start),
            secs(line.end),
            line.speaker,
            line.text
        ));
    }
    write_rows_or_none(path, &out)
}

pub fn read_lines(path: &Path) -> Result<Vec<Line>, String> {
    let Some(text) = read_to_string(path)? else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for row in rows(&text) {
        let fields: Vec<&str> = row.splitn(4, '\t').collect();
        // splitn(4) keeps a tab inside the text with the text, which is why the count is
        // tested as "at least" rather than equal.
        let [start, end, speaker, text] =
            <[&str; 4]>::try_from(fields.as_slice()).map_err(|_| {
                format!(
                    "{}: {row}: expected start, end, speaker and text",
                    path.display()
                )
            })?;
        out.push(Line {
            start: number(start, path, row)?,
            end: number(end, path, row)?,
            speaker: speaker.to_string(),
            text: (*text).to_string(),
        });
    }
    Ok(out)
}

/// Write `session.tsv`: `start\tend\tsource\tspeaker|EVENT\ttext`.
pub fn write_session(lines: &[SessionLine], path: &Path) -> Result<(), String> {
    let mut out = String::new();
    for line in lines {
        out.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\n",
            secs(line.start),
            secs(line.end),
            line.source,
            line.who,
            line.text
        ));
    }
    write_rows_or_none(path, &out)
}

pub fn read_session(path: &Path) -> Result<Vec<SessionLine>, String> {
    let Some(text) = read_to_string(path)? else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for row in rows(&text) {
        let fields: Vec<&str> = row.splitn(5, '\t').collect();
        let [start, end, source, who, text] =
            <[&str; 5]>::try_from(fields.as_slice()).map_err(|_| {
                format!(
                    "{}: {row}: expected start, end, source, speaker and text",
                    path.display()
                )
            })?;
        out.push(SessionLine {
            start: number(start, path, row)?,
            end: number(end, path, row)?,
            source: source.to_string(),
            who: who.to_string(),
            text: (*text).to_string(),
        });
    }
    Ok(out)
}

impl SessionLine {
    /// Is this row something that happened on screen rather than something said?
    pub fn is_event(&self) -> bool {
        self.who == "EVENT"
    }
}

pub fn is_same(text: &str) -> bool {
    let t = text.trim().to_lowercase();
    let t = t.trim_matches(|c: char| ".\"'".contains(c) || c == ' ');
    t == "same" || t == "(same)"
}

pub fn event_row(start: f64, end: f64, text: &str) -> String {
    format!("{}\t{}\t{}", secs(start), secs(end), text)
}

pub fn write_events(events: &[FrameEvent], path: &Path) -> Result<(), String> {
    let mut out = String::new();
    for event in events {
        out.push_str(&event_row(event.start, event.end, &event.text));
        out.push('\n');
    }
    write_rows(path, &out)
}

pub fn read_events(path: &Path) -> Result<Vec<FrameEvent>, String> {
    let Some(text) = read_to_string(path)? else {
        return Ok(Vec::new());
    };
    let mut out: Vec<FrameEvent> = Vec::new();
    for row in rows(&text) {
        let fields: Vec<&str> = row.splitn(3, '\t').collect();
        let (start, end, text): (&str, Option<&str>, &str) = match fields.as_slice() {
            [start, end, text] => (*start, Some(*end), *text),
            [start, text] => (*start, None, *text),
            _ => {
                return Err(format!(
                    "{}: {row}: expected start, end and text",
                    path.display()
                ))
            }
        };
        let start = number(start, path, row)?;
        let end = match end {
            Some(end) => number(end, path, row)?,
            None => start,
        };
        if is_same(text) {
            // Nothing to extend before the first event: a file that opens on a frame
            // where nothing changed has not yet said what it was the same as.
            if let Some(last) = out.last_mut() {
                last.end = end;
            }
            continue;
        }
        out.push(FrameEvent {
            start,
            end,
            text: text.to_string(),
        });
    }
    Ok(out)
}

pub const NO_MARKS: &str = "";

pub fn write_retakes(marks: &[Retake], path: &Path) -> Result<(), String> {
    let mut out = String::new();
    for m in marks {
        out.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}",
            secs(m.s),
            secs(m.e),
            secs(m.again),
            secs(m.to),
            m.text
        ));
        if !m.whole.is_empty() {
            out.push_str(&format!("\t{}", m.whole));
        }
        out.push('\n');
    }
    write_rows(path, &out)
}

pub fn read_retakes(path: &Path) -> Result<Vec<Retake>, String> {
    let Some(text) = read_to_string(path)? else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for row in rows(&text) {
        let f: Vec<&str> = row.split('\t').collect();
        if f.len() < 3 {
            continue;
        }
        let (Ok(s), Ok(e), Ok(again)) = (
            f[0].parse::<f64>(),
            f[1].parse::<f64>(),
            f[2].parse::<f64>(),
        ) else {
            continue;
        };
        let mut mark = Retake {
            s,
            e,
            again,
            to: e,
            text: String::new(),
            whole: String::new(),
        };
        match f.len() {
            3 => {}
            4 => mark.text = f[3].to_string(),
            // A fifth column is where the removal ends; a file with four was written
            // before the edges were placed, and removes the words alone.
            _ => {
                if let Ok(to) = f[3].parse::<f64>() {
                    if to > 0.0 {
                        mark.to = to;
                    }
                }
                mark.text = f[4].to_string();
                if f.len() > 5 {
                    mark.whole = f[5].to_string();
                }
            }
        }
        out.push(mark);
    }
    Ok(out)
}

/// The join marker of `final.txt`: `|cut N|` where N words went, `|cut|` where none did.
fn cut_mark(gone: usize) -> String {
    if gone == 0 {
        "|cut|".to_string()
    } else {
        format!("|cut {gone}|")
    }
}

pub fn cut_marker(gone: usize, whole: bool) -> String {
    let base = cut_mark(gone);
    if whole {
        format!("{base}whole take|")
    } else {
        base
    }
}

/// The words of the finished video as written, with a marker at every join (§6).
pub fn write_final(words: &[String], joins: &[usize]) -> String {
    let mut out = String::new();
    for (n, word) in words.iter().enumerate() {
        if n > 0 {
            out.push(' ');
            if let Some(gone) = joins.get(n - 1) {
                out.push_str(&cut_mark(*gone));
                out.push(' ');
            }
        }
        out.push_str(word);
    }
    out
}

pub fn read_final(text: &str) -> (Vec<String>, Vec<usize>) {
    let mut words = Vec::new();
    let mut joins = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        if rest.starts_with([' ', '\t', '\n']) {
            rest = rest[1..].trim_start();
            continue;
        }
        if let Some(len) = marker_len(rest) {
            joins.push(cut_count(&rest[..len]));
            rest = &rest[len..];
            continue;
        }
        let end = rest.find([' ', '\t', '\n']).unwrap_or(rest.len());
        words.push(rest[..end].to_string());
        rest = &rest[end..];
    }
    (words, joins)
}

fn marker_len(text: &str) -> Option<usize> {
    let after = text.strip_prefix('|')?;
    let rest = after.strip_prefix("cut")?;
    // `|cut N|`: a space, a count, a closing pipe.
    if let Some(inner) = rest.strip_prefix(' ') {
        let end = inner.find('|')?;
        return inner[..end]
            .trim()
            .parse::<usize>()
            .ok()
            .map(|_| "|cut ".len() + end + 1);
    }
    // `|cut|`, and every further field glued to it without a space. A field runs from its
    // opening pipe to the next one, spaces inside included — which is exactly why the words
    // of the longer mark are not words.
    let mut end = "|cut|".len();
    let mut tail = rest.strip_prefix('|')?;
    while !tail.is_empty() && !tail.starts_with([' ', '\t', '\n']) {
        end += tail.find('|')? + 1;
        tail = &rest[end - "|cut|".len()..];
    }
    Some(end)
}

fn cut_count(marker: &str) -> usize {
    let inner = marker
        .strip_prefix('|')
        .and_then(|m| m.strip_suffix('|'))
        .unwrap_or_default();
    inner
        .strip_prefix("cut")
        .map(str::trim)
        .filter(|rest| !rest.is_empty())
        .and_then(|rest| rest.parse::<usize>().ok())
        .unwrap_or(0)
}

/// A time column: a number, or an error naming the file and the row.
fn number(field: &str, path: &Path, row: &str) -> Result<f64, String> {
    field
        .parse::<f64>()
        .map_err(|_| format!("{}: {row}: {field:?} is not a time", path.display()))
}
