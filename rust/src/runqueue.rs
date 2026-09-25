//! F0.5 — a run's bookkeeping (spec/03-shell.md F0.5), the port of `gui/runqueue.go`.
//!
//! One press of ▶ turns into work, and the bar has to say where that work is without lying. Two
//! rules carry the whole module:
//!
//! * The **fraction** is never the queue's length. A queue opens as it runs (more work is found
//!   mid-run, so `push` is called again and again), and a total that grows under a fraction drags
//!   the needle backwards. Each track reports its own *absolute contribution* and the bar shows
//!   the sum, mapped through the phase's slice of the bar ([`Queue::phase`]).
//! * The **words** are two or three per job, with no filename and no count — the queue counts,
//!   the log names. `describe 1/2: chunk 4/12`, and the counting itself goes in the tooltip.
//!
//! Nothing here touches a widget: [`Queue`] is data, and the UI paints from
//! [`Queue::fraction`]/[`Queue::text_and_tip`] the same way a test reads them.

use std::time::Duration;

use crate::exchanges::RunLog;
use crate::run::{Run, RunBar, Snapshot, Step};
use crate::services::{Kind, UNLOAD_IS_BEST_EFFORT, UNLOAD_TIMEOUT};
use crate::shell::Page;

/// S3: how often a paused run re-asks whether it may go on, between subprocesses. §10 lists this
/// only inside its prose line (`spec/10-parameters.md`: "… hang watchdog 200 ms / 3 s; … progress
/// pulse 150 ms; checkpoint poll 200 ms …") and gives it no `P.` id, so it takes the bare
/// `machine.` prefix here rather than an invented one — the same choice `machine.jpegQuality` made.
pub const CHECKPOINT_POLL: Duration = Duration::from_millis(200);

/// The bar's idle pulse while a model thinks: the LLM calls hold nothing countable, so the needle
/// pulses until something with real news — the first drawing fraction — takes it (§10's prose
/// "progress pulse 150 ms"; prototype `pulseUntilCounted`, gui/runqueue.go:240).
pub const PROGRESS_PULSE: Duration = Duration::from_millis(150);

/// Which half of the bar. The two tracks are concurrent in Prepare (speech recognition on the GPU,
/// frame extraction on the CPU) and sequential on Describe + Transcript, but the arithmetic is the
/// same either way: each track owns half and reports its own absolute contribution. Letting either
/// write a raw fraction would make the bar bounce between them.
pub const TRACK_STT: usize = 0;
pub const TRACK_FRAMES: usize = 1;
/// The same two halves under the names Describe + Transcript uses them for: its jobs run one after
/// the other, but each still owns half the bar and its own line.
pub const TRACK_DESCRIBE: usize = TRACK_STT;
pub const TRACK_FIX: usize = TRACK_FRAMES;

/// One half of the bar: the job on it, the work it has left, and what the task at the head of the
/// queue is doing this second.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Track {
    /// "describe", "speech", "narrate" — what this half is.
    pub job: String,
    /// ...and which of the run's jobs it is, when there are two.
    pub phase: usize,
    pub of: usize,
    /// Tasks known about, including the ones already taken.
    pub queued: usize,
    /// How many have been picked up: the head's position.
    pub taken: usize,
    /// What the head is doing, in two or three words.
    pub what: String,
    /// What a task of this queue is, when nothing else is said.
    pub kind: String,
    /// The job finished; the other track may still be going.
    pub done_job: bool,
    /// A queue that was filled and emptied is not an unfilled one.
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
            // A finished job says its name and stops; the line belongs to whatever is still running.
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

    /// This track's part of the tooltip, where the counting is spelled out for the one moment anyone
    /// wants it. Empty for a track with no job or one that was never filled.
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
            format!("{}: task {} of {}, {} waiting", self.job, self.taken, self.queued, left)
        }
    }
}

/// The run's bookkeeping: two tracks, their shares of the bar, and the phase the next jobs draw in.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Queue {
    tracks: [Track; 2],
    parts: [f64; 2],
    base: f64,
    share: f64,
}

impl Queue {
    /// An empty queue: both halves read zero and say nothing.
    pub fn new() -> Self {
        Self::default()
    }

    /// S1 (`qReset`): empty both halves. A run starts here — the tracks are summed, so last run's
    /// leftovers would be added to every reading this one takes.
    pub fn reset(&mut self) {
        self.tracks = Default::default();
        self.parts = [0.0; 2];
        self.base = 0.0;
        self.share = 0.0;
    }

