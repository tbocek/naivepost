//! §02-services#2-model-roles-which-job-asks-which-server: the fourteen jobs of §2's table — who
//! answers each, whether it thinks, whether it gets the web, and what its answer may change.
//!
//! One test per clause of the table. Nothing here talks to a server: [naivepost::roles] is the
//! lookup a request is built from, and §1's table (which URL, which timeout) is tested in
//! tests/four_servers.rs.

use naivepost::project::{CutMode, MarkingPass};
use naivepost::roles as job;
use naivepost::services::Server;

/// The four audio rows: audio.cpp answers, and none of them has a thinking token to set.
#[test]
fn sec_02_services_2_model_roles_which_job_asks_which_server_s1_the_audio_jobs_ask_audio_cpp() {
    for which in [job::Job::Asr, job::Job::Align, job::Job::Diarize, job::Job::Separate] {
        assert_eq!(job::server(which), Server::Audio, "{which:?}");
        // `–` in the thinking column, read as "not applicable" — see roles::thinking.
        assert!(!job::thinking(which), "{which:?} must not ask for thinking");
        assert!(!job::web_tools(which), "{which:?}");
    }

    // Each names the audio model it needs, and the four are distinct: two models on one server
    // are two slot counts (§1), so a job that asked for the wrong one would queue behind work it
    // never did.
    use naivepost::services::AudioTask;
    assert_eq!(job::audio_task(job::Job::Asr), Some(AudioTask::Asr));
    assert_eq!(job::audio_task(job::Job::Align), Some(AudioTask::Align));
    assert_eq!(job::audio_task(job::Job::Diarize), Some(AudioTask::Diar));
    assert_eq!(job::audio_task(job::Job::Separate), Some(AudioTask::Sep));

    // The LLM and sd.cpp jobs have no audio model id to send.
    for which in job::Job::all() {
        if job::server(which) != Server::Audio {
            assert_eq!(job::audio_task(which), None, "{which:?}");
        }
    }
    // And no audio job is answered by anything but audio.cpp.
    for which in job::Job::all() {
        if job::audio_task(which).is_some() {
            assert_eq!(job::server(which), Server::Audio, "{which:?}");
        }
    }
}

/// Speech to text: ≤ 60 s for the qwen3 family, else ≤ 300 s, halved on an out-of-memory answer
/// down to 20 s.
#[test]
fn sec_02_services_2_model_roles_which_job_asks_which_server_s2_asr_chunks_halve_to_the_floor() {
    // P.machine.asrChunkQwenSeconds / P.machine.asrChunkMaxSeconds
    assert_eq!(job::asr_chunk_seconds("qwen3-asr", 0), 60.0);
    assert_eq!(job::asr_chunk_seconds("nemotron", 0), 300.0);
    // The family is whatever the server declared (§1's model list), so it is matched loosely and
    // without regard to case.
    assert!(job::asr_chunk_is_qwen_family("Qwen3-ASR"));
    assert!(job::asr_chunk_is_qwen_family("qwen3-asr-1.7b"));
    assert!(!job::asr_chunk_is_qwen_family("nemotron"));
    assert_eq!(job::asr_chunk_seconds("Qwen3", 0), 60.0);

    // Halved per out-of-memory answer.
    assert_eq!(job::asr_chunk_seconds("nemotron", 1), 150.0);
    assert_eq!(job::asr_chunk_seconds("nemotron", 2), 75.0);
    assert_eq!(job::asr_chunk_seconds("qwen3-asr", 1), 30.0);

    // P.machine.asrChunkMinSeconds: the floor, and reachable from the big chunk.
    assert_eq!(job::ASR_CHUNK_MIN_SECONDS, 20.0);
    assert_eq!(job::asr_chunk_seconds("nemotron", 3), 37.5);
    assert_eq!(job::asr_chunk_seconds("nemotron", 4), 20.0);
    // Past the floor it stays: a chunk shorter than a sentence transcribes worse than the failure
    // it was avoiding.
    assert_eq!(job::asr_chunk_seconds("nemotron", 5), 20.0);
    assert_eq!(job::asr_chunk_seconds("nemotron", 40), 20.0);
    // The qwen3 family halves from its own ceiling, and lands on the same floor.
    assert_eq!(job::asr_chunk_seconds("qwen3-asr", 9), 20.0);

    // The constants are the spec's numbers.
    assert_eq!(job::ASR_CHUNK_QWEN_SECONDS, 60.0); // P.machine.asrChunkQwenSeconds
    assert_eq!(job::ASR_CHUNK_MAX_SECONDS, 300.0); // P.machine.asrChunkMaxSeconds
}

