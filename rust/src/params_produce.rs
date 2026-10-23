//! §08-produce#4 §4's engineering sentence, spelled for the catalogue.
//!
//! §4 ends with "Engineering: input ordering, faststart flags, sd.cpp timeouts, poster quality, export ladder."
//! Those five values have no `P.` id in §10's tables — they sit in its prose catch-all line ("sd.cpp timeouts
//! 15/60/30/1/10 s") — so [`crate::params::produce`] catalogues them under the bare `produce.` prefix AGENT.md
//! gives a value §10 does not name, and this module supplies the spelling each row shows.
//!
//! It lives in its own file because `params.rs` is at its size budget; nothing here owns a number. Every function
//! reads the constant or the function the rule actually uses, so a change to the rule moves the row by itself and
//! a row that stops matching its home fails `tests/suite/produce_parameters.rs`.
//!
//! No GTK, and nothing computed: like the rest of `params`, this only spells what lives elsewhere.

use crate::bodies::{sd_waits, SdWait};
use crate::produce_embed::{POSTER_JPEG_QUALITY, POSTER_QSCALE};
use crate::produce_render::{INPUT_FLAG, MP4_START_FLAGS, MP4_STREAM_FLAGS, SEEK_FLAG};
use crate::produce_runs::{JPEG_QUALITIES, JPEG_RETRY_LIMIT};
use crate::services::Kind;

/// §4's "export ladder": the rungs an over-large thumbnail is re-encoded through, best first, read from
/// [`JPEG_QUALITIES`] rather than retyped. The rungs whose failure the export reports are marked, because
/// [`crate::produce_runs::jpeg_target`] names exactly those when it refuses.
pub fn export_ladder() -> String {
    let tried = JPEG_RETRY_LIMIT;
    JPEG_QUALITIES
        .iter()
        .enumerate()
        .map(|(index, rung)| {
            if index < tried {
                rung.to_string()
            } else {
                format!("{rung}*")
            }
        })
        .collect::<Vec<String>>()
        .join("/")
        + &format!(" (* = beyond the {tried} reported)")
}

/// §4's "poster quality": the JPEG requirement §F5.5 states and the `-q:v` rung `tool:ffmpeg.encode` is sent
/// to meet it, both from [`POSTER_JPEG_QUALITY`] and [`POSTER_QSCALE`] — one requirement at its two levels.
pub fn poster_quality() -> String {
    format!("{POSTER_JPEG_QUALITY} (-q:v {POSTER_QSCALE})")
}

/// §4's "sd.cpp timeouts", spelled out of [`sd_waits`] itself: every wait is asked of the function the drawing
/// legs call, so the row cannot quote a deadline the code no longer sets. The poll shows its interval too, since
/// that gap is as much §5's number as the timeout is.
pub fn sd_timeouts() -> String {
    // Named in the order §10 writes them: capabilities, submit, poll (with its gap), cancel.
    let waits: [(Kind, &str); 4] = [
        (Kind::Capabilities, "capabilities"),
        (Kind::SubmitImage, "submit"),
        (Kind::PollImage, "poll"),
        (Kind::CancelImage, "cancel"),
    ];
    waits
        .into_iter()
        .filter_map(|(kind, label)| sd_waits(kind).map(|wait| spell_wait(label, &wait)))
        .collect::<Vec<String>>()
        .join(", ")
}

/// One wait as `label Ns`, plus ` every Ms` where the kind polls.
fn spell_wait(label: &str, wait: &SdWait) -> String {
    match wait.interval {
        Some(gap) => format!(
            "{label} {}s every {}s",
            wait.timeout.as_secs(),
            gap.as_secs()
        ),
        None => format!("{label} {}s", wait.timeout.as_secs()),
    }
}

/// §4's "input ordering": the encoder puts the picture's seek before its input, so a clip starting forty
/// minutes in does not decode forty minutes first. Spelled from the two flags [`crate::produce_render::encode_command`]
/// pushes, which means renaming either flag shows up in the row instead of hiding it.
pub fn seek_rule() -> String {
    format!("{SEEK_FLAG} before {INPUT_FLAG}")
}

/// §4's "faststart flags": the mp4-only tail of the mux, taken from the two constants `mux_command` appends.
/// mkv and webm carry neither, which is why the row says which container they belong to.
pub fn mp4_flags() -> String {
    MP4_STREAM_FLAGS
        .iter()
        .chain(MP4_START_FLAGS.iter())
        .copied()
        .collect::<Vec<&str>>()
        .join(" ")
}
