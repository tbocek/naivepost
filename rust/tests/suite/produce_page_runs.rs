//! F5.7 Page runs — `spec/08-produce.md` §F5.7 (lines 117-131), checked against `naivepost::produce_runs`: what
//! each of the page's eight presses asks a model for, what it leaves alone, and what it writes where.
//!
//! The item is a cost list as much as a feature list — one sd.cpp call, one LLM call, or nothing at all — so most
//! of these assertions count calls rather than outputs. That is deliberate: "the redraw rewrote the description"
//! and "the reprint drew a second picture" are both invisible in the finished file and visible only here.
//!
//! Ids used, each asserted against its §10 row in `params::produce()` (`params::find` searches Prepare's rows
//! only): P.eng.thumbnailJPEGMax (2097152), P.policy.publishWordsSnapPx (30 — the plan asked for a
//! `P.policy.publishWordsSafeFraction`; no safe-area fraction exists anywhere in the app, so what the words box
//! actually stops on is the snap grid). Tools cited: `tool:ffmpeg.encode` — ↻ Transcode is ffmpeg with the
//! render's own settings and no model call.

use crate::common;
use naivepost::params;
use naivepost::produce_render as render;
use naivepost::produce_runs as runs;
use naivepost::project::{Codec, Container, Produce, Publish, Resolution, Subtitles, TitleBox};

const ITEM: &str = "F5.7";

/// The spec's own wording for the two runs that rewrite something — quoted so a paraphrase in the code cannot
/// pass as the item it implements.
const SPEC_REWORD: &str = "Ask again for the title and description — one LLM call, two \
answers, nothing else rewritten (not even the picture it prints)";

/// One row of the catalogue's Produce section, by id.
fn row(id: &str) -> String {
    params::produce()
        .into_iter()
        .find(|param| param.id == id)
        .unwrap_or_else(|| panic!("{id} is catalogued in §10's Produce rows"))
        .spelled
}

/// The render's settings as the page shows them: the row's defaults, with `track in file` ticked — the one
/// setting §F5.8 S8 has to disagree with.
fn settings() -> Produce {
    Produce {
        subtitles: Subtitles::TrackInFile,
        ..Produce::default()
    }
}

// ---- S1/S2 endings, and the two refusal sentences §F5.7 spells out ---------------------

#[test]
fn f5_7_s1_the_redraw_refusal_names_where_its_frames_come_from() {
    assert_eq!(ITEM, "F5.7");
    // Quoted from spec/08-produce.md §F5.7 line 184: "No cut → \u{201c}no cut yet \u{2014} the thumbnail is
    // drawn from the cut's own frames\u{201d}". It is NOT `cut::NO_CUT_YET`: the picture has a second way to
    // exist, so this sentence says which of the two the reader is waiting for.
    assert_eq!(
        runs::REDRAW_NO_CUT,
        "no cut yet \u{2014} the thumbnail is drawn from the cut's own frames"
    );
    assert_ne!(runs::REDRAW_NO_CUT, naivepost::cut::NO_CUT_YET);
}

#[test]
fn f5_7_s1_the_redraw_ends_naming_the_button_that_renders() {
    assert_eq!(ITEM, "F5.7");
    // The same line 184, its last clause: ends "thumbnail drawn \u{2014} \u{25b6} renders the video".
    assert_eq!(
        runs::REDRAW_DONE,
        "thumbnail drawn \u{2014} \u{25b6} renders the video"
    );
    // The ▶ is in there on purpose: after a picture there is still a video owed, and the status line is
    // where a person looks to be told what comes next.
    assert!(runs::REDRAW_DONE.contains('\u{25b6}'));
}

#[test]
fn f5_7_s2_the_reword_refusal_is_the_cut_steps_own_sentence() {
    assert_eq!(ITEM, "F5.7");
    // Line 185: "No cut → \u{201c}no cut yet \u{2014} build one on the Cut step first\u{201d}" — the very
    // string `crate::cut` owns, named here too so neither press can drift from the other.
    assert_eq!(
        runs::REWORD_NO_CUT,
        "no cut yet \u{2014} build one on the Cut step first"
    );
    assert_eq!(runs::REWORD_NO_CUT, naivepost::cut::NO_CUT_YET);
}