/// Who spoke when: speaker turns per window, the 90 → 45 → 25 s ladder.
#[test]
fn sec_02_services_2_model_roles_which_job_asks_which_server_s3_the_diarization_ladder() {
    // P.machine.diarWindowsSeconds (measured: 90 passes, 150 fails)
    assert_eq!(job::diar_ladder(), [90.0, 45.0, 25.0]);
    assert_eq!(job::diar_window(0), 90.0);
    assert_eq!(job::diar_window(1), 45.0);
    assert_eq!(job::diar_window(2), 25.0);

    // Past the ladder the smallest window repeats rather than a new size being invented, and the
    // window never grows: each retry is asked to fit in less.
    assert_eq!(job::diar_window(3), 25.0);
    assert_eq!(job::diar_window(99), 25.0);
    let mut last = f64::INFINITY;
    for attempt in 0..8 {
        let window = job::diar_window(attempt);
        assert!(window <= last, "attempt {attempt} grew to {window}");
        last = window;
    }

    // Diarization is an audio.cpp job with no thinking and no web (§1's table asks it of the
    // server's diar model).
    assert_eq!(job::server(job::Job::Diarize), Server::Audio);
    assert!(!job::thinking(job::Job::Diarize));
}

/// What is on screen: one EVENT per frame plus a running STATE, four frames a call, thinking off.
#[test]
fn sec_02_services_2_model_roles_which_job_asks_which_server_s4_four_frames_a_vision_call() {
    // P.machine.describeFramesPerReq
    assert_eq!(job::VISION_FRAMES_PER_CALL, 4);
    assert_eq!(job::vision_batches(0), 0, "no frames, no call");
    assert_eq!(job::vision_batches(1), 1);
    assert_eq!(job::vision_batches(4), 1, "exactly a batch fits in one call");
    assert_eq!(job::vision_batches(5), 2, "a fifth frame needs a second call");
    assert_eq!(job::vision_batches(8), 2);
    assert_eq!(job::vision_batches(9), 3);
    // Every frame goes out exactly once: whole calls of four plus a shorter last one.
    for frames in [0usize, 1, 3, 4, 7, 100, 101] {
        let calls = job::vision_batches(frames);
        assert!(calls * 4 >= frames, "{frames} in {calls} calls");
        // And no call is wasted: dropping one would leave a frame unsent.
        assert!(frames == 0 || (calls - 1) * 4 < frames, "{frames} in {calls} calls");
    }

    // The row's other two columns: the LLM with vision, thinking off.
    assert_eq!(job::server(job::Job::Describe), Server::Llm);
    assert!(!job::thinking(job::Job::Describe));
    assert!(!job::web_tools(job::Job::Describe));
}

/// Clean transcript: the same lines respelled, times and speakers unchanged — enforced.
#[test]
fn sec_02_services_2_model_roles_which_job_asks_which_server_s5_a_respell_cannot_move_a_time() {
    // The original lines: start, end, speaker, and a fourth field the job does not care about.
    let originals = vec![
        (0.0, 3.5, "1".to_string(), 42u32),
        (3.5, 8.0, "2".to_string(), 7u32),
        (8.0, 9.25, "1".to_string(), 1u32),
    ];
    let respelled: Vec<String> = vec!["first".into(), "second".into(), "third".into()];

    let out = job::clean_transcript(&originals, &respelled).expect("a same-length reply");
    assert_eq!(out.len(), 3);
    // The words are the reply's...
    let texts: Vec<&str> = out.iter().map(|(_, _, _, t)| t.as_str()).collect();
    assert_eq!(texts, vec!["first", "second", "third"]);
    // ...and everything else is the original's, line by line. A reply that renumbered the
    // speakers or shifted a time could not reach here even if it tried.
    let times: Vec<(f64, f64, String)> =
        out.iter().map(|(s, e, sp, _)| (*s, *e, sp.clone())).collect();
    assert_eq!(
        times,
        vec![
            (0.0, 3.5, "1".to_string()),
            (3.5, 8.0, "2".to_string()),
            (8.0, 9.25, "1".to_string())
        ]
    );

    // A reply that dropped a line, or invented one, is refused rather than re-paired: pairing the
    // surviving words with the times of different lines reads like a success.
    assert_eq!(job::clean_transcript(&originals, &respelled[..2]), None);
    let too_many: Vec<String> = respelled.iter().cloned().chain(["extra".into()]).collect();
    assert_eq!(job::clean_transcript(&originals, &too_many), None);
    // An empty transcript with an empty reply is the same length, and correct.
    assert_eq!(job::clean_transcript::<u32>(&[], &[]), Some(vec![]));

    // Thinking off: this job rewrites text, it does not reason about it.
    assert!(!job::thinking(job::Job::CleanTranscript));
    assert_eq!(job::server(job::Job::CleanTranscript), Server::Llm);
}

