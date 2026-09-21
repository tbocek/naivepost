//! F3.9 Captions proposed by the model (after the cut) — `spec/06-effects.md` F3.9, steps S1–S5, plus the log line
//! its flowchart's fallback branch prints.
//!
//! The model proposes; this pass places. So what these tests pin is the arithmetic and the wording around that: how
//! many clips one request carries (P.machine.captionBatch), that the message speaks in offsets inside each clip and
//! never in session seconds, that one clip number outside the batch throws away the whole reply and buys exactly one
//! retry, that a caption under P.policy.captionMinSeconds or without words vanishes without a word while its
//! neighbours survive, the `min(0.3, d/4)` fade, and the sentence said when a batch keeps failing. Strings are
//! compared whole — §F3.9 quotes four of them — and every float is derived on paper from the rule it tests and
//! compared through a tolerance, since 0.1 and 0.2 are not representable in binary. No widget and no display:
//! `rust/src/ui/window.rs` renders only Prepare, so there is no ▶ to press yet.

use naivepost::cut::Fx;
use naivepost::cut_captions::{self as captions, Call, Reply};
use naivepost::fx_record::{self, Field};
use naivepost::params;
use naivepost::tools::clips::{self, Clips};
use naivepost::roles::Job;
use naivepost::tools::Tool;

/// Floats from a rule, never `assert_eq!`: 1.2 - 1.0 is not 0.2, and this flow's numbers are all differences.
const EPS: f64 = 1e-9;

fn assert_close(asked: &str, got: f64, want: f64) {
    assert!((got - want).abs() < EPS, "{asked}: got {got}, want {want}");
}

/// One row of the catalogue, by id. Both of this flow's ids are catalogued — `P.machine.captionBatch` in §4's list
/// (which [`params::find`] answers) and `P.policy.captionMinSeconds` in §6's (`params::cut`) — so this looks through
/// both rather than guessing which page a row was filed under. A duplicate would be two homes for one bound, which
/// is what the catalogue test counts; asserting one here keeps that honest from this round's side too.
fn row(id: &str) -> params::Param {
    let mut found = params::prepare()
        .into_iter()
        .chain(params::cut())
        .filter(|row| row.id == id)
        .collect::<Vec<_>>();
    assert_eq!(found.len(), 1, "{id} catalogued {} times", found.len());
    found.pop().unwrap()
}

/// The batch S4 and S5 argue about: clips 6 to 10, ten seconds each. Ten is exact in binary, so every offset below
/// is derived by addition of whole or halved numbers rather than by a run. Starting at 6 rather than 1 is what makes
/// S4's refusal interesting — "clip 3" is then a clip that exists somewhere but not in this batch.
fn batch() -> Vec<(u32, f64)> {
    (6..=10).map(|n| (n, 10.0)).collect()
}

/// The whole session: fifteen clips of ten seconds, so the batches S1 prints for it are (1–5), (6–10), (11–15) and a
/// caption's session second is ten times the number of clips before it — 11 sits at 100.0, exactly.
fn session() -> Vec<(u32, f64)> {
    (1..=15).map(|n| (n, 10.0)).collect()
}

fn call(clip: u32, start: f64, end: f64, text: &str) -> Call {
    Call { clip, start, end, text: text.to_string() }
}

/// The captions of a reply, or the reason it was thrown away.
fn placed(clips_in: &[(u32, f64)], calls: &[Call]) -> Vec<Fx> {
    match captions::place(clips_in, calls) {
        Reply::Accepted(fxs) => fxs,
        Reply::Rejected(reason) => panic!("the reply was rejected: {reason}"),
    }
}

// --- S1: how many clips one request carries ----------------------------------------------------------------------

