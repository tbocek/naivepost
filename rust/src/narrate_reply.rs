// §07-narrate#6's reply rules are the module's own: an entry is matched forward-only, so a repeat of a
// clip already passed matches nothing and is refused with its bounds named.
use serde::Deserialize;

pub const ENTRY_MATCH_TOLERANCE_SECONDS: f64 = 0.5;

/// One entry as the model echoed it (§F4.2's `{start,end,at,text,emotion}`).
///
/// A local struct rather than [`crate::narration::Entry`] because the two differ where it matters: an
/// `Entry` stores its bounds as the clip's own (copied verbatim from the cut) and is what a saved file
/// holds, while this one carries what the model *echoed*, which is exactly the thing being checked —
/// a match within [`ENTRY_MATCH_TOLERANCE_SECONDS`] turns one into the other.
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

/// Read the entries out of whatever shape the server answered with.
///
/// Three shapes arrive from real servers and all three are answering the same question, so refusing
/// two of them would cost a whole attempt for a wrapper nobody asked for: the prototype's
/// `{"entries":[...]}` document (§F4.2), the same list as a bare top-level array, and a single bare
/// entry object when the writer only had one clip to answer.
///
/// The `Err` says what came back instead, quoted, because "the reply was not the entries shape" on its
/// own tells a person nothing about a model that answered in prose or an endpoint that returned HTML.
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

/// The first [`SHAPE_QUOTE_CHARS`] characters of an answer, on one line and in quotes, for a refusal.
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

/// An entry whose echoed bounds match no clip — or match one the reply already passed. Forward-only is the
/// whole of the ordering rule: a reply that walks back to an earlier clip is refused, never quietly
/// re-sorted, because silently reordering lines is how a narration ends up spoken out of order.
pub fn unmatched_entry_fault(start: f64, end: f64) -> String {
    format!("an entry says {start}-{end}, which matches no clip (or is out of order)")
}

/// Every clip answered with an empty line: a model that refused the job rather than one exercising taste.
pub fn all_silent_fault() -> String {
    crate::narrate_pass::no_line_fault()
}

/// A clip nobody answered at all — the same fault [`crate::narrate_pass::missing_clip_fault`] names, kept
/// here so §6's wording is pinned in one file. Bounds are echoed back because they are what the reply was
/// asked to copy.
pub fn clip_without_entry(clip: u32, segs: &[crate::cut::Seg]) -> String {
    match segs.get(clip as usize) {
        Some(seg) => crate::narrate_pass::missing_clip_fault(clip + 1, seg.s, seg.e),
        None => format!("clip {} got no entry", clip + 1),
    }
}

/// Where an entry lands inside its clip when the reply gives no `at`: the head of the clip. A reply that
/// leaves the moment out has not proposed one, and inventing a later slot would move a line the model
/// placed at the cut — so this returns the clip-relative zero the writer already asks for (§00: the app
/// does the arithmetic, but it does not second-guess a placement that was never offered).
pub fn entry_at(entry: &crate::narration::Entry, _seg: &crate::cut::Seg) -> f64 {
    if entry.at > 0.0 {
        return entry.at;
    }
    0.0
}
