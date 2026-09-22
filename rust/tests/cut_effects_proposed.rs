//! F3.11 Decorations proposed by the model (after the cut) — `spec/06-effects.md` F3.11, steps S1–S5.
//!
//! One call over every clip asks for zooms, a stop and some volume; what these tests pin is everything the model does
//! not decide: that the brief tells it which clips carry captions and which already run fast (S1), that a clip number
//! outside the list throws the whole reply away while an unknown kind or an empty span is skipped without touching the
//! rest (S2, S3), the app's own defaults for a decoration nobody sized by hand — zoom centred at 60 % height with glides
//! from P.policy.effectDefaultFades' zoom value, a stop as a rate-0 speed with this flow's 0.3 cap and P.policy.
//! effectDefaultSeconds as its asked-for length, volume ramps min(1, d/4) (S4), that a gain of 1 or none is silence while
//! a gain of 0 is kept (S4 again), and the three lines the log gets (S5). Every float is derived on paper from the rule it
//! tests and compared through a tolerance; clips are ten seconds each so those derivations stay exact. No widget and no
//! display: `rust/src/ui/window.rs` renders only Prepare, so there is no ▶ to press yet.

use naivepost::cut::Fx;
use naivepost::cut_captions;
use naivepost::cut_effects_pass::{self as pass, Answer, Call};
use naivepost::cut_speed;
use naivepost::fx_record;
use naivepost::params;
use naivepost::tools::clips::Clips;

/// Floats from a rule, never `assert_eq!`: this flow's numbers are all quotients.
const EPS: f64 = 1e-9;

fn assert_close(asked: &str, got: f64, want: f64) {
    assert!((got - want).abs() < EPS, "{asked}: got {got}, want {want}");
}

/// One row of the catalogue, by id. `params::find` answers Prepare's list only and this flow's rows live in §6's
/// (`params::cut`), so look through both — and assert exactly one hit, since two homes for one bound is what the
/// catalogue exists to prevent.
fn row(id: &str) -> params::Param {
    let mut found = params::prepare()
        .into_iter()
        .chain(params::cut())
        .filter(|row| row.id == id)
        .collect::<Vec<_>>();
    assert_eq!(found.len(), 1, "{id} catalogued {} times", found.len());
    found.pop().unwrap()
}

/// The cut S1–S6 argue about: five clips of ten seconds at their session seconds. Ten is exact in binary, and so are
/// the multiples, which is what lets every expected number below be derived by hand.
fn five_clips() -> Clips {
    Clips::new(&[(1, 0.0, 10.0), (2, 10.0, 10.0), (3, 20.0, 10.0), (4, 30.0, 10.0), (5, 40.0, 10.0)])
}

fn call(clip: u32, kind: &str, start: f64, end: f64, gain: Option<f64>) -> Call {
    Call { clip, kind: kind.to_string(), start, end, gain }
}

/// The one effect of a reply that should hold exactly one, or the reason it holds something else.
fn only(reply: &pass::Reply) -> Fx {
    assert_eq!(reply.effects.len(), 1, "{:?}", reply.effects);
    reply.effects[0].clone()
}

// --- S1: what the request says ------------------------------------------------------------------------------------------

