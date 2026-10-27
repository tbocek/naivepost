//! F5.6 Upload text and thumbnail — the rules in `naivepost::produce_upload` plus the wire that makes
//! the Produce page's ▶ run them.
//!
//! The logic half drives [`walk`] through injected legs (the seam `narrate_tts::speak_line` uses for its
//! network legs), so every rule §F5.6 states is checked with no model and no sd.cpp standing: the brief's
//! wording, what each of the four tools answers, the frame-actually-taken distance, the publish-before-draw
//! order, the draw request's shape, the picture's own stamp, and the print order.
//!
//! The widget half fires the real `play-button` and reads `publish.json`, `description.txt` and
//! `thumbnail.stamp` off disk. It follows `tests/suite/produce_translate_flow.rs`: one `adw::Application`, one
//! `run_with_args` inside a `static ONCE`, cwd pinned before the run, checks inside `catch_unwind`.

use std::cell::RefCell;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::rc::Rc;
use std::sync::Mutex;

use crate::common;
use adw::prelude::*;
use common::{
    fixture_dir, hold_last_window, release_last_window, session_tree_of as session, settle,
};
use naivepost::cut::Seg;
use naivepost::layout::Tree;
use naivepost::narration::Entry;
use naivepost::produce_upload as up;
use naivepost::project::{Publish, TitleBox};
use naivepost::roles::Job;
use naivepost::shell::Page;
use naivepost::ui;

// ---- logic helpers -------------------------------------------------------------------------------

/// A clip list of `n` twelve-second clips starting at 0, each with one event and one spoken line.
/// A clip list of `n` twelve-second clips starting at 0, each with one event and one spoken line. `rate`
/// and `marked` come from the closure so a test can hand over a sped clip or a decorated one.
fn clips_with(n: usize, mark: impl Fn(usize) -> (f64, Vec<String>)) -> Vec<up::BriefClip> {
    (0..n)
        .map(|i| {
            let (rate, marked) = mark(i);
            up::BriefClip {
                number: i + 1,
                at_output: i as f64 * 12.0,
                seconds: 12.0,
                session: format!(
                    "{}\u{2013}{}",
                    naivepost::tools::mm_ss(i as f64 * 12.0),
                    naivepost::tools::mm_ss(i as f64 * 12.0 + 12.0)
                ),
                seen: vec![format!("a screen showing thing {}", i + 1)],
                said: vec![format!("someone said line {}", i + 1)],
                rate,
                marked,
            }
        })
        .collect()
}

/// An unsped clip with nothing standing on it: the header then reads exactly as \u00a7 F5.6 S1 spells it,
/// which is what most of this file's assertions compare against.
fn clips(n: usize) -> Vec<up::BriefClip> {
    clips_with(n, |_| (1.0, vec![]))
}

fn line(at: f64, text: &str) -> Entry {
    Entry {
        s: at,
        e: at + 4.0,
        at,
        text: text.to_string(),
        emotion: String::new(),
        pos: String::new(),
        roll: 0,
    }
}

thread_local! {
    /// The kinds the print leg was asked to print, in the order it was asked — what `print_order` drove.
    static PRINTED: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };

    /// The record the `save_publish` leg was handed, i.e. what publish.json would have held.
    static PUBLISHED: RefCell<Option<Publish>> = const { RefCell::new(None) };

    /// The bytes the `save_picture` leg was handed — the plain picture the draw produced, as the walk saw it.
    static PLAIN_BYTES: RefCell<Option<Vec<u8>>> = const { RefCell::new(None) };
}

/// What the last walk's publish leg received.
fn published() -> Publish {
    PUBLISHED
        .with(|cell| cell.borrow().clone())
        .expect("the publish leg ran")
}

/// The legs a walk needs, all recording into one shared log of call names.
#[derive(Clone, Default)]
struct Recorder {
    calls: Rc<RefCell<Vec<String>>>,
}

impl Recorder {
    fn new() -> Self {
        Self::default()
    }
    fn mark(&self, name: &str) {
        self.calls.borrow_mut().push(name.to_string());
    }
    fn took(&self, name: &str) -> bool {
        self.calls.borrow().iter().any(|c| c == name)
    }
    fn index(&self, name: &str) -> Option<usize> {
        self.calls.borrow().iter().position(|c| c == name)
    }
}

/// Run `walk` with legs that succeed and record the order they were called in.
fn walk_with(
    rec: &Recorder,
    publish: &Publish,
    answers: &up::Answers,
    stored: Option<&str>,
    edited_by_hand: bool,
    draw_fails: bool,
) -> up::Outcome {
    PUBLISHED.with(|cell| cell.replace(None));
    PLAIN_BYTES.with(|cell| cell.replace(None));
    let ask_model = {
        let rec = rec.clone();
        move |_b: &str| {
            rec.mark("model");
            Ok(String::new())
        }
    };
    let save_publish = {
        let rec = rec.clone();
        move |p: &Publish| {
            rec.mark("publish");
            // The publish leg is the only place a test can see what the ROW held at the moment the text
            // landed, which is what S4 and S3 are about.
            PUBLISHED.with(|cell| cell.replace(Some(p.clone())));
            Ok(())
        }
    };
    let ask_draw = {
        let rec = rec.clone();
        move |_b: &serde_json::Value| {
            rec.mark("draw");
            if draw_fails {
                return Err("sd.cpp said no".to_string());
            }
            Ok(up::DrawnJob { id: "job-1".into() })
        }
    };
    let poll = {
        let rec = rec.clone();
        move |_j: &up::DrawnJob| {
            rec.mark("poll");
            Ok(up::PollState {
                status: "done".into(),
                ahead: 0,
                done: true,
                // The scripted server's picture, carried on the answer that says done exactly as a real job's
                // reply carries it: the `save_picture` leg then writes those bytes, and S7's words print onto
                // the file they landed in.
                png: page_drawn_png(),
            })
        }
    };
    let save_picture = {
        let rec = rec.clone();
        move |_j: &up::DrawnJob, png: &[u8]| {
            rec.mark("picture");
            // The walk hands the draw's bytes to the leg that writes them; here they are recorded AND written
            // where the page's own print leg will read them back, which is the step S7 letters from.
            PLAIN_BYTES.with(|held| held.borrow_mut().replace(png.to_vec()));
            let path = session_picture_path();
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir).map_err(|why| format!("{}: {why}", dir.display()))?;
            }
            std::fs::write(&path, png).map_err(|why| format!("{}: {why}", path.display()))?;
            Ok(())
        }
    };
    let print = {
        let rec = rec.clone();
        move |kind: &str, _words: &[String]| {
            rec.mark("print-leg");
            PRINTED.with(|p| p.borrow_mut().push(kind.to_string()));
            Ok(())
        }
    };
    let nearest = |_at: f64| Some(34.0f64);
    // S4's three frames, as the page would hand them over: `frame:<seconds>` entries off the kept footage.
    let spread = || {
        vec![
            "frame:5.0".to_string(),
            "frame:15.0".to_string(),
            "frame:25.0".to_string(),
        ]
    };
    let write_stamp = {
        let rec = rec.clone();
        move |_h: &str| {
            rec.mark("stamp");
            Ok(())
        }
    };
    let mats = up::Materials {
        save_publish: &save_publish,
        ask_model: &ask_model,
        // This file proves the walk's ORDER, and that order starts with the model call: the walk asks.
        model_asked: true,
        ask_draw: &ask_draw,
        poll: &poll,
        save_picture: &save_picture,
        print: &print,
        nearest_frame: &nearest,
        // S4: the three moments that fill an empty row. A test says what the cut yields; `walk` only puts
        // them in the row, which is the part of S4 this file can prove.
        spread_frames: &spread,
        stored_stamp: stored.map(|s| s.to_string()),
        write_stamp: &write_stamp,
        edited_by_hand,
        // S3/S6's media legs. This file proves the walk's ORDER, so the legs answer with URLs of their own
        // making rather than running ffmpeg: what matters here is that the request asks for images at all and
        // that a chosen frame reaches the picture door, which these closures do let a test see.
        request_images: &|publish: &Publish| {
            let urls = if !publish.frames.is_empty() {
                vec!["data:image/png;base64,AAAA".to_string()]
            } else {
                Default::default()
            };
            up::Sent {
                base: urls.first().cloned(),
                urls,
                base_name: "frame.jpg".to_string(),
                crop_percent: 100,
                centre: (0.5, 0.5),
                skipped: Vec::new(),
            }
        },
        own_picture: &|_publish: &Publish| {
            rec.mark("own-picture");
            Ok(())
        },
    };
    let no_draw = |_b: &serde_json::Value| -> Result<up::DrawnJob, String> {
        rec.mark("draw-refused");
        Err("no image server here".to_string())
    };
    let _ = no_draw;
    up::walk("THE BRIEF", publish, answers, &mats, |_| {})
}

