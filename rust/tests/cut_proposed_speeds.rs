//! F3.10 Speeds proposed by the model (after the cut) — `spec/06-effects.md` F3.10, steps S1–S5.
//!
//! One call over the whole cut asks for a rate per clip; what these tests pin is everything the model does not decide:
//! that the brief says how much footage there is and which clips carry captions (so they run at 1), that a fast rate on
//! a captioned clip is refused *for that call* while its neighbours still land, that a rate of about 1 or no rate at all
//! is silence rather than an effect, that a real rate covers the whole clip at the CLAMPED rate with only the ramps the
//! render can build (P.eng.rampStepSeconds decides what those are), that same-rate stretches nearer than
//! P.policy.speedGapSeconds become one run, and the two lines the log gets — `>>> speed: …` and `!!! speed: …`, both
//! with an em dash. Every float is derived on paper from the rule it tests and compared through a tolerance; clips are
//! ten seconds each so those derivations stay exact. No widget and no display: `rust/src/ui/window.rs` renders only
//! Prepare, so there is no ▶ to press yet.

use naivepost::cut::Fx;
use naivepost::cut_captions;
use naivepost::cut_speed_pass::{self as pass, Answer, Call};
use naivepost::params;
use naivepost::tools::clips::Clips;

#[allow(dead_code)] // every test binary compiles this whole module
mod common;
use common::{row};

/// Floats from a rule, never `assert_eq!`: this flow's numbers are all quotients and differences.
const EPS: f64 = 1e-9;

fn assert_close(asked: &str, got: f64, want: f64) {
    assert!((got - want).abs() < EPS, "{asked}: got {got}, want {want}");
}

/// One row of the catalogue, by id. `params::find` answers Prepare's list only, and this flow's rows live in §6's
/// (`params::cut`), so look through both — and assert exactly one hit, since two homes for one bound is what the
/// catalogue exists to prevent.
/// The cut S1–S6 argue about: five clips of ten seconds at their session seconds. Ten is exact in binary and so are
/// the multiples, which is what lets every expected number below be derived by hand.
fn five_clips() -> Clips {
    Clips::new(&[(1, 0.0, 10.0), (2, 10.0, 10.0), (3, 20.0, 10.0), (4, 30.0, 10.0), (5, 40.0, 10.0)])
}

fn call(clip: u32, rate: f64) -> Call {
    Call { clip, rate }
}

/// A speed effect for the merge rule, which is about stretches rather than about clips.
fn speed(t: f64, dur: f64, rate: f64, tout: f64) -> Fx {
    Fx { kind: "speed".into(), t, dur, rate, tout, ..Default::default() }
}

// --- S1: the one call -------------------------------------------------------------------------------------------------

/// S1 (`one call · the captions and the footage total`; briefs note captions `…, N caption(s) -- runs at 1`): the whole
/// request for the entire cut in one message — footage first, then each clip with its own seconds and, on a clip that
/// has captions, the promise that it runs at 1.
#[test]
fn f3_10_s1_one_call_over_the_whole_cut() {
    // P.policy.speedGapSeconds: four seconds of ordinary footage is far enough apart to read as two runs. Catalogued
    // from the pass that folds stretches; params' `num` drops the `.0`, so §10's 4.0 is spelled "4".
    assert_eq!(pass::GAP_SECONDS, 4.0);
    let gap = row("P.policy.speedGapSeconds");
    assert_eq!(gap.spelled, "4");
    assert_eq!(gap.from, "cut_speed_pass::GAP_SECONDS");
    assert_eq!(params::family("P.policy.speedGapSeconds"), params::Family::Policy);

    let clips = five_clips();
    // The footage total is the clips' own seconds, before any rate shortens them: 5 × 10.0.
    assert_close("five clips of ten seconds", pass::footage_total(&clips), 50.0);

    let text = pass::brief(&clips, pass::footage_total(&clips));
    // The footage line first — "longer dull stretch → higher rate" needs a denominator — then the clip list under the
    // app's one header (reused from §F3.9 rather than restated with this pass's own spelling of it).
    assert!(text.starts_with("FOOTAGE: 50 seconds over 5 clips.\n\n"), "{text}");
    let at = text.find(cut_captions::CLIPS_HEADER).expect("the header opens the clip list");
    let first_clip = text.find("CLIP 1:").expect("the first clip");
    assert!(at < first_clip, "{text}: a clip line precedes the header");
    let lines: Vec<&str> = text.lines().skip(2).collect();
    assert_eq!(
        lines,
        [
            cut_captions::CLIPS_HEADER,
            "CLIP 1: 10.0 s long",
            "CLIP 2: 10.0 s long",
            "CLIP 3: 10.0 s long",
            "CLIP 4: 10.0 s long",
            "CLIP 5: 10.0 s long",
        ]
    );

    // A clip carrying captions says so, in the brief's own words: comma, count, `--`, and what that means for its rate.
    let mut captioned = five_clips();
    captioned.add_caption(1, 1.0, 3.0, "first line");
    captioned.add_caption(1, 4.0, 6.0, "second line");
    let shown = pass::brief(&captioned, pass::footage_total(&captioned));
    assert!(shown.contains("CLIP 1: 10.0 s long, 2 caption(s) -- runs at 1"), "{shown}");
    // A clip with nothing on it carries no note — the count is per clip, not a summary line.
    assert!(shown.contains("\nCLIP 2: 10.0 s long\n"), "{shown}");
}

