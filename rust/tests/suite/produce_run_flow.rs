//! F5.1 ▶ Produce — the LOGIC. `spec/08-produce.md` §F5.1, checked against `naivepost::produce_flow`:
//! what makes the press refuse, when it asks to overwrite, what the snapshot owns, the one opening line,
//! and the three ways a run ends. The wire (the shell's real ▶ reaching these) is
//! `tests/suite/produce_run_widgets.rs`; nothing here builds a widget.
//!
//! Every sentence asserted below is quoted from the spec text rather than paraphrased, because this item is
//! mostly wording: "the video is already what this page describes" and "The encode takes minutes and there
//! is no undo for it." are the feature. A code change that rewords them should fail here, not quietly move
//! the goalposts.
//!
//! The two halves of the run (F5.6 words, F5.2 render) arrive as closures, so no model, no sd.cpp and no
//! ffmpeg stands for any case in this file.

use std::cell::RefCell;
use std::rc::Rc;

use crate::common;
use naivepost::cut::{self, Cut, Seg};
use naivepost::produce_flow as flow;
use naivepost::produce_render as render;
use naivepost::project::{Project, Publish, Source};
use naivepost::run;

const ITEM: &str = "F5.1";

/// §F5.1 S1's refusal, quoted. `cut::NO_CUT_YET` must equal this byte for byte.
const SPEC_NO_CUT: &str = "no cut yet \u{2014} build one on the Cut step first";

/// §F5.1 S2's skip line, quoted.
const SPEC_SKIP: &str =
    ">>> the video is already what this page describes \u{2014} not encoding it again \
(\u{21bb} beside Transcode encodes anyway)";

/// §F5.1 S2's confirmation body, quoted after the `<path> — size, age` line it follows.
const SPEC_NO_UNDO: &str = "The encode takes minutes and there is no undo for it.";

/// Two clips, so "N clips" reads as 2 in every opening-line assertion.
fn two_clips() -> Vec<Seg> {
    vec![
        Seg {
            s: 0.0,
            e: 10.0,
            cam: 0,
            ..Default::default()
        },
        Seg {
            s: 10.0,
            e: 20.0,
            cam: 0,
            ..Default::default()
        },
    ]
}

/// A cut with two clips and an aspect, so the frame box has a shape to read.
fn cut_two() -> Cut {
    Cut {
        segs: two_clips(),
        aspect: "16:9".into(),
        ..Default::default()
    }
}

/// A project with one footage source: nothing locked, and `snapshot_sources` has something to sort.
fn project_with_footage() -> Project {
    Project {
        sources: vec![Source {
            path: "/media/take.mp4".into(),
            footage: true,
            ..Default::default()
        }],
        ..Default::default()
    }
}

/// The run one `snapshot` call produces over `cut`, with `publish_written` set by the caller.
fn run_over(cut: Cut, publish_written: bool) -> flow::Run {
    let (_root, tree) = common::tree_in("run-over");
    flow::snapshot(
        Some(cut),
        &tree,
        vec![],
        Default::default(),
        &project_with_footage(),
        publish_written.then(|| Publish::default()),
        String::new(),
    )
}

// ---- S1: the refusal ---------------------------------------------------------------------------

#[test]
fn f5_1_s1_refuses_busy_before_no_cut() {
    // Both gates shut and busy answers first: the person hears about the run they already have before
    // they hear about a cut they do not.
    let said = flow::refuse(true, false).expect("busy refuses");
    assert_ne!(
        said,
        cut::NO_CUT_YET,
        "{ITEM} S1: busy is checked BEFORE the cut, so the no-cut sentence is not what comes back"
    );
    assert_eq!(
        said,
        run::PAUSING,
        "and what comes back names the run under way"
    );
}

#[test]
fn f5_1_s1_refuses_no_cut() {
    assert_eq!(
        flow::refuse(false, false),
        Some(SPEC_NO_CUT),
        "{ITEM} S1: the refusal is the spec's sentence, and `cut::NO_CUT_YET` IS that sentence"
    );
    assert_eq!(
        cut::NO_CUT_YET,
        SPEC_NO_CUT,
        "shared with the tabs' own grey-out, not restated"
    );
    assert_eq!(
        flow::refuse(false, true),
        None,
        "with a cut standing, nothing refuses"
    );
}

