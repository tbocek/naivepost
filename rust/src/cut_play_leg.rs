//! F2.2: the leg ▶✂ drives WHILE the cut preview runs. [`crate::cut_play`] holds the rules; this seam owns the one idle spare pipeline S5 arms ahead of each jump and paints the dimmed removed stretches (S6). Everything delegates to `cut_play::*`. Kept apart so `cut_play`'s pure arithmetic tests carry no surface or process state.

use std::rc::Rc;

use crate::{
    cut::{Cut, Seg},
    cut_hear, cut_play,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tick {
    pub head: f64,
    pub clock: f64,
    pub ended: bool,
    pub armed_before: Option<f64>,
    pub jumped: bool,
}

/// S5: advance the preview one tick. A jump over a removed stretch is reported as the new head, not elapsed time. `clock` falls back to the whole cut length when the head is off kept material (only on `ended`).
pub fn tick(cut: &Cut, from: f64, elapsed_ms: u64) -> Tick {
    let elapsed = elapsed_ms as f64 / 1000.0;
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
            // Hold the line on the last kept end rather than where the tick overshot.
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

/// S5: arm the spare pipeline for the next jump, three seconds ahead (`P.eng.preloadLeadSeconds`). Returns the second opened and remembers it as the process's one armed spare until the next arm or [`clear_spare`]; one preview plays at a time.
///
/// The arm is logged in [`spare_arms_for_test`] (second → outcome); the page starts the process (`CutPreviewPage::arm_spare_at`) as only it can name the footage.
pub fn arm_spare(cut: &Cut, t: f64) -> Option<f64> {
    let opened = cut_play::preload_ahead(&cut.segs, t);
    SPARE.with(|slot| *slot.borrow_mut() = opened);
    opened
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpareArm {
    pub at: Option<f64>,
    pub prerolled: bool,
    pub swapped: Option<bool>,
}

/// Ask for the next jump's spare AND record it, as the tick loop does. See [`arm_spare`] and [`spare_arms_for_test`].
pub fn arm_spare_page(cut: &Cut, t: f64, page: &dyn CutPreviewPage) -> Option<f64> {
    let opened = arm_spare(cut, t);
    // Re-arming the held second would only rebuild a pipeline, so the page is asked only when the armed second CHANGED and at the §6 lead's cadence ([`cut_play::worth_arming`], on times handed in).
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
    SPARE_ARMS.with(|log| {
        log.borrow_mut().push(SpareArm {
            at: opened,
            prerolled,
            swapped,
        })
    });
    opened
}

pub fn note_spare_swapped(found: bool) {
    SWAPPED.with(|cell| cell.set(Some(found)));
}

/// The arms [`arm_spare_page`] has recorded, oldest first: evidence that a jump was prerolled BEFORE the line got there. Test-only.
pub fn spare_arms_for_test() -> Vec<SpareArm> {
    SPARE_ARMS.with(|log| log.borrow().clone())
}

/// Forget the arm log AND the armed-second memory; test door. The program resets via [`clear_spare`].
pub fn clear_spare_arms_for_test() {
    SPARE_ARMS.with(|log| log.borrow_mut().clear());
    LAST_ARMED.with(|slot| *slot.borrow_mut() = None);
}

fn previous_spare() -> Option<f64> {
    LAST_ARMED.with(|slot| *slot.borrow())
}

/// Which second the spare slot holds, if any; no side effect.
pub fn peek_spare() -> Option<f64> {
    SPARE.with(|slot| *slot.borrow())
}

/// Whether the head reached the second the spare was prerolled for. The armed second comes from the tick ([`Tick::armed_before`], held on the way IN); `P.eng.preloadToleranceSeconds` is float slack, not a search radius.
pub fn reached_spare(armed: Option<f64>, t: f64) -> bool {
    armed.is_some_and(|armed| (armed - t).abs() <= crate::preview::PRELOAD_TOLERANCE_SECONDS)
}

/// Release the spare (paused, cut changed, or the review took the pipeline); the page's real process goes with it.
pub fn clear_spare(page: &dyn CutPreviewPage) {
    SPARE.with(|slot| *slot.borrow_mut() = None);
    LAST_ARMED.with(|slot| *slot.borrow_mut() = None);
    page.drop_spare();
}

/// S5's three "while playing" answers, routed: the leg reads [`frame`] and says which door to knock on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Routings {
    pub still: bool,
    pub rate: f64,
    /// The gain for these seconds: the room's slider × the cut's volume effects ([`cut_hear::mix_gain`] over [`cut_hear::gain_under`]), in `0..=MAX_GAIN`; the page must not write it back as a slider percent. See [`CutPreviewPage::apply_gain`].
    pub gain: f64,
}

pub fn route(cut: &Cut, t: f64, volume: f64) -> Routings {
    let under = cut_play::visible_seg_at(&cut.segs, t);
    let frame = frame(cut, under, t);
    Routings {
        still: frame.still,
        rate: frame.rate,
        gain: cut_hear::mix_gain(
            volume,
            if cut_hear::bed_silenced_under(&cut.fx, t) {
                0.0
            } else {
                cut_hear::gain_under(&cut.fx, t)
            },
        ),
    }
}

thread_local! {
    static SPARE: std::cell::RefCell<Option<f64>> = const { std::cell::RefCell::new(None) };
    static LAST_ARMED: std::cell::RefCell<Option<f64>> = const { std::cell::RefCell::new(None) };
    static LAST_ARM_AT_MS: std::cell::RefCell<Option<u64>> = const { std::cell::RefCell::new(None) };
    static SPARE_ARMS: std::cell::RefCell<Vec<SpareArm>> = const { std::cell::RefCell::new(Vec::new()) };
    static SWAPPED: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
}

/// What the preview shows for one frame while ▶✂ runs: a still or live picture, the flat rate, and whether volume effects apply.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    pub still: bool,
    pub rate: f64,
    pub volume: bool,
}

/// S5: the frame answer for the segment under `t`; `None` from `visible_seg_at` (line in a removed stretch) means nothing plays.
pub fn frame(cut: &Cut, seg: Option<&Seg>, t: f64) -> Frame {
    Frame {
        still: cut_play::shows_a_still(cut, t),
        rate: seg
            .map(cut_play::played_rate)
            .unwrap_or(cut_play::FLAT_RATE),
        volume: seg.map(cut_play::volume_applies).unwrap_or(true),
    }
}

/// S2's refusal for ways in that CANNOT switch the preview (Space, picture click, run bar); `None` means toggle as usual.
pub fn other_way_in(cut: &Cut) -> Option<&'static str> {
    if cut_play::empty(cut) {
        Some(cut_play::EMPTY_CUT_REFUSAL)
    } else {
        None
    }
}