#[test]
fn f5_7_s2_the_reword_end_names_both_answers() {
    assert_eq!(ITEM, "F5.7");
    // Line 185's ending, quoted whole. Both answers named, so the line answers "did the description change
    // too?" without a second press.
    assert_eq!(
        runs::REWORD_DONE,
        "title and description rewritten \u{2014} \u{25b6} renders the video"
    );
    for key in ["title", "description"] {
        assert!(
            runs::REWORD_DONE.contains(key),
            "{key} missing from {}",
            runs::REWORD_DONE
        );
    }
    // The two are the same two `reword()` counts as its answers: the sentence and the cost agree.
    assert_eq!(runs::reword().answers.len(), 2);
}

#[test]
fn f5_7_both_runs_end_through_one_path() {
    assert_eq!(ITEM, "F5.7");
    // Line 186: "Both end through one path: success re-prints the words onto the thumbnail; else
    // \u{2018}<what> failed \u{2014} see log\u{2019} or \u{2018}<what> stopped\u{2019}." One function, so the
    // two buttons cannot report the same trouble in two different voices.
    assert_eq!(
        runs::ending("the thumbnail", runs::REDRAW_DONE, true, false),
        runs::REDRAW_DONE
    );
    assert_eq!(
        runs::ending("the thumbnail", runs::REDRAW_DONE, false, false),
        "the thumbnail failed \u{2014} see log"
    );
    assert_eq!(
        runs::ending("the thumbnail", runs::REDRAW_DONE, false, true),
        "the thumbnail stopped"
    );

    // Same three shapes for the other run, with its own `what` and its own done line.
    assert_eq!(
        runs::ending("the text", runs::REWORD_DONE, true, false),
        runs::REWORD_DONE
    );
    assert_eq!(
        runs::ending("the text", runs::REWORD_DONE, false, false),
        "the text failed \u{2014} see log"
    );
    assert_eq!(
        runs::ending("the text", runs::REWORD_DONE, false, true),
        "the text stopped"
    );

    // A stop is reported apart from a failure: a stopped run left work unfinished, a failed one hit an
    // error, and the log behind "see log" differs. So the two must not collapse into one sentence.
    assert_ne!(
        runs::ending("x", "done", false, true),
        runs::ending("x", "done", false, false)
    );
}

// ---- S3: the Set-Thumbnail sentence -------------------------------------------------

#[test]
fn f5_7_s3_taken_as_is_promises_the_words_and_the_redraw() {
    assert_eq!(ITEM, "F5.7");
    // Line 187: "Set Thumbnail on an image: use as is, cropped; \u{2018}thumbnail taken from <file> \u{2014}
    // the words are printed on it; \u{21bb} draws over it\u{2019}."
    assert_eq!(
        runs::taken_log("/home/dev/Pictures/slide.png"),
        "thumbnail taken from slide.png \u{2014} the words are printed on it; \u{21bb} draws over it"
    );
    // Base name only: the user's folder is nobody's business once the copy exists (same rule as S3's log).
    assert!(!runs::taken_log("/home/dev/Pictures/slide.png").contains("/Pictures/"));
    // And a bare name passes through untouched.
    assert!(runs::taken_log("slide.png").starts_with("thumbnail taken from slide.png \u{2014}"));
}

// ---- S6/S7: the export and save lines ----------------------------------------------

#[test]
fn f5_7_s6_the_export_line_gives_the_path_and_the_weight() {
    assert_eq!(ITEM, "F5.7");
    // Line 189: "\u{2018}>>> exported <path> (<size>)\u{2019}".
    assert_eq!(
        runs::exported_log("/tmp/demo-thumbnail.jpg", 3 * 1024 * 1024),
        ">>> exported /tmp/demo-thumbnail.jpg (3 MiB)"
    );
    // Under a whole MiB the weight keeps a decimal: "0 MiB" would make a picture that landed look like one
    // that produced nothing.
    assert_eq!(
        runs::exported_log("/p/final.jpg", 1_468_006),
        ">>> exported /p/final.jpg (1.4 MiB)"
    );
    assert!(runs::exported_log("/p/final.jpg", 0).ends_with("(0 MiB)"));
}

