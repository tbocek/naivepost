// §05-cut#8-details-confirmed-against-the-code-verification-pass — bullet 3 (Suggest).
//
// ▶'s own round writes the pass; what §8 settles already, and what these tests pin, is everything around the
// call: which retry thinks, when the web tools come back off, how a streamed answer becomes one progress
// number, which segments are thrown away before their length is argued about, how the checks answer, and what
// a rate for a whole segment counts as. Strings are compared whole — a reworded log line passes `contains` and
// still fails the spec.

use naivepost::cut::EffectKind;
use naivepost::cut_suggest as suggest;
use naivepost::preview;
use naivepost::roles::{self, Job};
use naivepost::timeline;

/// Two recordings with a hole in the middle: nothing before 10 and nothing between 40 and 60 was filmed.
fn runs() -> Vec<(f64, f64)> {
    timeline::filmed_runs(&[
        timeline::Recording { base: "a".into(), start: 10.0, end: 40.0 },
        timeline::Recording { base: "b".into(), start: 60.0, end: 120.0 },
    ])
}

/// §05-cut#8-details-confirmed-against-the-code-verification-pass — `thinking on for the first attempt, off
/// after an all-reasoning reply`, and the log line that says so.
#[test]
fn sec_05_cut_8_details_confirmed_against_the_code_verification_pass_s3_thinking_stops_after_an_empty_reply() {
    assert_eq!(
        suggest::THINKING_RETRY_LOG,
        ">>> suggest: the model spent the whole call thinking and wrote nothing \u{2014} asking again with thinking off"
    );
    // P.eng.llmAttempts — three attempts at most, and the first always thinks.
    assert_eq!(roles::LLM_ATTEMPTS, 3);
    assert!(suggest::thinking_after_empty_reply(false, 1), "the first attempt thinks");
    assert!(suggest::thinking_after_empty_reply(true, 1), "even more so with nothing behind it");
    // An all-reasoning reply that wrote nothing: the next call asks for the answer alone.
    assert!(!suggest::thinking_after_empty_reply(true, 2));
    // Any other failure (bad seconds, a rejected segment) keeps thinking — roles owns that distinction.
    assert!(suggest::thinking_after_empty_reply(false, 2), "a wrong answer is not an empty one");
    assert!(suggest::thinking_after_empty_reply(false, 3));
    // Consistent with the request builder rather than re-decided: attempt 1 thinks whatever §2's row says.
    assert_eq!(
        suggest::thinking_after_empty_reply(false, 1),
        roles::cut_attempt(1).thinking && roles::thinking(Job::ModelCut)
    );
}

/// §05-cut#8-details-confirmed-against-the-code-verification-pass — `web tools withdrawn on the first
/// rejection`, and the log line for it.
#[test]
fn sec_05_cut_8_details_confirmed_against_the_code_verification_pass_s3_web_tools_go_after_one_rejection() {
    assert_eq!(suggest::WEB_TOOLS_WITHDRAWN_LOG, ">>> suggest: asking again without the web tools");
    assert!(suggest::offers_web(0), "nothing rejected yet, so the model may look things up");
    assert!(!suggest::offers_web(1), "one rejection is enough");
    assert!(!suggest::offers_web(2));

    // The same rule as roles::cut_attempt's `web_tools`, read from rejections instead of attempt numbers.
    for rejections in 0..roles::LLM_ATTEMPTS {
        assert_eq!(suggest::offers_web(rejections), roles::cut_attempt(rejections + 1).web_tools);
    }
    // And the cut model is one of the three jobs §2 marks `(+ web tools)` at all.
    assert!(roles::web_tools(Job::ModelCut));
}

/// §05-cut#8-details-confirmed-against-the-code-verification-pass — `progress "N moments, at m:ss of m:ss"
/// placed by the last closed segment's end, never below 0.02 nor backwards, pulsing under "thinking over the
/// whole session" until the first segment closes`.
#[test]
fn sec_05_cut_8_details_confirmed_against_the_code_verification_pass_s3_progress_never_goes_backwards() {
    assert_eq!(suggest::THINKING_STATUS, "thinking over the whole session");
    assert_eq!(
        suggest::progress(3, 125.0, 600.0),
        format!("3 moments, at {} of {}", naivepost::tools::mm_ss(125.0), naivepost::tools::mm_ss(600.0))
    );

    // Placed by the last closed segment's end.
    assert_eq!(suggest::place_progress(42.0, 10.0), 42.0);
    // Never below the floor — a bar at exactly 0% is indistinguishable from one that never started.
    // suggest.progressFloorSeconds
    assert_eq!(suggest::PROGRESS_FLOOR_SECONDS, 0.02);
    assert_eq!(suggest::place_progress(0.0, 0.0), 0.02);
    // Never backwards: a tick that has nothing new to report leaves the line where it was.
    assert_eq!(suggest::place_progress(5.0, 30.0), 30.0);
    assert_eq!(suggest::place_progress(30.0, 30.0), 30.0, "standing still is allowed");

    // Pulsing until the first segment closes; a floor reading with nothing closed is still "thinking".
    assert!(suggest::pulses(0.0, 0));
    assert!(suggest::pulses(suggest::PROGRESS_FLOOR_SECONDS, 0));
    assert!(!suggest::pulses(42.0, 1), "one closed segment is a position");
    assert!(!suggest::pulses(42.0, 0), "a placed line does not pulse either");

    // The status the page shows before then is the pulsing one, and it names no seconds.
    assert!(!suggest::THINKING_STATUS.contains(':'));
    // Which is not the mute log's prefix — two different `>>>` streams on one page.
    let mute_log = preview::sound_change(false, true, preview::MuteReason::NotHeard).unwrap();
    assert_ne!(mute_log.trim(), suggest::THINKING_STATUS);
}

