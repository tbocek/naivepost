//! Diarization's two-pass anchored scan (spec/04-prepare.md F1.3 S5, as §6 confirms it).
//!
//! §04-prepare#6-details-confirmed-against-the-code-verification-pass, first bullet: pass 1 strides
//! the recording looking for voices, one window becomes an anchor of a little of each voice, and pass
//! 2 asks about shorter windows with that anchor in front so its slots can be matched to known
//! voices. All of that is arithmetic on seconds, so [`naivepost::diarize`] answers it without a
//! server — which is what lets these checks hold the window ladder, the anchor's size and the matching
//! rule still true when nobody is listening at :8765.

use naivepost::diarize::{self, Anchor, Scan, Slot, ANCHOR_CUT_SECONDS, ANCHOR_MIN_SECONDS, MIN_ANCHOR_OVERLAP};
use naivepost::requests::Turn;
use naivepost::roles;
use naivepost::transcribe::SAMPLE_RATE;

/// A turn on the recording's clock, in seconds. The server speaks in samples at
/// [`SAMPLE_RATE`]; a test that thinks in seconds should not have to multiply everywhere.
fn turn(slot: u32, from: f64, to: f64) -> Turn {
    Turn {
        start_sample: (from * SAMPLE_RATE as f64) as u64,
        end_sample: (to * SAMPLE_RATE as f64) as u64,
        speaker_id: slot,
    }
}

/// A voice speaking `seconds` of sound every `every` seconds from `from`.
fn speaks(slot: u32, from: f64, every: f64, seconds: f64, times: usize) -> Vec<Turn> {
    (0..times)
        .map(|at| turn(slot, from + at as f64 * every, from + at as f64 * every + seconds))
        .collect()
}

fn extend(turns: &mut Vec<Turn>, more: &[Turn]) {
    turns.extend_from_slice(more);
}

// --- S1: pass 1 strides; the window never shrinks below 10 s --------------------------------------

#[test]
fn sec_04_prepare_6_details_confirmed_against_the_code_verification_pass_s1_pass_one_strides_two_thirds_of_the_window() {
    // P.machine.diarHopShare (2/3), P.machine.diarWindowsSeconds (90, 45, 25)
    let windows = diarize::scan_windows(300.0, 90.0);
    assert_eq!(windows.len(), 5, "hop 60 over 300 s: 0, 60, 120, 180, 240 — {windows:?}");
    assert_eq!(windows[0], (0.0, 90.0));
    assert_eq!(windows[1], (60.0, 150.0), "each voice need only be seen once, so the windows overlap by a third");
    assert_eq!(windows[4], (240.0, 300.0), "the last window stops at the end of the recording rather than running past it");

    // The stride is a share of whichever rung is being used, not a fixed number of seconds.
    let smaller = diarize::scan_windows(150.0, 45.0);
    assert_eq!(smaller[1], (30.0, 75.0), "P.machine.diarHopShare at the 45 s rung is a 30 s hop");

    // A recording shorter than one window still gets exactly one, from zero: there is nothing to
    // stride past, and an empty pass 1 would read as "no voices" rather than "nothing to scan".
    assert_eq!(diarize::scan_windows(40.0, 90.0), vec![(0.0, 40.0)]);
    assert_eq!(diarize::scan_windows(0.0, 90.0), Vec::<(f64, f64)>::new());
}