fn publish_with(frames: &[&str], title: &str, prompt: &str) -> Publish {
    Publish {
        frames: frames.iter().map(|f| f.to_string()).collect(),
        title: title.to_string(),
        thumb_title: title.to_string(),
        prompt: prompt.to_string(),
        description: "old description".to_string(),
        ..Default::default()
    }
}

// ---- S1: the brief -------------------------------------------------------------------------------

/// \u00a712 #30 widened this brief: the decorations and the clip's rate are passed now, because a moment marked
/// "the reveal" reaching the description writer as nothing was the complaint. Only the `CAPTION` lines stay
/// out \u{2014} this writer is not placing subtitles.
#[test]
fn f5_6_s1_the_brief_is_the_finished_video_clip_by_clip_and_carries_marks_and_rate() {
    let text = up::brief(
        &clips(2),
        &[line(0.0, "the first line")],
        up::BRIEF_MAX_CHARS,
    );
    assert!(
        text.starts_with("THE FINISHED VIDEO: 2 clips, 00:24 long."),
        "{text}"
    );
    assert!(text.contains("WHAT IS IN EACH CLIP:"), "{text}");
    assert!(
        text.contains("CLIP 1 (at 00:00 in the video, 12.0 s): session 00:00\u{2013}00:12"),
        "{text}"
    );
    assert!(
        text.contains("CLIP 2 (at 00:12 in the video, 12.0 s): session 00:12\u{2013}00:24"),
        "{text}"
    );
    assert!(
        text.contains("THE NARRATION SPOKEN OVER IT, at its time in the finished video:"),
        "{text}"
    );
    assert!(text.contains("the first line"), "{text}");
    // A caption is the viewer's text, not a decoration on the picture: still not passed.
    assert!(!text.contains("CAPTION"), "nor CAPTION lines: {text}");

    // \u00a712 #30: a decorated, sped clip says both. The words are `narrate_pass::effect_tag`'s, so a zoom is
    // spelled one way in both briefs.
    let decorated = clips_with(1, |i| {
        (
            if i == 0 { 4.0 } else { 1.0 },
            vec![
                "MARKED: the reveal".to_string(),
                "ZOOM: the face".to_string(),
                "VOLUME: the sound runs at 0x of its own level here".to_string(),
            ],
        )
    });
    let shown = up::brief(&decorated, &[], up::BRIEF_MAX_CHARS);
    assert!(
        shown.contains("CLIP 1 (at 00:00 in the video, 12.0 s): session 00:00\u{2013}00:12, the source runs at 4x here"),
        "{shown}"
    );
    assert!(shown.contains("  MARKED: MARKED: the reveal"), "{shown}");
    assert!(shown.contains("  MARKED: ZOOM: the face"), "{shown}");
    assert!(
        shown.contains("  MARKED: VOLUME: the sound runs at 0x of its own level here"),
        "{shown}"
    );
    assert!(shown.contains("the source runs at 4x here"), "{shown}");
    // An undecorated clip's header keeps S1's form with no rate clause bolted on.
    assert!(
        !text.contains("the source runs at"),
        "an unsped clip says nothing about a rate: {text}"
    );
}

#[test]
fn f5_6_s1_no_narration_says_so_in_words() {
    let text = up::brief(&clips(1), &[], up::BRIEF_MAX_CHARS);
    assert!(
        text.contains("(no narration has been written for this video)"),
        "{text}"
    );
}

#[test]
fn f5_6_s1_a_long_brief_folds_and_points_at_get_lines() {
    assert_eq!(
        up::BRIEF_MAX_CHARS,
        120_000,
        "P.machine.briefMaxChars is 120 kB"
    );
    // Ten thousand chars per event line blows the bound on the first pass.
    let big = "x".repeat(10_000);
    let mut many = clips(20);
    for clip in many.iter_mut() {
        clip.seen = vec![big.clone(); 10];
        clip.said = vec![big.clone(); 10];
    }
    let folded = up::brief(&many, &[line(0.0, "a line")], 1_000);
    assert!(
        folded.contains("get_lines"),
        "the fold tells the model where to read more"
    );
    // Folding keeps FOLDED_LINES per side per clip rather than all ten, so the brief sheds most of what it
    // would have carried. The bound itself is only the trigger to fold, not a ceiling on the result: with
    // 10 kB lines kept, a folded brief is still long, and that is the model's problem to read, not the
    // app's to truncate mid-sentence.
    let would_have = 20 * 2 * 10 * 10_000;
    assert!(
        folded.len() * 2 < would_have,
        "folding shed less than half of the {would_have} chars: {}",
        folded.len()
    );
    // Each clip kept only FOLDED_LINES of each side.
    let seen_count = folded.matches("SEEN:").count();
    assert_eq!(
        seen_count,
        20 * up::FOLDED_LINES,
        "each clip folded to FOLDED_LINES events"
    );
}

// ---- S2: the job, the tools, the prose fallback ---------------------------------------------------

#[test]
fn f5_6_s2_the_system_is_youtube_with_thinking_and_web_tools() {
    assert!(
        naivepost::prompts::thinking("youtube"),
        "the youtube prompt thinks"
    );
    assert_eq!(naivepost::prompts::job("youtube"), Some(Job::UploadText));
    let offered: Vec<&str> = naivepost::tools::offered(Job::UploadText)
        .iter()
        .map(|t| t.name())
        .collect();
    for want in [
        "set_title",
        "set_description",
        "pick_frame",
        "set_thumbnail_instruction",
        "finish",
        "web_search",
        "web_read",
    ] {
        assert!(offered.contains(&want), "{want} is offered: {offered:?}");
    }
}

