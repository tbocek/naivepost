//! §12-decisions#4-what-stays-out-of-the-models-hands-on-purpose — the six things the model is
//! never asked to decide, checked against the code that decides them instead.
//!
//! §4's test for admission is three-part: mechanical, checkable, visible on screen afterwards. Each
//! bullet below takes its evidence from the live path (a real envelope placement, a real clock, a real
//! clamp) rather than from the audit prose, the way `tests/cut_effect_decisions.rs` does.
//!
//! Cited ids: P.policy.snapToleranceSeconds, P.policy.deadAirMaxSeconds,
//! P.eng.effectMinSurvivingSeconds, P.eng.narrationMaxTempo, P.policy.gameVolume.
//! Cited tool: tool:set_policy.

use naivepost::clock;
use naivepost::cut::{Cut, Fx, Seg};
use naivepost::cut_clamp;
use naivepost::cut_effect_decisions as ced;
use naivepost::cut_speed_pass;
use naivepost::cut_trim;
use naivepost::decision_homes;
use naivepost::edges::{AlignedWord, Edges};
use naivepost::hands_off::{self as ho, HandsOff};
use naivepost::narrate_pass;
use naivepost::narrate_preview;
use naivepost::produce_render;
use naivepost::produce_subtitles;
use naivepost::project::{CutMode, Field, MarkingPass, Origin, Policy};
use naivepost::separate;
use naivepost::tools::clips::Clips;
use naivepost::tools::cutpass::Plan;
use naivepost::wave::Wave;

const HZ: f64 = 200.0;
const ROOM: u8 = 8;
const WORD: u8 = 120;

/// An envelope silent at room level except in these spans.
fn wave(total: f64, spans: &[(f64, f64, u8)]) -> Wave {
    let mut peaks = vec![ROOM; (total * HZ) as usize];
    for (start, end, level) in spans {
        for bucket in (start * HZ) as usize..(end * HZ) as usize {
            peaks[bucket] = *level;
        }
    }
    Wave { hz: HZ, chans: vec![peaks] }
}

fn seg(s: f64, e: f64) -> Seg {
    Seg { s, e, ..Default::default() }
}

fn fx(kind: &str, t: f64, dur: f64) -> Fx {
    Fx { kind: kind.into(), t, dur, ..Default::default() }
}

/// A speed run at one rate — `merge` only folds runs that hear the same thing, so both need a rate.
fn speed(t: f64, dur: f64, rate: f64) -> Fx {
    Fx { kind: "speed".into(), t, dur, rate, snd: String::new(), ..Default::default() }
}

// --- bullet 1: word times ----------------------------------------------------------------------

#[test]
fn sec_12_decisions_4_what_stays_out_of_the_models_hands_on_purpose_s1_word_times_are_the_aligners() {
    // Rule 2.1: a model is never asked to compute a timestamp. The writer gets clip-relative offsets
    // and the app turns them into seconds; nothing here asks for an absolute time.
    assert_eq!(narrate_pass::at_offset(3.0, 10.0), 3.0);
    // Clamped inside the clip it was shown, never past the last speakable second.
    assert_eq!(narrate_pass::at_offset(99.0, 10.0), 9.0);
    assert_eq!(narrate_pass::at_offset(-4.0, 10.0), 0.0);

    // A clip-relative ask through the clips pass: the model sends offsets inside the clip, the reply
    // reports them back in the same space.
    let mut batch = Clips::new(&[(1, 42.0, 30.0)]);
    let reply: serde_json::Value =
        serde_json::from_str(&batch.add_caption(1, 2.0, 8.0, "offsets not stamps")).unwrap();
    assert_eq!(reply["span"][0].as_f64().unwrap(), 2.0);
    assert_eq!(reply["span"][1].as_f64().unwrap(), 8.0);

    // And the category itself says so.
    assert!(HandsOff::WordTimes.why_it_is_hands_off().contains("aligner"));
}

// --- bullet 2: where between two known words -----------------------------------------------------

