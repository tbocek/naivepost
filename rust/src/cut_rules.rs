//! §05-cut#7-rules — the Cut page's standing rules, `spec/05-cut.md` §7 and `spec/inventory/cut.md` §J.
//!
//! Most of §7 already has a home: each rule lives in the module whose behaviour it bounds, and this module
//! exists only for the handful nothing else had stated — so it holds three functions rather than a summary of
//! everything else's. Where a rule is read here, it is reached through its own module:
//!
//! * one Undo per drag, an unmoved press is a click, a border belongs to both buttons, moving is the right
//!   button's verb — [`crate::cut_trim`]'s [`pushes_undo`](crate::cut_trim::pushes_undo),
//!   [`is_click`](crate::cut_trim::is_click), [`grab_border`](crate::cut_trim::grab_border),
//!   [`right_press_moves_line`](crate::cut_trim::right_press_moves_line),
//!   [`merge_pair`](crate::cut_trim::merge_pair), [`trimmable`](crate::cut_trim::trimmable);
//! * what a press holds and where the line goes — [`crate::cut_line::Held`] and
//!   [`crate::cut_line::click_outcome`];
//! * a selection is of what it was drawn on — [`crate::cut_select::draw`], [`Scope`](crate::cut_select::Scope),
//!   [`scoped_to_row`](crate::cut_select::scoped_to_row), with the two floors beside them;
//! * unfilmed time has no width, the seam second belongs to the later take, half-open ranges —
//!   [`crate::timeline::Span`] and [`crate::timeline::spans_overlap`], plus
//!   [`crate::preview::walk_on`];
//! * inserts are never given a hearing answer and cost no session time — [`crate::cut_hear::hush`] and
//!   [`crate::cut_copy::footage_stretches`];
//! * a re-suggest keeps inserts, the base moves with a suggestion, `cut.json`'s existence is the gate —
//!   [`Cut::keep_inserts`](crate::cut::Cut::keep_inserts), [`History::suggested`](crate::cut::History::suggested),
//!   [`Cut::exists`](crate::cut::Cut::exists);
//! * nothing decodes inside a draw, and the page is rebuilt only when Prepare's output changed —
//!   [`crate::cut_screen::visible_window`] and [`crate::hand_edit::edited`].

use crate::cut_line::Held;
use crate::cut_select::{MIN_SCENE_SECONDS, MIN_SECONDS};

/// Which verb a floor belongs to. §7 says `Remove takes exactly the selection`; §I's rule 13 finishes it —
/// `Remove takes exactly the selection (floor 0.04 s); 1 s is only for proposing/copying`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Floor {
    /// － Remove: the selection is taken as marked, so the only bound is a remainder about one frame long.
    Remove,
    /// ＋ Add / ⧉ Copy / ▶ Suggest: a scene shorter than a second is not something anyone chose to watch.
    Keep,
}

/// The shortest stretch a verb may leave or take. Two floors, one per verb, and the pair is what makes a rule
/// like `Remove takes exactly the selection` mean something other than `Add` does: refusing a 0.3 s Remove
/// would be keeping footage nobody marked to keep, while accepting it as a scene would be proposing a flash.
///
/// `Floor::Remove` is `P.eng.minPieceSeconds` and `Floor::Keep` is `P.policy.minSceneSeconds`; both are already
/// catalogued in [`crate::params::cut`], so this reads them rather than restating them.
pub fn floor(verb: Floor) -> f64 {
    match verb {
        // P.eng.minPieceSeconds — about one frame at 25 fps.
        Floor::Remove => MIN_SECONDS,
        // P.policy.minSceneSeconds.
        Floor::Keep => MIN_SCENE_SECONDS,
    }
}

/// §7 (`One thing held at a time`), §J rule 3: picking something up **replaces** what was held rather than
/// adding to it, so the page never has two things waiting for one drag. `next` is handed back whatever was in
/// hand — dropping a clip to grab its neighbour is not an edit and asks no confirmation, because holding
/// something is only where the next gesture will start.
pub fn pick_up(current: Option<Held>, next: Held) -> Option<Held> {
    let _ = current;
    Some(next)
}

/// §7 (`picking up is not an edit`): grabbing a clip changes nothing, so it earns no undo step and the page must
/// not claim one. Only a move does — which is why this answers `moved` and not whether anything was held: the
/// hand that picked the clip up has read the timeline, not changed it, and an ↶ that then did nothing would be
/// a lie about what the page owes back. The same rule at the history end is [`crate::cut_trim::pushes_undo`].
pub fn picking_up_edits(moved: bool) -> bool {
    moved
}
