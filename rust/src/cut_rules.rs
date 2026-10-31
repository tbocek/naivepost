//! §05-cut#7-rules — the Cut page's standing rules, `spec/05-cut.md` §7 and `spec/inventory/cut.md` §J.

use crate::cut_line::Held;
use crate::cut_select::{MIN_SCENE_SECONDS, MIN_SECONDS};

/// Which verb a floor belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Floor {
    Remove,
    Keep,
}

/// The shortest stretch a verb may leave or take.
pub fn floor(verb: Floor) -> f64 {
    match verb {
        // P.eng.minPieceSeconds — about one frame at 25 fps.
        Floor::Remove => MIN_SECONDS,
        // P.policy.minSceneSeconds.
        Floor::Keep => MIN_SCENE_SECONDS,
    }
}

/// §7 (`One thing held at a time`), §J rule 3: picking something up **replaces** what was held rather than
/// adding to it, so the page never has two things waiting for one drag.
pub fn pick_up(next: Held) -> Option<Held> {
    Some(next)
}

/// §7 (`picking up is not an edit`): grabbing a clip changes nothing, so it earns no undo step and the page must
/// not claim one.
pub fn picking_up_edits(moved: bool) -> bool {
    moved
}