#[test]
fn sec_04_prepare_6_details_confirmed_against_the_code_verification_pass_s1_a_window_is_never_below_ten_seconds() {
    // The floor the spec gives pass 1: hop ⅔ window, min 10 s. Below it a window holds too little of
    // a sentence for a server to separate two voices in it at all.
    assert_eq!(diarize::WINDOW_MIN_SECONDS, 10.0);
    // P.machine.diarHopShare at the floor is a hop of 6⅔ s, so the windows overlap by a third. Only a
    // window opened from scratch takes the floor; the last one is what is left of the recording, and a
    // tail shorter than a second is no window at all.
    let scanned = diarize::scan_windows(24.0, 6.0);
    assert_eq!(scanned.len(), 3, "{scanned:?}");
    assert_eq!(scanned[0], (0.0, 10.0), "a 6 s window is widened to the floor, not used as asked");
    let hop = scanned[1].0 - scanned[0].0;
    assert!((hop - 20.0 / 3.0).abs() < 1e-9, "the floor's own hop: {hop}");
    assert!(
        scanned[..scanned.len() - 1].iter().all(|(from, to)| to - from >= diarize::WINDOW_MIN_SECONDS - 1e-9),
        "{scanned:?} every full window reaches the floor"
    );

    // A tail of the recording under a second long is no window: it holds nothing the one before it did
    // not already hold, and it would still be counted in "finding voices i/n". The window that reaches
    // the end is therefore usually the shortest one, and always the last.
    assert_eq!(diarize::scan_windows(90.4, 90.0), vec![(0.0, 90.0)]);
}

// --- S2: which window becomes the anchor ----------------------------------------------------------

#[test]
fn sec_04_prepare_6_details_confirmed_against_the_code_verification_pass_s2_the_anchor_window_holds_most_voices() {
    // A voice counts with P.eng.anchorMinSeconds of speech or more; a two-second flicker is somebody
    // else's word mislabelled, not a third person.
    assert_eq!(ANCHOR_MIN_SECONDS, 4.0);
    let mut first = speaks(0, 0.0, 5.0, 6.0, 3); // one voice, all of window 0
    extend(&mut first, &speaks(1, 1.0, 5.0, 2.0, 2)); // a flicker: not a voice

    let mut second = speaks(0, 90.0, 6.0, 8.0, 4);
    extend(&mut second, &speaks(1, 92.0, 6.0, 7.0, 4)); // two real voices in window 1

    let windows = [first.as_slice(), second.as_slice()];
    assert_eq!(diarize::voices(&first), 1, "the 2 s slot does not count as a voice");
    assert_eq!(diarize::voices(&second), 2);
    assert_eq!(diarize::pick_window(&windows), Some(1));

    // Nothing anywhere long enough to anchor on: no window, and the caller logs it.
    let flicker = speaks(0, 0.0, 5.0, 3.9, 4);
    assert_eq!(diarize::pick_window(&[flicker.as_slice()]), None);
    assert_eq!(diarize::pick_window(&[]), None);
}

#[test]
fn sec_04_prepare_6_details_confirmed_against_the_code_verification_pass_s2_a_tie_goes_to_the_best_represented_quietest_voice() {
    // Two windows, the same two voices in each. In the second one the *least*-represented voice has
    // more of it (8 s against 4 s), so that is the window with material left over once every voice has
    // taken its share of the anchor — which is what "best-represented quietest voice" buys.
    let mut thin = speaks(0, 0.0, 5.0, 20.0, 3); // voice 0: 60 s, voice 1: 4 s in one turn
    extend(&mut thin, &speaks(1, 2.0, 100.0, 4.0, 1));
    let mut broad = speaks(0, 90.0, 5.0, 20.0, 3); // same crowd; voice 1 heard for 8 s
    extend(&mut broad, &speaks(1, 92.0, 8.0, 4.0, 2));

    let windows = [thin.as_slice(), broad.as_slice()];
    assert_eq!(diarize::voices(&thin), 2);
    assert_eq!(diarize::voices(&broad), 2);
    assert_eq!(diarize::pick_window(&windows), Some(1), "ties go to the window whose best-represented voice is quietest");

    // Reversing which side of the pair holds the longer stretch reverses the choice: the rule reads the
    // windows, it does not prefer the first one.
    assert_eq!(diarize::pick_window(&[broad.as_slice(), thin.as_slice()]), Some(0));
}

// --- S3: how much of each voice the anchor holds --------------------------------------------------

