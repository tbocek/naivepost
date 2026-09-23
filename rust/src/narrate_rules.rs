//! §07-narrate#5-rules — the standing rules of the Narrate page, `spec/07-narrate.md` §5.
//!
//! Like [`crate::effect_rules`] for the effects page and [`crate::cut_rules`] for the Cut page, this module states
//! the handful of §5's clauses nothing else had written down and reaches through its own module for the rest — a
//! rule restated twice is a rule that can be changed in one place only. In §5's order:
//!
//! * entries always sorted — [`crate::narration::Narration::sort`], applied by both [`crate::narration::load`] and
//!   [`crate::narration::save`], so a hand-edited file arrives in playing order;
//! * a line's clip bounds are the cut's numbers — [`crate::shell::refit`] moves them and
//!   [`crate::shell::refit_sentence`] says what it did; `narrate_screen`'s own `same_clip` asks the same question of
//!   two entries when it decides what "the same clip" means;//! * `at` never in the last second — [`crate::narrate_screen::tag_moves_line`] and
//!   [`crate::narrate_screen::add_below`] both clamp at the clip's last second, and `shell::refit` clamps again on
//!   the way when a cut moves under a line;
//! * empty text is a deliberate answer — [`crate::narration::Entry::text`] of `""`, read back as
//!   [`crate::narrate_screen::RowStatus::Silent`] by [`crate::narrate_screen::status`], with
//!   [`crate::narration::Narration::is_silent`] and [`crate::narrate_screen::remove_line`] for the marker behind it;
//! * preview plays the cut, sound equals the render's — [`crate::narrate_preview::lanes_heard`] asks
//!   [`crate::cut_hear::hush`] and [`crate::narrate_preview::seek_rate`] asks [`crate::cut_hear::rate_under`], the
//!   same two questions the render asks;
//! * boundary held while a line speaks — [`crate::narrate_preview::tick`]'s [`crate::narrate_preview::Tick::Hold`],
//!   bounded by [`crate::narrate_preview::MAX_EXTEND_SECONDS`] (P.eng.narrationMaxExtendSeconds) because that is
//!   how far the render may grow a clip for its line;
//! * a hold resumes at the line's start — [`crate::narrate_preview::resume_after_synthesis`]: a take that arrived
//!   moves the picture to where the line is spoken, a failure carries on from where it stopped;
//! * failed synthesis sticky until retried — [`crate::narrate_preview::Failed`] remembers by wav rather than by row,
//!   and [`crate::narrate_preview::failed_playing_on`] / [`crate::narrate_preview::sticky_failed`] say which of the
//!   two situations the log is in;
//! * captions-only short-circuits everything that speaks — [`voice_is_captions`] and [`speaks_anything`];
//! * the run bar is the preview's transport only once the preview started — [`bar_serves_the_preview`], next to
//!   [`crate::run::transport_for`], which answers the page question;
//! * cache keys stable, nothing deletes old wavs — [`deletes_a_spoken_take`], beside the frozen key in
//!   [`crate::narration`];
//! * hand-picked takes never re-ranked — [`takes_are_hand_picked`], beside
//!   [`crate::narrate_data::clean_takes`], which sorts and merges but judges nothing;
//! * a voice not on offer leaves the picker empty rather than pointing at the wrong speaker — [`narrator_slot`],
//!   [`picker_row`] and [`missing_voice`].
//!
//! It does NOT own: the dropdown or any other widget (the page sets sensitivity and selection from these answers),
//! the record itself (F4.1, [`crate::narration`]), speaking a line (F4.4), or the render's filter chain (F5.x).

use crate::narrate_data::{Take, OWN_VOICE};
use crate::narrate_screen::{CAPTIONS, VoiceOption};
use crate::run::{NARRATOR_SLOTS, Transport};

/// §5 (`captions-only short-circuits everything that speaks`): is the chosen voice the one that speaks nothing?
/// `captions` is the picker's first row and an **id**, not a label with no id behind it, which is why every step
/// that could speak checks it rather than matching words. It joins a run and never shortens it — F4.6's S1 says a
/// captions-only voice still opens and closes the speaking job so the bar finishes — and the lines it would have
/// spoken go through the same seam F4.8 uses ([`crate::narrate_off::lines_to_speak`]).
pub fn voice_is_captions(voice: &str) -> bool {
    voice == CAPTIONS
}

