//! F2.1 — ▶ on the Cut page: the recording, cuts and all.

use crate::run;
use crate::tools;

/// One preview, three ways to watch it (§D: "exactly one lit").
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Player {
    pub transport: run::Transport,
    pub cut_only: bool,
    pub reviewing: bool,
    /// The red line, in session seconds. `None` is a page that has never been clicked: §D's `--:--.-`.
    pub playhead: Option<f64>,
    pub volume: Option<f64>,
    pub footage_muted: bool,
}

impl Default for Player {
    fn default() -> Self {
        Self {
            transport: run::Transport::default(),
            cut_only: false,
            reviewing: false,
            playhead: None,
            // Full, not zero: an untouched slider means audible. `f64::default()` would start every
            // preview silent and read as a broken speaker rather than an unset control.
            volume: Some(crate::cut_hear::VOLUME_DEFAULT),
            footage_muted: false,
        }
    }
}

/// S1's status, §D verbatim — the sentence that tells the person the clock just changed meaning.
pub const RECORDING_STATUS: &str =
    "preview is the recording again \u{2014} everything plays, cuts and all";

/// What one press of ▶ did, so the page can print the right thing without deciding anything itself.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Press {
    Paused,
    Playing {
        from: f64,
    },
    /// S1: the preview became the recording and plays: carried on when it was playing (`kept_playing`), else
    /// started from `started`, the line.
    SwitchedToRecording {
        kept_playing: bool,
        started: Option<f64>,
    },
    NoFootage,
}

/// ▶ (§F2.1's S1 and S2).
pub fn press_recording(player: &mut Player, runs: &[(f64, f64)]) -> Press {
    if player.cut_only || player.reviewing {
        let kept_playing = player.transport.playing;
        player.cut_only = false;
        player.reviewing = false;
        // "already playing: carry on": pressing ▶ must not pause the preview it has just switched to a
        // view of, or the button would stop the thing its own status line says is running.
        if kept_playing {
            return Press::SwitchedToRecording {
                kept_playing,
                started: None,
            };
        }
        let Some(from) = player
            .playhead
            .or_else(|| runs.first().map(|(start, _)| *start))
        else {
            return Press::NoFootage;
        };
        player.transport.started = true;
        player.transport.playing = true;
        player.playhead = Some(from);
        return Press::SwitchedToRecording {
            kept_playing,
            started: Some(from),
        };
    }

    if player.transport.playing {
        player.transport.playing = false;
        return Press::Paused;
    }

    let Some(from) = player
        .playhead
        .or_else(|| runs.first().map(|(start, _)| *start))
    else {
        return Press::NoFootage;
    };
    // S2: play from the red line — which is where the person left it, and where the page put it if they
    // never did. Every second after that plays, cuts and all (S3).
    player.transport.started = true;
    player.transport.playing = true;
    player.playhead = Some(from);
    Press::Playing { from }
}

// --- S3: while playing -------------------------------------------------------------------------------

/// §B's status for a fold ▶ walked into, verbatim apart from the time it names.
pub fn unfolded_status(t: f64) -> String {
    format!("unfolded {} \u{2014} \u{25b6} ran into it", tools::mm_ss(t))
}

/// S3: "into a folded seam → it opens". A fold the line has reached is removed and its status returned.
pub fn walk_fold(player: &Player, folds: &mut Vec<[f64; 2]>) -> Option<String> {
    if !player.transport.playing || player.cut_only {
        return None;
    }
    let line = player.playhead?;
    let index = folds
        .iter()
        .position(|[start, end]| line >= *start && line < *end)?;
    let [start, _] = folds.remove(index);
    Some(unfolded_status(start))
}

/// S3: "at a recording's end the line walks on to the next one (or a camera still rolling then)".
pub fn walk_on(runs: &[(f64, f64)], t: f64) -> Option<f64> {
    if runs.iter().any(|(start, end)| t >= *start && t < *end) {
        return Some(t);
    }
    runs.iter()
        .find(|(start, _)| *start > t)
        .map(|(start, _)| *start)
}

// --- how far ahead the next clip is opened (P.eng.preloadLeadSeconds) ---------------------------

pub const PRELOAD_LEAD_SECONDS: f64 = 3.0;