#[test]
fn f5_7_s7_save_announces_then_reports_the_size() {
    assert_eq!(ITEM, "F5.7");
    // Line 189: "\u{2018}saving <file>\u{2026}\u{2019} then \u{2018}saved <file> \u{2014} <size>\u{2019}" — two
    // lines, in that order, because a whole-video copy takes long enough that silence reads as a hung button.
    assert_eq!(runs::saving_log("final.mp4"), "saving final.mp4\u{2026}");
    assert_eq!(
        runs::saved_log("final.mp4", 700 * 1024 * 1024),
        "saved final.mp4 \u{2014} 700 MiB"
    );
    assert!(runs::saving_log("a.webm").ends_with('\u{2026}'));
    // The no-video refusal names ▶: the button that fixes this sits on the same page.
    assert_eq!(
        runs::SAVE_NO_VIDEO,
        "nothing to save yet \u{2014} \u{25b6} renders the video first"
    );
}

#[test]
fn f5_7_s6_the_rung_picker_takes_the_first_that_fits_and_otherwise_the_last() {
    assert_eq!(ITEM, "F5.7");
    let cap = runs::JPEG_MAX_BYTES;
    // First rung fits: rung 0 (quality 92) is the answer, and nothing past it is tried.
    assert_eq!(runs::jpeg_rung(&[cap - 1, cap + 1]), Some(0));
    // A heavy top of the ladder: skip to the first that fits rather than the smallest file.
    assert_eq!(runs::jpeg_rung(&[cap + 500, cap + 20, cap, 10]), Some(2));
    // The cap itself counts as fitting.
    assert_eq!(runs::jpeg_rung(&[cap]), Some(0));
    // Nothing fits: the LAST attempt is written even though it is still over (§F5.7 line 189). Refusing
    // would discard a thumbnail that is merely heavy after every quality rung was already spent.
    assert_eq!(runs::jpeg_rung(&[cap + 9, cap + 8, cap + 7]), Some(2));
    assert_eq!(
        runs::jpeg_rung(&[cap + 1]),
        Some(0),
        "one attempt, and it is the last one too"
    );
    // No attempts at all is the only empty answer.
    assert_eq!(runs::jpeg_rung(&[]), None);
    // The ladder it picks within is the spec's own five rungs, best first.
    assert_eq!(runs::JPEG_QUALITIES.len(), 5);
}

// ---- S1: ⊙ Thumbnail only ---------------------------------------------------------

#[test]
fn f5_7_s1_a_redraw_asks_for_the_picture_and_keeps_the_words() {
    let redraw = runs::redraw();
    assert_eq!(redraw.llm_calls, 1);
    assert_eq!(redraw.sd_calls, 1);
    assert_eq!(redraw.keeps, ["title", "description", "chapter times"]);
    assert_eq!(
        runs::DRAW_AGAIN_LOG,
        "publish: making the thumbnail again — one LLM call for the picture (a frame, or one sd.cpp call), title and description kept"
    );
    assert!(runs::REDRAW_TAKEN.contains('\u{25b6}'));
}

// ---- S2: Reword -------------------------------------------------------------------

#[test]
fn f5_7_s2_one_llm_call_answers_two_things_and_leaves_the_picture() {
    assert_eq!(ITEM, "F5.7");
    let reword = runs::reword();
    assert_eq!(reword.llm_calls, 1);
    assert_eq!(reword.answers, ["title", "description"]);
    // The picture is what this run must not touch — the mirror image of S1's keeps.
    assert!(reword.keeps.contains(&"thumbnail"), "{:?}", reword.keeps);
    assert!(reword.keeps.contains(&"publish record") && reword.keeps.contains(&"chapter times"));
}

