//! §05-cut#7-rules bullet 1 — the page's ONE hand.
//!
//! §7 says `One thing held at a time`, and [`crate::cut_rules::pick_up`] states it as a rule over two values.
//! A rule about what is in hand only binds if there is one place things are put in hand, which is this module:
//! before it the page kept three separate slots (an edge, a clip, an effect) and a press that picked up a clip
//! left any effect still held, so `held_now()`'s if-ladder silently decided the winner and more than one thing
//! was waiting for one drag. Every reader and writer of the hold goes through here now; the exclusion is
//! `cut_rules::pick_up`'s answer, re-read rather than re-derived.
//!
//! Nothing here edits: holding is where the next gesture starts ([`crate::cut_rules::picking_up_edits`]), so no
//! door in this file touches the history or cut.json.

use crate::cut::{Fx, Seg};
use crate::cut_line::Held;
use crate::cut_rules;

/// What the hand is holding, with the thing itself beside the kind so ⌦ and the frame-step doors can act on it
/// without a second lookup into the cut.
#[derive(Debug, Clone, PartialEq)]
pub enum Hand {
    /// F2.4 S1b: a clip's EDGE, picked within `layout.lineReachPx` (12 px) of the border. Kept apart from a
    /// whole clip because a frame step must move that border alone.
    Edge(Seg),
    /// A whole clip — the pick a border or a mid-clip press takes.
    Clip(Seg),
    /// An effect card off the effects lane.
    Effect(Fx),
}

impl Hand {
    fn kind(&self) -> Held {
        match self {
            Hand::Edge(_) => Held::Edge,
            Hand::Clip(_) => Held::Clip,
            Hand::Effect(_) => Held::Effect,
        }
    }
}

thread_local! {
    /// The window's one hold. `None` = nothing picked up, which is also what a release leaves behind.
    static HELD: std::cell::RefCell<Option<Hand>> = const { std::cell::RefCell::new(None) };
}

/// Put `next` in the hand, whatever was in it before. The replacement is [`cut_rules::pick_up`]'s decision, so
/// the rule has one home: a second pickup cannot join the first. Returns what the hand holds afterwards, which
/// is always `Some(next)` — the caller keeps its own copy anyway, and reading it back here proves the slot took
/// the new value rather than keeping the old one.
pub fn pick_up(next: Hand) -> Option<Hand> {
    HELD.with(|cell| {
        let previous = cell.borrow().as_ref().map(Hand::kind);
        // The rule answers WHICH kind the hand ends on; the payload follows that answer.
        let kind = cut_rules::pick_up(previous, next.kind()).unwrap_or_else(|| next.kind());
        *cell.borrow_mut() = Some(match kind {
            Held::Edge => Hand::Edge(seg_of(&next)),
            Held::Clip => Hand::Clip(seg_of(&next)),
            Held::Effect => Hand::Effect(fx_of(&next)),
        });
        cell.borrow().clone()
    })
}

/// Empty the hand: a release, a ⌦ that took the thing away, or a form that consumed it.
pub fn clear_all() {
    HELD.with(|cell| *cell.borrow_mut() = None);
}

/// Which kind is in hand, in the order §1 S1 steps the picks. This is what a frame-step or arrow press asks.
pub fn held_kind() -> Option<Held> {
    HELD.with(|cell| cell.borrow().as_ref().map(Hand::kind))
}

/// The clip the hand took as a WHOLE clip. §7's mutual exclusion means one hand holds one thing, so the
/// segment in hand is a clip only when the kind says Clip: an EDGE hold names the same segment but means "this
/// border only", and a caller that read it as a whole clip would move a span the hand never took (a frame step
/// would slide the scene instead of nudging one border). Doors that move whatever segment is in hand, without
/// asking which, read [`held_clip_or_edge`].
pub fn held_clip() -> Option<Seg> {
    HELD.with(|cell| {
        cell.borrow().as_ref().and_then(|held| match held {
            Hand::Clip(seg) => Some(seg.clone()),
            _ => None,
        })
    })
}

/// The segment in hand, whether the pick took the whole clip or only its edge: both name a segment, and the
/// doors that move one (a trim, an insert edit, ⌦) ask for the segment they are to move.
pub fn held_clip_or_edge() -> Option<Seg> {
    HELD.with(|cell| {
        cell.borrow().as_ref().and_then(|held| match held {
            Hand::Edge(seg) | Hand::Clip(seg) => Some(seg.clone()),
            Hand::Effect(_) => None,
        })
    })
}

/// Only an EDGE pick answers here: the difference between nudging one border and dragging a whole clip.
pub fn held_edge() -> Option<Seg> {
    HELD.with(|cell| {
        cell.borrow().as_ref().and_then(|held| match held {
            Hand::Edge(seg) => Some(seg.clone()),
            _ => None,
        })
    })
}

/// The effect in hand, if the hand holds one.
pub fn held_effect() -> Option<Fx> {
    HELD.with(|cell| {
        cell.borrow().as_ref().and_then(|held| match held {
            Hand::Effect(fx) => Some(fx.clone()),
            _ => None,
        })
    })
}

/// §7 (`picking up is not an edit`): whether a gesture that started by picking something up owes an undo step.
/// It does only if it moved — [`cut_rules::picking_up_edits`] is the rule; this is the door the page reaches
/// through so the rule has a caller.
pub fn picking_up(moved: bool) -> bool {
    cut_rules::picking_up_edits(moved)
}

fn seg_of(hand: &Hand) -> Seg {
    match hand {
        Hand::Edge(seg) | Hand::Clip(seg) => seg.clone(),
        Hand::Effect(_) => Seg::default(),
    }
}

fn fx_of(hand: &Hand) -> Fx {
    match hand {
        Hand::Effect(fx) => fx.clone(),
        _ => Fx::default(),
    }
}