pub const PRELOAD_TOLERANCE_SECONDS: f64 = 0.01;

/// The next run start worth preparing from `t` — the earliest one strictly ahead of the tolerance.
pub fn next_jump_ahead(runs: &[(f64, f64)], t: f64) -> Option<f64> {
    runs.iter()
        .find(|(start, _)| *start > t + PRELOAD_TOLERANCE_SECONDS)
        .map(|(start, _)| *start)
}

/// Which second to hand the spare pipeline from `t`, if any: the next run's start, once the JUMP to it is within the
/// lead — the end of the run holding `t`, which is where the transport leaves it, however long the removed stretch
/// after it. Measured to the next start instead, a hole longer than the lead was never prerolled: the line jumped
/// six seconds before the target came within three, and every such join reloaded. In a removed stretch the jump is
/// now, so the next start is handed over at once.
pub fn preload_target(runs: &[(f64, f64)], t: f64) -> Option<f64> {
    let next = next_jump_ahead(runs, t)?;
    let leaves_at = runs
        .iter()
        .find(|(start, end)| t >= *start && t < *end)
        .map(|(_, end)| *end)
        .unwrap_or(t);
    if leaves_at - t <= PRELOAD_LEAD_SECONDS {
        Some(next)
    } else {
        None
    }
}

// --- S4: the clock and the line ------------------------------------------------------------------------

/// How often the line follows the player: spec/10-parameters.md:133's "tick 100 ms" (no `P.*` row of its
/// own), held here because this is the module whose rule reads it — S4's "ten times a second".
pub const TICK_MS: u64 = 100;

/// The same beat in seconds, for a clock that walks by it: one tick's worth of picture.
pub const TICK_SECONDS: f64 = TICK_MS as f64 / 1000.0;

/// The clock between two ticks (§D: "liveClock extrapolates ≤ one tick at the current rate, monotone").
pub fn live_clock(base: f64, since_ms: u64, rate: f64) -> f64 {
    base + (rate * since_ms.min(TICK_MS) as f64 / 1000.0).max(0.0)
}

/// The clock's face: `mm:ss.d` of session time (§D), and `--:--.-` for a page nobody has clicked yet.
pub fn clock(playhead: Option<f64>) -> String {
    let Some(t) = playhead else {
        return "--:--.-".to_string();
    };
    let tenths = (t * 10.0) as u64 % 10;
    format!("{}.{}", tools::mm_ss(t), tenths)
}

// --- §05-cut#8: what the preview says about its own sound ---------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MuteReason {
    /// An insert stands over the picture — its `mute` tick (F2.7) or a stop holding the same frame.
    OverPicture,
    /// The scene under the line does not hear the footage's sound.
    NotHeard,
}

/// §05-cut#8: the one line a change of the footage's own mute logs, or `None` when nothing changed — which is
/// what makes it ONE line per change rather than one per tick.
pub fn sound_change(heard_before: bool, heard_now: bool, reason: MuteReason) -> Option<String> {
    if heard_before == heard_now {
        return None;
    }
    if heard_now {
        return Some(">>> preview: the footage's own sound is heard again".to_string());
    }
    Some(match reason {
        MuteReason::OverPicture => {
            ">>> preview: the footage's own sound is muted -- a card or a stop stands over the picture".to_string()
        }
        MuteReason::NotHeard => {
            ">>> preview: the footage's own sound is muted -- the scene under the line does not hear it".to_string()
        }
    })
}

pub const STREAM_VOLUME: f64 = 1.0;

pub fn volume_is_app_owned() -> bool {
    true
}

pub fn reset_stream_volume(reset_already: bool) -> Option<f64> {
    (!reset_already).then_some(STREAM_VOLUME)
}

pub fn playback_failure(page: &str, reason: &str) -> String {
    format!("!!! {page}: playback failed \u{2014} {reason}")
}

/// §05-cut#8 (status `"<page> would not play — see log"`): what the status line says when the log holds the
/// reason. Short, because the reason is long and already written down.
pub fn playback_status(page: &str) -> String {
    format!("{page} would not play \u{2014} see log")
}

pub fn mix_lane_failure(base: &str, reason: &str) -> String {
    format!("!!! preview: {base} will not play \u{2014} {reason}")
}
