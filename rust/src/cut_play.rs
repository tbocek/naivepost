//! F2.2 — ▶✂: play the CUT instead of the recording.
//!
//! Where ▶ (F2.1, [`crate::preview`]) plays every second the session holds, ▶✂ plays the finished
//! video: removed stretches are skipped, the line jumps to the next clip's first playable second, and
//! the clock reads the cut's own time rather than the session's. Nothing here decides anything the
//! page could decide for itself (spec/00-principles.md §5): the button forwards, this answers.
//!
//! The three branches of the flowchart in spec/05-cut.md §F2.2 are S1-S6 below. S3 deliberately reuses
//! [`crate::cut_review::switch_over`] rather than clearing the review flag by hand, so "a review is
//! running → end it, carry on as the plain cut" has exactly one implementation between the two items.

use crate::{
    cut::{Cut, Seg},
    cut_fold, cut_review, cut_screen,
    preview::{self, Player},
};

// --- S1 / S2: does ▶✂ have anything to do? ----------------------------------------------------------

/// S1's greyed rule lives in [`cut_screen::can_play_cut`] — one function answers whether the button is
/// sensitive and whether a press refuses, so the two can never disagree. This is the same question asked
/// from the logic side.
pub fn empty(cut: &Cut) -> bool {
    !cut_screen::can_play_cut(cut)
}

/// S2, reached with no clips at all: what ▶✂ says when the person switched to a cut that holds nothing.
pub const NOTHING_TO_PLAY: &str = "preview is the cut \u{2014} and the cut is empty, so \u{25b6}\u{2702} \
has nothing to play until a clip is added";

/// S2's refusal from the OTHER ways in — Space, a click on the picture, the run bar. Those paths cannot
/// switch the preview the way the button does, so they name the alternative rather than the switch.
pub const EMPTY_CUT_REFUSAL: &str = "the cut is empty \u{2014} add a clip to play it, or press \u{25b6} \
to play the recording instead";

/// S2's status once the preview has become the cut: the clock's meaning changed, so say so out loud.
pub const SWITCH_STATUS: &str = "preview is the cut \u{2014} the clock reads the finished video";

/// S3's status: the review was ended and the same kept material plays on, without the join-by-join tour.
pub const REVIEW_ENDED_STATUS: &str =
    "the review is over \u{2014} playing the plain cut, every kept second and nothing else";

// --- S2: the line snaps onto kept material ---------------------------------------------------------

/// A seg the cut actually shows: cards (`e <= s`) carry no length of their own and would make a "clip"
/// out of an instant. `shell::cut_seconds` filters the same way, so the line and the clock agree about
/// what counts as material.
fn visible(seg: &Seg) -> bool {
    seg.e > seg.s
}

/// S2: the first kept second at or after `line`.
///
/// A line parked inside a removed stretch must land on material rather than on nothing, because ▶✂ has
/// no such thing as "play the gap": the nearest forward clip start is the honest answer. Past the last
/// clip there is nothing ahead, so the line goes to the last clip's start rather than off the end of
/// the video (the spec is silent; landing on material beats parking on a hole).
pub fn snap_to_kept(segs: &[Seg], line: f64) -> f64 {
    let Some(last) = segs.iter().filter(|seg| visible(seg)).last() else {
        return 0.0;
    };
    // Inside a kept clip already: the line stays where it is. Forward otherwise.
    if segs
        .iter()
        .any(|seg| visible(seg) && line >= seg.s && line < seg.e)
    {
        return line;
    }
    match segs.iter().find(|seg| visible(seg) && seg.s > line) {
        Some(seg) => seg.s,
        None => last.s,
    }
}

// --- S2 / S3 / S4: one press ---------------------------------------------------------------------