/// Joins (markingPass joins): thinking on, and only ever run under that pass.
#[test]
fn sec_02_services_2_model_roles_which_job_asks_which_server_s6_joins_think_and_only_under_joins()
{
    // P.machine.textEditThinking: the measured case where reasoning pays (15/29 → 20/29).
    assert!(job::TEXT_EDIT_THINKING);
    assert!(job::thinking(job::Job::Joins));
    assert_eq!(job::server(job::Job::Joins), Server::Llm);
    // Not one of the `(+ web tools)` rows: a join is decided from the two takes in front of it.
    assert!(!job::web_tools(job::Job::Joins));

    // Only under markingPass joins — and never alongside retakes, which is the other pass.
    for mode in [CutMode::Words, CutMode::Model] {
        assert!(job::job_applies(job::Job::Joins, MarkingPass::Joins, mode));
        assert!(!job::job_applies(job::Job::Joins, MarkingPass::Retakes, mode));
        // "No marking pass" asks for neither marking job.
        assert!(!job::job_applies(job::Job::Joins, MarkingPass::None, mode));
    }
}

/// Retakes (markingPass retakes): thinking off, three runs pooled, only under that pass.
#[test]
fn sec_02_services_2_model_roles_which_job_asks_which_server_s7_retakes_are_pooled_not_thought() {
    assert!(!job::thinking(job::Job::Retakes));
    assert_eq!(job::server(job::Job::Retakes), Server::Llm);
    assert!(!job::web_tools(job::Job::Retakes));

    // P.machine.retakeRuns: three identical calls pooled.
    assert_eq!(job::retake_runs_pooled(), 3);
    assert_eq!(job::RETAKE_RUNS_POOLED, 3);
    // Pooling is about repeats, not volume: distinct stretches each still need their own call, so
    // the count of calls equals the count of distinct stretches handed over.
    assert_eq!(job::retake_batches(0), 0);
    assert_eq!(job::retake_batches(1), 1);
    assert_eq!(job::retake_batches(5), 5);

    for mode in [CutMode::Words, CutMode::Model] {
        assert!(job::job_applies(job::Job::Retakes, MarkingPass::Retakes, mode));
        assert!(!job::job_applies(job::Job::Retakes, MarkingPass::Joins, mode));
        assert!(!job::job_applies(job::Job::Retakes, MarkingPass::None, mode));
    }
}

/// Model cut: three attempts, web tools withdrawn after the first rejected one, thinking off for a
/// retry when the model reasoned and wrote nothing, and only under cutMode model.
#[test]
fn sec_02_services_2_model_roles_which_job_asks_which_server_s8_the_cut_gets_three_attempts() {
    // P.eng.llmAttempts: three attempts, and the web goes after the first rejection.
    assert_eq!(job::LLM_ATTEMPTS, 3);
    let first = job::cut_attempt(1);
    assert_eq!(first.attempt, 1);
    assert!(first.web_tools, "the first attempt may go looking");
    assert!(first.thinking);

    for attempt in [2, 3] {
        let next = job::cut_attempt(attempt);
        assert!(!next.web_tools, "attempt {attempt} still has the web");
    }
    // The row itself is a `(+ web tools)` row: it is the retry that loses them.
    assert!(job::web_tools(job::Job::ModelCut));

    assert!(!job::cut_attempts_exhausted(1));
    assert!(!job::cut_attempts_exhausted(2));
    assert!(job::cut_attempts_exhausted(3), "the third attempt is the last");
    assert!(job::cut_attempts_exhausted(4));

    // Thinking: on for a retry that wrote something and had it rejected (the answer was written,
    // it was just wrong)...
    assert!(job::cut_retry_thinking(false, false));
    assert!(job::cut_retry_thinking(true, false));
    assert!(job::cut_retry_thinking(false, true));
    // ...and off when the model reasoned and wrote nothing: reasoning again into the same void
    // produces more thinking and still no answer.
    assert!(!job::cut_retry_thinking(true, true));

    // Only under cutMode model; under words the timeline comes from the words alone and nobody is
    // asked.
    for pass in [MarkingPass::Joins, MarkingPass::Retakes, MarkingPass::None] {
        assert!(job::job_applies(job::Job::ModelCut, pass, CutMode::Model));
        assert!(!job::job_applies(job::Job::ModelCut, pass, CutMode::Words));
    }

    // A call may run this many tool rounds before the step gets no answer from it.
    assert_eq!(job::TOOL_ROUNDS, 8); // P.eng.llmToolRounds
}