#[test]
fn sec_12_decisions_4_what_stays_out_of_the_models_hands_on_purpose_s2_the_envelope_places_inside_the_fence() {
    // The model chose the word ("stays", stamped 1.0..1.5). Where the cut actually falls inside the
    // fence after it is read off the sound, not proposed.
    let tight = Edges::new(wave(6.0, &[(1.0, 1.5, WORD)]), 0.0);
    let w = AlignedWord { word: "stays".into(), s: 1.0, e: 1.5 };
    // No width to choose in: the aligner's time is all there is.
    assert_eq!(tight.end_after(&w, 1.5), 1.5);

    // With room behind it, the envelope picks the quiet: a breath placed later moves the edge off the
    // stamp without changing which word was chosen.
    let with_room = Edges::new(wave(6.0, &[(1.0, 1.5, WORD), (2.4, 3.0, WORD)]), 0.0);
    let placed = with_room.end_after(&w, 2.4);
    assert!(placed >= 1.5, "never before the word's own end");
    assert!(placed <= 2.4, "never past the limit the caller set");
    // The reach bound keeps it honest: nothing further than edges::EDGE_REACH may be claimed.
    assert!(placed - 1.5 <= naivepost::edges::EDGE_REACH + 1e-9);
}

// --- bullet 3: the session clock ---------------------------------------------------------------

#[test]
fn sec_12_decisions_4_what_stays_out_of_the_models_hands_on_purpose_s3_the_clock_is_read_not_proposed() {
    // Placement comes out of the file names: the earliest stamp is second zero, and every other source
    // sits at its own distance from it. Absolute instants are large (a wall clock since the epoch), so
    // the assertions are on differences rather than on the numbers themselves. `name_stamp` reads the
    // date-time down to whole seconds and no further — milliseconds belong to `frame_name`'s grid, not
    // to a source's placement — hence whole-second gaps here.
    let names = [
        "2026-08-08_19-55-15.000",
        "2026-08-08_19-55-25.000",
        "2026-08-08_19-56-15.000",
    ];
    let c = clock::clock(&names);
    assert_eq!(c.offset(0), 0.0, "the earliest recording starts the clock");
    for src in &c.sources {
        assert!(src.stamped, "every name here carries a moment");
    }
    let d1 = c.offset(1) - c.offset(0);
    let d2 = c.offset(2) - c.offset(0);
    assert!((d1 - 10.0).abs() < 1e-3, "ten seconds after the first: {d1}");
    assert!((d2 - 60.0).abs() < 1e-3, "a minute after the first: {d2}");
    assert!(clock::name_stamp("2026-08-08_19-55-15.000").is_some());
    // A name that names no moment is placed at the start and warned about, never guessed at.
    assert!(clock::name_stamp("clip_final_final.mp4").is_none());

    // A hand shift correction is a number typed by hand into the map, then slid whole.
    let start: std::collections::BTreeMap<String, f64> =
        [("mic".to_string(), 0.0)].into_iter().collect();
    let shifted = cut_trim::slide_shift(&start, &["mic".to_string()], 2.5);
    assert_eq!(shifted["mic"], 2.5);
    // Reading the same source twice from the OPENING map does not compound: one gesture, one step.
    let again = cut_trim::slide_shift(&start, &["mic".to_string()], 2.5);
    assert_eq!(again["mic"], shifted["mic"]);
}

// --- bullet 4: the walk-back -------------------------------------------------------------------

