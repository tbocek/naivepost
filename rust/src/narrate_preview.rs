//! F4.5 Preview the cut with narration — `spec/07-narrate.md` F4.5, steps S1–S6.
//!
//! ▶ on the Narrate page runs the *cut* with the narration riding along: the picture leads and a line's wav
//! is laid over it at the second the picture reaches that line. Two things make this more than "play the
//! video": the stretch between two clips is material the edit removed, so the tick skips it forward instead
//! of playing it, and a line whose take does not exist yet stops the picture until it does — a few seconds of
//! waiting is the honest version of previewing a cut nobody would watch otherwise.
//!
//! Everything here is a plain function over state the page already holds: which clips there are, what the
//! lines say and where, which takes exist, and where the picture is. The GTK layer calls these and decides
//! nothing (spec/00-principles.md §5), so every branch below is testable without a display or a player.
//!
//! What this module does **not** own:
//! - the players themselves — [`crate::preview`] is F2.1's ▶ and its `Player`/`Press`; the two pipelines here
//!   (picture, narration) are the page's to build and cue;
//! - speaking a line ([`crate::narrate_tts`], F4.4). This module notices that a take is missing and says what
//!   to tell the person; it never calls a server;
//! - fitting a line to its clip — F4.3's packing, growing and tempo live in the render, and the boundary this
//!   flow holds at [`MAX_EXTEND_SECONDS`] is the same ceiling that lets the render grow a clip at all;
//! - the row buttons' six answers ([`crate::narrate_screen::audition`]) and everything else about the row
//!   ([`crate::narrate_screen`]);
//! - the tick timer, the seeks and the camera reloads they imply — the page's plumbing, called from here by
//!   the answers below.

use crate::cut::{Cut, Fx, Seg};
use crate::cut_hear;
use crate::narrate_screen::AUDITION_LEAD_SECONDS;
use crate::narration::Entry;

/// P.eng.narrationMaxExtendSeconds: how long past its own end a clip boundary is held while a line still
/// speaks. The render may grow a clip by exactly this much to fit its line, so the preview holds a boundary
/// the render would hold and no longer — otherwise the preview runs on two seconds early and stops being a
/// preview of the render.
pub const MAX_EXTEND_SECONDS: f64 = 4.0;

// --- What the person is told (S1, S2) ------------------------------------------------------------------------

/// S1: there is nothing to play. Em dash because that is how the sentence reads in the prototype and on the
/// screen — a hyphen here would be a different string than the one a person reports seeing.
pub const NOTHING_TO_PREVIEW: &str = "nothing to preview yet \u{2014} cut some clips first";

/// S1: there are clips, but the second being cued is on no recording, so nothing can be loaded and the
/// frame would sit black while the transport claimed to be playing.
pub const NO_RECORDING_AT_START: &str = "no recording covers the start of the cut";

/// S2: the picture has paused for a line that has never been spoken. Numbered from one, because the rows are
/// counted from one on the screen.
pub fn synthesizing(line: usize) -> String {
    format!("synthesizing line {}", line + 1)
}

/// S2: the take arrived and the picture is about to move again.
pub fn line_ready(line: usize) -> String {
    format!("line {} ready", line + 1)
}

/// S2: a line failed while the picture was waiting for it, and the run carries on without its voice. Two
/// hyphens, as the prototype writes them — deliberately unlike [`sticky_failed`]'s em dash, which is a
/// different sentence in the same log and keeps its own punctuation.
pub fn failed_playing_on(line: usize) -> String {
    format!("line {} failed -- see log; playing on without it", line + 1)
}

/// S2: this take's synthesis already failed, so the clip runs mute again. A sticky silent failure reads as
/// "the TTS stopped working" rather than as a line that failed once, so it is said every time it comes
/// around — and it names the row's own ▶ as the way out. Em dash and ▶ (U+25B6) as written in the prototype.
pub fn sticky_failed(line: usize) -> String {
    format!(
        "line {} failed to synthesize \u{2014} see log; its \u{25b6} retries",
        line + 1
    )
}

