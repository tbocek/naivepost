//! F3.6 S5 (`applied in the preview even while paused`): hands a parked second's gain to the sound.
//! No rule of its own: asks [`crate::ui::paused_preview_gain`], folds in the room slider via [`crate::cut_hear::mix_gain`] (product, not either alone), and gives it to the footage leg through [`crate::ui::cut_cut_play::hand_gain`].

use crate::cut_hear;
use crate::ui;

/// F3.6 S5: while PAUSED inside a raised passage, this is the gain the sound gets. Returns it so a test can see the number.
/// Called from `ui::set_line_position` when not playing; while playing the tick carries it. [`crate::play_back::set_volume`] skips unchanged asks.
pub fn apply_paused_gain(window: &adw::ApplicationWindow, at: f64) -> f64 {
    let total = cut_hear::mix_gain(
        ui::preview_volume(window),
        ui::paused_preview_gain(window, at),
    );
    ui::cut_cut_play::hand_gain(window, total, at);
    total
}