#[test]
fn sec_04_prepare_6_details_confirmed_against_the_code_verification_pass_s3_twelve_seconds_each_at_the_biggest_window_a_quarter_below_it() {
    // P.machine.anchorPerSeconds is 12 at the 90 s rung; §10's own row says a quarter of the window at
    // the lower rungs, because 12 s of anchor in a 25 s window leaves nothing to place.
    assert_eq!(roles::ANCHOR_PER_SECONDS, 12.0);
    assert_eq!(diarize::anchor_per(90.0), 12.0);
    assert_eq!(diarize::anchor_per(45.0), 11.25);
    assert_eq!(diarize::anchor_per(25.0), 6.25, "a quarter of the window, not a fifteenth of the biggest one");

    let windows = vec![(0.0, 90.0)];
    // Voice 0 speaks in long turns; voice 1 only ever says four seconds at a time.
    let mut turns = speaks(0, 2.0, 20.0, 15.0, 4);
    extend(&mut turns, &speaks(1, 5.0, 12.0, 4.0, 6));
    let Scan::Planned { anchor, .. } = diarize::plan(90.0, 90.0, &windows, &[turns.clone()]) else {
        panic!("two voices in a 90 s window must produce an anchor");
    };
    assert_eq!(anchor.slots.len(), 2);
    // Most speech first: the order of anchor.list is what a reader checks by eye.
    assert_eq!(anchor.slots[0].slot, 0);
    assert_eq!(anchor.slots[1].slot, 1);
    for slot in &anchor.slots {
        assert!(
            (slot.to - slot.from) <= diarize::anchor_per(90.0) + 1e-9,
            "voice {} contributes {} s, over the ceiling",
            slot.slot,
            slot.to - slot.from
        );
    }
    // The pieces are laid end to end, so pass 2's overlap can be measured against them directly.
    assert_eq!(anchor.slots[0].anchor_from, 0.0);
    assert_eq!(anchor.slots[1].anchor_from, anchor.slots[0].anchor_to);
    assert!(anchor.seconds() <= 24.0 + 1e-9);

    // The log line §6 quotes, with the anchor's real figures in it.
    let log = diarize::anchor_log("talk", &anchor, 90.0, diarize::anchor_per(90.0));
    assert!(log.starts_with(">>> [talk] anchor: 2 voice(s) in "), "{log}");
    assert!(log.ends_with(" s of a 90 s window -- 12 s of new audio each"), "{log}");
}

#[test]
fn sec_04_prepare_6_details_confirmed_against_the_code_verification_pass_s3_a_voice_is_cut_from_its_longest_stretch() {
    // P.eng.anchorCutSeconds: a stretch shorter than this is no identification, only a seam.
    assert_eq!(ANCHOR_CUT_SECONDS, 0.3);
    let windows = vec![(0.0, 90.0)];
    let mut turns = speaks(0, 60.0, 100.0, 40.0, 1); // one long stretch, far into the window
    extend(&mut turns, &speaks(0, 3.0, 5.0, 0.2, 20)); // twenty flickers, none worth cutting
    let Scan::Planned { anchor, .. } = diarize::plan(90.0, 90.0, &windows, &[turns]) else {
        panic!("a 40 s voice is an anchor");
    };
    assert_eq!(anchor.slots.len(), 1);
    let slot = &anchor.slots[0];
    assert_eq!(slot.from, 60.0, "the cut comes from the long stretch, not from the flickers in front of it");
    assert!((slot.to - slot.from - 12.0).abs() < 1e-9);

    // A turn reaching past the window is clipped to it: the anchor cannot hold audio the server was
    // never sent, or its answer would be about something outside the request. The voice needs 4 s of
    // speech to count at all, so the clip has to leave an anchorable voice standing.
    let spanning = vec![turn(0, 20.0, 30.0), turn(0, 80.0, 200.0)];
    let Scan::Planned { anchor, .. } = diarize::plan(90.0, 90.0, &windows, &[spanning]) else {
        panic!("one voice is still an anchor");
    };
    assert!(anchor.slots[0].to <= 90.0 + 1e-9, "{:?}", anchor.slots[0]);
}

// --- S4: pass 2 asks about (window − anchor − 1) s -----------------------------------------------