// --- S2: rate > 1 on a captioned clip ------------------------------------------------------------------------------------

/// S2 (`rate over 1 on a captioned clip? yes → refused`): the refusal is about that call and not about the reply — the
/// words on screen would be unreadable, so the clip keeps its own speed while everything else in the same answer lands.
#[test]
fn f3_10_s2_a_captioned_clip_is_refused_not_dropped() {
    let mut clips = five_clips();
    clips.add_caption(2, 1.0, 4.0, "words on screen");

    let reply = pass::place(&clips, &[call(2, 4.0)]);
    assert_eq!(reply.fault, None, "a refusal is not a fault: nothing here asks for a retry");
    assert!(reply.effects.is_empty(), "a refusal places nothing: {:?}", reply.effects);
    let Answer::Refused(reason) = &reply.answers[0] else {
        panic!("a fast rate over captions was not refused: {:?}", reply.answers[0]);
    };
    // §3.7's tool's own sentence, said back rather than paraphrased: the model learns what to do about it.
    assert!(reason.contains("cannot be sped up"), "{reason}");
    assert!(reason.contains("clip 2"), "{reason}");

    // One refusal does not lose the reply — contrast F3.9, where a clip number outside the batch throws the whole
    // answer away. Here the other call in the same reply is placed as normal.
    let mixed = pass::place(&clips, &[call(2, 4.0), call(1, 2.0)]);
    assert_eq!(mixed.fault, None);
    assert_eq!(mixed.effects.len(), 1, "{:?}", mixed.effects);
    assert_eq!(mixed.effects[0].t, 0.0, "the effect that survived is clip 1's");
    assert!(matches!(mixed.answers[0], Answer::Refused(_)));
    assert!(matches!(mixed.answers[1], Answer::Placed(_)));

    // Only the FAST half is refused: a caption over a slowed clip is on screen longer rather than shorter, so slow
    // motion over words is allowed and placed.
    let slow = pass::place(&clips, &[call(2, 0.5)]);
    assert!(matches!(slow.answers[0], Answer::Placed(_)), "{:?}", slow.answers[0]);
    assert_eq!(slow.effects.len(), 1);
    assert_close("the asked-for slow rate stands", slow.effects[0].rate, 0.5);

    // And the caption count in the brief is the same fact this refusal enforces — said first, enforced after.
    assert!(pass::brief(&clips, pass::footage_total(&clips)).contains("CLIP 2: 10.0 s long, 1 caption(s) -- runs at 1"));
}

// --- S3: rate ≈ 1 -------------------------------------------------------------------------------------------------------