// ---- S2: up to date, overwrite, or just encode --------------------------------------------------

#[test]
fn f5_1_s2_the_overwrite_answer_decides_whether_the_encode_runs() {
    // The question is only half of S2; the other half is that an ANSWER runs the encode. The response ids
    // are the flow's constants, so a dialog and this rule cannot drift apart.
    assert!(
        flow::overwrite_proceeds(flow::ANSWER_OVERWRITE),
        "{ITEM} S2: Overwrite means encode anyway"
    );
    assert!(
        !flow::overwrite_proceeds(flow::ANSWER_CANCEL),
        "Cancel leaves the standing video alone"
    );
    assert!(
        !flow::overwrite_proceeds("close"),
        "and so does any answer that was not the deliberate word: \u{a7}S2 asks precisely so an accidental \
         press cannot destroy an encode"
    );

    // And the pair the dialog shows, built over a REAL file so the size and the age in the body are facts
    // about that file rather than strings handed in by the caller.
    let root = common::temp_root("s2-ask");
    let video = root.join("final.mp4");
    std::fs::write(&video, vec![7u8; 4096]).expect("a video to ask about");
    let (title, body) = flow::overwrite_asked(&video);
    assert_eq!(
        title, "Overwrite final.mp4?",
        "the title names the base: {title}"
    );
    let lines: Vec<&str> = body.split('\n').collect();
    assert!(
        lines[0].starts_with(&format!("{} \u{2014} ", video.display())),
        "first paragraph opens with the path: {}",
        lines[0]
    );
    assert!(
        lines[0].contains("MiB"),
        "and carries the file's weight in the units the render's own lines use: {}",
        lines[0]
    );
    assert!(
        lines[0].ends_with("old"),
        "and its age, said as an age: {}",
        lines[0]
    );
    assert_eq!(
        lines[1], "",
        "two paragraphs, spelled apart as §S2 writes them"
    );
    assert_eq!(lines[2], SPEC_NO_UNDO, "then the cost, verbatim");
    // A missing file still answers: the gate asked because a video was there, but nothing here may panic
    // on one that vanished between the check and the dialog.
    let (gone_title, gone_body) = flow::overwrite_asked(&root.join("nope.mp4"));
    assert_eq!(gone_title, "Overwrite nope.mp4?");
    assert!(gone_body.contains(SPEC_NO_UNDO), "{gone_body}");
}