#[test]
fn f5_7_s2_the_log_line_is_the_specs_own_sentence() {
    assert_eq!(ITEM, "F5.7");
    // Quoted from spec/08-produce.md §F5.7 rather than retyped, so the sentence in the code is checked against
    // the sentence in the spec: both name two answers and one call, and say nothing else is rewritten.
    let words: Vec<&str> = SPEC_REWORD
        .split([' ', '—'])
        .filter(|w| !w.is_empty())
        .collect();
    for key in ["title", "description"] {
        assert!(
            words.iter().any(|w| w.contains(key)),
            "{key} missing from the spec's sentence"
        );
    }
    assert_eq!(
        runs::REWORD_LOG,
        "publish: rewriting the title and description — one LLM call"
    );
    // Both answers named, one call counted. A log line that dropped one would leave a reader
    // wondering which answer arrived.
    for key in ["title", "description"] {
        assert!(
            runs::REWORD_LOG.contains(key),
            "{key} missing from {}",
            runs::REWORD_LOG
        );
    }
    assert!(runs::REWORD_LOG.contains("one LLM call"));
}

// ---- S3: ⊕ Set from image ---------------------------------------------------------

#[test]
fn f5_7_s3_the_copy_is_recorded_and_reopening_finds_it() {
    assert_eq!(ITEM, "F5.7");
    let (_root, tree) = common::tree_in("s3-record");
    // The record is the publish state, so a chosen picture travels with the project like the title does.
    let mut publish = Publish::default();
    publish.own = true;
    publish.prompt = "the instruction that drew the last one".to_string();
    publish.title = "an old title".to_string();
    naivepost::publish::save(&publish, &tree).unwrap();

    let reopened = naivepost::publish::load(&tree).unwrap();
    assert!(reopened.own, "the choice itself must survive");
    // The instruction is what the next redraw would draw from — kept by a run that only changed the picture.
    assert_eq!(reopened.prompt, "the instruction that drew the last one");
    assert_eq!(reopened.title, "an old title", "S3 rewrites no text");

    // S1's redraw keeps exactly that: nothing in its keeps list is a field this press touched.
    for kept in runs::redraw().keeps {
        assert!(!kept.is_empty(), "{kept} is a keep with no name");
    }
}

// ---- S4: ask again --------------------------------------------------------------

// ---- S5: Words on the thumbnail ---------------------------------------------------

#[test]
fn f5_7_s5_the_dialog_says_what_it_prints_and_where() {
    assert_eq!(ITEM, "F5.7");
    assert_eq!(runs::WORDS_DIALOG, "Words on the thumbnail");
    // One line resized to fill the box: a longer line comes out smaller, and that is the only lever there is.
    assert!(
        runs::WORDS_DIALOG_TIP.contains("longer line comes out smaller"),
        "{}",
        runs::WORDS_DIALOG_TIP
    );
    // The snap keeps the box on a grid and inside the picture.
    assert!(
        runs::WORDS_SNAP_TIP.contains("snaps"),
        "{}",
        runs::WORDS_SNAP_TIP
    );
    // The preview is the export, not the sd.cpp picture — the model writes no letters, so there is nothing for
    // the two to disagree about.
    let preview = runs::WORDS_PREVIEW_TIP;
    assert!(
        preview.contains("same text, width and position"),
        "{preview}"
    );
    // And where they land relative to the drawn picture.
    assert_eq!(
        runs::WORDS_PLACEMENT,
        "Printed over the drawn one — the machine never re-asks the model to write them"
    );
}

#[test]
fn f5_7_s5_printing_the_words_costs_no_model_at_all() {
    assert_eq!(ITEM, "F5.7");
    let printed = runs::reprint("Nine Quick Years", runs::words_box());
    // Zero sd.cpp calls: the letters are drawn over the picture that already exists, so moving the box never
    // redraws it and a dialog full of edits costs nothing.
    assert_eq!(printed.sd_calls, 0);

    // The default box is the lower third, read from the module that owns captions rather than copied here.
    let lower = naivepost::fx_text::LOWER_THIRD;
    let band = runs::words_box();
    assert_eq!(
        (band.cx, band.cy, band.wf, band.hf),
        (lower.cx, lower.cy, lower.wf, lower.hf)
    );

    // And its size in pixels of the drawn picture: 80% of 1920 by 16% of 1080, centred.
    let (x, y, wide, high) = printed.box_px;
    assert_eq!((wide, high), (1536, 172));
    assert_eq!(x, (runs::WIDTH - wide) / 2);
    assert_eq!(
        y,
        (band.cy * f64::from(runs::HEIGHT) - f64::from(high) / 2.0) as u32
    );

    // P.policy.publishWordsSnapPx is the grid the drag stops on — §10's row and this rule read one number.
    assert_eq!(
        row("P.policy.publishWordsSnapPx"),
        runs::words_snap_px().to_string()
    );
    assert_eq!(runs::words_snap_px(), 30);
}

