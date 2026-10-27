//! §02-services#3-tool-catalogue-rewrite-directive-b — 3. Tool catalogue, the shared layer (3.1).
//!
//! What every job is offered, the two shapes a result comes back in, and how long a call may keep
//! asking. What an individual tool does to the work belongs to its own section's tests; this file
//! pins the parts all eleven sections share.

use naivepost::roles::Job;
use naivepost::tools::{self, Tool};

/// Every tool §3 names, spelled as the tables spell it, once each.
#[test]
fn sec_02_services_3_tool_catalogue_rewrite_directive_b_s1_the_names_are_the_spec_spelling() {
    let names = [
        // 3.1 shared
        "get_context",
        "get_lines",
        "get_events",
        "web_search",
        "web_read",
        "get_frames",
        "finish",
        // 3.2 describe
        "record_event",
        "set_state",
        "speech_around",
        // 3.3 fix
        "fix_line",
        "flag_line",
        // 3.4 retake marking
        "mark_abandoned",
        "unmark",
        // 3.5 text edit
        "drop_words",
        "keep_join",
        "get_words",
        // 3.6 cut
        "add_segment",
        "remove_segment",
        "set_speed",
        "cut_status",
        "finish_cut",
        // 3.7 captions / speed / decorations
        "add_caption",
        "set_clip_speed",
        "add_effect",
        // 3.8 narration
        "write_line",
        "leave_silent",
        "list_emotions",
        "describe_insert",
        // 3.9 upload text and thumbnail
        "set_title",
        "set_description",
        "pick_frame",
        "set_thumbnail_instruction",
        // 3.10 translate
        "translate_line",
        // 3.11 policy derivation
        "set_policy",
        // 3.12 text gate
        "judge_text",
    ];
    assert_eq!(names.len(), 36);

    // The array above and the enum describe the same set: no name twice, none missing.
    let all = names
        .iter()
        .map(|name| {
            Tool::all()
                .into_iter()
                .find(|tool| tool.name() == *name)
                .unwrap_or_else(|| panic!("{name} is in the spec's tables but not in Tool"))
        })
        .collect::<Vec<_>>();
    assert_eq!(all.len(), names.len());
    for name in names {
        assert_eq!(
            all.iter().filter(|tool| tool.name() == name).count(),
            1,
            "{name} offered more than once"
        );
    }
    // And no variant of the enum is unnamed by §3.
    assert_eq!(Tool::all().len(), names.len());
    for tool in Tool::all() {
        assert!(
            names.contains(&tool.name()),
            "{} is not one of §3's tools",
            tool.name()
        );
    }
}