/// Captions, speed and effects: per clip, in the clip's own seconds, thinking off.
#[test]
fn sec_02_services_2_model_roles_which_job_asks_which_server_s9_clip_rules_are_per_clip_off() {
    assert_eq!(job::server(job::Job::ClipRules), Server::Llm);
    assert!(!job::thinking(job::Job::ClipRules));
    assert!(!job::web_tools(job::Job::ClipRules));
    // Not gated by the marking pass or the cut mode: every project gets its captions.
    for pass in [MarkingPass::Joins, MarkingPass::Retakes, MarkingPass::None] {
        for mode in [CutMode::Words, CutMode::Model] {
            assert!(job::job_applies(job::Job::ClipRules, pass, mode));
        }
    }
}

/// Narration (LLM + web → TTS) and upload text (LLM + web): thinking on, both get the web, and
/// only narration's answer is spoken after it arrives.
#[test]
fn sec_02_services_2_model_roles_which_job_asks_which_server_s10_narration_and_upload_text() {
    for which in [job::Job::Narrate, job::Job::UploadText] {
        assert_eq!(job::server(which), Server::Llm, "{which:?}");
        assert!(job::thinking(which), "{which:?}");
        assert!(job::web_tools(which), "{which:?} is a (+ web tools) row");
    }

    // The `→ TTS` arrow belongs to narration alone: its line per clip with an emotion is spoken in
    // the cloned voice once the model has written it.
    assert!(job::answers_after_tts(job::Job::Narrate));
    assert!(!job::answers_after_tts(job::Job::UploadText));
    // And upload text is the row whose answer names a frame *or* an instruction — the instruction
    // being what may then ask sd.cpp, see s11.
    assert!(!job::answers_after_tts(job::Job::Thumbnail));

    // Neither job is gated by policy: both run whatever the marking pass was.
    for which in [job::Job::Narrate, job::Job::UploadText] {
        assert!(job::job_applies(which, MarkingPass::None, CutMode::Words));
    }
}

/// Thumbnail: sd.cpp, and only when an instruction exists and no frame was named.
#[test]
fn sec_02_services_2_model_roles_which_job_asks_which_server_s11_a_thumbnail_needs_an_instruction() {
    assert_eq!(job::server(job::Job::Thumbnail), Server::Image);
    // `–` in the thinking column: sd.cpp has no thinking token either.
    assert!(!job::thinking(job::Job::Thumbnail));
    assert!(!job::web_tools(job::Job::Thumbnail));

    // The one case that asks: an instruction, and no frame named.
    assert!(job::thumbnail_wanted("a close-up of the diagram", false));
    // A named frame needs no server — it is cut out of the source — so an instruction beside it is
    // an instruction nobody needs.
    assert!(!job::thumbnail_wanted("a close-up of the diagram", true));
    // No instruction: nothing to draw from, and the first frame stands.
    assert!(!job::thumbnail_wanted("", false));
    assert!(!job::thumbnail_wanted("   \n ", false));
    // Both missing at once is still no.
    assert!(!job::thumbnail_wanted("", true));

    // Not gated by policy either: a thumbnail is asked for when the instruction says so.
    assert!(job::job_applies(job::Job::Thumbnail, MarkingPass::None, CutMode::Words));
}

