//! The shell's tab switching (spec/03-shell.md F0.1).
//!
//! Which page is showing, whether a tab may be entered, and what has to be redone on arrival — all
//! of it here rather than in a `notify` handler, so a bounce, a flush and a refit are testable
//! without a window (spec/00-principles.md §5). The UI renders [`Shell`] and forwards clicks.

use std::fs;
use std::path::Path;
use std::time::Duration;

use crate::cut::{self, Seg};
use crate::layout::Tree;
use crate::narration::{self, Entry};
use crate::project::Project;
use crate::publish;
use crate::tools::mm_ss;

/// The four tabs, in the order the pipeline runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Prepare,
    Cut,
    Narrate,
    Produce,
}

impl Page {
    /// The tab's word, and the visible child name of the page stack — the same strings as
    /// [`crate::PAGES`].
    pub fn label(self) -> &'static str {
        match self {
            Self::Prepare => "Prepare",
            Self::Cut => "Cut",
            Self::Narrate => "Narrate",
            Self::Produce => "Produce",
        }
    }

    /// Its place in [`crate::PAGES`], which is the order the tabs are in.
    pub fn index(self) -> usize {
        match self {
            Self::Prepare => 0,
            Self::Cut => 1,
            Self::Narrate => 2,
            Self::Produce => 3,
        }
    }

    pub fn all() -> [Page; 4] {
        [Self::Prepare, Self::Cut, Self::Narrate, Self::Produce]
    }

    /// The icon named by spec/03-shell.md §1 ("Tabs: Prepare (view-list), Cut (edit-cut), Narrate
    /// (microphone), Produce (multimedia)"), spelled as a GTK icon name for the tab row.
    pub fn icon(self) -> &'static str {
        match self {
            Self::Prepare => "view-list-symbolic",
            Self::Cut => "edit-cut-symbolic",
            Self::Narrate => "audio-input-microphone-symbolic",
            Self::Produce => "applications-multimedia-symbolic",
        }
    }

    /// The one-line tooltip of an unlocked tab (§1's tab row; the prototype's steps table). A locked
    /// tab shows [`lock`]'s reason instead — that swap is the whole of §1's "tooltip = the reason".
    pub fn tip(self) -> &'static str {
        match self {
            Self::Prepare => {
                "The sources, their transcripts, their frames, and what the models make of them"
            }
            Self::Cut => "Choose the clips the video is made of",
            Self::Narrate => "The narration, and the voice it is spoken in",
            Self::Produce => "Write the upload text, draw the thumbnail and render the video",
        }
    }

    /// The page a run starts at, and where a page whose prerequisites vanished is sent (§1: "A page
    /// whose prerequisites vanish while open switches silently to Prepare").
    pub const START: Page = Page::Prepare;
}

/// S2's refusal sentence for Cut — §1's exact wording, em dash included. The prototype carries it
/// as the tab's `wait` text (gui/main.go:77) so a click and a hover say the same thing.
pub const CUT_LOCK: &str = "Add footage on the Prepare step first \u{2014} the cut is laid out from the recordings";

/// Why a page cannot be entered, `None` meaning it can (S2's question).
///
/// Prepare never is: it is where the sources get added, so an empty project still has to open it.
/// Cut waits for a source marked footage, because the cut is laid out from the recordings. Narrate
/// and Produce are never locked (§1: "Narrate and Produce never locked; their ▶ refuses without a
/// cut") — [`cut::NO_CUT_YET`] is what ▶ says, and putting a page behind it would hide the voice
/// picker, which lives beside the lines it speaks but needs no cut to be useful. An unknown page
/// cannot be named here, which is the advantage of an enum over the prototype's index lookup.
pub fn lock(page: Page, project: &Project) -> Option<&'static str> {
    match page {
        Page::Prepare => None,
        Page::Cut if project.sources.iter().any(|source| source.footage) => None,
        Page::Cut => Some(CUT_LOCK),
        Page::Narrate | Page::Produce => None,
    }
}