/// What pressing ▶✂ did, so the page prints the right sentence without deciding anything itself.
#[derive(Debug, Clone, PartialEq)]
pub enum Pressed {
    /// S1/S2 with no clips: nothing to skip to.
    Refused(&'static str),
    /// S2: the preview became the cut. `kept_playing` = it was already playing and carries on.
    SwitchedToCut { kept_playing: bool, status: String },
    /// S3: a review was running; it ended and the plain cut carries on.
    ReviewEnded { status: &'static str },
    /// S4: the ordinary play/pause toggle of a preview that was already the cut.
    Toggled(Toggle),
}

/// S4's two answers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Toggle {
    Paused,
    Playing { from: f64 },
}

/// ▶✂ pressed, in the flowchart's own order.
pub fn pressed(player: &mut Player, cut: &Cut) -> Pressed {
    // S1: no clips. Greyed on the button, and refused outright when the press came another way.
    if empty(cut) {
        return Pressed::Refused(NOTHING_TO_PLAY);
    }

    // S2: the preview is the recording → switch to the cut, snapping the line onto kept material.
    // A review counts as the recording side here only in that it is not the plain cut; S3 handles it
    // before we get here, so this branch is the true recording preview.
    if !player.cut_only && !player.reviewing {
        let kept_playing = player.transport.playing;
        player.cut_only = true;
        player.transport.started = true;
        let snapped = snap_to_kept(&cut.segs, player.playhead.unwrap_or(0.0));
        player.playhead = Some(snapped);
        return Pressed::SwitchedToCut { kept_playing, status: SWITCH_STATUS.to_string() };
    }

    // S3: a review is running → end it, carry on as the plain cut. `switch_over` leaves the
    // transport alone, which is exactly "carry on": no pause, no restart.
    if player.reviewing {
        cut_review::switch_over(player);
        player.cut_only = true;
        if let Some(line) = player.playhead {
            player.playhead = Some(snap_to_kept(&cut.segs, line));
        }
        return Pressed::ReviewEnded { status: REVIEW_ENDED_STATUS };
    }

    // S4: already the cut — toggle.
    if player.transport.playing {
        player.transport.playing = false;
        return Pressed::Toggled(Toggle::Paused);
    }
    let from = snap_to_kept(&cut.segs, player.playhead.unwrap_or_else(|| first_start(&cut.segs)));
    player.transport.started = true;
    player.transport.playing = true;
    player.playhead = Some(from);
    Pressed::Toggled(Toggle::Playing { from })
}

/// The first kept start, for a page that has never had a line placed on it. `segs` is not guaranteed
/// sorted by `s`, so this takes the minimum rather than assuming the front of the list is earliest.
fn first_start(segs: &[Seg]) -> f64 {
    segs.iter()
        .filter(|seg| visible(seg))
        .map(|seg| seg.s)
        .min_by(f64::total_cmp)
        .unwrap_or(0.0)
}

// --- S5: while playing, where the line goes ------------------------------------------------------

/// One tick of the cut preview's line.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum At {
    /// Still inside a kept clip.
    Kept { t: f64 },
    /// The seconds crossed a removed stretch: jump to the next clip's first playable second.
    JumpTo { t: f64 },
    /// Past the last clip — the caller pauses ("past the last clip → pause").
    Ended,
}

/// S5: advance the line over the cut's own material.
///
/// Unlike [`preview::play_advance`], which walks straight through everything because ▶ shows cuts and
/// all, this skips: a target that lands in a removed stretch becomes a jump to the next clip's start,
/// and a target past the final clip ends playback rather than running off the end.
pub fn advance(segs: &[Seg], from: f64, elapsed: f64) -> At {
    let target = from + elapsed;
    if segs.iter().any(|seg| visible(seg) && target >= seg.s && target < seg.e) {
        return At::Kept { t: target };
    }
    match segs.iter().find(|seg| visible(seg) && seg.s > target) {
        Some(seg) => At::JumpTo { t: seg.s },
        None => At::Ended,
    }
}