/// Tools are offered per job: the audio jobs get none, every other job gets the shared set plus its
/// own section's tools and nobody else's.
#[test]
fn sec_02_services_3_tool_catalogue_rewrite_directive_b_s1_tools_are_offered_per_job() {
    // One audio.cpp task call is not a tool loop.
    for which in [Job::Asr, Job::Align, Job::Diarize, Job::Separate] {
        assert!(tools::offered(which).is_empty(), "{which:?} asks no tool");
    }
    // §3.5: the text edit answers the words it removes, quoted, in one reply, and is offered no tool.
    assert!(
        tools::offered(Job::Joins).is_empty(),
        "the text edit asks no tool"
    );
    // §3.3: the transcript fix answers its block's rows back in one reply, and is offered no tool.
    assert!(
        tools::offered(Job::CleanTranscript).is_empty(),
        "the transcript fix asks no tool"
    );

    // Each section's own tools arrive on exactly one job (or, for §3.9's shared state, on the two
    // jobs whose table it is) and nowhere else.
    let exclusive = [
        (Tool::RecordEvent, &[Job::Describe][..]),
        (Tool::SetState, &[Job::Describe]),
        (Tool::SpeechAround, &[Job::Describe]),
        // The fix's own names, kept for the `Block` that applies a respelling, offered to no job (§3.3).
        (Tool::FixLine, &[]),
        (Tool::FlagLine, &[]),
        (Tool::MarkAbandoned, &[Job::Retakes]),
        (Tool::Unmark, &[Job::Retakes]),
        // The join rules' own names, kept for the `Join` that checks a quoted removal, offered to no job.
        (Tool::DropWords, &[]),
        (Tool::KeepJoin, &[]),
        (Tool::GetWords, &[]),
        (Tool::AddSegment, &[Job::ModelCut]),
        (Tool::RemoveSegment, &[Job::ModelCut]),
        (Tool::SetSpeed, &[Job::ModelCut]),
        (Tool::CutStatus, &[Job::ModelCut]),
        (Tool::FinishCut, &[Job::ModelCut]),
        (Tool::AddCaption, &[Job::ClipRules]),
        (Tool::SetClipSpeed, &[Job::ClipRules]),
        (Tool::AddEffect, &[Job::ClipRules]),
        (Tool::WriteLine, &[Job::Narrate]),
        (Tool::LeaveSilent, &[Job::Narrate]),
        (Tool::ListEmotions, &[Job::Narrate]),
        (Tool::DescribeInsert, &[Job::Narrate]),
        (Tool::SetTitle, &[Job::UploadText, Job::Thumbnail]),
        (Tool::SetDescription, &[Job::UploadText, Job::Thumbnail]),
        (Tool::PickFrame, &[Job::UploadText, Job::Thumbnail]),
        (
            Tool::SetThumbnailInstruction,
            &[Job::UploadText, Job::Thumbnail],
        ),
        (Tool::TranslateLine, &[Job::Subtitles]),
    ];
    for (tool, owners) in exclusive {
        for which in Job::all() {
            let offered = tools::offered(which).contains(&tool);
            assert_eq!(
                offered,
                owners.contains(&which),
                "{} on {which:?}: offered {offered}, expected {}",
                tool.name(),
                owners.contains(&which)
            );
        }
    }

    // The shared set is on every job that has tools at all — except the two §3.2/§3.6 rules below.
    for which in Job::all() {
        let list = tools::offered(which);
        if list.is_empty() {
            continue;
        }
        assert!(list.contains(&Tool::GetContext), "{which:?}");
        assert!(list.contains(&Tool::GetFrames), "{which:?}");
    }
    // get_lines is shared, but a chunk of frames has no transcript lines to read (§3.2's Reads row).
    for which in Job::all() {
        let list = tools::offered(which);
        if list.is_empty() {
            continue;
        }
        assert_eq!(
            list.contains(&Tool::GetLines),
            which != Job::Describe,
            "{which:?}"
        );
    }

    // The cut ends with finish_cut and has no plain finish: one way to end a job, not two.
    let cut = tools::offered(Job::ModelCut);
    assert!(cut.contains(&Tool::FinishCut));
    assert!(
        !cut.contains(&Tool::Finish),
        "the cut has finish_cut instead"
    );
    for which in Job::all() {
        if which == Job::ModelCut {
            continue;
        }
        let list = tools::offered(which);
        if !list.is_empty() {
            assert!(
                list.contains(&Tool::Finish),
                "{which:?} has no way to finish"
            );
        }
    }

    // No tool is offered twice to one job.
    for which in Job::all() {
        let list = tools::offered(which);
        for tool in &list {
            assert_eq!(
                list.iter().filter(|t| *t == tool).count(),
                1,
                "{} twice on {which:?}",
                tool.name()
            );
        }
    }
}

/// The web goes only where the prototype offered it — cut, narrate, upload text.
#[test]
fn sec_02_services_3_tool_catalogue_rewrite_directive_b_s1_the_web_follows_the_web_tools_column() {
    for which in Job::all() {
        let list = tools::offered(which);
        assert_eq!(
            list.contains(&Tool::WebSearch),
            matches!(which, Job::ModelCut | Job::Narrate | Job::UploadText),
            "web_search on {which:?}"
        );
        assert_eq!(
            list.contains(&Tool::WebRead),
            list.contains(&Tool::WebSearch),
            "{which:?}"
        );
        // §2's column is the single source: both tables agree because one asks the other.
        assert_eq!(
            tools::web_search_offered(which),
            naivepost::roles::web_tools(which)
        );
    }
}

