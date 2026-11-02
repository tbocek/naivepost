//! What pressing ▶ means (spec/03-shell.md F0.2), decided here rather than in a click handler.

use crate::project::Project;
use crate::shell::Page;

/// S1's sentence when ▶ pauses a run: the stage in hand finishes first, because stopping a
/// subprocess mid-way is how a half-written file gets left behind. The ellipsis is the spec's.
pub const PAUSING: &str = "pausing after the current stage\u{2026}";
/// S1's sentence when ▶ resumes it.
pub const RESUMED: &str = "resumed";

/// ▶'s tooltip when nothing is busy (§2, em dash included).
pub const PLAY_TOOLTIP: &str = "Run this step \u{2014} or resume what is paused";
/// ▶'s tooltip while something is busy — the same button, so no second one goes dead.
pub const PAUSE_TOOLTIP: &str = "Pause";

pub const PLAY_ICON: &str = "media-playback-start-symbolic";
pub const PAUSE_ICON: &str = "media-playback-pause-symbolic";

/// F0.3 S1's sentence once the page's preview has been stopped.
pub const PLAYBACK_STOPPED: &str = "playback stopped";
/// F0.3 S3's sentence while a run is being brought down. The ellipsis is the spec's, and it is
/// honest: the stop only lands between subprocesses (S4), so the run is still going when this shows.
pub const STOPPING: &str = "stopping\u{2026}";
pub const STOP_ICON: &str = "media-playback-stop-symbolic";
/// ⏹'s tooltip, §2's own words: what it ends, and why ⏸ is a different control.
pub const STOP_TOOLTIP: &str =
    "Stop the run or the playback \u{2014} \u{23f8} is what parks one to carry on later";

/// The page's own preview: the recording on Cut, the voice sample on Narrate.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Transport {
    pub playing: bool,
    pub started: bool,
}

impl Transport {
    /// Started but paused, so ▶ means resume rather than start.
    pub fn cued(self) -> bool {
        self.started && !self.playing
    }
}

/// The visible page's transport, if that page has one — Prepare and Produce never do, which is why
/// they ignore a playing preview and run their step.
pub fn transport_for(page: Page, transport: Transport) -> Option<Transport> {
    match page {
        Page::Cut | Page::Narrate => Some(transport),
        Page::Prepare | Page::Produce => None,
    }
}

/// The step ▶ runs on each page (S4). Cut's is suggesting a cut rather than the page itself: adding
/// a selection is ＋ Add, and ⏹/▶ belong to the long job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Prepare,
    Suggest,
    Narrate,
    Produce,
}

impl Step {
    /// The flow this step is, named as the spec names it — for the log line and for a test to pin
    /// the page-to-step mapping against.
    pub fn flow(self) -> &'static str {
        match self {
            Self::Prepare => "F1.1 \u{b7} Prepare",
            Self::Suggest => "F2.14 \u{b7} Suggest a cut",
            Self::Narrate => "F4.1 \u{b7} Write and speak",
            Self::Produce => "F5.1 \u{b7} Produce",
        }
    }

    /// What the step is called on screen, for the status line and the log.
    pub fn label(self) -> &'static str {
        match self {
            Self::Prepare => "Prepare",
            Self::Suggest => "Cut",
            Self::Narrate => "Narrate",
            Self::Produce => "Produce",
        }
    }
}

/// S4: the step of a page.
pub fn step(page: Page) -> Step {
    match page {
        Page::Prepare => Step::Prepare,
        Page::Cut => Step::Suggest,
        Page::Narrate => Step::Narrate,
        Page::Produce => Step::Produce,
    }
}

pub const NARRATOR_SLOTS: usize = 4;

pub fn is_slot(slot: u32) -> bool {
    (1..=NARRATOR_SLOTS as u32).contains(&slot)
}