/// S3 (`rate ≈ 1? yes → ignored`): a rate that is the footage's own speed says nothing, and neither does one nobody
/// sent. Both are silence — no effect, no refusal, and no tool call to argue with.
#[test]
fn f3_10_s3_a_rate_near_1_is_ignored() {
    // effects.neutralRateTolerance: a hundredth either side of 1 is rounding from a model that meant "leave it alone".
    assert_eq!(pass::NEUTRAL_TOLERANCE, 0.01);
    assert!(pass::is_neutral(1.0));
    assert!(pass::is_neutral(1.0 - 0.005), "just under");
    assert!(pass::is_neutral(1.0 + 0.005), "just over");
    // Outside the window in both directions is a decision: 2 % off is slow motion or speed-up, and 0 is no rate at all
    // rather than something near 1.
    assert!(!pass::is_neutral(1.02));
    assert!(!pass::is_neutral(0.98));
    assert!(!pass::is_neutral(0.0));

    let clips = five_clips();
    let reply = pass::place(&clips, &[call(1, 1.0), call(2, 1.004)]);
    assert!(reply.usable());
    assert!(reply.effects.is_empty(), "{:?}", reply.effects);
    assert!(reply.answers.iter().all(|a| *a == Answer::Ignored), "{:?}", reply.answers);

    // A rate nobody sent arrives as 0 — an omitted JSON number and a 0 are the same thing, and reading that as a freeze
    // would hand a still to every reply that forgot to name a speed. So: no effect, and specifically no ×0 record.
    let omitted = pass::place(&clips, &[call(3, 0.0)]);
    assert!(omitted.effects.is_empty(), "{:?}", omitted.effects);
    assert_eq!(omitted.answers, vec![Answer::Ignored]);
    assert!(!omitted.effects.iter().any(|fx| fx.rate == 0.0), "an omitted rate must not become a stop");

    // The two numbers this flow adds to the catalogue, both read from the pass rather than restated. §10 gives them no
    // `P.` row, so they carry the bare prefix of the rule that reads them.
    let ramp = row("effects.proposedRampSeconds");
    assert_eq!(ramp.spelled, "1");
    assert_eq!(ramp.from, "cut_speed_pass::PROPOSED_RAMP_SECONDS");
    let neutral = row("effects.neutralRateTolerance");
    assert_eq!(neutral.spelled, "0.01");
    assert_eq!(neutral.from, "cut_speed_pass::NEUTRAL_TOLERANCE");
    assert_eq!(params::family("effects.neutralRateTolerance"), params::Family::Other);
}

// --- S4: the effect, its fades, and the rate that was actually applied ----------------------------------------------------

/// S4 (`a speed effect over the clip`, ramp `min(1, d/4)`): a proposed speed covers its clip's whole span at the clip's
/// own session second, with only the ramps F3.3 says can be built, at the rate §3.7's tool reports rather than the one
/// that was asked for.
#[test]
fn f3_10_s4_the_effect_is_the_clip_and_the_ramp_may_be_refused_by_arithmetic() {
    // Positions the caller supplies, not a sum of lengths: clip 2 sits at 70.0 whatever clip 1's length is.
    let clips = Clips::new(&[(1, 60.0, 10.0), (2, 70.0, 30.0)]);
    let reply = pass::place(&clips, &[call(2, 2.0)]);
    assert_eq!(reply.effects.len(), 1);
    let fx = &reply.effects[0];
    assert_eq!(fx.kind, "speed");
    assert_close("the clip's own session second", fx.t, 70.0);
    assert_close("the clip's whole span", fx.dur, 30.0);

    // The ramp: `min(1, d/4)` asked, then refused if it cannot be built. For d = 30 that is min(1, 7.5) = 1.0 each way,
    // and at ×2 the least payable stair is P.eng.rampStepSeconds · √2 ≈ 0.85 < 1.0, so both edges keep their ramp.
    let (trans, tout) = pass::ramp(2.0, 30.0);
    assert_close("a ×2 run ramps in for a second", trans, 1.0);
    assert_close("and out for the same", tout, 1.0);
    assert_close("the placed effect carries them", fx.trans, 1.0);
    assert_close("both edges alike", fx.tout, 1.0);

    // P.eng.rampStepSeconds = 0.6 decides the other case: at ×4 the least stair is 0.6 · √4 = 1.2 s, and the ramp asked
    // for can never exceed `min(1, d/4)` ≤ 1.0 — so NO proposed ×4 speed anywhere gets a ramp. Arithmetic, not taste.
    assert!((pass::ramp(4.0, 30.0)).0 == 0.0);
    assert!((pass::ramp(4.0, 30.0)).1 == 0.0);
    // A short clip loses the ramp for the other reason: d/4 = 0.5 asked, and at ×2 that clears 0.85? No — it does not,
    // so a two-second clip at ×2 is a hard change of speed too.
    assert!((pass::ramp(2.0, 2.0)).0 == 0.0);

    // The rate stored is the one §3.7 CLAMPS to, not the one typed: P.policy.maxSpeedRate caps a cut at ×4 ("eight is
    // for a minute of loading screen"), so an asked-for ×100 over ten seconds comes back as ×4 — 2.5 s on screen, well
    // clear of P.eng.minClipSeconds' 0.5. A record holding the asked number would promise seconds the render never
    // makes, and the lane would show a rate nothing plays at.
    let fast = pass::place(&clips, &[call(1, 100.0)]);
    assert_eq!(fast.effects.len(), 1);
    assert_close("the clamp moved it to the ceiling", fast.effects[0].rate, 4.0);
    // …and its fades are the ones THAT rate can afford: at ×4 the least stair is P.eng.rampStepSeconds · √4 = 1.2 s,
    // more than any proposed ramp (min(1, d/4) ≤ 1), so a clamped speed arrives with hard edges.
    assert_close("no ramp at that rate", fast.effects[0].trans, 0.0);
    assert_close("neither way", fast.effects[0].tout, 0.0);

    // A speed effect needs no box and no words: it is the same record ⏩ Speed writes by hand (§F3.3), which is why the
    // lane, the preview and the render need no second path for a proposed one.
    assert_eq!((fx.cx, fx.cy, fx.hf, fx.wf), (None, None, None, None));
    assert_eq!(fx.text, "");
    assert_eq!(fx.ease, "");
}

