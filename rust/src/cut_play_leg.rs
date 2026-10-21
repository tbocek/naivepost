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
    cut_hear, cut_play,
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
    /// The second armed for the next jump before this tick moved, so the caller can tell a jump the spare
    /// was waiting for from one it was not. Read BEFORE the tick re-armed the slot.
    pub armed_before: Option<f64>,
    /// Whether this tick's move was a jump across a removed stretch.
    pub jumped: bool,
}

/// S5: advance the preview one tick over the cut's material.
///
/// A jump across a removed stretch is reported as the new head, not as elapsed time, because the
/// person sees the line teleport to the next clip's first playable second while the wall clock keeps
/// ticking at its own rate. `clock` falls back to the whole cut's length when the head ends up off the
/// kept material (only reachable on `ended`), so the readout never goes blank mid-pause.
pub fn tick(cut: &Cut, from: f64, elapsed_ms: u64) -> Tick {
    let elapsed = elapsed_ms as f64 / 1000.0;
    // Read BEFORE the arm below rewrites the slot: this is the second the spare was holding on the way in.
    let armed_before = peek_spare();
    let moved = cut_play::advance(&cut.segs, from, elapsed);
    match moved {
        cut_play::At::Kept { t } | cut_play::At::JumpTo { t } => Tick {
            head: t,
            clock: cut_play::cut_time(&cut.segs, t).unwrap_or_else(|| kept_length(cut)),
            ended: false,
            armed_before,
            jumped: matches!(moved, cut_play::At::JumpTo { .. }),
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
                armed_before,
                jumped: false,
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
///
/// The ARM ITSELF is reported through [`spare_arms_for_test`], the leg's own log of second → outcome, one entry per
/// call. The process the arm starts belongs to the page (`CutPreviewPage::arm_spare_at`), because only a
/// page can name the footage files; the leg records that the arm was asked and what the page answered, so
/// the ORDER of arms across a test's pages is visible here rather than in a counter a test would have to
/// carry itself.
pub fn arm_spare(cut: &Cut, t: f64) -> Option<f64> {
    let opened = cut_play::preload_ahead(&cut.segs, t);
    SPARE.with(|slot| *slot.borrow_mut() = opened);
    opened
}

/// What an arm did, as recorded by [`arm_spare_page`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpareArm {
    /// The second the spare was opened at; `None` when nothing was worth opening.
    pub at: Option<f64>,
    /// Whether the page started a pipeline there. `false` when nothing was opened, or the page refused.
    pub prerolled: bool,
    /// Whether this same tick crossed onto the spare, and whether the swap found a process waiting.
    pub swapped: Option<bool>,
}

/// Ask for the next jump's spare AND record it: the arm the tick loop uses. See [`arm_spare`] for what the
/// slot means and [`spare_arms_for_test`] for the log this appends to.
pub fn arm_spare_page(cut: &Cut, t: f64, page: &dyn CutPreviewPage) -> Option<f64> {
    let opened = arm_spare(cut, t);
    // Re-arming the second already held would only rebuild a pipeline that is already sitting on it, so the
    // page is asked only when the armed second CHANGED — and at the §6 lead's cadence, so a jump the line is
    // crawling toward does not cost one fresh process per tick. [`cut_play::worth_arming`] answers both, on
    // times handed to it rather than read inside, so the rule is testable without a player or a clock.
    let now = std::time::UNIX_EPOCH
        .elapsed()
        .map(|since| since.as_millis() as u64)
        .unwrap_or_default();
    let since_last = LAST_ARM_AT_MS.with(|slot| *slot.borrow());
    let worth = cut_play::worth_arming(
        since_last.map(|last| now.saturating_sub(last)),
        previous_spare(),
        opened,
    );
    let prerolled = match (worth, opened) {
        (true, Some(at)) => page.arm_spare_at(at),
        _ => false,
    };
    if prerolled {
        LAST_ARM_AT_MS.with(|slot| *slot.borrow_mut() = Some(now));
    }
    LAST_ARMED.with(|slot| *slot.borrow_mut() = opened);
    let swapped = SWAPPED.with(|cell| cell.take());
    SPARE_ARMS.with(|log| log.borrow_mut().push(SpareArm { at: opened, prerolled, swapped }));
    opened
}

/// Note that this tick crossed onto the spare, before the arm entry is written ([`spare_arms_for_test`]).
pub fn note_spare_swapped(found: bool) {
    SWAPPED.with(|cell| cell.set(Some(found)));
}

/// The arms [`arm_spare_page`] has recorded, oldest first — the evidence that a jump was prerolled BEFORE
/// the line got there, which is the whole point of the spare. The program never reads this log (the arm and
/// swap paths act on each entry as they are written); it is the outside look a test needs.
pub fn spare_arms_for_test() -> Vec<SpareArm> {
    SPARE_ARMS.with(|log| log.borrow().clone())
}

/// Forget the arm log AND the armed-second memory, so a caller measures only what its own ticks did. A test
/// door with [`spare_arms_for_test`]; the program's own reset is [`clear_spare`].
pub fn clear_spare_arms_for_test() {
    SPARE_ARMS.with(|log| log.borrow_mut().clear());
    LAST_ARMED.with(|slot| *slot.borrow_mut() = None);
}

fn previous_spare() -> Option<f64> {
    LAST_ARMED.with(|slot| *slot.borrow())
}

/// Which second the spare slot holds, if any — a LOOK, with no side effect (the arm bookkeeping keeps its
/// own copy in [`previous_spare`]).
pub fn peek_spare() -> Option<f64> {
    SPARE.with(|slot| *slot.borrow())
}

/// Whether the head has reached the second the spare was prerolled for — the moment to swap it in. The
/// armed second is handed in by the caller, read from the tick ([`Tick::armed_before`], the second held on
/// the way IN, before this tick re-armed for the next jump): `P.eng.preloadToleranceSeconds` is float slack
/// between two numbers computed from the same cut, not a search radius.
pub fn reached_spare(armed: Option<f64>, t: f64) -> bool {
    armed.is_some_and(|armed| (armed - t).abs() <= crate::preview::PRELOAD_TOLERANCE_SECONDS)
}

/// Release the spare (playback paused, the cut changed, the review took the pipeline instead).
///
/// The page's real process goes with it — a spare left prerolled is a process holding a file and a
/// pipeline for a jump that will now never happen.
pub fn clear_spare(page: &dyn CutPreviewPage) {
    SPARE.with(|slot| *slot.borrow_mut() = None);
    LAST_ARMED.with(|slot| *slot.borrow_mut() = None);
    page.drop_spare();
}

/// S5's three "while playing" answers, routed. The page owns the processes and the volume door; the leg
/// only reads [`frame`] and says which door to knock on, so the ordering lives here and nowhere else.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Routings {
    /// A stop is under the line: the picture holds its still.
    pub still: bool,
    /// The flat rate to hand the player ([`crate::cut_play_back::set_rate`]).
    pub rate: f64,
    /// The gain for these seconds — the room's slider × the cut's own volume effects, the same rule F2.5's
    /// page asks ([`cut_hear::mix_gain`] over [`cut_hear::gain_under`]). A GAIN, in `0..=MAX_GAIN`: the page
    /// that owns the volume door decides how a gain reaches it, and must not write it back as a slider
    /// percent (that would move the room's own loudness). See the door on [`CutPreviewPage::apply_gain`].
    pub gain: f64,
}

/// S5: what to route for the seconds under the line, in the order one frame asks them.
pub fn route(cut: &Cut, t: f64, volume: f64) -> Routings {
    let under = cut_play::visible_seg_at(&cut.segs, t);
    let frame = frame(cut, under, t);
    Routings {
        still: frame.still,
        rate: frame.rate,
        gain: cut_hear::mix_gain(volume, if cut_hear::bed_silenced_under(&cut.fx, t) { 0.0 } else { cut_hear::gain_under(&cut.fx, t) }),
    }
}

thread_local! {
    static SPARE: std::cell::RefCell<Option<f64>> = const { std::cell::RefCell::new(None) };
    /// The second armed last tick, so a re-arm of the same second is recognisable as nothing to do.
    static LAST_ARMED: std::cell::RefCell<Option<f64>> = const { std::cell::RefCell::new(None) };
    /// When an arm last built a process, for [`cut_play::worth_arming`]'s cadence.
    static LAST_ARM_AT_MS: std::cell::RefCell<Option<u64>> = const { std::cell::RefCell::new(None) };
    static SPARE_ARMS: std::cell::RefCell<Vec<SpareArm>> = const { std::cell::RefCell::new(Vec::new()) };
    /// This tick's swap outcome, set by the loop and folded into the arm entry written just after it.
    static SWAPPED: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
}

/// What the preview should show for one frame while ▶✂ runs: a still or live picture, the flat rate,
/// and whether the volume effects are in force (S5's three "while playing" answers, one read).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    pub still: bool,
    pub rate: f64,
    pub volume: bool,
}