/// S6: the triangle ⏵/▶ hands back to the run's button once the picture and the voice are both stopped.
pub const PLAY_ICON: &str = "\u{25b6}";

// --- S1 where the preview starts -----------------------------------------------------------------------------

/// S1 (`play from the line, else the cut's start`): what a press of ▶ or a click on the picture means.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Start {
    /// Cue here and play.
    Play(f64),
    /// No clips at all — say [`NOTHING_TO_PREVIEW`] rather than toggling an empty pipeline.
    NoClips,
    /// The second is on no recording: cueing it would report itself as playing over a black frame.
    Uncovered { at: f64 },
}

/// S1: the second to start from, and whether anything can be shown there. A selected line wins over the top
/// of the cut — that is what "play from the line" means — and either way the second has to be inside a
/// recording, which `covered` answers ([`crate::timeline::filmed_runs`] is who computes it).
pub fn start_point(segs: &[Seg], line: Option<f64>, covered: &[(f64, f64)]) -> Start {
    let Some(first) = segs.first() else {
        return Start::NoClips;
    };
    let at = line.unwrap_or(first.s);
    let shown = covered.iter().any(|(from, to)| at >= *from && at < *to);
    if shown {
        return Start::Play(at);
    }
    Start::Uncovered { at }
}

// --- S2 which line owns a second -----------------------------------------------------------------------------

/// S2: the second a line's window ends. The next spoken line on the same clip starts this one's end — a clip
/// may carry several lines, and each owns the seconds up to its successor — and otherwise it is the clip's
/// own end. Timed by session second rather than by index so an edited cut still speaks the right line.
pub fn line_end(index: usize, entries: &[Entry]) -> f64 {
    let entry = &entries[index];
    entries
        .iter()
        .skip(index + 1)
        .find(|next| same_clip(next, entry) && !next.text.trim().is_empty())
        .map_or(entry.e, |next| next.s + next.at)
}

/// S2: the line whose seconds `t` falls in — from its own start (`s + at`) to wherever its window ends. A
/// line with no words is skipped entirely: it is a deliberate silence, and offering it to the tick would send
/// an empty string to the speech server.
pub fn line_at(t: f64, entries: &[Entry]) -> Option<usize> {
    (0..entries.len()).find(|&index| {
        let entry = &entries[index];
        !entry.text.trim().is_empty() && t >= entry.s + entry.at && t < line_end(index, entries)
    })
}

/// Two lines belong to the same clip when they were written against the same bounds — the record stores the
/// cut's own seconds, which is what keeps "the same clip" exact after an edit.
fn same_clip(one: &Entry, other: &Entry) -> bool {
    one.s == other.s && one.e == other.e
}

// --- S2 what the tick does -----------------------------------------------------------------------------------

/// Which takes this line of sight has: on disk, not yet spoken, or refused by the server and remembered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Take {
    Exists,
    Missing,
    Failed,
}

/// S2: what one tick of the picture means. The page acts on this and nothing else — the decisions are here so
/// they can be tested without a player.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Tick {
    /// Nothing to change: still inside the same line's seconds, or over a clip with no line.
    Idle,
    /// A gap, but a line is still speaking and the render would have grown the clip for it: hold the
    /// boundary rather than jumping away from the middle of a sentence.
    Hold,
    /// Past the last clip: the cut is over, so both players stop.
    Pause,
    /// A gap the edit removed: skip forward to where the cut starts again.
    SkipTo(f64),
    /// Entering a line whose take exists: start its wav at the offset the picture is at.
    Speak { line: usize, offset: f64 },
    /// Entering a line with no take: pause the picture and speak it ([`synthesizing`]).
    Synthesize { line: usize },
    /// Entering a line whose take already failed: run the clip mute and say so again ([`sticky_failed`]).
    Mute { line: usize },
}

