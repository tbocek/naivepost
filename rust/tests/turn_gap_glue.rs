// §10-parameters · P.eng.turnGapSeconds — the gap that ends a turn (0.5 s; prototype `diarTurnGap`,
// gui/pipeline.go). The rule is `transcribe::glue_turns`: walking the diarizer's turns in order, two
// of the SAME speaker with no more than this much silence between them are one turn, extended to the
// later end. Without it the diarizer's split at every breath hands one sentence to two people, and a
// word falling between the halves gets a different name from its neighbours.
//
// What this file pins: the value and its single catalogue row; the bound being inclusive at exactly
// 0.5 and one tenth over; same-speaker chains collapsing while a second speaker never glues; the two
// guards inside the glue arm (never shorten, only the previous kept turn is compared); and the
// neighbouring constants that share or nearly share the number without sharing the rule.

use naivepost::params;
use naivepost::requests::Turn;
use naivepost::transcribe;

#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::{all_rows};

/// A diarizer turn built from seconds, counted in samples at the one rate every sidecar uses.
fn turn(start: f64, end: f64, speaker: u32) -> Turn {
    Turn {
        start_sample: (start * 16000.0) as u64,
        end_sample: (end * 16000.0) as u64,
        speaker_id: speaker,
    }
}

fn turns(spans: &[(f64, f64, u32)]) -> Vec<Turn> {
    spans.iter().map(|(s, e, id)| turn(*s, *e, *id)).collect()
}

/// `glue_turns` over seconds, since a test's turns are written in seconds and the diarizer's answer
/// is samples.
fn spoken(spans: &[(f64, f64, u32)]) -> Vec<(f64, f64, String)> {
    transcribe::glue_turns(&turns(spans))
}

/// S1: `P.eng.turnGapSeconds` = 0.5, held by `transcribe::TURN_GLUE`, catalogued once under the
/// family §10 files it in.
#[test]
fn p_eng_turngapseconds_s1_the_gap_is_0_5_and_catalogued_once() {
    // P.eng.turnGapSeconds — "gap that ends a turn".
    assert_eq!(transcribe::TURN_GLUE, 0.5);

    let row = params::prepare()
        .into_iter()
        .find(|param| param.id == "P.eng.turnGapSeconds")
        .expect("the turn gap must be catalogued for Prepare, whose transcription reads it");
    assert_eq!(
        row.from,
        "transcribe::TURN_GLUE",
        "a row must name the constant its rule reads"
    );
    assert_eq!(row.spelled, "0.5", "§10 spells it 0.5, so the catalogue must too");
    assert_eq!(row.spelled.parse::<f64>().unwrap(), 0.5);

    // §10 files it among the engineering constants, so the prefix answers to Eng.
    assert_eq!(params::family("P.eng.turnGapSeconds"), params::Family::Eng);

    // And exactly one row for it across all five lists — one row, one home.
    assert_eq!(
        all_rows().iter().filter(|param| param.id == "P.eng.turnGapSeconds").count(),
        1,
        "P.eng.turnGapSeconds catalogued more than once"
    );
}

/// S2: the bound is inclusive — a gap of exactly 0.5 still leaves one turn, one tenth more ends it.
#[test]
fn p_eng_turngapseconds_s2_exactly_half_a_second_of_silence_still_glues() {
    // Gap = 1.5 - 1.0 = 0.5 exactly, which clears the inclusive `<=`, so the two pieces are one turn
    // spanning the whole of both.
    let glued = spoken(&[(0.0, 1.0, 0), (1.5, 2.5, 0)]);
    assert_eq!(glued.len(), 1, "exactly 0.5 s of gap: still one turn: {glued:?}");
    assert_eq!(glued[0], (0.0, 2.5, "SPEAKER_00".to_string()));

    // One tenth over the bound and the turn ends there.
    let split = spoken(&[(0.0, 1.0, 0), (1.6, 2.5, 0)]);
    assert_eq!(split.len(), 2, "0.6 s of gap: the turn ended: {split:?}");
    assert_eq!(split[0].1, 1.0, "the first turn keeps its own end");
    assert_eq!(split[1].0, 1.6, "the second starts where the diarizer put it");

    // 0.1 s of breath decides between one speaker's sentence and two turns.
    assert_ne!(glued.len(), split.len());
}

