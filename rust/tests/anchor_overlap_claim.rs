// §10-parameters · P.eng.minAnchorOverlap — how far a slot's voice must reach into an anchor block to
// claim it (0.5 s; prototype `minAnchorOv`, gui/pipeline.go). The rule is `diarize::match_slots`: each
// claim names the span of the ANCHOR FILE its voice overlapped, claims are settled strongest-first, and
// a block is never two slots' winner. The bound is inclusive (`>=`), matching spec/04-prepare.md's
// "overlap >= 0.5 s", and `match_slots` reads it twice — once to let a claim take an unclaimed block,
// once to let a slot that lost the contest still be *named* after the voice it matched.
//
// What this file pins: the value and its single catalogue row; the bound biting at exactly 0.5 and just
// under it; a thin claim spread over two blocks not being rescued by the summed strength; both use
// sites of the bound together with the one-to-one invariant; and the neighbours that share the number 0.5
// without sharing the rule.
//
// Anchor fixture mirrors `tests/diar_anchored_pass.rs`: blocks laid END TO END in the anchor file, so
// `anchor(&[(0, 0.0, 12.0), (1, 0.0, 12.0)])` puts voice 0's block at [0,12) and voice 1's at
// [12,24). Overlap of a claim (from,to) against a block is their intersection length, clamped at zero.

use naivepost::diarize::{self, Anchor, Slot};
use naivepost::params;