/// S1 (`one call over every clip · told the captions and speeds`): one message for the whole cut, each clip's line
/// carrying what has already been decided about it — a caption means it runs at 1, a rate means the picture already
/// moves — because both change what a decoration should be.
#[test]
fn f3_11_s1_one_call_told_the_captions_and_speeds() {
    let mut clips = five_clips();
    clips.add_caption(1, 1.0, 3.0, "first line");
    clips.add_caption(1, 4.0, 6.0, "second line");
    let speeds = [(2u32, 2.0f64), (3, 1.5), (4, 1.0), (5, 0.0)];

    let text = pass::brief(&clips, &speeds);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], cut_captions::CLIPS_HEADER, "{text}");
    // Captions first on clip 1, in §F3.10's spelling; the rate note is this flow's own and drops a whole rate's
    // decimals ("2x", not "2.0x") while keeping a fraction's.
    assert_eq!(lines[1], "CLIP 1: 10.0 s long, 2 caption(s) -- runs at 1");
    assert_eq!(lines[2], "CLIP 2: 10.0 s long, plays at 2x");
    assert_eq!(lines[3], "CLIP 3: 10.0 s long, plays at 1.5x");
    // A rate of 1 is the footage's own speed and a rate of 0 was never applied: neither says anything about the picture,
    // so neither appears.
    assert_eq!(lines[4], "CLIP 4: 10.0 s long");
    assert_eq!(lines[5], "CLIP 5: 10.0 s long");

    // Both notes on one line, in that order: what the clip runs at is decided by its captions, and only then does a rate
    // get mentioned — the pair reads as a reason and an exception.
    let mut both = five_clips();
    both.add_caption(2, 1.0, 3.0, "words");
    let noted = pass::brief(&both, &[(2, 4.0)]);
    assert!(noted.contains("CLIP 2: 10.0 s long, 1 caption(s) -- runs at 1, plays at 4x"), "{noted}");

    // The two numbers this flow adds to the catalogue. §10 gives them no `P.` row — the 60 % and the 0.3 come from
    // §F3.11's own text — so they carry the bare prefix of the rule that reads them, and answer to no family.
    let height = row("effects.proposedZoomHeight");
    assert_eq!(height.spelled, "0.6");
    assert_eq!(height.from, "cut_effects_pass::ZOOM_HEIGHT");
    let fade = row("effects.proposedStopFadeSeconds");
    assert_eq!(fade.spelled, "0.3");
    assert_eq!(fade.from, "cut_effects_pass::STOP_FADE_CAP_SECONDS");
    assert_eq!(params::family("effects.proposedZoomHeight"), params::Family::Other);
    assert_eq!(params::family("effects.proposedStopFadeSeconds"), params::Family::Other);
}

// --- S2: a clip outside the list ------------------------------------------------------------------------------------------

/// S2 (`a clip in the list? no → the whole reply rejected`): one invented clip number says the answer is not about this
/// cut, so nothing from it is placed — including the decorations that were fine. This is F3.9's rule, not F3.10's: a fast
/// rate over captions refuses ONE call, because there the model was right about the cut and wrong about one clip.
#[test]
fn f3_11_s2_a_clip_outside_the_list_loses_the_reply() {
    let clips = five_clips();
    let mixed = vec![call(3, "zoom", 1.0, 4.0, None), call(9, "stop", 1.0, 3.0, None)];
    let reply = pass::place(&clips, &mixed);
    assert_eq!(
        reply.fault,
        Some("clip 9 is not one of the clips given (1 to 5)".to_string()),
        "{:?}",
        reply.fault
    );
    assert!(!reply.usable());
    // The good zoom did not survive: a batch is one answer about a known set of clips.
    assert!(reply.effects.is_empty(), "{:?}", reply.effects);

    // One formatter for three passes — F3.9's captions, F3.10's speeds and this flow all name the range through the same
    // function, so the number the model has to correct cannot be spelled three ways.
    assert_eq!(reply.fault, cut_captions::clip_outside(9, 1, 5));

    // Order does not matter: membership is checked over the whole reply before any tool call is made.
    let reversed = vec![call(9, "stop", 1.0, 3.0, None), call(3, "zoom", 1.0, 4.0, None)];
    let second = pass::place(&clips, &reversed);
    assert!(second.fault.is_some(), "{:?}", second.fault);
    assert!(second.effects.is_empty());
    // And no tool call happened either — the answers list is empty when the reply never got that far.
    assert!(second.answers.is_empty(), "{:?}", second.answers);

    // Inside at both ends of a cut numbered from 1, and two rounds then no more.
    assert!(pass::place(&clips, &[call(1, "zoom", 1.0, 4.0, None)]).usable());
    assert!(pass::place(&clips, &[call(5, "zoom", 1.0, 4.0, None)]).usable());
    assert!(cut_captions::retries(1), "one attempt made, so one retry owed");
    assert!(!cut_captions::retries(2), "two attempts made: never a third");
}