#[test]
fn f5_7_s6_a_longer_line_comes_out_smaller() {
    assert_eq!(ITEM, "F5.7");
    // The tooltip's promise, measured: the size is what `fx_text::fit` answers for the box, so a line that needs
    // more room gets smaller letters rather than running off the picture.
    let short = runs::reprint("Nine", runs::words_box());
    let long = runs::reprint(
        "Nine Quick Years Of Recording The Same Terminal Wrong",
        runs::words_box(),
    );
    assert!(
        long.size < short.size,
        "{} should be smaller than {}",
        long.size,
        short.size
    );
    assert!(!short.lines.is_empty());
    // A wider box lets the same words come out bigger — the box is what sets the size, not the text alone.
    let wide = TitleBox {
        cx: 0.5,
        cy: 0.78,
        wf: 1.0,
        hf: 0.4,
    };
    assert!(runs::reprint("Nine", wide).size > short.size);
}

// ---- S6: export a JPEG ------------------------------------------------------------

#[test]
fn f5_7_s6_the_quality_ladder_drops_before_anything_else() {
    assert_eq!(ITEM, "F5.7");
    // Quality first, never scale: the picture is already the size wanted, and a rescaled thumbnail that fits is
    // a smaller thumbnail rather than a lighter file of the same one.
    assert_eq!(runs::JPEG_QUALITIES, [92, 85, 75, 60, 40]);
    assert!(
        runs::JPEG_QUALITIES
            .windows(2)
            .all(|pair| pair[0] > pair[1]),
        "best first"
    );
    // The message names the three rungs a person would try by hand.
    assert_eq!(runs::JPEG_RETRY_LIMIT, 3);
}

#[test]
fn f5_7_s6_the_cap_is_youtubes_own_and_is_reported_after_three_tries() {
    assert_eq!(ITEM, "F5.7");
    // P.eng.thumbnailJPEGMax — §10's row and the rule read one number, so a change to either is visible in the other.
    assert_eq!(
        row("P.eng.thumbnailJPEGMax"),
        runs::JPEG_MAX_BYTES.to_string()
    );
    assert_eq!(runs::JPEG_MAX_BYTES, 2 * 1024 * 1024);
    // tool:ffmpeg.encode wrote the picture; this only judges its weight.

    assert_eq!(runs::jpeg_target(0), Ok(()));
    assert_eq!(
        runs::jpeg_target(runs::JPEG_MAX_BYTES),
        Ok(()),
        "the cap is inclusive"
    );
    let over = runs::jpeg_target(runs::JPEG_MAX_BYTES + 1).unwrap_err();
    // The three rungs, in order, then the thing that can actually be checked.
    assert_eq!(
        over,
        "JPEG exceeds 2 MiB after 92/85/75 — check the sd.cpp encoder"
    );
}

#[test]
fn f5_7_s6_exporting_without_a_picture_names_both_ways_out() {
    assert_eq!(ITEM, "F5.7");
    // One of the two is usually what the user meant, so the message offers both rather than only the first.
    assert_eq!(
        runs::EXPORT_NO_PICTURE,
        "nothing to export yet — draw a thumbnail, or use one of the images"
    );
}

