//! F2.1 — ▶ on the Cut page: the recording, cuts and all.
//!
//! The page's button forwards here and decides nothing (spec/00-principles.md §5): which of play,
//! pause or switch-to-the-recording a press means, from where playback starts, and what the clock reads.
//! spec/inventory/cut.md §D is normative for the wording; F2.1's own branches are S1-S4 below.

use crate::run;
use crate::tools;

/// One preview, three ways to watch it (§D: "exactly one lit").
///
/// `cut_only` is ▶✂'s switch (F2.2) and `reviewing` ▶✂✂'s (F2.3); both are read here because ▶ has to
/// know which one it is interrupting — the recording is not a fourth preview, it is what is left when
/// neither is on.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Player {
    pub transport: run::Transport,
    pub cut_only: bool,
    pub reviewing: bool,
    /// The red line, in session seconds. `None` is a page that has never been clicked: §D's `--:--.-`.
    pub playhead: Option<f64>,
}

/// S1's status, §D verbatim — the sentence that tells the person the clock just changed meaning.
pub const RECORDING_STATUS: &str = "preview is the recording again \u{2014} everything plays, cuts and all";

/// What one press of ▶ did, so the page can print the right thing without deciding anything itself.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Press {
    Paused,
    Playing { from: f64 },
    SwitchedToRecording { kept_playing: bool },
    NoFootage,
}

/// ▶ (§F2.1's S1 and S2).
///
/// `runs` are the filmed stretches — the merged spans from [`crate::timeline::filmed_runs`] — because
/// a session with nothing filmed has nothing for ▶ to show, and starting at second 0 would pretend
/// otherwise (the spec is silent here; refusing names the reason, which is what §0's failure rule asks).
pub fn press_recording(player: &mut Player, runs: &[(f64, f64)]) -> Press {
    // S1: the preview is the cut. A review counts as one too — ▶✂✂ skips the removed stretches exactly
    // as ▶✂ does, so switching back from it means the same thing and prints the same sentence.
    if player.cut_only || player.reviewing {
        let kept_playing = player.transport.playing;
        player.cut_only = false;
        player.reviewing = false;
        // "already playing: carry on": pressing ▶ must not pause the preview it has just switched to a
        // view of, or the button would stop the thing its own status line says is running.
        return Press::SwitchedToRecording { kept_playing };
    }

    // S2: the toggle. A pause clears `playing` and leaves `started`, which is what keeps the preview the
    // run bar's business until ⏹ ends it (run.rs).
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

/// Where the line is after `elapsed`, given it was at `from`.
///
/// S3: "every second plays, cuts and all" — in the recording preview nothing is skipped, so a line that
/// starts inside a stretch the cut removed still walks straight through it. The skipping belongs to the
/// ✂ preview (F2.2); this takes `cut_only` and answers `from + elapsed` for it too rather than inventing
/// that round's rule here — an invented one would be a second place F2.2 has to remember to change.
pub fn play_advance(from: f64, elapsed: f64, _cut_only: bool) -> f64 {
    from + elapsed
}

/// §B's status for a fold ▶ walked into, verbatim apart from the time it names.
pub fn unfolded_status(t: f64) -> String {
    format!("unfolded {} \u{2014} \u{25b6} ran into it", tools::mm_ss(t))
}

/// S3: "into a folded seam → it opens". A fold the line has reached is removed and its status returned.
///
/// Not while paused (nothing is running to run into anything) and not in the ✂ preview — §B: "▶✂ never
/// enters one", because a fold is a stretch the person chose not to look at, and the cut preview walking
/// into it would unfold their own edit under them.
pub fn walk_fold(player: &Player, folds: &mut Vec<[f64; 2]>) -> Option<String> {
    if !player.transport.playing || player.cut_only {
        return None;
    }
    let line = player.playhead?;
    let index = folds.iter().position(|[start, end]| line >= *start && line < *end)?;
    let [start, _] = folds.remove(index);
    Some(unfolded_status(start))
}

/// S3: "at a recording's end the line walks on to the next one (or a camera still rolling then)".
///
/// `runs` are merged spans from [`crate::timeline::filmed_runs`] — sorted and disjoint, which is what
/// makes "covering this second" and "the next start" mean one thing each. Another run covering `t` means
/// a camera is still rolling: the line stays put and the player re-cues onto that file. Otherwise the
/// next run's start; past the last run there is nothing to walk on to.
pub fn walk_on(runs: &[(f64, f64)], t: f64) -> Option<f64> {
    if runs.iter().any(|(start, end)| t >= *start && t < *end) {
        return Some(t);
    }
    runs.iter().find(|(start, _)| *start > t).map(|(start, _)| *start)
}

// --- S4: the clock and the line ------------------------------------------------------------------------

/// How often the line follows the player: spec/10-parameters.md:133's "tick 100 ms" (no `P.*` row of its
/// own), held here because this is the module whose rule reads it — S4's "ten times a second".
pub const TICK_MS: u64 = 100;

/// S4: the line follows the player ten times a second.
pub fn follows_per_second() -> usize {
    1000 / TICK_MS as usize
}

/// The clock between two ticks (§D: "liveClock extrapolates ≤ one tick at the current rate, monotone").
///
/// The cap is what keeps it honest: a clock that ran half a second ahead of the last real position would
/// jump back when that position arrived, and a playhead that steps backwards twice a second is unreadable
/// even when every one of its positions was correct.
pub fn live_clock(base: f64, since_ms: u64, rate: f64) -> f64 {
    base + (rate * since_ms.min(TICK_MS) as f64 / 1000.0).max(0.0)
}

/// The clock's face: `mm:ss.d` of session time (§D), and `--:--.-` for a page nobody has clicked yet.
///
/// Session time, always: under ▶✂ the clock reads the cut's own time and grows a tooltip explaining it,
/// which is F2.2's rule rather than this one.
pub fn clock(playhead: Option<f64>) -> String {
    let Some(t) = playhead else {
        return "--:--.-".to_string();
    };
    let tenths = (t * 10.0) as u64 % 10;
    format!("{}.{}", tools::mm_ss(t), tenths)
}