/// S2: one tick. In the prototype's order, because the order is the behaviour:
///
/// 1. inside a clip → look at its line (or stay [`Tick::Idle`]);
/// 2. in a gap with a line still speaking within [`MAX_EXTEND_SECONDS`] of its clip's end → [`Tick::Hold`];
/// 3. in a gap past the last clip → [`Tick::Pause`];
/// 4. any other gap → [`Tick::SkipTo`] the next clip's start.
///
/// `takes` is one entry per line, in order; a missing answer counts as [`Take::Missing`], which is the safe
/// reading — a take nobody has looked for has not been spoken yet. `speaking` is the line whose audio is
/// running, if any.
pub fn tick(t: f64, segs: &[Seg], entries: &[Entry], takes: &[Take], speaking: Option<usize>) -> Tick {
    if cut_hear::scene_at(segs, t).is_some() {
        let Some(line) = line_at(t, entries) else {
            return Tick::Idle;
        };
        let entry = &entries[line];
        let offset = t - (entry.s + entry.at);
        return match takes.get(line).copied().unwrap_or(Take::Missing) {
            Take::Exists => Tick::Speak { line, offset },
            Take::Missing => Tick::Synthesize { line },
            Take::Failed => Tick::Mute { line },
        };
    }
    // A second the cut removed. The line still talking is the render's business up to the extension it is
    // allowed, so hold rather than skip: the boundary jump would drop the middle of a sentence.
    if let Some(line) = speaking.filter(|line| entries.get(*line).is_some()) {
        if t < entries[line].e + MAX_EXTEND_SECONDS {
            return Tick::Hold;
        }
    }
    match segs.iter().filter(|seg| seg.s > t).min_by(|left, right| left.s.total_cmp(&right.s)) {
        Some(next) => Tick::SkipTo(next.s),
        None => Tick::Pause,
    }
}

/// S2: where the picture goes when a synthesis ends. Success resumes at the line's own start, not at the
/// second the picture froze: a hold can land mid-line (an edited line is re-synthesized where its old audio
/// used to be), and starting the new wav at that stale offset would begin it past its end whenever the new
/// line is shorter — a video with no voice. Failure has no new line to land on, so the picture simply carries
/// on from where it stopped.
pub fn resume_after_synthesis(frozen: f64, line_start: f64, ok: bool) -> f64 {
    if ok {
        line_start
    } else {
        frozen
    }
}

/// S2: the takes the server has already refused. Keyed on the wav rather than the row — a take is named by
/// what was spoken, so re-editing a line gives it a new key and a fresh chance, while the same failed take
/// stays mute however many times the cut runs past it.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Failed {
    wavs: Vec<String>,
}

impl Failed {
    /// Remember that this take was refused.
    pub fn add(&mut self, wav: &str) {
        if !self.holds(wav) {
            self.wavs.push(wav.to_string());
        }
    }

    /// Is this take known-bad, so the clip should simply run mute?
    pub fn holds(&self, wav: &str) -> bool {
        self.wavs.iter().any(|known| known == wav)
    }

    /// The row's ▶ retried this line: clear the failure first, so a second failure is noticed as a second
    /// failure. Answers whether there was one to clear — which is what tells the page this press was a retry
    /// rather than a first attempt.
    pub fn retry(&mut self, wav: &str) -> bool {
        let before = self.wavs.len();
        self.wavs.retain(|known| known != wav);
        self.wavs.len() != before
    }
}

// --- S3 the sound is the render's ------------------------------------------------------------------------------

/// S3 (`ducked by the game volume`): how loud the whole clip is while a line speaks. The value is the
/// project's P.policy.gameVolume — §10's default 0.22 belongs to [`crate::project`] and is not re-declared
/// here — and 1 when nothing is being spoken, so the duck is only ever a lowering.
///
/// Where this sits in the render's chain is [`crate::render_fx`]'s answer (after the lane mix, before the
/// narration), and the per-lane dips are [`crate::fx_lane`]'s; the preview multiplies this by what those
/// owners already said rather than inventing a third gain.
pub fn duck(line_speaking: bool, game_volume: f64) -> f64 {
    if line_speaking {
        game_volume
    } else {
        1.0
    }
}

