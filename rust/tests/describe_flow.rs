//! F1.7 Describe — the stage's rules, tested without a window or a model.
//!
//! spec/04-prepare.md F1.7: which frames go (S0), how they are scaled for sending (S1), how a
//! stopped run resumes (S2), what the request says (S3), how it is cached (S4), the tools (S5),
//! where the answers land (S6) and what gets logged (S7). The tool half itself lives in
//! `naivepost::tools::describe` and is exercised here through `describe::apply_tool`, the one door
//! the flow uses.

use std::fs;
use std::path::{Path, PathBuf};

use naivepost::describe;
use naivepost::layout::Tree;
use naivepost::tools::describe::{self as tool, Batch, Event, MissingReason};

/// A scratch project folder, the way `tests/prepare_run_flow.rs::scratch` makes one.
fn scratch(tag: &str) -> (PathBuf, Tree) {
    let root = std::env::temp_dir().join(format!("naivepost-f17-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let dir = root.join(format!("{tag}.naivepost"));
    fs::create_dir_all(&dir).expect("project folder");
    let tree = Tree::new(&dir).expect("a .naivepost folder is a project");
    (root, tree)
}

// ---- S0 ---------------------------------------------------------------------

/// S0: a 12 s take with one change at 4 s, at Freq 1.0, sends twelve frames — one per second,
/// including the change's own first frame. P.project.frameInterval
#[test]
fn f1_7_s0_one_frame_every_freq_per_scene() {
    let picked = describe::pick_frames(&[4.0], 12.0, 0.25, 1.0);
    assert_eq!(picked.len(), 12, "twelve seconds at Freq 1.0 is twelve frames: {picked:?}");
    assert_eq!(picked.first(), Some(&0.0));
    assert!(picked.contains(&4.0), "the scene change's first frame is there: {picked:?}");
    assert!(!picked.contains(&12.0), "nothing past the take's end");
}

/// S0: a scene shorter than Freq sends its first frame ONLY. This is the whole reason the loop
/// starts from the scene's own first frame rather than from zero.
#[test]
fn f1_7_s0_a_scene_shorter_than_freq_sends_only_its_first_frame() {
    // 5 s take, changes at 0.7 (scene of 0.3 s) and 1.0 (scene of 4 s).
    let picked = describe::pick_frames(&[0.7, 1.0], 5.0, 0.25, 1.0);
    let in_short_scene = picked.iter().filter(|at| **at >= 0.7 && **at < 1.0).count();
    assert_eq!(in_short_scene, 1, "the 0.3 s scene sends exactly its first frame: {picked:?}");
    let short_first = *picked.iter().find(|at| **at >= 0.7).expect("the short scene sent a frame");
    assert!(short_first < 1.0 && short_first >= 0.7, "and it is the change's own frame, snapped up onto the grid: {short_first}");
}

/// S0: the first picture of EVERY new scene reaches the model, however many there are and wherever
/// they fall relative to the Freq steps.
#[test]
fn f1_7_s0_every_scene_change_has_its_first_picture() {
    let changes = [1.1, 2.35, 3.9, 7.05];
    let picked = describe::pick_frames(&changes, 10.0, 0.25, 1.0);
    for change in changes {
        let has_own = picked
            .iter()
            .any(|at| *at >= change && *at - change < 0.25 + 1e-9);
        assert!(has_own, "no frame covers the change at {change}s: {picked:?}");
    }
    // And no frame is written twice where a Freq step lands on a change.
    let mut sorted = picked.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    for pair in sorted.windows(2) {
        assert!((pair[1] - pair[0]).abs() > 1e-9, "duplicate frame in {picked:?}");
    }
}

/// S0: an every-frame folder is refused with §F1.7's exact sentence -- describe cannot stamp, chunk
/// or resume arithmetic off a set with no known interval.
#[test]
fn f1_7_s0_an_every_frame_folder_is_refused() {
    // A folder extracted frame-by-frame: every picture one grid step apart, nothing thinned.
    let raw: Vec<f64> = (0..49).map(|i| i as f64 * 0.25).collect();
    assert!(
        describe::is_every_frame(&raw, 0.25),
        "pictures sitting `grid` apart all the way through is an every-frame extraction"
    );
    assert_eq!(
        describe::every_frame_refusal("lecture"),
        "lecture was extracted as every-frame; describe needs a fixed interval -- rerun Prepare with e.g. 1s"
    );
    // What F1.6 writes for any Freq coarser than the grid has wider gaps, so it is NOT refused.
    let picked = describe::pick_frames(&[4.0], 12.0, 0.25, 1.0);
    assert!(
        !describe::is_every_frame(&picked, 0.25),
        "the Freq-thinned set of {} frames is not every-frame: {:?}",
        picked.len(),
        picked
    );
    // A short take is not every-frame either -- this is why the test reads spacing, not count.
    let tiny = vec![0.0];
    assert!(!describe::is_every_frame(&tiny, 0.25), "one frame cannot be judged by spacing");
    assert!(!describe::is_every_frame(&[], 0.25), "an empty folder is missing, not every-frame");
}

// ---- S1 ---------------------------------------------------------------------

/// S1: frames are sorted by stamp before anything numbers them, because the request's FRAME labels
/// and events.tsv's rows both come off that order.
#[test]
fn f1_7_s1_frames_are_sorted_by_stamp() {
    let sorted = describe::sorted(&[3.0, 0.75, 1.0]);
    assert_eq!(sorted, vec![0.75, 1.0, 3.0]);
}

/// S1: an empty (or missing) frame list is refused. A scene shorter than Freq is NOT this case --
/// it sends its single frame and is ordinary.
#[test]
fn f1_7_s1_a_missing_or_empty_folder_is_refused() {
    assert!(describe::refuse_missing(&[]).is_some(), "no frames at all is refused");
    assert!(describe::refuse_missing(&[0.0]).is_none(), "one frame is enough to describe");
}

/// S1: each frame that goes is scaled ONCE to P.machine.describeFrameWidth wide, height by aspect,
/// into `.llmframes/`; nothing is spawned. P.machine.describeFrameWidth
#[test]
fn f1_7_s1_the_scale_plan_hits_896_wide_into_llmframes() {
    let plan = describe::scale_plan(describe::FRAME_WIDTH, &[1.0, 0.0]);
    let joined = plan.join(" ");
    assert!(joined.contains("scale=896:-2"), "896 wide, height by aspect: {joined}");
    assert_eq!(describe::FRAME_WIDTH, 896, "P.machine.describeFrameWidth");
    // One name per frame, in clock order, and the names are the stored frames' own names so a row and
    // a file point at the same instant.
    let names: Vec<&String> = plan.iter().filter(|arg| arg.ends_with(".jpg")).collect();
    assert_eq!(names.len(), 2, "one scaled copy per frame: {joined}");
    assert!(names[0] < names[1], "written in stamp order: {names:?}");
    // Where they land is the layout's own answer, so this stage cannot invent another folder.
    let (_keep, tree) = scratch("scale");
    let llm = tree.llm_frames_dir("lecture");
    assert!(llm.ends_with("describe/lecture/.llmframes"), "{llm:?}");
    assert!(llm.to_string_lossy().contains(".llmframes"), "the scaled copies live under .llmframes/");
}

/// S1: the frames on disk stay the video's own size -- this stage scales only the copies it sends.
/// Proven by pointing at F1.6's own plan, which mentions no scaling of delivered frames.
#[test]
fn f1_7_s1_stored_frames_stay_at_the_videos_own_size() {
    let extract = naivepost::frames::extract_plan("v.mkv", &[0.0, 1.0], Path::new("/tmp/f17-x"), 0.0);
    let joined = extract
        .iter()
        .flatten()
        .cloned()
        .collect::<Vec<String>>()
        .join(" ");
    assert!(
        !joined.contains("896"),
        "the extraction plan that writes the kept frames never scales to the LLM width: {joined}"
    );
}

/// S1: each recording heard alongside the video is logged with its offset.
#[test]
fn f1_7_s1_an_alongside_recording_is_logged_with_its_offset() {
    assert_eq!(
        describe::hearing_log("lecture.mkv", "mic.wav", 12.5),
        ">>> [lecture.mkv] hearing mic.wav alongside it, starting 12.5 s in"
    );
    assert_eq!(
        describe::hearing_log("lecture.mkv", "mic.wav", 0.0),
        ">>> [lecture.mkv] hearing mic.wav alongside it, starting 0 s in",
        "a whole number prints without a decimal tail"
    );
}

// ---- S2 ---------------------------------------------------------------------

/// S2: a chunk whose START already has a row in events.tsv is skipped -- that row is written after
/// the answer arrives, so it proves the whole chunk landed.
#[test]
fn f1_7_s2_chunks_already_in_events_tsv_are_skipped() {
    // 8 frames, 4 per request: chunks start at index 0 (t=0) and 4 (t=4). P.machine.describeFramesPerReq
    let frames: Vec<f64> = (0..8).map(|i| i as f64).collect();
    let done = vec![
        describe::EventRow { at: 0.0, text: "title slide".into() },
        describe::EventRow { at: 4.0, text: "a graph appears".into() },
    ];
    assert_eq!(
        describe::starts_already_logged(&done, &frames, describe::FRAMES_PER_REQ),
        vec![0, 4],
        "both chunks are done"
    );
    // Only the first chunk's start present: the second must run again.
    let one = vec![describe::EventRow { at: 0.0, text: "title slide".into() }];
    assert_eq!(describe::starts_already_logged(&one, &frames, 4), vec![0]);
    // A row in the MIDDLE of a chunk does not mark it done -- that would leave the chunk's tail
    // unanswered forever.
    let middle = vec![describe::EventRow { at: 1.0, text: "middle".into() }];
    assert!(describe::starts_already_logged(&middle, &frames, 4).is_empty(), "{middle:?} marks nothing");
}

/// S2: the rolling window is seeded from the LAST three rows, oldest first.
#[test]
fn f1_7_s2_the_last_three_rows_seed_the_history() {
    let rows: Vec<describe::EventRow> = (1..=5)
        .map(|i| describe::EventRow { at: i as f64, text: format!("e{i}") })
        .collect();
    let seeded = describe::seed_history(&rows);
    assert_eq!(seeded, vec!["e3", "e4", "e5"], "exactly the last RECENT_EVENTS rows");
    assert_eq!(seeded.len(), tool::RECENT_EVENTS, "P.machine.describeRecentEvents = 3");
    // Fewer rows than three seeds with what exists, not with padding.
    assert_eq!(describe::seed_history(&rows[..2]), vec!["e1", "e2"]);
}

/// S2: state.txt seeds STATE; with none, the recording just started -- a real answer, not an empty one.
#[test]
fn f1_7_s2_state_txt_seeds_the_state() {
    assert_eq!(describe::seed_state(Some("Talking about the graph.")), "Talking about the graph.");
    assert_eq!(describe::seed_state(Some("   \n")), "Recording just started.");
    assert_eq!(describe::seed_state(None), "Recording just started.");
}

/// S2: the files on disk feed the resume through the Tree paths, so a killed run picks up where its
/// own writes stopped.
#[test]
fn f1_7_s2_resume_reads_the_pair_off_disk() {
    let (_keep, tree) = scratch("resume");
    fs::create_dir_all(tree.describe_dir("lecture")).unwrap();
    fs::write(
        tree.events_tsv("lecture"),
        "0.0\ttitle slide\n1.0\tsame\n2.0\tthe speaker points left\n",
    )
    .unwrap();
    fs::write(tree.describe_state("lecture"), "Mid-slide.\n").unwrap();

    let rows = describe::read_events(&tree, "lecture");
    assert_eq!(rows.len(), 3, "three rows parsed: {rows:?}");
    assert_eq!(describe::seed_history(&rows), vec!["title slide", "same", "the speaker points left"]);
    assert_eq!(describe::read_state(&tree, "lecture").map(|s| s.trim().to_string()), Some("Mid-slide.".into()));

    // A half-written trailing line from a killed run is skipped, not fatal.
    fs::write(tree.events_tsv("lecture"), "0.0\tok\n1.0\tha").unwrap();
    assert_eq!(
        describe::read_events(&tree, "lecture").len(),
        2,
        "a truncated TEXT still describes its frame and is kept; only an unreadable STAMP is dropped"
    );
    fs::write(tree.events_tsv("lecture"), "0.0\tok\nnot-a-second\tgone\n").unwrap();
    assert_eq!(describe::read_events(&tree, "lecture").len(), 1, "a line with no parsable second is dropped");
}

// ---- S3 ---------------------------------------------------------------------

/// S3: the message carries, in order, STATE, the t=A to t=B line, the last events, all THREE speech
/// sections, then one labelled frame each.
#[test]
fn f1_7_s3_the_message_layout_is_the_specs() {
    let speech = describe::Speech {
        before: vec!["before line".into()],
        during: vec!["during line".into()],
        after: vec![],
    };
    let stamps = vec![
        describe::Stamp { at: 0.0, new_scene: true },
        describe::Stamp { at: 0.8, new_scene: false },
        describe::Stamp { at: 1.6, new_scene: false },
        describe::Stamp { at: 2.4, new_scene: false },
    ];
    let text = describe::message(
        "Ada presents the deck.",
        "Mid-slide.",
        0.0,
        2.4,
        &["one".into(), "two".into()],
        &speech,
        &stamps,
    );
    let head = text.lines().take(3).collect::<Vec<_>>().join("\n");
    assert!(text.starts_with("Ada presents the deck."), "User Context leads: {head}");
    assert!(text.contains("STATE so far: Mid-slide."), "STATE line: {head}");
    assert!(
        text.contains("Frames cover t=0s to t=2.4s. A frame marked NEW SCENE is the first after a scene change."),
        "the covering line, verbatim: {head}"
    );
    assert!(text.contains("Just before this: one | two"), "the last events ride along");
    for heading in tool::SPEECH_HEADINGS {
        assert!(text.contains(heading), "all three speech headings are present, missing {heading}");
    }
    let order: Vec<usize> = tool::SPEECH_HEADINGS.iter().map(|h| text.find(h).unwrap()).collect();
    assert!(order.windows(2).all(|p| p[0] < p[1]), "before, during, after, in that order: {order:?}");
    assert!(text.contains("[+0.0s] FRAME 1 of 4, NEW SCENE"), "the scene's first frame is marked: {text}");
    assert!(text.contains("[+0.8s] FRAME 2 of 4\n"), "an ordinary frame is unmarked");
    assert!(text.contains("[+2.4s] FRAME 4 of 4"), "the batch's last frame is numbered");
    assert!(
        text.find("--- spoken during these frames ---").unwrap() < text.find("[+0.0s] FRAME 1").unwrap(),
        "the speech block precedes the frames"
    );
}

/// S3: with no history the `Just before this:` block is absent entirely -- an empty heading would
/// invite an invented lead-in. The three speech sections still appear.
#[test]
fn f1_7_s3_no_history_drops_the_before_block_but_keeps_all_three_sections() {
    let text = describe::message(
        "",
        "Recording just started.",
        0.0,
        3.0,
        &[],
        &describe::Speech::default(),
        &[describe::Stamp { at: 0.0, new_scene: true }],
    );
    assert!(!text.contains("Just before this:"), "no history, no before block: {text}");
    assert_eq!(
        tool::SPEECH_HEADINGS.iter().filter(|h| text.contains(*h)).count(),
        3,
        "the empty sections still print, each with its own word for empty"
    );
    assert!(text.contains(tool::NO_SPEECH_DURING), "silence over the frames is stated as a fact");
    assert!(!text.starts_with('\n'), "no stray blank where the context was");
}

/// S3: the stamps are NOT evenly spaced -- a batch spanning a scene change carries the change's own
/// first frame between two Freq frames, and each label shows that frame's own second.
#[test]
fn f1_7_s3_stamps_follow_the_real_uneven_spacing() {
    // Frames that went: 0.0, 1.0, then the change at 1.35, then 2.35.
    let picked = describe::pick_frames(&[1.35], 4.0, 0.25, 1.0);
    let stamps: Vec<describe::Stamp> = picked
        .iter()
        .enumerate()
        .map(|(i, at)| describe::Stamp {
            at: *at,
            new_scene: describe::is_new_scene(*at, if i == 0 { None } else { Some(picked[i - 1]) }, &[1.35]),
        })
        .collect();
    let gaps: Vec<f64> = picked.windows(2).map(|w| w[1] - w[0]).collect();
    assert!(
        gaps.iter().any(|g| (*g - 1.0).abs() > 1e-6),
        "the chosen frames are unevenly spaced, otherwise this proves nothing: {picked:?}"
    );
    let text = describe::message("", "s", picked[0], *picked.last().unwrap(), &[], &describe::Speech::default(), &stamps);
    for (i, at) in picked.iter().enumerate() {
        assert!(
            text.contains(&format!("[+{:.1}s] FRAME {} of {}", at, i + 1, picked.len())),
            "frame {} stamped with its own second {at}: {text}",
            i + 1
        );
    }
    let marked: Vec<usize> = stamps.iter().enumerate().filter(|(_, s)| s.new_scene).map(|(i, _)| i + 1).collect();
    assert_eq!(marked, vec![1, 3], "the take's first frame and the change's first frame are marked: {stamps:?}");
}

/// S3: which frame opens a scene is measured against the changes, not against equal spacing.
#[test]
fn f1_7_s3_new_scene_is_measured_against_the_changes() {
    assert!(describe::is_new_scene(0.0, None, &[]), "the take's first frame opens the first scene");
    assert!(describe::is_new_scene(1.0, Some(0.0), &[0.5]), "a change between two frames means a new scene");
    assert!(!describe::is_new_scene(1.0, Some(0.0), &[2.5]), "a change ahead of both means the same scene");
}

// ---- S4 ---------------------------------------------------------------------

/// S4: the key covers the whole content WITH the images inline, so a changed picture is a different
/// question and an unchanged one costs nothing.
#[test]
fn f1_7_s4_the_key_covers_the_content_and_the_images() {
    let images = vec![b"frame-a".to_vec(), b"frame-b".to_vec()];
    let key = describe::cache_key("ask about four frames", &images);
    assert_eq!(key, describe::cache_key("ask about four frames", &images), "same request, same key");
    assert_ne!(
        key,
        describe::cache_key("ask about four frames?", &images),
        "changed wording is a different question"
    );
    assert_ne!(
        key,
        describe::cache_key("ask about four frames", &[b"frame-A".to_vec(), b"frame-b".to_vec()]),
        "a re-encoded frame misses the cache -- hashing names only would make this a silent hit"
    );
    assert_ne!(
        describe::cache_key("x", &[b"ab".to_vec()]),
        describe::cache_key("x", &[b"a".to_vec(), b"b".to_vec()]),
        "image boundaries are part of the key, so images cannot be merged into one blob"
    );
    assert_eq!(key.len(), 64, "a sha256 hex digest names the file");
}

/// S4: the reply is read from and written to `cache/llm/describe/<key>` through the Tree path.
#[test]
fn f1_7_s4_cache_round_trip_under_the_step_folder() {
    let (_keep, tree) = scratch("cache");
    let key = describe::cache_key("ask", &[b"img".to_vec()]);
    assert!(describe::cached_reply(&tree, &key).is_none(), "cold cache is a miss");
    describe::store_reply(&tree, &key, "EVENT: a slide").expect("stored");
    assert_eq!(describe::cached_reply(&tree, &key).as_deref(), Some("EVENT: a slide"));
    assert!(tree.cache_llm(describe::STEP, &key).exists(), "under cache/llm/describe/");
    assert_eq!(describe::STEP, "describe", "the step folder is named for the stage");
    // An unreadable file is a miss, not a failure: the step asks again rather than stopping.
    assert!(describe::cached_reply(&tree, "deadbeef").is_none());
    assert_eq!(describe::cached_note(3), ", 3 answered from the cache");
}

// ---- S5 ---------------------------------------------------------------------

/// S5: `record_event` through the one door -- ok naming the frame and its second, twice replaces,
/// out-of-range refused.
#[test]
fn f1_7_s5_record_event_through_apply_tool() {
    let mut batch = Batch::new(10.0, 1.0, 4);
    let answer = describe::apply_tool(
        &mut batch,
        "record_event",
        &serde_json::json!({"frame": 2, "text": "the graph appears", "calm": true}),
    );
    let value: serde_json::Value = serde_json::from_str(&answer).expect("ok shape");
    assert_eq!(value["frame"], 2, "it names the frame the event landed on");
    assert_eq!(value["at"], 11.0, "and that frame's session second");
    assert_eq!(value["replaced"], false);
    let again = describe::apply_tool(
        &mut batch,
        "record_event",
        &serde_json::json!({"frame": 2, "text": "a bar chart appears", "calm": false}),
    );
    let again: serde_json::Value = serde_json::from_str(&again).unwrap();
    assert_eq!(again["replaced"], true, "a frame recorded twice replaces");
    assert_eq!(batch.events().len(), 1, "and leaves one event, not two");
    let bad = describe::apply_tool(
        &mut batch,
        "record_event",
        &serde_json::json!({"frame": 9, "text": "nowhere", "calm": false}),
    );
    assert!(bad.contains("error"), "a frame outside the batch is refused: {bad}");
    assert_eq!(batch.events().len(), 1, "the refusal changed nothing");
}

/// S5: `set_state` through the door -- text sets it, empty clears rather than leaving the old one.
#[test]
fn f1_7_s5_set_state_clears_on_empty_text() {
    let mut batch = Batch::new(0.0, 1.0, 4);
    describe::apply_tool(&mut batch, "set_state", &serde_json::json!({"text": "Mid-slide."}));
    assert_eq!(batch.state(), Some("Mid-slide."));
    describe::apply_tool(&mut batch, "set_state", &serde_json::json!({"text": ""}));
    assert_eq!(batch.state(), None, "empty text CLEARS the state");
    let answer: serde_json::Value =
        serde_json::from_str(&describe::apply_tool(&mut batch, "set_state", &serde_json::json!({"text": " "})))
            .unwrap();
    assert_eq!(answer["cleared"], true, "whitespace alone clears too");
}

/// S5: `finish` reports the frames still without an event, and names the first-frame-with-no-history
/// case rather than silently writing a description for it.
#[test]
fn f1_7_s5_finish_asks_for_what_is_unanswered() {
    let mut batch = Batch::new(0.0, 1.0, 3);
    batch.record_event(2, "something happens", false);
    let answer: serde_json::Value =
        serde_json::from_str(&describe::apply_tool(&mut batch, "finish", &serde_json::json!({})))
            .unwrap();
    assert_eq!(answer["missing"], 2, "frames 1 and 3 are unanswered");
    assert_eq!(answer["first_frame_has_no_history"], true, "frame 1 has nothing behind it");

    // With history, an unanswered first frame is an ordinary gap.
    let mut warm = Batch::new(0.0, 1.0, 2).with_history();
    warm.record_event(2, "still the same slide", false);
    let warm: serde_json::Value =
        serde_json::from_str(&describe::apply_tool(&mut warm, "finish", &serde_json::json!({})))
            .unwrap();
    assert_eq!(warm["missing"], 1);
    assert_eq!(warm["first_frame_has_no_history"], false);

    // Nothing missing when every frame was answered.
    let mut full = Batch::new(0.0, 1.0, 1).with_history();
    full.record_event(1, "a new slide", false);
    let full: serde_json::Value =
        serde_json::from_str(&describe::apply_tool(&mut full, "finish", &serde_json::json!({})))
            .unwrap();
    assert_eq!(full["missing"], 0);
}

/// S5: a tool that is not offered comes back as an error in the channel the model reads, not a panic.
#[test]
fn f1_7_s5_an_unknown_tool_is_refused_not_fatal() {
    let mut batch = Batch::new(0.0, 1.0, 4);
    let answer = describe::apply_tool(&mut batch, "cut_here", &serde_json::json!({}));
    assert!(answer.contains("error"), "{answer}");
    assert!(answer.contains("record_event") && answer.contains("set_state") && answer.contains("finish"),
        "the refusal lists what IS offered: {answer}");
}

// ---- S6 ---------------------------------------------------------------------

/// S6: one row per frame; a frame with no event is `same`.
#[test]
fn f1_7_s6_one_row_per_frame_with_same_as_the_fallback() {
    let frames = vec![0.0, 1.0, 2.0];
    let events = vec![Event { frame: 2, text: "a slide changes".into(), calm: false }];
    let rows = describe::events_rows(&frames, &events, true);
    assert_eq!(
        rows,
        vec!["0\tsame", "1\ta slide changes", "2\tsame"],
        "every frame gets a row, unanswering ones say `same`: {rows:?}"
    );
}

/// S6: a batch's FIRST frame with no history must NOT be written `same` -- there is nothing behind it
/// to be the same as. It is left out, which is what keeps it visible as still unanswered.
#[test]
fn f1_7_s6_the_first_frame_of_a_cold_batch_is_never_written_same() {
    let frames = vec![0.0, 1.0];
    let rows = describe::events_rows(&frames, &[], false);
    assert_eq!(rows, vec!["1\tsame"], "frame 1 is not asserted about: {rows:?}");
    assert!(!rows.iter().any(|r| r.starts_with("0\tsame")), "no cold `same` at t=0");
    // Same frames, but with history: now `same` is a true statement.
    let warm = describe::events_rows(&frames, &[], true);
    assert_eq!(warm, vec!["0\tsame", "1\tsame"]);
    // The condition matches what the tool reports, so the two cannot drift apart.
    let mut batch = Batch::new(0.0, 1.0, 2);
    let missing = batch.finish();
    assert!(missing.iter().any(|m| m.frame == 1 && m.reason == MissingReason::FirstFrameWithNoHistory));
}

/// S6: state.txt is rewritten after EVERY chunk -- including when the state was cleared, so the old
/// state cannot be read forward by the next chunk as though it still applied.
#[test]
fn f1_7_s6_state_txt_is_rewritten_after_every_chunk() {
    let (_keep, tree) = scratch("write");
    describe::write_chunk(&tree, "lecture", &["0.0\ttitle".into()], Some("Title slide."))
        .expect("chunk 1");
    assert_eq!(fs::read_to_string(tree.describe_state("lecture")).unwrap(), "Title slide.");
    assert_eq!(fs::read_to_string(tree.events_tsv("lecture")).unwrap(), "0.0\ttitle\n");

    describe::write_chunk(&tree, "lecture", &["1.0\tsame".into()], Some("Still the title."))
        .expect("chunk 2");
    let tsv = fs::read_to_string(tree.events_tsv("lecture")).unwrap();
    assert_eq!(tsv, "0.0\ttitle\n1.0\tsame\n", "rows APPEND across chunks");
    assert_eq!(fs::read_to_string(tree.describe_state("lecture")).unwrap(), "Still the title.", "state OVERWRITES");

    describe::write_chunk(&tree, "lecture", &[], None).expect("a cleared state still writes");
    assert_eq!(fs::read_to_string(tree.describe_state("lecture")).unwrap(), "", "cleared state reaches the file");
    assert_eq!(fs::read_to_string(tree.events_tsv("lecture")).unwrap(), tsv, "no rows, no change to events.tsv");
    assert_eq!(describe::state_line(None), "");
    assert_eq!(describe::state_line(Some("x")), "x");
}

// ---- S7 ---------------------------------------------------------------------

/// S7: the two closing sentences, exactly as §F1.7 words them.
#[test]
fn f1_7_s7_the_completion_line_names_the_chunks_and_any_cache_hits() {
    assert_eq!(
        describe::complete_log("lecture", 12),
        ">>> [lecture] event log complete (12 chunks)"
    );
    assert_eq!(
        describe::complete_log_cached("lecture", 12, 9),
        ">>> [lecture] event log complete (12 chunks, 9 answered from the cache)"
    );
    // The cached form is built from the note S4 supplies, so the two spellings cannot diverge.
    assert_eq!(
        describe::complete_log("lecture", 12)
            .replace("(12 chunks)", &format!("(12 chunks{})", describe::cached_note(9))),
        describe::complete_log_cached("lecture", 12, 9)
    );
}

// ---- the chunk plan --------------------------------------------------------

/// The chunk boundaries: P.machine.describeFramesPerReq frames per request, the last allowed short,
/// and the count agrees with the readout F1.1 logs.
#[test]
fn f1_7_chunk_plan_uses_frames_per_request() {
    let frames: Vec<f64> = (0..10).map(|i| i as f64).collect();
    assert_eq!(describe::FRAMES_PER_REQ, 4, "P.machine.describeFramesPerReq");
    assert_eq!(describe::chunk_plan(&frames, 4), vec![0, 4, 8], "three chunks, the last two frames long");
    assert_eq!(describe::requests_for(frames.len()), 3, "and the count matches prepare's own arithmetic");
    assert_eq!(describe::chunk_plan(&frames[..4], 4), vec![0], "an exact multiple is not padded");
    assert_eq!(describe::chunk_plan(&[], 4), Vec::<usize>::new(), "no frames, no requests");
    assert_eq!(describe::chunk_plan(&frames, 0), Vec::<usize>::new(), "a zero-width request asks for nothing");
}
