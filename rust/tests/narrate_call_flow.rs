// F4.2 S3/S4/S5 — the ATTEMPT LOOP, `naivepost::narrate_call::drive`. The message pair is pinned in
// tests/narrate_call.rs (s1..s15); what is tested here is only what the loop adds: the forward-only
// match against echoed bounds, the refusal of a reply that leaves a clip unanswered or says nothing at
// all, thinking being switched off after a call that wrote nothing, the bar counting only closed clips
// and never going backwards, giving up after three attempts, and never answering from the reply cache.
//
// No display, no pipeline, no server: every answer comes from a closure, and every claim is read back
// out of the values and callback observations below rather than from anything printed.

use naivepost::cut::{Fx, Seg};
use naivepost::narrate_call::drive;
use naivepost::narrate_pass as pass;
use naivepost::narrate_reply::{
    self, ENTRY_MATCH_TOLERANCE_SECONDS, all_silent_fault, clip_without_entry, unmatched_entry_fault,
};
use naivepost::textfmt::SessionLine;

#[allow(dead_code)] // every test binary compiles this whole module
mod common;
use common::{seg};

/// The transcript's own lane name: what `session.tsv` writes in the source column.
const MIC: &str = "2026-09-16 17-26-20";

fn row(start: f64, end: f64, source: &str, who: &str, text: &str) -> SessionLine {
    SessionLine { start, end, source: source.into(), who: who.into(), text: text.into() }
}

fn effect(kind: &str, t: f64, dur: f64, text: &str) -> Fx {
    Fx { kind: kind.into(), t, dur, text: text.into(), ..Default::default() }
}

/// One entry in the prototype's reply shape, with the bounds the model echoed.
fn entry(start: f64, end: f64, at: f64, text: &str, emotion: &str) -> String {
    format!(
        "{{\"start\":{start},\"end\":{end},\"at\":{at},\"text\":\"{text}\",\"emotion\":\"{emotion}\"}}"
    )
}

fn entries(list: &[String]) -> String {
    format!("{{\"entries\":[{}]}}", list.join(","))
}

/// A two-clip cut: the batch every test below asks about.
fn two_clips() -> Vec<Seg> {
    vec![seg(100.0, 120.0), seg(120.0, 130.0)]
}

/// Nothing unfitted: these tests are about matching and retrying, not about F4.3's fit arithmetic, so
/// the caller hands in an empty list and `finish` answers on completeness alone.
fn everything_fits(_: &[naivepost::narrate_run::Written]) -> Vec<u32> {
    Vec::new()
}

/// s16 — the tolerance is `ENTRY_MATCH_TOLERANCE_SECONDS` and nothing else: an echo 0.4 s off a clip
/// still lands on it, while 0.6 s off matches no clip and is refused with the fault's exact wording.
#[test]
fn f4_2_s16_an_echo_within_the_tolerance_lands_and_one_outside_it_is_refused_by_name() {
    // 0.4 is inside 0.5; both bounds of clip 1 shift together so only the closeness varies.
    assert!(0.4 <= ENTRY_MATCH_TOLERANCE_SECONDS, "the near miss is inside the bound");
    assert!(0.6 > ENTRY_MATCH_TOLERANCE_SECONDS, "the far miss is outside it");

    let clips = two_clips();
    let near = entries(&[
        entry(99.6, 119.6, 1.0, "close enough", "calm"),
        entry(120.0, 130.0, 0.0, "second clip", "calm"),
    ]);
    let got = drive(
        &clips,
        |_, _| Ok(near.clone()),
        |_, _| {},
        &everything_fits,
        |_| {},
    );
    let written = got.expect("a 0.4 s echo matches its clip");
    assert_eq!(written.len(), 2);
    // What is stored is the CLIP's own bounds, not the echoed ones.
    assert_eq!((written[0].start, written[0].end), (100.0, 120.0));
    assert_eq!(written[0].at, 1.0);

    let far = entries(&[entry(99.4, 119.4, 1.0, "too far", "calm")]);
    let refused = drive(
        &clips,
        |_, _| Ok(far.clone()),
        |_, _| {},
        &everything_fits,
        |_| {},
    )
    .expect_err("a 0.6 s echo matches nothing");
    assert_eq!(refused, unmatched_entry_fault(99.4, 119.4));
    assert!(
        refused.contains("matches no clip (or is out of order)"),
        "the refusal names the echoed bounds, verbatim: {refused}"
    );
}