/// How the page is being changed — S1's two doors and §1's third case.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Move {
    /// A tab was clicked. The only door that can bounce (S2).
    Click,
    /// A run moved to the page ("I'm feeling lucky" walks the pipeline). A run has already decided
    /// this page is its business, so a lock does not send it elsewhere — its own ▶ says why it
    /// cannot proceed, which is the honest place for that.
    Run,
    /// The page's prerequisites vanished while it was open: switch silently to Prepare (§1).
    Vanished,
}

/// What S5 did on arrival, so a caller can redraw exactly what changed and a test can assert which
/// refresh ran.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Refresh {
    /// Prepare shows the sources; there is nothing upstream of it to re-read.
    None,
    CutRebuild { stale: bool },
    NarrateRefit { moved: usize, orphan: usize },
    ProducePanels,
}

/// What [`Shell::switch`] decided.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// S2: the page did not move and the status line carries the reason.
    Bounced { reason: &'static str },
    Shown {
        page: Page,
        /// S3: whether a narration write was owed and went out first.
        flushed: bool,
        refresh: Refresh,
    },
}

/// The narration file's beat (§7-narrate.md:215 "autosaved 400 ms after typing; flushed on tab
/// leave, window close, and before Produce reads the file"; spec/10-parameters.md lists it among
/// the engineering constants).
///
/// Not to be confused with [`crate::checks::CONF_SAVE_WAIT`], which is the *settings* dialog's beat
/// of 600 ms on `llm.conf`. Two files, two beats; neither one's rule belongs to the other.
pub const NARRATION_AUTOSAVE: Duration = Duration::from_millis(400);

/// A narration write that typing owes. The timer is the caller's (a GTK timeout here, nothing in a
/// test), so this only answers when it may fire and whether there was anything to fire for.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Pending {
    due: Option<Duration>,
}

impl Pending {
    /// Typing: push the pending write back, so one write follows a pause rather than one per letter.
    pub fn touched(&mut self, now: Duration) {
        self.due = Some(now + NARRATION_AUTOSAVE);
    }

    pub fn due_at(&self) -> Option<Duration> {
        self.due
    }

    /// Whether the timer may write at `now`.
    pub fn writable(&self, now: Duration) -> bool {
        self.due.is_some_and(|due| now >= due)
    }

    /// Whether anything is owed at all — S3 asks this, not whether the beat has elapsed: leaving the
    /// tab writes what is half-typed even a beat early.
    pub fn owe(&self) -> bool {
        self.due.is_some()
    }

    /// Write it now. `true` when something was owed, which is what S3 reports as `flushed`.
    pub fn flush(&mut self) -> bool {
        let owed = self.owe();
        self.due = None;
        owed
    }
}

/// Prepare's output for one source: its path inside the project and a stamp of size+mtime.
pub type PrepareFile = (String, u64);

/// What Prepare had produced when the cut was last built — S5's "since its build".
///
/// The files are Prepare's per-source output under `prepare/inputs/<source>/`: the transcript text,
/// its word timings and the frames marker. Those three move exactly when Prepare has done more work
/// (a re-transcribe, a re-align, frames drawn), which is what a cut built from them would go stale
/// on; the source files themselves are not stamped because adding one changes the source list, and
/// §1 treats that as Prepare's own business. A file that is not there yet reads as absent rather
/// than as an error: "Prepare has not produced it" is a real state, and it must differ from "it
/// exists with this size".
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PrepareStamp {
    pub sources: Vec<PrepareFile>,
}

/// Stamp the footage sources' Prepare output. Sorted by name so two runs over an unchanged project
/// give the same stamp and a save stays byte-stable the way §2 asks of the other files.
pub fn prepare_stamp(tree: &Tree, project: &Project) -> PrepareStamp {
    let mut sources = Vec::new();
    for source in project.sources.iter().filter(|source| source.footage) {
        for file in [
            tree.transcript_txt(&source.path),
            tree.words_json(&source.path),
            tree.frames_marker(&source.path),
        ] {
            let name = file
                .strip_prefix(tree.dir())
                .unwrap_or(&file)
                .to_string_lossy()
                .replace('\\', "/");
            sources.push((name, stamp_of(&file)));
        }
    }
    sources.sort();
    PrepareStamp { sources }
}

