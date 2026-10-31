//! F4.5 Preview the cut with narration — `spec/07-narrate.md` F4.5, steps S1–S6.

use crate::cut::{Cut, Seg};
use crate::cut_hear;
use crate::narrate_screen::AUDITION_LEAD_SECONDS;
use crate::narration::Entry;

/// P.eng.narrationMaxExtendSeconds: how long past its own end a clip boundary is held while a line still
/// speaks.
pub const MAX_EXTEND_SECONDS: f64 = 4.0;

// --- What the person is told (S1, S2) ------------------------------------------------------------------------

pub const NOTHING_TO_PREVIEW: &str = "nothing to preview yet \u{2014} cut some clips first";

/// S1: there are clips, but the second being cued is on no recording, so nothing can be loaded and the
/// frame would sit black while the transport claimed to be playing.
pub const NO_RECORDING_AT_START: &str = "no recording covers the start of the cut";

/// S2: the picture has paused for a line that has never been spoken.
pub fn synthesizing(line: usize) -> String {
    format!("synthesizing line {}", line + 1)
}

/// S2: the take arrived and the picture is about to move again.
pub fn line_ready(line: usize) -> String {
    format!("line {} ready", line + 1)
}

/// S2: a line failed while the picture was waiting for it, and the run carries on without its voice.
pub fn failed_playing_on(line: usize) -> String {
    format!("line {} failed -- see log; playing on without it", line + 1)
}

/// S2: this take's synthesis already failed, so the clip runs mute again.
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
    Play(f64),
    NoClips,
    Uncovered { at: f64 },
}

/// S1: what a press of ▶ or a click on the picture amounts to.
#[derive(Debug, Clone, PartialEq)]
pub enum Pressed {
    Refused(String),
    Playing { from: f64 },
    Paused,
}

impl Pressed {
    /// The refusal's sentence, `None` when the press started or paused playback.
    pub fn refused(&self) -> Option<&str> {
        match self {
            Self::Refused(why) => Some(why),
            _ => None,
        }
    }

    /// Whether this press leaves the preview running (the state the page stores).
    pub fn playing(&self) -> bool {
        matches!(self, Self::Playing { .. })
    }
}

/// S1: the whole of one click.
pub fn press_picture(
    playing: bool,
    segs: &[Seg],
    line_start: Option<f64>,
    covered: &[(f64, f64)],
) -> Pressed {
    if playing {
        return Pressed::Paused;
    }
    match start_point(segs, line_start, covered) {
        Start::Play(at) => Pressed::Playing { from: at },
        Start::NoClips => Pressed::Refused(NOTHING_TO_PREVIEW.to_string()),
        Start::Uncovered { .. } => Pressed::Refused(NO_RECORDING_AT_START.to_string()),
    }
}

/// S1: the second to start from, and whether anything can be shown there.
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

/// S2: the second a line's window ends.
pub fn line_end(index: usize, entries: &[Entry]) -> f64 {
    let entry = &entries[index];
    entries
        .iter()
        .skip(index + 1)
        .find(|next| same_clip(next, entry) && !next.text.trim().is_empty())
        .map_or(entry.e, |next| next.s + next.at)
}

/// S2: the line whose seconds `t` falls in — from its own start (`s + at`) to wherever its window ends.
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

/// S2: what one tick of the picture means.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Tick {
    /// Nothing to change: still inside the same line's seconds, or over a clip with no line.
    Idle,
    Hold,
    Pause,
    SkipTo(f64),
    Speak {
        line: usize,
        offset: f64,
    },
    Synthesize {
        line: usize,
    },
    Mute {
        line: usize,
    },
}

pub fn tick(
    t: f64,
    segs: &[Seg],
    entries: &[Entry],
    takes: &[Take],
    speaking: Option<usize>,
) -> Tick {
    if cut_hear::scene_at(segs, t).is_some() {
        let Some(line) = line_at(t, entries) else {
            return Tick::Idle;
        };
        let entry = &entries[line];
        let offset = t - (entry.s + entry.at);
        return match takes.get(line).copied().unwrap_or(Take::Missing) {
            Take::Exists if speaking == Some(line) => Tick::Idle,
            Take::Exists => Tick::Speak { line, offset },
            Take::Missing => Tick::Synthesize { line },
            Take::Failed => Tick::Mute { line },
        };
    }
    if let Some(line) = speaking.filter(|line| entries.get(*line).is_some()) {
        if t < entries[line].e + MAX_EXTEND_SECONDS {
            return Tick::Hold;
        }
    }
    match segs
        .iter()
        .filter(|seg| seg.s > t)
        .min_by(|left, right| left.s.total_cmp(&right.s))
    {
        Some(next) => Tick::SkipTo(next.s),
        None => Tick::Pause,
    }
}

