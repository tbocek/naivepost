//! The row-oriented text formats of spec/01-project-and-files.md §6: the transcripts,
//! the merged timeline, the event log, the marks and the finished words.
//!
//! Pure data — reading and writing rows, no UI and no pipeline. Times go out with two
//! decimals (§6's `%.2f`), which is what makes a re-write of an unchanged file byte-for-byte
//! the same file.

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

/// One line of `session.tsv`: the same shape with a source and, in the speaker's place,
/// either a speaker or the word `EVENT` — which is what marks a picture event rather than
/// somebody talking ([`SessionLine::is_event`]). Everything in it is on the session clock.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SessionLine {
    pub start: f64,
    pub end: f64,
    pub source: String,
    pub who: String,
    pub text: String,
}

/// One line of `events.tsv`: what happened at one frame, and until when it still read as
/// the same picture.
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
    /// Where what the cut removes ends: `e` when the words are all that goes, the
    /// retake's own onset when everything up to it goes with them.
    pub to: f64,
    pub text: String,
    /// The recordings this mark takes out entire, base names comma-separated; empty or
    /// absent when none.
    pub whole: String,
}

/// `%.2f`, which is what every time in these files is written with.
fn secs(v: f64) -> String {
    format!("{v:.2}")
}

/// Read a whole file, treating "not there" as no rows: a project that has not run the
/// step yet has no file, which §6 does not call an error.
fn read_to_string(path: &Path) -> Result<Option<String>, String> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(format!("{}: {err}", path.display())),
    }
}

/// Write rows the way §6 spells them, through the caller's `path`. The file is written
/// directly rather than through [`crate::layout::Tree::write_file`] because these formats
/// are read and written by whatever step owns them, at paths that outlive one project
/// folder; the mode is set after the write for the same reason §1 gives — a new file's
/// creation mode is masked by the umask.
pub(crate) fn write_rows(path: &Path, text: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| format!("{}: {err}", parent.display()))?;
    }
    fs::write(path, text).map_err(|err| format!("{}: {err}", path.display()))?;
    std::fs::set_permissions(
        path,
        std::os::unix::fs::PermissionsExt::from_mode(0o644),
    )
    .map_err(|err| format!("{}: {err}", path.display()))
}

/// One row per line of a TSV, blank lines skipped.
fn rows(text: &str) -> impl Iterator<Item = &str> {
    text.lines().filter(|line| !line.trim().is_empty())
}

// --- transcript.tsv / transcript.fixed.tsv ------------------------------------

/// Write `transcript.tsv` (or a `*.fixed.tsv`): `start\tend\tspeaker\ttext`. A text is
/// written verbatim, tabs included — one row per line, never re-flowed.
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
    write_rows(path, &out)
}

/// Read `transcript.tsv`. A row with fewer than the four columns is corruption rather
/// than an empty transcript: a half-written row means the file was cut off, and reading
/// it as no words would delete speech from the page.
pub fn read_lines(path: &Path) -> Result<Vec<Line>, String> {
    let Some(text) = read_to_string(path)? else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for row in rows(&text) {
        let fields: Vec<&str> = row.splitn(4, '\t').collect();
        // splitn(4) keeps a tab inside the text with the text, which is why the count is
        // tested as "at least" rather than equal.
        let [start, end, speaker, text] = <[&str; 4]>::try_from(fields.as_slice())
            .map_err(|_| format!("{}: {row}: expected start, end, speaker and text", path.display()))?;
        out.push(Line {
            start: number(start, path, row)?,
            end: number(end, path, row)?,
            speaker: speaker.to_string(),
            text: (*text).to_string(),
        });
    }
    Ok(out)
}

// --- session.tsv ---------------------------------------------------------------

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
    write_rows(path, &out)
}