/// A call may run P.eng.llmToolRounds (8) tool-call rounds; beyond that the step gets no answer.
#[test]
fn sec_02_services_3_tool_catalogue_rewrite_directive_b_s1_eight_rounds_then_no_answer() {
    // P.eng.llmToolRounds
    assert_eq!(tools::TOOL_ROUNDS, 8);
    for round in 1..=8 {
        assert!(
            !tools::rounds_exhausted(round),
            "round {round} is inside the budget"
        );
    }
    assert!(tools::rounds_exhausted(9));
    assert!(tools::rounds_exhausted(u32::MAX));

    // The log names the limit and the step, so a stalled run reads as a budget rather than a hang.
    let line = tools::exhausted_log("cut");
    assert!(line.starts_with("!!! "), "{line}");
    assert!(line.contains("cut"), "{line}");
    assert!(line.contains("8"), "{line}");
    // The number comes from the constant, so it cannot drift from the budget it reports.
    assert_eq!(tools::exhausted_log("narrate").matches('8').count(), 1);
}

/// A round with no tool calls and no finish is asked once to call finish, then taken as finished.
#[test]
fn sec_02_services_3_tool_catalogue_rewrite_directive_b_s1_the_finish_nudge_happens_once() {
    // Still calling tools: no nudge owed.
    assert!(!tools::asked_to_finish(0));
    // Silent for one round: this is the round that asks.
    assert!(tools::asked_to_finish(1));
    // Silent twice: already taken as finished, and a second ask would be a round the spec does not
    // give the model.
    assert!(!tools::asked_to_finish(2));
    assert!(!tools::asked_to_finish(9));
}

