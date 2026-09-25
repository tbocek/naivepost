//! F0.4 "I'm feeling lucky" — the whole pipeline in one press, as a state machine with no window in
//! it. The chain is a *list* that the driver advances: each step reports what became of it and this
//! module decides whether the next runs or the press is over. Nothing here spawns a process, switches
//! a page or reads a project, which is what lets S4-S6 be tested at all (spec/00-principles.md §5).
//!
//! Mirrors the prototype `gui/runchain.go`: `chainRun` (:32) starts, `chainNext` (:63) takes the
//! next step off the list, `chainEnd` (:134) says what the press cost, and `chainTime` (:52) is how a
//! duration is spelled. The steps are the pages in pipeline order because that is the only order that
//! makes sense — the cut reads what Prepare wrote, the narration is written over the cut, and the
//! render needs both.
//!
//! Two places the spec overrides the prototype, both noted where they apply:
//! - The prototype emits **no** end line when nothing got tallied (`chainEnd` returns early on an
//!   empty `chainRan`). §S6 requires the line with N = 4 even then, so we emit it.
//! - A single step run through ▶ never gets an "all done" sentence (`tests/press_play.rs:344` pins
//!   that for F0.2); the chain is the only thing allowed to say "all done", so that rule lives here
//!   and nowhere else.

use crate::run::{self, Step};
use crate::shell::Page;

/// S1: the press refused because something else is going. Names ⏹ in the sentence so the reader is
/// told which button to press rather than being left to find it.
pub const BUSY_REFUSAL: &str = "a run is already active \u{2014} stop it first (\u{23f9})";

/// S4's skip sentences, in the spec's own wording (em dash U+2014).
pub const CUT_SKIPPED: &str =
    ">>> run: Cut skipped \u{2014} the cut has hand edits, which are kept";
pub const NARRATE_SKIPPED: &str =
    ">>> run: Narrate skipped \u{2014} this video has no narration";

/// The button's label. §03 names the control by this phrase and the prototype's run-bar image shows it
/// as text beside the gears, so the words go on the button rather than being implied by an icon.
pub const LUCKY_LABEL: &str = "I'm feeling lucky";

/// The button's tooltip. Mirrors the prototype's step-picker tooltip (gui/runchain.go:220-221), which
/// is the only explanatory text that control carries; the prototype supplies no separate tooltip for the
/// press itself, so this states what the press does and where it goes.
pub const LUCKY_TOOLTIP: &str = "Run every step, in page order: Prepare \u{2192} Cut \u{2192} Narrate \u{2192} Produce. \
Narrate skips itself when the video has no narration, and Cut skips itself when the cut has hand edits.";

/// The four steps in the order the pipeline runs them, taken from [`Page::all`] so the order lives in
/// one place. Each entry is the page to switch to and the name the log calls it by — which differs from
/// the page's own step label only in that Cut's step is "Suggest" while the chain calls it "Cut".
fn chain_steps() -> [(Page, &'static str); 4] {
    [
        (Page::Prepare, Page::Prepare.label()),
        (Page::Cut, Page::Cut.label()),
        (Page::Narrate, Page::Narrate.label()),
        (Page::Produce, Page::Produce.label()),
    ]
}

/// S3: the opening line, listing every step that will be attempted. A step that goes on to skip itself
/// is still named here — the list is what was asked for, not what turned out to run.
pub fn opening_line() -> String {
    let names: Vec<&str> = chain_steps().iter().map(|(_, name)| *name).collect();
    format!(">>> run: {}", names.join(" \u{2192} "))
}

/// S4's per-step line: which step is starting.
pub fn step_line(name: &str) -> String {
    format!(">>> run: {name}")
}

/// A stretch of a run said the way a person would: under a minute in seconds, over it in minutes and
/// seconds with the seconds zero-padded ("2m 03s"). Not `mm:ss` — "3:04" beside a step name reads as
/// a timestamp in the session rather than as how long the step took, and this is the one number on the
/// page that is a duration. Mirrors prototype `chainTime`, gui/runchain.go:52.
pub fn chain_time(seconds: u64) -> String {
    if seconds < 60 {
        return format!("{seconds}s");
    }
    format!("{}m {:02}s", seconds / 60, seconds % 60)
}

/// What the driver reports back after a step it was handed. The chain decides what that means; the
/// caller never decides the next move itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepOutcome {
    /// The step ran and took this many seconds.
    Ran { seconds: u64 },
    /// The step refused at its own start (nothing to do, or a precondition unmet). Carried past, not
    /// waited for, and it earns no tally entry because there was no duration.
    Declined,
    /// A step skipped itself here rather than being reported by the driver: §S4's narration-off and
    /// hand-edits branches both report this after [`Advance::Skipped`] so the chain advances past it.
    /// It closes no running step, because the skip was decided by the chain, not by a step ending.
    Skipped,
    /// The step failed. Per the prototype a failing step logs its failure and the run **walks on** —
    /// it is not a halt, unlike a stop.
    Failed,
    /// ⏹ was pressed. This ends the chain right here (§S5).
    Stopped,
}

