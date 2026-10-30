//! The diarization window ladder, walked (spec/02-services.md §2's diarization row: "speaker turns per
//! window (90 → 45 → 25 s ladder)"; the reason a walk is needed at all is [`Scan::AnchorTooLong`]'s own
//! note in [`crate::diarize`] — "so the ladder moves on or the step fails").
//!
//! Nothing here does arithmetic on seconds: window spans, the anchor and the pass-2 spans are
//! [`crate::diarize`]'s, and this module only picks rungs, asks about them, and says which one served.
//! That split is what lets a test drive the whole walk with a fake that never dials a socket, the way
//! [`crate::narrate_tts::speak_line`] takes its two network legs as closures.
//!
//! The rung that plans is not the end of the scan: pass 2 asks about every span `plan` left to place,
//! with the anchor in front of what it hears, and the slots of that answer are named against the anchor
//! here rather than by the caller — so a rung is pass 1's windows *and* pass 2's, and a log that only
//! counted the first would read as half a scan.

use crate::diarize::{self, Anchor, Scan};
use crate::requests::Turn;
use crate::transcribe::SAMPLE_RATE;

/// One rung's outcome, as the walk reports it.
#[derive(Debug, Clone, PartialEq)]
pub struct Rung {
    /// The window size this rung asked about, in seconds.
    pub window: f64,
    /// Pass 1's windows at this size — the caller's cut list, returned so a log line can count them.
    pub scan: Vec<(f64, f64)>,
    /// The turns the rung answered, flattened across its windows. Pass 2's answers arrive here too, each
    /// slot renamed to the voice the anchor says it is, which is the whole point of the second pass.
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
///
/// `log` receives §6's lines as they become true — pass 1's `finding voices i/n`, the anchor line, pass 2's
/// `placing speakers i/n`, and `no voice long enough to anchor` when that is the answer. A rung that is
/// refused costs no line of its own: it is a rung the run never used.
pub fn run<F, G, L>(
    duration: f64,
    model: &str,
    base: &str,
    mut scan: F,
    mut place: G,
    mut log: L,
) -> Result<Rung, String>
where
    F: FnMut(&[(f64, f64)]) -> Result<Vec<Vec<Turn>>, String>,
    G: FnMut((f64, f64), &Anchor) -> Result<Vec<Turn>, String>,
    L: FnMut(&str),
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
        // Pass 1 asks about the rung's whole window list and says `finding voices i/n` once per window,
        // so a run that stops mid-scan can be read as far into the recording as it got.
        for (index, _) in windows.iter().enumerate() {
            log(&diarize::scan_log(base, index, windows.len()));
        }
        let heard = scan(&windows)?;
        match diarize::plan(duration, *window, &windows, &heard) {
            Scan::Planned {
                anchor,
                place: spans,
                ..
            } => {
                log(&diarize::anchor_log(
                    base,
                    &anchor,
                    *window,
                    diarize::anchor_per(*window),
                ));
                // The serving rung's own answers: pass 1's flattened, then pass 2's with every slot
                // renamed against the anchor. Asking pass 1 again would be a second set of requests for
                // one result.
                let mut turns: Vec<Turn> = heard.into_iter().flatten().collect();
                turns.extend(placed(&spans, &anchor, base, &mut place, &mut log)?);
                return Ok(Rung {
                    window: *window,
                    scan: windows,
                    turns,
                });
            }
            Scan::NoVoice => {
                log(&diarize::no_voice_log(base));
                // An empty answer is an answer: §6 wants the line, and `turns.json` written from what was
                // heard, which is `[]` — the file is the resume marker a later run reads as "nobody spoke".
                return Ok(Rung {
                    window: *window,
                    scan: windows,
                    turns: heard.into_iter().flatten().collect(),
                });
            }
            // Too much anchor for this window: try the next rung down before believing it.
            Scan::AnchorTooLong {
                anchor,
                window: rung,
            } => last = Some((anchor, rung)),
        }
    }
    let (anchor, window) = last.expect("the ladder has rungs");
    Err(format!(
        "{model}: {}",
        diarize::anchor_too_long(anchor, window)
    ))
}

