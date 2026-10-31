//! F0.5: a run's bookkeeping (spec/03-shell.md F0.5), the port of `go/runqueue.go`. Nothing here touches a widget.
//! * The **fraction** is never the queue's length (work is found mid-run and a growing total drags the needle back). Each track reports its absolute contribution and the bar shows the sum, mapped through [`Queue::phase`].
//! * The **words** are two or three per job, no filename or count; the counting goes in the tooltip: `describe 1/2: chunk 4/12`.

use std::time::Duration;

use crate::exchanges::RunLog;
use crate::run::{Run, RunBar, Snapshot, Step};
use crate::services::{Kind, UNLOAD_IS_BEST_EFFORT, UNLOAD_TIMEOUT};

/// S3: how often a paused run re-asks whether it may go on. §10 gives it no `P.` id (only prose), so it takes the bare `machine.` prefix, like `machine.jpegQuality`.
pub const CHECKPOINT_POLL: Duration = Duration::from_millis(200);

/// The bar's idle pulse while a model thinks, until something countable arrives (§10 "progress pulse 150 ms"; prototype `pulseUntilCounted`).
pub const PROGRESS_PULSE: Duration = Duration::from_millis(150);

/// Which half of the bar. Each track owns half and reports its absolute contribution, concurrent (Prepare) or sequential (Describe + Transcript); a raw fraction would make the bar bounce.
pub const TRACK_STT: usize = 0;
pub const TRACK_FRAMES: usize = 1;
/// The same halves under Describe + Transcript's names.
pub const TRACK_DESCRIBE: usize = TRACK_STT;
pub const TRACK_FIX: usize = TRACK_FRAMES;

/// §2: the tracks' lines are joined by a middle dot with TWO spaces each side (spec/03-shell.md §2); one const so joiner and test agree.
pub const TRACK_JOIN: &str = "  \u{00B7}  ";
/// §2: the bar's tooltip when nothing is queued.
pub const STANDING_TIP: &str =
    "The run: the job, which of the run's jobs it is, and the task it is on";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Track {
    pub job: String,
    pub phase: usize,
    pub of: usize,
    pub queued: usize,
    pub taken: usize,
    pub what: String,
    pub kind: String,
    pub done_job: bool,
    pub ever_filled: bool,
}

impl Track {
    /// A track's whole contribution to the bar's text:
    ///
    /// ```text
    /// describe 1/2: chunk 4/12
    /// transcript 2/2: fixing block 3/7
    /// speech: recognising 2/3
    /// ```
    ///
    /// Every piece is dropped when it has nothing to say, so a job with one task and no phases is
    /// simply `thumbnail: drawing`.
    pub fn line(&self) -> String {
        if self.done_job {
            return if self.job.is_empty() {
                String::new()
            } else {
                format!("{} done", self.job)
            };
        }
        let mut head = self.job.clone();
        if self.of > 1 {
            head.push_str(&format!(" {}/{}", self.phase, self.of));
        }
        let mut what = if self.what.is_empty() {
            self.kind.clone()
        } else {
            self.what.clone()
        };
        if self.ever_filled && self.taken > 0 && what.is_empty() {
            what = format!("{}/{}", self.taken, self.taken.max(self.queued));
        }
        match (head.is_empty(), what.is_empty()) {
            (true, _) => what,
            (_, true) => head,
            _ => format!("{head}: {what}"),
        }
    }