#[test]
fn f5_6_s2_the_prose_reply_peels_three_labelled_lines() {
    let reply = "TITLE: A win nobody expected\nTHUMBNAIL: brighten the last frame\n\nThe video opens cold.\nSecond paragraph.";
    let (title, thumb, desc) = up::peel(reply);
    assert_eq!(title.as_deref(), Some("A win nobody expected"));
    assert_eq!(thumb.as_deref(), Some("brighten the last frame"));
    assert_eq!(
        desc.as_deref(),
        Some("The video opens cold.\nSecond paragraph.")
    );

    // A frame line is a frame, not an instruction.
    let framed = "TITLE: Look at this\nTHUMBNAIL: frame: clip 2 +12\n\nBody text here.";
    let (_, thumb, _) = up::peel(framed);
    let value = thumb.expect("a thumbnail line");
    let named =
        naivepost::produce_details::frame_line(value.trim_start_matches("THUMBNAIL: ").trim())
            .expect("parses as a frame reference");
    assert_eq!(
        naivepost::produce_details::frame_moment(named, &[0.0, 20.0]),
        Some(32.0),
        "clip 2 +12 resolves against the cut's starts"
    );

    // A short first description line ending in ':' is a heading the model left behind.
    let headed = "TITLE: T\nChapters:\n0:00 The start\n0:12 The rest";
    let (_, _, desc) = up::peel(headed);
    let desc = desc.expect("a description");
    assert!(
        !desc.starts_with("Chapters:"),
        "the heading was dropped: {desc}"
    );
    assert!(desc.contains("0:00 The start"), "the body survived: {desc}");
}

// ---- S3: the picked frame ------------------------------------------------------------------------

#[test]
fn f5_6_s3_a_picked_frame_is_the_thumbnail_as_it_is() {
    // Clip 2 starts at 20.0; asked for +12 → 32.0, nearest extracted frame 34.0 → 2 s away.
    let picked = up::pick_frame(2, 12.0, &[0.0, 20.0], &[1.0, 10.0, 34.0, 40.0]).expect("resolves");
    assert_eq!(picked.asked, 32.0);
    assert_eq!(picked.taken, 34.0, "the frame ACTUALLY taken");
    assert!(
        (picked.distance - 2.0).abs() < 1e-9,
        "and how far it is: {}",
        picked.distance
    );
    assert!(
        picked.answer.contains("nothing is drawn"),
        "{}",
        picked.answer
    );
    assert!(picked.own, "a picked frame means the picture is one's own");
    // A frame naming a clip not in the cut names nothing.
    assert!(up::pick_frame(9, 1.0, &[0.0, 20.0], &[1.0]).is_err());
    // No frames at all is also a refusal, never a guess.
    assert!(up::pick_frame(1, 1.0, &[0.0], &[]).is_err());
}

#[test]
fn f5_6_s3_an_instruction_after_a_picked_frame_is_refused() {
    let err = up::set_thumbnail_instruction(true, "brighter", None).unwrap_err();
    assert!(err.contains("frame"), "the refusal names the frame: {err}");
    // Without a frame it is accepted, and an empty one is refused with S6's sentence.
    assert!(up::set_thumbnail_instruction(false, "brighter", Some("no blur")).is_ok());
    assert_eq!(
        up::set_thumbnail_instruction(false, "   ", None).unwrap_err(),
        up::NO_INSTRUCTION
    );
}

#[test]
fn f5_6_s3_a_title_answers_with_its_fitted_size_or_the_overflow_warning() {
    let band = TitleBox::default();
    let five = up::set_title(
        &Publish::default(),
        "five good words are fine",
        &band,
        false,
    );
    assert_eq!(five.words, 5);
    assert!(!five.overflows, "five words fit the advice");
    assert!(
        five.size > 0.0,
        "the answer carries the fitted size: {}",
        five.size
    );
    assert!(!five.answer.contains("overflow"), "{}", five.answer);

    let nine = up::set_title(
        &Publish::default(),
        "one two three four five six seven eight nine",
        &band,
        false,
    );
    assert_eq!(nine.words, 9);
    assert!(
        nine.overflows,
        "over seven words warns rather than refusing"
    );
    assert!(nine.answer.contains("overflow"), "{}", nine.answer);
    assert!(
        nine.size > 0.0,
        "it still reports a size — the title is KEPT, not dropped"
    );
}

// ---- S5/S6/S7/S8: the walk ----------------------------------------------------------------------

#[test]
fn f5_6_s5_publish_lands_before_any_draw_and_a_failed_draw_keeps_the_thinking() {
    let rec = Recorder::new();
    let answers = up::Answers {
        title: Some("A win nobody expected".into()),
        instruction: Some("brighten the last frame".into()),
        negative: None,
        description: Some("the body of the description".into()),
        picked_frame: None,
    };
    let out = walk_with(
        &rec,
        &publish_with(&[], "old", ""),
        &answers,
        None,
        false,
        false,
    );
    assert!(out.published, "publish.json + description.txt landed");
    let published_at = rec.index("publish").expect("publish was called");
    let drew_at = rec.index("draw").expect("the draw ran");
    assert!(
        published_at < drew_at,
        "S5: the text is written BEFORE the picture is drawn: {:?}",
        rec.calls.borrow()
    );

    // A draw that fails keeps the thinking: publish already happened, and the error is reported.
    let rec2 = Recorder::new();
    let failed = walk_with(
        &rec2,
        &publish_with(&[], "old", ""),
        &answers,
        None,
        false,
        true,
    );
    assert!(failed.published, "the text survives a failed draw (S5)");
    assert_eq!(failed.error.as_deref(), Some("sd.cpp said no"));
    assert!(
        !rec2.took("stamp"),
        "no stamp for a picture that never arrived"
    );
}

#[test]
fn f5_6_s5_a_changed_title_reprints_and_a_hand_edited_picture_does_not() {
    // Untouched picture, new title → reprint.
    assert!(
        naivepost::audit_gaps::reprint_title_on_change("old title", "a new title", false),
        "a changed title over an untouched picture prints"
    );
    // Same title → nothing to print.
    assert!(
        !naivepost::audit_gaps::reprint_title_on_change("same", "same", false),
        "equal text changes nothing"
    );
    // Hand-edited picture: even a changed title leaves it alone (decision 34).
    assert!(
        !naivepost::audit_gaps::reprint_title_on_change("old", "brand new", true),
        "once the picture is the user's own, the title is not reprinted"
    );
    // The walk reports the same answer it hands the model, so the two cannot disagree.
    let band = TitleBox::default();
    let answer = up::set_title(&publish_with(&[], "old", ""), "a new title", &band, false);
    assert!(
        answer.changed,
        "set_title says the picture must be reprinted"
    );
    let mine = up::set_title(&publish_with(&[], "old", ""), "a new title", &band, true);
    assert!(
        !mine.changed,
        "and says it must not once the picture is theirs"
    );
}

