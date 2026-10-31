//! §05-cut#7-rules bullet 1 — the page's ONE hand.
//!
//! §7's `One thing held at a time` ([`crate::cut_rules::pick_up`]) only binds if there is one place things are
//! put in hand; before this the page kept three slots (edge, clip, effect) and `cut_hand::held_kind()`'s if-ladder
//! silently picked a winner. Holding edits nothing ([`crate::cut_rules::picking_up_edits`]), so no door here
//! touches the history or cut.json.

use crate::cut::{Fx, Seg};
use crate::cut_line::Held;
use crate::cut_rules;

/// What the hand is holding, with the thing itself beside the kind so ⌦ and the frame-step doors need no second lookup.
#[derive(Debug, Clone, PartialEq)]
pub enum Hand {
    /// F2.4 S1b: a clip's EDGE, picked within `layout.lineReachPx` (12 px) of the border; apart from a whole
    /// clip because a frame step must move that border alone.
    Edge(Seg),
    /// A whole clip: the pick a border or a mid-clip press takes.
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
    /// The window's one hold. `None` = nothing picked up, also what a release leaves behind.
    static HELD: std::cell::RefCell<Option<Hand>> = const { std::cell::RefCell::new(None) };
}

/// Put `next` in the hand, whatever was in it before; [`cut_rules::pick_up`] decides the replacement, so a
/// second pickup cannot join the first. Returns what the hand holds afterwards, always `Some(next)`.
pub fn pick_up(next: Hand) -> Option<Hand> {
    HELD.with(|cell| {
        // The rule answers WHICH kind the hand ends on; the payload follows that answer.
        let kind = cut_rules::pick_up(next.kind()).unwrap_or_else(|| next.kind());
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

/// Which kind is in hand, in the order §1 S1 steps the picks; what a frame-step or arrow press asks.
pub fn held_kind() -> Option<Held> {
    HELD.with(|cell| cell.borrow().as_ref().map(Hand::kind))
}

/// The clip the hand took as a WHOLE clip. An EDGE hold names the same segment but means "this border only",
/// and reading it as a whole clip would slide the scene instead of nudging one border. Doors that move
/// whatever segment is in hand read [`held_clip_or_edge`].
pub fn held_clip() -> Option<Seg> {
    HELD.with(|cell| {
        cell.borrow().as_ref().and_then(|held| match held {
            Hand::Clip(seg) => Some(seg.clone()),
            _ => None,
        })
    })
}

/// The segment in hand, whole clip or only its edge: what a trim, an insert edit or ⌦ is to move.
pub fn held_clip_or_edge() -> Option<Seg> {
    HELD.with(|cell| {
        cell.borrow().as_ref().and_then(|held| match held {
            Hand::Edge(seg) | Hand::Clip(seg) => Some(seg.clone()),
            Hand::Effect(_) => None,
        })
    })
}

/// Only an EDGE pick answers here: nudging one border versus dragging a whole clip.
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