/// §05-cut#8-details-confirmed-against-the-code-verification-pass — `segments with no recording at either end
/// dropped before the length is judged`, and the log line naming how many on which attempt.
#[test]
fn sec_05_cut_8_details_confirmed_against_the_code_verification_pass_s3_unfilmed_segments_go_first() {
    let filmed = runs();
    // Both ends inside recordings; one end in the hole; both ends in it; one past every recording.
    let answer = vec![(20.0, 30.0), (35.0, 45.0), (45.0, 55.0), (200.0, 210.0)];
    let (kept, dropped) = suggest::drop_unfilmed(answer.clone(), &filmed);
    assert_eq!(kept, vec![(20.0, 30.0)], "only the segment filmed at both ends survives");
    assert_eq!(dropped, answer.len() - kept.len());

    // A length judged on a void segment would be argued about; dropping it first cannot be, so the count is
    // what reaches the log: attempt 2 of three.
    assert_eq!(
        suggest::dropped_log(2, dropped),
        ">>> suggest attempt 2: 3 segment(s) dropped for having no footage"
    );
    assert_eq!(suggest::dropped_log(1, 0), ">>> suggest attempt 1: 0 segment(s) dropped for having no footage");

    // Nothing unfilmed, nothing said: an empty drop is not a fault.
    let (all_kept, none) = suggest::drop_unfilmed(vec![(70.0, 80.0)], &filmed);
    assert_eq!(all_kept.len(), 1);
    assert_eq!(none, 0);
    // A recording's first second is filmed; its last one is not — the same half-open reading as the band.
    let (edge, _) = suggest::drop_unfilmed(vec![(120.0, 130.0)], &filmed);
    assert!(edge.is_empty(), "the run ends at 120");
}

/// §05-cut#8-details-confirmed-against-the-code-verification-pass — `the checks answer with every fault at
/// once, worst first, joined by "; "`, and the past-the-end fault showing the conversion on the model's own
/// number.
#[test]
fn sec_05_cut_8_details_confirmed_against_the_code_verification_pass_s3_every_fault_at_once_worst_first() {
    // The example §8 spells out: 2804 read as a stamp is 28*60+04.
    assert_eq!(suggest::past_end_fault(2804), "2804 is not a second: a stamp [28:04] is mm*60+ss, 1684");
    // And one whose seconds need no padding still shows both numbers.
    assert_eq!(suggest::past_end_fault(905), "905 is not a second: a stamp [9:05] is mm*60+ss, 545");

    let faults = vec![
        suggest::Fault::Short(0.5),
        suggest::Fault::PastEnd(2804),
        suggest::Fault::TooMany(60),
    ];
    let ordered = suggest::worst_first(faults.clone());
    assert_eq!(ordered[0], suggest::Fault::PastEnd(2804), "out of range leads");
    assert_eq!(ordered[1], suggest::Fault::TooMany(60), "then a count");
    assert_eq!(ordered[2], suggest::Fault::Short(0.5), "then a length");

    let said = suggest::answer(&faults);
    // Every fault at once — nothing held back for another round trip.
    assert!(said.contains("is not a second: a stamp"), "{said}");
    assert!(said.contains("over the 59 this cut allows"), "{said}");
    assert!(said.contains("under the shortest stretch this cut keeps"), "{said}");
    // Joined by "; ", one per fault.
    assert_eq!(said.matches("; ").count(), faults.len() - 1, "{said}");
    assert!(said.starts_with("2804 is not a second"), "worst first: {said}");

    // One fault answers alone; none answers with nothing rather than an empty apology.
    assert_eq!(suggest::answer(&[suggest::Fault::PastEnd(705)]), suggest::past_end_fault(705));
    assert_eq!(suggest::answer(&[]), "");
}

