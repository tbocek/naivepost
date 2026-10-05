//! F2.1's tick: while ▶ runs, the line follows the player ten times a second (S4), every second plays
//! cuts and all, a folded seam the line reaches opens, and at a recording's end the line walks on (S3).
//!
//! The rules stay in [`crate::preview`] (`play_advance`, `walk_on`, `walk_fold`); this module only sequences
//! them per tick and reads/writes the page through the public `ui::` doors — no rule is decided here, and
//! `window.rs` stays out of it (it is past its size budget).

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use adw::prelude::*;

use crate::cut::Cut;
use crate::play_back;
use crate::preview::{self, TICK_MS};
use crate::ui::window::{find_status, log_line};
use crate::ui::{cut_folds, cut_record_play, window as ui};

/// What the loop needs from the page. Mirrors [`crate::cut_play_leg::CutPreviewPage`]'s shape on purpose:
/// the leg holds no GTK-slot knowledge, so the same sequence can be driven by a test table.
pub trait RecordingPreviewPage {
    /// Is OUR preview (the recording) running right now? False while paused, and false for ▶✂/▶✂✂.
    fn is_recording_playing(&self) -> bool;
    /// Where the line sits now, in session seconds.
    fn playhead(&self) -> Option<f64>;
    /// Move the line.
    fn set_playhead(&self, at: f64);
    /// This window's cut, folds included.
    fn cut(&self) -> Cut;
    /// Open the fold covering `at` and answer §B's sentence, or `None` when nothing covers it.
    fn unfold_at(&self, at: f64) -> Option<String>;
    /// Put a line on the status row.
    fn say(&self, line: &str);
}

/// A page made of plain data — what a test drives `step` with, and anything else that has the state but not
/// a window. `folds` is written back as folds open, exactly as the window page persists them.
#[derive(Debug, Default)]
pub struct TablePage {
    pub playing: bool,
    /// Written by `set_playhead`, which is why it sits behind a cell like the window's own slot does.
    pub head: RefCell<Option<f64>>,
    pub cut: RefCell<Cut>,
    pub said: RefCell<Vec<String>>,
}

impl RecordingPreviewPage for TablePage {
    fn is_recording_playing(&self) -> bool {
        self.playing
    }
    fn playhead(&self) -> Option<f64> {
        *self.head.borrow()
    }
    fn set_playhead(&self, at: f64) {
        *self.head.borrow_mut() = Some(at);
    }
    fn cut(&self) -> Cut {
        self.cut.borrow().clone()
    }
    /// The table opens its own fold, because a window page's door does the same thing to its own cut slot:
    /// the fold goes away and §B's sentence comes back.
    fn unfold_at(&self, at: f64) -> Option<String> {
        let index = self
            .cut
            .borrow()
            .folds
            .iter()
            .position(|[start, end]| at >= *start && at < *end)?;
        self.cut.borrow_mut().folds.remove(index);
        Some(preview::unfolded_status(at))
    }
    fn say(&self, line: &str) {
        self.said.borrow_mut().push(line.to_string());
    }
}

/// One tick's work, in the order inventory §D gives `followPlayback`: walk-on first, then the position,
/// then the fold. Returns what to say (a fold opening), if anything.
///
/// `runs` are the merged filmed spans ([`crate::timeline::filmed_runs`]); `elapsed_secs` is one tick's
/// worth, which is what makes this callable from a test without a timer.
pub fn step(page: &dyn RecordingPreviewPage, runs: &[(f64, f64)], elapsed_secs: f64) -> Option<String> {
    let from = page.playhead()?;
    // S3: "every second plays, cuts and all" — the advance never skips, whatever the cut removed.
    let mut at = preview::play_advance(from, elapsed_secs, false);
    // S3: at a recording's end the line walks on to the next one; a camera still rolling there keeps the
    // line where it is and the player re-cues onto that file instead.
    at = preview::walk_on(runs, at).unwrap_or(at);
    page.set_playhead(at);
    // S3: a folded seam the line ran into opens. `walk_fold` itself refuses while paused or under ✂.
    let player = preview::Player {
        transport: crate::run::Transport { playing: true, started: true },
        cut_only: false,
        reviewing: false,
        playhead: Some(at),
        ..Default::default()
    };
    let mut folds = page.cut().folds;
    let opened = preview::walk_fold(&player, &mut folds);
    if opened.is_some() {
        let said = page.unfold_at(at);
        return said.or(opened);
    }
    None
}