#[test]
fn sec_04_prepare_6_details_confirmed_against_the_code_verification_pass_s4_every_second_is_placed_once() {
    let windows = vec![(0.0, 90.0)];
    let mut turns = speaks(0, 2.0, 20.0, 15.0, 4);
    extend(&mut turns, &speaks(1, 5.0, 12.0, 7.0, 4));
    let anchor_seconds;
    let place;
    match diarize::plan(600.0, 90.0, &windows, &[turns]) {
        Scan::Planned { place: p, anchor, .. } => {
            anchor_seconds = anchor.seconds();
            place = p;
        }
        other => panic!("expected a plan, got {other:?}"),
    }
    // The one second of slack is what keeps the anchor and the window from meeting mid-word.
    let span = 90.0 - anchor_seconds - diarize::WINDOW_SLACK_SECONDS;
    assert!(span >= diarize::NEW_AUDIO_MIN_SECONDS, "the plan only stands while {} s remain", span);
    assert!((place[0].1 - place[0].0 - span).abs() < 1e-9, "{:?}", place[0]);

    // Contiguous, in order, and covering the recording: a second placed twice would come back with two
    // ids to reconcile for nothing, and a second left out would keep no speaker at all.
    for pair in place.windows(2) {
        assert!((pair[0].1 - pair[1].0).abs() < 1e-9, "{place:?} has a gap or an overlap");
    }
    assert_eq!(place.first().unwrap().0, 0.0);
    assert!((place.last().unwrap().1 - 600.0).abs() < 1e-9, "the tail of the recording must be placed too: {place:?}");

    // Where an answer's sample lands inside its own window — the number the caller subtracts before
    // asking which anchor block that is.
    let first = diarize::Placement { from: place[0].0, to: place[0].1 };
    assert!((first.within(3 * SAMPLE_RATE) - 3.0).abs() < 1e-9);
}

// --- S5: slots are claimed one-to-one by anchor overlap ------------------------------------------

fn anchor(slots: &[(u32, f64, f64)]) -> Anchor {
    let mut at = 0.0;
    let slots = slots
        .iter()
        .map(|(slot, from, length)| {
            let held = Slot {
                slot: *slot,
                speech: *length,
                from: *from,
                to: from + length,
                anchor_from: at,
                anchor_to: at + length,
            };
            at += length;
            held
        })
        .collect();
    Anchor {
        window: 0,
        from: 0.0,
        to: 90.0,
        slots,
    }
}