#[test]
fn sec_12_decisions_4_what_stays_out_of_the_models_hands_on_purpose_s4_the_walk_back_runs_after_finish_on_every_reply() {
    // Edge snapping: P.policy.snapToleranceSeconds bounds how far an edge may travel.
    let mut plan = Plan::new(600.0, 300.0).with_snap_points(vec![10.0, 40.5]);
    plan.add_segment(10.4, 40.2, "hand or model, snapped either way");
    let got = &plan.segments()[0];
    assert!((got.start - 10.0).abs() < 1e-9);
    assert!((got.end - 40.5).abs() < 1e-9);

    // Coalescing: two adjacent runs at one rate become one. GAP_SECONDS (4.0) decides "adjacent" and
    // the bound is strictly nearer-than, so a 3 s gap merges and exactly 4 s does not.
    let near = cut_speed_pass::merge(&[
        speed(10.0, 5.0, 2.0),
        speed(18.0, 4.0, 2.0),
    ]);
    let exact = cut_speed_pass::merge(&[
        speed(10.0, 5.0, 2.0),
        speed(19.0, 4.0, 2.0),
    ]);
    assert_eq!(cut_speed_pass::GAP_SECONDS, 4.0);
    assert_eq!(near.len(), 1, "the touching pair coalesced into one run: {near:?}");
    assert!((near[0].t - 10.0).abs() < 1e-9, "the merged run starts at the earlier one");
    assert!(near[0].dur > 5.0, "and carries the later stretch into itself: {}", near[0].dur);
    assert_eq!(exact.len(), 2, "a gap of exactly the bound stays two runs");

    // Clamping: P.eng.effectMinSurvivingSeconds drops what has too little survivor left.
    let cut = Cut {
        segs: vec![seg(0.0, 10.0)],
        fx: vec![fx("zoom", 1.0, 4.0), fx("volume", 30.0, 4.0)],
        ..Default::default()
    };
    let clamped = cut_clamp::clamp_to_cut(&cut);
    assert_eq!(clamped.kept.len(), 1);
    assert_eq!(clamped.dropped, 1);
    // The page shows what it did: the log states the count.
    assert!(cut_clamp::log_line(clamped.dropped).starts_with(">>> 1 effect(s)"));

    // Dead air: P.policy.deadAirMaxSeconds is the bound, and the comparison is the app's.
    assert!(!ced::dead_air_is_cut(7.0, 8.0));
    assert!(ced::dead_air_is_cut(9.0, 8.0));

    // Admission holds for the walk-back itself: mechanical, checkable, visible.
    assert!(ho::admits_to_hands_off(true, true, true));
    // Fail any one of the three and it is not hands-off — it belongs at finish.
    assert!(!ho::admits_to_hands_off(true, true, false));
    assert!(!ho::admits_to_hands_off(false, true, true));
    assert!(!ho::admits_to_hands_off(true, false, true));
}

// --- bullet 5: the render's arithmetic ----------------------------------------------------------

#[test]
fn sec_12_decisions_4_what_stays_out_of_the_models_hands_on_purpose_s5_the_render_does_the_numbers() {
    // Fitting up to P.eng.narrationMaxTempo: the tempo ceiling is the app's, not a request.
    assert_eq!(produce_render::MAX_TEMPO, 1.25);
    let (placed, logs) = produce_render::fit(&[(1, 2.0, 4.0)], 10.0);
    assert_eq!(placed.len(), 1);
    // Enough room means no squeeze and no log about one.
    assert!(logs.iter().all(|l| !l.contains("sped")), "{logs:?}");

    // Ducking: the game bed drops to the project volume only while a line is spoken.
    // P.policy.gameVolume
    let game = 0.22;
    assert_eq!(narrate_preview::duck(true, game), game);
    assert_eq!(narrate_preview::duck(false, game), 1.0);

    // Cue wrapping: the row width is the app's rule, applied whether or not the writer wrapped.
    let wrapped = produce_subtitles::wrap("one two three four five six seven eight nine ten eleven");
    assert!(!wrapped.is_empty());
    assert!(produce_subtitles::ROW_CHARS == 42);

    // Loudness: reported as measured, never guessed at.
    let line = separate::loudness_log("base.wav", "-16 LUFS", "-15 LUFS", "-18 LUFS");
    assert!(line.contains("-16 LUFS"), "{line}");
    assert!(line.contains("voice"), "{line}");
}

// --- bullet 6: nothing chooses the pipeline but the User Context ---------------------------------