/// S1 (`clips in batches of P.machine.captionBatch`): five per request, a short final batch, and no request at all
/// for nothing to caption. The ranges are what S4's refusal names and S6's log line prints, so their shape — 1-based,
/// inclusive, contiguous, each clip exactly once — is the fact rather than an implementation detail.
#[test]
fn f3_9_s1_clips_are_asked_about_at_a_time() {
    // P.machine.captionBatch: five clips per caption request, catalogued from the pass that builds the batches.
    assert_eq!(captions::BATCH, 5);
    let batch_row = row("P.machine.captionBatch");
    assert_eq!(batch_row.spelled, captions::BATCH.to_string());
    assert_eq!(batch_row.from, "cut_captions::BATCH");
    assert_eq!(params::family("P.machine.captionBatch"), params::Family::Machine);

    // P.policy.captionMinSeconds: the floor a proposed caption has to clear, catalogued from §3.7's tool — the one
    // place 0.3 is written for this rule.
    let floor = row("P.policy.captionMinSeconds");
    assert_eq!(floor.spelled, "0.3");
    assert_eq!(floor.from, "tools::clips::CAPTION_MIN_SECONDS");
    assert_eq!(params::family("P.policy.captionMinSeconds"), params::Family::Policy);

    // An empty cut asks nothing: no request, so no cost and no log line.
    assert_eq!(captions::batches(0), Vec::<(u32, u32)>::new());
    assert_eq!(captions::batches(1), [(1, 1)]);
    // A full batch is one request, not two — the boundary case a `count / BATCH` off-by-one would get wrong.
    assert_eq!(captions::batches(5), [(1, 5)]);
    // The sixth clip opens a second request on its own.
    assert_eq!(captions::batches(6), [(1, 5), (6, 6)]);
    // And twelve clips are three requests, the last short.
    assert_eq!(captions::batches(12), [(1, 5), (6, 10), (11, 12)]);

    // The shape, for counts around every multiple of five: inclusive, 1-based, contiguous, covering each clip once.
    for count in [0usize, 1, 4, 5, 6, 9, 10, 11, 23, 25] {
        let ranges = captions::batches(count);
        let mut next = 1u32;
        for (first, last) in &ranges {
            assert_eq!(*first, next, "{count} clips: a gap or an overlap at {first}");
            assert!(last >= first, "{count} clips: {first}–{last} is not inclusive-forward");
            assert!(last - first < captions::BATCH as u32, "{count} clips: {first}–{last} is over a batch");
            next = last + 1;
        }
        assert_eq!(next, count as u32 + 1, "{count} clips: the clips stop at {}", next - 1);
    }
}

// --- S2: the message -----------------------------------------------------------------------------------------------

/// S2 (`message = User Context + "THE CLIPS, AND WHAT WAS SAID OVER EACH:" + "CLIP n: X s long" + the lines as
/// offsets`): what one request looks like, and the fact that its times belong to the clip rather than to the session.
#[test]
fn f3_9_s2_the_message_shows_clips_and_words_as_offsets() {
    // §F3.9's own example line: one decimal, a space before `s`, and the number is the CLIP's length.
    assert_eq!(captions::clip_line(1, 14.2), "CLIP 1: 14.2 s long");
    assert_eq!(captions::clip_line(12, 8.0), "CLIP 12: 8.0 s long", "one decimal even on a whole length");

    // The `+` says which way the offset runs; brackets and `s` are §F3.9's example exactly (`[+0.4s] …`).
    assert_eq!(captions::offset_line(0.4, "the words"), "[+0.4s] the words");

    let clips = [(1u32, 14.2f64), (2u32, 8.0f64)];
    let words = [(1u32, 0.4f64, "first line"), (2u32, 1.5, "second line")];
    let text = captions::message("the user context", &clips, &words);

    // The order is the spec's sentence: context, header, then each clip with its own words under it.
    assert!(text.starts_with("the user context\n"), "{text}");
    let at = text.find(captions::CLIPS_HEADER).expect("the header opens the clip list");
    let clips_at = text.find("CLIP 1:").expect("the first clip");
    assert!(at < clips_at, "{text}: the header comes after a clip line");
    assert_eq!(captions::CLIPS_HEADER, "THE CLIPS, AND WHAT WAS SAID OVER EACH:");
    let lines: Vec<&str> = text.lines().skip(2).collect();
    assert_eq!(
        lines,
        [
            captions::CLIPS_HEADER,
            "CLIP 1: 14.2 s long",
            "[+0.4s] first line",
            "CLIP 2: 8.0 s long",
            "[+1.5s] second line",
        ]
    );

    // The offsets are CLIP-relative, which is the whole point of S2: a word 0.4 s into a clip that sits at session
    // 60.0 prints its offset, and the session second appears nowhere in the message — the model is never asked for,
    // or given, an absolute timestamp it would have to compute (`spec/00-principles.md`).
    let later = [(3u32, 5.0f64)];
    let later_words = [(3u32, 0.4f64, "hello")];
    let shown = captions::message("", &later, &later_words);
    assert!(shown.contains("[+0.4s] hello"), "{shown}");
    assert!(!shown.contains("60.4"), "{shown}: a session second leaked into the request");
    assert!(!shown.contains("+60"), "{shown}: an absolute second leaked into the request");

    // An empty User Context adds nothing at all — no blank block to be ignored — and words for a clip this batch
    // does not contain are left out rather than sent unlabelled.
    let bare = captions::message("   ", &clips, &words);
    assert!(bare.starts_with(captions::CLIPS_HEADER), "{bare}");
    assert!(!bare.starts_with('\n'), "{bare}: a blank block opens the message");
    let foreign = [(1u32, 0.4f64, "kept"), (9u32, 0.0, "not in this batch")];
    let filtered = captions::message("", &clips, &foreign);
    assert!(filtered.contains("[+0.4s] kept"), "{filtered}");
    assert!(!filtered.contains("not in this batch"), "{filtered}");
}

