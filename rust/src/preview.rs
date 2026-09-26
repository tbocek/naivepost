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
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Player {
    pub transport: run::Transport,
    pub cut_only: bool,
    pub reviewing: bool,
    /// The red line, in session seconds. `None` is a page that has never been clicked: §D's `--:--.-`.
    pub playhead: Option<f64>,
    /// F2.5 S6: this preview's loudness — the app's ONE preview volume, mirrored here so the player
    /// reads one number rather than reaching for the setting. Full by default (a preview nobody has set
    /// is audible, not mute). `Option::default()` is `None`, which lets the pre-F2.5 literals that
    /// build a `Player` without this field keep compiling — the reason it is an `Option` at all.
    /// `Some(0.0)` is a slider deliberately dragged to silence; `None` is nobody having said anything.
    pub volume: Option<f64>,
    /// F2.5 S2: whether the footage's own sound is muted right now. Muted BY PROPERTY, never by
    /// stopping the stream — the answer flips every tick as the line moves. False until a scene says
    /// otherwise, which is what `Default` already gives.
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

// --- how far ahead the next clip is opened (P.eng.preloadLeadSeconds) ---------------------------

/// `P.eng.preloadLeadSeconds`: how far ahead of a jump the next clip is opened. A jump the transport
/// will make — a gap skipped, a review walking on, a recording ending — is known this many seconds
/// early, and telling the player's spare pipeline that early makes the jump a swap rather than a reload,
/// so the cut lands without a hitch (prototype `preloadLead`, gui/cut_preload.go).
pub const PRELOAD_LEAD_SECONDS: f64 = 3.0;

/// The tolerance §10 names with it: which side of "now" a clip start falls on. A start inside this
/// slack is the second we are already in, not a jump to prepare for (the prototype's `t - 0.01`).
pub const PRELOAD_TOLERANCE_SECONDS: f64 = 0.01;

/// The next run start worth preparing from `t` — the earliest one strictly ahead of the tolerance.
///
/// Same `runs` contract as [`walk_on`]: merged, sorted, disjoint spans from
/// [`crate::timeline::filmed_runs`]. Past the last run there is nothing ahead to open.
pub fn next_jump_ahead(runs: &[(f64, f64)], t: f64) -> Option<f64> {
    runs.iter()
        .find(|(start, _)| *start > t + PRELOAD_TOLERANCE_SECONDS)
        .map(|(start, _)| *start)
}

/// Which second to hand the spare pipeline from `t`, if any.
///
/// Only a jump within [`PRELOAD_LEAD_SECONDS`] is worth opening now, inclusive at exactly the lead:
/// three seconds is the amount asked for, so three seconds is enough. Farther off, nothing opens —
/// the spare would sit idle on a clip while the playhead is still elsewhere.
pub fn preload_target(runs: &[(f64, f64)], t: f64) -> Option<f64> {
    let next = next_jump_ahead(runs, t)?;
    if next - t <= PRELOAD_LEAD_SECONDS {
        Some(next)
    } else {
        None
    }
}

/// Whether the time left on the clip being watched is short enough that the next one should already be
/// open — the same question from the current clip's end rather than the next clip's start (the
/// prototype's `end - t < preloadLead`).
///
/// Inclusive at the lead, so a clip with exactly three seconds left preloads rather than risking the
/// reload: the cost of opening one clip too early is an idle pipeline, the cost of one too late is a
/// visible stall on every cut.
pub fn opens_ahead(remaining_seconds: f64) -> bool {
    remaining_seconds <= PRELOAD_LEAD_SECONDS
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

// --- §05-cut#8: what the preview says about its own sound ---------------------------------------------------

/// §05-cut#8 (`each change of the footage's own mute logs one line naming the reason`): why the picture's own
/// sound went silent. Two reasons, because the two are fixed by different things and only one of them is the
/// person's doing: a card or a stop laid over the picture, and the scene's own hearing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MuteReason {
    /// An insert stands over the picture — its `mute` tick (F2.7) or a stop holding the same frame.
    OverPicture,
    /// The scene under the line does not hear the footage's sound.
    NotHeard,
}

/// §05-cut#8: the one line a change of the footage's own mute logs, or `None` when nothing changed — which is
/// what makes it ONE line per change rather than one per tick.
///
/// [`MuteReason::OverPicture`] outranks [`MuteReason::NotHeard`] whenever both are true: the card is the reason
/// a person can see on the band, and "the scene does not hear it" would send them looking for a switch that is
/// not what silenced it. The heard/not-heard answer itself comes from
/// [`crate::cut_hear::footage_sound_heard`], never from here.
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

/// §05-cut#8 (`the player's per-application stream volume … is reset to full once at build`;
/// `// preview.streamVolume` — §10 gives this no `P.` id, so it carries a bare prefix): what the stream volume
/// is put back to. Full, because anything else the sound server keeps between runs of this app.
pub const STREAM_VOLUME: f64 = 1.0;

/// §05-cut#8 (`gain and mute go to an app-owned element, never the player's per-application stream volume`):
/// `true` for the element the preview's gain and mute are written to — one this app owns and forgets when it
/// exits, unlike a stream volume the sound server remembers. The page reads this instead of hard-coding which
/// element a slider moves, so there is no second place that could quietly decide otherwise.
pub fn volume_is_app_owned() -> bool {
    true
}

/// §05-cut#8 (`which is reset to full once at build`): the value to write to the stream volume, or `None` when
/// it has already been written. Once, not per play: a preview that set it on every ▶ would leave the last
/// session's loudness behind for everything else on the desktop.
pub fn reset_stream_volume(reset_already: bool) -> Option<f64> {
    (!reset_already).then_some(STREAM_VOLUME)
}

/// §05-cut#8 (`a failing pipeline says so`): the log line for a pipeline that would not start, naming the page
/// it failed on — `!!! <page>: playback failed -- <reason>` in the log's own style, with the em dash the spec
/// spells. The reason is the server's or GStreamer's own words, never a paraphrase of them.
pub fn playback_failure(page: &str, reason: &str) -> String {
    format!("!!! {page}: playback failed \u{2014} {reason}")
}

/// §05-cut#8 (status `"<page> would not play — see log"`): what the status line says when the log holds the
/// reason. Short, because the reason is long and already written down.
pub fn playback_status(page: &str) -> String {
    format!("{page} would not play \u{2014} see log")
}

/// §05-cut#8 (a mix lane `!!! preview: <base> will not play — …`): the failure of one lane's pipeline. The
/// preview carries on with the rest, so this names the recording that dropped out rather than the whole page —
/// which is what §0's "failure is specific and local" asks of it.
pub fn mix_lane_failure(base: &str, reason: &str) -> String {
    format!("!!! preview: {base} will not play \u{2014} {reason}")
}