#[test]
fn sec_12_decisions_4_what_stays_out_of_the_models_hands_on_purpose_s6_the_pipeline_is_one_visible_setting() {
    // The two fields the policy form writes, with their serde names.
    let policy = Policy::default();
    let json = serde_json::to_string(&policy).unwrap();
    assert!(json.contains("markingPass"), "{json}");
    assert!(json.contains("cutMode"), "{json}");
    // Each is a Field carrying an origin beside the value.
    let marking: Field<MarkingPass> = policy.marking_pass.clone();
    assert_eq!(marking.value, MarkingPass::Retakes);
    assert_eq!(marking.origin, Origin::Default);

    // The three deciding fields named for the form.
    let fields = ho::pipeline_fields();
    assert_eq!(fields.len(), 3);
    assert_eq!(fields[0].field, "markingPass");
    assert_eq!(fields[1].field, "cutMode");
    assert!(fields[2].decides.contains("cut-stage"), "{}", fields[2].decides);

    // Hand-set sticks; derived and default do not. This is what keeps the policy home both derived
    // and editable rather than a hidden style dropdown.
    assert!(!ho::model_may_set_pipeline(Origin::User));
    assert!(ho::model_may_set_pipeline(Origin::Model));
    assert!(ho::model_may_set_pipeline(Origin::Default));
    // Same predicate decision_homes gives the whole policy home.
    assert_eq!(
        ho::model_may_set_pipeline(Origin::User),
        decision_homes::overridable_by_derivation(Origin::User)
    );

    // Both cut modes exist: words or a model, chosen once and shown.
    assert_eq!(ho::cut_modes(), [CutMode::Words, CutMode::Model]);
    assert_eq!(
        ho::marking_passes(),
        [MarkingPass::Joins, MarkingPass::Retakes, MarkingPass::None]
    );
}

// --- the table and the cited ids ----------------------------------------------------------------

#[test]
fn sec_12_decisions_4_what_stays_out_of_the_models_hands_on_purpose_audit_covers_all_six_bullets_in_order() {
    let rows = ho::audit();
    assert_eq!(rows.len(), 6);
    for row in &rows {
        assert!(!row.what.is_empty(), "{row:?}");
        assert!(!row.home.is_empty(), "{row:?}");
        assert!(!row.lives_in.is_empty(), "{row:?}");
    }
    // Order follows §4's bullets.
    assert!(rows[0].what.contains("word times"), "{}", rows[0].what);
    assert!(rows[1].what.contains("between two known words"), "{}", rows[1].what);
    assert!(rows[2].what.contains("session clock"), "{}", rows[2].what);
    assert!(rows[3].what.contains("snapping"), "{}", rows[3].what);
    assert!(rows[4].what.contains("render's arithmetic"), "{}", rows[4].what);
    assert!(rows[5].what.contains("pipeline"), "{}", rows[5].what);
    // Every category answers the admission questions with something to show.
    assert!(ho::all_admitted());
}

#[test]
fn sec_12_decisions_4_what_stays_out_of_the_models_hands_on_purpose_cited_ids_answer_from_the_catalogue() {
    fn row(id: &str) -> naivepost::params::Param {
        let mut found = naivepost::params::prepare()
            .into_iter()
            .chain(naivepost::params::cut())
            .chain(naivepost::params::effects())
            .chain(naivepost::params::narrate())
            .chain(naivepost::params::produce())
            .filter(|row| row.id == id)
            .collect::<Vec<_>>();
        assert_eq!(found.len(), 1, "{id} catalogued {} times", found.len());
        found.pop().unwrap()
    }
    for id in ho::CITED_PARAMS {
        let _ = row(id);
    }
    // tool:set_policy and the read tool the policy job uses are both known tools.
    let known = [naivepost::tools::Tool::SetPolicy, naivepost::tools::Tool::GetContext];
    for name in ho::CITED_TOOLS {
        assert!(known.iter().any(|t| t.name() == name), "{name} is not a known tool");
    }
}