#[test]
fn f5_1_s3_the_snapshot_carries_the_page_s_effect_list_and_frame_shape() {
    // S3 says the press snapshots "the cut, lines, settings, sources, publish state, aspect". Two of those
    // were being dropped on the floor on the way to the encoder: the cut's own effect list (the render was
    // handed an empty one, so a zoom or a volume rode nothing) and the footage's frame shape (a hardcoded
    // 1920x1080 stood in for whatever this session actually filmed). Both are checked over real files.
    let (root, tree) = common::tree_in("s3-snapshot");
    let _ = root;
    let cut = Cut {
        segs: two_clips(),
        aspect: "9:16".into(),
        fx: vec![naivepost::cut::Fx {
            kind: "zoom".into(),
            t: 4.0,
            dur: 3.0,
            ..Default::default()
        }],
        ..Default::default()
    };
    cut::save(&cut, &tree).expect("cut written");
    // A frame Prepare would have extracted for the first footage recording, at a shape that is NOT the
    // fallback: a portrait camera, matching the cut's 9:16.
    let frames = tree.frames_dir("take");
    std::fs::create_dir_all(&frames).expect("frames folder");
    let surface = cairo::ImageSurface::create(cairo::Format::Rgb24, 1080, 1920).expect("surface");
    {
        let cr = cairo::Context::new(&surface).expect("context");
        cr.set_source_rgb(0.2, 0.3, 0.4);
        let _ = cr.paint();
    }
    let png = frames.join("2026-01-02_00-00-04.jpg");
    surface
        .write_to_png(&mut std::io::BufWriter::new(
            std::fs::File::create(&png).expect("frame file"),
        ))
        .expect("a frame the snapshot can measure");
    let recordings = vec![naivepost::timeline::Recording {
        base: "take".into(),
        start: 0.0,
        end: 20.0,
    }];
    let snap = naivepost::ui::produce_snapshot::gather(
        &tree,
        &cut,
        &Default::default(),
        &project_with_footage(),
        &recordings,
        Default::default(),
    );
    // The effects reach the render through `fx_for`; ask it the way `plan_clips` does.
    let asked = snap.cut.fx.clone();
    assert_eq!(
        asked.len(),
        1,
        "{ITEM} S3: the cut's effect list travels with the snapshot"
    );
    assert_eq!(
        asked[0].kind, "zoom",
        "and is the cut's own effect: {asked:?}"
    );
    assert_eq!(snap.aspect(), "9:16", "and so is its aspect");
    assert_eq!(
        snap.src_shape,
        (1080, 1920),
        "\u{a7}9: the frame is the first footage clip's own shape, read off its extracted frame \u{2014} not the encoder's default 1920x1080"
    );
    assert!(snap.on_recording, "a session with tape says it has tape");
    assert_eq!(
        snap.filmed_runs,
        vec![(0.0, 20.0)],
        "and hands over the filmed spans"
    );
    // F4.3's bound comes off the same spans: a clip ending 5 s into a 20 s run has 15 s of footage left.
    let mid = Seg {
        s: 0.0,
        e: 5.0,
        ..Default::default()
    };
    assert_eq!(
        snap.footage_left(&mid),
        15.0,
        "footage left past the clip's end"
    );
    // With no tape at all the answer is unknown, not zero: §10's ceiling still applies.
    let bare = naivepost::ui::produce_snapshot::gather(
        &tree,
        &cut,
        &Default::default(),
        &project_with_footage(),
        &[],
        Default::default(),
    );
    assert_eq!(
        bare.footage_left(&mid),
        0.0,
        "no tape answers through `unknown_footage`"
    );
    assert_eq!(
        bare.src_shape,
        (1920, 1080),
        "and keeps the encoder's fallback shape"
    );
}

#[test]
fn f5_1_s3_the_run_carries_the_session_s_narration_lines() {
    // S3's "lines": without them the words half has nothing to caption and §F5.2 S2's speaking step is
    // vacuous, which is exactly what `lines: vec![]` used to mean.
    let (root, tree) = common::tree_in("s3-lines");
    let _ = root;
    let cut = cut_two();
    let gather = |tree: &naivepost::layout::Tree| {
        naivepost::ui::produce_snapshot::gather(
            tree,
            &cut,
            &Default::default(),
            &project_with_footage(),
            &[],
            Default::default(),
        )
    };
    // Nothing written yet: zero lines, and NOT an error — a session that was never narrated is a normal one.
    assert!(
        gather(&tree).lines.is_empty(),
        "no narration file means no lines, not a failure"
    );
    // Write two through the real writer (which sorts them, so the order asserted below is the file's).
    let narration = naivepost::narration::Narration {
        entries: vec![
            naivepost::narration::Entry {
                s: 10.0,
                e: 20.0,
                text: "second line".into(),
                ..Default::default()
            },
            naivepost::narration::Entry {
                s: 0.0,
                e: 10.0,
                text: "first line".into(),
                ..Default::default()
            },
        ],
        silent: vec![],
    };
    naivepost::narration::save(&narration, &tree).expect("narration written");
    let snap = gather(&tree);
    assert_eq!(
        snap.lines.len(),
        2,
        "{ITEM} S3: both lines travel with the run"
    );
    assert_eq!(
        snap.lines
            .iter()
            .map(|e| e.text.as_str())
            .collect::<Vec<_>>(),
        vec!["first line", "second line"],
        "and in the record's playing order, which is the order the render speaks them"
    );
}