impl StepOutcome {
    /// Whether this outcome leaves a duration for the end line's tally. A declined or skipped step has
    /// none, and putting one in would tell the reader the press spent time it did not.
    pub fn tallies(self) -> bool {
        matches!(self, Self::Ran { .. })
    }
}

/// What the driver should do next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Advance {
    /// Switch to this page synchronously (F0.1), log [`step_line`], and run that step. `name` is what
    /// the log calls it; the caller logs [`step_line(name)`] itself, so the name arrives before any
    /// outcome does — which is why the chain tracks the running step separately from `pending`.
    Run { page: Page, name: &'static str },
    /// The press is over. `line` is the full log sentence, `status` the short mirror for the status
    /// line — S6 gives both, and they are not the same text.
    End { line: String, status: String },
    /// A step skipped itself (§S4). Log `line` and call [`Chain::next`] again with
    /// [`StepOutcome::Skipped`] to move on to whatever comes after it.
    Skipped { line: String },
    /// Nothing left to do and nothing to say — the chain was already finished.
    Idle,
}

/// One press of "I'm feeling lucky".
#[derive(Debug, Default)]
pub struct Chain {
    /// Steps not yet reached, in page order.
    pending: Vec<(Page, &'static str)>,
    /// The step handed out by the last [`Advance::Run`] and not yet reported back. Kept apart from
    /// `pending` because a running step has been reached but is not finished: conflating the two makes
    /// the count off by one at every hand-over.
    running: Option<&'static str>,
    /// The name of the step most recently handed out, kept so a `Ran` outcome can be tallied under the
    /// right name after `running` has been cleared.
    last_reported: Option<&'static str>,
    /// The steps that actually ran, spelled for the end line's tally: "Prepare 2m 03s".
    ran: Vec<String>,
    /// Seconds spent so far, summed from the reported durations so the total does not depend on a
    /// clock this module cannot read.
    elapsed: u64,
    /// §S4's two skip conditions, held as data the driver sets rather than callbacks into the pages.
    narration_off: bool,
    hand_edits: bool,
    /// Set once the chain has emitted its end line, so a second report cannot say "all done" twice.
    finished: bool,
}

impl Chain {
    /// S1 + S2: refuse if anything is busy, otherwise open the chain. The returned line is S3's — the
    /// order the press intends, logged before any step has run.
    pub fn start(busy: bool) -> Result<(Chain, String), &'static str> {
        if busy {
            return Err(BUSY_REFUSAL);
        }
        let chain = Chain {
            pending: chain_steps().to_vec(),
            ..Default::default()
        };
        Ok((chain, opening_line()))
    }

    /// The skip condition for a narration-less video (§S4).
    pub fn set_narration_off(&mut self, off: bool) {
        self.narration_off = off;
    }

    /// The skip condition for a cut carrying hand edits (§S4).
    pub fn set_hand_edits(&mut self, edits: bool) {
        self.hand_edits = edits;
    }

    /// How many steps have neither been reached nor are running.
    pub fn left(&self) -> usize {
        self.pending.len()
    }