/// A slot as its 0-based index into the snapshot's `narrators` list, or `None` for a value that is
/// not a slot — so no caller indexes that list with a wild number from a project file.
pub fn slot_index(slot: u32) -> Option<usize> {
    if is_slot(slot) {
        Some(slot as usize - 1)
    } else {
        None
    }
}

/// The sources as they were when ▶ was pressed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub footage: Vec<String>,
    pub voice: Vec<String>,
    /// Slot 1..=[`NARRATOR_SLOTS`] as index 0..; `None` for a slot nobody holds.
    pub narrators: Vec<Option<String>>,
}

/// S3: take the snapshot from the project's source list, using the same two questions the page
/// answers — is it footage, and does it hold a narrator slot.
pub fn snapshot_sources(project: &Project) -> Snapshot {
    let mut snap = Snapshot {
        narrators: vec![None; NARRATOR_SLOTS],
        ..Default::default()
    };
    for source in &project.sources {
        if source.footage {
            snap.footage.push(source.path.clone());
        } else {
            snap.voice.push(source.path.clone());
        }
        let slot = source.narrator;
        // Voices are shared: the slot names the FIRST row in that voice, as §4 names voice 1's recording.
        if let Some(index) = slot_index(slot) {
            if snap.narrators[index].is_none() {
                snap.narrators[index] = Some(source.path.clone());
            }
        }
    }
    snap
}

/// The subprocesses a run has out there right now.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Subprocesses {
    pids: Vec<u32>,
}

impl Subprocesses {
    /// Note a child the run has started, so a stop can reach it.
    pub fn register(&mut self, pid: u32) {
        self.pids.push(pid);
    }

    /// How many children are outstanding.
    pub fn len(&self) -> usize {
        self.pids.len()
    }

    /// Whether anything is registered. Required beside `len`.
    pub fn is_empty(&self) -> bool {
        self.pids.is_empty()
    }

    pub fn kill_all(&mut self) -> Vec<u32> {
        std::mem::take(&mut self.pids)
    }
}

/// What one press of ⏹ did. Like [`Pressed`], the UI draws from this and decides nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stopped {
    /// S1 with no run behind it: only the page's preview was going.
    TransportOnly,
    /// S2: neither a preview nor a run, so the press is ignored.
    NothingToDo,
    /// S3: the run was brought down, and these are the children handed off to be killed.
    RunStopped { killed: Vec<u32> },
}

impl Stopped {
    /// The sentence the status line shows after the press. An empty string means "say nothing" —
    /// S2 must not blank a status line that already carries something.
    pub fn status(&self) -> &'static str {
        match self {
            Self::TransportOnly => PLAYBACK_STOPPED,
            Self::NothingToDo => "",
            Self::RunStopped { .. } => STOPPING,
        }
    }
}

/// A run under way: which step, whether it is paused, and what it is working on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    pub step: Step,
    pub paused: bool,
    /// startRun expands the log (F0.5 S1): a run that says nothing where you can see it did not run.
    pub log_expanded: bool,
    pub sources: Snapshot,
}

/// How a step ended, which decides what the bar and the log say afterwards.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Finish {
    Done,
    /// The step's own refusal (F1.1/F2.14/F4.1/F5.1 refuse at their own start) — not the bar's.
    Refused {
        reason: String,
    },
}

/// What one press of ▶ did. The UI draws from this and forwards it; it decides nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pressed {
    /// S1: a run was under way, so ▶ was the pause button.
    ToggledPause { paused: bool },
    /// S2: the page's preview owns the button while it is going or started.
    ToggledTransport { resume: bool },
    /// S3 + S4: nothing was busy, so this page's step starts.
    Started { step: Step, sources: Snapshot },
}

/// The bar itself: at most one run, and the status line's current sentence.
#[derive(Debug, Default)]
pub struct RunBar {
    pub running: Option<Run>,
    pub status: String,
    /// F0.3 S3: set by ⏹ and read by every loop between subprocesses. Not cleared here — the flow
    /// that was stopped clears it when it has finished unwinding, so a later run cannot inherit it.
    pub stop_flag: bool,
    pub cancelled: bool,
    /// F0.3 S5: a stop landed inside Describe, so the next Prepare run describes from the start.
    pub describe_from_start: bool,
    pub run_epoch: u64,
}