pub fn dimmed(cut: &Cut, filmed: &[(f64, f64)]) -> Vec<(f64, f64)> {
    cut_play::dimmed_spans(filmed, cut)
}

/// The removed stretches are dimmed with a solid grey, `rgb(0.45, 0.45, 0.45)`: a translucent black wash cannot stay darker than the 0.95 ground without muddying the kept fills, which paint AFTER it. Same weight as `fx_lane::DIM_ALPHA`.
/// The grey is [`crate::ui::cut_colours::SCRIM`] (§6), shared with the band.
pub const DIM_GREY: f64 = crate::ui::cut_colours::SCRIM.0;

/// The old alpha: 0.18 measured invisible against the ground (§F2.2 S6 redo).
pub const DIM_ALPHA: f64 = 0.18;

/// S5: run the cut preview's tick loop while ▶✂ is the preview. Started once per window from `build_window`; every [`crate::preview::TICK_MS`] it asks [`tick`] where the line goes, moves the playhead, arms the spare ([`arm_spare`]) and writes the CUT's time to the status line ([`cut_play::cut_clock_note`]; no clock widget exists yet). No-op unless the preview is the plain cut.
pub fn start_cut_preview_tick(page: Rc<dyn CutPreviewPage>) {
    glib::timeout_add_local(
        std::time::Duration::from_millis(crate::preview::TICK_MS),
        move || {
            // Only a PLAYING ▶✂ drives this loop; a paused cut's line and picture stand.
            if !page.cut_only() || !page.playing() {
                return glib::ControlFlow::Continue;
            }
            let from = page
                .player_second()
                .or_else(|| page.playhead())
                .unwrap_or(0.0);
            let cut_ = page.cut();
            let step = tick(&cut_, from, crate::preview::TICK_MS);
            page.set_playhead(step.head);
            // S5 order: the swap first (the join must not wait), then the spare for the NEXT jump, then the three answers.
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
            // Ask for a rate only when the process is not held on a stop's frame; a re-cue would lose the still.
            if changed_rate(answers.rate) && !released {
                page.play_rate(answers.rate, step.head);
            }
            page.apply_gain(answers.gain, step.head);
            page.say_clock(step.clock, step.ended);
            if step.ended {
                clear_spare(&*page);
                // Past the last clip the preview pauses. Left playing, this tick moved the line back and wrote all this again ten times a second.
                page.pause_at_end();
            }
            glib::ControlFlow::Continue
        },
    );
}