/// S2: the second the picture stands on one tick later.
pub fn advance(t: f64, delta: f64, rate: f64, segs: &[Seg]) -> f64 {
    let Some(scene) = cut_hear::scene_at(segs, t).and_then(|index| segs.get(index)) else {
        return t;
    };
    ((t + delta * rate).max(0.0)).min(scene.e)
}

/// S2: where the picture goes when a synthesis ends.
pub fn resume_after_synthesis(frozen: f64, line_start: f64, ok: bool) -> f64 {
    if ok {
        line_start
    } else {
        frozen
    }
}

/// S2: the takes the server has already refused.
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
    /// failure.
    pub fn retry(&mut self, wav: &str) -> bool {
        let before = self.wavs.len();
        self.wavs.retain(|known| known != wav);
        self.wavs.len() != before
    }
}

// --- S3 the sound is the render's ------------------------------------------------------------------------------

/// S3 (`ducked by the game volume`): how loud the whole clip is while a line speaks.
pub fn duck(line_speaking: bool, game_volume: f64) -> f64 {
    if line_speaking {
        game_volume
    } else {
        1.0
    }
}

/// S3 (`lanes and hushes follow the cut`): which recordings this second hears.
pub fn lanes_heard(cut: &Cut, t: f64) -> Vec<String> {
    match cut_hear::scene_at(&cut.segs, t).and_then(|scene| cut.segs.get(scene)) {
        Some(scene) => cut_hear::hush(scene).to_vec(),
        None => Vec::new(),
    }
}

// --- S4 seeks land on the cut --------------------------------------------------------------------------------

/// S4: a target inside a clip is its own answer; a target in a gap goes back to the previous clip's end or
/// forward to the next clip's start, because those seconds are not in the video at all.
pub const SCRUB_EDGE_SECONDS: f64 = 0.05;

/// The second a seek's CLOCK takes (what the label reads), as opposed to the second the player is cued at
/// ([`snap_seek`]): going back, the end of the clip nearest behind the target entered by `SCRUB_EDGE_SECONDS`,
/// so the reading is inside the clip whose frame shows.
pub fn snap_clock(t: f64, segs: &[Seg], forward: bool) -> f64 {
    if forward || cut_hear::scene_at(segs, t).is_some() {
        return snap_seek(t, segs, true);
    }
    segs.iter()
        .filter(|seg| seg.e <= t)
        .max_by(|left, right| left.e.total_cmp(&right.e))
        .map_or(t, |seg| (seg.e - SCRUB_EDGE_SECONDS).max(seg.s))
}

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
    // The clip that ends nearest before the target.
    segs.iter()
        .filter(|seg| seg.e <= t)
        .max_by(|left, right| left.e.total_cmp(&right.e))
        .map_or(t, |seg| seg.e)
}

// --- S5 the rows follow the picture, and the picture follows the rows ----------------------------------------

/// S5 (`selecting a row seeks to its line's lead-in`): where ▶ drops the picture so the line can be seen to
/// land — [`AUDITION_LEAD_SECONDS`] ahead of it, never back before the clip starts (there is no video there),
/// and not into another line's seconds either: those belong to whoever is speaking, so this line starts the
/// preview instead.
pub fn lead_in(target: usize, entries: &[Entry]) -> f64 {
    let entry = &entries[target];
    let start = entry.s + entry.at;
    let ahead = start - AUDITION_LEAD_SECONDS;
    let before = ahead.max(entry.s);
    match line_at(before, entries) {
        Some(other) if other != target => start,
        _ => before,
    }
}

// --- S6 ⏹ ----------------------------------------------------------------------------------

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

/// S6 (`hands ▶ back to the step`): the sentence the run bar shows once ⏹ has stopped the preview, naming
/// whose button the triangle is again.
pub const HANDED_BACK: &str = "\u{25b6} is the step's again \u{2014} the preview stopped";

/// What ⏹ hands back: the ownership sentence when both players really stopped, `None` when the struct says
/// otherwise (a `Stopped` built by hand with a player left running must not announce itself as stopped).
pub fn hand_play_back(stopped: Stopped) -> Option<&'static str> {
    if stopped.picture_paused && stopped.voice_paused && !stopped.started {
        Some(HANDED_BACK)
    } else {
        None
    }
}