// --- S3: unknown kind or no span is skipped ---------------------------------------------------------------------------------

/// S3 (`a known kind, a real span? no → skipped`): an unusable LINE is not an unusable answer. The tool's own sentence
/// says why, the reply stays usable, and the other calls in it land.
#[test]
fn f3_11_s3_an_unknown_kind_or_an_empty_span_is_skipped() {
    let clips = five_clips();

    // `blur` is not this pass's; `text` is a caption, which §F3.9 places and this flow must not write twice; an empty
    // kind is the same nothing. Each names what was sent, so the model can correct it on the retry it does NOT get for
    // this reason — a skipped line costs nothing but that line.
    for kind in ["blur", "text", "", "ZOOM"] {
        let reply = pass::place(&clips, &[call(1, kind, 1.0, 4.0, None), call(2, "zoom", 1.0, 3.0, None)]);
        assert!(reply.usable(), "{kind} must not fault the whole reply: {:?}", reply.fault);
        let Answer::Refused(reason) = &reply.answers[0] else {
            panic!("{kind:?} was not skipped: {:?}", reply.answers[0]);
        };
        assert!(reason.contains("not an effect this pass places"), "{reason}");
        // The valid call in the same reply is placed — skipped, not fatal.
        assert_eq!(reply.effects.len(), 1, "{kind}: {:?}", reply.effects);
        assert_close("the survivor is clip 2's zoom", reply.effects[0].t, 10.0 + 1.0);
    }

    // A span that is empty after clamping into the clip: end == start, and a reversed pair (which clamps to nothing
    // rather than being read backwards — an effect has no direction to travel in).
    for (start, end) in [(4.0, 4.0), (6.0, 2.0)] {
        let reply = pass::place(&clips, &[call(1, "zoom", start, end, None)]);
        assert!(reply.usable());
        assert!(reply.effects.is_empty(), "{start}..{end}: {:?}", reply.effects);
        let Answer::Refused(reason) = &reply.answers[0] else {
            panic!("{start}..{end} was not skipped: {:?}", reply.answers[0]);
        };
        assert!(reason.contains("the span is empty"), "{reason}");
    }

    // A span reaching past the clip's end is CLAMPED and placed, at the clip's own session second: clip 2 sits at 10.0,
    // so 6.0..30.0 lands as 16.0 for 4.0 s — the clip cannot show what it does not have.
    let clamped = pass::place(&clips, &[call(2, "zoom", 6.0, 30.0, None)]);
    let fx = only(&clamped);
    assert_close("clamped to the clip's start offset", fx.t, 10.0 + 6.0);
    assert_close("and to its end", fx.dur, 4.0);
}

// --- S4: the app's defaults per kind -----------------------------------------------------------------------------------------