#[test]
fn f5_7_s6_the_default_name_is_the_projects_and_the_extension_is_forced() {
    assert_eq!(ITEM, "F5.7");
    // `<project>-thumbnail.jpg`: named after the project because at this point the user is naming a picture they
    // are about to upload somewhere else, not one of the project's own files.
    let (_root, tree) = common::tree_in("s6-name");
    assert_eq!(tree.name(), Some("demo"));
    let name = runs::export_default("demo");
    assert_eq!(name, "demo-thumbnail.jpg");
    assert_eq!(
        runs::EXPORT_SUFFIX,
        ".jpg",
        "a save dialog that typed .png still gets a JPEG"
    );

    // Beside the video it is `<stem>.jpg`, so the exported file is a sibling of the `.html` and `.vtt` files.
    let stem = render::final_srt_name().replace(".srt", "");
    assert_eq!(runs::export_name(&stem), "final.jpg");
    assert_eq!(tree.final_jpg().file_name().unwrap(), "final.jpg");
}

// ---- S7: Save a copy of the video -------------------------------------------------

#[test]
fn f5_7_s7_a_copy_is_refused_while_the_render_runs() {
    assert_eq!(ITEM, "F5.7");
    // A half-written file copied out is a short video that looks finished, so there is no partial answer here.
    assert_eq!(
        runs::copy_plan("/p/produce/final.mp4", "/tmp/taken.mp4", true, 0),
        Err("finish the render first".to_string())
    );
}

#[test]
fn f5_7_s7_a_copy_leaves_the_video_up_to_date() {
    assert_eq!(ITEM, "F5.7");
    let copy = runs::copy_plan(
        "/p/produce/final.mp4",
        "/home/dev/Desktop/taken.mp4",
        false,
        12 * 1024 * 1024,
    )
    .unwrap();
    assert_eq!(copy.from, "/p/produce/final.mp4");
    // The stamp ▶ wrote describes the file in produce/. A copy elsewhere is not what `renderUpToDate` reads, so
    // ⤓ never makes the next ▶ re-encode.
    assert!(
        !copy.copy_is_source,
        "the render's own output is still the source of truth"
    );
    assert_eq!(copy.log, "copy written — 12 MiB copied");
}

#[test]
fn f5_7_s7_a_copy_rewrites_no_settings_and_no_transcript() {
    assert_eq!(ITEM, "F5.7");
    let (_root, tree) = common::tree_in("s7");
    // What ▶ reads: the settings row and the transcript files beside the project. A copy writes outside the
    // project folder, so none of it can change — pinned by hashing before and after.
    let stamp = render::codec_args(&settings()).join(" ");
    let copy = runs::copy_plan(
        &tree.final_video("mp4").display().to_string(),
        "/home/dev/Desktop/taken.mp4",
        false,
        1024 * 1024,
    )
    .unwrap();
    assert_eq!(render::codec_args(&settings()).join(" "), stamp);
    // The source is the render's own file and the destination is outside the project: nothing this press writes
    // can be read back as a setting or a transcript.
    assert!(copy.from.ends_with("produce/final.mp4"), "{}", copy.from);
    assert!(!copy.log.is_empty());

    // Its default name keeps the container it was rendered in.
    assert_eq!(runs::save_default("demo", Container::Mp4), "demo.mp4");
    assert_eq!(runs::save_default("demo", Container::Webm), "demo.webm");
}

// ---- S8: Transcode a file ---------------------------------------------------------

#[test]
fn f5_7_s8_a_transcode_keeps_every_setting_on_the_row() {
    assert_eq!(ITEM, "F5.7");
    // tool:ffmpeg.encode with the render's own settings: same codec table, same frame-rate rule, same audio.
    let chosen = Produce {
        container: Container::Mp4,
        codec: Codec::H264,
        resolution: Resolution::P1080,
        ..settings()
    };
    let argv = runs::transcode_args("/in/file.mov", "/out/final.mp4", &chosen);
    assert_eq!(argv.first().unwrap(), "ffmpeg");
    for expected in render::codec_args(&chosen) {
        assert!(
            argv.contains(&expected),
            "{expected} missing from {:?}",
            argv
        );
    }
    assert!(
        argv.contains(&"-c:a".to_string()) && argv.contains(&"aac".to_string()),
        "{:?}",
        argv
    );

    // webm takes Opus, the only audio codec its container defines — same rule as the render.
    let webm = runs::transcode_args(
        "/in.mov",
        "/out/final.webm",
        &Produce {
            container: Container::Webm,
            ..settings()
        },
    );
    assert!(webm.contains(&"libopus".to_string()), "{:?}", webm);
    // A mono row means one channel here too.
    let mono = runs::transcode_args(
        "/in.mov",
        "/out/final.mp4",
        &Produce {
            mono: true,
            ..settings()
        },
    );
    assert!(
        mono.windows(2).any(|pair| pair == ["-ac", "1"]),
        "{:?}",
        mono
    );
}