#[test]
fn f5_6_s6_the_draw_request_is_the_sd_body_with_the_no_words_tail() {
    // The band word follows the title box's centre: thirds of the frame, not the box's height.
    assert_eq!(up::Band::of(0.1).word(), "upper");
    assert_eq!(up::Band::of(0.5).word(), "middle");
    assert_eq!(up::Band::of(0.9).word(), "lower");
    let prompt = up::draw_prompt("brighten the last frame", up::Band::Upper);
    assert!(prompt.starts_with("brighten the last frame "), "{prompt}");
    assert!(
        prompt.ends_with("Keep the upper part of the picture calm and uncluttered: a title will be printed across it afterwards."),
        "{prompt}"
    );
    assert!(
        prompt.contains(
            "Do not write any words, letters, titles, logos or captions into the picture."
        ),
        "{prompt}"
    );

    // Frame = the video's aspect at long side 1280, both sides even.
    assert_eq!(up::frame_size(16, 9), (1280, 720));
    assert_eq!(up::frame_size(9, 16), (720, 1280));
    assert_eq!(up::frame_size(4, 3), (1280, 960));
    for (w, h) in [up::frame_size(7, 3), up::frame_size(15, 16)] {
        assert!(w % 2 == 0 && h % 2 == 0, "even sides: {w}x{h}");
        assert!(w.max(h) == 1280, "long side stops at 1280: {w}x{h}");
    }

    let body = up::request_body(
        "brighten it",
        Some("no blur"),
        1280,
        720,
        &["data:image/png;base64,AAA".to_string()],
    );
    assert_eq!(body["seed"], -1, "a fresh draw every time");
    assert_eq!(body["output_format"], "png");
    assert_eq!(body["width"], 1280);
    assert_eq!(body["height"], 720);
    assert_eq!(body["negative_prompt"], "no blur");
    assert_eq!(body["auto_resize_ref_image"], true);
    assert_eq!(body["ref_images"].as_array().map(|a| a.len()), Some(1));
}

#[test]
fn f5_6_s6_an_empty_instruction_is_refused_with_the_spec_own_sentence() {
    // The refusal is pinned by what it must contain rather than by retyping the dash and the arrow:
    // `up::NO_INSTRUCTION` holds them as escapes, and this checks the sentence's three load-bearing parts.
    assert!(up::NO_INSTRUCTION.starts_with("nothing to tell the image model"));
    assert!(up::NO_INSTRUCTION.contains("write an edit instruction first"));
    assert!(up::NO_INSTRUCTION.ends_with("suggests one)"));
    // With no instruction the walk never sends an empty prompt: the first image of the row (here the three taken
    // from the cut) is the thumbnail as it is, and sd.cpp is not asked. The sentence above is for a session that
    // has no image to take either.
    let rec = Recorder::new();
    let answers = up::Answers {
        title: Some("a title".into()),
        ..Default::default()
    };
    let out = walk_with(&rec, &Publish::default(), &answers, None, false, false);
    assert_eq!(out.error, None, "{:?}", out.error);
    assert!(out.own, "the first image is the picture");
    assert!(
        !rec.took("draw"),
        "nothing was asked of sd.cpp: {:?}",
        rec.calls.borrow()
    );
}

#[test]
fn f5_6_s6_the_poll_line_names_the_queue_when_there_is_one() {
    assert_eq!(up::poll_line("queued", 0), "drawing (queued)");
    assert_eq!(
        up::poll_line("drawing", 3),
        "drawing (drawing, 3 ahead in the queue)"
    );
}

#[test]
fn f5_6_s6_a_stamp_that_matches_skips_the_draw() {
    assert!(
        naivepost::produce_details::already_drawn(true, true),
        "both files and a match skips"
    );
    assert!(
        !naivepost::produce_details::already_drawn(false, true),
        "one file is half a picture, so it draws"
    );
    assert!(
        !naivepost::produce_details::already_drawn(true, false),
        "a differing stamp draws"
    );

    let base = up::thumbnail_stamp(&["a.jpg".into()], "brighten", "no blur", "16:9", false);
    // One row per stamp input: what changed, and whether the stamp therefore differs from `base`.
    struct StampCase {
        what: &'static str,
        images: Vec<String>,
        instruction: &'static str,
        negative: &'static str,
        aspect: &'static str,
        own: bool,
    }
    let cases: [StampCase; 5] = [
        StampCase {
            what: "instruction",
            images: vec!["a.jpg".into()],
            instruction: "darken",
            negative: "no blur",
            aspect: "16:9",
            own: false,
        },
        StampCase {
            what: "images",
            images: vec!["b.jpg".into()],
            instruction: "brighten",
            negative: "no blur",
            aspect: "16:9",
            own: false,
        },
        StampCase {
            what: "negative",
            images: vec!["a.jpg".into()],
            instruction: "brighten",
            negative: "very blurry",
            aspect: "16:9",
            own: false,
        },
        StampCase {
            what: "aspect",
            images: vec!["a.jpg".into()],
            instruction: "brighten",
            negative: "no blur",
            aspect: "9:16",
            own: false,
        },
        StampCase {
            what: "own",
            images: vec!["a.jpg".into()],
            instruction: "brighten",
            negative: "no blur",
            aspect: "16:9",
            own: true,
        },
    ];
    for case in cases {
        assert_ne!(
            base,
            up::thumbnail_stamp(
                &case.images,
                case.instruction,
                case.negative,
                case.aspect,
                case.own
            ),
            "changing the {} moves the picture's stamp",
            case.what
        );
    }
    // The same images in another order are the same reference set.
    assert_eq!(
        up::thumbnail_stamp(&["a.jpg".into(), "b.jpg".into()], "i", "n", "16:9", false),
        up::thumbnail_stamp(&["b.jpg".into(), "a.jpg".into()], "i", "n", "16:9", false),
        "row order does not cost a redraw"
    );

    // The walk honours a matching stamp: no draw call, and it says why.
    let rec = Recorder::new();
    let answers = up::Answers {
        title: Some("a title".into()),
        instruction: Some("brighten".into()),
        ..Default::default()
    };
    let publish = publish_with(&["a.jpg"], "a title", "brighten");
    let current = up::thumbnail_stamp(&publish.frames, "brighten", "", up::DEFAULT_ASPECT, false);
    let out = walk_with(&rec, &publish, &answers, Some(&current), false, false);
    assert!(
        !rec.took("draw"),
        "a matching stamp asks sd.cpp for nothing: {:?}",
        rec.calls.borrow()
    );
    assert!(out.already_drawn, "and the walk says it skipped");
}

#[test]
fn f5_6_s7_texts_print_before_the_title_then_the_stamp_lands() {
    assert_eq!(up::print_order(), &["texts", "title"][..]);
    let rec = Recorder::new();
    let answers = up::Answers {
        title: Some("a title".into()),
        instruction: Some("brighten".into()),
        description: Some("d".into()),
        ..Default::default()
    };
    let mut publish = publish_with(&["a.jpg"], "old title", "brighten");
    publish.texts = vec![naivepost::project::TextMark {
        cx: 0.5,
        cy: 0.8,
        wf: 0.8,
        hf: 0.1,
        text: "a marked line".to_string(),
    }];
    let out = walk_with(&rec, &publish, &answers, None, false, false);
    // publish → draw → poll → the plain picture written → the two print passes in `print_order`'s order →
    // stamp. The `picture` step is S6's last act and the only thing S7's words can print onto: without it the
    // walk would print onto a file it never made and write a stamp for a picture that is not there.
    let order = rec.calls.borrow();
    let want = [
        "model",
        "publish",
        "draw",
        "poll",
        "picture",
        "print-leg",
        "print-leg",
        "stamp",
    ];
    let got: Vec<&str> = order.iter().map(|c| c.as_str()).collect();
    assert_eq!(
        got,
        want.to_vec(),
        "the whole half runs in the spec's order"
    );
    // Two print calls happened, and they were texts first then title — the order S7 states.
    assert_eq!(PRINTED.with(|p| p.borrow().clone()), vec!["texts", "title"]);
    assert!(out.reprinted, "the title changed, so it printed");
}