/// Start the recording preview's own tick, once per window, beside ▶✂'s loop and on the same
/// [`TICK_MS`] cadence — one shared rate, two owners of the line, each standing down when the other's
/// preview is showing. A second timer rather than a branch inside `start_cut_preview_tick` because that
/// loop's early-return is ▶✂'s rule (`cut_only`) and folding ours into it would put two previews'
/// conditions in one place; both cost one boolean read between presses.
///
/// Between ticks the clock face smooths itself with [`preview::live_clock`] (≤ one tick ahead at rate 1,
/// monotone) — the playhead this loop writes is already ≤ one tick old, so the smoothed value is what the
/// face shows and the raw write lands once per tick. That is S4's "line follows the player ten times a
/// second, smoothed live clock in between".
pub fn start(window: Rc<adw::ApplicationWindow>) {
    let page = Rc::new(WindowPage(window));
    let last = Rc::new(RefCell::new(Instant::now()));
    glib::timeout_add_local(Duration::from_millis(TICK_MS), move || {
        if !page.is_recording_playing() {
            return glib::ControlFlow::Continue;
        }
        let since_ms = last.borrow().elapsed().as_millis() as u64;
        *last.borrow_mut() = Instant::now();
        let runs = cut_record_play::runs_now();
        let said = step(page.as_ref(), &runs, TICK_MS as f64 / 1000.0);
        if let Some(line) = said {
            page.say(&line);
            log_line(&format!(">>> {line}"));
        }
        // The session clock, smoothed to the moment between ticks (§D's face, mm:ss.d).
        if let Some(head) = page.playhead() {
            let shown = preview::live_clock(head, since_ms, 1.0);
            if let Some(status) = find_status(page.0.upcast_ref()) {
                status.set_text(&clock_note(shown));
            }
        }
        glib::ControlFlow::Continue
    });
}

/// The status line while ▶ runs: the session clock's face and what it means. Wording travels with the loop
/// that writes it, the way [`crate::cut_play_leg::clock_line`] does for ▶✂.
pub fn clock_note(clock: f64) -> String {
    format!(
        ">>> preview: {} \u{b7} the red line, session time \u{b7} the recording, cuts and all",
        preview::clock(Some(clock))
    )
}

/// The page over a real window: every door is one `ui::` call, so no slot knowledge lives here.
struct WindowPage(Rc<adw::ApplicationWindow>);

impl RecordingPreviewPage for WindowPage {
    fn is_recording_playing(&self) -> bool {
        // S4: the line follows THE PLAYER — with no player process running there is nothing to follow, so
        // a press whose cue failed (logged where it happened) moves no line.
        play_back::playing_pid().is_some()
            && {
                let player = ui::preview_player(&self.0);
                player.transport.playing && !player.cut_only && !player.reviewing
            }
    }
    fn playhead(&self) -> Option<f64> {
        ui::preview_player(&self.0).playhead
    }
    fn set_playhead(&self, at: f64) {
        ui::set_playhead(&self.0, at);
    }
    fn cut(&self) -> Cut {
        ui::review_cut_of(&self.0)
    }
    fn unfold_at(&self, at: f64) -> Option<String> {
        let index = self
            .cut()
            .folds
            .iter()
            .position(|[start, end]| at >= *start && at < *end)?;
        cut_folds::unfold_fold_for_playback(&self.0, index)
    }
    fn say(&self, line: &str) {
        if let Some(status) = find_status(self.0.upcast_ref()) {
            status.set_text(line);
        }
    }
}