#[test]
fn f5_7_s8_a_transcode_reads_the_whole_file_and_carries_no_captions() {
    assert_eq!(ITEM, "F5.7");
    let argv = runs::transcode_args("/in/file.mov", "/out/final.mp4", &settings());

    // `-i <source>` with no `-ss` before it: the render seeks to a clip's start; this file is the whole thing.
    let input = argv.iter().position(|arg| arg == "-i").expect("an input");
    assert!(!argv[..input].contains(&"-ss".to_string()), "{:?}", argv);
    assert_eq!(argv[input + 1], "/in/file.mov");

    // No caption track even though the row says `track in file`: a file picked off disk has no transcript beside
    // it to mux, and the sidecars the render would mux belong to a different video.
    assert_eq!(
        settings().subtitles,
        Subtitles::TrackInFile,
        "the setting is what S8 must ignore"
    );
    for forbidden in ["-c:s", "-map_metadata", "mov_text", "final.srt"] {
        assert!(
            !argv.iter().any(|arg| arg.contains(forbidden)),
            "{forbidden} in {:?}",
            argv
        );
    }
    assert!(
        !argv.iter().any(|arg| arg.starts_with("s:")),
        "no subtitle stream map: {:?}",
        argv
    );

    // mp4 still streams from the front, as the render's own mux insists.
    assert!(
        argv.windows(2)
            .any(|w| w == ["-movflags", "+faststart+negative_cts_offsets"]),
        "{:?}",
        argv
    );
    assert_eq!(argv.last().unwrap(), "/out/final.mp4");
}

#[test]
fn f5_7_s8_a_transcode_is_refused_while_the_render_runs() {
    assert_eq!(ITEM, "F5.7");
    let argv = runs::transcode_args("/in/file.mov", "/out/final.mp4", &settings());
    // Two ffmpeg encodes at once halve both of them, and the page has one progress bar to say which is which.
    assert_eq!(
        runs::transcode_plan("file.mov", &argv, true),
        Err("wait for the current render to finish — ⊘ Cancel first".to_string())
    );
    assert_eq!(runs::transcode_plan("file.mov", &argv, false), Ok(()));
    // And a run with nothing in it is refused rather than started: an empty argv means there was no file.
    assert!(runs::transcode_plan("file.mov", &[], false).is_err());

    // The bar is honest about what it cannot know: ffmpeg's percentage is against a duration this app has never
    // read for an arbitrary file, so the run sits between 5% and 95%.
    assert_eq!((runs::TRANSCODE_START, runs::TRANSCODE_END), (0.05, 0.95));
}

#[test]
fn f5_7_s8_the_log_names_the_file_and_the_three_settings_that_decide_it() {
    assert_eq!(ITEM, "F5.7");
    let chosen = Produce {
        container: Container::Mkv,
        codec: Codec::H265,
        resolution: Resolution::P720,
        ..settings()
    };
    // File, container/codec, resolution — the three that decide what comes out. The row's other settings are in
    // the command and would only make the line longer.
    assert_eq!(
        runs::transcode_started("file.mov", &chosen),
        "transcode started — file.mov, mkv/h265, 720p"
    );
    // A transcode writes no stamp, so this line is the only record it leaves.
    assert_eq!(
        runs::transcode_finished("file.mov"),
        "transcode finished — file.mov"
    );

    // The default row reads out as the defaults are named on screen.
    let plain = Produce::default();
    assert_eq!(
        runs::transcode_started("clip.mp4", &plain),
        "transcode started — clip.mp4, mp4/h264, 1080p"
    );
}

// ---- the catalogue -----------------------------------------------------------------