    /// Say where in the bar the jobs that come next are drawn (`base`, `share`) and clear the queue
    /// for them. A single-step press never calls this: Prepare's ▶ is two steps back to back, each
    /// reporting its own whole bar, and the scaling happens here rather than in every place that
    /// moves the needle. A phase that has reported nothing stands where the one before it finished.
    pub fn phase(&mut self, base: f64, share: f64) {
        self.base = base;
        self.share = share;
        self.tracks = Default::default();
        self.parts = [base / 2.0; 2];
    }

    /// Map a track's own fraction into the phase's slice of the bar. Share 0 means no phase was set,
    /// which is the whole bar — that is every step but Prepare, and also the headless queue a test
    /// builds.
    fn scaled(&self, fraction: f64) -> f64 {
        if self.share == 0.0 {
            fraction
        } else {
            self.base / 2.0 + fraction * self.share
        }
    }

    /// S2 (`qJob`): which job now owns a track, and which of the run's jobs it is — "describe 1/2",
    /// then "transcript 2/2". Pass `of = 0` for a run that is one job, or whose jobs are concurrent
    /// and named rather than numbered.
    pub fn job(&mut self, track: usize, name: &str, phase: usize, of: usize) {
        self.tracks[track] = Track {
            job: name.to_string(),
            phase,
            of,
            ..Default::default()
        };
    }

    /// S2 (`qPush`): queue `n` more tasks of one kind. Called again whenever more work is found: a
    /// run that could count all of its work in advance would not need a queue. `n <= 0` is a no-op
    /// rather than a way to take work back off.
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

    /// S2 (`qTake`): pick the next task up. Called once per task whatever becomes of it — work
    /// already on disk from an earlier run is a task this run is done with, and a queue that skipped
    /// those would stall at the position where the resume started.
    pub fn take(&mut self, track: usize) {
        let t = &mut self.tracks[track];
        t.taken += 1;
        t.what = String::new();
    }

    /// S2 (`prog`): how far this track has got and what its task is doing. The fraction is the
    /// track's absolute contribution; the text is two or three words, no filename, no count.
    pub fn prog(&mut self, track: usize, fraction: f64, text: &str) {
        self.parts[track] = self.scaled(fraction);
        self.tracks[track].what = text.to_string();
    }

    /// S2 (`qDone`): end a track's job. It keeps the half of the bar it earned and stops saying
    /// anything but its name, so the line belongs to whatever is still running.
    pub fn done(&mut self, track: usize, share: f64) {
        self.parts[track] = self.scaled(share);
        self.tracks[track].done_job = true;
    }

    /// The bar's fraction: the two halves summed, clamped to what a bar can show.
    pub fn fraction(&self) -> f64 {
        (self.parts[0] + self.parts[1]).clamp(0.0, 1.0)
    }

    /// The status line's words and the bar's tooltip. At most two jobs, joined; the tooltip keeps
    /// counting even for a job whose short line has gone quiet.
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
        (lines.join(" · "), tips.join("; "))
    }

    /// The short line alone — what the status line shows while a run works.
    pub fn text(&self) -> String {
        self.text_and_tip().0
    }

    /// Read one track, for a test or a painter that wants more than the joined line.
    pub fn track(&self, track: usize) -> &Track {
        &self.tracks[track]
    }
}

/// S3: what a `checkpoint()` call found between two subprocesses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Checkpoint {
    /// Neither paused nor stopping: go straight on.
    Ran,
    /// Paused — wait [`CHECKPOINT_POLL`] and ask again. Not a failure, and not a stop: the run is
    /// still the run that will carry on.
    Waited,
    /// ⏹ was pressed. This carries the stop error, and the caller unwinds rather than continuing.
    Stopped,
}

/// S3: the check every long flow makes between subprocesses.
///
/// Stop wins over pause: a user who pressed ⏹ while a pause was held asked for the run to end, and
/// making them un-pause first to stop it would be absurd. Pause polls rather than blocks, because
/// the GUI thread has to stay alive to see the ⏹ press at all.
pub fn checkpoint(paused: bool, stop_flag: bool) -> Checkpoint {
    if stop_flag {
        Checkpoint::Stopped
    } else if paused {
        Checkpoint::Waited
    } else {
        Checkpoint::Ran
    }
}

/// What starting a run did to the bookkeeping, so a test can read the order rather than infer it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Started {
    /// The run context's generation after the bump.
    pub epoch: u64,
    /// Whether the model log page was open when it got closed.
    pub closed_log_page: bool,
}