    /// This track's part of the tooltip, where the counting is spelled out. Empty for a track with no job or never filled.
    pub fn tip(&self) -> String {
        if self.job.is_empty() || !self.ever_filled {
            return String::new();
        }
        let total = self.queued.max(self.taken);
        let left = total.saturating_sub(self.taken);
        if self.done_job {
            format!("{}: {} task(s), all done", self.job, self.taken)
        } else if left == 0 {
            format!("{}: task {} of {total}, none waiting", self.job, self.taken)
        } else {
            format!(
                "{}: task {} of {}, {} waiting",
                self.job, self.taken, self.queued, left
            )
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Queue {
    tracks: [Track; 2],
    parts: [f64; 2],
    base: f64,
    share: f64,
}

impl Queue {
    pub fn new() -> Self {
        Self::default()
    }

    /// S1 (`qReset`): empty both halves; the tracks are summed, so leftovers would add to the next run.
    pub fn reset(&mut self) {
        self.tracks = Default::default();
        self.parts = [0.0; 2];
        self.base = 0.0;
        self.share = 0.0;
    }

    /// Say where in the bar the next jobs are drawn (`base`, `share`) and clear the queue. Prepare's ▶ is two steps back to back, each reporting its own whole bar; scaling happens here. A phase that reported nothing stands where the last finished.
    pub fn phase(&mut self, base: f64, share: f64) {
        self.base = base;
        self.share = share;
        self.tracks = Default::default();
        self.parts = [base / 2.0; 2];
    }

    /// Map a track's fraction into the phase's slice. Share 0 means no phase: the whole bar.
    fn scaled(&self, fraction: f64) -> f64 {
        if self.share == 0.0 {
            fraction
        } else {
            self.base / 2.0 + fraction * self.share
        }
    }

    /// S2 (`qJob`): which job now owns a track and which of the run's jobs it is ("describe 1/2"). `of = 0` for one job or concurrent named jobs.
    pub fn job(&mut self, track: usize, name: &str, phase: usize, of: usize) {
        self.tracks[track] = Track {
            job: name.to_string(),
            phase,
            of,
            ..Default::default()
        };
    }

    /// S2 (`qPush`): queue `n` more tasks; called again as work is found. `n <= 0` is a no-op.
    pub fn push(&mut self, track: usize, n: usize, kind: &str) {
        if n == 0 {
            return;
        }
        let t = &mut self.tracks[track];
        t.queued += n;
        t.ever_filled = true;
        if !kind.is_empty() {
            t.kind = kind.to_string();
        }
    }

    /// S2 (`qTake`): pick the next task up, once per task whatever becomes of it; resumed work already on disk is a task too, else the queue stalls.
    pub fn take(&mut self, track: usize) {
        let t = &mut self.tracks[track];
        t.taken += 1;
        t.what = String::new();
    }

    /// S2 (`prog`): the track's absolute contribution and what its task is doing (two or three words).
    pub fn prog(&mut self, track: usize, fraction: f64, text: &str) {
        self.parts[track] = self.scaled(fraction);
        self.tracks[track].what = text.to_string();
    }

    /// S2 (`qDone`): end a track's job; it keeps its share of the bar and says only its name.
    pub fn done(&mut self, track: usize, share: f64) {
        self.parts[track] = self.scaled(share);
        self.tracks[track].done_job = true;
    }

    pub fn fraction(&self) -> f64 {
        (self.parts[0] + self.parts[1]).clamp(0.0, 1.0)
    }

    /// The status line's words and the bar's tooltip; the tooltip keeps counting for a job whose short line is quiet.
    pub fn text_and_tip(&self) -> (String, String) {
        let mut lines: Vec<String> = Vec::new();
        let mut tips: Vec<String> = Vec::new();
        for track in &self.tracks {
            let line = track.line();
            if !line.is_empty() {
                lines.push(line);
            }
            let tip = track.tip();
            if !tip.is_empty() {
                tips.push(tip);
            }
        }
        (lines.join(TRACK_JOIN), tips.join("; "))
    }

    pub fn text(&self) -> String {
        self.text_and_tip().0
    }

    pub fn track(&self, track: usize) -> &Track {
        &self.tracks[track]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Checkpoint {
    Ran,
    /// Paused: wait [`CHECKPOINT_POLL`] and ask again; not a stop.
    Waited,
    Stopped,
}

/// S3: the check every long flow makes between subprocesses. Stop wins over pause; pause polls so the GUI thread can see ⏹.
pub fn checkpoint(paused: bool, stop_flag: bool) -> Checkpoint {
    if stop_flag {
        Checkpoint::Stopped
    } else if paused {
        Checkpoint::Waited
    } else {
        Checkpoint::Ran
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Started {
    pub epoch: u64,
    pub closed_log_page: bool,
}

/// S1: `startRun`: running, flags cleared, fresh cancel context, queue reset (closing the model log page), controls, log expanded. Every ▶ and ↻ goes through here. Flags clear BEFORE the queue reset, else its repaint shows a stopped run as paused.
/// The cancel context is a generation counter: a call asks [`is_cancelled`] with its start epoch, so an old `cancelled` cannot abort this run.
pub fn start_run(
    bar: &mut RunBar,
    queue: &mut Queue,
    log: &mut RunLog,
    step: Step,
    sources: Snapshot,
) -> Started {
    let epoch = bump_cancel_context(bar);
    bar.running = Some(Run {
        step,
        paused: false,
        log_expanded: true,
        sources,
    });
    queue.reset();
    // Closing the old exchange page here, the one call every run makes at its start, so the next LLM call opens a new one. The recorder's boundary closes in the same breath (§7).
    crate::exchange_leg::note_run_started();
    crate::request_run::note_run_begins();
    let closed_log_page = log.reset().is_some();
    Started {
        epoch,
        closed_log_page,
    }
}

/// S1: the fresh cancel context on its own for callers with no queue or log; bumps the generation and clears stop and pause.
pub fn bump_cancel_context(bar: &mut RunBar) -> u64 {
    bar.run_epoch += 1;
    bar.stop_flag = false;
    // The pause flag lives on the run (F0.2's ▶ toggles `Run::paused`), so a fresh context clears it.
    if let Some(run) = bar.running.as_mut() {
        run.paused = false;
    }
    bar.cancelled = false;
    // Open the matching [`crate::cancel_leg`] switch at the SAME generation, so a leg that captured this epoch is cancellable by the next ⏹ only.
    cancel_switch(cancel_epoch(bar));
    bar.run_epoch
}

/// The generation of the cancel context a leg rides; a flow hands it to [`crate::cancel_leg::cancel_check`] at request start so a stop cancels THIS run's legs only (F0.5 S1).
pub fn cancel_epoch(bar: &RunBar) -> u64 {
    bar.run_epoch
}

/// Keep the shared cancel switch on the bar's epoch counter, so [`crate::cancel_leg::cancelled_at`] means what [`is_cancelled`] means.
pub fn cancel_switch(epoch: u64) {
    crate::cancel_leg::reset_to(epoch);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndRun {
    /// The audio server housekeeping call: `unload_all_models`, best-effort, 20 s deadline. Always present, even for runs using no audio.
    pub audio_unload: Kind,
}

/// S4: `endRun`: running off, controls refreshed, audio models unloaded off-thread. The unload is recorded, not performed here.
pub fn end_run(bar: &mut RunBar) -> EndRun {
    bar.running = None;
    bar.status.clear();
    // F0.3 S4: the run has unwound, so its stop is spent; clearing the shared switch keeps a stale ⏹ from cancelling the next run.
    crate::cancel_leg::clear();
    // §7: with the run gone the next chat call opens a fresh page.
    crate::exchange_leg::note_run_ended();
    EndRun {
        audio_unload: Kind::UnloadAll,
    }
}

/// Whether the unload [`end_run`] asks for is the bounded best-effort one §1 describes (20 s, quiet, never failing the run); for a test.
pub fn audio_unload_is_bounded() -> (Duration, bool) {
    (UNLOAD_TIMEOUT, UNLOAD_IS_BEST_EFFORT)
}

/// Whether a cancellation flagged at `epoch` applies to the run holding `now`: only equal epochs.
pub fn is_cancelled(flagged: bool, epoch: u64, now: u64) -> bool {
    flagged && epoch == now
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn f0_5_s1_start_run_clears_flags_and_opens_the_log() {
        let mut bar = RunBar::default();
        let mut queue = Queue::new();
        let mut log = RunLog::new();
        queue.push(TRACK_STT, 3, "stale");
        let started = start_run(
            &mut bar,
            &mut queue,
            &mut log,
            Step::Prepare,
            Snapshot::default(),
        );
        assert_eq!(started.epoch, 1);
        assert!(bar.running.is_some(), "startRun flips running on");
        assert!(
            !bar.stop_flag && !bar.cancelled,
            "F0.5 S1 clears both stop and cancel"
        );
        assert!(
            bar.running.as_ref().unwrap().log_expanded,
            "startRun expands the log (F0.5 S1)"
        );
        assert_eq!(queue.fraction(), 0.0, "queue reset to zero");
    }

    #[test]
    fn f0_5_s2_track_line_formats() {
        let mut q = Queue::new();
        q.job(TRACK_STT, "describe", 1, 2);
        q.push(TRACK_STT, 12, "chunk");
        for _ in 0..4 {
            q.take(TRACK_STT);
        }
        q.prog(TRACK_STT, 4.0 / 12.0, "chunk 4/12");
        let (text, tip) = q.text_and_tip();
        // Spell out the position only when the head has no words of its own; `prog`'s text already counts.
        assert_eq!(text, "describe 1/2: chunk 4/12");
        assert_eq!(tip, "describe: task 4 of 12, 8 waiting");
        assert!((q.fraction() - 4.0 / 12.0).abs() < 1e-9);
    }

    #[test]
    fn f0_5_s3_checkpoint_stop_wins_over_pause() {
        assert_eq!(checkpoint(false, false), Checkpoint::Ran);
        assert_eq!(checkpoint(true, false), Checkpoint::Waited);
        assert_eq!(checkpoint(true, true), Checkpoint::Stopped);
    }

    #[test]
    fn f0_5_s4_end_run_unloads_audio_best_effort() {
        let mut bar = RunBar::default();
        let end = end_run(&mut bar);
        assert_eq!(end.audio_unload, Kind::UnloadAll);
        assert!(bar.running.is_none());
        let (dur, best) = audio_unload_is_bounded();
        assert_eq!(dur.as_secs(), 20);
        assert!(best);
    }
}
