//! §07-narrate#5-rules — the standing rules of the Narrate page, `spec/07-narrate.md` §5.
//!
//! Like [`crate::effect_rules`] and [`crate::cut_rules`], this states the clauses nothing else had written
//! down and points at the owning module for the rest: sorting ([`crate::narration::Narration::sort`]), refit
//! ([`crate::shell::refit`]), the preview rules in [`crate::narrate_preview`], and the voice picker
//! ([`narrator_slot`], [`picker_row`], [`missing_voice`]). It owns no widget, the record, speaking or the render.

use crate::narrate_data::{Take, OWN_VOICE};
use crate::narrate_screen::{VoiceOption, CAPTIONS};
use crate::run::{Transport, NARRATOR_SLOTS};

/// §5 (`captions-only short-circuits everything that speaks`): is the chosen voice the one that speaks nothing?
/// `captions` is the picker's first row and an **id**, so every step checks it rather than matching words. It
/// joins a run and never shortens it (F4.6 S1: the speaking job still opens and closes so the bar finishes).
pub fn voice_is_captions(voice: &str) -> bool {
    voice == CAPTIONS
}

/// §5 (`the run bar is the preview's transport only once the preview started`): ▶ belongs to the step until
/// the preview has actually started, and stays its transport until ⏹. Not started and not playing → the step;
/// playing → the transport; started and paused ([`Transport::cued`]) → still the transport (⏸ must resume,
/// ⏹ end). A clip merely cued by clicking a line is not playback (prototype `pageTransport`). Which *pages*
/// have a transport is [`crate::run::transport_for`]'s.
pub fn bar_serves_the_preview(transport: Transport) -> bool {
    transport.started
}

/// §5 (`cache keys stable, nothing deletes old wavs`): would removing this file throw away a line somebody
/// heard? A take is never cleaned up (the key is frozen, derived from what was spoken). Only the built voice
/// reference and its base ([`crate::narrate_data::drop_reference`]) are removed.
pub fn deletes_a_spoken_take(removed: &std::path::Path) -> bool {
    removed
        .ancestors()
        .any(|dir| dir.file_name() == Some(std::ffi::OsStr::new("tts")))
}

/// §5 (`hand-picked takes never re-ranked`): did somebody choose the seconds to clone from? Chosen takes win
/// outright (F4.6's `hand-picked takes?` yes-branch); [`crate::narrate_data::clean_takes`] sorts and merges but
/// never ranks, its one floor being P.eng.takeMinSeconds ([`crate::narrate_screen::TAKE_MIN_SECONDS`]).
pub fn takes_are_hand_picked(takes: &[Take]) -> bool {
    !takes.is_empty()
}

/// §5 (`a voice not on offer leaves the picker empty`): which narrator slot does this id name? `own` is slot 1
/// and `narrator2`..`narratorN` are 2..=N; anything else, including a number past [`NARRATOR_SLOTS`], names none.
pub fn narrator_slot(id: &str) -> Option<usize> {
    if id == OWN_VOICE {
        return Some(1);
    }
    let number = id.strip_prefix("narrator")?.parse::<usize>().ok()?;
    (1..=NARRATOR_SLOTS).contains(&number).then_some(number)
}

/// The row the dropdown should show for the stored voice, or `None` when nothing on offer is that voice; the
/// page then shows **no** row rather than the first, which would silently pick the wrong speaker.
///
/// Narrator 1's row is `own` (the frozen id); a `narrator1` in `voice.txt` names the same slot. A file's name
/// matches its own row exactly.
pub fn picker_row(options: &[VoiceOption], stored: &str) -> Option<usize> {
    let wanted = if narrator_slot(stored) == Some(1) {
        OWN_VOICE
    } else {
        stored
    };
    options.iter().position(|option| option.id == wanted)
}

/// Which of the two sentences to show when [`picker_row`] found nothing: a narrator slot is tagged on Prepare,
/// a file lives in the voices folder. EM DASH (U+2014) as §F4.6 S1 writes both, the id quoted as the prototype's `%q`.
pub fn missing_voice(stored: &str, voices_dir: &str) -> String {
    if let Some(slot) = narrator_slot(stored) {
        return format!(
            "narrator {slot} is not tagged on the Prepare step \u{2014} tag a recording, or pick another voice"
        );
    }
    format!("voice \"{stored}\" is no longer in {voices_dir} \u{2014} pick another")
}
