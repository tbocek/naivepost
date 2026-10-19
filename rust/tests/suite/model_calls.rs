//! §11-flow-index.md §4 Model calls at a glance — the five rows of the table, each checked
//! against the module that actually owns the count it shows.

use naivepost::{
    cut_captions, llm_cache, model_calls::{self, Call, Stage}, narrate_pass, prepare,
    produce_subtitles, project::{CutMode, MarkingPass}, roles, shell,
};

const ITEM: &str = "§11-flow-index#4-model-calls-at-a-glance";

/// The model-name-to-job mapping §2's tables are keyed on, so this test can hold §4's flags
/// against them. `policy`, `tts` and `sd.cpp` are absent on purpose: the first has no job row and
/// the other two are not LLM calls, which is why `model_calls` states their flags directly.
fn job_of(model: &str) -> Option<roles::Job> {
    Some(match model {
        "describe" => roles::Job::Describe,
        "fix" => roles::Job::CleanTranscript,
        "retake" => roles::Job::Retakes,
        "textedit" => roles::Job::Joins,
        "cut" => roles::Job::ModelCut,
        "captions" | "speed" | "effects" => roles::Job::ClipRules,
        "narrate" => roles::Job::Narrate,
        "translate" => roles::Job::Subtitles,
        "youtube" => roles::Job::UploadText,
        _ => return None,
    })
}

// ---- S1: Prepare --------------------------------------------------------------------------

#[test]
fn sec_11_flow_index_4_model_calls_at_a_glance_s1_prepare_describe_fix_and_one_marking_pass() {
    assert_eq!(ITEM, "§11-flow-index#4-model-calls-at-a-glance");
    let s = model_calls::prepare(40, 100, MarkingPass::Retakes, 0);
    assert_eq!(s.stage, Stage::Prepare);

    // describe ×(frames/4) — P.machine.describeFramesPerReq, four frames a vision call.
    let describe = s.of("describe").expect("Prepare always describes its footage");
    assert_eq!(describe.calls, 10, "40 frames / 4 a call");
    assert_eq!(describe.calls, prepare::vision_calls(40), "same divisor as the request builder");
    assert!(describe.cached, "§4 marks the whole Prepare row cached");

    // fix ×(lines/25) — P.machine.fixBlockLines.
    let fix = s.of("fix").expect("the fixer runs on every session with lines");
    assert_eq!(fix.calls, 4, "100 lines / 25 a block");
    assert_eq!(fix.calls, prepare::fixer_calls(100));
    assert!(fix.cached);

    // retake ×3 — P.machine.retakeRuns, the pooled identical runs.
    let retake = s.of("retake").expect("markingPass retakes asks for retakes");
    assert_eq!(retake.calls, 3);
    assert_eq!(retake.calls, roles::RETAKE_RUNS_POOLED as usize);
    assert!(retake.cached);
    assert!(s.of("textedit").is_none(), "the table says \"or\": one marking pass, not both");

    // …and the other half of the "or".
    let joined = model_calls::prepare(8, 50, MarkingPass::Joins, 7);
    assert_eq!(joined.of("textedit").expect("joins runs textedit").calls, 7);
    assert!(joined.of("retake").is_none(), "no retake call under the joins pass");
    assert!(joined.of("describe").is_some() && joined.of("fix").is_some());

    // MarkingPass::None asks nobody to mark: neither row appears.
    let unmarked = model_calls::prepare(8, 50, MarkingPass::None, 9);
    assert!(unmarked.of("retake").is_none() && unmarked.of("textedit").is_none());
    assert_eq!(unmarked.asked(), prepare::vision_calls(8) + prepare::fixer_calls(50));

    // The step's total is the sum of its rows, nothing hidden.
    assert_eq!(
        s.asked(),
        describe.calls + fix.calls + retake.calls,
        "asked() is the bill the rows add up to"
    );
}

// ---- S2: Cut ------------------------------------------------------------------------------