/// s17 — forward-only. Two entries for clips 1 and 2 arrive in order and land in play order; a third
/// replaying clip 1 after clip 2 is REFUSED, not quietly re-sorted, because silently reordering lines is
/// how a narration ends up spoken out of order.
#[test]
fn f4_2_s17_entries_are_matched_forward_only_and_a_replay_of_a_passed_clip_is_refused() {
    let clips = two_clips();
    let good = entries(&[
        entry(100.0, 120.0, 0.0, "first", "calm"),
        entry(120.0, 130.0, 2.0, "second", "happy"),
    ]);
    let written = drive(&clips, |_, _| Ok(good.clone()), |_, _| {}, &everything_fits, |_| {})
        .expect("in-order entries land");
    assert_eq!(
        written.iter().map(|w| w.start).collect::<Vec<_>>(),
        vec![100.0, 120.0],
        "the written order is the play order"
    );

    // Same two, then a walk back to clip 1: the cursor has already passed it.
    let walked_back = entries(&[
        entry(100.0, 120.0, 0.0, "first", "calm"),
        entry(120.0, 130.0, 2.0, "second", "happy"),
        entry(100.0, 120.0, 5.0, "again", "angry"),
    ]);
    let refused = drive(&clips, |_, _| Ok(walked_back.clone()), |_, _| {}, &everything_fits, |_| {})
        .expect_err("a replay of a passed clip cannot be placed");
    assert_eq!(refused, unmatched_entry_fault(100.0, 120.0));
}

/// s18 — thinking. A first reply that wrote NOTHING switches thinking off for attempt 2; a reply that
/// wrote some keeps it on. Read through the bool the fake `reply` closure receives, which is exactly what
/// the next request would carry.
#[test]
fn f4_2_s18_a_reply_that_wrote_nothing_switches_thinking_off_for_the_next_attempt() {
    // Case A: attempt 1 is unparseable, so it wrote nothing -> attempt 2 goes out without thinking.
    let mut seen: Vec<(u32, bool)> = Vec::new();
    let ok_two = entries(&[
        entry(100.0, 120.0, 0.0, "first", "calm"),
        entry(120.0, 130.0, 0.0, "second", "calm"),
    ]);
    let got = drive(
        &two_clips(),
        |attempt, thinking| {
            seen.push((attempt, thinking));
            if attempt == 1 {
                Ok("not json at all".to_string())
            } else {
                Ok(ok_two.clone())
            }
        },
        |_, _| {},
        &everything_fits,
        |_| {},
    );
    assert!(got.is_ok());
    assert_eq!(seen[0], (1, true), "the first attempt thinks (§F4.2: Thinking on)");
    assert_eq!(seen[1], (2, false), "an attempt that wrote nothing does not get to think again");

    // Case B: attempt 1 places one line but leaves clip 2 open, so it wrote SOMETHING -> thinking stays.
    let mut kept: Vec<bool> = Vec::new();
    let partial = entries(&[entry(100.0, 120.0, 0.0, "only one", "calm")]);
    let _ = drive(
        &two_clips(),
        |attempt, thinking| {
            kept.push(thinking);
            if attempt == 1 {
                Ok(partial.clone())
            } else {
                Ok(ok_two.clone())
            }
        },
        |_, _| {},
        &everything_fits,
        |_| {},
    );
    assert_eq!(kept, vec![true, true], "a call that wrote a line was not held back by reasoning");
}

/// s19 — three unusable replies give up. The count is `pass::attempts()` and the error names the last
/// problem, so the log says what broke rather than just that something did.
#[test]
fn f4_2_s19_three_unusable_replies_give_up_naming_the_last_problem() {
    assert_eq!(pass::attempts(), roles_attempts(), "one attempt count for every LLM job");
    let mut calls = 0u32;
    // Each reply echoes bounds that match no clip, so the refusal names them: attempt 3's echo is
    // `0-4`, which is what the run must end on rather than attempt 1's `0-2`.
    let got = drive(
        &two_clips(),
        |attempt, _| {
            calls += 1;
            Ok(entries(&[entry(0.0, 1.0 + attempt as f64, 0.0, "nothing", "calm")]))
        },
        |_, _| {},
        &everything_fits,
        |_| {},
    );
    let err = got.expect_err("the run gives up with nothing written");
    assert_eq!(calls, pass::attempts(), "exactly the allowed number of asks, no more");
    assert!(
        err.contains("0-4") && !err.contains("0-2"),
        "the error names the LAST rejection (attempt 3 echoed 0-4), not the first: {err}"
    );
}

