//! F3.6 S5 (`applied in the preview even while paused`) — the door that puts a parked second's gain on sound.
//!
//! [`crate::ui::paused_preview_gain`] answers how loud one session second is, and until now nothing asked it:
//! the number was computed and left in the slot, so "overlapping gains multiply · applied in the preview even
//! while paused" described an arithmetic nothing consumed. This module is the consuming half — no rule of its
//! own, only the hand: ask the same function the playing preview asks, fold in the room's slider the way every
//! other tick does, and give the result to the footage leg through [`crate::ui::cut_cut_play::hand_gain`].
//!
//! The number comes from [`crate::ui::paused_preview_gain`] rather than from a fresh rule on purpose: that door
//! is `fx_volume::heard_while_paused`, which IS [`crate::cut_hear::gain_under`] under volume's name, so a paused
//! line and a moving one cannot grow separate answers about the same passage. The slider is folded in by
//! [`crate::cut_hear::mix_gain`] — the same composition `cut_play_leg::route` uses while playing — because a
//! passage lifted to 200 % in a room turned down to 40 % is neither 2.0 nor 0.8 but their product, and the
//! ceiling (playbin's own, `P.eng.maxGain`) belongs to that one function too.

use crate::cut_hear;
use crate::ui;

/// F3.6 S5: park the red line inside a raised passage and this is what the sound gets — while PAUSED, which is
/// the whole point of the clause: without it the fades cannot be judged unless the frame is moving. Returns the
/// gain handed over, so a caller (and a test watching the real player process) can see the number rather than
/// infer it.
///
/// Called from `ui::set_line_position` for a window whose transport is not playing; while playing, the tick's
/// own `CutPreviewPage::apply_gain` carries the same number at the same second, so the two never both ask. And
/// a repeat costs nothing anyway: [`crate::play_back::set_volume`] keeps a stamp of the last ask and does
/// exactly nothing when the answer has not changed.
pub fn apply_paused_gain(window: &adw::ApplicationWindow, at: f64) -> f64 {
    let total = cut_hear::mix_gain(
        ui::preview_volume(window),
        ui::paused_preview_gain(window, at),
    );
    ui::cut_cut_play::hand_gain(window, total, at);
    total
}