/// Every problem comes back as `{"error": "..."}` — and never ends the flow.
#[test]
fn sec_02_services_3_tool_catalogue_rewrite_directive_b_s1_a_problem_is_one_actionable_sentence() {
    let body = tools::error("frame 9 is not in this batch");
    assert_eq!(body, r#"{"error":"frame 9 is not in this batch"}"#);

    // Valid JSON whatever the sentence holds: a quote or a newline inside it must not produce a
    // result the client reads as a broken reply.
    let tricky = tools::error("the field \"again\" is missing -- send 0 for never picked up");
    let parsed: serde_json::Value = serde_json::from_str(&tricky).expect("still valid JSON");
    assert_eq!(
        parsed["error"].as_str().unwrap(),
        "the field \"again\" is missing -- send 0 for never picked up"
    );
    let multiline = tools::error("line 3\nhas a break");
    let parsed: serde_json::Value = serde_json::from_str(&multiline).expect("still valid JSON");
    assert!(parsed["error"].as_str().unwrap().contains('\n'));

    // An error object has exactly the one key: no code, no severity for the model to interpret.
    let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(parsed.as_object().unwrap().len(), 1);

    // A success is a short JSON object of the caller's own shape.
    #[derive(serde::Serialize)]
    struct Placed {
        frame: u32,
        at: f64,
    }
    let ok = tools::ok(&Placed { frame: 3, at: 12.5 });
    assert_eq!(ok, r#"{"frame":3,"at":12.5}"#);
}

/// The web caps: eight hits, six thousand bytes, forty-five seconds — one budget for the ladder.
#[test]
fn sec_02_services_3_tool_catalogue_rewrite_directive_b_s1_the_web_caps_are_small_on_purpose() {
    // A page past these bounds is a cost, not an answer: it buys nothing a snippet would not.
    assert_eq!(tools::WEB_SEARCH_MAX_HITS, 8);
    assert_eq!(tools::WEB_READ_MAX_BYTES, 6000);
    assert_eq!(tools::WEB_CALL_SECONDS, 45);

    // Three queries share the one budget rather than each getting 45 s, which would hold a step
    // open for minutes on a search nobody finished.
    let (seconds, queries) = tools::web_ladder_budget();
    assert_eq!((seconds, queries), (45, 3));
}

/// `mm:ss` is how a moment is quoted back to the model — including its own number, when that is the
/// thing that was wrong.
#[test]
fn sec_02_services_3_tool_catalogue_rewrite_directive_b_s1_seconds_are_read_as_mm_ss() {
    assert_eq!(tools::mm_ss(0.0), "00:00");
    assert_eq!(
        tools::mm_ss(59.4),
        "00:59",
        "the fraction is not a rounding question"
    );
    assert_eq!(tools::mm_ss(60.0), "01:00");
    // Past an hour the minutes keep counting: 62:05 says where the second hour is, 02:05 would
    // read as two minutes past something.
    assert_eq!(tools::mm_ss(3725.0), "62:05");
    // A negative number has no place on a timeline, so it gets none in the string either.
    assert_eq!(tools::mm_ss(-3.0), "00:00");
}

/// §3.11 runs once per project (F0.7), so it belongs to no job and is offered separately.
#[test]
fn sec_02_services_3_tool_catalogue_rewrite_directive_b_s1_policy_derivation_is_offered_alone() {
    let list = tools::policy_tools();
    let names: Vec<&str> = list.iter().map(|tool| tool.name()).collect();
    assert_eq!(names, vec!["get_context", "set_policy"]);

    // Not reachable through any job's list: it sets the fields that decide which jobs run at all,
    // so offering it inside one would let a job re-shape the run it is part of.
    for which in Job::all() {
        assert!(
            !tools::offered(which).contains(&Tool::SetPolicy),
            "set_policy leaked into {which:?}"
        );
    }
}

/// §3.1's four shared reads, answered for real. A job is *offered* these by `tools::offered`; before
/// [`naivepost::tool_reads`] nothing in `src` answered one, so a model that asked for the lines around
/// what it was fixing got silence and stopped trying. These drive the dispatcher directly with the rows
/// a leg would hand it — the same data the request was built from, which is the whole point: a read must
/// describe the block being answered, not a fresh copy of the session.
/// The policy pairs `get_context` echoes, leaked once so the borrowed slice can outlive any one test.
fn policy_pairs() -> &'static [(&'static str, String)] {
    let built: Vec<(&'static str, String)> = vec![("markingPass", "retakes".to_string())];
    Box::leak(built.into_boxed_slice())
}

fn reads_context<'a>(
    lines: &'a [naivepost::textfmt::Line],
    events: &'a [naivepost::textfmt::FrameEvent],
) -> naivepost::tool_reads::Context<'a> {
    static FRAMES: [f64; 3] = [10.0, 12.5, 15.0];
    naivepost::tool_reads::Context {
        user_context: "hand-cut demo, no music",
        job: Job::CleanTranscript,
        policy: policy_pairs(),
        lines,
        events,
        frames: &FRAMES,
    }
}

fn call(name: &str, args: serde_json::Value) -> naivepost::exchanges::ToolCall {
    naivepost::exchanges::ToolCall {
        name: name.to_string(),
        args: args.to_string(),
        result: String::new(),
    }
}

#[test]
fn sec_02_services_3_tool_catalogue_rewrite_directive_b_s2_get_lines_answers_with_the_stamped_rows()
{
    let lines = [
        naivepost::textfmt::Line {
            start: 0.0,
            end: 1.5,
            speaker: "S1".into(),
            text: "first row".into(),
        },
        naivepost::textfmt::Line {
            start: 1.5,
            end: 62.25,
            speaker: "S1".into(),
            text: "second row".into(),
        },
        naivepost::textfmt::Line {
            start: 62.25,
            end: 64.0,
            speaker: "S2".into(),
            text: "third row".into(),
        },
    ];
    let ctx = reads_context(&lines, &[]);
    let answer = naivepost::tool_reads::dispatch(
        &ctx,
        &call("get_lines", serde_json::json!({"from": 1, "to": 2})),
    )
    .expect("get_lines is a shared read and must be answered");
    let body: serde_json::Value = serde_json::from_str(&answer).expect("ok shape");
    assert_eq!(body["from"], 1.0);
    assert_eq!(body["to"], 2.0);
    let rows = body["lines"].as_array().expect("a list of rows");
    assert_eq!(
        rows.len(),
        2,
        "line numbers are 1-based over the rows the request numbered"
    );
    // Stamped the way the request stamps them (mm:ss both ends), then speaker, then text: a number the
    // model reads back means the second it was shown.
    assert_eq!(rows[0], "00:00-00:01\tS1\tfirst row");
    assert_eq!(rows[1], "00:01-01:02\tS1\tsecond row");

    // A quoted number is a spelling, not a wrong argument: refusing "4" would turn format into failure.
    let quoted = naivepost::tool_reads::dispatch(
        &ctx,
        &call("get_lines", serde_json::json!({"from": "2", "to": "4"})),
    )
    .expect("answered");
    let body: serde_json::Value = serde_json::from_str(&quoted).unwrap();
    assert_eq!(
        body["lines"].as_array().unwrap().len(),
        2,
        "rows 2 and 3, clamped to what exists"
    );

    // Missing arguments come back as one actionable sentence (§3), never silence and never a guess.
    let missing = naivepost::tool_reads::dispatch(&ctx, &call("get_lines", serde_json::json!({})))
        .expect("answered");
    assert!(
        missing.contains("\"error\""),
        "bad args are an error sentence: {missing}"
    );
    assert!(
        missing.contains("from") && missing.contains("to"),
        "it names what to send: {missing}"
    );
}