// --- S3: the answer's two tools --------------------------------------------------------------------------------------

/// S3 (`tools: add_caption(clip, start, end, text)`, `finish` — spec/02-services.md §3.7): the reply is a list of
/// caption calls closed by `finish`, and each call is answered by §3.7's tool, which reports the caption as PLACED —
/// clamped into the clip rather than as written. This pass offers exactly those four per-clip tools and nothing else.
#[test]
fn f3_9_s3_the_answer_is_two_tools() {
    // The names §3.7 gives them, asserted through the catalogue rather than re-spelled here.
    assert_eq!(Tool::AddCaption.name(), "add_caption");
    assert_eq!(Tool::Finish.name(), "finish");

    // What the captions/decorations job is offered: the clip's own tools plus `finish` to close the reply.
    let offered = naivepost::tools::offered(Job::ClipRules);
    assert!(offered.contains(&Tool::AddCaption), "{offered:?}");
    assert!(offered.contains(&Tool::Finish), "{offered:?}");

    // A valid call comes back ok with the caption as placed, and an offset past the clip's end is CLAMPED there
    // rather than refused: the clip cannot show what it does not have. This is §3.7's tool's rule, reused here —
    // F3.9 adds nothing to it, which is why the fade in this answer (0.15) is the tool's own and not S5's.
    let mut batch = Clips::new(&[(1, 60.0, 10.0)]);
    let answer = batch.add_caption(1, 8.0, 30.0, "runs past the end");
    let body: serde_json::Value = serde_json::from_str(&answer).expect("a tool answer is JSON");
    assert!(body.get("error").is_none(), "{answer}");
    assert_eq!(body["span"].as_array().unwrap()[0].as_f64(), Some(8.0));
    assert_eq!(body["span"].as_array().unwrap()[1].as_f64(), Some(10.0));
    assert_eq!(body["clamped"], serde_json::json!(true));

    // And the two refusals F3.9 wants silence for, in §3.7's own words — quoted here so this round's skip rule
    // cannot drift from the tool it reads without failing somewhere.
    let short = batch.add_caption(1, 1.0, 1.2, "too short to read");
    assert!(short.contains("dropped"), "{short}");
    let empty = batch.add_caption(1, 1.0, 3.0, "   ");
    assert_eq!(empty, r#"{"error":"the caption has no words in it"}"#);

    // This pass's own answer type is the call itself: a clip NUMBER and two offsets inside that clip.
    assert_eq!(call(6, 1.0, 2.5, "words"), call(6, 1.0, 2.5, "words"));
}

// --- S4: one bad clip number loses everything ----------------------------------------------------------------------

/// S4 (`the clip one of this batch? no → the whole reply rejected, retried once`): membership is checked before
/// anything is placed, so a reply that names a clip outside its batch leaves nothing behind — and it costs exactly
/// one retry, never a third attempt.
#[test]
fn f3_9_s4_a_clip_outside_the_batch_loses_the_whole_reply() {
    // Inside the range at both ends, outside either side of it. The wording is §F3.6's flowchart box: the clip number
    // the model has to correct, and the range it should have stayed inside.
    assert_eq!(captions::clip_outside(9, 6, 10), None);
    assert_eq!(captions::clip_outside(6, 6, 10), None, "the batch's first clip is one of them");
    assert_eq!(captions::clip_outside(10, 6, 10), None, "and its last");
    assert_eq!(captions::clip_outside(5, 6, 10), Some("clip 5 is not one of the clips given (6 to 10)".to_string()));
    assert_eq!(captions::clip_outside(11, 6, 10), Some("clip 11 is not one of the clips given (6 to 10)".to_string()));

    // The good caption first, then the impossible one: nothing from this reply survives, which is what a batch means
    // as one answer about a known set of clips. Asserted as an empty result rather than as "one fewer".
    let mixed = vec![call(6, 1.0, 3.0, "fine"), call(3, 1.0, 3.0, "counted from the wrong end")];
    match captions::place(&batch(), &mixed) {
        Reply::Rejected(reason) => {
            assert_eq!(reason, "clip 3 is not one of the clips given (6 to 10)");
        }
        Reply::Accepted(fxs) => panic!("a reply naming clip 3 was accepted with {} caption(s): {fxs:?}", fxs.len()),
    }

    // The order does not matter either: membership is checked over the whole reply before anything is placed, so a
    // bad number in first place loses the good one that follows just as completely.
    let reversed = vec![call(3, 1.0, 3.0, "wrong"), call(6, 1.0, 3.0, "fine")];
    assert!(matches!(captions::place(&batch(), &reversed), Reply::Rejected(_)));

    // Retried once: after one rejection the batch goes out again, and after two it does not — S6's line is what is
    // left of a batch that keeps failing.
    assert!(captions::retries(1), "one attempt made, so one retry owed");
    assert!(!captions::retries(2), "two attempts made: never a third");
    assert!(!captions::retries(0), "nothing asked yet, so nothing to retry");
}

// --- S5: the floor, the fades, and whose seconds these are ---------------------------------------------------------

/// S5 (`≥ P.policy.captionMinSeconds, with words? no → skipped · yes → placed · fades min 0.3, d/4`): a caption under
/// the floor or without words disappears silently while its neighbours in the same reply still land; a placed caption
/// is a text effect at the SESSION second the app worked out, faded by S5's own rule.
#[test]
fn f3_9_s5_short_or_wordless_captions_are_skipped_quietly() {
    // P.policy.captionMinSeconds: 0.3 s, read from §3.7's tool rather than restated. 0.2 is under it, 0.5 over.
    assert_eq!(clips::CAPTION_MIN_SECONDS, 0.3);

    let reply = vec![
        call(6, 1.0, 1.2, "too short to read"),
        call(6, 3.0, 5.0, "   "),
        call(7, 4.0, 5.2, "kept"),
    ];
    let fxs = placed(&batch(), &reply);
    assert_eq!(fxs.len(), 1, "{fxs:?}: the two refusals were not skipped");
    assert_eq!(fxs[0].text, "kept");

    // The record a caption becomes is a text effect — `cut.json` holds no separate kind for a proposed caption, which
    // is what lets a hand-typed one and a proposed one be edited the same way afterwards.
    let fx = &fxs[0];
    assert_eq!(fx.kind, "text");
    // No box: §A.6's lower third it lands in, so `centre()` answers the default rather than (0.5, 0.5).
    assert!(!fx.has_box());
    assert_eq!(fx.centre(), (0.5, 0.78));
    // And a text effect's own fields only: words yes, a zoom's box and a volume's gain no.
    assert!(fx_record::uses(fx.effect_kind().unwrap(), Field::Words));
    assert!(!fx_record::uses(fx.effect_kind().unwrap(), Field::Gain));

    // The session second is the app's arithmetic: clip 7 sits after clip 6, which is ten seconds long, so its 4.0 s
    // offset is session 14.0 — the model was never asked where clip 7 begins and could not have answered.
    assert_close("clip 6 starts at 0.0", captions::session_start(&batch(), 6), 0.0);
    assert_close("clip 7 starts after ten seconds", captions::session_start(&batch(), 7), 10.0);
    assert_close("the caption's session second", fx.t, 14.0);
    // The same rule with positions the caller knows: 60.0 + 0.4 = 60.4.
    let at = captions::on_the_timeline_at(60.0, (0.4, 1.6), "words");
    assert_close("at a known clip start", at.t, 60.4);

    // S5's fade: P.policy.effectDefaultFades' text value (0.3) capped by a quarter of the caption's length, so a
    // short caption cannot spend most of itself fading. Derived on paper — 1.2/4 = 0.3 exactly, so the cap and the
    // quarter meet there; below it d/4 wins (0.8/4 = 0.2, 0.3/4 = 0.075); above it the cap holds (4.0/4 = 1.0 → 0.3).
    assert_close("the two rules meet at 1.2 s", captions::fade(1.2), 0.3);
    assert_close("d/4 wins below that", captions::fade(0.8), 0.2);
    assert_close("and again at the floor", captions::fade(0.3), 0.075);
    assert_close("the cap holds above it", captions::fade(4.0), 0.3);
    // The placed caption carries that fade on both edges: dur 5.2 - 4.0 = 1.2, so 0.3 each side.
    assert_close("its length", fx.dur, 1.2);
    assert_close("faded in", fx.trans, captions::fade(fx.dur));
    assert_close("and out", fx.tout, captions::fade(fx.dur));
    assert_close("which is the cap here", fx.trans, 0.3);
    // A short one fades by d/4 instead — and it is NOT §3.7's tool fade (0.15), which stays what the tool reports.
    let brief = captions::on_the_timeline_at(0.0, (0.0, 0.8), "brief");
    assert_close("a short caption fades by d/4", brief.trans, 0.2);
    assert!((brief.trans - clips::FADE_SECONDS).abs() > EPS, "S5's fade is not the tool's 0.15");

    // Silence at nought gain is nothing to do with this flow, but a caption is not silence either: an accepted reply
    // never leaves a caption-less record behind when the model sent only refusals.
    assert!(matches!(captions::place(&batch(), &[call(6, 1.0, 1.2, "x")]), Reply::Accepted(ref fxs) if fxs.is_empty()));
}

// --- S6: the batch that keeps failing --------------------------------------------------------------------------------

/// S6 (`still failing → "!!! captions: clips a–b skipped -- the cut stands without them"`): what the log says when a
/// batch has been asked twice and both replies were rejected — an en dash in the range, `--` before the clause, and
/// the promise that nothing else was lost.
#[test]
fn f3_9_s6_a_batch_that_keeps_failing_is_dropped_and_said_so() {
    assert_eq!(
        captions::skipped(1, 5),
        "!!! captions: clips 1\u{2013}5 skipped -- the cut stands without them"
    );
    // The dash is §F3.9's en dash, not a hyphen — every range in this app prints one — and `--` (not an em dash) is
    // what separates the fault from its consequence, as `!!!` lines do elsewhere.
    let line = captions::skipped(1, 5);
    assert!(line.contains("1\u{2013}5"), "{line}");
    let between = &line[line.find("clips ").unwrap() + 6..line.find(" skipped").unwrap()];
    assert!(!between.contains('-'), "{between}: an ASCII hyphen in the range");
    assert!(line.starts_with("!!! captions: "), "{line}");

    // The flow read whole. One batch is asked twice and both replies name a clip outside it, so it contributes no
    // captions and exactly one line; its neighbour answers properly on the first try and keeps its caption. That is
    // what "the cut stands without them" means in code: failure is specific and local.
    // `kept` is whatever the batch that answers returns, so it is declared here and bound where it is decided.
    let kept: Vec<Fx>;
    let mut log: Vec<String> = Vec::new();

    // The session as the pass sees it: fifteen clips, which S1 says are asked about in three batches.
    let all = session();
    assert_eq!(captions::batches(all.len()), [(1, 5), (6, 10), (11, 15)]);

    // Batch 6–10: two attempts, both naming clip 5 — a clip that exists in the cut but was not one of the clips
    // given to this request. `place` takes only the batch it was asked about, which is what makes S4's check real.
    let first_batch: Vec<(u32, f64)> = all[5..10].to_vec();
    let hopeless = call(5, 1.0, 3.0, "counted from the wrong end");
    for runs in 1..=2 {
        match captions::place(&first_batch, std::slice::from_ref(&hopeless)) {
            Reply::Accepted(fxs) => panic!("clip 5 is not in 6\u{2013}10: {} caption(s)", fxs.len()),
            // The first rejection owes a retry; the second does not, and that is when S6's line is written.
            Reply::Rejected(_) if captions::retries(runs) => continue,
            Reply::Rejected(_) => log.push(captions::skipped(6, 10)),
        }
    }
    assert_eq!(log.len(), 1, "one line for a batch asked twice: {log:?}");

    // Batch 11–15 answers first time, so its caption is kept — at the session second ten clips in, not at the offset
    // the model sent. The whole cut goes in as well as the batch, because that is where a clip's seconds come from:
    // given the batch alone, which has no memory of what preceded it, the same caption would sit at 1.0.
    let second_batch: Vec<(u32, f64)> = all[10..].to_vec();
    match captions::place(&all, &[call(11, 1.0, 3.0, "words")]) {
        Reply::Accepted(fxs) => kept = fxs,
        Reply::Rejected(reason) => panic!("a reply inside its batch was rejected: {reason}"),
    }
    assert_eq!(second_batch.first(), Some(&(11, 10.0)), "the batch asked about starts at clip 11");
    assert_eq!(log, vec![captions::skipped(6, 10).to_string()], "{log:?}");
    assert_eq!(kept.len(), 1, "the cut stands: one caption from the batch that answered");
    assert_eq!(kept[0].text, "words");
    // Ten clips of ten seconds came before clip 11, so its 1.0 s offset is session 101.0 — computed here, and never
    // something the model was shown or asked for.
    assert_close("session second of the surviving caption", kept[0].t, 100.0 + 1.0);
}