/// S3: the glue is per speaker, and it chains — each piece within the gap joins the one before, so a
/// sentence cut at three breaths is still one turn.
#[test]
fn p_eng_turngapseconds_s3_same_speaker_chains_and_another_speaker_never_joins() {
    // Different speakers are never glued however close — here they touch exactly, gap 0.0.
    let two = spoken(&[(0.0, 1.0, 0), (1.0, 2.0, 1)]);
    assert_eq!(two.len(), 2, "a speaker change always ends a turn: {two:?}");
    assert_eq!(two[0].2, "SPEAKER_00");
    assert_eq!(two[1].2, "SPEAKER_01");

    // Three same-speaker pieces, each pair within the bound (0.2 then 0.3): one turn over the whole span.
    let chain = spoken(&[(0.0, 1.0, 0), (1.2, 2.0, 0), (2.3, 3.0, 0)]);
    assert_eq!(chain.len(), 1, "each gap is inside the bound: {chain:?}");
    assert_eq!(chain[0], (0.0, 3.0, "SPEAKER_00".to_string()));

    // The FIRST gap alone pushed past the bound (0.7) cuts the chain at once and nothing downstream
    // rejoins it: the split falls where the silence is, and the later pieces keep their own span.
    let broken = spoken(&[(0.0, 1.0, 0), (1.7, 2.0, 0), (2.3, 3.0, 0)]);
    assert_eq!(broken.len(), 2, "gap of 0.7 before the second piece ends the first turn: {broken:?}");
    assert_eq!(broken[0], (0.0, 1.0, "SPEAKER_00".to_string()), "the first turn stops where the silence began");
    // The last two DID glue (0.3 between them), so they came out as one turn spanning both.
    assert_eq!(broken[1], (1.7, 3.0, "SPEAKER_00".to_string()), "pieces two and three glued to each other");
}

/// S4: the two guards inside the glue arm — a shorter later turn does not pull the end back, and only
/// the previous KEPT turn is compared, so the input's order is what makes this work.
#[test]
fn p_eng_turngapseconds_s4_a_shorter_later_turn_never_shortens_and_only_the_previous_turn_is_compared() {
    // (a) The second turn lies inside the first's span and starts 0.1 after its end: it glues, but its
    // end (2.0) is BEFORE the kept end (3.0), so the `end > previous.1` guard leaves the span alone.
    // A turn keeps the widest span it has been shown, never the narrowest.
    let inside = spoken(&[(0.0, 3.0, 0), (3.1, 2.0, 0)]);
    assert_eq!(inside.len(), 1, "0.1 s apart, same speaker: one turn: {inside:?}");
    assert_eq!(inside[0].1, 3.0, "the earlier, wider end survives");
    assert_eq!(inside[0].0, 0.0);

    // (b) Only the last kept turn is looked at. Here the two same-speaker pieces that WOULD glue are
    // separated in the LIST by another speaker, so each comparison is against a different-speaker
    // neighbour and nothing merges: 3 turns stand. glue_turns walks the diarizer's ordered answer and
    // compares against the previous kept turn only — reordering the input would change the result,
    // which is why it is fed what the diarizer wrote in order, and why running it twice changes nothing.
    let interleaved = spoken(&[(0.0, 1.0, 0), (5.0, 6.0, 1), (6.2, 7.0, 0)]);
    assert_eq!(interleaved.len(), 3, "no adjacent same-speaker pair: {interleaved:?}");
    assert_eq!(
        interleaved.iter().map(|t| t.2.as_str()).collect::<Vec<_>>(),
        vec!["SPEAKER_00", "SPEAKER_01", "SPEAKER_00"]
    );

    // Idempotence: gluing an already-glued list again changes nothing, because every resulting turn is
    // further from its neighbour than the bound or belongs to another speaker.
    let once = spoken(&[(0.0, 1.0, 0), (1.2, 2.0, 0), (2.3, 3.0, 0)]);
    let twice = transcribe::glue_turns(&[turn(once[0].0, once[0].1, 0)]);
    assert_eq!(twice, once, "one turn in, one turn out");
}

