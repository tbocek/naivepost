//! The shell's tab switching (spec/03-shell.md F0.1).

use std::fs;
use std::path::Path;
use std::time::Duration;

use crate::cut::{self, Seg};
use crate::layout::Tree;
use crate::narration::{self, Entry};
use crate::prepare;
use crate::project::Project;
use crate::publish;
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

pub const WINDOW_TITLE: &str = "Naivepost";

/// §1's window MINIMUM, width: "Its minimum size is 1240×900: it opens at that size and cannot be made
/// smaller", echoed by §10's window row. The width at which every control a page names fits on screen.
pub const WINDOW_MIN_W: i32 = 1240;

/// §1's window MINIMUM, height — the other half of [`WINDOW_MIN_W`]. §10 gives this pair no `P.*` id, so
/// it lives here as a bare constant rather than in `params.rs`.
pub const WINDOW_MIN_H: i32 = 900;

pub const PAGE_BAND_PX: i32 = 560;

pub const CUT_LOCK: &str =
    "Add footage on the Prepare step first \u{2014} the cut is laid out from the recordings";

/// The bottom bar's Inputs row for a Cut page with nothing to cut — `spec/inventory/cut.md` §A spells it
/// and §1 makes that inventory normative ("Tooltips and greyed rules: inventory/cut.md §A, normative").
pub const NOTHING_TO_CUT: &str = "nothing to cut \u{2014} no source on Inputs is marked as footage";

/// Why a page cannot be entered, `None` meaning it can (S2's question).
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
    /// The page's prerequisites vanished while it was open: switch silently to Prepare (§1).
    Vanished,
}

/// What S5 did on arrival, so a caller can redraw exactly what changed and a test can assert which
/// refresh ran.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Refresh {
    /// Prepare shows the sources; there is nothing upstream of it to re-read.
    None,
    CutRebuild {
        stale: bool,
    },
    NarrateRefit {
        moved: usize,
        orphan: usize,
    },
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
            page: Page::START,
            status: String::new(),
            help_page: Page::START,
            narration_pending: Pending::default(),
            cut_build: CutBuild::default(),
        }
    }
}

impl Shell {
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
        let silently = why == Move::Vanished && reason.is_some();

        // S3: whatever was half-typed is on disk before the page is left.
        let flushed = self.narration_pending.flush();
        if silently {
            self.status = String::new();
        }

        self.page = to;
        self.help_page = to;

        let refresh = self.refresh(to, project, tree, segs, entries);
        Outcome::Shown {
            page: to,
            flushed,
            refresh,
        }
    }

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
}

impl Shell {
    pub fn inputs(
        &self,
        tree: Option<&Tree>,
        project: &Project,
        cut: &cut::Cut,
        narration: &narration::Narration,
    ) -> String {
        match self.page {
            // Prepare's row is spec/04-prepare.md §1 verbatim; with no folder there is
            // nothing counted yet, and "0 frames → 0 vision" would be a claim about files.
            Page::Prepare => match tree {
                Some(tree) => prepare::inputs_readout(&prepare::count(tree, project)),
                None => prepare::NO_INPUTS.to_string(),
            },
            // With no footage there is nothing the cut could be laid out from, so the row says that rather
            // than counting an empty cut — "0 clip(s)" would read as a session someone had trimmed to nothing.
            Page::Cut if project.sources.iter().any(|source| source.footage) => {
                format!("{} clip(s)", cut.segs.len())
            }
            Page::Cut => NOTHING_TO_CUT.to_string(),
            Page::Narrate => {
                let tally = crate::ui::narrate_inputs_tally::from_session(
                    tree,
                    &cut.segs,
                    &narration.entries,
                );
                crate::narrate_screen::inputs_readout(
                    cut.segs.len(),
                    crate::cut_screen::cut_seconds(cut),
                    tally.unwritten,
                    tally.off_cut,
                    tally.has_timeline,
                    tally.has_cut,
                )
            }
            // §08-produce §1's row, "N clip(s) · mm:ss[ · no narration | · N to speak][ · no upload text]",
            // or its no-cut sentence. The lines owed and the upload text are counted from the session's files
            // by the same function the page uses, so the bar and the page cannot disagree.
            Page::Produce => {
                let counts = match tree {
                    Some(tree) => crate::ui::produce_page::session_counts(tree),
                    None => crate::ui::produce_page::SessionCounts {
                        owed: 0,
                        narrated: false,
                        publish_written: project.publish.is_some(),
                    },
                };
                crate::produce_screen::inputs_line(
                    cut.segs.len(),
                    crate::cut_screen::cut_seconds(cut),
                    !counts.narrated,
                    counts.owed,
                    counts.publish_written,
                )
            }
        }
    }