/// §5's clause read with F4.8's: is there anything for a voice to speak at all? A video gets here two ways and the
/// spec keeps them distinct — the voice chosen is `captions` (§1's picker), or narration is switched off
/// (`no_narration`, §01 line 75). Either answer means no request, no take and no subtitle track from a voice.
pub fn speaks_anything(voice: &str, no_narration: bool) -> bool {
    !voice_is_captions(voice) && !no_narration
}

/// §5 (`the run bar is the preview's transport only once the preview started`): ▶ belongs to the step until the
/// preview has actually been started, and stays the preview's transport until ⏹ ends it. Four states, two answers:
/// not started and not playing → the step; playing whether or not it was started first → the transport; started
/// and paused ([`Transport::cued`]) → still the transport, because ⏸ has to be able to resume and ⏹ to end what is
/// parked part way through. A clip merely cued by clicking a line to look at it is not playback (prototype
/// `pageTransport`, gui/runbar.go:28-52). Which *pages* have a transport at all is [`crate::run::transport_for`]'s.
pub fn bar_serves_the_preview(transport: Transport) -> bool {
    transport.started
}

/// §5 (`cache keys stable, nothing deletes old wavs`): would removing this file throw away a line somebody already
/// heard? A take is never cleaned up — the key is frozen and derived from what was spoken, so an unchanged line is
/// free to re-speak and a project folder carried to another machine still plays every line it ever spoke. The only
/// files this page removes are the built voice reference and its base ([`crate::narrate_data::drop_reference`]),
/// which are answers to a question that has just changed, not recordings.
pub fn deletes_a_spoken_take(removed: &std::path::Path) -> bool {
    removed
        .ancestors()
        .any(|dir| dir.file_name() == Some(std::ffi::OsStr::new("tts")))
}

/// §5 (`hand-picked takes never re-ranked`): did somebody choose the seconds to clone from? Chosen takes win
/// outright — no cap, no diarization, no re-ranking (F4.6's `hand-picked takes?` yes-branch). That is why
/// [`crate::narrate_data::clean_takes`] sorts by time and merges overlaps but never orders by any quality: the
/// person chose them, and its one floor is P.eng.takeMinSeconds ([`crate::narrate_screen::TAKE_MIN_SECONDS`]),
/// which catches a click that moved two pixels rather than judging a take worth less than another.
pub fn takes_are_hand_picked(takes: &[Take]) -> bool {
    !takes.is_empty()
}

/// §5 (`a voice not on offer leaves the picker empty`): which narrator slot does this id name? `own` is slot 1 —
/// the project's own voice, before slots were numbered — and `narrator2`..`narratorN` are 2..=N; anything else,
/// including a slot number past [`NARRATOR_SLOTS`], names no slot at all (prototype `narratorSlot`).
pub fn narrator_slot(id: &str) -> Option<usize> {
    if id == OWN_VOICE {
        return Some(1);
    }
    let number = id.strip_prefix("narrator")?.parse::<usize>().ok()?;
    (number >= 1 && number <= NARRATOR_SLOTS).then_some(number)
}

/// The row the dropdown should show for the voice this project stores, or `None` when nothing on offer is that
/// voice — and then the page shows **no** row rather than the first one, because a picker silently pointing at a
/// voice nobody chose is how the wrong speaker gets into a finished video.
///
/// `own` selects the Narrator 1 row: a project written before slots were numbered stores `own`, and it means the
/// same recording slot 1 holds today, so naming that row is honest where an empty picker would say "nothing was
/// chosen". A file's name matches its own row exactly, which is how the voices folder stays recognisable.
pub fn picker_row(options: &[VoiceOption], stored: &str) -> Option<usize> {
    let wanted = if stored == OWN_VOICE {
        "narrator1"
    } else {
        stored
    };
    options.iter().position(|option| option.id == wanted)
}

/// Which of the two sentences to show when [`picker_row`] found nothing, since they are fixed in different places:
/// a narrator slot is tagged on Prepare, a file lives in the voices folder. EM DASH (U+2014) as §F4.6 S1 writes
/// both, and the id quoted the way the prototype's `%q` quotes it.
pub fn missing_voice(stored: &str, voices_dir: &str) -> String {
    if let Some(slot) = narrator_slot(stored) {
        return format!(
            "narrator {slot} is not tagged on the Prepare step \u{2014} tag a recording, or pick another voice"
        );
    }
    format!("voice \"{stored}\" is no longer in {voices_dir} \u{2014} pick another")
}