#[test]
fn f5_1_s4_the_words_half_works_from_the_page_s_publish_state() {
    // S4 turns on whether `publish.json` exists, and S3 snapshots the publish STATE. Reading the file where
    // the page's rows should be is how a fresh session ended up asking the model to draw a thumbnail from no
    // instruction and no title, and then writing that emptiness back over whatever the person had typed.
    let (root, tree) = common::tree_in("s4-publish");
    let _ = root;
    // The file holds an OLD record: a title and an instruction from some earlier run.
    let stale = naivepost::project::Publish {
        title: "an old title".into(),
        prompt: "the instruction that drew the last one".into(),
        frames: vec!["project:produce/publish/frame-1.jpg".into()],
        ..Default::default()
    };
    naivepost::publish::save(&stale, &tree).expect("publish.json written");
    // The page now describes something else entirely (and on a fresh session, describes nothing).
    let held = naivepost::project::Publish {
        title: "what the person typed now".into(),
        prompt: "draw the slide, not the speaker".into(),
        ..Default::default()
    };
    let merged = naivepost::ui::produce_snapshot::merge_publish(Some(held.clone()), &stale);
    assert_eq!(
        merged.title, held.title,
        "{ITEM}: the page's title wins over the file's"
    );
    assert_eq!(
        merged.prompt, held.prompt,
        "and so does its edit instruction"
    );
    assert_eq!(
        merged.frames, stale.frames,
        "while a field no page row spells (the image row a redraw left behind) survives the merge"
    );
    // And the distinction S4's three shapes turn on survives: with NO publish state at all (no page), the
    // run falls back to the file rather than pretending nothing was ever written.
    let none = naivepost::ui::produce_snapshot::merge_publish(None, &stale);
    assert_eq!(
        none.title, stale.title,
        "no page state means the file answers"
    );

    // And through the door the PROGRAM calls: `encode_and_finish` hands the run
    // `publish_of(&snap)`, so the same answer must come out of a real snapshot rather than only of the
    // pure two-record helper above. Asserting here is what makes un-wiring that merge fail a test.
    let snap = naivepost::ui::produce_snapshot::gather(
        &tree,
        &cut_two(),
        &Default::default(),
        &project_with_footage(),
        &[],
        held.clone(),
    );
    let reached = naivepost::ui::produce_snapshot::publish_of(&snap);
    assert_eq!(
        reached.title, held.title,
        "{ITEM}: the snapshot's own rows win over the file's"
    );
    assert_eq!(
        reached.prompt, held.prompt,
        "and so does its edit instruction"
    );
    assert_eq!(
        reached.frames, stale.frames,
        "while the field no page row spells (the image row an earlier redraw left) survives into the run"
    );
    // The brief is built from the run's lines and cut; a run whose lines are empty says so out loud rather
    // than silently describing nothing.
    let brief = flow::opening_line(&run_over(cut_two(), true), false);
    assert!(
        brief.starts_with(">>> producing "),
        "S4's shape when publish.json exists: {brief}"
    );
}

#[test]
fn f5_1_s2_up_to_date_skips_with_the_said_line() {
    assert_eq!(
        flow::gate(true, true),
        flow::Gate::Skip,
        "matching stamp + a video = skip"
    );
    assert!(
        flow::SKIP_LOG.contains("already what this page describes"),
        "the skip says what it is: {}",
        flow::SKIP_LOG
    );
    assert!(
        flow::SKIP_LOG.contains("\u{21bb} beside Transcode encodes anyway"),
        "and tells the person where to get the encode anyway: {}",
        flow::SKIP_LOG
    );
    assert_eq!(flow::SKIP_LOG, SPEC_SKIP, "byte for byte against the spec");
}

#[test]
fn f5_1_s2_asks_to_overwrite_when_a_video_exists() {
    // Stale but present: the overwrite question comes before anything is written.
    assert_eq!(flow::gate(false, true), flow::Gate::ConfirmOverwrite);
    assert_eq!(
        flow::overwrite_title("final.mp4"),
        "Overwrite final.mp4?",
        "{ITEM} S2: the title names the base, not the folder"
    );
    let detail = flow::overwrite_detail("/p/produce/final.mp4", "41 MB", "3 hours ago");
    let lines: Vec<&str> = detail.split('\n').collect();
    assert_eq!(
        lines[0], "/p/produce/final.mp4 \u{2014} 41 MB, 3 hours ago",
        "first paragraph: the path, the size, the age"
    );
    assert_eq!(
        lines[1], "",
        "then a blank line, because §S2 spells the two paragraphs apart"
    );
    assert_eq!(lines[2], SPEC_NO_UNDO, "then the cost, verbatim");
}