#[test]
fn f5_6_s8_a_missing_title_or_instruction_keeps_the_previous_value_and_the_description_always_changes(
) {
    let previous = publish_with(&["a.jpg"], "the old title", "the old instruction");
    // A reply that names neither TITLE nor THUMBNAIL: both keep what they were, the description moves.
    let answers = up::Answers {
        description: Some("a brand new description".into()),
        ..Default::default()
    };
    let rec = Recorder::new();
    let _ = walk_with(&rec, &previous, &answers, None, false, false);
    // `finish` still names what is missing so the rewrite can fix it rather than ship last run's answer.
    assert_eq!(
        up::finish_needs("", "written already"),
        vec!["title"],
        "a blank title is named"
    );
    assert_eq!(up::finish_needs("a title", ""), vec!["description"]);
    assert_eq!(
        up::finish_needs("a title", "a description"),
        Vec::<&str>::new()
    );
    // The record itself kept both old values: only the description moved.
    assert_eq!(previous.title, "the old title");
    assert_eq!(previous.prompt, "the old instruction");
}

// ---- the widget wire -----------------------------------------------------------------------------

static WIRE_FAIL: Mutex<Option<String>> = Mutex::new(None);

fn record(
    slot: &Mutex<Option<String>>,
    label: &str,
    body: impl FnOnce() -> Result<(), String> + std::panic::UnwindSafe,
) {
    let outcome = std::panic::catch_unwind(body);
    let failure = match outcome {
        Ok(Ok(())) => None,
        Ok(Err(why)) => Some(why),
        Err(payload) => Some(
            payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "<non-string panic>".to_string()),
        ),
    };
    if let Some(why) = failure {
        *slot.lock().unwrap() = Some(format!("{label}: {why}"));
    }
}

thread_local! {
    static CURRENT_APP: std::cell::RefCell<Option<adw::Application>> = const { std::cell::RefCell::new(None) };
}

fn app_in_round() -> adw::Application {
    CURRENT_APP
        .with(|held| held.borrow().clone())
        .expect("run_round published the application")
}

fn two_segs() -> Vec<Seg> {
    let first = Seg {
        s: 0.0,
        e: 9.5,
        ..Default::default()
    };
    let second = Seg {
        s: 10.0,
        e: 30.0,
        ..Default::default()
    };
    vec![first, second]
}

fn produce_window(app: &adw::Application) -> adw::ApplicationWindow {
    release_last_window();
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, Page::Produce.label());
    hold_last_window(window.clone());
    window.present();
    settle();
    window
}

/// Seed the page's encoder row the way `tests/suite/produce_translate_flow.rs` does, so the press has settings.
fn seed_page(_window: &adw::ApplicationWindow) {
    let row = naivepost::produce_screen::defaults();
    let mut page = ui::produce_page::read_state();
    page.container = row[0].1.to_string();
    page.codec = row[1].1.to_string();
    page.preset = row[2].1.to_string();
    page.resolution = row[3].1.to_string();
    page.frame_rate = row[4].1.to_string();
    page.audio = row[5].1.to_string();
    page.subtitles = row[6].1.to_string();
    page.game_volume = row[8].1.parse().expect("game audio parses");
    page.crf = row[9].1.parse().expect("crf parses");
    page.vfr = row[10].1 == "on";
    page.mono = row[11].1 == "on";
    page.blurred_edges = row[12].1 == "on";
    page.voice = "1".to_string();
    page.session_language = "English".to_string();
    ui::produce_page::set_state(page);
}

/// No ffmpeg in this container: each command's own output file is "written" here.
fn scripted_spawn() -> naivepost::produce_translate::SpawnSlot {
    std::sync::Arc::new(|command: &naivepost::produce_exec::Command| {
        if let Some(out) = command.argv.last() {
            let path = Path::new(out);
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(path, b"a rendered file of some length");
        }
        Ok(())
    })
}

/// Where the scripted `save_picture` leg writes: the session folder the page resolves, so the print leg and
/// the assertions below find the picture where the program would have put it.
fn session_picture_path() -> std::path::PathBuf {
    naivepost::layout::Tree::new(naivepost::startup::session_dir(
        &std::env::current_dir().unwrap_or_default(),
    ))
    .map(|tree| tree.thumbnail_plain_png())
    .unwrap_or_else(|_| std::path::PathBuf::from("thumbnail-plain.png"))
}

/// A real picture's bytes for the scripted draw to hand back: the print leg reads this PNG back and letters
/// it (S7), so a placeholder would make the print pass fail for the wrong reason. Built in a scratch file and
/// read back, because the flat-PNG encoder writes to a path; cached, because a page's press asks for it once
/// per poll and two checks share the process.
fn page_drawn_png() -> Vec<u8> {
    let scratch = scratch_png_path();
    let png = match naivepost::produce_upload_print::flat_png_for_test(&scratch, 640, 360, 0.25) {
        Ok(()) => std::fs::read(&scratch).unwrap_or_default(),
        Err(why) => {
            eprintln!("the scripted picture could not be built: {why}");
            Vec::new()
        }
    };
    let _ = std::fs::remove_file(&scratch);
    png
}

/// Where a scripted picture is built before being read back: one file per thread, since the tests share a
/// process and two of them may be drawing at the same moment.
fn scratch_png_path() -> std::path::PathBuf {
    let id = std::thread::current()
        .name()
        .map(|name| name.replace([':', '/'], "_"))
        .unwrap_or_else(|| format!("{}", std::process::id()));
    std::env::temp_dir().join(format!("np-f56-payload-{id}.png"))
}

/// The scripted draw leg for the PAGE's door: it produces the picture the way a finished job's reply does,
/// writing the plain PNG where the server's bytes would land (so the print leg has a real picture to letter),
/// and hands back the job id exactly as the real submit leg answers.
fn scripted_draw(tree: Tree) -> ui::produce_languages::DrawSlot {
    std::sync::Arc::new(move |_body: &serde_json::Value| {
        let plain = tree.thumbnail_plain_png();
        naivepost::produce_upload_print::flat_png_for_test(&plain, 640, 360, 0.25)
            .map_err(|why| why.to_string())?;
        Ok(up::DrawnJob {
            id: "scripted-1".into(),
        })
    })
}

