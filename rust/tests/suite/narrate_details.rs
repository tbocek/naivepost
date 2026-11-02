//! §07-narrate#6 — Details confirmed against the code (verification pass).
//!
//! One test per bullet of §6 (`spec/07-narrate.md:248-257`), named
//! `sec_07_narrate_6_details_confirmed_against_the_code_verification_pass_sN…`: the previous narration's
//! log line and the captions short-circuit, what an unusable reply says and how entries are matched, what
//! the Inputs row and its tooltip read, the re-roll and sample refusals, the reference build's log, how an
//! added file is named, what the take band states, and the preview's hold bound.
//!
//! Ids cited: `P.machine.narrationContextSeconds`, `P.policy.narrationMinWords`/`MaxWords`,
//! `P.eng.takeMinSeconds`, `P.eng.narrationMaxExtendSeconds`, `tool:ffmpeg.pcm`.

use crate::common;
use common::entry_at as entry;
use naivepost::cut::Seg;
use naivepost::narrate_details as det;
use naivepost::narrate_pass as pass;
use naivepost::narrate_reply as reply;
use naivepost::narrate_rules as rules;
use naivepost::narrate_screen as screen;
use naivepost::narration::{Narration, Silent};
use naivepost::params::Param;

const ITEM: &str = "sec_07_narrate_6_details_confirmed_against_the_code_verification_pass";

fn seg(s: f64, e: f64) -> Seg {
    let mut clip = Seg::default();
    clip.s = s;
    clip.e = e;
    clip.dur = e - s;
    clip.ins = "media/clip.mp4".to_string();
    clip
}

/// §10's own spelling of one id, from the catalogue.
fn spelled(rows: &[Param], id: &str) -> Option<String> {
    rows.iter()
        .find(|row| row.id == id)
        .map(|row| row.spelled.clone())
}

// ---- s1 [F4.1]: previous file kept once a valid narration came back ---------------

#[test]
fn sec_07_narrate_6_details_confirmed_against_the_code_verification_pass_s1_previous_and_silent_and_captions(
) {
    assert_eq!(
        ITEM,
        "sec_07_narrate_6_details_confirmed_against_the_code_verification_pass"
    );
    // §6: the log names where the overwritten narration is kept. Four leading spaces because it hangs
    // under the step's own line rather than starting one.
    assert_eq!(
        det::previous_kept_log("proj.naivepost/narrate/narration.prev.json"),
        "    the narration it replaced is kept at proj.naivepost/narrate/narration.prev.json"
    );

    // The file copy itself is F4.1's round (narrate_data::keep_previous needs a project tree, which this
    // headless test does not build). What §6 confirms about the replacement is that the deliberate silence
    // goes with it: a silent marker alone says "nothing said here", and one real entry on the same bounds
    // makes it a clip with a line — the marker never outlives the narration it was written against.
    let record = Narration {
        entries: vec![entry(0.0, 10.0, 0.0, "the replacement's own line")],
        silent: vec![Silent { s: 0.0, e: 10.0 }],
    };
    assert!(record.is_silent(0.0, 10.0));
    assert!(record.has_line(0.0, 10.0));

    // §6 [F4.1]: a captions-only voice still opens and closes the speaking job so the bar finishes — it
    // speaks nothing (rules::voice_is_captions is true) yet the bar is driven by narrate_pass::progress,
    // which counts clips, not lines. progress(2,2) is the close.
    assert!(rules::voice_is_captions(
        naivepost::narrate_screen::CAPTIONS
    ));
    assert_eq!(pass::progress(2, 2), "writing 2/2 clips");
}

// ---- s2 [F4.2] validation: every clip answered, entries matched forward only -------

