//! §F2.4 S1/S1b/S2/S3 / §F2.9 — what the hand holds and which picture row this window watches: the effect /
//! clip / edge slots, the pick that fills them, and the watched-row read and write.

use crate::cut::{self};
use crate::cut_line;
use crate::ui::window_page_state::{WindowPage, PAGE as WINDOW_PAGE};

/// What ⌦ would take first: the effect in hand. A seam so the delete order is testable before the lane pixels exist.
///
/// `None` puts the effect down and nothing else (`crate::ui::cut_hand::drop_effect`).
pub fn set_held_effect(effect: Option<cut::Fx>) {
    match effect {
        Some(fx) => {
            crate::ui::cut_hand::pick_up(crate::ui::cut_hand::Hand::Effect(fx));
        }
        None => crate::ui::cut_hand::clear_all(),
    };
}

/// What ⌦ takes next: the clip in hand, `the only way to remove a spliced card` (§J rule 8). A seam as above.
pub fn set_held_clip(seg: Option<cut::Seg>) {
    match seg {
        Some(seg) => {
            crate::ui::cut_hand::pick_up(crate::ui::cut_hand::Hand::Clip(seg));
        }
        None => crate::ui::cut_hand::clear_all(),
    };
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

/// F2.4 S1b: turn what the first left press picked into the hold it means. An edge goes to the edge slot so
/// a frame step moves only that border; a border or whole-clip pick takes the clip.
pub fn apply_pick(pick: cut_line::PressPick, seg: Option<cut::Seg>) {
    match pick {
        cut_line::PressPick::Edge => set_held_edge(seg),
        cut_line::PressPick::Border | cut_line::PressPick::Clip => set_held_clip(seg),
    }
}

/// F2.4 S1: which picture row this window watches, set by a picture-band click through `place_line_press`.
pub fn watched_row() -> Option<usize> {
    WINDOW_PAGE.with(
        |WindowPage {
             watched_row: cell, ..
         }| *cell.borrow(),
    )
}

pub fn set_watched_row(row: Option<usize>) {
    WINDOW_PAGE.with(
        |WindowPage {
             watched_row: cell, ..
         }| *cell.borrow_mut() = row,
    );
}