    pub fn inputs_tip(&self, tree: Option<&Tree>, project: &Project) -> Option<String> {
        match self.page {
            Page::Prepare => Some(match tree {
                Some(tree) => prepare::inputs_tip(tree, project),
                None => prepare::NO_INPUTS_TIP.to_string(),
            }),
            Page::Narrate => {
                let state = crate::ui::narrate_page::read_state();
                let tip = crate::ui::narrate_inputs::tooltip(tree, &state);
                (!tip.is_empty()).then_some(tip)
            }
            // §08-produce §1: what will be spoken first, which recordings are mixed, the voice, and whether
            // `publish.json` exists. The first three are what the page's flows published.
            Page::Produce => {
                let state = crate::ui::produce_page::read_state();
                let written = tree.map_or(state.publish_written, crate::publish::is_written);
                Some(crate::produce_screen::inputs_tooltip(
                    &state.cut_line,
                    (!state.first_spoken.is_empty()).then_some(state.first_spoken.as_str()),
                    &state.mixed_recordings,
                    (!state.voice.is_empty()).then_some(state.voice.as_str()),
                    written,
                ))
            }
            _ => None,
        }
    }

    /// The folder whose files `outputs` just counted, so the folder button beside the count points at
    /// exactly what that number means. One match for both, which is why the two cannot disagree.
    pub fn output_dir(&self, tree: &Tree) -> std::path::PathBuf {
        match self.page {
            Page::Prepare => tree.dir().join("prepare"),
            Page::Cut => tree.dir().join("cut"),
            Page::Narrate => tree.dir().join("narrate"),
            Page::Produce => tree.dir().join("produce"),
        }
    }

    /// The Outputs readout: what the page wrote, and the folder button's tooltip is the page's own
    /// (§1). Here only the row's sentence, which is what the label draws.
    pub fn outputs(&self, tree: Option<&Tree>) -> String {
        let Some(tree) = tree else {
            return String::new();
        };
        // Prepare carries both §1's shape and §6's in ONE row: "Prepare:" (the label §1 spells out)
        // then "N files, size" (§6's), with the age on hover. A bare count would answer §1 and ignore
        // §6; dropping the label would do the reverse.
        if self.page == Page::Prepare {
            let (text, _) = self.prepare_readout(tree);
            // Zero keeps §1's bare "nothing yet": "Prepare: nothing yet" reads as a run that produced
            // nothing on purpose, which is the second fact §6 refuses to confuse with the first.
            return if text == prepare::NO_OUTPUTS {
                text
            } else {
                // §1 spells the row `Prepare: 5 files, 1.4 MB` — the label and its count separated by ONE
                // space, the same shape as the "Outputs:" label in front of it.
                format!("{} {}", prepare::OUTPUTS_LABEL, text)
            };
        }
        if self.page == Page::Narrate {
            return crate::narrate_screen::outputs_readout().to_string();
        }
        // §08-produce §1: "count only (\"61 files, 220 MB\")".
        if self.page == Page::Produce {
            let (files, bytes) = files_and_bytes(&self.output_dir(tree));
            return crate::produce_screen::outputs_summary(files, bytes);
        }
        format!("{} file(s)", count_files(&self.output_dir(tree)))
    }

    /// One walk of `prepare/` decides both halves of the row: the count+size sentence and its age.
    fn prepare_readout(&self, tree: &Tree) -> (String, String) {
        let (bytes, newest) = prepare::outputs_size(tree);
        prepare::outputs_readout(prepare::outputs_count(tree), bytes, newest)
    }

    pub fn outputs_tip(&self, tree: Option<&Tree>) -> Option<String> {
        if self.page != Page::Prepare {
            return None;
        }
        let tree = tree?;
        let (_, tip) = self.prepare_readout(tree);
        (!tip.is_empty()).then_some(tip)
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

/// Files under `dir`, counting recursively and missing nothing quietly: a folder that is not there
/// has no output yet, which is zero rather than an error.
fn files_and_bytes(dir: &Path) -> (usize, u64) {
    let Ok(entries) = fs::read_dir(dir) else {
        return (0, 0);
    };
    entries.flatten().fold((0, 0), |(files, bytes), entry| {
        let path = entry.path();
        if path.is_dir() {
            let (more, size) = files_and_bytes(&path);
            (files + more, bytes + size)
        } else {
            (files + 1, bytes + entry.metadata().map_or(0, |m| m.len()))
        }
    })
}

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

const CLIP_TOLERANCE: f64 = 0.05;

fn clip_for(segs: &[Seg], entry: &Entry) -> Option<usize> {
    if entry.e <= entry.s {
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

pub fn refit_sentence(moved: usize, orphan: usize) -> String {
    if orphan > 0 {
        return format!(
            "the cut moved \u{2014} {moved} line(s) followed their clips, {orphan} sit on video the cut no longer has"
        );
    }
    format!("the cut moved \u{2014} {moved} line(s) followed their clips")
}
