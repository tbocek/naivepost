//! F2.2 — the leg ▶✂ drives WHILE the cut preview runs.
//!
//! [`crate::cut_play`] holds the rules (what a tick means, where the line lands, what the clock
//! reads). This module is the seam the page drives those rules through: it owns the one idle spare
//! pipeline that S5 says must be armed ahead of each jump, and it paints the dimmed removed stretches
//! S6 asks for. Nothing here decides anything — every function delegates to `cut_play::*`, so a rule
//! retuned there moves the running page with it.
//!
//! Why a second file rather than more functions in `cut_play.rs`: `cut_play` is pure arithmetic and
//! settles with no widget and no cairo context, which is what makes its tests cheap. The parts below
//! hold process state (the armed spare) and draw pixels; mixing them into the rule module would make
//! every rule test carry a surface allocation.

use std::rc::Rc;

use crate::{
    cut::{Cut, Seg},
    cut_play,
};

/// One tick of the cut preview's line: where the head is, what the clock shows, whether to pause.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tick {
    /// The session second the line sits on after this tick (jumped over any removed stretch).
    pub head: f64,
    /// The CUT's own second at that head — the finished video's time, not the session's (S5).
    pub clock: f64,
    /// Past the last clip: the caller pauses ("past the last clip → pause").
    pub ended: bool,
}

/// S5: advance the preview one tick over the cut's material.
///
/// A jump across a removed stretch is reported as the new head, not as elapsed time, because the
/// person sees the line teleport to the next clip's first playable second while the wall clock keeps
/// ticking at its own rate. `clock` falls back to the whole cut's length when the head ends up off the
/// kept material (only reachable on `ended`), so the readout never goes blank mid-pause.
pub fn tick(cut: &Cut, from: f64, elapsed_ms: u64) -> Tick {
    let elapsed = elapsed_ms as f64 / 1000.0;
    match cut_play::advance(&cut.segs, from, elapsed) {
        cut_play::At::Kept { t } | cut_play::At::JumpTo { t } => Tick {
            head: t,
            clock: cut_play::cut_time(&cut.segs, t).unwrap_or_else(|| kept_length(cut)),
            ended: false,
        },
        cut_play::At::Ended => {
            // Hold the line on the last kept end rather than wherever the tick overshot to: pausing
            // mid-air past the end of the video would leave the line on nothing.
            let last_end = last_kept_end(cut);
            Tick {
                head: last_end,
                clock: cut_play::cut_time(&cut.segs, last_end)
                    .or_else(|| cut_play::cut_time(&cut.segs, from))
                    .unwrap_or(kept_length(cut)),
                ended: true,
            }
        }
    }
}

/// S5: arm the spare pipeline for the next jump, three seconds ahead (`P.eng.preloadLeadSeconds`).
///
/// Returns the second that was opened, and remembers it as this process's armed spare until the next
/// arm or [`clear_spare`]. One slot per process: the spare is an idle pipeline waiting to be swapped
/// in, and only one preview plays at a time, so a per-window stack would only let a stale window's
/// arm survive.
pub fn arm_spare(cut: &Cut, t: f64) -> Option<f64> {
    let opened = cut_play::preload_ahead(&cut.segs, t);
    SPARE.with(|slot| *slot.borrow_mut() = opened);
    opened
}

/// Which second the spare pipeline is holding, if any.
pub fn spare_armed() -> Option<f64> {
    SPARE.with(|slot| *slot.borrow())
}

/// Release the spare (playback paused, the cut changed, the review took the pipeline instead).
pub fn clear_spare() {
    SPARE.with(|slot| *slot.borrow_mut() = None);
}

thread_local! {
    static SPARE: std::cell::RefCell<Option<f64>> = const { std::cell::RefCell::new(None) };
}

/// What the preview should show for one frame while ▶✂ runs: a still or live picture, the flat rate,
/// and whether the volume effects are in force (S5's three "while playing" answers, one read).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    pub still: bool,
    pub rate: f64,
    pub volume: bool,
}

/// S5: the frame answer for the segment under `t`.
pub fn frame(cut: &Cut, seg: &Seg, t: f64) -> Frame {
    Frame {
        still: cut_play::shows_a_still(cut, t),
        rate: cut_play::played_rate(seg),
        volume: cut_play::volume_applies(seg),
    }
}

/// S2's refusal for the ways in that CANNOT switch the preview: Space, a click on the picture, the
/// run bar. Those paths have no switch to offer, so they name the alternative. `None` means the way in
/// may go ahead and toggle whatever the preview already is.
pub fn other_way_in(cut: &Cut) -> Option<&'static str> {
    if cut_play::empty(cut) {
        Some(cut_play::EMPTY_CUT_REFUSAL)
    } else {
        None
    }
}

/// S6: the spans to draw dimmed on the tracks — the removed stretches inside filmed material.
pub fn dimmed(cut: &Cut, filmed: &[(f64, f64)]) -> Vec<(f64, f64)> {
    cut_play::dimmed_spans(filmed, cut)
}

