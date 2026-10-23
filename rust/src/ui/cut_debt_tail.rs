//! §06-effects#2-the-lane-and-the-preview — the dashed debt tail on a lane bar.
//!
//! A bar whose sound answer runs to the scene's end ("1× to the scene's end") can leave the sound behind the
//! picture, and §2 says the only thing that shows that is a dashed tail on the bar itself: "1× to the effect's
//! end" closes its gap on the effect's last frame, so it gets none. Whether a bar earns one is not composed
//! here — [`crate::effect_details::read_head_open`] answers it, and that is [`crate::fx_lane::draws_debt_tail`]'s
//! three conditions (the `scene` answer, a debt worth a line at `fx_lane::DEBT_TAIL_MIN_SECONDS`, and a scene
//! that still runs on) plus the two that say there is nothing to read back: a card or a held frame films
//! nothing, and a clip can sit on ground no recording covers.
//!
//! Split out of `window.rs`, which is past its size budget. This module decides nothing about seconds either:
//! the debt is `fx_lane::debt` over the clip's own on-screen length ([`crate::narrate_pass::on_screen`], which
//! already knows a card runs for its `dur` and a rate runs for its seconds divided by that rate).

use adw::prelude::*;
use gtk4 as gtk;

use crate::cut::{Cut, Fx, Seg};
use crate::cut_hear;
use crate::effect_details;
use crate::fx_lane;
use crate::narrate_pass;
use crate::timeline;

/// Does this record earn the dashed tail on its bar? Asked of the whole cut rather than the one record,
/// because three of the five things `read_head_open` wants are not properties of the record at all: the clip it
/// stands on supplies the on-screen seconds the debt is measured against, that clip's camera may be on ground
/// no recording filmed, and the scene has to run on past the record's last frame for the tail to point at
/// anything.
///
/// `t` is the red line, which is where the page reads every other sound question from; a record that does not
/// cover it simply has no debt to show and answers `false`.
pub fn tail_open(cut: &Cut, index: usize, t: f64, recordings: &[timeline::Recording]) -> bool {
    let Some(effect) = cut.fx.get(index) else {
        return false;
    };
    let Some(clip) = clip_under(&cut.segs, effect.t) else {
        // A record standing on a gap the cut dropped: there is no picture for its sound to be behind.
        return false;
    };
    // The debt is the whole record's length less the seconds it has run by the line, clamped inside the record
    // exactly as `fx_lane::plate_debt_at` clamps the plate's number, so a stale line cannot draw a tail longer
    // than the effect.
    let on_screen = (t - effect.t).clamp(0.0, effect.dur);
    effect_details::read_head_open(
        &effect.snd,
        fx_lane::debt(effect.dur, on_screen),
        scene_runs_on(cut, effect, clip, t),
        filmed(clip, recordings),
        is_insert(clip, effect),
    )
}

/// The clip the record stands on: the one whose bounds hold its start. Half-open like every other range here.
fn clip_under(segs: &[Seg], at: f64) -> Option<&Seg> {
    segs.iter().find(|seg| seg.s <= at && at < seg.e)
}

/// Does the scene run on past the record's last frame? The seconds that survive to the video are the clip's
/// own ([`narrate_pass::on_screen`] — a card's `dur`, a rate's divided seconds), so a record ending at or after
/// the clip's end has nothing after it: the tail would reach past the cut and point at seconds that are not in
/// the video. A rate under the record counts too (§2's "preview runs speed at one flat rate" is the same
/// [`cut_hear::rate_under`] the sound uses), because a slowed clip runs on longer than its session seconds.
fn scene_runs_on(cut: &Cut, effect: &Fx, clip: &Seg, t: f64) -> bool {
    let rate = cut_hear::rate_under(&cut.fx, t);
    let survived = narrate_pass::on_screen(clip) / rate.max(f64::EPSILON);
    // The record's end measured from the clip's start: a record whose own seconds outlast the clip leaves the
    // scene no room for a tail.
    t + fx_lane::debt(effect.dur, (t - effect.t).clamp(0.0, effect.dur)) < clip.s + survived
}

/// Is there film under this clip? A clip on ground no recording covers has sound exactly as long as its
/// picture, so there is no debt to open a tail for however the arithmetic reads. Asked of the session's own
/// filmed runs ([`timeline::filmed_runs`]), the same list the preview's ▶ is refused with.
fn filmed(clip: &Seg, recordings: &[timeline::Recording]) -> bool {
    let runs = timeline::filmed_runs(recordings);
    if runs.is_empty() {
        // No session named at all (a page with no tape yet): say nothing rather than a tail about nothing.
        return false;
    }
    runs.iter()
        .any(|(start, end)| start < &clip.e && clip.s < *end)
}

/// Is this record on an insert? A card films nothing — its sound is exactly as long as its picture, so there
/// is no debt for a tail to point at. A card is any segment with an `ins` (the page's own meaning of
/// "insert"); plain footage carries none, and footage is the only ground a delayed sound can be behind.
/// The card's KIND is deliberately not asked: the kind reader maps an empty path onto a still, which would
/// make every footage clip look like a card and refuse the tail everywhere it is owed.
fn is_insert(clip: &Seg, _effect: &Fx) -> bool {
    !clip.ins.is_empty()
}

/// Put the dashed tail on the bar it explains, or take it off — added AND removed off the answer, so a paid-off
/// sound cannot keep a tail claiming otherwise. Whether is not composed here: [`tail_open`] asks
/// [`crate::effect_details::read_head_open`].
pub fn wear_tail(
    bar: &gtk::Widget,
    cut: &Cut,
    index: usize,
    t: f64,
    recordings: &[timeline::Recording],
) {
    let held_classes = bar.css_classes();
    let mut classes: Vec<&str> = held_classes
        .iter()
        .map(|class| class.as_str())
        .filter(|class| *class != "fx-debt-tail")
        .collect();
    if tail_open(cut, index, t, recordings) {
        classes.push("fx-debt-tail");
    }
    bar.set_css_classes(&classes);
}

/// The tail's CSS. One class on the bar it explains: a dashed violet RIGHT edge — the tail runs on past the
/// bar's last frame, which is the whole point of drawing it. Its colour is [`fx_lane::HELD_OUTLINE`]'s violet,
/// the same one the held overlay's dashed outline wears, because both say "this part is not yet paid for" and two
/// dash colours on one page would be two meanings. No number is spelled here:
/// `fx_lane::DEBT_TAIL_MIN_SECONDS` is what decides whether a bar gets the class at all.
pub fn css() -> String {
    let violet = fx_lane::HELD_OUTLINE;
    format!(
        ".fx-debt-tail {{ border-right: 2px dashed rgba({}, {}, {}, 1.0); }}\n",
        (violet[0] * 255.0).round() as i32,
        (violet[1] * 255.0).round() as i32,
        (violet[2] * 255.0).round() as i32
    )
}