#[test]
fn sec_11_flow_index_4_model_calls_at_a_glance_s2_cut_is_gated_by_cutmode_and_three_switches() {
    assert_eq!(ITEM, "§11-flow-index#4-model-calls-at-a-glance");
    let all = model_calls::cut(23, CutMode::Model, true, true, true);

    // cut ×1 (+web) — only under P.policy.cutMode = model.
    let cut = all.of("cut").expect("cutMode model asks the cut model");
    assert_eq!(cut.calls, 1);
    assert!(cut.web, "§2 gives the cut the web tools");

    // captions ×(clips/5) — P.machine.captionBatch.
    let captions = all.of("captions").expect("captionsPass on");
    assert_eq!(captions.calls, 5, "23 clips in batches of 5, the short tail still a batch");
    assert_eq!(captions.calls, cut_captions::batches(23).len());
    assert!(!captions.cached, "the caption pass is not one of cache/llm's five jobs");

    // speed ×1 · effects ×1.
    assert_eq!(all.of("speed").expect("speedPass on").calls, 1);
    assert_eq!(all.of("effects").expect("decorationsPass on").calls, 1);

    // P.policy.cutMode = words: the timeline comes from the marked text, so the cut model is not
    // asked — but the three passes are still each their own switch.
    let words = model_calls::cut(23, CutMode::Words, true, true, true);
    assert!(words.of("cut").is_none(), "cutMode model only");
    assert_eq!(words.asked(), all.asked() - 1);
    assert!(words.of("captions").is_some() && words.of("speed").is_some() && words.of("effects").is_some());

    // Each pass only when its own switch is on: P.policy.captionsPass, P.policy.speedPass,
    // P.policy.decorationsPass. Turning one off drops exactly that row — and since the captions
    // batch for 23 clips is itself five calls, the drop is counted per call, not per row.
    let baseline = all.asked();
    for (gone, index) in [("captions", 0usize), ("speed", 1), ("effects", 2)] {
        let switches = [index != 0, index != 1, index != 2];
        let off = model_calls::cut(
            23,
            CutMode::Model,
            switches[0],
            switches[1],
            switches[2],
        );
        assert!(off.of(gone).is_none(), "{gone} asked with its switch off");
        let dropped = match gone {
            "captions" => captions.calls,
            _ => 1,
        };
        assert_eq!(off.asked(), baseline - dropped, "one switch off costs {gone}'s calls");
        assert_eq!(off.calls.len(), all.calls.len() - 1, "and one row out of the bill");
    }

    // Nothing on: the step asks only for the cut itself.
    let bare = model_calls::cut(23, CutMode::Model, false, false, false);
    assert_eq!(bare.asked(), 1);
    // Words mode with nothing switched on asks nobody at all.
    assert_eq!(model_calls::cut(23, CutMode::Words, false, false, false).asked(), 0);
}

// ---- S3: Narrate ---------------------------------------------------------------------------

#[test]
fn sec_11_flow_index_4_model_calls_at_a_glance_s3_narrate_writes_once_and_speaks_per_line() {
    assert_eq!(ITEM, "§11-flow-index#4-model-calls-at-a-glance");
    let s = model_calls::narrate(9);

    // narrate ×1 (+web), and NOT cached: the same brief must be rewritten when the prompt or the
    // cut moves, so the words are paid for again by design.
    let write = s.of("narrate").expect("Narrate writes");
    assert_eq!(write.calls, 1);
    assert!(write.web);
    assert_eq!(write.cached, narrate_pass::served_from_cache());
    assert!(!write.cached, "the reply cache never serves narration");

    // TTS ×(lines), cached under the frozen take key — changing that key re-speaks every project,
    // which is why an unchanged line costs nothing to speak twice.
    let voice = s.of("tts").expect("every line is spoken");
    assert_eq!(voice.calls, 9);
    assert!(voice.cached, "cache/tts holds the takes");
    assert!(!voice.web, "audio.cpp gets no web tools");

    assert_eq!(s.asked(), 10, "one writer plus nine voices");
    // No lines means no voices, but the writer is still asked once.
    assert_eq!(model_calls::narrate(0).asked(), 1);
}

// ---- S4: Produce ---------------------------------------------------------------------------

#[test]
fn sec_11_flow_index_4_model_calls_at_a_glance_s4_produce_upload_thumbnail_and_translate() {
    assert_eq!(ITEM, "§11-flow-index#4-model-calls-at-a-glance");
    let s = model_calls::produce("a title card over a black frame", false, 320, 2);

    // youtube ×1 (+web).
    let upload = s.of("youtube").expect("Produce writes the upload text");
    assert_eq!(upload.calls, 1);
    assert!(upload.web);
    assert!(!upload.cached, "the upload text is not one of cache/llm's five jobs");

    // sd.cpp ×1 when drawn.
    assert_eq!(s.of("sd.cpp").expect("an instruction and no named frame draws one").calls, 1);

    // translate ×(languages × batches) — P.machine.translateBatch, 150 cues a request.
    let translate = s.of("translate").expect("two languages translate");
    assert_eq!(translate.calls, 6, "320 cues = 3 batches, times 2 languages");
    assert_eq!(
        translate.calls,
        produce_subtitles::batches(320).len() * 2,
        "the product §4 spells out, from the batching that exists"
    );
    assert!(translate.cached, "translation is one of cache/llm's five jobs");

    // A named frame needs no server: cutting it out of the source is not a model call.
    let framed = model_calls::produce("a title card over a black frame", true, 320, 2);
    assert!(framed.of("sd.cpp").is_none());
    assert!(!roles::thumbnail_wanted("a title card over a black frame", true));

    // No instruction either — nothing to draw from.
    assert!(model_calls::produce("", false, 320, 2).of("sd.cpp").is_none());

    // No languages: zero requests, and therefore no row rather than a `×0`.
    let none = model_calls::produce("x", false, 320, 0);
    assert_eq!(none.of("translate").map(|c| c.calls), Some(0));
    assert!(!none.one_line().contains("translate"), "a zero-count call renders nothing");
}