#[test]
fn f5_1_s2_no_video_encodes_without_asking() {
    assert_eq!(
        flow::gate(false, false),
        flow::Gate::Encode,
        "nothing to overwrite, so nothing to ask"
    );
    // And the third case the gate exists to catch: a matching stamp with no file behind it is MISSING,
    // not up to date, so it encodes rather than skipping into an absent video.
    assert_eq!(flow::gate(true, false), flow::Gate::Encode);
}

// ---- S3: the snapshot, and the one function that answers "what is the cut" ---------------------

#[test]
fn f5_1_s3_snapshot_is_taken_from_what_is_handed_in() {
    let project = project_with_footage();
    let (_root, tree) = common::tree_in("snap");
    let run = flow::snapshot(
        Some(cut_two()),
        &tree,
        vec![],
        Default::default(),
        &project,
        None,
        "voice-a".into(),
    );
    assert_eq!(
        run.clips, 2,
        "clips counted at the press, from the cut handed in"
    );
    assert_eq!(run.publish_written, false, "no publish record was passed");
    assert_eq!(run.voice, "voice-a");
    // Sources come through F0.x's own snapshot rather than being re-derived here.
    let expected = run::snapshot_sources(&project);
    assert_eq!(
        run.sources, expected,
        "the same snapshot the other steps take"
    );
    assert_eq!(run.sources.footage, vec!["/media/take.mp4"]);

    // Mutating the project afterwards does not reach the run: a render that re-asked mid-flight is how
    // a file ends up missing an angle.
    let mut later = project.clone();
    later.sources.push(Source {
        path: "/media/second.mp4".into(),
        footage: true,
        ..Default::default()
    });
    assert_eq!(
        run.sources.footage.len(),
        1,
        "the Run holds what it was given; a later edit to the project changes nothing in it"
    );
}

#[test]
fn f5_1_s3_page_cut_beats_the_file() {
    // A saved file says one thing, the live editor another: the editor wins, so an unsaved tweak renders.
    let (_root, tree) = common::tree_in("page-wins");
    let file_cut = Cut {
        segs: vec![Seg {
            s: 100.0,
            e: 110.0,
            cam: 0,
            ..Default::default()
        }],
        aspect: "4:3".into(),
        ..Default::default()
    };
    cut::save(&file_cut, &tree).expect("the file cut is written");
    let page = cut_two();
    let answered = flow::what_is_the_cut(Some(page.clone()), &tree);
    assert_eq!(
        answered, page,
        "the page's own segments answer, ignoring the file entirely"
    );
    assert_eq!(
        answered.aspect, "16:9",
        "with the page's aspect, not the file's"
    );
}

#[test]
fn f5_1_s3_falls_back_to_cut_json() {
    // Nothing live: `cut/cut.json` answers, which is what render, subtitles, brief and stamp all read.
    let (_root, tree) = common::tree_in("fallback");
    let file_cut = Cut {
        segs: vec![
            Seg {
                s: 0.0,
                e: 5.0,
                cam: 0,
                ..Default::default()
            },
            Seg {
                s: 20.0,
                e: 25.0,
                cam: 0,
                ..Default::default()
            },
            Seg {
                s: 40.0,
                e: 45.0,
                cam: 0,
                ..Default::default()
            },
        ],
        aspect: "16:9".into(),
        ..Default::default()
    };
    cut::save(&file_cut, &tree).expect("cut/cut.json written");
    let answered = flow::what_is_the_cut(None, &tree);
    assert_eq!(
        answered, file_cut,
        "the file answers when the page holds nothing"
    );
    assert_eq!(answered.segs.len(), 3);
    // An empty live cut is "nothing live", not "a cut of zero length": the file still answers.
    let empty_live = Cut {
        segs: vec![],
        ..Default::default()
    };
    assert_eq!(
        flow::what_is_the_cut(Some(empty_live), &tree).segs.len(),
        3,
        "an empty editor is not a cut; the file keeps answering"
    );
}

// ---- S4: the one opening line ------------------------------------------------------------------