/// S1: `startRun` — running, flags cleared, fresh cancel context, queue reset (which also closes
/// the model log page), controls, log expanded. Every ▶ and ↻ goes through here, so the order lives
/// once: the flags are cleared *before* the queue is reset, because a queue reset repaints the bar
/// and a stale flag in that paint shows a stopped run as still paused.
///
/// The cancel context is a generation counter rather than a channel: a model or audio call asks
/// [`is_cancelled`] with the epoch it started on, so a `cancelled` left by the run before it cannot
/// abort this one.
pub fn start_run(bar: &mut RunBar, queue: &mut Queue, log: &mut RunLog, step: Step, sources: Snapshot) -> Started {
    let epoch = bump_cancel_context(bar);
    bar.running = Some(Run {
        step,
        paused: false,
        // startRun expands the log: a run that says nothing where you can see it did not run.
        log_expanded: true,
        sources,
    });
    queue.reset();
    // Closing the old exchange page happens here because this is the one call every run makes at its
    // start: the next LLM call then opens a new one, named for whichever step makes it.
    let closed_log_page = log.reset().is_some();
    Started {
        epoch,
        closed_log_page,
    }
}

/// S1: the fresh cancel context, on its own so a caller that starts a run without a queue or a log
/// (a test, a headless driver) still gets a clean one. Bumps the generation and clears both stop and
/// pause, which is what "fresh" means: neither flag may survive into the new run.
pub fn bump_cancel_context(bar: &mut RunBar) -> u64 {
    bar.run_epoch += 1;
    bar.stop_flag = false;
    // The pause flag lives on the run, not the bar (F0.2's ▶ toggles `Run::paused`), so a fresh
    // context clears it here for a run already standing as well as starting a clean one.
    if let Some(run) = bar.running.as_mut() {
        run.paused = false;
    }
    bar.cancelled = false;
    bar.run_epoch
}

/// S4: what ending a run handed over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndRun {
    /// The housekeeping call the audio server gets: `unload_all_models`, best-effort, 20 s deadline.
    /// Always present — every run ends with it, even one that used no audio, because leaving weights
    /// loaded is memory nothing is waiting on any more.
    pub audio_unload: Kind,
    /// Whether the lucky chain has another step to move to.
    pub chain_moves_on: bool,
}

/// S4: `endRun` — running off, controls refreshed, a lucky run moves to its next step, and the
/// audio models unloaded off-thread.
///
/// The chain moves *before* the unload is handed over: the next step usually wants the same models
/// back, so firing the unload first would make it load them again. Both are recorded rather than
/// performed here — the runner sends the request off-thread and the window comes back to life now,
/// not when the server answers.
pub fn end_run(bar: &mut RunBar, next: Option<Page>) -> EndRun {
    bar.running = None;
    bar.status.clear();
    EndRun {
        audio_unload: Kind::UnloadAll,
        chain_moves_on: next.is_some(),
    }
}

/// Whether the unload that [`end_run`] asks for really is the best-effort, bounded one §1 describes
/// (20 s, called quietly, never failing the run that made it). Exposed so a test pins the pair
/// rather than trusting the constants' names.
pub fn audio_unload_is_bounded() -> (Duration, bool) {
    (UNLOAD_TIMEOUT, UNLOAD_IS_BEST_EFFORT)
}

/// Whether a cancellation flagged at `epoch` applies to the run now holding epoch `now`. A call
/// started in an earlier run must not be aborted by a later run's flag, and vice versa: only equal
/// epochs talk.
pub fn is_cancelled(flagged: bool, epoch: u64, now: u64) -> bool {
    flagged && epoch == now
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn f0_5_s1_start_run_clears_flags_and_opens_the_log() {
        // F0.5 S1: running, flags cleared, fresh cancel context, queue reset, log expanded.
        let mut bar = RunBar::default();
        let mut queue = Queue::new();
        let mut log = RunLog::new();
        queue.push(TRACK_STT, 3, "stale");
        let started = start_run(&mut bar, &mut queue, &mut log, Step::Prepare, Snapshot::default());
        assert_eq!(started.epoch, 1);
        assert!(bar.running.is_some(), "startRun flips running on");
        assert!(!bar.stop_flag && !bar.cancelled, "F0.5 S1 clears both stop and cancel");
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
        // The position is only spelled out when the head has no words of its own: `prog`'s text already
        // counts, so appending "4/12" on top of "chunk 4/12" would say it twice.
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
        let end = end_run(&mut bar, Some(Page::Cut));
        assert!(end.chain_moves_on);
        assert!(bar.running.is_none());
        let (dur, best) = audio_unload_is_bounded();
        assert_eq!(dur.as_secs(), 20);
        assert!(best);
    }
}