/// Size and mtime of a file, packed into one number: `Err` on no file reads as zero, which is how
/// "not produced yet" differs from "produced as an empty file".
fn stamp_of(path: &Path) -> u64 {
    let Ok(meta) = fs::metadata(path) else {
        return 0;
    };
    let seconds = meta
        .modified()
        .ok()
        .and_then(|when| when.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |span| span.as_secs());
    // These files are nowhere near 2^32 bytes, so the shift leaves room for the seconds without
    // either field crowding the other out.
    (meta.len() << 32) ^ seconds
}

/// When the cut was last built, as far as Prepare's output is concerned.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct CutBuild {
    pub stamp: Option<PrepareStamp>,
}

impl CutBuild {
    /// S5's condition, read straight: rebuild when there was no build, or when Prepare's output is
    /// not what that build was made from.
    pub fn needs_rebuild(&self, now: &PrepareStamp) -> bool {
        self.stamp.as_ref() != Some(now)
    }

    /// Note what the build just made used.
    pub fn record(&mut self, now: PrepareStamp) {
        self.stamp = Some(now);
    }
}

/// The publish panel as Produce sees it on arrival (§5 of 08-produce.md: the file's existence is
/// what "the text is written" means, and the picture on the page is the file on disk).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PublishPanel {
    pub text_written: bool,
    pub base: Option<String>,
    pub references: usize,
}

impl PublishPanel {
    /// Read it again — which is S5's "refresh the readouts and the publish panel". A file that does
    /// not parse says so in the reason rather than keeping whatever was on screen.
    pub fn refresh(tree: &Tree) -> Result<Self, String> {
        if !publish::is_written(tree) {
            return Ok(Self::default());
        }
        let record = publish::load(tree)?;
        Ok(Self {
            text_written: true,
            base: publish::base(&record).map(str::to_string),
            references: publish::references(&record).len(),
        })
    }
}

/// S4 + S5's state the window shows. `status` is the status line's text; the UI never composes one
/// of its own, so every sentence on that row traces back to a rule here.
#[derive(Debug, Clone)]
pub struct Shell {
    pub page: Page,
    pub status: String,
    /// Which page ⓘ describes — S4 syncs it with the page shown, and it is kept apart from `page`
    /// only because a bounce must not move it either.
    pub help_page: Page,
    /// S3's pending narration write. Public so the Narrate page can mark it on typing.
    pub narration_pending: Pending,
    /// S5's Cut condition, remembered across switches.
    pub cut_build: CutBuild,
}

impl Default for Shell {
    fn default() -> Self {
        Self {
            // The window opens on Prepare (F0.6), so that is where switching starts from.
            page: Page::START,
            status: String::new(),
            help_page: Page::START,
            narration_pending: Pending::default(),
            cut_build: CutBuild::default(),
        }
    }
}

impl Shell {
    /// S1-S5 in one call. `entries` is the narration as the Narrate page holds it (half-typed rows
    /// pulled in first, which is the prototype's `pullRows`, gui/narrate.go:2473); it is refitted in
    /// place when the target is Narrate. `tree` is needed for the two refreshes that read disk and
    /// may be `None` for a project with no folder yet — a page then shows what it has rather than
    /// failing to open.
    pub fn switch(
        &mut self,
        to: Page,
        why: Move,
        project: &Project,
        tree: Option<&Tree>,
        segs: &[Seg],
        entries: &mut Vec<Entry>,
    ) -> Outcome {
        // S2. Only a click bounces (§1: "S2's bounce is only for a click"): a run moves where the
        // pipeline says, and a page whose prerequisites vanished goes back to Prepare in silence.
        let reason = lock(to, project);
        if why == Move::Click {
            if let Some(reason) = reason {
                self.status = reason.to_string();
                return Outcome::Bounced { reason };
            }
        }
        let to = match why {
            Move::Vanished if reason.is_some() => Page::START,
            _ => to,
        };
        // A silent return to Prepare says nothing: an old status line would be a lie about what just
        // happened, and the reason for a switch the user did not make is not their news (§1: "S2's
        // bounce is only for a click").
        let silently = why == Move::Vanished && reason.is_some();

        // S3: whatever was half-typed is on disk before the page is left.
        let flushed = self.narration_pending.flush();
        if silently {
            self.status = String::new();
        }

        // S4: show the page, its readouts and ⓘ together.
        self.page = to;
        self.help_page = to;

        // S5: the refresh that page needs.
        let refresh = self.refresh(to, project, tree, segs, entries);
        Outcome::Shown { page: to, flushed, refresh }
    }