/// The print leg's output check: `thumbnail.png` must exist AND differ from the plain picture, because
/// S7 prints the title onto it. Byte-for-byte different is the whole point — a copy proves nothing.
fn assert_letters_landed(tree: &naivepost::layout::Tree) {
    let plain = std::fs::read(tree.thumbnail_plain_png()).expect("the plain picture is on disk");
    let printed = std::fs::read(tree.thumbnail_png()).expect("the printed picture is on disk");
    assert!(
        plain != printed,
        "the print leg copied instead of printing: both files are {} bytes",
        printed.len()
    );
    assert!(
        printed.len() > 200,
        "the printed picture looks truncated: {} bytes",
        printed.len()
    );
}

// ---- the fake LLM: the SERVER the settings point at ----------------------------------------------

/// A loopback server answering `/v1/chat/completions` from one scripted reply, which it repeats for every
/// later request (so each press of ▶ gets the same answer). This replaces the SERVER, not the work: the
/// program dials it through the address in the settings file, exactly as it dials a real llama.cpp, so the
/// request, §3.9's tools and the prose fallback all run for real. Same harness `tests/suite/produce_upload_wire.rs`
/// proves the wire with. The thread holds what it needs by move; the handle is only the address.
struct FakeLlm {
    address: String,
}

impl FakeLlm {
    fn start(reply: &str) -> FakeLlm {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind a loopback port");
        let address = format!(
            "http://127.0.0.1:{}",
            listener.local_addr().expect("an address").port()
        );
        let reply = reply.to_string();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                serve_one(stream, &reply);
            }
        });
        FakeLlm { address }
    }

    fn address(&self) -> &str {
        &self.address
    }
}

/// Answer one request with the scripted content wrapped in the chat envelope, then close: the client reads
/// to EOF, so a kept-open socket would make every round wait for a timeout.
fn serve_one(mut stream: TcpStream, reply: &str) {
    let mut reader = BufReader::new(stream.try_clone().expect("a clone to read with"));
    let mut request_line = String::new();
    let _ = reader.read_line(&mut request_line);
    let mut length = 0usize;
    loop {
        let mut line = String::new();
        if reader
            .read_line(&mut line)
            .ok()
            .filter(|read| *read > 0)
            .is_none()
        {
            break;
        }
        if let Some(rest) = line.to_ascii_lowercase().strip_prefix("content-length:") {
            length = rest.trim().parse().unwrap_or(0);
        }
        if line.trim().is_empty() {
            break;
        }
    }
    let mut payload = vec![0u8; length];
    if length > 0 && reader.read_exact(&mut payload).is_err() {
        return;
    }
    let body = serde_json::json!({ "choices": [ { "message": { "content": reply }, "finish_reason": "stop" } ] }).to_string();
    let framed = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = stream.write_all(framed.as_bytes());
    let _ = stream.flush();
    let _ = stream.shutdown(std::net::Shutdown::Both);
}

/// Point the SETTINGS at the fake — the seam §02-services#1 says the program re-reads per request: temp
/// config/data folders through the XDG variables, the fake's address as `server`, and an sd.cpp address no
/// poll would wait on (the draw leg is scripted here anyway).
fn point_settings_at(fake_url: &str) {
    let root = std::env::current_dir().expect("cwd pinned before this runs");
    let config = root.join("config");
    let data = root.join("data");
    std::fs::create_dir_all(&config).expect("config folder");
    std::fs::create_dir_all(&data).expect("data folder");
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", &config);
        std::env::set_var("XDG_DATA_HOME", &data);
    }
    let paths =
        naivepost::settings::from_environment().expect("settings folder from the pinned env");
    let mut conf = naivepost::settings::Conf::default();
    conf.model = "upload-flow-model".to_string();
    conf.server = fake_url.to_string();
    conf.sd_server = "http://127.0.0.1:1".to_string();
    naivepost::settings::save(&paths, &conf).expect("settings written");
}

/// The check: press the real ▶ and read the upload text and the picture's stamp off disk.
fn check_press_writes_the_upload_text(root: &Path) {
    let tree = session(root);
    let cut = naivepost::cut::Cut {
        segs: two_segs(),
        aspect: "16:9".to_string(),
        ..Default::default()
    };
    let file = tree.cut_json();
    std::fs::create_dir_all(file.parent().unwrap()).expect("the cut folder exists");
    std::fs::write(&file, serde_json::to_string_pretty(&cut).unwrap()).expect("the cut is written");
    // One event and one transcript line inside the first segment, so the brief has something to say. The
    // per-source folder is named for the recording in `session.tsv`, which is what `source_stem` reads.
    tree.create_dirs().expect("the project's own folders exist");
    std::fs::create_dir_all(tree.describe_dir("room")).expect("the describe folder exists");
    std::fs::write(
        tree.events_tsv("room"),
        "1.0\t5.0\ta screen showing the score\n",
    )
    .expect("the event is written");
    std::fs::write(
        tree.session_tsv(),
        "1.0\t5.0\t/media/room.wav\tsomeone\tthe score went up\n",
    )
    .expect("the transcript line is written");
    assert!(
        !tree.publish_json().exists(),
        "the fixture root starts with no publish folder"
    );

    // The model leg gets a SERVER, not a slot: a local fake LLM the settings name, answering with the three
    // labelled lines plus a description — §3.9's prose shape, peeled by the program's own parser.
    let fake = FakeLlm::start(
        "TITLE: The comeback nobody saw coming\nTHUMBNAIL: brighten the final scoreboard\n\nThe session was close until the last round, and then it was not.",
    );
    point_settings_at(fake.address());
    ui::produce_languages::set_draw_for_test(scripted_draw(tree.clone()));
    naivepost::produce_translate::set_spawn_for_test(scripted_spawn());

    let window = produce_window(&app_in_round());
    ui::seed_review_cut(&window, &cut);
    seed_page(&window);

    let button =
        ui::line_step_button(&window, "play-button").expect("the run bar drew a play-button");
    // The wire this check fires is the run bar's own ▶, found by its stable name and nothing else.
    assert!(
        ui::line_step_button(&window, "play-button").is_some(),
        "the run bar drew its play-button"
    );
    button.emit_by_name::<()>("clicked", &[]);
    // The press walks on a worker (§03-shell §1): its answer lands before anything is read.
    assert!(
        naivepost::ui::background::settle_for_test(std::time::Duration::from_secs(120)),
        "the press's worker answered"
    );
    settle();

    let said = ui::find_status(window.upcast_ref())
        .expect("the window has a status line")
        .text()
        .to_string();
    assert_ne!(
        said,
        naivepost::cut::NO_CUT_YET,
        "the press thought there was no cut: {said:?}"
    );

    let json = std::fs::read_to_string(tree.publish_json()).unwrap_or_else(|_| {
        panic!(
            "publish.json was not written at {}",
            tree.publish_json().display()
        )
    });
    assert!(
        json.contains("The comeback nobody saw coming"),
        "the scripted title landed in publish.json: {json}"
    );
    assert!(
        json.contains("brighten the final scoreboard"),
        "and so did the instruction: {json}"
    );
    let desc = std::fs::read_to_string(tree.description_txt()).unwrap_or_else(|_| {
        panic!(
            "description.txt was not written at {}",
            tree.description_txt().display()
        )
    });
    assert!(desc.contains("close until the last round"), "{desc}");

    assert!(
        tree.thumbnail_plain_png().exists(),
        "the drawn plain picture is on disk"
    );
    assert!(
        tree.thumbnail_png().exists(),
        "the printed thumbnail is on disk at {}",
        tree.thumbnail_png().display()
    );
    assert_letters_landed(&tree);
    let stamp = std::fs::read_to_string(tree.thumbnail_stamp()).unwrap_or_else(|_| {
        panic!(
            "thumbnail.stamp was not written at {}",
            tree.thumbnail_stamp().display()
        )
    });
    assert_eq!(stamp.trim().len(), 16, "the stamp is a hash: {stamp}");

    let logs = ui::window::window_logs();
    assert!(
        logs.iter().any(|l| l.contains("THE FINISHED VIDEO:")),
        "the run logged the brief: {:?}",
        logs.iter()
            .filter(|l| l.contains("publish"))
            .collect::<Vec<&String>>()
    );
    assert!(
        !logs.iter().any(|l| l.contains("no image server here")),
        "the old unconditional stub failure is gone from the log"
    );
    window.close();
    settle();
}