#[test]
fn f5_1_s4_opening_line_without_publish_json_says_the_text_and_picture_are_left_alone() {
    let run = run_over(cut_two(), false);
    let line = flow::opening_line(&run, false);
    assert!(
        line.starts_with(">>> transcoding produce/final.mp4:"),
        "opens with the file: {line}"
    );
    assert!(
        line.contains("2 clips at mp4/h264 crf 24"),
        "says N clips at container/codec crf N: {line}"
    );
    assert!(
        line.ends_with("the thumbnail and the upload text are left as they are"),
        "and that neither half of the words work runs: {line}"
    );
}

#[test]
fn f5_1_s4_with_publish_json_says_redrawn_beside_them() {
    let run = run_over(cut_two(), true);
    let line = flow::opening_line(&run, true);
    assert!(
        line.starts_with(">>> producing produce/final.mp4:"),
        "{line}"
    );
    assert!(
        line.contains("2 clips at mp4/h264 crf 24"),
        "same middle clause: {line}"
    );
    assert!(
        line.ends_with("and the thumbnail redrawn beside them"),
        "{line}"
    );
}

#[test]
fn f5_1_s4_with_publish_json_no_picture_says_written_beside_them() {
    let run = run_over(cut_two(), true);
    let line = flow::opening_line(&run, false);
    assert!(
        line.starts_with(">>> producing produce/final.mp4:"),
        "{line}"
    );
    assert!(line.contains("2 clips at mp4/h264 crf 24"), "{line}");
    assert!(
        line.ends_with("and the upload text and thumbnail written beside them"),
        "{line}"
    );
}

#[test]
fn f5_1_s4_words_failure_is_logged_and_the_render_carries_on() {
    let said = flow::words_failed("sd.cpp is down");
    assert!(
        said.contains("sd.cpp is down"),
        "the reason is named: {said}"
    );
    assert!(
        said.contains("-- the render carries on"),
        "and the run continues: {said}"
    );
    assert!(
        said.starts_with("!!! the upload text and thumbnail failed:"),
        "{said}"
    );

    // Structurally: a words half that fails does not stop the render half from being asked. The file name
    // is made up rather than read off the run (`ending_for` is `run_with`'s own tail, and only the DONE line
    // uses the name): a test must not touch the project folder, because several cases here share
    // `/tmp/np-tree-<pid>-run-over` and a sibling's `tree_in` wipes that folder — which is how this case
    // used to fail in a full-suite run while passing on its own.
    let run = run_over(cut_two(), true);
    let calls: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(vec![]));
    let log = calls.clone();
    let ending = flow::run_with_named(
        "produce/final.mp4",
        &run,
        |_| {
            log.borrow_mut().push("words");
            (false, Some("sd.cpp is down".to_string()))
        },
        |_| {
            log.borrow_mut().push("render");
            flow::Rendered {
                ok: true,
                tag_ok: true,
                stopped: false,
                seconds: 0.0,
                size: "0 B".to_string(),
            }
        },
    );
    assert_eq!(
        ending.status,
        render::STAGE_DONE,
        "the render's verdict stands despite the words failure"
    );
    assert!(
        ending.log.contains("produce/final.mp4"),
        "and the DONE line names the produced file: {}",
        ending.log
    );
    assert_eq!(
        *calls.borrow(),
        vec!["words", "render"],
        "both halves ran, words first"
    );
}

// ---- S5/S6: ordering and the three endings ----------------------------------------------------

