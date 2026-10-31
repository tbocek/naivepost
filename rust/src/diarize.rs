//! Diarization's two-pass anchored scan (spec/04-prepare.md F1.3 S5, confirmed by §6).

use crate::requests::Turn;
use crate::roles::{ANCHOR_PER_SECONDS, DIAR_HOP_SHARE, DIAR_WINDOWS_SECONDS};
use crate::transcribe::SAMPLE_RATE;

pub const ANCHOR_MIN_SECONDS: f64 = 4.0;

pub const ANCHOR_CUT_SECONDS: f64 = 0.3;

/// Anchor-block overlap that claims a slot. Below it two voices merely happen to be near each other.
pub const MIN_ANCHOR_OVERLAP: f64 = 0.5;

pub const WINDOW_MIN_SECONDS: f64 = 10.0;

pub const WINDOW_SLACK_SECONDS: f64 = 1.0;

pub const NEW_AUDIO_MIN_SECONDS: f64 = 15.0;

/// A voice's slot in the anchor, and where its piece of the anchor file lands.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Slot {
    /// The server's own id inside the window it was heard in — never a session-wide speaker.
    pub slot: u32,
    /// Seconds of this voice in the anchor window.
    pub speech: f64,
    /// Where its cut starts on the recording's clock.
    pub from: f64,
    pub to: f64,
    /// Its offset inside the anchor file, which is what a pass-2 overlap is measured against.
    pub anchor_from: f64,
    pub anchor_to: f64,
}

/// The anchor pass 1 chose and built.
#[derive(Debug, Clone, PartialEq)]
pub struct Anchor {
    /// Index into the pass-1 windows of the one that carried the most voices.
    pub window: usize,
    /// Where that window sits on the recording's clock.
    pub from: f64,
    pub to: f64,
    /// One entry per voice, longest speech first — the order their cuts are concatenated in.
    pub slots: Vec<Slot>,
}

impl Anchor {
    /// Seconds of anchor audio: what pass 2 prepends, and what a window loses from its own length.
    pub fn seconds(&self) -> f64 {
        self.slots.iter().map(|slot| slot.to - slot.from).sum()
    }
}

/// What the scan decided, or why it stopped.
#[derive(Debug, Clone, PartialEq)]
pub enum Scan {
    /// Nothing in any window was a voice long enough to anchor on: no turns, and that is the answer.
    NoVoice,
    AnchorTooLong {
        anchor: f64,
        window: f64,
    },
    /// The windows pass 1 read, the anchor it built, and the spans pass 2 is sent.
    Planned {
        scan: Vec<(f64, f64)>,
        anchor: Anchor,
        place: Vec<(f64, f64)>,
    },
}

pub fn scan_windows(duration: f64, window: f64) -> Vec<(f64, f64)> {
    let window = window.max(WINDOW_MIN_SECONDS);
    let hop = (window * DIAR_HOP_SHARE.0 as f64 / DIAR_HOP_SHARE.1 as f64).max(1.0);
    let mut windows = Vec::new();
    let mut from = 0.0;
    while from < duration {
        let to = (from + window).min(duration);
        // A window shorter than a second of audio holds no sentence worth asking about. The last window
        // ends at the end of the recording rather than being padded to the full width — padding would send
        // the server silence and call it a window — so it is usually the shortest one there is.
        if to - from < 1.0 {
            break;
        }
        windows.push((from, to));
        // A window that already reaches the end of the recording is the last one: another would only be
        // opened for seconds the one just pushed covered anyway, and every window is counted in
        // "finding voices i/n" as if the server had been sent it.
        if duration - to < 1.0 {
            break;
        }
        from += hop;
    }
    windows
}

pub fn anchor_per(window: f64) -> f64 {
    ANCHOR_PER_SECONDS.min(window / 4.0)
}

pub fn voices(window_turns: &[Turn]) -> usize {
    let mut voices: Vec<(u32, f64)> = Vec::new();
    for turn in window_turns {
        let seconds = speech(turn);
        if seconds < ANCHOR_MIN_SECONDS {
            continue;
        }
        match voices.iter_mut().find(|(slot, _)| *slot == turn.speaker_id) {
            Some((_, held)) => *held += seconds,
            None => voices.push((turn.speaker_id, seconds)),
        }
    }
    voices.len()
}

pub fn pick_window(windows: &[&[Turn]]) -> Option<usize> {
    let mut best: Option<(usize, usize, f64)> = None;
    for (index, turns) in windows.iter().enumerate() {
        let mut heard: Vec<(u32, f64)> = Vec::new();
        for turn in *turns {
            let seconds = speech(turn);
            if seconds < ANCHOR_MIN_SECONDS {
                continue;
            }
            match heard.iter_mut().find(|(slot, _)| *slot == turn.speaker_id) {
                Some((_, held)) => *held += seconds,
                None => heard.push((turn.speaker_id, seconds)),
            }
        }
        let held = heard.len();
        if held == 0 {
            continue;
        }
        let quietest = heard
            .iter()
            .map(|(_, seconds)| *seconds)
            .fold(f64::INFINITY, f64::min);
        // More voices wins; then the window whose least-represented voice has more of it.
        if best.is_none_or(|(_, best_held, best_quiet)| {
            held > best_held || (held == best_held && quietest > best_quiet)
        }) {
            best = Some((index, held, quietest));
        }
    }
    best.map(|(index, _, _)| index)
}