/// How far the removed stretches are dimmed: black at alpha 0.18.
///
/// Chosen against the two fills the strip already uses — kept footage `rgba(0.2, 0.8, 0.3, 0.3)` and
/// insert `rgba(0.55, 0.35, 0.75, 0.55)` (`src/ui/window.rs`, `paint_track_strip`) over a 0.95 grey
/// ground. At 0.18 the gap reads clearly darker than the ground and clearly lighter than a border, so
/// it looks like "material that is not in the cut" rather than a third track someone could click.
pub const DIM_ALPHA: f64 = 0.18;

/// S6: paint the dimmed stretches across one bar row of the strip.
///
/// `pps` is the strip's pixels-per-second (the same number the kept boxes were converted with), so the
/// dim lines up with the ruler and the clips without a second scale factor anywhere.
pub fn paint_dimmed(
    cr: &cairo::Context,
    spans: &[(f64, f64)],
    top: f64,
    height: f64,
    pps: f64,
) {
    cr.set_source_rgba(0.0, 0.0, 0.0, DIM_ALPHA);
    for &(start, end) in spans {
        let x = start * pps;
        let w = (end - start).max(0.0) * pps;
        if w > 0.0 {
            cr.rectangle(x, top, w, height);
            let _ = cr.fill();
        }
    }
}

/// S5: run the cut preview's own tick loop while ▶✂ is the preview.
///
/// Started once per window from `build_window`'s Cut wiring. Every [`crate::preview::TICK_MS`] it asks
/// [`tick`] where the line goes, moves the window's playhead there, arms the spare pipeline for the
/// next jump ([`arm_spare`]) and writes the CUT's own time to the status line. It no-ops whenever the
/// preview is not the plain cut — a recording preview or a review owns its own line, and ▶ must be
/// undisturbed — so the timer costs one boolean read between ▶✂ presses.
///
/// The clock has no widget of its own on this page today (the only clock face is `selection-readout`,
/// which shows the selected band), so the number goes to the status line with [`cut_play::cut_clock_note`]
/// as its explanation; a dedicated readout belongs to whichever round draws the real tracks.
pub fn start_cut_preview_tick(window: &adw::ApplicationWindow, page: Rc<dyn CutPreviewPage>) {
    let _ = window;
    glib::timeout_add_local(
        std::time::Duration::from_millis(crate::preview::TICK_MS),
        move || {
            // Only ▶✂ drives this loop; every other preview mode leaves the line alone.
            if !page.cut_only() {
                return glib::ControlFlow::Continue;
            }
            let from = page.playhead().unwrap_or(0.0);
            let cut_ = page.cut();
            let step = tick(&cut_, from, crate::preview::TICK_MS);
            page.set_playhead(step.head);
            arm_spare(&cut_, step.head);
            page.say_clock(step.clock, step.ended);
            glib::ControlFlow::Continue
        },
    );
}

/// What the tick loop needs from the page, so this module holds no GTK-slot knowledge of its own.
/// `src/ui/window.rs` implements it over its player and cut slots.
pub trait CutPreviewPage {
    /// Is this window's preview the plain cut (▶✂)? False for the recording and for a review.
    fn cut_only(&self) -> bool;
    /// Where the line sits now, in session seconds.
    fn playhead(&self) -> Option<f64>;
    /// Move the line.
    fn set_playhead(&self, at: f64);
    /// This window's cut.
    fn cut(&self) -> Cut;
    /// Print the cut's own clock; `ended` adds the pause note ("past the last clip → pause").
    fn say_clock(&self, clock: f64, ended: bool);
}

/// The status-line sentence the tick writes: the CUT's time, explained as such (S5). Kept here so the
/// wording travels with the loop that writes it rather than being spelled again in the shell.
pub fn clock_line(clock: f64, ended: bool) -> String {
    let face = crate::preview::clock(Some(clock));
    if ended {
        format!(">>> preview: {face} \u{2014} past the last clip, paused \u{b7} {}", cut_play::cut_clock_note())
    } else {
        format!(">>> preview: {face} \u{b7} {}", cut_play::cut_clock_note())
    }
}

/// The finished video's length as the sum of visible clips — the clock's ceiling. Matches
/// [`crate::shell::cut_seconds`] by counting only segs with `e > s`, the same filter
/// [`cut_play::visible`] applies.
fn kept_length(cut: &Cut) -> f64 {
    cut.segs
        .iter()
        .filter(|seg| seg.e > seg.s)
        .map(|seg| seg.e - seg.s)
        .sum()
}

/// The last visible clip's end, in SESSION seconds — where the line parks when playback runs out.
fn last_kept_end(cut: &Cut) -> f64 {
    cut.segs
        .iter()
        .filter(|seg| seg.e > seg.s)
        .map(|seg| seg.e)
        .fold(0.0, f64::max)
}