#[test]
fn sec_02_services_3_tool_catalogue_rewrite_directive_b_s2_get_events_keeps_rows_that_overlap_the_range(
) {
    let events = [
        naivepost::textfmt::FrameEvent {
            start: 0.0,
            end: 5.0,
            text: "editor open".into(),
        },
        naivepost::textfmt::FrameEvent {
            start: 5.0,
            end: 12.0,
            text: "clip dragged in".into(),
        },
        naivepost::textfmt::FrameEvent {
            start: 20.0,
            end: 24.0,
            text: "export dialog".into(),
        },
    ];
    let ctx = reads_context(&[], &events);
    let answer = naivepost::tool_reads::dispatch(
        &ctx,
        &call("get_events", serde_json::json!({"from": 4.0, "to": 6.0})),
    )
    .expect("get_events is a shared read and must be answered");
    let body: serde_json::Value = serde_json::from_str(&answer).unwrap();
    let rows = body["events"].as_array().unwrap();
    // Overlap, not containment: a reading that started before the window still describes the picture
    // inside it, which is the rule fix_transcripts::context_text already applies unasked.
    assert_eq!(
        rows.len(),
        2,
        "the two rows whose span touches 4-6 s: {rows:?}"
    );
    assert!(rows[0].as_str().unwrap().contains("editor open"));
    assert!(rows[1].as_str().unwrap().contains("clip dragged in"));
}

#[test]
fn sec_02_services_3_tool_catalogue_rewrite_directive_b_s2_get_context_echoes_the_context_and_this_jobs_policy(
) {
    let ctx = reads_context(&[], &[]);
    let answer = naivepost::tool_reads::dispatch(&ctx, &call("get_context", serde_json::json!({})))
        .expect("get_context is a shared read and must be answered");
    let body: serde_json::Value = serde_json::from_str(&answer).unwrap();
    assert_eq!(body["userContext"], "hand-cut demo, no music");
    assert_eq!(
        body["policy"]["markingPass"], "retakes",
        "only the fields this job reads"
    );
    assert!(
        body["job"].is_string(),
        "the job says which rules these answers are under"
    );
}

#[test]
fn sec_02_services_3_tool_catalogue_rewrite_directive_b_s2_finish_is_left_to_the_job_that_owns_its_check_list(
) {
    let ctx = reads_context(&[], &[]);
    // Every job finishes with ITS OWN check list (§3.2's frames with no event, §3.8's clips with no
    // line, §3.6's segments and footage), so a generic ok here would pre-empt all of them.
    assert!(
        naivepost::tool_reads::dispatch(&ctx, &call("finish", serde_json::json!({}))).is_none(),
        "finish belongs to the caller's job tools"
    );
    assert!(
        naivepost::tool_reads::dispatch(&ctx, &call("fix_line", serde_json::json!({"n": 1})))
            .is_none(),
        "a job's own tool is not the shared layer's to answer"
    );
}