impl RunBar {
    /// S1 → S2 → S3 (+S5), in that precedence and no other.
    pub fn press_stop(
        &mut self,
        page: Page,
        transport: Transport,
        in_describe: bool,
        procs: &mut Subprocesses,
    ) -> Stopped {
        let stopped_playback =
            transport_for(page, transport).is_some_and(|t| t.playing || t.cued());
        if stopped_playback {
            self.status = PLAYBACK_STOPPED.to_string();
        }

        let Some(run) = self.running.as_mut() else {
            return if stopped_playback {
                Stopped::TransportOnly
            } else {
                Stopped::NothingToDo
            };
        };

        run.paused = false;
        self.stop_flag = true;
        self.cancelled = true;
        // The same press has to reach the legs already in flight on another thread, which cannot
        // borrow this bar: §S3's "cancel the run context (aborts model and audio calls)" lands
        // through the shared switch (§02-services#1 puts every request on it).
        crate::cancel_leg::cancel_now();
        let killed = procs.kill_all();
        self.status = STOPPING.to_string();

        if in_describe {
            self.describe_from_start = true;
        }
        Stopped::RunStopped { killed }
    }

    pub fn describe_restarts(&mut self) -> bool {
        std::mem::replace(&mut self.describe_from_start, false)
    }

    /// S1 → S2 → S3+S4, in that precedence and no other.
    pub fn press(&mut self, page: Page, transport: Transport, project: &Project) -> Pressed {
        // S1: a run under way, so ▶ is the pause button and says so. Nothing else happens —
        // pausing is not a new run and must not take a fresh snapshot of anything.
        if let Some(run) = self.running.as_mut() {
            run.paused = !run.paused;
            self.status = if run.paused { PAUSING } else { RESUMED }.to_string();
            return Pressed::ToggledPause { paused: run.paused };
        }

        if let Some(t) = transport_for(page, transport) {
            if t.playing || t.cued() {
                return Pressed::ToggledTransport { resume: t.cued() };
            }
        }

        // S3 then S4: snapshot the sources and run this page's step alone. The status line goes
        // quiet because the run is about to overwrite it with its own progress (§2).
        let sources = snapshot_sources(project);
        let step = step(page);
        self.running = Some(Run {
            step,
            paused: false,
            log_expanded: true,
            sources: sources.clone(),
        });
        self.status = String::new();
        Pressed::Started { step, sources }
    }

    /// The run is over. `Done` logs the step's own line and nothing else. A refusal
    /// reaches the status line too — it is why ▶ did not do what it was pressed for.
    pub fn finish(&mut self, reason: Finish) -> Vec<String> {
        let step = self.running.take().map(|run| run.step);
        match reason {
            Finish::Done => {
                let name = step.map_or_else(String::new, |step| step.label().to_string());
                vec![format!("{name} done")]
            }
            Finish::Refused { reason } => {
                self.status = reason.clone();
                vec![reason]
            }
        }
    }
}

/// The run bar's two buttons as they should be drawn right now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Controls {
    pub icon: &'static str,
    pub tooltip: &'static str,
    pub stop_sensitive: bool,
}

pub fn controls(running: &Option<Run>, transport: Option<Transport>) -> Controls {
    let busy = running.as_ref().is_some_and(|run| !run.paused)
        || transport.is_some_and(|transport| transport.playing);
    let stop_sensitive =
        running.is_some() || busy || transport.is_some_and(|transport| transport.cued());
    Controls {
        icon: if busy { PAUSE_ICON } else { PLAY_ICON },
        tooltip: if busy { PAUSE_TOOLTIP } else { PLAY_TOOLTIP },
        stop_sensitive,
    }
}