/// s20 — the streaming bar counts only clips that actually closed and never moves backwards across a
/// retry: a second attempt that gets further starts from where the first stopped, not from zero.
#[test]
fn f4_2_s20_progress_counts_closed_clips_and_never_goes_backwards_across_a_retry() {
    let mut ticks: Vec<(u32, u32)> = Vec::new();
    let partial = entries(&[entry(100.0, 120.0, 0.0, "one closed", "calm")]);
    let full = entries(&[
        entry(100.0, 120.0, 0.0, "one closed", "calm"),
        entry(120.0, 130.0, 0.0, "two closed", "calm"),
    ]);
    let got = drive(
        &two_clips(),
        |attempt, _| Ok(if attempt == 1 { partial.clone() } else { full.clone() }),
        |written, total| ticks.push((written, total)),
        &everything_fits,
        |_| {},
    );
    assert!(got.is_ok());
    assert_eq!(
        ticks.iter().map(|t| t.1).collect::<Vec<_>>(),
        vec![2, 2, 2],
        "the total is always the whole batch"
    );
    let counted: Vec<u32> = ticks.iter().map(|t| t.0).collect();
    assert_eq!(counted, vec![1, 1, 2], "closed clips only, and the retry resumes at 1 rather than 0");
    let mut previous = 0u32;
    for value in &counted {
        assert!(*value >= previous, "the bar never falls: {counted:?}");
        previous = *value;
    }
}

/// s21 — silence must be SAID. A reply whose every line is empty is a model refusing the job and is
/// refused with `all_silent_fault`; a reply that skips a clip is refused with `clip_without_entry`,
/// naming that clip and its bounds. Neither writes a silent video.
#[test]
fn f4_2_s21_an_all_empty_reply_and_a_reply_with_a_hole_are_both_refused() {
    let clips = two_clips();
    let empty = entries(&[entry(100.0, 120.0, 0.0, "", ""), entry(120.0, 130.0, 0.0, "   ", "")]);
    let refused = drive(&clips, |_, _| Ok(empty.clone()), |_, _| {}, &everything_fits, |_| {})
        .expect_err("every line empty is a refusal, not taste");
    assert_eq!(refused, all_silent_fault());
    assert_eq!(refused, pass::no_line_fault(), "one string for the fault, wherever it is asked");

    let hole = entries(&[entry(120.0, 130.0, 0.0, "only the second", "calm")]);
    let refused = drive(&clips, |_, _| Ok(hole.clone()), |_, _| {}, &everything_fits, |_| {})
        .expect_err("a skipped clip fails the attempt");
    // Clip 1 is the one nobody answered; the forward walk put the second entry on clip 2.
    assert_eq!(refused, clip_without_entry(0, &clips));
    assert!(refused.contains("got no entry"), "it says so plainly: {refused}");
}

/// s22 — never served from the reply cache. Driving the same request twice asks the model twice, and
/// `served_from_cache` answers false: the expensive part is the voice, not the words, and the TTS cache
/// is the cache.
#[test]
fn f4_2_s22_the_same_request_driven_twice_asks_again_and_is_never_answered_from_cache() {
    assert!(!pass::served_from_cache(), "the writer is never cached (§F4.2)");
    let clips = two_clips();
    let script = entries(&[
        entry(100.0, 120.0, 0.0, "same brief", "calm"),
        entry(120.0, 130.0, 0.0, "same answer", "calm"),
    ]);
    let mut asked = 0u32;
    let first = drive(
        &clips,
        |_, _| {
            asked += 1;
            Ok(script.clone())
        },
        |_, _| {},
        &everything_fits,
        |_| {},
    )
    .expect("first drive writes");
    let second = drive(
        &clips,
        |_, _| {
            asked += 1;
            Ok(script.clone())
        },
        |_, _| {},
        &everything_fits,
        |_| {},
    )
    .expect("second drive writes too -- nothing was remembered");
    assert_eq!(asked, 2, "an identical request is asked again, not replayed from disk");
    assert_eq!(first, second, "and both drives produce the same lines");
}

/// The single attempt count lives in `roles`; asserted here so a retuned number moves this test rather
/// than passing quietly on either side of the change.
fn roles_attempts() -> u32 {
    naivepost::roles::LLM_ATTEMPTS
}