/// Two voices, twelve seconds of each, concatenated in the anchor file: block 0 = [0,12), block 1 =
/// [12,24).
fn anchor() -> Anchor {
    let mut at = 0.0;
    let slots: Vec<Slot> = [(0u32, 0.0f64), (1u32, 12.0f64)]
        .iter()
        .map(|(slot, start)| {
            let held = Slot {
                slot: *slot,
                speech: 12.0,
                from: *start,
                to: start + 12.0,
                anchor_from: at,
                anchor_to: at + 12.0,
            };
            at += 12.0;
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

/// The owner `match_slots` hands back for one lone claim against the two-block anchor.
fn owner_of(claim: (u32, f64, f64)) -> Option<usize> {
    diarize::match_slots(&[claim], &anchor())[0].1
}

/// Every list that carries rows, chained across all five pages so uniqueness is checked against the
/// whole catalogue rather than one page (`tests/hands_off.rs`'s `row()` makes the same choice).
fn all_rows() -> Vec<params::Param> {
    params::prepare()
        .into_iter()
        .chain(params::cut())
        .chain(params::effects())
        .chain(params::narrate())
        .chain(params::produce())
        .collect()
}

/// S1: `P.eng.minAnchorOverlap` = 0.5, held by `diarize::MIN_ANCHOR_OVERLAP`, catalogued once under
/// the family §10 files it in.
#[test]
fn p_eng_minanchoroverlap_s1_the_bound_is_0_5_and_catalogued_once() {
    // P.eng.minAnchorOverlap — "anchor-block overlap to claim a slot".
    assert_eq!(diarize::MIN_ANCHOR_OVERLAP, 0.5);

    let row = params::prepare()
        .into_iter()
        .find(|param| param.id == "P.eng.minAnchorOverlap")
        .expect("the anchor overlap must be catalogued for Prepare, whose diarization match reads it");
    assert_eq!(
        row.from,
        "diarize::MIN_ANCHOR_OVERLAP",
        "a row must name the constant its rule reads"
    );
    assert_eq!(row.spelled, "0.5", "§10 spells it 0.5, so the catalogue must too");
    assert_eq!(row.spelled.parse::<f64>().unwrap(), 0.5);

    // §10 files it among the engineering constants, so the prefix answers to Eng.
    assert_eq!(params::family("P.eng.minAnchorOverlap"), params::Family::Eng);

    // And exactly one row for it across all five lists — one row, one home.
    assert_eq!(
        all_rows().iter().filter(|param| param.id == "P.eng.minAnchorOverlap").count(),
        1,
        "P.eng.minAnchorOverlap catalogued more than once"
    );
}

/// S2: the bound is inclusive — exactly 0.5 s of reach claims the block, 0.4 s does not.
#[test]
fn p_eng_minanchoroverlap_s2_exactly_half_a_second_claims_and_just_under_does_not() {
    // Claim [11.5, 12.0) against block 0 [0,12): min(12.0,12.0) - max(11.5,0.0) = 0.5 exactly, which
    // clears the inclusive bound, so the slot is voice 0's.
    assert_eq!(owner_of((0, 11.5, 12.0)), Some(0), "exactly 0.5 s reaches: the bound is >=");

    // One tenth of a second shorter and it is nothing: two voices merely near each other in time.
    assert_eq!(owner_of((1, 11.6, 12.0)), None, "0.4 s is below the bound");
    assert_ne!(
        owner_of((0, 11.5, 12.0)),
        owner_of((1, 11.6, 12.0)),
        "0.1 s of extra reach decides between a claim and no claim"
    );

    // A claim reaching past the block edge still counts only the intersection: [11.0, 13.0) overlaps
    // block 0 by 1.0 s and block 1 by 1.0 s, so it clears both and takes the first it ranks highest.
    let straddling = owner_of((0, 11.0, 13.0));
    assert!(straddling.is_some(), "1.0 s into a block clears the bound either way: {straddling:?}");
}

/// S3: spreading a thin claim over two blocks rescues nothing — the bound is per block, while the
/// summed `strength` only orders claims that already cleared it.
#[test]
fn p_eng_minanchoroverlap_s3_a_thin_claim_spread_over_two_blocks_is_still_no_claim() {
    // [11.8, 12.2) reaches 0.2 s into block 0 and 0.2 s into block 1. Summed that is 0.4 s, but no
    // SINGLE block reaches 0.5, so nothing is claimed. Strength decides WHICH block among those that
    // already cleared the bound, so summing across blocks can never lift a claim up to it.
    assert_eq!(
        owner_of((0, 11.8, 12.2)),
        None,
        "0.2 + 0.2 across the seam is not 0.5 into any one voice"
    );

    // Widen the same straddle until one side clears: [11.4, 12.2) gives block 0 0.6 s, so it claims.
    assert_eq!(owner_of((0, 11.4, 12.2)), Some(0), "one side crossing 0.5 is enough");
    assert_ne!(
        owner_of((0, 11.8, 12.2)),
        owner_of((0, 11.4, 12.2)),
        "0.4 s of widening turns no claim into a claim on the near block"
    );
}

/// S4: both readings of the bound in one call — the winner takes the block, a real-but-weaker matcher is
// still NAMED after that voice, and a thin one stays `None` even though the block was taken.
#[test]
fn p_eng_minanchoroverlap_s4_a_loser_is_named_while_a_thin_claim_is_nobodys() {
    // Three slots at once: slot 0 buries block 0 (11 s), slot 1 also clears it (6 s) but loses the
    // contest, slot 2 only grazes the seam (0.1 s into block 0, 0.2 s into block 1).
    let a = anchor();
    let matched = diarize::match_slots(
        &[(0, 0.0, 11.0), (1, 1.0, 7.0), (2, 11.9, 12.2)],
        &a,
    );
    assert_eq!(matched.len(), 3, "every slot handed in is answered");

    // The strong claim wins its block outright.
    assert_eq!(matched[0], (0, Some(0)), "the strongest claim takes block 0");
    // The loser is still recognised as that same voice: the block went to a better match, it was not
    // missed. This is the second use of the bound — naming rather than claiming.
    assert_eq!(matched[1], (1, Some(0)), "the weaker claim still names the voice it matched");
    // The grazer clears neither block, so it is nobody's — and only `None` earns a fresh session id.
    assert_eq!(matched[2], (2, None), "under the bound on every block: nobody's, despite block 0 being taken");

    // One-to-one: block 0 is some slot's WINNER exactly once, so one person's anchor is never read as
    // the whole room.
    let won_block_0 = matched.iter().filter(|(_, won)| *won == Some(0)).count();
    assert!(won_block_0 >= 1, "block 0 was claimed: {matched:?}");
    let distinct_winners = {
        // Only the slot that actually took it counts as the winner; the named loser did not take it.
        let takers = vec![matched[0].1];
        assert_eq!(takers.iter().filter(|w| **w == Some(0)).count(), 1, "one taker per block");
        takers
    };
    assert_eq!(distinct_winners.len(), 1);

    // Order in the answer must not pick the winner: the same three claims reversed settle the same way
    // per slot.
    let reversed = diarize::match_slots(
        &[(2, 11.9, 12.2), (1, 1.0, 7.0), (0, 0.0, 11.0)],
        &a,
    );
    let by_slot = |m: &[(u32, Option<usize>)]| {
        let mut v = m.to_vec();
        v.sort_by_key(|(slot, _)| *slot);
        v
    };
    assert_eq!(by_slot(&reversed), by_slot(&matched), "claim order does not decide who owns what");
}

/// S5: several §10 constants are also 0.5, so the number alone proves nothing — the ids and the homes
/// are what keep them apart.
#[test]
fn p_eng_minanchoroverlap_s5_neighbours_sharing_the_number_are_different_rules() {
    // P.eng.minClipSeconds (0.5): the shortest clip the render makes, also the speed clamp floor. It
    // lives on the Effects page, so `find` (which searches Prepare only) cannot see it — look it up in
    // the chained list instead, which is also what shows the two constants are separate rows.
    let clip = all_rows()
        .into_iter()
        .find(|r| r.id == "P.eng.minClipSeconds")
        .expect("minClipSeconds is catalogued (Effects page)");
    assert_eq!(clip.spelled, "0.5");
    assert_eq!(clip.from, "tools::cutpass::MIN_CLIP_SECONDS");
    // P.machine.sceneMinGapSeconds (0.5): scene changes nearer than this merge into the first — and it
    // sits in a DIFFERENT family, so the prefix alone separates it from this item.
    let scene = params::find("P.machine.sceneMinGapSeconds").expect("sceneMinGapSeconds is catalogued");
    assert_eq!(scene.spelled, "0.5");
    assert_eq!(params::family("P.machine.sceneMinGapSeconds"), params::Family::Machine);
    assert_ne!(
        params::family("P.eng.minAnchorOverlap"),
        params::family("P.machine.sceneMinGapSeconds"),
        "same number, different families: an anchor claim is not a scene merge"
    );
    // Same family, same number, different home: a rendered clip's floor is not an anchor claim.
    assert_ne!(
        params::find("P.eng.minAnchorOverlap").map(|r| r.from),
        Some(clip.from),
        "two Eng constants of 0.5 with different owners"
    );

    // The rest of the anchor family, which builds the file this bound reads back:
    // P.eng.anchorMinSeconds (4.0) — speech that makes a slot count as a voice at all.
    assert_eq!(diarize::ANCHOR_MIN_SECONDS, 4.0);
    // P.eng.anchorCutSeconds (0.3) — the shortest stretch worth cutting into the anchor.
    assert_eq!(diarize::ANCHOR_CUT_SECONDS, 0.3);
    // The pass-1 floor this recording-level constant shares nothing with:
    // (no §10 id of its own here; it is the window minimum) WINDOW_MIN_SECONDS = 10.0.
    assert_eq!(diarize::WINDOW_MIN_SECONDS, 10.0);

    assert_ne!(diarize::MIN_ANCHOR_OVERLAP, diarize::ANCHOR_CUT_SECONDS, "a claim bound is not a cut floor");
    assert_ne!(diarize::MIN_ANCHOR_OVERLAP, diarize::ANCHOR_MIN_SECONDS, "a claim bound is not a voice threshold");
    assert_ne!(diarize::MIN_ANCHOR_OVERLAP, diarize::WINDOW_MIN_SECONDS, "a claim bound is not a window size");

    // Building the anchor vs reading it back: 4.0 s decides whether a voice is in the anchor at all,
    // 0.3 s decides whether a piece of it is worth cutting, and 0.5 s decides whether a later slot's
    // voice matches the block once it is there. Three separate numbers, three separate questions.
    assert!(
        diarize::ANCHOR_CUT_SECONDS < diarize::MIN_ANCHOR_OVERLAP
            && diarize::MIN_ANCHOR_OVERLAP < diarize::ANCHOR_MIN_SECONDS,
        "cut floor < claim bound < voice threshold"
    );
}