/// Subtitles in other languages: numbered lines translated, one call per language, thinking off.
#[test]
fn sec_02_services_2_model_roles_which_job_asks_which_server_s12_one_call_per_language() {
    assert_eq!(job::server(job::Job::Subtitles), Server::Llm);
    assert!(!job::thinking(job::Job::Subtitles));
    assert!(!job::web_tools(job::Job::Subtitles));

    // One call per language, however many there are.
    assert_eq!(job::subtitle_calls(&[]), 0);
    assert_eq!(job::subtitle_calls(&["German"]), 1);
    assert_eq!(job::subtitle_calls(&["German", "French", "English"]), 3);

    // The settings list is `code:tag:name` (00-principles.md §6, SUBTITLE_LANGUAGES in
    // 03-shell.md §6), so the call asks for the name and a filename still has the code.
    let langs = job::subtitle_languages("de:ger:German, fr:fre:French, en:eng:English");
    assert_eq!(langs.len(), 3);
    assert_eq!(langs[0].code, "de");
    assert_eq!(langs[0].tag, "ger");
    assert_eq!(langs[0].ask_for(), "German");
    assert_eq!(job::subtitle_calls(
        &langs.iter().map(|l| l.ask_for()).collect::<Vec<_>>()
    ), 3);

    // Order is the settings' order — the list is what decides which file is written first — and a
    // blank entry asks for nothing.
    let langs = job::subtitle_languages("fr:fre:French,de:ger:German");
    assert_eq!(
        langs.iter().map(|l| l.ask_for()).collect::<Vec<_>>(),
        vec!["French", "German"]
    );
    assert!(job::subtitle_languages("").is_empty());
    assert!(job::subtitle_languages("  ,,  ").is_empty());

    // A shorter entry fills what it left out from the code rather than asking for an empty string,
    // and a field present but blank counts as absent.
    let langs = job::subtitle_languages("de");
    assert_eq!((langs[0].code.as_str(), langs[0].ask_for()), ("de", "de"));
    let langs = job::subtitle_languages("de::German");
    assert_eq!(langs[0].tag, "de");
    assert_eq!(langs[0].ask_for(), "German");
}

/// The whole table at once: every row's server, thinking and web tools in one place, so a job
/// added to [`job::Job::all()`] fails here until it is described too.
#[test]
fn sec_02_services_2_model_roles_which_job_asks_which_server_s13_the_table_row_by_row() {
    let table = [
        // job, server, thinking (false covers both `off` and `–`), web tools
        (job::Job::Asr, Server::Audio, false, false),
        (job::Job::Align, Server::Audio, false, false),
        (job::Job::Diarize, Server::Audio, false, false),
        (job::Job::Separate, Server::Audio, false, false),
        (job::Job::Describe, Server::Llm, false, false),
        (job::Job::CleanTranscript, Server::Llm, false, false),
        (job::Job::Joins, Server::Llm, true, false),
        (job::Job::Retakes, Server::Llm, false, false),
        (job::Job::ModelCut, Server::Llm, true, true),
        (job::Job::ClipRules, Server::Llm, false, false),
        (job::Job::Narrate, Server::Llm, true, true),
        (job::Job::UploadText, Server::Llm, true, true),
        (job::Job::Thumbnail, Server::Image, false, false),
        (job::Job::Subtitles, Server::Llm, false, false),
    ];
    assert_eq!(job::Job::all().len(), table.len(), "a row was added or lost");

    for (which, server, thinking, web) in table {
        assert_eq!(job::server(which), server, "{which:?} server");
        assert_eq!(job::thinking(which), thinking, "{which:?} thinking");
        assert_eq!(job::web_tools(which), web, "{which:?} web tools");
    }

    // Every job is described: `all()` covers each variant exactly once, so no row can hide from
    // the asserts above.
    let all = job::Job::all();
    for which in all {
        assert_eq!(all.iter().filter(|j| **j == which).count(), 1, "{which:?} twice");
    }

    // The `–` rows are exactly the ones no server has a thinking token for: the four audio jobs
    // and sd.cpp. They agree with `off` in behaviour, which is what `thinking` returns; what they
    // must not do is gain a web tool or change server.
    for which in [
        job::Job::Asr,
        job::Job::Align,
        job::Job::Diarize,
        job::Job::Separate,
        job::Job::Thumbnail,
    ] {
        assert!(!job::thinking(which), "{which:?}");
        assert_eq!(job::answers_after_tts(which), false, "{which:?}");
    }
    // And exactly one row ends in a TTS call.
    let spoken: Vec<_> = all
        .iter()
        .filter(|j| job::answers_after_tts(**j))
        .copied()
        .collect();
    assert_eq!(spoken, vec![job::Job::Narrate]);
}
