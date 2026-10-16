//! §12 entry 19's second half: the clamp's drop count becomes a line the user can act on.
//!
//! [`crate::cut_clamp`] keeps working out WHICH effects the cut leaves without a home; this module only
//! says them. It is a separate file because `clamp_to_cut` runs from several places (the suggest walk-back,
//! the page's clamp verb) and the sentence belongs with none of them — and because `src/ui/window.rs` is far
//! past its size budget and must not grow for a log line.

use crate::cut::Fx;
use crate::cut_clamp::Clamped;

/// How many dropped effects are named before the line gives up and counts the rest. A caption run can drop
/// dozens, and a log the user has to scroll past is worse than a bare count; the cap keeps the row one screen
/// tall while still naming everything the person is likely to have placed by hand, which is what they are
/// looking for.
const NAMED_MAX: usize = 6;

/// One dropped effect as the log names it: the kind word the dropdown and the status line already use
/// ([`crate::fx_record::label_of`], so no second vocabulary), spelled lower case inside the sentence, plus
/// where on the cut it was.
fn describe(fx: &Fx) -> String {
    let kind = match fx.effect_kind() {
        // A stop is a speed whose rate is 0 (§1's column), and §9 refuses it as a STOP, so the log says stop.
        Some(_) if crate::effect_overlap::is_a_stop(fx) => "stop".to_string(),
        Some(kind) => crate::fx_record::label_of(kind).to_lowercase(),
        // An effect whose kind this build does not know still went, and saying `unknown effect` is truer
        // than dropping it from the list the count claims to explain.
        None => format!("{} effect", fx.kind),
    };
    format!("{kind} at {}", crate::tools::mm_ss(fx.t))
}

/// The count sentence on its own — §F3.12's line, byte for byte, with nothing appended. Kept separate from
/// [`dropped_names`] because the page logs the two rows one after the other: §F3.12 quotes the count row
/// verbatim and tests compare whole strings, so folding the names into it would either break them or force a
/// weaker comparison. Past [`NAMED_MAX`] the remainder is counted, not listed, so a caption run cannot bury
/// the rest of the log.
///
/// `None` when nothing was dropped: a clean clamp stays silent, exactly as the count-only line did.
pub fn dropped_log(clamped: &Clamped) -> Option<String> {
    if clamped.dropped == 0 {
        return None;
    }
    Some(crate::cut_clamp::log_line(clamped.dropped))
}

/// The second row — the names, which is what §12 entry 19 added. `None` when nothing was dropped (a clean
/// clamp stays silent) or when drops happened without a note, which cannot happen from `clamp_to_cut` and is
/// answered quietly rather than by printing an empty row.
pub fn dropped_names(clamped: &Clamped) -> Option<String> {
    if clamped.dropped == 0 || clamped.dropped_lines.is_empty() {
        return None;
    }
    let mut tail = String::new();
    let mut named: Vec<&str> = Vec::with_capacity(NAMED_MAX);
    for (index, note) in clamped.dropped_lines.iter().enumerate() {
        if index < NAMED_MAX {
            named.push(note.as_str());
        } else if tail.is_empty() {
            tail = format!("... and {} more", clamped.dropped_lines.len() - NAMED_MAX);
        }
    }
    if !tail.is_empty() {
        named.push(tail.as_str());
    }
    Some(named.join(", "))
}

/// The note `clamp_to_cut` files beside each drop, so the wording lives here rather than in the loop that
/// decides drops. Kept in this module so the sentence and the line that prints it cannot drift apart.
pub fn note(fx: &Fx) -> String {
    describe(fx)
}
