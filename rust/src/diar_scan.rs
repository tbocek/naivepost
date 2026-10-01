//! The diarization window ladder, walked (spec/02-services.md §2's diarization row: "speaker turns per
//! window (90 → 45 → 25 s ladder)"; the reason a walk is needed at all is [`Scan::AnchorTooLong`]'s own
//! note in [`crate::diarize`] — "so the ladder moves on or the step fails").
//!
//! Nothing here does arithmetic on seconds: window spans, the anchor and the pass-2 spans are
//! [`crate::diarize`]'s, and this module only picks rungs, asks about them, and says which one served.
//! That split is what lets a test drive the whole walk with a fake that never dials a socket, the way
//! [`crate::narrate_tts::speak_line`] takes its two network legs as closures.

use crate::diarize::{self, Scan};
use crate::requests::Turn;

/// What one rung of the ladder needs from the caller, and what it gives back.
///
/// `windows` is pass 1's span list for this rung, already cut by [`diarize::scan_windows`]; the ask answers
/// each with the turns heard inside it, in order. It is `FnMut` because a rung is several requests, and
/// `FnOnce` would stop after the first.
pub type Ask<'a> = dyn FnMut(&[(f64, f64)]) -> Result<Vec<Vec<Turn>>, String> + 'a;
/// One rung's outcome, as the walk reports it.
#[derive(Debug, Clone, PartialEq)]
pub struct Rung {
    /// The window size this rung asked about, in seconds.
    pub window: f64,
    /// Pass 1's windows at this size — the caller's cut list, returned so a log line can count them.
    pub scan: Vec<(f64, f64)>,
    /// The turns the rung answered, flattened across its windows.
    pub turns: Vec<Turn>,
}

/// Walk [`diarize::ladder()`] biggest-first and return the first rung that answers.
///
/// A rung whose plan is [`Scan::AnchorTooLong`] moves on to the next one down — that variant's own note says
/// the ladder moves on or the step fails, and this is the moving-on. Every other answer serves the run as it
/// stands, [`Scan::NoVoice`] included: an empty answer is an answer about the audio this rung was sent, and
/// re-asking at a smaller window would only ask again for less.
///
/// Every rung refusing means the diarization model is being asked for more reference than any window on the
/// ladder can hold, and the `Err` names the model and both numbers of the last refusal (§Failure-is-specific:
/// name the model and the reason where it failed).
pub fn run<F>(duration: f64, model: &str, mut ask: F) -> Result<Rung, String>
where
    F: FnMut(&[(f64, f64)]) -> Result<Vec<Vec<Turn>>, String>,
{
    let mut last: Option<(f64, f64)> = None;
    for window in diarize::ladder() {
        let windows = diarize::scan_windows(duration, *window);
        if windows.is_empty() {
            // Nothing to ask about at this size or any smaller one either: a recording under a second has
            // no window on the ladder, and saying so beats returning an empty turn list that reads as
            // "nobody spoke".
            return Err(format!(
                "{model}: {duration:.1} s of audio is too short for a diarization window"
            ));
        }
        let heard = ask(&windows)?;
        match diarize::plan(duration, *window, &windows, &heard) {
            Scan::Planned { .. } | Scan::NoVoice => {
                // The rung that planned is the rung whose answers serve, so its turns are returned as they
                // were heard — asking again would be a second set of requests for one result.
                return Ok(Rung {
                    window: *window,
                    scan: windows,
                    turns: heard.into_iter().flatten().collect(),
                });
            }
            // Too much anchor for this window: try the next rung down before believing it.
            Scan::AnchorTooLong { anchor, window } => last = Some((anchor, window)),
        }
    }
    let (anchor, window) = last.expect("the ladder has rungs");
    Err(format!(
        "{model}: {}",
        diarize::anchor_too_long(anchor, window)
    ))
}