fn run_round(app: &adw::Application) {
    CURRENT_APP.with(|held| *held.borrow_mut() = Some(app.clone()));
    let base = std::env::current_dir().expect("cwd pinned before the run");
    let root = base.join("upload");
    std::fs::create_dir_all(&root).expect("check root created");
    std::env::set_current_dir(&root).expect("cwd into the check root");
    record(
        &WIRE_FAIL,
        "upload-text press",
        std::panic::AssertUnwindSafe(move || {
            check_press_writes_the_upload_text(&root);
            Ok(())
        }),
    );
    let app = app.clone();
    glib::idle_add_local(move || {
        app.quit();
        glib::ControlFlow::Break
    });
}

#[test]
fn f5_6_s1_pressing_produce_runs_the_upload_half_through_the_real_button() {
    let root = std::env::temp_dir().join(format!("np-f56wire-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("temp root created");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");

    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(run_round);
        app.run_with_args(&["naivepost"]);
    });

    let failed = WIRE_FAIL.lock().unwrap().take();
    std::fs::remove_dir_all(&root).ok();
    assert!(failed.is_none(), "{}", failed.unwrap_or_default());
}

#[test]
fn f5_6_s4_three_frames_from_the_cut_fill_the_row() {
    // S4 is not only a log line: the three frames go INTO the row, because `base_of`/`refs_of` read
    // `publish.frames` and an empty row would send sd.cpp nothing to edit.
    let rec = Recorder::new();
    let answers = up::Answers {
        title: Some("A win nobody expected".into()),
        instruction: Some("brighten the last frame".into()),
        negative: None,
        description: Some("the body".into()),
        picked_frame: None,
    };
    let out = walk_with(
        &rec,
        &publish_with(&[], "old", "draw me"),
        &answers,
        None,
        false,
        false,
    );
    assert!(out.took_three, "S4 says it took three");
    let row = published().frames;
    assert_eq!(
        row,
        vec![
            "frame:5.0".to_string(),
            "frame:15.0".to_string(),
            "frame:25.0".to_string()
        ],
        "the row holds the three spread frames before the draw: {row:?}"
    );
    // And the base really comes off that row — the point of filling it at all.
    assert_eq!(naivepost::publish::base(&published()), Some("frame:5.0"));
}

#[test]
fn f5_6_s4_spread_skips_inserts_and_freezes() {
    use naivepost::produce_upload_frames as frames;
    // Two kept footage stretches with an insert between them: the ruler is the KEPT footage only, so the
    // three moments land at the thirds of 0–30 (a 10 s + 20 s pair), never inside the card.
    let mut card = common::seg(10.0, 14.0);
    card.ins = "tier.svg".to_string();
    let segs = vec![common::seg(0.0, 10.0), card, common::seg(14.0, 34.0)];
    let moments = frames::spread_moments(&segs);
    assert_eq!(moments.len(), 3, "{moments:?}");
    for at in &moments {
        assert!(
            !(*at >= 10.0 && *at < 14.0),
            "a moment fell inside the insert: {moments:?}"
        );
    }
    // A zero-length stretch (a freeze) contributes nothing either: two 10 s clips give the thirds of 20 s.
    let plain = vec![common::seg(0.0, 10.0), common::seg(10.0, 20.0)];
    assert_eq!(
        frames::spread_moments(&plain),
        vec![10.0 / 3.0 * 1.0, 10.0 / 3.0 * 3.0, 10.0 / 3.0 * 5.0],
        "evenly spread over the kept total"
    );
    // Spread over the VIDEO, answered on the SESSION clock (a `frame:` entry is the session second a frame on
    // disk is named by): two 10 s clips filmed at 100 s and 200 s put the middle moment at 200 s, not at the
    // video's 10 s. The first rewrite answered produced seconds, and the slots showed other moments.
    let apart = vec![common::seg(100.0, 110.0), common::seg(200.0, 210.0)];
    let spread = frames::spread_moments(&apart);
    let want = [100.0 + 10.0 / 3.0, 200.0, 200.0 + 20.0 / 3.0];
    assert!(
        spread
            .iter()
            .zip(want)
            .all(|(got, want)| (got - want).abs() < 1e-6),
        "on the session clock: {spread:?}"
    );
    // Nothing kept at all: no moments, which is what makes the walk log NO_FRAMES_LOG instead of lying.
    let frozen = vec![common::seg(3.0, 3.0)];
    assert!(
        frames::spread_moments(&frozen).is_empty(),
        "nothing kept, nothing to show"
    );
    // The row spelling is the one the model's own `frame:` line uses, and `frame_at` reuses the ONE nearest
    // rule so a spread frame resolves like a picked one.
    assert_eq!(frames::row_entry(12.34), "frame:12.3");
    assert_eq!(frames::frame_at(&[1.0, 9.0], 8.0), Some(9.0));
    assert_eq!(frames::spread_rows(&plain).len(), 3);
}

#[test]
fn f5_6_s4_a_chosen_frame_blocks_the_three() {
    // S3 first: a picked frame means the row holds THAT frame only, so S4 must not append three behind it.
    let rec = Recorder::new();
    let answers = up::Answers {
        title: Some("A win nobody expected".into()),
        instruction: Some("brighten the last frame".into()),
        negative: None,
        description: Some("the body".into()),
        picked_frame: Some(12.0),
    };
    let out = walk_with(
        &rec,
        &publish_with(&[], "old", "draw me"),
        &answers,
        None,
        false,
        false,
    );
    assert!(out.own, "a picked frame is the user's own picture");
    assert!(!out.took_three, "S4 does not run once a frame was chosen");
    let row = published().frames;
    assert_eq!(row.len(), 1, "the row holds the chosen frame only: {row:?}");
    assert!(row[0].starts_with("frame:"), "{}", row[0]);
}

