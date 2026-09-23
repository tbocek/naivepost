// §07-narrate#6's reply rules are the module's own: an entry is matched forward-only, so a repeat of a
// clip already passed matches nothing and is refused with its bounds named.
pub const ENTRY_MATCH_TOLERANCE_SECONDS: f64 = 0.5;

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