/// Read `session.tsv`. A row with fewer than the five columns is an error naming the
/// path, as for [`read_lines`].
pub fn read_session(path: &Path) -> Result<Vec<SessionLine>, String> {
    let Some(text) = read_to_string(path)? else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for row in rows(&text) {
        let fields: Vec<&str> = row.splitn(5, '\t').collect();
        let [start, end, source, who, text] = <[&str; 5]>::try_from(fields.as_slice())
            .map_err(|_| {
                format!("{}: {row}: expected start, end, source, speaker and text", path.display())
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

// --- events.tsv ----------------------------------------------------------------

/// A frame's line saying nothing changed. The prototype trims whitespace and trailing
/// `.`/quotes and lower-cases before comparing, and accepts the older `(same)`
/// (gui/describe.go:336-339) — a file written then still means the same thing now.
pub fn is_same(text: &str) -> bool {
    let t = text.trim().to_lowercase();
    let t = t.trim_matches(|c: char| ".\"'".contains(c) || c == ' ');
    t == "same" || t == "(same)"
}

/// Write `events.tsv`, one row per frame as given — the folding is a read-side rule
/// ([`read_events`]), so a file keeps every frame it was written with.
pub fn write_events(events: &[FrameEvent], path: &Path) -> Result<(), String> {
    let mut out = String::new();
    for event in events {
        out.push_str(&format!(
            "{}\t{}\t{}\n",
            secs(event.start),
            secs(event.end),
            event.text
        ));
    }
    write_rows(path, &out)
}

/// Read `events.tsv`, folding each `same` row into the row before it by extending that
/// row's end (§6). Two frames in a row of nothing changing extend it twice, so what comes
/// back is one event per thing that happened, dated from when it started to the last
/// second it still held.
pub fn read_events(path: &Path) -> Result<Vec<FrameEvent>, String> {
    let Some(text) = read_to_string(path)? else {
        return Ok(Vec::new());
    };
    let mut out: Vec<FrameEvent> = Vec::new();
    for row in rows(&text) {
        let fields: Vec<&str> = row.splitn(3, '\t').collect();
        let [start, end, text] = <[&str; 3]>::try_from(fields.as_slice())
            .map_err(|_| format!("{}: {row}: expected start, end and text", path.display()))?;
        let (start, end) = (number(start, path, row)?, number(end, path, row)?);
        if is_same(text) {
            // Nothing to extend before the first event: a file that opens on a frame
            // where nothing changed has not yet said what it was the same as.
            if let Some(last) = out.last_mut() {
                last.end = end;
            }
            continue;
        }
        out.push(FrameEvent { start, end, text: text.to_string() });
    }
    Ok(out)
}

// --- retakes.tsv ---------------------------------------------------------------

/// Write `retakes.tsv`: `S\tE\tAgain\tTo\tText`, plus a sixth column only when there is
/// something to say about whole takes (§6: "empty or absent when none").
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

/// Read `retakes.tsv`, accepting the 3- to 6-column files §6 allows and the prototype's
/// reader accepts (gui/retake.go:371-402). A line that is too short or whose first three
/// columns are not numbers is skipped rather than failing the file — a mark written by
/// hand with a typo costs that mark, not every other mark.
///
/// The shapes, oldest first: 3 columns predate the placed edges; 4 was written before the
/// edges were placed and removes the words alone (its fourth column is the text); from 5
/// on `To` is its own column and a `To` of nought means "the words alone", so it reads
/// back as `E`.
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
        // The prototype parses the first three and drops the line if any fails.
        let (Ok(s), Ok(e), Ok(again)) = (f[0].parse::<f64>(), f[1].parse::<f64>(), f[2].parse::<f64>())
        else {
            continue;
        };
        let mut mark = Retake { s, e, again, to: e, text: String::new(), whole: String::new() };
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

// --- final.txt -----------------------------------------------------------------

/// The join marker of `final.txt`: `|cut N|` where N words went, `|cut|` where none did.
fn cut_mark(gone: usize) -> String {
    if gone == 0 {
        "|cut|".to_string()
    } else {
        format!("|cut {gone}|")
    }
}

/// The words of the finished video as written, with a marker at every join (§6).
///
/// `joins` is one entry per transition between the words passed — so its length is
/// `words.len() - 1`, and an entry of nought is a join where nothing went. A caller that
/// only knows where words were removed passes zeros everywhere else, which writes the
/// bare `|cut|` §6 spells out.
///
/// The prototype's third spelling — a mark carrying that a whole take went — is 08's rule
/// and is deliberately not produced here; the reader below still recognises it and reads
/// no words out of it, so such a file loses nothing either way.
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

/// Read `final.txt` back: the surviving words with every marker taken out, and the counts
/// the markers carried.
///
/// A marker is recognised by its shape — a pipe, then `cut`, then optionally a number,
/// then a pipe — rather than by "it contains a pipe", so that a word of a marked join does
/// not have to be guessed at: the prototype's longer mark (`|cut|whole take|`) has fields
/// separated by spaces inside it, and treating those as words would put `whole` and `take`
/// on the page. Anything else holding a pipe is left where it is, since dropping text that
/// was not a marker is worse than keeping an odd word.
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

/// How many bytes the marker at the start of `text` runs to. Two shapes: the two §6 names
/// (`|cut|`, `|cut 10|`) and the prototype's longer one, which is a bare `|cut|` followed
/// with no space by another `|…|` field whose own text may hold spaces —
/// `|cut|whole take|`. `None` when this is not a marker.
fn marker_len(text: &str) -> Option<usize> {
    let after = text.strip_prefix('|')?;
    let rest = after.strip_prefix("cut")?;
    // `|cut N|`: a space, a count, a closing pipe.
    if let Some(inner) = rest.strip_prefix(' ') {
        let end = inner.find('|')?;
        return inner[..end].trim().parse::<usize>().ok().map(|_| "|cut ".len() + end + 1);
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

/// The number a marker carried: `|cut 10|` is ten, `|cut|` nought. A marker of another
/// shape — the whole-take spelling carries words, not a count — says nought and stays a
/// join, because something did go there.
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
    field.parse::<f64>().map_err(|_| {
        format!("{}: {row}: {field:?} is not a time", path.display())
    })
}