#[test]
fn sec_04_prepare_6_details_confirmed_against_the_code_verification_pass_s5_the_strongest_claim_takes_the_anchor_block() {
    // The anchor file is 12 s of voice 0 at [0,12) and 12 s of voice 1 at [12,24); a claim names the span
    // of *that file* the caller measured a slot's voice against. Slot 0 reaches 9 s into voice 1's block,
    // slot 1 only 2 s — and one block is never two slots' winner. Both claims clear the bound, so it is
    // their strength that decides: claim order in the answer must not pick the winner.
    let anchor = anchor(&[(0, 0.0, 12.0), (1, 30.0, 12.0)]);
    // Their strength is what decides, so claim order in the answer must not be able to: the same two
    // claims give the same winner whichever way round they arrive. Slot 1 overlaps both blocks — 2 s of
    // voice 0 and 2 s of voice 1 — so it is named after the block it reaches most, which at an exact tie
    // is the first. What the pair proves is order-independence, and that one block is never two winners.
    for claims in [
        [(0, 12.0, 21.0), (1, 10.0, 14.0)].as_slice(),
        [(1, 10.0, 14.0), (0, 12.0, 21.0)].as_slice(),
    ] {
        let matched = diarize::match_slots(claims, &anchor);
        assert_eq!(matched.len(), 2);
        // Read by slot, not by position: the point is that both orders give the same voice per slot.
        let voice = |wanted: u32| matched.iter().find(|(slot, _)| *slot == wanted).map(|(_, block)| *block);
        assert_eq!(voice(0), Some(Some(1)), "the strongest claim on the second block wins it: {claims:?}");
        assert_eq!(
            voice(1),
            Some(Some(1)),
            "a slot is named after the voice it reaches most, tie and all — never left unidentified because a stronger slot got there first: {claims:?}"
        );
    }

    // A slot the anchor has nothing to say about — measured entirely outside every block — is nobody's,
    // which is what earns it a session id of its own.
    let stranger = diarize::match_slots(&[(0, 12.0, 21.0), (1, 40.0, 47.0)], &anchor);
    assert_eq!(stranger, vec![(0, Some(1)), (1, None)]);

    // One-to-one: no anchor block is ever two slots' *winner*, which is what stops one person's anchor
    // being read as the whole room. Both of these reach the first block; only the stronger wins it, and
    // the loser is still named after that voice — see the stranger check for what makes a slot nobody's.
    let crowded = diarize::match_slots(&[(0, 1.0, 12.0), (1, 2.0, 12.5)], &anchor);
    assert_eq!(crowded[0], (0, Some(0)), "the stronger of two claims on one block wins it");
    assert_eq!(crowded[1], (1, Some(0)), "the loser is the same voice, not a new one: the block was taken, not missed");

    // Each voice in turn, when each claims a different block.
    let both = diarize::match_slots(&[(0, 12.0, 16.0), (1, 2.0, 5.0)], &anchor);
    assert_eq!(both, vec![(0, Some(1)), (1, Some(0))]);

    // Overlap below the bound claims nothing at all — two voices merely near each other in time.
    let thin = diarize::match_slots(&[(0, 0.4, 0.8)], &anchor);
    assert_eq!(thin, vec![(0, None)]);
    assert_eq!(MIN_ANCHOR_OVERLAP, 0.5);
}

#[test]
fn sec_04_prepare_6_details_confirmed_against_the_code_verification_pass_s5_an_unmatched_slot_gets_a_fresh_id() {
    // The anchor holds one voice, its whole 12 s at the front of the file. A claim that reaches into it
    // is that voice; a claim measured entirely past it is a person the anchor never heard, and inventing
    // an id is the only answer that cannot put their words in another's mouth.
    let anchor = anchor(&[(0, 0.0, 12.0)]);
    let matched = diarize::match_slots(&[(0, 4.0, 12.0), (1, 20.0, 27.0)], &anchor);
    assert_eq!(matched[0].1, Some(0));
    assert_eq!(matched[1].1, None, "the second slot matches no anchor block");

    // Nothing claimed at all still answers for every slot it was handed, in the order it heard them.
    let none = diarize::match_slots(&[(3, 20.0, 26.0), (4, 21.0, 27.0)], &anchor);
    assert_eq!(none, vec![(3, None), (4, None)]);
    assert!(diarize::match_slots(&[], &anchor).is_empty());
}

// --- S6: the two ways the scan refuses ----------------------------------------------------------

#[test]
fn sec_04_prepare_6_details_confirmed_against_the_code_verification_pass_s6_an_anchor_that_eats_the_window_is_an_error() {
    // Fewer than 15 s of new audio per window is no room to identify anything, so this is the ladder's
    // cue to take a shorter rung — not a reason to send an anchor-only request.
    let windows = vec![(0.0, 25.0)];
    // Six voices' worth of anchor in a 25 s window: at a quarter of the window each (6.25 s), six of
    // them leave under 15 s of new audio, which is the ladder's cue to move on rather than send an
    // anchor-only request. Each voice speaks in turns over ANCHOR_MIN_SECONDS (§10's anchorMin) and
    // contributes a quarter of the window to the anchor, so six of them eat 37.5 s of a 25 s window.
    let mut turns = speaks(0, 1.0, 8.0, 6.25, 3);
    for slot in 1..6 {
        extend(&mut turns, &speaks(slot, 2.0 + slot as f64, 8.0, 6.25, 3));
    }
    let plan = diarize::plan(600.0, 25.0, &windows, &[turns]);
    let Scan::AnchorTooLong { anchor, window } = plan else {
        panic!("six voices' worth of anchor in a 25 s window must be refused, got {plan:?}");
    };
    assert_eq!(window, 25.0);
    let text = diarize::anchor_too_long(anchor, window);
    assert!(text.starts_with("anchor too long ("), "{text}");
    assert!(text.ends_with(" s of 25 s window)"), "{text}");
    assert_eq!(diarize::NEW_AUDIO_MIN_SECONDS, 15.0);
}

