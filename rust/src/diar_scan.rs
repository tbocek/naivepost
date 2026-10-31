//! The diarization window ladder, walked (spec/02-services.md §2's diarization row: "speaker turns per window (90 → 45 → 25 s ladder)"); a walk is needed because of [`Scan::AnchorTooLong`] in [`crate::diarize`].
//!
//! Nothing here does arithmetic on seconds: window spans, the anchor and pass-2 spans are [`crate::diarize`]'s; this module picks rungs, asks about them, and says which one served,
//! so a test can drive the walk with a fake that dials no socket (as [`crate::narrate_tts::speak_line`]).
//!
//! A rung is pass 1's windows *and* pass 2's: pass 2 asks about every span `plan` left to place, with the anchor in front, and names the answer's slots against the anchor here.

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
    /// The turns the rung answered, flattened across its windows; pass 2's slots are renamed to the voice the anchor says they are.
    pub turns: Vec<Turn>,
}

/// Walk [`diarize::ladder()`] biggest-first and return the first rung that answers.
///
/// A rung whose plan is [`Scan::AnchorTooLong`] moves on to the next one down. Every other answer serves the run, [`Scan::NoVoice`] included: re-asking at a smaller window would only ask again for less.
///
/// Every rung refusing means the model is asked for more reference than any window can hold; the `Err` names the model and both numbers of the last refusal (§Failure-is-specific).
///
/// `log` receives §6's lines as they become true: pass 1's `finding voices i/n`, the anchor line, pass 2's `placing speakers i/n`, and `no voice long enough to anchor`. A refused rung costs no line.
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
            // Nothing to ask about at this size or smaller (a recording under a second has no window): saying so beats an empty turn list that reads as "nobody spoke".
            return Err(format!(
                "{model}: {duration:.1} s of audio is too short for a diarization window"
            ));
        }
        // Pass 1 says `finding voices i/n` once per window, so a stopped run can be read as far as it got.
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
                // The serving rung's answers: pass 1's flattened, then pass 2's with every slot renamed against the anchor; asking pass 1 again would be a second set of requests.
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
                // An empty answer is an answer: §6 wants the line, and `turns.json` written as `[]`, the resume marker a later run reads as "nobody spoke".
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

/// Pass 2, one request per span `plan` left to place, each answered with its slots named after the anchor's voices. Every span is asked: placed twice means ids to reconcile, never placed means a second that belongs to nobody.
///
/// Turn edges come back in *answer* seconds (`within` is how far into the audio after the anchor a sample falls). The id is the answer's slot number, so a slot the anchor reaches keeps that voice's
/// slot and one it never heard earns `slot_base + slot`.
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
    // Pass 1 heard the anchor's window, so its slot numbers are the anchor's; fresh ids start above the highest to avoid collisions.
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
        // One claim per slot, in anchor seconds, measured from the start of the anchor file; `match_slots` then decides one-to-one, strongest claim first.
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

/// What pass 2's answer is measured against: the anchor file's own seconds (the caller put the anchor in front of `window − anchor − slack` of new audio), so a claim may not reach past the reference it was matched on
/// (the bound `match_slots` compares at `MIN_ANCHOR_OVERLAP`). A slot heard in several stretches is one voice, so its claim spans all of them.
fn slot_claims(heard: &[Turn], anchor: &Anchor) -> Vec<(u32, f64, f64)> {
    let width = anchor.seconds();
    let mut claims: Vec<(u32, f64, f64)> = Vec::new();
    for turn in heard {
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