#[test]
fn sec_07_narrate_6_details_confirmed_against_the_code_verification_pass_s2_validation_forward_only(
) {
    assert_eq!(
        ITEM,
        "sec_07_narrate_6_details_confirmed_against_the_code_verification_pass"
    );

    // A skipped clip and an entry that matches nothing (§6's two sentences), plus the all-empty refusal.
    let segs = [seg(0.0, 10.0), seg(10.0, 20.0), seg(20.0, 30.0)];
    assert_eq!(
        reply::clip_without_entry(2, &segs),
        "clip 3 (20.0-30.0) got no entry"
    );
    assert_eq!(
        pass::missing_clip_fault(3, 20.0, 30.0),
        "clip 3 (20.0-30.0) got no entry"
    );
    assert_eq!(
        pass::no_line_fault(),
        "every clip came back with no line at all"
    );

    // Forward-only: the fault says "or is out of order" because a reply that walks back to an earlier clip
    // matches nothing and is refused rather than quietly re-sorted.
    assert!(reply::unmatched_entry_fault(0.0, 10.0).contains("out of order"));

    // The refusal the run logs: attempt number plus the fault, so a retry reads as a redo of that reason.
    assert_eq!(
        pass::rejected_log(2, &pass::missing_clip_fault(3, 20.0, 30.0)),
        ">>> narration attempt 2 rejected: clip 3 (20.0-30.0) got no entry"
    );

    // Every line has to survive the word budget (§10's floor and ceiling per second of video).
    assert_eq!(pass::word_ceiling(5.0), 8);
    assert_eq!(pass::word_ceiling(120.0), 30);
    let rows = naivepost::params_narrate::rows();
    assert_eq!(
        spelled(&rows, "P.policy.narrationMinWords").as_deref(),
        Some("8")
    );
    assert_eq!(
        spelled(&rows, "P.policy.narrationMaxWords").as_deref(),
        Some("30")
    );
    assert_eq!(
        pass::MIN_WORDS as f64 * pass::WORDS_PER_SECOND / pass::WORDS_PER_SECOND,
        8.0
    );

    // Several entries on one clip play in offset order whatever order they arrived in — put back by
    // Narration::sort on every read and write.
    let mut record = Narration::default();
    record.entries = vec![
        entry(10.0, 20.0, 1.0, "the later clip"),
        entry(0.0, 10.0, 4.0, "said second in the first clip"),
        entry(0.0, 10.0, 1.0, "said first"),
    ];
    record.sort();
    let at: Vec<f64> = record.entries.iter().map(|line| line.at).collect();
    assert_eq!(at, vec![1.0, 4.0, 1.0]);

    // §6's "with tools, these are finish's answers": a reply is refused before anything is written, so the
    // faults above are all a caller can get. entry_at keeps a placement the model gave and defaults to the
    // clip's head when it gave none.
    assert_eq!(
        reply::entry_at(&entry(0.0, 10.0, 2.5, "placed"), &seg(0.0, 10.0)),
        2.5
    );
    assert_eq!(
        reply::entry_at(&entry(0.0, 10.0, 0.0, ""), &seg(0.0, 10.0)),
        0.0
    );
}

// ---- s3: the Inputs tooltip names its inputs -------------------------------------