// ---- S5: Setup -----------------------------------------------------------------------------

#[test]
fn sec_11_flow_index_4_model_calls_at_a_glance_s5_setup_is_the_new_policy_call_once() {
    assert_eq!(ITEM, "§11-flow-index#4-model-calls-at-a-glance");
    // F0.7 — runs when the User Context changes (new): one derivation per change, debounced.
    let s = model_calls::setup();
    assert_eq!(s.stage, Stage::Setup);
    assert_eq!(s.calls.len(), 1, "exactly one model, asked once");
    let policy = &s.calls[0];
    assert_eq!(policy.model, "policy");
    assert_eq!(policy.calls, 1);
    assert!(!policy.web, "the policy call reads the context, not the web");
    assert!(!policy.cached, "not one of cache/llm's five jobs: the debounce is what bounds it");
    assert_eq!(s.asked(), 1);

    // Setup is a step, not a tab: nothing in the shell's four pages corresponds to it.
    assert_eq!(shell::Page::all().len(), 4);
    for page in shell::Page::all() {
        assert_ne!(page.label(), Stage::Setup.label(), "no tab is called Setup");
    }
    // And §4's five stages are these five, in the table's order.
    assert_eq!(
        [Stage::Prepare, Stage::Cut, Stage::Narrate, Stage::Produce, Stage::Setup]
            .map(Stage::label),
        ["Prepare", "Cut", "Narrate", "Produce", "Setup"]
    );
}

// ---- S6: the flags come from §2's tables ----------------------------------------------------

#[test]
fn sec_11_flow_index_4_model_calls_at_a_glance_s6_flags_agree_with_the_job_tables() {
    assert_eq!(ITEM, "§11-flow-index#4-model-calls-at-a-glance");
    let steps = [
        model_calls::prepare(40, 100, MarkingPass::Retakes, 7),
        model_calls::prepare(40, 100, MarkingPass::Joins, 7),
        model_calls::cut(23, CutMode::Model, true, true, true),
        model_calls::narrate(9),
        model_calls::produce("draw it", false, 320, 2),
    ];
    let mut seen = Vec::new();
    for step in &steps {
        for call in &step.calls {
            if let Some(job) = job_of(call.model) {
                assert_eq!(call.web, roles::web_tools(job), "{}'s web flag vs §2", call.model);
                assert_eq!(
                    call.cached,
                    llm_cache::uses_cache(job),
                    "{}'s cached flag vs cache/llm's five jobs",
                    call.model
                );
                seen.push(call.model);
            } else {
                // Only these three have no job to inherit from: `policy` is not one of §2's
                // fourteen jobs, and `tts`/`sd.cpp` are audio.cpp and sd.cpp rather than the LLM.
                assert!(
                    matches!(call.model, "policy" | "tts" | "sd.cpp"),
                    "{} has no job mapping and should not be here",
                    call.model
                );
            }
        }
    }
    // Every mapped model showed up somewhere, so the check above was not vacuous.
    for model in [
        "describe", "fix", "retake", "textedit", "cut", "captions", "speed", "effects",
        "narrate", "translate", "youtube",
    ] {
        assert!(seen.contains(&model), "{model} never appeared to be checked");
    }
}

#[test]
fn sec_11_flow_index_4_model_calls_at_a_glance_s6_line_renders_the_notation() {
    assert_eq!(ITEM, "§11-flow-index#4-model-calls-at-a-glance");
    let plain = Call { model: "speed", calls: 1, web: false, cached: false };
    assert_eq!(plain.line(), "speed \u{d7}1");
    let web = Call { model: "cut", calls: 1, web: true, cached: false };
    assert_eq!(web.line(), "cut \u{d7}1 (+web)");
    let both = Call { model: "describe", calls: 10, web: false, cached: true };
    assert_eq!(both.line(), format!("describe \u{d7}10 \u{b7} cached"));
    assert!(both.line().contains("describe") && both.line().contains("10") && both.line().contains("cached"));
    // Neither marker leaks into a call that has neither.
    assert!(!plain.line().contains("web") && !plain.line().contains("cached"));
    // Zero renders nothing, in the line and in a whole step's summary.
    assert_eq!(Call { model: "translate", calls: 0, web: false, cached: true }.line(), "");
    assert_eq!(
        model_calls::produce("x", false, 320, 0).one_line(),
        "youtube \u{d7}1 (+web) \u{b7} sd.cpp \u{d7}1",
        "the zero-count translate row drops out of the joined line"
    );
    // A step that asks nobody renders empty rather than a stray separator.
    assert_eq!(model_calls::cut(23, CutMode::Words, false, false, false).one_line(), "");
}