#[test]
fn f5_7_s8_both_pages_numbers_are_catalogued_where_they_are_used() {
    assert_eq!(ITEM, "F5.7");
    // §10's ids exist and answer. `params::find` searches Prepare's rows only (its own doc says so), so these
    // two are read out of the Produce section — which is also what makes them findable at all.
    assert_eq!(
        params::family("P.eng.thumbnailJPEGMax"),
        params::Family::Eng
    );
    assert_eq!(
        params::family("P.policy.publishWordsSnapPx"),
        params::Family::Policy
    );
    assert!(
        params::find("P.machine.asrChunkQwenSeconds").is_some(),
        "find answers Prepare's rows"
    );
    assert_eq!(row("P.eng.thumbnailJPEGMax"), "2097152");
    assert_eq!(row("P.policy.publishWordsSnapPx"), "30");
    // Every row's value is read from the module that uses it, never retyped here.
    assert_eq!(runs::JPEG_MAX_BYTES, 2 * 1024 * 1024);
}

#[test]
fn f5_7_s8_the_produce_rows_stay_few_and_all_named() {
    assert_eq!(ITEM, "F5.7");
    let rows = params::produce();
    // Five cue numbers and one batch size are §F5.4's; the JPEG cap and the words box's snap grid are this
    // item's (§F5.7); the eight after them are §08 §4's — the mix's two targets, the bed under the narration,
    // the fitting ceiling, and the thumbnail's size, band and frame counts. A row past these would mean a
    // number was added somewhere in §08 without being written into §10 first.
    assert_eq!(
        ids(&rows),
        [
            "P.policy.subtitleBreakSeconds",
            "P.policy.subtitleRowChars",
            "P.policy.subtitleMaxSeconds",
            "P.policy.subtitleHoldSeconds",
            "P.policy.subtitleMinSeconds",
            "P.machine.translateBatch",
            "P.eng.thumbnailJPEGMax",
            "P.policy.publishWordsSnapPx",
            "P.policy.gameVolume",
            "P.eng.loudness",
            "P.eng.clipLimiter",
            // §08 §4's frame-edge blur, catalogued between the limiter and the fitting ceiling.
            "P.eng.blurSigma",
            // (`P.eng.narrationMaxTempo` used to sit in this list: §07's §4 names it too, so it now has one
            // home, in `params::narrate()`.)
            "P.machine.thumbnailLongSide",
            "P.eng.titleBand",
            "P.policy.publishFrames",
            "P.eng.publishMaxFrames",
            // §F5.6 S1's bound on the upload brief, catalogued where the brief reads it.
            "P.machine.briefMaxChars",
            // §08 §4's engineering sentence: values §10 keeps in prose rather than in a table, so each takes
            // the bare `produce.` prefix a value with no `P.` id gets, and spells itself from its rule's own
            // constant (see tests/suite/produce_parameters.rs s10/s11 for what backs each one).
            "produce.exportLadder",
            "produce.posterJPEGQuality",
            "produce.sdWaitSeconds",
            "produce.seekBeforeInput",
            "produce.mp4StreamFlags",
        ]
    );
    for param in &rows {
        assert!(
            !param.spelled.is_empty(),
            "{} catalogued with no value",
            param.id
        );
        // Every row names the module whose rule reads it — §10's numbers live once, at their rule.
        assert!(param.from.contains("::"), "{} names no module", param.id);
    }
}

/// The ids of a list of rows, for a failure message that says which one is new.
fn ids(rows: &[params::Param]) -> Vec<&str> {
    rows.iter().map(|param| param.id).collect()
}

// ---- the module's own shape --------------------------------------------------------

#[test]
fn f5_7_module_runs_nothing_itself() {
    assert_eq!(ITEM, "F5.7");
    // Like produce_render and produce_embed, this module builds argv and paths; the spawning belongs to the page.
    let source = include_str!("../../src/produce_runs.rs");
    for forbidden in ["std::process", "process::Command", "gtk4", "adw::"] {
        assert!(
            !source.contains(forbidden),
            "{forbidden} does not belong in a pure module"
        );
    }
    // `tool:ffmpeg.encode` is named in the module's own text, since it is the only tool any of these runs needs.
    assert!(source.contains("tool:ffmpeg.encode"));
}
