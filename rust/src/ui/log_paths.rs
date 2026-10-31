//! §1's last sentence: "New: every path the log names is a link to it."
//!
//! `super::log_link` owns WHICH line opens and HOW; this module owns the reading rule, which words in a log
//! message name a file, with no widget or session. A path token is:
//!
//! * absolute: starts with `/`;
//! * project-rooted: starts with `project:` (the one path rule of `01-project-and-files.md`);
//! * root-relative: its FIRST segment is one of [`ROOTS`], or the whole token is `requests.tsv`.
//!
//! A known first segment keeps prose out of the links: `h264/vp9`, `0.5/1.0` and `mm:ss.s` are not paths, and
//! a bare dotted word (`audio.cpp`) has no slash and stays plain text.

/// The project's own folder names (`01-project-and-files.md` §3); a root-relative token must start under one.
pub const ROOTS: [&str; 10] = [
    "llm", "prepare", "cut", "narrate", "produce", "sources", "stems", "cache", "inputs", "assets",
];

/// The one project FILE that may appear root-relative with no folder in front (§09 §10's request log).
pub const LOOSE_FILE: &str = "requests.tsv";

/// The characters a token may touch and still end there: whitespace plus the punctuation the log's sentences
/// put around a path ("(produce/final.mp4)", "`narrate/narration.json`"). `:` is NOT one, since splitting on
/// it would break `project:` + root-relative paths.
fn is_delimiter(c: char) -> bool {
    c.is_whitespace()
        || matches!(
            c,
            ',' | '(' | ')' | '[' | ']' | '"' | '\u{201c}' | '\u{201d}' | '\u{2014}' | ';'
        )
}

/// Whether one token, as written, names a path. See the module docs for the three shapes.
pub fn looks_like_path(token: &str) -> bool {
    if token.is_empty() {
        return false;
    }
    if token.starts_with('/') || token.starts_with("project:") {
        // An absolute or `project:`-rooted token needs something after the root and a SECOND `/` segment: a bare
        // "/health" or "/v1/models" is a server endpoint in a sentence, not a project file (the app names its own
        // files with two or more segments, or as a full path under a folder it opened).
        let tail = token.strip_prefix("project:").unwrap_or(token);
        let body = tail.trim_matches('/');
        return !body.is_empty() && body.contains('/');
    }
    if token == LOOSE_FILE {
        return true;
    }
    let mut parts = token.split('/');
    let Some(first) = parts.next() else {
        return false;
    };
    // A known root AND a second part: `produce/` alone is a folder with no file, `cache` alone a word.
    ROOTS.contains(&first) && parts.next().is_some()
}

/// The path tokens a log message names, in order of appearance, de-duplicated.
///
/// Asked of the MESSAGE (what the log draws): `super::log_level::body_of` has already taken the marker off.
pub fn paths_of(message: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for token in split_tokens(message) {
        if looks_like_path(token) && !found.iter().any(|seen| seen == token) {
            found.push(token.to_string());
        }
    }
    found
}

/// The message cut into candidate tokens on the delimiters above, shared by [`paths_of`] and `log_link`'s span search.
pub fn split_tokens(message: &str) -> Vec<&str> {
    message
        .split(is_delimiter)
        .map(|piece| piece.trim_matches('`'))
        .filter(|piece| !piece.is_empty())
        .collect()
}

/// Where each named path sits in `message`, as byte offsets `(start, end)`, the spans `paint_line` tags.
///
/// Offsets rather than a re-search, since a token may repeat inside one line and the de-duplicated
/// [`paths_of`] would flatten it.
pub fn path_spans(message: &str) -> Vec<(usize, usize)> {
    let mut spans: Vec<(usize, usize)> = Vec::new();
    // `split` drops each delimiter, so a piece begins one byte past the delimiter that ended the previous one.
    // Byte offsets are what a text buffer tags by, so they are walked over the raw pieces, never recomputed from trimmed text.
    let mut at = 0usize;
    for (index, piece) in message.split(is_delimiter).enumerate() {
        let lead = if index == 0 { 0 } else { 1 };
        let open = piece.len() - piece.trim_start_matches(['`', '\u{201c}']).len();
        let word = piece
            .trim_start_matches(['`', '\u{201c}'])
            .trim_end_matches(['`', '\u{201d}']);
        let start = at + lead + open;
        if looks_like_path(word) && !spans.iter().any(|(from, _)| *from == start) {
            spans.push((start, start + word.len()));
        }
        at += lead + piece.len();
    }
    spans.sort_by_key(|(from, _)| *from);
    spans
}

/// The URI a path token opens: `project:x/y` and a root-relative token under the session folder, an
/// absolute token as it stands.
///
/// Nothing touches the disk: a line may name a file a failed step never wrote (§10 wants the request on file
/// even when it died); whether it opens is `log_link::activate_line`'s report.
pub fn resolve(token: &str, session_dir: &std::path::Path) -> String {
    let path = if let Some(rel) = token.strip_prefix("project:") {
        session_dir.join(rel.trim_start_matches('/'))
    } else if token.starts_with('/') {
        std::path::PathBuf::from(token)
    } else {
        session_dir.join(token)
    };
    format!("file://{}", path.display())
}
