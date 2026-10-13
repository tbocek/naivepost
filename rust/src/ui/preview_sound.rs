//! §05-cut#8 bullet 2 — `each change of the footage's own mute logs one line naming the reason`, and the
//! stream volume that is reset once at build.
//!
//! The wording of every line here belongs to [`crate::preview`]; this module is the MISSING HALF — the call.
//! Until now `preview::sound_change` answered a question nobody asked and `preview::reset_stream_volume`
//! described a reset nothing performed, so no mute reason could ever reach the log and the spec's bullet was
//! true of strings rather than of the program. Two doors, each called from the one place that already holds
//! the state the line is about:
//!
//! - [`report_mute_change`] from `ui::window::apply_mix`, the door that already lands `footage_muted` on the
//!   player every tick. It reads the player's PREVIOUS answer before overwriting it, which is what makes the
//!   log one line per CHANGE rather than one line per 100 ms tick.
//! - [`reset_stream_volume_once`] from the window's build, once, before any tick has asked for a gain.

use crate::cut_hear;
use crate::preview;

thread_local! {
    /// Whether this process has already put the stream volume back to full. `reset_stream_volume` is the rule
    /// (`Some` the first time, `None` after it); this is the fact it is asked about.
    static STREAM_RESET: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Log the ONE line a change of the footage's own mute deserves, or nothing at all when nothing changed.
///
/// `heard_before` is what the player was told at the previous tick and `heard_now` what this tick's mix says;
/// `card_held` is the card-over-the-picture answer, which outranks the scene's own hearing because it is the
/// reason a person can see on the band (`preview::sound_change`'s rule, not a second one taken here).
pub fn report_mute_change(
    heard_before: bool,
    heard_now: bool,
    card_held: bool,
    log: &dyn Fn(&str),
) {
    let reason = if card_held {
        preview::MuteReason::OverPicture
    } else {
        preview::MuteReason::NotHeard
    };
    if let Some(line) = preview::sound_change(heard_before, heard_now, reason) {
        log(&line);
    }
}

/// Put the stream volume back to full, once per process, through the element this app owns.
///
/// `preview::volume_is_app_owned` is the answer §8 asks for — gain and mute go to an element of this app's,
/// never to the sound server's per-application stream volume, which the server remembers between runs and
/// would leave last session's loudness behind for everything else on the desktop. There is no pactl/wpctl/
/// amixer leg anywhere in `src/` and none is wanted: `Player::volume` is the app's own gain slot, multiplied
/// into every pipeline by `cut_hear::mix_at`, so writing `STREAM_VOLUME` there IS the reset §8 describes.
/// `preview::reset_stream_volume` decides whether it is still owed.
pub fn reset_stream_volume_once(window: &adw::ApplicationWindow) {
    if !preview::volume_is_app_owned() {
        return;
    }
    let owed = STREAM_RESET.with(|done| preview::reset_stream_volume(done.get()));
    let Some(gain) = owed else { return };
    // `RESET_TO` is the same number the rule answers with, so the level written and the level §8 names cannot
    // drift apart unnoticed.
    debug_assert_eq!(gain, RESET_TO, "the reset writes the level it was told to");
    STREAM_RESET.with(|done| done.set(true));
    crate::ui::cut_cut_play::hand_gain(window, gain, crate::ui::line_position(window).t);
}

/// What the gain a built window carries after the build-time reset — a read for a check, not a door the
/// program uses: it reads the same player state a press reads, so a test measures the reset rather than this
/// module's intention.
pub fn stream_gain_for_test(window: &adw::ApplicationWindow) -> Option<f64> {
    crate::ui::window::preview_player(window).volume
}

/// The full level §8 requires the stream volume be put at once: named here so the value the reset writes and
/// the value `preview::reset_stream_volume` answers with are the same number, read from one place.
pub const RESET_TO: f64 = preview::STREAM_VOLUME;

/// What the mix's footage answer was before this tick, which is the `heard_before` half of
/// [`report_mute_change`]. Kept here so `apply_mix` stays one line of wiring.
pub fn heard_before(window: &adw::ApplicationWindow) -> bool {
    crate::ui::window::live_player(window)
        .map(|player| !player.borrow().footage_muted)
        .unwrap_or(true)
}

/// Whether a card holds the footage at `t` — a read for a check, not a door the program uses: the card
/// question is `apply_mix`'s own (it mutes for the same reason), and this only lets a test say WHICH reason a
/// printed line named.
pub fn card_holds_for_test(_window: &adw::ApplicationWindow, segs: &[crate::cut::Seg], t: f64) -> bool {
    crate::ui::cut_card_preview::card_hearing(
        &crate::cut::Cut { segs: segs.to_vec(), ..Default::default() },
        t,
    )
    .is_some_and(|hearing| hearing.footage_held)
}

/// The mix's footage answer as `heard` — the one spelling both halves of [`report_mute_change`] use, so a
/// `footage_muted` flip cannot be read backwards somewhere later.
pub fn heard_now(mix: &cut_hear::Mix) -> bool {
    !mix.footage_muted
}
