//! §02-services#2-model-roles-which-job-asks-which-server: the fourteen jobs of §2's table — who
//! answers each, whether it thinks, whether it gets the web, and what its answer may change.
//!
//! One test per clause of the table. Nothing here talks to a server: [naivepost::roles] is the
//! lookup a request is built from, and §1's table (which URL, which timeout) is tested in
//! tests/suite/four_servers.rs.

use naivepost::project::{CutMode, MarkingPass};
use naivepost::roles as job;
use naivepost::services::Server;

/// The four audio rows: audio.cpp answers, and none of them has a thinking token to set.
#[test]
fn sec_02_services_2_model_roles_which_job_asks_which_server_s1_the_audio_jobs_ask_audio_cpp() {
    for which in [
        job::Job::Asr,
        job::Job::Align,
        job::Job::Diarize,
        job::Job::Separate,
    ] {
        assert_eq!(job::server(which), Server::Audio, "{which:?}");
        // `–` in the thinking column, read as "not applicable" — see roles::thinking.
        assert!(!job::thinking(which), "{which:?} must not ask for thinking");
        assert!(!job::web_tools(which), "{which:?}");
    }

    // Each names the audio model it needs, and the four are distinct: a job that asked for the wrong
    // one would get the wrong model's answer.
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

/// What is on screen: one EVENT per frame plus a running STATE, four frames a call, thinking off.
#[test]
fn sec_02_services_2_model_roles_which_job_asks_which_server_s4_four_frames_a_vision_call() {
    // P.machine.describeFramesPerReq
    assert_eq!(job::VISION_FRAMES_PER_CALL, 4);
    assert_eq!(job::vision_batches(0), 0, "no frames, no call");
    assert_eq!(job::vision_batches(1), 1);
    assert_eq!(
        job::vision_batches(4),
        1,
        "exactly a batch fits in one call"
    );
    assert_eq!(
        job::vision_batches(5),
        2,
        "a fifth frame needs a second call"
    );
    assert_eq!(job::vision_batches(8), 2);
    assert_eq!(job::vision_batches(9), 3);
    // Every frame goes out exactly once: whole calls of four plus a shorter last one.
    for frames in [0usize, 1, 3, 4, 7, 100, 101] {
        let calls = job::vision_batches(frames);
        assert!(calls * 4 >= frames, "{frames} in {calls} calls");
        // And no call is wasted: dropping one would leave a frame unsent.
        assert!(
            frames == 0 || (calls - 1) * 4 < frames,
            "{frames} in {calls} calls"
        );
    }

    // The row's other two columns: the LLM with vision, thinking off.
    assert_eq!(job::server(job::Job::Describe), Server::Llm);
    assert!(!job::thinking(job::Job::Describe));
    assert!(!job::web_tools(job::Job::Describe));
}

/// Joins (markingPass joins): thinking on, and only ever run under that pass.
#[test]
fn sec_02_services_2_model_roles_which_job_asks_which_server_s6_joins_think_and_only_under_joins() {
    // P.machine.textEditThinking: the measured case where reasoning pays (15/29 → 20/29).
    assert!(job::TEXT_EDIT_THINKING);
    assert!(job::thinking(job::Job::Joins));
    assert_eq!(job::server(job::Job::Joins), Server::Llm);
    // Not one of the `(+ web tools)` rows: a join is decided from the two takes in front of it.
    assert!(!job::web_tools(job::Job::Joins));

    // Only under markingPass joins — and never alongside retakes, which is the other pass.
    for mode in [CutMode::Words, CutMode::Model] {
        assert!(job::job_applies(job::Job::Joins, MarkingPass::Joins, mode));
        assert!(!job::job_applies(
            job::Job::Joins,
            MarkingPass::Retakes,
            mode
        ));
        // "No marking pass" asks for neither marking job.
        assert!(!job::job_applies(job::Job::Joins, MarkingPass::None, mode));
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
    assert!(
        job::cut_attempts_exhausted(3),
        "the third attempt is the last"
    );
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

/// Thumbnail: sd.cpp, and only when an instruction exists and no frame was named.
#[test]
fn sec_02_services_2_model_roles_which_job_asks_which_server_s11_a_thumbnail_needs_an_instruction()
{
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
    assert!(job::job_applies(
        job::Job::Thumbnail,
        MarkingPass::None,
        CutMode::Words
    ));
}
