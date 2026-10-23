//! `P.project.frameInterval` — the frame grid a model is shown and the frame a pick resolves to
//! come from the PROJECT's interval, not from a fixed 1 s spread.
//!
//! §02 §3.9 states the rule as a fact about the session ("frames exist only every
//! `P.project.frameInterval` seconds"), so what these tests pin is that the number on the wire is
//! the one in `naivepost.json`: a session at Freq 4 s is offered frames 4 s apart and a 1 s ask
//! finds nothing, while a session with no opinion keeps the 1 s spacing it always had.
//! Pure logic — no display, no socket, each test its own temp folder.

use std::path::{Path, PathBuf};

use naivepost::clip_rules_tools_leg;
use naivepost::exchanges::ToolCall;
use naivepost::prepare::{self, FREQ_STOPS};
use naivepost::produce_upload as up;
use naivepost::produce_upload_frames as frames;
use naivepost::project::{self, Project};

/// A throwaway session folder for one case: `<tmp>/np-freq-<pid>-<case>`, wiped first so an earlier
/// run's `naivepost.json` cannot answer for this one.
fn session(case: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("np-freq-{}-{case}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("session folder created");
    dir
}

/// Frames laid out on the 4 s grid Freq offers, over a 20 s clip.
const GRID_4S: [f64; 5] = [0.0, 4.0, 8.0, 12.0, 16.0];

/// S1: the interval read is the project's own, written through the same door the Prepare page uses.
#[test]
fn p_project_frameinterval_s1_the_grid_read_is_the_project_written_value() {
    let dir = session("read");
    let mut held = Project::default();
    assert!(
        prepare::set_freq(&mut held, FREQ_STOPS[5]),
        "FREQ_STOPS[5] is a listed stop, so set_freq must accept it"
    );
    assert_eq!(
        held.interval, 4.0,
        "Freq 4 s is what §10 §3 lists as stop 6"
    );
    project::save(&held, &dir).expect("naivepost.json written");

    assert_eq!(
        project::frame_interval(&dir),
        4.0,
        "the live doors read the interval this session was told to use"
    );

    // A folder with no project answers the default, named through the type rather than retyped.
    let empty = session("unread");
    assert_eq!(
        project::frame_interval(&empty),
        Project::default().interval,
        "no project is no opinion, which is `Project::default`'s own number"
    );
    assert_eq!(Project::default().interval, prepare::FREQ_DEFAULT);
}

/// S1: the window `get_frames` narrows to is the project's interval either side of the moment asked
/// for. The contrast assertion is the point: the SAME moments and the SAME ask answered on the old
/// fixed 1 s spread name nothing at all.
#[test]
fn p_project_frameinterval_s1_the_window_shown_follows_the_projects_interval() {
    let near = frames::get_frames(&GRID_4S, 0.0, 20.0, Some(6.0), 4.0);
    assert!(
        near.contains(&4.0) && near.contains(&8.0),
        "both neighbours are 2 s off the ask: {near:?}"
    );
    assert!(
        !near.contains(&0.0) && !near.contains(&12.0),
        "a frame further than one interval away is outside the window: {near:?}"
    );

    // The pre-change behaviour, spelled out: a 1 s window around 6 s holds no 4 s-grid frame.
    let tight = frames::get_frames(&GRID_4S, 0.0, 20.0, Some(6.0), Project::default().interval);
    assert!(
        tight.is_empty(),
        "a 1 s interval on 4 s-spaced frames answers NOTHING — which is exactly why the \
         interval has to come from the project: {tight:?}"
    );

    // No `at` still answers every frame inside the span, whatever the interval.
    let all = frames::get_frames(&GRID_4S, 0.0, 20.0, None, 4.0);
    assert_eq!(
        all,
        GRID_4S.to_vec(),
        "an unaimed look shows the whole clip"
    );
    let clipped = frames::get_frames(&GRID_4S, 8.0, 20.0, None, 4.0);
    assert_eq!(clipped, vec![8.0, 12.0, 16.0], "and the span bounds it");
}

/// One `get_frames` call as the model sends it.
fn look(args: &str) -> ToolCall {
    ToolCall {
        name: "get_frames".to_string(),
        args: args.to_string(),
        result: String::new(),
    }
}

/// The `at`s out of a `{"frames":[{"clip":..,"at":..}]}` answer.
fn shown(answer: &str) -> Vec<f64> {
    let parsed: serde_json::Value = serde_json::from_str(answer).expect("the answer is JSON");
    parsed["frames"]
        .as_array()
        .expect("the answer names frames")
        .iter()
        .map(|frame| {
            frame["at"]
                .as_f64()
                .expect("every frame carries its offset")
        })
        .collect()
}

/// S2: the §3.7 batch answers `get_frames` on the interval its caller set, because that is the grid
/// this session extracted on. A batch left at `Clips::new`'s default answers 1 s spacing, so the
/// difference below is `set_frame_interval` and nothing else.
#[test]
fn p_project_frameinterval_s2_the_clip_batch_answers_on_the_projects_grid() {
    let mut aimed = clip_rules_tools_leg::batch_of(&[(1, 10.0)]);
    aimed.set_frame_interval(FREQ_STOPS[5]);
    let answer = clip_rules_tools_leg::answer(&look(r#"{"clip":1}"#), &mut aimed);
    let offsets = shown(&answer);
    assert_eq!(
        offsets,
        vec![0.0, 4.0, 8.0],
        "a 10 s clip on the 4 s grid: {answer}"
    );
    for at in &offsets {
        assert_eq!(at % 4.0, 0.0, "{at} is not on the 4 s grid: {offsets:?}");
    }

    let mut plain = clip_rules_tools_leg::batch_of(&[(1, 10.0)]);
    let untouched = shown(&clip_rules_tools_leg::answer(
        &look(r#"{"clip":1}"#),
        &mut plain,
    ));
    assert_eq!(
        untouched.len(),
        11,
        "left alone the batch walks 1 s apart: {untouched:?}"
    );
    assert_ne!(
        untouched, offsets,
        "the grid is what moved the answer, not the clip"
    );
}

/// S3: refusals keep their own sentences — an empty session, a clip outside the batch, and a pick
/// with nothing to resolve against. None of them got softer when the grid became the project's.
#[test]
fn p_project_frameinterval_s3_an_empty_or_unknown_answer_still_refuses_in_words() {
    assert!(
        frames::get_frames(&[], 0.0, 20.0, None, 4.0).is_empty(),
        "no frames extracted, none shown"
    );
    assert!(
        frames::get_frames(&GRID_4S, 40.0, 60.0, None, 4.0).is_empty(),
        "a span past the clip holds nothing"
    );

    let mut batch = clip_rules_tools_leg::batch_of(&[(1, 10.0)]);
    batch.set_frame_interval(FREQ_STOPS[5]);
    let unknown = clip_rules_tools_leg::answer(&look(r#"{"clip":9}"#), &mut batch);
    assert!(
        unknown.contains("clip 9 is not in this batch"),
        "the §3.1 error names the clip: {unknown}"
    );
    assert!(
        unknown.starts_with("{\"error\":"),
        "still tools::error's shape: {unknown}"
    );

    let refused = up::pick_frame(1, 1.0, &[0.0], &[]).expect_err("nothing to take");
    assert!(
        refused.contains("no frames have been extracted"),
        "a pick with no frames says so: {refused}"
    );
}

/// The helper is asked for the session folder the tree names, so prove the read survives a rewrite:
/// changing Freq mid-session moves the grid the next request is built on.
#[test]
fn p_project_frameinterval_s1_a_rewritten_freq_moves_the_grid() {
    let dir: &Path = &session("rewrite");
    project::save(&Project::default(), dir).expect("default project written");
    assert_eq!(project::frame_interval(dir), Project::default().interval);

    let mut wider = Project::default();
    assert!(prepare::set_freq(&mut wider, FREQ_STOPS[1]));
    project::save(&wider, dir).expect("the new Freq saved");
    assert_eq!(
        project::frame_interval(dir),
        0.5,
        "the next read sees the change, so no leg can be stuck on the old grid"
    );
}