/// S3 (`lanes and hushes follow the cut`): which recordings this second hears. [`crate::cut_hear`] owns the
/// rule — a scene silences lanes by property, an insert has no say over any — so this only asks it the
/// question at the picture's second; a preview that disagreed with the cut would preview a different video.
pub fn lanes_heard(cut: &Cut, t: f64) -> Vec<String> {
    match cut_hear::scene_at(&cut.segs, t).and_then(|scene| cut.segs.get(scene)) {
        Some(scene) => cut_hear::hush(scene).to_vec(),
        None => Vec::new(),
    }
}

/// S3 (`speed effects apply at seeks`): the rate to seek and play at for this second. Owned by
/// [`crate::cut_hear::rate_under`], which is also what the render reads, so a clip hurried by an effect is
/// hurried in both.
pub fn seek_rate(fx: &[Fx], t: f64) -> f64 {
    cut_hear::rate_under(fx, t)
}

// --- S4 seeks land on the cut --------------------------------------------------------------------------------

/// S4: a target inside a clip is its own answer; a target in a gap goes back to the previous clip's end or
/// forward to the next clip's start, because those seconds are not in the video at all. Outside the cut there
/// is no boundary to snap to, so the target stands and the caller decides what it means.
pub fn snap_seek(t: f64, segs: &[Seg], forward: bool) -> f64 {
    if cut_hear::scene_at(segs, t).is_some() {
        return t;
    }
    if forward {
        // The next clip that actually starts ahead of the target — not the first one in the list: a cut's
        // segments are stored in session order, but "ahead" is a question about seconds.
        return segs
            .iter()
            .filter(|seg| seg.s > t)
            .min_by(|left, right| left.s.total_cmp(&right.s))
            .map_or(t, |seg| seg.s);
    }
    // The clip that ends nearest before the target. Not "the last one in the list": a preview runs the cut,
    // so going back means the end closest behind this second, whatever order the segments arrived in.
    segs.iter()
        .filter(|seg| seg.e <= t)
        .max_by(|left, right| left.e.total_cmp(&right.e))
        .map_or(t, |seg| seg.e)
}

// --- S5 the rows follow the picture, and the picture follows the rows ----------------------------------------

/// S5 (`row selection follows the playhead`): which row is blue right now. The same lookup the tick uses —
/// one line owns a second, whether it was found for the picture or for the list.
pub fn selected_row(t: f64, entries: &[Entry]) -> Option<usize> {
    line_at(t, entries)
}

/// S5 (`selecting a row seeks to its line's lead-in`): where ▶ drops the picture so the line can be seen to
/// land — [`AUDITION_LEAD_SECONDS`] ahead of it, never back before the clip starts (there is no video there),
/// and not into another line's seconds either: those belong to whoever is speaking, so this line starts the
/// preview instead.
pub fn lead_in(target: usize, entries: &[Entry]) -> f64 {
    let entry = &entries[target];
    let start = entry.s + entry.at;
    let ahead = start - AUDITION_LEAD_SECONDS;
    let before = ahead.max(entry.s);
    match selected_row(before, entries) {
        Some(other) if other != target => start,
        _ => before,
    }
}

// --- S6 ⏹ ----------------------------------------------------------------------------------

/// S6: what a press of ⏹ leaves behind — both players stopped and the triangle free for the step's own use
/// again, not held by a preview that is no longer running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stopped {
    pub picture_paused: bool,
    pub voice_paused: bool,
    pub started: bool,
    pub play_button: &'static str,
}

/// S6 (`⏹ stops both players`): the picture and the narration riding along with it stop together, and ▶ goes
/// back to being the run's button rather than a row's pause.
pub fn stop() -> Stopped {
    Stopped {
        picture_paused: true,
        voice_paused: true,
        started: false,
        play_button: PLAY_ICON,
    }
}