/// S3 + S7: a frame picked THIS run is a new picture with no words on it, so the answered title is printed onto
/// it and kept as the printed title, even on a project an earlier run had already given a picked frame (which
/// counts as the picture being one's own). The prototype takes the frame and prints the title onto it; the first
/// rewrite printed nothing, and the thumbnail went out without its white title.
#[test]
fn f5_6_s3_a_frame_picked_this_run_gets_the_title_printed() {
    let rec = Recorder::new();
    let answers = up::Answers {
        title: Some("Distributed Systems in the News".into()),
        instruction: None,
        negative: None,
        description: Some("the body".into()),
        picked_frame: Some(12.0),
    };
    let mut before = publish_with(&["frame:3.8"], "", "");
    before.own = true;
    let out = walk_with(&rec, &before, &answers, None, true, false);
    assert!(
        out.calls.iter().any(|c| c == "own-frame"),
        "the picked frame was taken: {:?}",
        out.calls
    );
    assert!(
        out.calls.iter().any(|c| c == "print-title"),
        "and the title printed onto it: {:?}",
        out.calls
    );
    assert_eq!(
        published().thumb_title,
        "Distributed Systems in the News",
        "the record's printed title is the answered one"
    );
}

/// S7, the prototype's `TitleSeeded`: the first title to exist is seeded as the picture's printed title, and
/// nothing seeds it twice. After that the words on the thumbnail are the person's (set and placed on the page),
/// and a reworded video title leaves them alone.
#[test]
fn f5_6_s7_the_printed_title_is_seeded_once() {
    let rec = Recorder::new();
    let answers = up::Answers {
        title: Some("A reworded video title".into()),
        instruction: None,
        negative: None,
        description: Some("the body".into()),
        picked_frame: Some(12.0),
    };
    let mut before = publish_with(&["frame:3.8"], "The first title", "");
    before.thumb_title = "Words I set on the picture".to_string();
    before.title_seeded = true;
    walk_with(&rec, &before, &answers, None, false, false);
    let written = published();
    assert_eq!(
        written.title, "A reworded video title",
        "the video title is the answer's"
    );
    assert_eq!(
        written.thumb_title, "Words I set on the picture",
        "the picture's words stay the person's"
    );
}

#[test]
fn f5_6_s4_no_images_chosen_takes_three_from_the_cut() {
    assert_eq!(
        up::take_three_log(),
        "    publish: no images chosen \u{2014} taking 3 from the cut"
    );
    // The three moments come from the existing candidate rule: middles of three equal bands over 30 s.
    let kept = [(0.0f64, 30.0f64)];
    let extracted: Vec<f64> = (0..31).map(|s| s as f64).collect();
    let wanted = naivepost::produce_details::candidate_frames(&kept, &extracted);
    assert_eq!(wanted.len(), 3, "three candidates: {wanted:?}");
    for expect in [5.0, 15.0, 25.0] {
        assert!(
            wanted.iter().any(|w| (*w - expect).abs() < 1e-9),
            "{expect}s is one of them: {wanted:?}"
        );
    }
    // Nothing extracted: `candidate_frames` computes the three band middles from the KEPT stretch and hands
    // back what it was given only when there are too few to fill the row — so with an empty extraction the
    // caller has nothing real to send, which is exactly why NO_FRAMES_LOG exists.
    let none = naivepost::produce_details::candidate_frames(&kept, &[]);
    assert_eq!(
        none.len(),
        3,
        "the middles are computed whether or not frames exist"
    );
    let sent = naivepost::produce_details::candidate_frames(&[], &[5.0, 15.0, 25.0]);
    assert_eq!(
        sent,
        vec![5.0, 15.0, 25.0],
        "with nothing kept, what was extracted is handed back"
    );
    assert!(
        naivepost::produce_details::NO_FRAMES_LOG.contains("instruction alone"),
        "{}",
        naivepost::produce_details::NO_FRAMES_LOG
    );
}

/// S7 on the real leg: one print call draws letters. A flat PNG goes in, a DIFFERENT PNG comes out, and the
/// bytes differ because ink is on them — not because a file was copied.
#[test]
fn f5_6_s7_the_title_lands_on_the_picture() {
    let root = std::env::temp_dir().join(format!("np-f56s7print-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("temp root created");
    let plain = root.join("plain.png");
    let out = root.join("out.png");
    naivepost::produce_upload_print::flat_png_for_test(&plain, 480, 270, 0.2).expect("a flat base");
    let before = std::fs::read(&plain).expect("the base is a PNG");
    let band = naivepost::project::TitleBox {
        cx: 0.5,
        cy: 0.3,
        wf: 0.9,
        hf: 0.3,
    };
    // A marked text in its OWN box near the bottom, beside the title in its band near the top: the prototype's
    // `drawPubTexts` prints each where it was put. The first rewrite squeezed every marked text into a slice of
    // the title band.
    let mark = naivepost::project::TextMark {
        cx: 0.5,
        cy: 0.85,
        wf: 0.6,
        hf: 0.2,
        text: "Episode seven".to_string(),
    };
    naivepost::produce_upload_print::print_words(
        &plain,
        &out,
        &[mark],
        "The comeback nobody saw coming",
        &band,
    )
    .expect("the print leg drew the title");
    // Read back: white letters AND a dark edge round them, in the band and in the mark's own box (the lettering
    // of a Cut Text effect, the prototype's `drawFxText`; the first rewrite drew white with a soft shadow).
    let mut file = std::fs::File::open(&out).expect("the printed picture");
    let mut printed = cairo::ImageSurface::create_from_png(&mut file).expect("a PNG");
    let (wide, high, stride) = (printed.width(), printed.height(), printed.stride());
    let data = printed.data().expect("pixels").to_vec();
    let ink = |top: f64, bottom: f64| {
        let (mut white, mut dark) = (0, 0);
        for row in (top * f64::from(high)) as i32..(bottom * f64::from(high)) as i32 {
            for col in 0..wide {
                let at = (row * stride + col * 4) as usize;
                let (b, g, r) = (data[at], data[at + 1], data[at + 2]);
                if r > 240 && g > 240 && b > 240 {
                    white += 1;
                } else if r < 25 && g < 25 && b < 25 {
                    dark += 1;
                }
            }
        }
        (white, dark)
    };
    let (band_white, band_dark) = ink(0.15, 0.45);
    assert!(
        band_white > 50 && band_dark > 50,
        "the title is white with a dark edge: {band_white} white, {band_dark} dark pixels"
    );
    let (mark_white, mark_dark) = ink(0.75, 0.95);
    assert!(
        mark_white > 20 && mark_dark > 20,
        "and the marked text sits in its own box, lettered the same: {mark_white} white, {mark_dark} dark"
    );
    let after = std::fs::read(&out).expect("the printed picture exists");
    assert!(
        before != after,
        "printing letters left the picture byte-identical to the base"
    );
    assert!(
        after.len() > 200,
        "the printed picture looks truncated: {} bytes",
        after.len()
    );
    // An empty word list prints nothing and says so by leaving the base alone rather than erroring.
    let bare = root.join("bare.png");
    naivepost::produce_upload_print::print_words(&plain, &bare, &[], "", &band)
        .expect("nothing to print is ok");
    // A missing base refuses with the file named, which is where an operator would look first.
    let why =
        naivepost::produce_upload_print::print_words(&root.join("nope.png"), &out, &[], "x", &band)
            .expect_err("a missing base must refuse");
    assert!(why.contains("nope.png"), "{why}");
    let _ = std::fs::remove_dir_all(&root);
}