/// S4 (`the app's defaults: zoom centred at 60 % · stop 2 s · volume ramps min 1, d/4`): where the tool says WHAT landed,
/// §F3.11 decides how it looks and fades — and those are three different rules, none of them the tool's own report.
#[test]
fn f3_11_s4_the_apps_defaults_per_kind() {
    // A zoom over 6 s: centred at sixty per cent height, gliding a second each way — min(1, 6/3) = min(1, 2.0), so the
    // cap wins. `// P.policy.effectDefaultFades`, whose row reads "zoom 1": a camera that arrives without gliding is a
    // jump cut, the same reason §F3.1 gives a hand-placed zoom the same default.
    let zoom = pass::defaults("zoom", 0.0, (0.0, 6.0), None);
    assert_eq!(zoom.kind, "zoom");
    assert_eq!((zoom.cx, zoom.cy), (Some(0.5), Some(0.5)));
    assert_eq!(zoom.hf, Some(0.6));
    assert_close("the glide cap", zoom.trans, 1.0);
    assert_close("both edges", zoom.tout, 1.0);
    // And d/3 wins below the cap: 1.5 / 3 = 0.5, exact in binary.
    let brief_zoom = pass::defaults("zoom", 0.0, (0.0, 1.5), None);
    assert_close("a short zoom glides for half a second", brief_zoom.trans, 0.5);
    assert_eq!(pass::glide(6.0), 1.0);
    assert_eq!(pass::glide(1.5), 0.5);

    // A stop over 8 s is a SPEED whose rate is 0 — §06#1's table has no stop kind, because the picture standing still
    // while its footage runs on IS a rate of nought. Faded min(0.3, d/4) = min(0.3, 2.0) = 0.3: this 0.3 is §F3.11's own
    // cap and NOT P.policy.effectDefaultFades' "stop 0.5", which is §F3.3's value for a stop placed by hand from ⏩.
    let stop = pass::defaults("stop", 0.0, (0.0, 8.0), None);
    assert_eq!(stop.kind, "speed");
    assert_close("a stop is a rate of nought", stop.rate, 0.0);
    assert!(fx_record::is_stop(&stop), "{stop:?}");
    assert_close("faded to §F3.11's cap", stop.trans, 0.3);
    assert_close("both edges", stop.tout, 0.3);
    // d/4 wins below the cap: 0.8 / 4 = 0.2, exact.
    let short_stop = pass::defaults("stop", 0.0, (0.0, 0.8), None);
    assert_close("a short stop fades less", short_stop.trans, 0.2);
    // §F3.11's "stop 2 s" is P.policy.effectDefaultSeconds ("stop/speed/volume/label 2") — the length a stop is ASKED
    // for. It cannot be applied to a span here: the flowchart skips an empty one instead of padding it, which is why a
    // model's span is never silently stretched to this constant (the prototype's `if d <= 0 { d = 2 }` was unreachable
    // behind its own parse for the same reason).
    assert_close("P.policy.effectDefaultSeconds", cut_speed::STOP_SECONDS, 2.0);

    // A volume over 4 s: ramps min(1, 4/4) = 1.0 each way, the same 1 s cap F3.10 gives a proposed speed (read from that
    // pass's constant, so one number is one decision), with this flow's quarter.
    let volume = pass::defaults("volume", 0.0, (0.0, 4.0), Some(0.5));
    assert_eq!(volume.kind, "volume");
    assert_close("the gain as sent", volume.gain, 0.5);
    assert_close("a second of ramp", volume.trans, 1.0);
    assert_close("both ways", volume.tout, 1.0);
    let half = pass::defaults("volume", 0.0, (0.0, 2.0), Some(0.5));
    assert_close("2 s of passage ramps for a quarter of itself", half.trans, 0.5);

    // The record carries nothing else: no words, no file, no lane — §1's table marks all of those "–" for these kinds,
    // and the box belongs to a zoom alone.
    assert_eq!((zoom.text.as_str(), zoom.src.as_str(), zoom.lane.as_str()), ("", "", ""));
    assert_eq!((volume.cx, volume.hf), (None, None));
}

// --- S4/S5: gain 1 and no gain are ignored, 0 is kept --------------------------------------------------------------------------