/// S5: the neighbours. Several constants sit near 0.5, so the value proves nothing — the ids and the
/// homes are what keep the rules apart.
#[test]
fn p_eng_turngapseconds_s5_neighbours_are_tighter_looser_and_differently_owned() {
    // P.eng.mergeGapSeconds (0.7, `transcribe::MERGE_GAP`): the silence that splits a word group.
    // The TURN boundary is deliberately tighter than the WORD-GROUP boundary: a pause short enough to
    // keep one speaker talking can still be long enough that no word crosses it.
    assert_eq!(transcribe::MERGE_GAP, 0.7);
    assert!(
        transcribe::TURN_GLUE < transcribe::MERGE_GAP,
        "a turn ends sooner than a word group does"
    );

    // P.eng.mergeNearSeconds (1.0, `transcribe::MERGE_NEAR`): how far a word sitting outside every
    // turn still reaches for a speaker — looser than the turn gap, because diarization edges are not
    // exact. P.eng.mergeMaxSeconds (12.0) caps a segment's length; neither is a turn boundary.
    assert_eq!(transcribe::MERGE_NEAR, 1.0);
    assert_eq!(transcribe::MERGE_MAX_LEN, 12.0);
    assert_ne!(transcribe::TURN_GLUE, transcribe::MERGE_NEAR, "a turn gap is not a word's reach for a speaker");
    assert_ne!(transcribe::TURN_GLUE, transcribe::MERGE_MAX_LEN, "a turn gap is not a segment cap");

    // Other 0.5 constants, told apart by home rather than by value:
    // P.eng.minAnchorOverlap — anchor-block overlap that claims a slot (diarize::MIN_ANCHOR_OVERLAP).
    let anchor = params::find("P.eng.minAnchorOverlap").expect("minAnchorOverlap is catalogued");
    assert_eq!(anchor.spelled, "0.5");
    assert_eq!(anchor.from, "diarize::MIN_ANCHOR_OVERLAP");
    assert_ne!(anchor.from, "transcribe::TURN_GLUE", "same number, different rule and module");
    // P.machine.sceneMinGapSeconds — scene changes nearer than this merge into the first; a different
    // FAMILY as well, so the prefix alone separates it.
    let scene = params::find("P.machine.sceneMinGapSeconds").expect("sceneMinGapSeconds is catalogued");
    assert_eq!(scene.spelled, "0.5");
    assert_eq!(params::family("P.machine.sceneMinGapSeconds"), params::Family::Machine);
    assert_ne!(
        params::family("P.eng.turnGapSeconds"),
        params::family("P.machine.sceneMinGapSeconds"),
        "a turn gap is Eng, a scene gap is Machine"
    );
    // P.eng.minClipSeconds — the shortest clip the render makes, also the speed clamp floor. It lives
    // on the EFFECTS page, so `find` (Prepare only) cannot see it; look it up in the chained list.
    let clip = all_rows()
        .into_iter()
        .find(|r| r.id == "P.eng.minClipSeconds")
        .expect("minClipSeconds is catalogued (Effects page)");
    assert_eq!(clip.spelled, "0.5");
    assert_eq!(clip.from, "tools::cutpass::MIN_CLIP_SECONDS");
    assert!(params::find("P.eng.minClipSeconds").is_none(), "`find` searches Prepare only");

    // Three 0.5s answering three questions: when a turn stops, when an anchor block counts as a voice's,
    // and how short a rendered clip may be.
    assert_eq!(
        [
            params::find("P.eng.turnGapSeconds").map(|r| r.from),
            params::find("P.eng.minAnchorOverlap").map(|r| r.from),
            Some(clip.from),
        ]
        .iter()
        .cloned()
        .collect::<std::collections::HashSet<_>>()
        .len(),
        3,
        "three distinct homes behind the same number"
    );
}