#[test]
fn sec_04_prepare_6_details_confirmed_against_the_code_verification_pass_s6_no_voice_long_enough_is_no_turns() {
    // The recording has sound but no voice the anchor could be cut from: an empty answer, said out loud
    // so an absent turns.json is never read as a run that did not happen.
    let windows = vec![(0.0, 90.0)];
    let flickers = speaks(0, 1.0, 5.0, 2.0, 10);
    assert_eq!(diarize::plan(300.0, 90.0, &windows, &[flickers]), Scan::NoVoice);
    assert_eq!(diarize::plan(300.0, 90.0, &windows, &[]), Scan::NoVoice);

    let log = diarize::no_voice_log("talk");
    assert_eq!(log, ">>> [talk] diarization: no voice long enough to anchor -- no turns");
}

#[test]
fn sec_04_prepare_6_details_confirmed_against_the_code_verification_pass_s6_the_ladder_is_walked_biggest_first() {
    // P.machine.diarWindowsSeconds: the caller tries 90, then 45, then 25; a rung that answers
    // AnchorTooLong is the ladder's cue to take the next one.
    assert_eq!(diarize::ladder(), &roles::DIAR_WINDOWS_SECONDS);
    assert_eq!(roles::DIAR_WINDOWS_SECONDS, [90.0, 45.0, 25.0]);

    // A crowded room is refused where the window cannot hold six voices' worth of anchor. The anchor is
    // a quarter of whichever window is in use, so shrinking the window shrinks the anchor with it: at 45 s
    // and 25 s six voices eat the whole window, while the 90 s rung serves them thinly — 72 s of anchor
    // leaves 17 s of new audio, just over NEW_AUDIO_MIN_SECONDS. The ladder therefore ends in a plan here,
    // and in failure only when even the top rung is too crowded to leave 15 s.
    let mut turns = speaks(0, 1.0, 8.0, 6.25, 3);
    for slot in 1..6 {
        extend(&mut turns, &speaks(slot, 2.0 + slot as f64, 8.0, 6.25, 3));
    }
    let mut refused = 0;
    let mut planned = Vec::new();
    for window in diarize::ladder() {
        let windows = vec![(0.0, *window)];
        match diarize::plan(600.0, *window, &windows, &[turns.clone()]) {
            Scan::AnchorTooLong { .. } => refused += 1,
            Scan::Planned { place, anchor, .. } => {
                assert!(place[0].1 - place[0].0 >= diarize::NEW_AUDIO_MIN_SECONDS, "{anchor:?}");
                planned.push(*window);
            }
            other => panic!("six voices in a {} s window: {other:?}", window),
        }
    }
    assert_eq!(planned, vec![90.0], "only the top rung leaves room to place anything");
    assert_eq!(refused, 2, "the two lower rungs are eaten by their own anchors");
}

#[test]
fn sec_04_prepare_6_details_confirmed_against_the_code_verification_pass_s6_both_passes_log_their_own_count() {
    // The two passes count windows, not requests, and they say different words: pass 1 is looking for
    // voices to anchor on, pass 2 is placing speakers against what it found. A log line that mixed them
    // would make a resumed run look like it had skipped half of pass 1.
    assert_eq!(diarize::scan_log("talk", 0, 4), ">>> [talk] finding voices 1/4");
    assert_eq!(diarize::scan_log("talk", 3, 4), ">>> [talk] finding voices 4/4");
    assert_eq!(diarize::placement_log("talk", 0, 9), ">>> [talk] placing speakers 1/9");
    assert_eq!(diarize::placement_log("talk", 8, 9), ">>> [talk] placing speakers 9/9");
}