// --- S4b: same-rate stretches nearer than the gap ------------------------------------------------------------------------

/// S4 (`same-rate stretches nearer than P.policy.speedGapSeconds merged`): two runs at one rate with less than four
/// seconds of ordinary footage between them are one run — the island of normal speed in the middle is what reads as a
/// hiccup. The bound is "nearer than", so exactly four seconds stays two runs.
#[test]
fn f3_10_s5_same_rate_stretches_nearer_than_the_gap_merge() {
    // Gap 12.0 - (0.0 + 10.0) = 2.0 < 4.0 → one stretch from 0.0 to max(10.0, 22.0) = 22.0, and the fade out is the
    // later stretch's (the cut ends where the last run ends, with its ramp).
    let merged = pass::merge(&[speed(0.0, 10.0, 2.0, 0.5), speed(12.0, 10.0, 2.0, 0.75)]);
    assert_eq!(merged.len(), 1, "{merged:?}");
    assert_close("the run starts at the first", merged[0].t, 0.0);
    assert_close("and ends where the later one does", merged[0].dur, 22.0);
    assert_close("the fade out is the later stretch's", merged[0].tout, 0.75);
    assert_close("one rate still", merged[0].rate, 2.0);

    // Exactly the gap does NOT merge: 14.0 - 10.0 = 4.0, and `nearer than` is `<`, not `≤`.
    let exact = pass::merge(&[speed(0.0, 10.0, 2.0, 0.5), speed(14.0, 10.0, 2.0, 0.5)]);
    assert_eq!(exact.len(), 2, "{exact:?}");

    // A hair over it either: four seconds and a tenth of a nanosecond is still not nearer than four seconds. (The
    // float risk runs the other way — arithmetic can land a merge just under — which is why the app's seconds come from
    // word edges rather than from sums.)
    let hair = pass::merge(&[speed(0.0, 10.0, 2.0, 0.5), speed(14.0000000001, 10.0, 2.0, 0.5)]);
    assert_eq!(hair.len(), 2, "{hair:?}");

    // Two rates two seconds apart are two decisions, not one: merging them would silently pick a rate for the second
    // stretch.
    let rates = pass::merge(&[speed(0.0, 10.0, 2.0, 0.5), speed(12.0, 10.0, 4.0, 0.5)]);
    assert_eq!(rates.len(), 2, "{rates:?}");

    // Out of order input is still one run (the fold sorts the speeds it folds), and everything that is not a speed
    // survives untouched and in the order it arrived — a caption must not be shuffled against the zoom it was written
    // for.
    let text = Fx { kind: "text".into(), t: 5.0, dur: 2.0, text: "words".into(), ..Default::default() };
    let mixed = pass::merge(&[speed(12.0, 10.0, 2.0, 0.75), text.clone(), speed(0.0, 10.0, 2.0, 0.5)]);
    assert_eq!(mixed.len(), 2, "{mixed:?}");
    assert_close("the two runs folded", mixed[0].dur, 22.0);
    assert_eq!(mixed[1], text, "the caption came back changed: {:?}", mixed[1]);
    assert_eq!((mixed[1].kind.as_str(), mixed[1].t), ("text", 5.0));

    // Nothing in, nothing out.
    assert!(pass::merge(&[]).is_empty());
}