/// S5: the frame answer for the segment under `t`. `None` from `visible_seg_at` (the line sits in a
/// removed stretch) means nothing of the cut is playing, so no rate of its own and no still of its own.
pub fn frame(cut: &Cut, seg: Option<&Seg>, t: f64) -> Frame {
    Frame {
        still: cut_play::shows_a_still(cut, t),
        rate: seg.map(cut_play::played_rate).unwrap_or(cut_play::FLAT_RATE),
        volume: seg.map(cut_play::volume_applies).unwrap_or(true),
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

/// How far the removed stretches are dimmed: a solid neutral grey, `rgb(0.45, 0.45, 0.45)`.
///
/// A translucent black wash cannot satisfy S6 here: over the strip's 0.95 ground it is a function of
/// the alpha alone, and any alpha high enough to beat the ground also muddies the kept fills it must
/// stay distinct from. A solid mid-grey settles both sides at once — clearly darker than the 0.95
/// ground and than the pale kept green (lit ~0.71), while the kept boxes paint AFTER this so they
/// always win, and it matches the 0.45 "dimmed black" weight `fx_lane::DIM_ALPHA` already uses for
/// the same idea on the paused frame.
///
/// The grey itself is [`crate::ui::cut_colours::SCRIM`], §6's colour: the strip and the band dim their dropped
/// stretches with one number, so it is spelled once and read here rather than repeated as a literal.
pub const DIM_GREY: f64 = crate::ui::cut_colours::SCRIM.0;

/// The old name stays as the alpha this replaced, so a reader who knows the number finds the reason it
/// went: 0.18 was measured invisible against the ground in `rust/shots/05-cut.png` (§F2.2 S6 redo).
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
    cr.set_source_rgb(DIM_GREY, DIM_GREY, DIM_GREY);
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
            // S5, in the order the frame is asked its questions: the swap first (the join must not wait),
            // then the spare for the NEXT jump, then the three answers for these seconds.
            let onto_the_spare = step.jumped
                || reached_spare(step.armed_before, step.head)
                || reached_spare(peek_spare(), step.head);
            if onto_the_spare {
                let found = page.cross_into_spare();
                note_spare_swapped(found);
                if found {
                    clear_spare(&*page);
                }
            }
            arm_spare_page(&cut_, step.head, &*page);
            let answers = route(&cut_, step.head, page.volume());
            let released = page.hold_still(answers.still);
            // Only ask for a rate when the process is not being held on a stop's frame: a frozen player
            // would be replaced by the re-cue and the still would be lost.
            if changed_rate(answers.rate) && !released {
                page.play_rate(answers.rate, step.head);
            }
            page.apply_gain(answers.gain, step.head);
            page.say_clock(step.clock, step.ended);
            // Past the last clip the preview pauses (S5), so the processes it was holding go back now.
            if step.ended {
                clear_spare(&*page);
            }
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
    /// The preview's ONE volume, as a gain — the number `route` multiplies the cut's own by (F2.5 S6).
    fn volume(&self) -> f64;
    /// S5: preroll the spare process at `at` — the page names the files (only it can reach the session's
    /// source list), the leg owns the process. Answers whether a pipeline was opened.
    fn arm_spare_at(&self, at: f64) -> bool;
    /// S5: the head landed on the second the spare holds — swap it in rather than rebuilding anything.
    fn cross_into_spare(&self) -> bool;
    /// S5: release the spare process (pause, or the cut changed under it).
    fn drop_spare(&self);
    /// S5 "stops show their still": freeze or release the running player's frame.
    fn hold_still(&self, hold: bool) -> bool;
    /// S5 "speed effects at their flat rate": hand the rate to the player at the head.
    fn play_rate(&self, rate: f64, at: f64) -> bool;
    /// S5 "volume effects apply": hand this second's GAIN to the preview's sound. Not a percent, and the
    /// page must not write it into the slider — the slider belongs to the room (F2.5 S6); the second's own
    /// gain goes to the player's volume, the way `cut_hear::Mix::gain` is handed about everywhere else.
    /// `at_secs` travels with it because the leg asks playbin's volume property AT a second (F3.6 S5).
    fn apply_gain(&self, gain: f64, at_secs: f64);
}

/// The cut's rate for these seconds, as last handed to the player — so a re-cue costs one per rate CHANGE,
/// not one per tick. Thread-local because each window's tick is the only writer of its own preview.
fn changed_rate(rate: f64) -> bool {
    thread_local! {
        static RATE: std::cell::Cell<f64> = const { std::cell::Cell::new(f64::NAN) };
    }
    RATE.with(|cell| {
        let previous = cell.get();
        cell.set(rate);
        // NAN on the first tick, which means "hand it once".
        (previous - rate).abs() > f64::EPSILON
    })
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
pub(crate) fn kept_length(cut: &Cut) -> f64 {
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
