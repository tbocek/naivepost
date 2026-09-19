//! What pressing ▶ means (spec/03-shell.md F0.2), decided here rather than in a click handler.
//!
//! The run bar has one button and four meanings, and which one applies is a rule with an order:
//! a run under way beats the page's preview, which beats starting the page's step. Keeping that
//! precedence in plain data is what lets it be tested without a window (spec/00-principles.md §5) —
//! the button only draws [`controls`] and forwards a press to [`RunBar::press`].

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

/// The page's own preview: the recording on Cut, the voice sample on Narrate.
///
/// Only those two pages have one (§2). `started` is not `playing`: on both pages the player is
/// loaded the moment the timeline or a line is clicked, which cues a frame to look at and is not
/// playback — counting that as playback once made ▶ play a video on a Cut page whose cut was still
/// empty. So a transport becomes the bar's business only once its preview has actually been started,
/// and stays so until ⏹ ends it (prototype `pageTransport`, gui/runbar.go:28-52).
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

/// How many narrator slots a project has (prototype `narratorSlots`, gui/sources.go:28).
pub const NARRATOR_SLOTS: usize = 4;

/// The sources as they were when ▶ was pressed.
///
/// A run works on this rather than re-reading the project because the list is editable while a run
/// works: transcripts take minutes, and a run that asked the page again half-way through would
/// answer "what is footage" and "who holds slot 3" differently from the page that started it — the
/// usual way a render ends up missing an angle (prototype `snapSources`/`snapItems`,
/// gui/main.go:941-975).
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
        if (1..=NARRATOR_SLOTS as u32).contains(&slot) {
            snap.narrators[slot as usize - 1] = Some(source.path.clone());
        }
    }
    snap
}

/// A run under way: which step, whether it is paused, and what it is working on.
///
/// F0.5's queue (`qJob`/`qPush`/`qTake`/`prog`/`qDone`) and its 200 ms checkpoint are deliberately
/// not here — that flow has its own round, and a work queue with no work in it would be structure
/// for its own sake. What F0.2 needs is the run's identity and the pause flag ▶ toggles.
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
    Refused { reason: String },
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
}

impl RunBar {
    /// S1 → S2 → S3+S4, in that precedence and no other.
    pub fn press(&mut self, page: Page, transport: Transport, project: &Project) -> Pressed {
        // S1: a run under way, so ▶ is the pause button and says so. Nothing else happens —
        // pausing is not a new run and must not take a fresh snapshot of anything.
        if let Some(run) = self.running.as_mut() {
            run.paused = !run.paused;
            self.status = if run.paused { PAUSING } else { RESUMED }.to_string();
            return Pressed::ToggledPause { paused: run.paused };
        }

        // S2: playback beats the page's action. Once something is playing, or has been started and
        // parked, the button belongs to it until ⏹ ends it.
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

    /// The run is over. `Done` logs the step's own line and nothing else; a one-step run never gets
    /// an "all done" sentence, which is F0.4's lucky run talking about all four steps. A refusal
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

/// Whether any of a run's log lines claims everything finished. Kept as a function so the rule
/// "no `all done` line for one step" (S4) is asserted rather than eyeballed.
pub fn announced_all_done(lines: &[String]) -> bool {
    lines
        .iter()
        .any(|line| line.to_lowercase().contains("all done"))
}

/// The run bar's two buttons as they should be drawn right now.
///
/// ▶ is one button that becomes ⏸ rather than a second button beside it: the prototype had a
/// dedicated ⏸ and it was dead on every page whose ▶ is not a run, which is why ⏸ and ⏹ sat there
/// doing nothing while a voice sample played (gui/runbar.go:141-153). "Busy" is therefore either a
/// run that is not paused or a preview that is actually playing — a paused run has nothing to pause.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Controls {
    pub icon: &'static str,
    pub tooltip: &'static str,
    pub stop_sensitive: bool,
}

pub fn controls(running: &Option<Run>, transport: Option<Transport>) -> Controls {
    let busy = running.as_ref().is_some_and(|run| !run.paused)
        || transport.is_some_and(|transport| transport.playing);
    // ⏹ has something to end if a run is under way, or a preview is playing, or one is parked part
    // way through — a started-but-paused preview still needs ending.
    let stop_sensitive = running.is_some()
        || busy
        || transport.is_some_and(|transport| transport.cued());
    Controls {
        icon: if busy { PAUSE_ICON } else { PLAY_ICON },
        tooltip: if busy { PAUSE_TOOLTIP } else { PLAY_TOOLTIP },
        stop_sensitive,
    }
}
