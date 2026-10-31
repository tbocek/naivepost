// §07-narrate#6's reply rules are the module's own: an entry is matched forward-only, so a repeat
// of a clip already passed matches nothing and is refused with its bounds named.
use serde::Deserialize;

pub const ENTRY_MATCH_TOLERANCE_SECONDS: f64 = 0.5;

/// One entry as the model echoed it (§F4.2's `{start,end,at,text,emotion}`).
#[derive(Debug, Clone, Deserialize)]
pub struct PlainEntry {
    pub start: f64,
    pub end: f64,
    #[serde(default)]
    pub at: f64,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub emotion: String,
}

/// How much of an unreadable answer is quoted back in the refusal: enough to see what came instead
/// (an apology, an error page, half a document) without pasting a whole transcript into the log.
const SHAPE_QUOTE_CHARS: usize = 120;

pub fn entries_of(answer: &str) -> Result<Vec<PlainEntry>, String> {
    let trimmed = answer.trim();
    if let Ok(body) = serde_json::from_str::<Vec<PlainEntry>>(trimmed) {
        return Ok(body);
    }
    if let Ok(body) = serde_json::from_str::<PlainEntry>(trimmed) {
        return Ok(vec![body]);
    }
    #[derive(Deserialize)]
    struct Wrapped {
        entries: Vec<PlainEntry>,
    }
    if let Ok(wrapped) = serde_json::from_str::<Wrapped>(trimmed) {
        return Ok(wrapped.entries);
    }
    Err(format!(
        "expected a list of entries, an {{\"entries\":[...]}} document or one entry object; got {}",
        quote(trimmed)
    ))
}

fn quote(answer: &str) -> String {
    let flat: String = answer
        .chars()
        .take(SHAPE_QUOTE_CHARS)
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .collect();
    let more = if answer.chars().count() > SHAPE_QUOTE_CHARS {
        "…"
    } else {
        ""
    };
    format!("{flat:?}{more}")
}

pub fn unmatched_entry_fault(start: f64, end: f64) -> String {
    format!("an entry says {start}-{end}, which matches no clip (or is out of order)")
}

/// A clip nobody answered at all — the same fault [`crate::narrate_pass::missing_clip_fault`]
/// names, kept here so §6's wording is pinned in one file. Bounds are echoed back because they are
/// what the reply was asked to copy.
pub fn clip_without_entry(clip: u32, segs: &[crate::cut::Seg]) -> String {
    match segs.get(clip as usize) {
        Some(seg) => crate::narrate_pass::missing_clip_fault(clip + 1, seg.s, seg.e),
        None => format!("clip {} got no entry", clip + 1),
    }
}

pub fn entry_at(entry: &crate::narration::Entry, _seg: &crate::cut::Seg) -> f64 {
    if entry.at > 0.0 {
        return entry.at;
    }
    0.0
}