/// What the tick loop needs from the page; `src/ui/window.rs` implements it over its player and cut slots.
pub trait CutPreviewPage {
    fn cut_only(&self) -> bool;
    /// Is the transport playing? A paused ▶✂ holds its line (S4); defaults to playing.
    fn playing(&self) -> bool {
        true
    }
    fn playhead(&self) -> Option<f64>;
    fn set_playhead(&self, at: f64);
    fn cut(&self) -> Cut;
    fn say_clock(&self, clock: f64, ended: bool);
    /// The line ran past the last clip: stop the transport and what plays, so a later tick has nothing to do.
    fn pause_at_end(&self) {}
    fn volume(&self) -> f64;
    /// S5: preroll the spare at `at`; the page names the files, the leg owns the process. Answers whether a pipeline opened.
    fn arm_spare_at(&self, at: f64) -> bool;
    fn cross_into_spare(&self) -> bool;
    fn drop_spare(&self);
    fn hold_still(&self, hold: bool) -> bool;
    fn play_rate(&self, rate: f64, at: f64) -> bool;
    /// S5 "volume effects apply": hand this second's GAIN to the preview sound, not a percent and never into the slider (F2.5 S6). `at_secs` because playbin's volume is set AT a second (F3.6 S5).
    fn apply_gain(&self, gain: f64, at_secs: f64);
    /// Where the player stands as a session second, when it can say; the tick starts there so the line cannot drift from the picture. `None` keeps the line's own second.
    fn player_second(&self) -> Option<f64> {
        None
    }
}

/// The cut's rate as last handed to the player, so a re-cue costs one per rate CHANGE; thread-local, as each window's tick is the only writer.
fn changed_rate(rate: f64) -> bool {
    thread_local! {
        static RATE: std::cell::Cell<f64> = const { std::cell::Cell::new(f64::NAN) };
    }
    RATE.with(|cell| {
        let previous = cell.get();
        cell.set(rate);
        (previous - rate).abs() > f64::EPSILON
    })
}

/// The status-line sentence the tick writes: the CUT's time, explained as such (S5).
pub fn clock_line(clock: f64, ended: bool) -> String {
    let face = crate::preview::clock(Some(clock));
    if ended {
        format!(
            ">>> preview: {face} \u{2014} past the last clip, paused \u{b7} {}",
            cut_play::cut_clock_note()
        )
    } else {
        format!(">>> preview: {face} \u{b7} {}", cut_play::cut_clock_note())
    }
}

/// The finished video's length: visible clips with `e > s`, as [`crate::shell::cut_seconds`] counts.
pub(crate) fn kept_length(cut: &Cut) -> f64 {
    cut.segs
        .iter()
        .filter(|seg| seg.e > seg.s)
        .map(|seg| seg.e - seg.s)
        .sum()
}

fn last_kept_end(cut: &Cut) -> f64 {
    cut.segs
        .iter()
        .filter(|seg| seg.e > seg.s)
        .map(|seg| seg.e)
        .fold(0.0, f64::max)
}