pub fn plan(duration: f64, window: f64, windows: &[(f64, f64)], per_window: &[Vec<Turn>]) -> Scan {
    let borrowed: Vec<&[Turn]> = per_window.iter().map(Vec::as_slice).collect();
    let Some(chosen) = pick_window(&borrowed) else {
        return Scan::NoVoice;
    };
    let (from, to) = windows[chosen];
    let per = anchor_per(to - from);

    // Each voice's speech in this window: its total, and the turns it was heard in, longest first, so a
    // flicker never stands in for a long stretch when the cut has room for the long one.
    struct Voice {
        slot: u32,
        speech: f64,
        cuts: Vec<(f64, f64)>,
    }
    let mut voices: Vec<Voice> = Vec::new();
    for turn in &per_window[chosen] {
        let start = (turn.start_sample as f64 / SAMPLE_RATE as f64).clamp(from, to);
        let end = (turn.end_sample as f64 / SAMPLE_RATE as f64).clamp(from, to);
        // Speech *inside this window* is what counts: a turn reaching past the window's end says nothing
        // about audio the server was not sent, and counting it would both inflate the voice's weight and
        // let the cut run past the request.
        let seconds = (end - start).max(0.0);
        if seconds <= 0.0 {
            continue;
        }
        match voices
            .iter_mut()
            .find(|voice| voice.slot == turn.speaker_id)
        {
            Some(voice) => {
                voice.speech += seconds;
                if end - start >= ANCHOR_CUT_SECONDS {
                    voice.cuts.push((start, end));
                }
            }
            None => voices.push(Voice {
                slot: turn.speaker_id,
                speech: seconds,
                cuts: if end - start >= ANCHOR_CUT_SECONDS {
                    vec![(start, end)]
                } else {
                    Vec::new()
                },
            }),
        }
    }
    // Only a voice with ANCHOR_MIN_SECONDS of speech is one; most speech first, which is the order
    // anchor.list is written in and so has to be decided, not whatever the map yielded.
    voices.retain(|voice| voice.speech >= ANCHOR_MIN_SECONDS);
    if voices.is_empty() {
        return Scan::NoVoice;
    }
    voices.sort_by(|a, b| {
        b.speech
            .partial_cmp(&a.speech)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut slots = Vec::new();
    let mut at = 0.0;
    for voice in voices {
        let mut cuts = voice.cuts;
        cuts.sort_by(|a, b| {
            (b.1 - b.0)
                .partial_cmp(&(a.1 - a.0))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let mut taken = 0.0;
        // Where the anchor's piece for this voice starts on the recording's clock. `to` is that start
        // plus what was cut, not the end of the last turn it was cut from — those two differ as soon as
        // a voice is heard in more than one stretch, and reading the field as an end would put audio in
        // the anchor that was never cut.
        let mut start = 0.0;
        let mut opened = false;
        for (cut_from, cut_to) in cuts {
            if taken >= per {
                break;
            }
            let keep = (per - taken).min(cut_to - cut_from);
            if keep < ANCHOR_CUT_SECONDS {
                continue; // too short to identify anything, and it would only add a seam
            }
            if !opened {
                start = cut_from;
                opened = true;
            }
            taken += keep;
        }
        if !opened {
            continue;
        }
        slots.push(Slot {
            slot: voice.slot,
            speech: voice.speech,
            from: start,
            to: start + taken,
            anchor_from: at,
            anchor_to: at + taken,
        });
        at += taken;
    }
    if slots.is_empty() {
        return Scan::NoVoice;
    }

    let anchor = Anchor {
        window: chosen,
        from,
        to,
        slots,
    };
    let span = (window - anchor.seconds() - WINDOW_SLACK_SECONDS).max(0.0);
    if span < NEW_AUDIO_MIN_SECONDS {
        return Scan::AnchorTooLong {
            anchor: anchor.seconds(),
            window,
        };
    }
    // Pass 2 strides the whole recording without overlap: each second is placed once, and a second
    // heard twice would come back with two ids to reconcile for nothing. A stretch under a second of new
    // audio at the end is not a request; it rides on the window before it.
    let mut place = Vec::new();
    let mut from = 0.0;
    while from < duration {
        let to = (from + span).min(duration);
        if to - from < 1.0 {
            break;
        }
        place.push((from, to));
        from += span;
    }
    Scan::Planned {
        scan: windows.to_vec(),
        anchor,
        place,
    }
}

pub fn anchor_log(base: &str, anchor: &Anchor, window: f64, per_window: f64) -> String {
    format!(
        ">>> [{base}] anchor: {} voice(s) in {} s of a {} s window -- {} s of new audio each",
        anchor.slots.len(),
        seconds(anchor.seconds()),
        seconds(window),
        seconds(per_window),
    )
}

pub fn scan_log(base: &str, index: usize, total: usize) -> String {
    format!(">>> [{base}] finding voices {}/{}", index + 1, total)
}

pub fn placement_log(base: &str, index: usize, total: usize) -> String {
    format!(">>> [{base}] placing speakers {}/{}", index + 1, total)
}

pub fn no_voice_log(base: &str) -> String {
    format!(">>> [{base}] diarization: no voice long enough to anchor -- no turns")
}

/// `anchor too long (X s of Y s window)` — the anchor left this little new audio per window.
pub fn anchor_too_long(anchor: f64, window: f64) -> String {
    format!(
        "anchor too long ({} s of {} s window)",
        seconds(anchor),
        seconds(window)
    )
}

/// One pass-2 request's span and where it sits on the recording's clock.
pub struct Placement {
    pub from: f64,
    pub to: f64,
}

impl Placement {
    /// Seconds into the window — hence into the audio after the anchor — a turn's start answers about.
    pub fn within(&self, start_sample: u64) -> f64 {
        start_sample as f64 / SAMPLE_RATE as f64 - self.from
    }
}

pub fn match_slots(claims: &[(u32, f64, f64)], anchor: &Anchor) -> Vec<(u32, Option<usize>)> {
    // Strongest claim first, over all blocks: a tie keeps the order the answer came in, so the result
    // does not depend on how the sort broke it. The owner is recorded per slot, not per rank — see the
    // `owner` writes below, which is what lets the answer be rewritten in its own order afterwards.
    let mut order: Vec<usize> = (0..claims.len()).collect();
    order.sort_by(|a, b| {
        strength(claims[*b], anchor)
            .partial_cmp(&strength(claims[*a], anchor))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Slot id -> the block it was given. Keyed by slot because a pass-2 answer may name the same slot in
    // more than one turn, and all of its turns are the same voice; keyed by claim as well, since two
    // slots' claims compete for the same block.
    let mut winners: Vec<(u32, usize)> = Vec::new();
    let mut taken: Vec<bool> = vec![false; anchor.slots.len()];
    for index in order {
        let (slot, _, _) = claims[index];
        // The block this claim overlaps most, provided it clears MIN_ANCHOR_OVERLAP and nobody else has
        // been given it. A claim overlapping nothing that far is a voice the anchor never held.
        let best = anchor
            .slots
            .iter()
            .enumerate()
            .filter(|(block, _)| !taken[*block])
            .map(|(block, held)| (block, overlap(claims[index], held)))
            .filter(|(_, seconds)| *seconds >= MIN_ANCHOR_OVERLAP)
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        if let Some((block, _)) = best {
            taken[block] = true;
            winners.push((slot, block));
        }
    }
    let owners: Vec<(u32, Option<usize>)> = claims
        .iter()
        .map(|(slot, from, to)| {
            let claim = (*slot, *from, *to);
            // Which block this slot matches best, win or lose the contest for it.
            let matched = anchor
                .slots
                .iter()
                .enumerate()
                .max_by(|(_, a), (_, b)| {
                    overlap(claim, a)
                        .partial_cmp(&overlap(claim, b))
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .filter(|(_, held)| overlap(claim, held) >= MIN_ANCHOR_OVERLAP)
                .map(|(block, _)| block);
            // A loser is only named after a block that was actually taken; otherwise it is nobody's.
            let named = matched.filter(|block| {
                winners.iter().any(|(_, won)| won == block)
                    || winners.iter().any(|(id, _)| id == slot)
            });
            (*slot, named)
        })
        .collect();
    // Answered in the order the answer came in, not in the order the claims were settled: the caller
    // rewrites its own turns by position.
    owners
}

/// Seconds a claim reaches into one anchor block: the length of their intersection, never negative.
fn overlap(claim: (u32, f64, f64), block: &Slot) -> f64 {
    (claim.2.min(block.anchor_to) - claim.1.max(block.anchor_from)).max(0.0)
}

fn strength(claim: (u32, f64, f64), anchor: &Anchor) -> f64 {
    anchor.slots.iter().map(|block| overlap(claim, block)).sum()
}

fn speech(turn: &Turn) -> f64 {
    let start = turn.start_sample as f64 / SAMPLE_RATE as f64;
    let end = turn.end_sample as f64 / SAMPLE_RATE as f64;
    (end - start).max(0.0)
}

/// Seconds, spelled the way Prepare logs them: whole numbers lose the `.0`.
fn seconds(value: f64) -> String {
    if value == value.trunc() {
        return format!("{}", value as i64);
    }
    let mut text = format!("{value:.2}");
    while text.ends_with('0') && !text.ends_with(".0") {
        text.pop();
    }
    text.trim_end_matches('.').to_string()
}

pub fn ladder() -> &'static [f64] {
    &DIAR_WINDOWS_SECONDS
}