/// The kept clips as merged runs, so back-to-back clips are one stretch rather than a fake jump.
///
/// Sorted and disjoint, the same contract [`crate::timeline::filmed_runs`] gives the recording side —
/// which is what lets [`preview::preload_target`] answer for both.
pub fn kept_runs(segs: &[Seg]) -> Vec<(f64, f64)> {
    let mut clips: Vec<(f64, f64)> = segs.iter().filter(|seg| visible(seg)).map(|seg| (seg.s, seg.e)).collect();
    clips.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut runs: Vec<(f64, f64)> = Vec::with_capacity(clips.len());
    for (start, end) in clips {
        match runs.last_mut() {
            // Touching counts as merged: no gap means no jump to preload for.
            Some(last) if start <= last.1 => last.1 = last.1.max(end),
            _ => runs.push((start, end)),
        }
    }
    runs
}

/// S5: which second to hand the spare pipeline, if any.
///
/// `P.eng.preloadLeadSeconds` — the same three seconds and the same helper ([`preview::preload_target`])
/// serve all three jumps the spec says share the one spare pipeline: the cut's skipped stretch (here),
/// the review's next run-up (`cut_review`'s seek), and the walk-on at a recording's end
/// ([`preview::walk_on`]). One lead, three callers, so a retune moves them together.
pub fn preload_ahead(segs: &[Seg], t: f64) -> Option<f64> {
    preview::preload_target(&kept_runs(segs), t)
}

// --- S5: the clock reads the cut's own time ----------------------------------------------------

/// The cut's own second for a session second: the kept length before it, plus how far into the clip
/// holding it we are. `None` when the line sits in a removed stretch, where the cut has no time to show.
///
/// Consistent with [`crate::shell::cut_seconds`] by construction: both count only segs with `e > s`,
/// so the last clip's end maps onto exactly the finished video's length and the clock can never read
/// past the video it is describing.
pub fn cut_time(segs: &[Seg], session_t: f64) -> Option<f64> {
    let mut out = 0.0;
    for seg in segs.iter().filter(|seg| visible(seg)) {
        if session_t < seg.s {
            break;
        }
        if session_t < seg.e {
            return Some(out + (session_t - seg.s));
        }
        out += seg.e - seg.s;
    }
    None
}

/// §D: the tooltip that explains the clock while ▶✂ runs — the number is not the session's.
pub fn cut_clock_note() -> &'static str {
    "the clock reads the cut's own time, not the session's \u{2014} the removed stretches are not counted"
}

/// S5: speed effects play at their flat rate — no ramp, no staircase, just the rate itself. Held here
/// because this is the module whose rule reads it; [`crate::cut_speed`] owns the rates themselves.
pub const FLAT_RATE: f64 = 1.0;

/// S5: a stop shows its still. A stop is not the zero-rate case of a speed effect — `cut_speed` writes
/// one as `kind: "speed"` with `rate: 0.0` over a two-second stretch (`STOP_SECONDS`), and a seg with
/// no rate written at all is an ordinary clip whose `rate` field is also 0.0. So the still is read from
/// the effect list, not off the clip: asking the clip would call every untouched segment a freeze frame.
pub fn shows_a_still(cut: &Cut, session_t: f64) -> bool {
    cut.fx.iter().any(|fx| {
        fx.kind == "speed"
            && fx.rate == 0.0
            && session_t >= fx.t
            && session_t < fx.t + fx.dur.max(crate::cut_speed::STOP_SECONDS)
    })
}

/// S5: the rate the preview plays a clip at — its own if it has one, otherwise flat.
pub fn played_rate(seg: &Seg) -> f64 {
    if seg.rate > 0.0 { seg.rate } else { FLAT_RATE }
}

/// S5: volume effects apply during the cut preview. They always do — the rule exists as a named answer
/// so the page reads a decision instead of hard-coding one, and so a later change of mind has a home.
pub fn volume_applies(_seg: &Seg) -> bool {
    true
}

// --- S6: dropped stretches dimmed on the tracks -----------------------------------------------

/// S6: the spans to draw dimmed while ▶✂ runs — the removed stretches, computed once in
/// [`crate::cut_fold::dropped_gaps`] rather than re-derived here.
pub fn dimmed_spans(runs: &[(f64, f64)], cut: &Cut) -> Vec<(f64, f64)> {
    cut_fold::dropped_gaps(runs, &cut.segs)
}