/// S4 (`gain of 1 or none ignored, 0 kept`): a volume that asks for no change is not a refusal the model has to fix — it
/// is nothing at all — while silence is a decision someone can mean and reaches the record.
#[test]
fn f3_11_s5_gain_1_and_no_gain_are_ignored_and_zero_is_kept() {
    let clips = five_clips();

    // No gain field: an omitted JSON number means "make it different, you decide", and §F3.11 says ignore that rather
    // than guess a level.
    let none = pass::place(&clips, &[call(1, "volume", 1.0, 4.0, None)]);
    assert_eq!(none.answers, vec![Answer::Ignored]);
    assert!(none.effects.is_empty(), "{:?}", none.effects);

    // A gain of exactly 1 is the same nothing. Answered here rather than by §3.7's tool — which replies "a volume gain of
    // 1 changes nothing -- leave the audio alone" — because that sentence reads as a fault to act on when nothing is
    // wrong, and §F3.11 calls this case ignored, not skipped.
    let untouched = pass::place(&clips, &[call(1, "volume", 1.0, 4.0, Some(1.0))]);
    assert_eq!(untouched.answers, vec![Answer::Ignored], "{:?}", untouched.answers);
    assert!(untouched.effects.is_empty());
    assert!(untouched.usable());

    // A gain of 0 is silence and stays: the whole reason a session says "do not use this audio".
    let hush = pass::place(&clips, &[call(1, "volume", 1.0, 4.0, Some(0.0))]);
    let Answer::Placed(fx) = &hush.answers[0] else {
        panic!("silence was not placed: {:?}", hush.answers[0]);
    };
    assert_close("the gain is nought", fx.gain, 0.0);
    assert!(fx_record::gain_is_silence(fx));
    // Ramped like any other volume — a silence that arrives on one sample is a cut in the audio. The span asked for was
    // 1.0..4.0, so d = 3.0 and the ramp is min(1, 3/4) = 0.75: this record's length decides, not the call's offsets.
    assert_close("its length", fx.dur, 3.0);
    assert_close("ramped in", fx.trans, 0.75);

    // And the three outcomes together: ignored, refused and placed in one reply, with the fault staying empty and exactly
    // one effect reaching the cut.
    let mixed = pass::place(
        &clips,
        &[call(1, "volume", 1.0, 4.0, None), call(2, "blur", 1.0, 4.0, None), call(3, "zoom", 1.0, 4.0, None)],
    );
    assert_eq!(mixed.fault, None);
    assert_eq!(mixed.effects.len(), 1, "{:?}", mixed.effects);
    assert_eq!(
        mixed.answers.iter().map(|a| matches!(a, Answer::Placed(_))).collect::<Vec<_>>(),
        vec![false, false, true]
    );
}

// --- S5: the three lines and the budget of two rounds ------------------------------------------------------------------------------

/// S5 (`two rounds fail → "!!! effects: no usable answer -- the cut stands without them"`): with an ASCII `--`, verified
/// against §F3.11's bytes — as in F3.9's captions line and unlike F3.10's em-dashed pair, since each flow quotes its own
/// punctuation. The other two lines are the prototype's: a run that places three decorations should say so.
#[test]
fn f3_11_s6_the_three_lines_and_the_two_round_budget() {
    assert_eq!(pass::no_answer(), "!!! effects: no usable answer -- the cut stands without them");
    let line = pass::no_answer();
    assert!(line.contains("--"), "{line}: this line's break is ASCII --");
    assert!(!line.contains('\u{2014}'), "{line}: an em dash belongs to §F3.10's lines, not this one");

    // Success and the fault said back before the retry — both `>>>`, both the prototype's wording.
    assert_eq!(pass::placed(3), ">>> effects: 3 decoration(s)");
    assert_eq!(pass::placed(0), ">>> effects: 0 decoration(s)", "a plain cut is still an answer");
    assert_eq!(pass::rejected("x"), ">>> effects rejected: x");

    // The whole flow for a reply that names a clip nobody gave, twice: unusable both times, so nothing is placed and the
    // log is one fault line plus the giving-up line. `cut_captions::retries` counts attempts MADE (one owed retry after
    // one attempt, none after two), so the second rejection is reported by no_answer rather than a second `>>> rejected`.
    let clips = five_clips();
    let hopeless = [call(9, "zoom", 1.0, 4.0, None)];
    let mut log: Vec<String> = Vec::new();
    let effects: Vec<Fx>;
    let mut runs = 0u32;
    loop {
        runs += 1;
        let reply = pass::place(&clips, &hopeless);
        assert_eq!(reply.fault, Some("clip 9 is not one of the clips given (1 to 5)".to_string()));
        assert!(reply.effects.is_empty(), "an unusable reply places nothing");
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
    // "the cut stands without them": a plain cut, and a whole one.
    assert!(effects.is_empty(), "no decoration survived the pass: {effects:?}");
}
