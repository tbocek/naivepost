//! §06-effects#7-rules — the standing rules of the effects page, `spec/06-effects.md` §7 and
//! `spec/inventory/effects.md` §G.

use crate::cut::Fx;

pub const REACH_SECONDS: f64 = 0.001;

pub const GONE: &str = "that effect is no longer in the cut \u{2014} nothing was changed";

/// §F3.8 S4 (`Edit re-finds the effect by kind and start`): which record a form opened with ✎ Edit is talking about.
pub fn find_by_value(fx: &[Fx], kind: &str, t: f64) -> Option<usize> {
    fx.iter()
        .position(|effect| effect.kind == kind && (effect.t - t).abs() < REACH_SECONDS)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Press {
    /// Nothing armed and the preview paused: the picture is there to be framed.
    Idle,
    /// A kind is armed, so the drag belongs to the new effect rather than to an existing one.
    Armed,
    /// Something is playing: a press cues the clock, it does not grab a box.
    Playing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    /// An overlay box in hand — above all others because it is already being judged.
    HeldBox(usize),
    /// The camera rect of a zoom in hand.
    HeldCamera(usize),
    /// A text on the picture: the one whose words the pointer is over, topmost first.
    Text(usize),
    /// The framing camera rect, taken only when nothing else is held.
    FramingZoom(usize),
}

pub fn grab(
    press: Press,
    held_box: Option<usize>,
    held_camera: Option<usize>,
    texts: &[(usize, f64)],
    framing: Option<usize>,
) -> Option<Hit> {
    if press != Press::Idle {
        return None;
    }
    if let Some(index) = held_box {
        return Some(Hit::HeldBox(index));
    }
    if let Some(index) = held_camera {
        return Some(Hit::HeldCamera(index));
    }
    if let Some(&(index, _)) = texts.iter().min_by(|(_, a), (_, b)| a.total_cmp(b)) {
        return Some(Hit::Text(index));
    }
    framing.map(Hit::FramingZoom)
}

pub fn nudge(t: f64, delta: f64) -> f64 {
    t + delta
}