// --- S5: the two lines, and the budget of two rounds ------------------------------------------------------------------------

/// S5 (`Log ">>> speed: N clip(s) run fast — a of video from b of footage"` / `two rounds fail → "!!! speed: no usable
/// answer — every clip plays at 1"`): both with an EM dash (U+2014), verified against §F3.10's bytes — the opposite of
/// F3.9's fallback line, which spells its break `--`. And the reason a failed pass is survivable: every clip plays at 1.
#[test]
fn f3_10_s6_the_two_lines_and_the_two_round_budget() {
    let clips = five_clips();

    // Derived on paper: clips 2 and 3 run at ×2, so the video is 10 + 10/2 + 10/2 + 10 + 10 = 40.0 s of the 50.0 s of
    // footage. Both clock times go through tools::mm_ss, which PADS — `00:40`, not the prototype's unpadded `0:40` —
    // because that is how every other status line in this app prints a second.
    let reply = pass::place(&clips, &[call(2, 2.0), call(3, 2.0)]);
    assert_eq!(reply.effects.len(), 1, "the two runs are nearer than the gap, so they merge");
    assert_eq!(
        pass::runs_fast(&clips, &reply.effects, pass::footage_total(&clips)),
        format!(">>> speed: 2 clip(s) run fast \u{2014} {} of video from {} of footage", "00:40", "00:50")
    );
    let line = pass::runs_fast(&clips, &reply.effects, pass::footage_total(&clips));
    assert!(line.contains('\u{2014}'), "{line}: no em dash");
    assert!(!line.contains("--"), "{line}: this line's break is an em dash, not --");
    // N counts CLIPS running fast — the merged pair is two clips, which is what the sentence says.
    assert!(line.starts_with(">>> speed: 2 clip(s) run fast"), "{line}");

    // A reply that proposes nothing is a right answer to a cut with nothing dull in it: no length gate, and the line
    // still prints, at zero, with the video as long as the footage.
    let none = pass::place(&clips, &[call(1, 1.0)]);
    assert!(none.usable(), "an empty answer is not a fault");
    assert!(none.effects.is_empty());
    assert_eq!(
        pass::runs_fast(&clips, &none.effects, pass::footage_total(&clips)),
        ">>> speed: 0 clip(s) run fast \u{2014} 00:50 of video from 00:50 of footage"
    );

    // The giving-up line, whole, with its promise.
    assert_eq!(pass::no_answer(), "!!! speed: no usable answer \u{2014} every clip plays at 1");
    assert!(!pass::no_answer().contains("--"), "{}", pass::no_answer());

    // Two rounds and no more, read through the budget F3.9 already owns: one attempt made owes a retry, two do not.
    assert!(cut_captions::retries(1));
    assert!(!cut_captions::retries(2));
    // What is logged between them: the fault said back before the same call goes out again.
    assert_eq!(pass::rejected("bad json"), ">>> speed rejected: bad json");

    // The whole flow for a reply that names a clip nobody gave: unusable, so no effects and no tool calls; retried once;
    // then dropped with one line — and the cut left at 1 throughout, which is a longer video and a whole one.
    let hopeless = [call(9, 2.0)];
    let mut log: Vec<String> = Vec::new();
    let effects: Vec<Fx>;
    let mut runs = 0u32;
    loop {
        runs += 1;
        let reply = pass::place(&clips, &hopeless);
        assert_eq!(
            reply.fault,
            Some("clip 9 is not one of the clips given (1 to 5)".to_string()),
            "the refusal names the whole cut's range"
        );
        assert!(reply.effects.is_empty(), "an unusable reply places nothing");
        // The fault is said back, and the same call goes out again — until the budget says otherwise. `retries` counts
        // attempts MADE (one owed retry after one attempt, none after two), so the second round is the last ask and its
        // rejection is reported by the giving-up line rather than by a second `>>> speed rejected:`.
        if cut_captions::retries(runs) {
            log.push(pass::rejected(reply.fault.as_deref().unwrap()));
            continue;
        }
        log.push(pass::no_answer());
        effects = reply.effects;
        break;
    }
    assert_eq!(runs, 2, "two rounds and no more");
    assert_eq!(log, vec![pass::rejected("clip 9 is not one of the clips given (1 to 5)"), pass::no_answer()]);
    assert!(effects.is_empty(), "every clip plays at 1: {effects:?}");
}
