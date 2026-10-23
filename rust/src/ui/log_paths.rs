//! §1's last sentence: "New: every path the log names is a link to it."
//!
//! `super::log_link` owns WHICH line opens and HOW it is opened; this module owns only the reading rule —
//! which words inside a log message name a file. It has no widget and no session in sight, so the rule can
//! be asked about any line the log has ever printed. `log_link::paint_line` asks it for the spans to tag
//! and `log_link::activate_line` asks it for what to open when a line is not §7's exchange page.
//!
//! The shape of a path token (§1 says "every path", not "every filename", and the log spells its paths
//! three ways — absolute, `project:`-rooted per the one path rule of `01-project-and-files.md`, and
//! root-relative under a project folder):
//!
//! * absolute: starts with `/`;
//! * project-rooted: starts with `project:`;
//! * root-relative: its FIRST `/`-separated segment is one of [`ROOTS`] (the folders §01's tree lists),
//!   or the whole token is `requests.tsv`.
//!
//! Requiring a known first segment is what keeps prose out of the log's links: `h264/vp9`, `0.5/1.0` and
//! `mm:ss.s` all carry a slash and none is a path, while `produce/final.mp4` and `llm/0307-142201.html`
//! are exactly what the log names. A bare word with dots in it (`audio.cpp`, `naivepost.json`) is not a
//! path either — it has no slash and is not absolute, so it stays plain text.

/// The project's own folder names, from `01-project-and-files.md` §3's tree. A root-relative token is a
/// path only when it starts under one of these.
pub const ROOTS: [&str; 10] = [
    "llm", "prepare", "cut", "narrate", "produce", "sources", "stems", "cache", "inputs", "assets",
];

/// The one project FILE that may appear root-relative with no folder in front of it (§09 §10's request log).
pub const LOOSE_FILE: &str = "requests.tsv";

/// The characters a token may touch and still end there: whitespace plus the punctuation the log's own
/// sentences put around a path ("opened /tmp/x.naivepost", "(produce/final.mp4)", "`narrate/narration.json`").
/// `:` is NOT one of them even though prose uses it, because the project's own path spelling is
/// `project:` + root-relative (`01-project-and-files.md`'s one path rule) and splitting on the colon
/// would turn that path into two words and lose its root.
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
        // An absolute or `project:`-rooted token still has to have something after the root, or the log's
        // own prose ("see project:", "no / in that name") would become a link. And it needs a SECOND `/`
        // segment: a bare "/health", "/v1/models" or "/embeddings" is an endpoint path inside a sentence
        // about a server, not one of the project's files — §1 links the paths the log NAMES, and the app
        // only ever names its own files with two or more segments (`produce/final.mp4`,
        // `llm/0307-142201-Describe.html`) or as a full path under a folder it opened.
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
    // Both a known root AND a second part: `produce/` alone is a folder the log points at with no file,
    // and `cache` alone is a word in a sentence.
    ROOTS.contains(&first) && parts.next().is_some()
}

/// The path tokens a log message names, in order of appearance, de-duplicated.
///
/// Asked of the MESSAGE (what the log draws), not of the recorded line: `super::log_level::body_of` has
/// already taken the `>>> `/`!!! `/four-space marker off, and §1 draws the message alone.
pub fn paths_of(message: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for token in split_tokens(message) {
        if looks_like_path(token) && !found.iter().any(|seen| seen == token) {
            found.push(token.to_string());
        }
    }
    found
}

/// The message cut into candidate tokens on the delimiters above. Split out so [`paths_of`] and the span
/// search in `log_link` agree about where a token begins and ends.
pub fn split_tokens(message: &str) -> Vec<&str> {
    message
        .split(is_delimiter)
        .map(|piece| piece.trim_matches('`'))
        .filter(|piece| !piece.is_empty())
        .collect()
}

/// Where each named path sits in `message`, as byte offsets `(start, end)` — the spans `paint_line` tags.
///
/// Offsets rather than a re-search by the caller because the caller must tag the DRAWN text, and a token
/// may repeat inside one line (two files of the same folder), which the de-duplicated [`paths_of`] would
/// flatten.
pub fn path_spans(message: &str) -> Vec<(usize, usize)> {
    let mut spans: Vec<(usize, usize)> = Vec::new();
    // `split` drops each delimiter, so a piece begins one byte past the delimiter that ended the previous
    // one (none before the first piece). Byte offsets are what a text buffer tags by, so they are walked
    // over the raw pieces and never recomputed from trimmed text.
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

/// The URI a path token opens: `project:x/y` under the session folder, a root-relative token under the
/// session folder too, an absolute token as it stands.
///
/// Nothing here touches the disk. A log line may name a file a failed step never wrote — §10 wants the
/// request on file even when it died, and its line names the output — and the link still exists; whether
/// it opens is the opener's answer, reported by `log_link::activate_line`.
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