    /// S5 for one page. Only three pages have anything to redo; Prepare is where the work starts, so
    /// arriving there changes nothing upstream of it. `segs` is the cut's clips as the caller holds
    /// them (the live editor's if it has any), which is what Narrate's lines are refitted against.
    fn refresh(
        &mut self,
        to: Page,
        project: &Project,
        tree: Option<&Tree>,
        segs: &[Seg],
        entries: &mut Vec<Entry>,
    ) -> Refresh {
        match to {
            Page::Prepare => Refresh::None,
            Page::Cut => {
                let Some(tree) = tree else {
                    return Refresh::CutRebuild { stale: false };
                };
                let now = prepare_stamp(tree, project);
                let stale = self.cut_build.needs_rebuild(&now);
                if stale {
                    self.cut_build.record(now);
                }
                Refresh::CutRebuild { stale }
            }
            Page::Narrate => {
                // The cut is the page most likely to have moved under these lines — it is where the
                // user came from.
                let (moved, orphan) = refit(segs, entries);
                if moved > 0 || orphan > 0 {
                    self.status = refit_sentence(moved, orphan);
                }
                Refresh::NarrateRefit { moved, orphan }
            }
            Page::Produce => {
                // Readouts plus the publish panel; a broken publish.json is worth a status line, and
                // it must not stop the page from opening.
                if let Some(tree) = tree {
                    if let Err(reason) = PublishPanel::refresh(tree) {
                        self.status = reason;
                    }
                }
                Refresh::ProducePanels
            }
        }
    }

    /// The Inputs readout of the page shown (spec/03-shell.md §1: "the visible tab's `Inputs:` and
    /// `Outputs:` readouts"). Produce's wording is spec/08-produce.md §1 verbatim; the other three
    /// pages' rows are specified in 04/05/07, whose rounds have not come, so these are the rewrite's
    /// own short sentences — one line each, naming what ▶ on that page would read.
    pub fn inputs(&self, project: &Project, cut: &cut::Cut, narration: &narration::Narration) -> String {
        match self.page {
            Page::Prepare => format!(
                "{} source(s), {} of them footage",
                project.sources.len(),
                project.sources.iter().filter(|source| source.footage).count()
            ),
            Page::Cut => format!("{} clip(s)", cut.segs.len()),
            Page::Narrate if project.no_narration => "no narration — captions only".to_string(),
            Page::Narrate => format!(
                "{} line(s) over {} clip(s)",
                narration.entries.len(),
                cut.segs.len()
            ),
            // "N clip(s) · mm:ss[ · no narration][ · no upload text]" (§8-produce.md §1).
            Page::Produce => {
                let spoken = !project.no_narration && !narration.entries.is_empty();
                let mut line = format!(
                    "{} clip(s) · {}",
                    cut.segs.len(),
                    mm_ss(cut_seconds(cut))
                );
                if !spoken {
                    line.push_str(" · no narration");
                }
                if project.publish.is_none() {
                    line.push_str(" · no upload text");
                }
                line
            }
        }
    }

    /// The Outputs readout: a count, and the folder button's tooltip is the page's own (§1). Here
    /// only the count sentence, which is what the row draws.
    pub fn outputs(&self, tree: Option<&Tree>) -> String {
        let Some(tree) = tree else {
            return String::new();
        };
        // §1 asks for a count of what the page wrote; counting the page's own output folder is the
        // one number that cannot lie about a run that has not happened yet.
        let dir = match self.page {
            Page::Prepare => tree.dir().join("prepare"),
            Page::Cut => tree.dir().join("cut"),
            Page::Narrate => tree.dir().join("narrate"),
            Page::Produce => tree.dir().join("produce"),
        };
        format!("{} file(s)", count_files(&dir))
    }

    /// What ⓘ says: the current tab's label and help text (spec/03-shell.md §1). The long paragraph
    /// per page is F0.1's neighbours' business (§1's tooltip carries it); this names what it leads to.
    pub fn info_tip(&self) -> String {
        info_tip(self.help_page)
    }
}