/// Pass 2, one request per span `plan` left to place, each answered with its slots named after the anchor's
/// voices. Every span is asked — a second placed twice would come back with two ids to reconcile for nothing,
/// and a second never placed would silently belong to nobody.
///
/// The turn edges come back in *answer* seconds: `within` says how far into the audio after the anchor a
/// sample falls, and an anchor whose first cut starts away from the recording's zero would otherwise have
/// its whole file counted twice over. The id is the answer's own slot number, so a slot the anchor reaches
/// keeps that voice's slot and one it never heard earns `slot_base + slot`.
fn placed<F, L>(
    spans: &[(f64, f64)],
    anchor: &Anchor,
    base: &str,
    mut ask: F,
    mut log: L,
) -> Result<Vec<Turn>, String>
where
    F: FnMut((f64, f64), &Anchor) -> Result<Vec<Turn>, String>,
    L: FnMut(&str),
{
    // Pass 1 heard the anchor's window, so its slot numbers are the anchor's own; a stranger in a later
    // answer must not collide with them, which is why fresh ids start above the highest one.
    let slot_base = anchor
        .slots
        .iter()
        .map(|slot| slot.slot + 1)
        .max()
        .unwrap_or_default();
    let mut turns = Vec::new();
    for (index, span) in spans.iter().enumerate() {
        log(&diarize::placement_log(base, index, spans.len()));
        let heard = ask(*span, anchor)?;
        // One claim per slot, in anchor seconds: every turn that slot owns, its whole reach measured from
        // the start of the anchor file. `match_slots` then decides one-to-one, strongest claim first.
        let claims = slot_claims(&heard, anchor);
        let named: Vec<(u32, u32)> = diarize::match_slots(&claims, anchor)
            .into_iter()
            .map(|(slot, block)| {
                (
                    slot,
                    block.map_or(slot_base + slot, |block| anchor.slots[block].slot),
                )
            })
            .collect();
        // The answer's zero is the anchor file's start: what lies inside the anchor is the reference, heard and
        // placed by pass 1 already, and what lies past it is this span, `width` seconds later on the file than on
        // the recording. The first rewrite added the anchor's start and width to every turn instead, which puts
        // a span anywhere but right after the anchor on the wrong seconds, and kept the reference's turns too.
        let width = (anchor.seconds() * SAMPLE_RATE as f64) as u64;
        let span_start = (span.0 * SAMPLE_RATE as f64) as u64;
        for turn in heard {
            if turn.end_sample <= width {
                continue;
            }
            let slot = named
                .iter()
                .find(|(slot, _)| *slot == turn.speaker_id)
                .map_or(slot_base + turn.speaker_id, |(_, voice)| *voice);
            turns.push(Turn {
                start_sample: span_start + turn.start_sample.max(width) - width,
                end_sample: span_start + turn.end_sample - width,
                speaker_id: slot,
            });
        }
    }
    Ok(turns)
}

/// What pass 2's answer is measured against: the anchor file's own seconds. The caller cut
/// `window − anchor − slack` of new audio and put the anchor in front of it, so a turn's samples are read
/// from the front of that file, and a claim may not reach past the reference it was matched on — which is
/// exactly the bound `match_slots` compares at `MIN_ANCHOR_OVERLAP`.
///
/// A slot heard in several stretches is one voice, so its claim spans all of them: `match_slots` measures
/// the whole overlap and takes the strongest claim first.
fn slot_claims(heard: &[Turn], anchor: &Anchor) -> Vec<(u32, f64, f64)> {
    let width = anchor.seconds();
    let mut claims: Vec<(u32, f64, f64)> = Vec::new();
    for turn in heard {
        // Only what the slot said INSIDE the anchor is a claim on it. A turn past the anchor, clamped to its
        // end, stretched the claim over every later block: the first rewrite let a voice heard again in the
        // new audio claim the next voice's block, and that voice came back as a stranger.
        if turn.start_sample as f64 / SAMPLE_RATE as f64 >= width {
            continue;
        }
        let from = (turn.start_sample as f64 / SAMPLE_RATE as f64).min(width);
        let to = (turn.end_sample as f64 / SAMPLE_RATE as f64)
            .min(width)
            .max(from);
        match claims
            .iter_mut()
            .find(|(slot, _, _)| *slot == turn.speaker_id)
        {
            Some((_, low, high)) => {
                *low = low.min(from);
                *high = high.max(to);
            }
            None => claims.push((turn.speaker_id, from, to)),
        }
    }
    claims
}
