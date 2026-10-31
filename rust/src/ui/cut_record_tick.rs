//! F2.1 tick: while ▶ runs the line follows the player ten times a second (S4), every second plays cuts and all, a reached folded seam opens, and at a recording's end the line walks on (S3).
//! Rules stay in [`crate::preview`] (`play_advance`, `walk_on`, `walk_fold`); this only sequences them through the public `ui::` doors.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use adw::prelude::*;

use crate::cut::Cut;
use crate::play_back;
use crate::preview::{self, TICK_MS};
use crate::ui::window::{find_status, log_line};
use crate::ui::{cut_folds, cut_record_play, window as ui};

/// What the loop needs from the page; mirrors [`crate::cut_play_leg::CutPreviewPage`] so a test table can drive it.
pub trait RecordingPreviewPage {
    fn is_recording_playing(&self) -> bool;
    fn playhead(&self) -> Option<f64>;
    fn set_playhead(&self, at: f64);
    fn cut(&self) -> Cut;
    fn unfold_at(&self, at: f64) -> Option<String>;
    fn say(&self, line: &str);
    /// F2.5: ask what this second hears, drive the lane pipelines to match, hand back the answer. The default does nothing.
    fn mix_now(&self, _at: f64) -> Option<crate::cut_hear::Mix> {
        None
    }
    fn root(&self) -> Option<std::path::PathBuf> {
        None
    }
    /// Where the player stands as a session second, when it can say (S4). `None` advances the line by the tick.
    fn player_second(&self) -> Option<f64> {
        None
    }
}

/// A page of plain data for driving `step` in a test; `folds` is written back as folds open.
#[derive(Debug, Default)]
pub struct TablePage {
    pub playing: bool,
    pub head: RefCell<Option<f64>>,
    pub cut: RefCell<Cut>,
    pub said: RefCell<Vec<String>>,
    /// F2.5: the mix `mix_now` hands back; `None` means the tick asks nothing.
    pub mix: RefCell<Option<crate::cut_hear::Mix>>,
    pub root: Option<std::path::PathBuf>,
}

impl RecordingPreviewPage for TablePage {
    fn is_recording_playing(&self) -> bool {
        self.playing
    }
    fn mix_now(&self, _at: f64) -> Option<crate::cut_hear::Mix> {
        self.mix.borrow().clone()
    }
    fn root(&self) -> Option<std::path::PathBuf> {
        self.root.clone()
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
    /// The table opens its own fold, as a window page's door does.
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

/// One tick's work in inventory §D's `followPlayback` order: walk-on, position, fold. Returns what to say, if anything. `runs` are [`crate::timeline::filmed_runs`].
pub fn step(
    page: &dyn RecordingPreviewPage,
    runs: &[(f64, f64)],
    elapsed_secs: f64,
) -> Option<String> {
    let from = page.playhead()?;
    // F2.12 S7: a spliced card holds the footage on the wall clock, so the tick asks `cut_card_preview` how far this second may move (0 while one is up). Asked BEFORE the advance.
    if let Some(held) = crate::ui::card_tick(&page.cut(), from, elapsed_secs, 1.0) {
        if held.advance == 0.0 {
            return held.status;
        }
    }
    // S3: the advance never skips cuts. One tick at the FLAT rate of the second the line left (§06 §2), so a hand-drawn ×2 moves twice as far.
    let mut at = page
        .player_second()
        .unwrap_or_else(|| crate::cut_speed_play::advance(&page.cut().fx, from, elapsed_secs));
    // S3: at a recording's end the line walks on; a camera still rolling keeps the line and the player re-cues.
    at = preview::walk_on(runs, at).unwrap_or(at);
    page.set_playhead(at);
    // F2.5 S1-S6: the mix follows the line, once per tick, after the advance so it is for the second reached.
    if let Some(mix) = page.mix_now(at) {
        if let Some(root) = page.root() {
            crate::lane_hear::tick(&mix, &page.cut().lanes, &root, at, &|line| log_line(line));
        }
    }
    let player = preview::Player {
        transport: crate::run::Transport {
            playing: true,
            started: true,
        },
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

/// Start the recording preview's own tick once per window, on the same [`TICK_MS`] cadence as ▶✂'s loop; each stands down when the other's preview shows.
/// Between ticks the clock face is smoothed with [`preview::live_clock`] (S4).
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
        // The built-in player ran out of its file and nothing took over: pause there.
        if crate::play_built_in::ended() {
            play_back::halt();
            ui::publish_preview_transport(crate::run::Transport {
                playing: false,
                started: true,
            });
            cut_record_play::paint_transport(&page.0);
        }
        if let Some(head) = page.playhead() {
            let shown = preview::live_clock(
                head,
                since_ms,
                crate::cut_speed_play::flat_rate_at(&page.cut().fx, head),
            );
            if let Some(status) = find_status(page.0.upcast_ref()) {
                status.set_text(&clock_note(shown));
            }
        }
        glib::ControlFlow::Continue
    });
}

/// The status line while ▶ runs: the session clock's face.
pub fn clock_note(clock: f64) -> String {
    format!(
        ">>> preview: {} \u{b7} the red line, session time \u{b7} the recording, cuts and all",
        preview::clock(Some(clock))
    )
}

struct WindowPage(Rc<adw::ApplicationWindow>);

impl RecordingPreviewPage for WindowPage {
    fn is_recording_playing(&self) -> bool {
        // S4: the line follows the player; a failed cue moves no line.
        play_back::running() && {
            let player = ui::preview_player();
            player.transport.playing && !player.cut_only && !player.reviewing
        }
    }
    fn playhead(&self) -> Option<f64> {
        ui::preview_player().playhead
    }
    fn set_playhead(&self, at: f64) {
        ui::set_playhead(&self.0, at);
    }
    fn cut(&self) -> Cut {
        crate::ui::window_preview::newest_review_cut()
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
    fn mix_now(&self, at: f64) -> Option<crate::cut_hear::Mix> {
        let cut = self.cut();
        Some(ui::apply_mix(&cut.segs, &cut.lanes, &cut.fx, at))
    }
    fn root(&self) -> Option<std::path::PathBuf> {
        ui::line_root()
    }
    fn player_second(&self) -> Option<f64> {
        cut_record_play::playing_second()
    }
}