/// §05-cut#8-details-confirmed-against-the-code-verification-pass — `a speed/rate on a segment is a speed
/// effect over it, exempt from any decorations cap`.
#[test]
fn sec_05_cut_8_details_confirmed_against_the_code_verification_pass_s3_a_rate_is_not_a_decoration() {
    assert!(suggest::is_speed_effect(EffectKind::Speed));
    for other in [EffectKind::Zoom, EffectKind::Text, EffectKind::Svg, EffectKind::Volume, EffectKind::Label] {
        assert!(!suggest::is_speed_effect(other), "{other:?} is a decoration");
    }
    // Exempt exactly where it is a speed: the cap counts what the decoration pass adds, not how long the cut
    // decided to run.
    assert!(suggest::exempt_from_decorations_cap(EffectKind::Speed));
    assert!(!suggest::exempt_from_decorations_cap(EffectKind::Text));
    assert!(!suggest::exempt_from_decorations_cap(EffectKind::Volume));

    // The names the model writes, read through cut.rs rather than re-parsed here: `speed`, and a stop, which is
    // the same kind at rate 0.
    let speed = naivepost::cut::Fx { kind: "speed".into(), t: 5.0, ..Default::default() };
    let stop = naivepost::cut::Fx { kind: "stop".into(), t: 5.0, ..Default::default() };
    assert!(suggest::exempt_from_decorations_cap(speed.effect_kind().expect("speed is a kind")));
    assert!(suggest::exempt_from_decorations_cap(stop.effect_kind().expect("a stop is a speed")));
    let caption = naivepost::cut::Fx { kind: "text".into(), t: 5.0, ..Default::default() };
    assert!(!suggest::exempt_from_decorations_cap(caption.effect_kind().expect("text is a kind")));
}

/// §05-cut#8-details-confirmed-against-the-code-verification-pass — `Streamed progress everywhere = objects
/// closed inside the text's last "<key>": [ (braces inside strings ignored) and the last closed object's end`.
#[test]
fn sec_05_cut_8_details_confirmed_against_the_code_verification_pass_s3_streamed_counts_closed_objects() {
    let whole = r#"{"segments": [{"start": 0.0, "end": 12.5}, {"start": 20.0, "end": 31.0}]}"#;
    assert_eq!(suggest::streamed(whole, "segments"), Some((2, Some(31.0))));

    // Truncated mid-answer: two closed, the third still arriving.
    let half = r#"{"segments": [{"start": 0.0, "end": 12.5}, {"start": 20.0, "end": 31.0}, {"start": 40.0"#;
    assert_eq!(suggest::streamed(half, "segments"), Some((2, Some(31.0))));

    // A brace inside a string value is text and closes nothing; an escaped quote does not end the string.
    let tricky = r#"{"segments": [{"start": 0.0, "end": 5.0, "why": "he said \"then }\" twice {"}]}"#;
    assert_eq!(suggest::streamed(tricky, "segments"), Some((1, Some(5.0))));

    // The LAST `"<key>": [` is the one that counts — a re-issued answer replaces what came before it.
    let twice = r#"{"segments": [{"start": 0.0, "end": 9.0}], "why": "first try",
        "segments": [{"start": 1.0, "end": 2.0}, {"start": 3.0, "end": 44.0}]}"#;
    assert_eq!(suggest::streamed(twice, "segments"), Some((2, Some(44.0))));

    // An object that closed without naming `end` leaves the last known one standing.
    let no_end = r#"{"segments": [{"start": 0.0, "end": 7.0}, {"start": 9.0, "end": 80.5}]}"#;
    assert_eq!(suggest::streamed(no_end, "segments"), Some((2, Some(80.5))));
    let no_ends = r#"{"segments": [{"start": 0.0}, {"start": 9.0}]}"#;
    assert_eq!(suggest::streamed(no_ends, "segments"), Some((2, None)));

    // No array opened yet: there is no progress to place, which is what keeps the page pulsing under
    // `THINKING_STATUS` rather than jumping to the floor.
    // Opened and never closed, with no object finished inside it: still thinking, nothing placed. The page
    // keeps pulsing under THINKING_STATUS rather than jumping to a number of its own — there is no `]` to
    // notice the emptiness yet, which is exactly why "nothing closed" has to be the answer here.
    assert_eq!(suggest::streamed(r#"{"thinking": "reading the session", "segments": ["#, "segments"), Some((0, None)));
    // Closed but empty — the model answered with no segment at all, which is also nothing to place.
    assert_eq!(suggest::streamed(r#"{"thinking": "looked it over", "segments": []}"#, "segments"), None);
    assert_eq!(suggest::streamed("still thinking", "segments"), None);
}