    /// Every step the press will not get through: still pending, plus one now running if the chain ends
    /// mid-step.
    fn undone(&self) -> usize {
        self.pending.len() + usize::from(self.running.is_some())
    }

    /// The tally so far, as it will appear in the end line.
    pub fn tally(&self) -> String {
        self.ran.join(", ")
    }

    /// S4 → S6. Feed it the outcome of the step last handed over and it says what comes next: another
    /// step, a skip sentence, or the end.
    ///
    /// Skip decisions are taken from the chain's own flags *before* the step is handed back, so a
    /// skipped step is carried past rather than waited for — the prototype `continue`s in exactly this
    /// spot, and waiting on a step that was never started is how a chain hangs.
    pub fn next(&mut self, outcome: StepOutcome) -> Advance {
        if self.finished {
            return Advance::Idle;
        }

        // S5: a stop ends the press here, whatever else was pending or running.
        if outcome == StepOutcome::Stopped {
            return self.end("stopped");
        }

        // Close out the step that was handed out, if any. A skip is not an outcome about that step —
        // it was decided here, not reported — so it must not consume the running step.
        if outcome != StepOutcome::Skipped && self.running.take().is_some() {
            match outcome {
                StepOutcome::Ran { seconds } => {
                    self.elapsed += seconds;
                    // The tally names the step that just finished, so take it from `running`, which
                    // still holds it until the line above clears it.
                    if let Some(name) = self.last_reported {
                        self.ran.push(format!("{name} {}", chain_time(seconds)));
                    }
                }
                // A failed step is logged by the driver with the step's own reason and the chain walks
                // on (prototype: `endRun` records the failure and calls `chainNext`). Declined is the
                // same shape: nothing ran, so nothing is tallied.
                StepOutcome::Failed | StepOutcome::Declined => {}
                _ => {}
            }
        }

        // Walk forward to the first step that is neither skipped nor already dealt with.
        loop {
            let Some((page, name)) = self.pending.first().copied() else {
                return self.end("done");
            };
            if page == Page::Narrate && self.narration_off {
                self.pending.remove(0);
                return Advance::Skipped {
                    line: NARRATE_SKIPPED.to_string(),
                };
            }
            if page == Page::Cut && self.hand_edits {
                self.pending.remove(0);
                return Advance::Skipped {
                    line: CUT_SKIPPED.to_string(),
                };
            }
            // Hand it over. It leaves `pending` here and lives in `running` until its outcome lands,
            // so "left" never counts a step that is already under way.
            self.pending.remove(0);
            self.running = Some(name);
            self.last_reported = Some(name);
            return Advance::Run { page, name };
        }
    }

    /// S6: the end line and its status mirror. Which of the three forms is chosen by why the chain
    /// stopped and how much of it got done.
    fn end(&mut self, how: &str) -> Advance {
        self.finished = true;
        let left = self.undone();
        self.pending.clear();
        self.running = None;
        let total = chain_time(self.elapsed);
        let tally = self.tally();

        // §S6 overrides the prototype here: `chainEnd` returns with no line at all when nothing was
        // tallied, but the spec asks for the same sentence with N = 4. Saying nothing leaves the reader
        // with a silent press, which is the thing S6 exists to prevent.
        if how == "stopped" {
            return Advance::End {
                line: format!(
                    ">>> run: stopped after {total} \u{2014} {tally} \u{2014} {left} step(s) left undone"
                ),
                status: format!("stopped after {total}"),
            };
        }
        if left > 0 {
            return Advance::End {
                line: format!(
                    ">>> run: {total} \u{2014} {tally} \u{2014} {left} step(s) left undone"
                ),
                status: format!("ran {total}, {left} left undone"),
            };
        }
        Advance::End {
            line: format!(">>> run: all done in {total} \u{2014} {tally}"),
            status: format!("all done in {total}"),
        }
    }
}


/// The step a page's chain entry corresponds to, exposed so a test can pin the mapping without
/// reaching into [`chain_steps`].
pub fn step_of(page: Page) -> Step {
    run::step(page)
}