/// ⓘ's tooltip for a page: its word, then the one-line description that says what is on it. Built
/// from [`Page::tip`] so the tab and the ⓘ can never disagree about what a page is.
pub fn info_tip(page: Page) -> String {
    format!("{} — {}", page.label(), page.tip())
}

/// How long the cut is, in output seconds: the end of the last segment. Cards (`e <= s`) add no
/// time, which is what makes them not show up as duration.
pub fn cut_seconds(cut: &cut::Cut) -> f64 {
    cut.segs
        .iter()
        .filter(|seg| seg.e > seg.s)
        .map(|seg| seg.e - seg.s)
        .sum()
}

/// Files under `dir`, counting recursively and missing nothing quietly: a folder that is not there
/// has no output yet, which is zero rather than an error.
fn count_files(dir: &Path) -> usize {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    entries.flatten().fold(0, |count, entry| {
        let path = entry.path();
        if path.is_dir() {
            count + count_files(&path)
        } else {
            count + 1
        }
    })
}

/// S5's refit: move the lines onto the cut as it is now without asking the model. The words are
/// kept, the times follow. Returns `(moved, orphan)`.
///
/// A line keeps its place against the *video*, not its offset into the clip — so `at` is re-based
/// when the clip's start moved, which is what keeps the words over the same moment of the
/// recording (prototype gui/narrate.go:2421-2438). A line whose clip is gone is left exactly where
/// it was and counted: deleting narration because a clip went away is worse than a line with no
/// clip under it, and the count is what tells the user why.
pub fn refit(segs: &[Seg], entries: &mut [Entry]) -> (usize, usize) {
    let (mut moved, mut orphan) = (0, 0);
    for entry in entries {
        let Some(index) = clip_for(segs, entry) else {
            orphan += 1;
            continue;
        };
        let seg = &segs[index];
        // Close enough is the same clip: a cut re-saved at the frame interval moves bounds by
        // hundredths, and calling that "the cut moved" would say so on every visit.
        if (seg.s - entry.s).abs() <= CLIP_TOLERANCE && (seg.e - entry.e).abs() <= CLIP_TOLERANCE {
            continue;
        }
        let at = entry.s + entry.at - seg.s;
        entry.s = seg.s;
        entry.e = seg.e;
        // Never past the clip's last second, and never before its start: a line dragged off the end
        // would otherwise be scheduled where nothing plays.
        entry.at = at.clamp(0.0, (entry.e - entry.s - 1.0).max(0.0));
        moved += 1;
    }
    (moved, orphan)
}

/// How close a clip's bounds have to be for the line on it to count as unmoved (prototype: 0.05 s).
const CLIP_TOLERANCE: f64 = 0.05;

/// The clip a line belongs to now: the one it shares the most video with — not "the one holding its
/// start", because a clip trimmed at the front leaves that start outside the very clip the rest of
/// the line plainly sits in (prototype `clipFor`, gui/narrate.go:2443-2462).
fn clip_for(segs: &[Seg], entry: &Entry) -> Option<usize> {
    if entry.e <= entry.s {
        // A line written on a card has no span to share, so its clip is the zero-span segment still
        // sitting at the same moment, if there is one.
        return segs
            .iter()
            .position(|seg| seg.e <= seg.s && (seg.s - entry.s).abs() <= CLIP_TOLERANCE);
    }
    let mut best: Option<(usize, f64)> = None;
    for (index, seg) in segs.iter().enumerate() {
        let overlap = entry.e.min(seg.e) - entry.s.max(seg.s);
        if overlap > best.map_or(0.0, |(_, most)| most) {
            best = Some((index, overlap));
        }
    }
    best.map(|(index, _)| index)
}

/// The status line S5 writes after a refit — the prototype's sentence (gui/narrate.go:2482-2484),
/// because it is the one place the app explains that its work was automatic.
pub fn refit_sentence(moved: usize, orphan: usize) -> String {
    if orphan > 0 {
        return format!(
            "the cut moved \u{2014} {moved} line(s) followed their clips, {orphan} sit on video the cut no longer has"
        );
    }
    format!("the cut moved \u{2014} {moved} line(s) followed their clips")
}
