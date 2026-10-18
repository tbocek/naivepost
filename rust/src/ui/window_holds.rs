//! §F2.4 S1/S1b/S2/S3 / §F2.9 — what the hand holds and which picture row this window watches:
//! the effect / clip / edge slots, the pick that fills them, and the watched-row read and write. Split out
//! of `window.rs`, which is past its size budget; its doors are re-exported through `window`, so no
//! caller's path changed.

use crate::cut::{self};
use crate::cut_line;

thread_local! {
    /// F2.4 S1: which picture row this window is watching. Only a picture-band click sets it, and only
    /// while nothing plays with sources loaded — `cut_line::watches_row` owns that; this just remembers.
    static WATCHED_ROW: std::cell::RefCell<Option<usize>> =
        const { std::cell::RefCell::new(None) };
}

/// What ⌦ would take first: the effect in hand. Set by the effects-lane rounds when a person picks one
/// up; a seam today so the delete order is testable before those pixels exist.
///
/// `None` puts the effect down and nothing else ([`crate::ui::cut_hand::drop_effect`]): the door that deleted
/// the effect does not also decide what the next ⌦ takes.
pub fn set_held_effect(effect: Option<cut::Fx>) {
    match effect {
        Some(fx) => {
            crate::ui::cut_hand::pick_up(crate::ui::cut_hand::Hand::Effect(fx));
        }
        None => crate::ui::cut_hand::clear_all(),
    };
}

/// Read side of [`set_held_effect`].
pub fn held_effect() -> Option<cut::Fx> {
    crate::ui::cut_hand::held_effect()
}

/// What ⌦ takes next: the clip in hand — `the only way to remove a spliced card` (§J rule 8). Same
/// seam arrangement as the held effect: real drags set it from F2.8/F2.9 onward.
pub fn set_held_clip(seg: Option<cut::Seg>) {
    match seg {
        Some(seg) => {
            crate::ui::cut_hand::pick_up(crate::ui::cut_hand::Hand::Clip(seg));
        }
        None => crate::ui::cut_hand::clear_all(),
    };
}

/// Read side of [`set_held_clip`]: the hand holds a WHOLE clip, as opposed to one of its edges.
pub fn held_clip() -> Option<cut::Seg> {
    crate::ui::cut_hand::held_clip()
}

/// The segment in hand either way — whole clip or held edge — for the doors that move a segment without asking
/// which of the two the hand took.
pub fn held_segment() -> Option<cut::Seg> {
    crate::ui::cut_hand::held_clip_or_edge()
}

/// F2.4 S1b: put a picked clip EDGE in the hand (the 12 px reach, `// layout.lineReachPx`).
pub fn set_held_edge(seg: Option<cut::Seg>) {
    match seg {
        Some(seg) => {
            crate::ui::cut_hand::pick_up(crate::ui::cut_hand::Hand::Edge(seg));
        }
        None => crate::ui::cut_hand::clear_all(),
    };
}

/// Read side of [`set_held_edge`].
pub fn held_edge() -> Option<cut::Seg> {
    crate::ui::cut_hand::held_edge()
}

/// F2.4 S2/S3: what the line is holding right now, in the order the spec steps them — edge first, then
/// the whole clip, then an effect. This is what a frame-step or arrow press asks; with the hardcoded
/// `None` that stood here before, a real key press could never take the nudge branch at all.
pub fn held_now() -> Option<cut_line::Held> {
    crate::ui::cut_hand::held_kind()
}

/// F2.4 S1b: turn what the first left press picked into the hold it means. Edge goes to the edge slot so
/// a frame step moves only that border; a border or whole-clip pick takes the clip.
pub fn apply_pick(pick: cut_line::PressPick, seg: Option<cut::Seg>) {
    match pick {
        cut_line::PressPick::Edge => set_held_edge(seg),
        cut_line::PressPick::Border | cut_line::PressPick::Clip => set_held_clip(seg),
    }
}

/// F2.4 S1: which picture row this window watches. Set by a picture-band click through
/// [`place_line_press`]; read by whatever opens a row on the watched one.
pub fn watched_row() -> Option<usize> {
    WATCHED_ROW.with(|cell| *cell.borrow())
}

/// Write side of [`watched_row`].
pub fn set_watched_row(row: Option<usize>) {
    WATCHED_ROW.with(|cell| *cell.borrow_mut() = row);
}

thread_local! {
    /// F2.12 S1: whether this session has been told where its red line stands. `None` means no line was
    /// ever placed, which is the state that makes "click the timeline where the insert goes first" true.
    pub(crate) static INSERT_PLACE_KNOWN: std::cell::RefCell<Option<bool>> =
        const { std::cell::RefCell::new(None) };
}