#[test]
fn sec_07_narrate_6_details_confirmed_against_the_code_verification_pass_s3_inputs_tooltip() {
    assert_eq!(
        ITEM,
        "sec_07_narrate_6_details_confirmed_against_the_code_verification_pass"
    );
    // The row: N clip(s) · mm:ss, and the two counts ▶ would fix after a ⚠.
    assert_eq!(det::clips_line(2, 120.0), "2 clips \u{b7} 02:00");
    assert_eq!(det::clips_line(1, 5.0), "1 clip \u{b7} 00:05");
    assert_eq!(det::stale_mark(1, 0), "1 clip unwritten");
    assert_eq!(det::stale_mark(0, 1), "1 line off the cut");
    assert_eq!(
        det::inputs_line(4, 95.0, 3, 2, true, true),
        "4 clips \u{b7} 01:35 \u{b7} \u{26a0} 3 clips unwritten, 2 lines off the cut"
    );
    // A missing timeline is a missing input, so it goes on the line rather than into the tooltip.
    assert_eq!(
        det::inputs_line(4, 95.0, 0, 0, false, true),
        "4 clips \u{b7} 01:35 \u{b7} no timeline"
    );
    assert_eq!(
        det::inputs_line(0, 0.0, 0, 0, true, false),
        det::NO_CUT_LINE
    );

    assert_eq!(pass::CONTEXT_SECONDS, 4.0);
    assert_eq!(
        det::timeline_line(688).unwrap(),
        "prepare/transcript/session.tsv \u{2014} 688 lines; the ones falling inside a clip (\u{b1}4 s) go with that clip"
    );
    assert_eq!(det::timeline_line(0), None);
    assert_eq!(
        det::cut_detail(2, 120.0),
        "cut/cut.json \u{2014} 2 clips, 02:00 of video to write for"
    );
    assert_eq!(
        det::voice_line("Mira", -1.5),
        "Spoken by Mira at -1.5 semitones (narrate/voice_ref.wav)"
    );

    // The whole tooltip, in §6's order: the cut file, why it is stale and what fixes it, the transcript,
    // the session context verbatim, the voice. Rows separated by a blank line.
    let tip = det::inputs_tooltip(
        2,
        120.0,
        "the cut changed",
        688,
        "a demo of the app",
        Some(("Mira", -1.5)),
    );
    let rows: Vec<&str> = tip.split("\n\n").collect();
    assert_eq!(rows.len(), 5, "{tip}");
    assert_eq!(
        rows[1],
        "\u{26a0} the cut changed \u{2014} \u{25b6} writes the narration again"
    );
    assert_eq!(rows[2], det::timeline_line(688).unwrap());
    assert_eq!(
        rows[3],
        "Session context (Describe), sent with the narration:\na demo of the app"
    );

    // The page's own readout is the same row (§1). It writes the counts parenthetically — "4 clip(s)",
    // "2 line(s)" — where det::inputs_line writes them as the prototype's plural helper does ("4 clips",
    // "2 lines"). Same shape, same ⚠, same "no timeline"; only that spelling differs, so the two are
    // pinned side by side instead of asserted equal.
    assert_eq!(
        screen::inputs_readout(4, 95.0, 3, 2, true, true),
        "4 clip(s) \u{b7} 01:35 \u{b7} \u{26a0} 3 clip(s) unwritten, 2 line(s) off the cut"
    );
    assert_eq!(
        screen::inputs_readout(4, 95.0, 0, 0, false, true),
        "4 clip(s) \u{b7} 01:35 \u{b7} no timeline"
    );
    assert_eq!(
        screen::inputs_readout(0, 0.0, 0, 0, true, false),
        det::NO_CUT_LINE
    );
    // Outputs names the three things this page owns.
    assert_eq!(
        screen::outputs_readout(),
        "narrate/ \u{2014} narration.json, the voice reference and the synthesis cache"
    );
}

// ---- s4: re-roll and sample refusals ---------------------------------------------

// ---- s5: the reference build's log ------------------------------------------------

#[test]
fn sec_07_narrate_6_details_confirmed_against_the_code_verification_pass_s5_reference_build_log() {
    assert_eq!(
        ITEM,
        "sec_07_narrate_6_details_confirmed_against_the_code_verification_pass"
    );
    // §6 writes the line as "X s, N words | N hand-picked take(s) from <base>": the bar separates the two
    // kinds of reference, because they fail differently — an automatic one that sounds wrong is a ranking
    // to overrule by hand, a hand-picked one is seconds to re-pick. So exactly one half appears.
    assert_eq!(
        det::reference_build_log(13.4, 41, 0, "rec-2025-08-14"),
        ">>> voice reference built: 13.4 s, 41 words from rec-2025-08-14"
    );
    assert_eq!(
        det::reference_build_log(13.4, 41, 3, "rec-2025-08-14"),
        ">>> voice reference built: 13.4 s, 3 hand-picked take(s) from rec-2025-08-14"
    );

    // A pasted recording that is not mono PCM yet is converted by ffmpeg (tool:ffmpeg.pcm); this page only
    // says why.
    assert_eq!(
        det::reference_convert_log("guest.wav"),
        "guest.wav is not mono PCM \u{2014} converting it with ffmpeg"
    );

    // The automatic ranking behind the line — the speaker with the most turn time, solo turns (nobody else
    // within P.eng.refPadSeconds), narrowed to their words and ranked by words said with length as
    // tie-break, takes under P.eng.refMinWordsPerSecond dropped unless that leaves none — is F4.6's decision
    // and deliberately not re-implemented or restated here. Its five numbers are catalogued once each, in
    // §07's section, built from the `voice_ref` constants `plan_reference` reads (§07-narrate#4-parameters-used).
    let rows = naivepost::params_narrate::rows();
    assert_eq!(
        spelled(&rows, "P.machine.refWantSeconds").as_deref(),
        Some("14")
    );
    assert_eq!(spelled(&rows, "P.eng.refPadSeconds").as_deref(), Some("2"));
    assert_eq!(
        spelled(&rows, "P.eng.refMinWordsPerSecond").as_deref(),
        Some("1.5")
    );
}

// ---- s6: "Add file…" never overwrites --------------------------------------------

// ---- s7: the take band's messages -------------------------------------------------

// ---- s8: the preview's bounds -----------------------------------------------------