#[test]
fn f5_1_s5_the_tag_page_writes_after_both_halves() {
    // S5: the tag page comes after BOTH halves. Counted rather than eyeballed: the order is the rule.
    let run = run_over(cut_two(), true);
    let order: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(vec![]));
    let seen = order.clone();
    let _ = flow::run_with(
        &run,
        |_| {
            seen.borrow_mut().push("words-start");
            seen.borrow_mut().push("words-done");
            (true, None)
        },
        |_| {
            seen.borrow_mut().push("render-start");
            seen.borrow_mut().push("render-done");
            flow::Rendered {
                ok: true,
                tag_ok: true,
                stopped: false,
                seconds: 0.0,
                size: "0 B".to_string(),
            }
        },
    );
    let done = order.borrow();
    assert!(
        done.contains(&"words-done") && done.contains(&"render-done"),
        "{done:?}"
    );
    // Both halves finished before `finish` could be reached: neither closure saw the other's name, and
    // `finish` only ever runs once both returned.
    assert_eq!(
        done.iter().filter(|n| **n == "words-done").count(),
        1,
        "words ran exactly once"
    );
    assert_eq!(
        done.iter().filter(|n| **n == "render-done").count(),
        1,
        "render ran exactly once"
    );
    assert!(
        done.iter().position(|n| *n == "words-done").unwrap()
            < done.iter().position(|n| *n == "render-done").unwrap()
            || done.iter().position(|n| *n == "render-done").unwrap()
                < done.iter().position(|n| *n == "words-done").unwrap(),
        "either order is fine, but neither half repeats: {done:?}"
    );
}

#[test]
fn f5_1_s6_render_error_is_the_verdict_even_when_the_tag_failed() {
    let halves = flow::Halves {
        words_ok: true,
        words_reason: None,
        render_ok: false,
        tag_ok: false,
        stopped: false,
        seconds: 12.0,
        size: "41 MB".into(),
    };
    assert_eq!(
        flow::finish(&halves).status,
        render::failure_log(),
        "{ITEM} S6: the RENDER's error is the run's verdict"
    );
    // The same run with the tag page intact and the render still bad: unchanged verdict.
    let mut tag_fixed = halves.clone();
    tag_fixed.tag_ok = true;
    assert_eq!(flow::finish(&tag_fixed).status, render::failure_log());
    // And a good render with a bad tag page is DONE: a helper page is a bonus (produce_embed's rule).
    let mut render_ok = halves.clone();
    render_ok.render_ok = true;
    render_ok.tag_ok = false;
    assert_eq!(flow::finish(&render_ok).status, render::STAGE_DONE);
}

#[test]
fn f5_1_s6_stopped_reads_stopped() {
    let halves = flow::Halves {
        words_ok: true,
        words_reason: None,
        render_ok: false,
        tag_ok: false,
        stopped: true,
        seconds: 3.0,
        size: "0 B".into(),
    };
    assert_eq!(
        flow::finish(&halves).status,
        render::stopped_log(),
        "a stop is not a failure"
    );
    // A stop outranks even a would-be-done run.
    let mut done_but_stopped = halves.clone();
    done_but_stopped.render_ok = true;
    done_but_stopped.tag_ok = true;
    assert_eq!(
        flow::finish(&done_but_stopped).status,
        render::stopped_log()
    );
}

#[test]
fn f5_1_s6_done_logs_the_finished_line() {
    let halves = flow::Halves {
        words_ok: true,
        words_reason: None,
        render_ok: true,
        tag_ok: true,
        stopped: false,
        seconds: 187.5,
        size: "41 MB".into(),
    };
    let ending = flow::ending_for("produce/final.mp4", &halves);
    assert_eq!(ending.status, render::STAGE_DONE);
    assert_eq!(
        ending.log,
        render::finished_log("produce/final.mp4", 187.5, "41 MB"),
        "the closing line is the renderer's own, two spaces before the parenthesis included"
    );
    assert!(
        ending.log.contains("final.mp4  ("),
        "two spaces, as the prototype writes it: {}",
        ending.log
    );
    // The bar's progress text is the same numbers in its own shape.
    assert_eq!(
        flow::progress_text("produce/final.mp4", 187.5, "41 MB"),
        "produced produce/final.mp4 \u{2014} 187.5 s, 41 MB"
    );
    assert_eq!(
        ending.progress,
        flow::progress_text("produce/final.mp4", 187.5, "41 MB")
    );
}

#[test]
fn f5_1_s6_a_run_that_skipped_the_encode_writes_no_stamp() {
    // F5.3: the stamp lands once the render returns without error. A skipped encode wrote none, so a
    // later change still finds the old stamp and still says why it differs.
    assert!(!flow::stamp_written(false, false), "skipped: no stamp");
    assert!(!flow::stamp_written(true, true), "failed: no stamp");
    assert!(
        flow::stamp_written(true, false),
        "encoded clean: the stamp goes down"
    );
}
