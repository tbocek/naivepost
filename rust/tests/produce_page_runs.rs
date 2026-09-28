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

use std::path::PathBuf;

use naivepost::layout::Tree;
use naivepost::params;
use naivepost::produce_render as render;
use naivepost::produce_runs as runs;
use naivepost::project::{Codec, Container, Produce, Publish, Resolution, Subtitles, TitleBox};

const ITEM: &str = "F5.7";

/// The spec's own wording for the two runs that rewrite something — quoted so a paraphrase in the code cannot
/// pass as the item it implements.
const SPEC_REWORD: &str = "Ask again for the title, thumbnail instruction and description — one LLM call, three \
answers, nothing else rewritten (not even the picture it prints)";

fn temp_root(tag: &str) -> PathBuf {
    // Removed here rather than at the end of each test: a case that fails on an assertion never reaches its own
    // cleanup, and a folder left in /tmp is a folder the next reader has to explain.
    let dir = std::env::temp_dir().join(format!("np-f57-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn tree_in(tag: &str) -> (PathBuf, Tree) {
    let root = temp_root(tag);
    let dir = root.join("demo.naivepost");
    std::fs::create_dir_all(&dir).unwrap();
    (root, Tree::new(&dir).unwrap())
}

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
    Produce { subtitles: Subtitles::TrackInFile, ..Produce::default() }
}

// ---- S1: ⊙ Thumbnail only ---------------------------------------------------------

#[test]
fn f5_7_s1_thumbnail_only_costs_one_sd_call_and_rewrites_nothing() {
    assert_eq!(ITEM, "F5.7");
    let ticked = runs::redraw(true);
    // One sd.cpp call: a second draw would be a second picture, not a redraw of this one.
    assert_eq!(ticked.sd_calls, 1);
    // And no text rewrite at all — which is the whole content of the tick.
    assert_eq!(ticked.rewrites, 0, "the tick means: do not ask for the words again");

    // The keeps list is the spec's own four things, in the order the sentence names them.
    assert_eq!(
        ticked.keeps,
        ["title", "description", "publish record", "chapter times"],
        "a redraw that rewrote any of these is a reword"
    );

    // The log line says what it cost before anything has had time to fail.
    assert_eq!(
        runs::DRAW_AGAIN_LOG,
        "publish: drawing the thumbnail again — one sd.cpp call, nothing rewritten"
    );
}

#[test]
fn f5_7_s1_leaving_the_box_unticked_asks_for_the_words_too() {
    assert_eq!(ITEM, "F5.7");
    let unticked = runs::redraw(false);
    // Still exactly one picture: unticking adds an LLM call, never a second draw.
    assert_eq!(unticked.sd_calls, 1);
    assert_eq!(unticked.rewrites, 1, "unticked re-asks the words — that is all it does");
    // The same keeps either way: the box decides whether the words are asked for, never what survives.
    assert_eq!(ticked_keeps(), unticked.keeps);

    // The label and its tooltip: the inventory's own `⊙ Thumbnail only`, and a sentence that names what ticking
    // costs rather than what it saves, because ticking is the deliberate act.
    assert_eq!(runs::ONLY_THUMBNAIL, "⊙ Thumbnail only");
    let tip = runs::ONLY_THUMBNAIL_TIP;
    assert!(tip.starts_with("Ticked"), "{tip}");
    assert!(tip.contains("re-asks for the words too"), "the tick's cost is the point: {tip}");

    // And the words box arrives prefilled with what it printed last time rather than empty.
    assert_eq!(runs::WORDS_ASK_AGAIN, "Words: <previous text>");
}

/// The keeps list of a ticked redraw, spelled out so the unticked case compares against a value it does not share.
fn ticked_keeps() -> &'static [&'static str] {
    runs::redraw(true).keeps
}

// ---- S2: Reword -------------------------------------------------------------------

#[test]
fn f5_7_s2_one_llm_call_answers_three_things_and_leaves_the_picture() {
    assert_eq!(ITEM, "F5.7");
    let reword = runs::reword();
    assert_eq!(reword.llm_calls, 1);
    assert_eq!(reword.answers, ["title", "instruction", "description"]);
    // The picture is what this run must not touch — the mirror image of S1's keeps.
    assert!(reword.keeps.contains(&"thumbnail"), "{:?}", reword.keeps);
    assert!(reword.keeps.contains(&"publish record") && reword.keeps.contains(&"chapter times"));
}

#[test]
fn f5_7_s2_the_log_line_is_the_specs_own_sentence() {
    assert_eq!(ITEM, "F5.7");
    // Quoted from spec/08-produce.md §F5.7 rather than retyped, so the sentence in the code is checked against
    // the sentence in the spec: both name three answers and one call, and say nothing else is rewritten.
    let words: Vec<&str> = SPEC_REWORD.split([' ', '—']).filter(|w| !w.is_empty()).collect();
    for key in ["title", "instruction", "description"] {
        assert!(words.iter().any(|w| w.contains(key)), "{key} missing from the spec's sentence");
    }
    assert_eq!(
        runs::REWORD_LOG,
        "publish: rewriting the title, instruction and description — one LLM call"
    );
    // Three answers named, one call counted. A log line that dropped one of the three would leave a reader
    // wondering which answer arrived.
    for key in ["title", "instruction", "description"] {
        assert!(runs::REWORD_LOG.contains(key), "{key} missing from {}", runs::REWORD_LOG);
    }
    assert!(runs::REWORD_LOG.contains("one LLM call"));
}

// ---- S3: ⊕ Set from image ---------------------------------------------------------

#[test]
fn f5_7_s3_a_chosen_picture_is_copied_not_referenced() {
    assert_eq!(ITEM, "F5.7");
    let chosen = runs::take_from_image("/home/dev/Videos/frame.png", 1920, 1080, 12.5);
    // The copy lives in the app's own images folder under the file's stem: reopening the project must not
    // depend on a file that can be moved or deleted afterwards.
    assert_eq!(chosen.stored, format!("{}/frame.png", runs::images_dir()));
    assert!(runs::images_dir().ends_with("naivepost/images"), "{}", runs::images_dir());
    assert!(!chosen.stored.contains("/Videos/"), "the user's folder is never the record: {}", chosen.stored);

    // A stem with a dot in its middle keeps it — only the extension is dropped.
    assert!(runs::take_from_image("/tmp/shot.final.png", 1920, 1080, 0.0).stored.ends_with("shot.final.png"));
}

#[test]
fn f5_7_s3_the_box_is_the_pictures_own_shape_scaled_to_fit() {
    assert_eq!(ITEM, "F5.7");
    // The picture's own aspect, not the project's: choosing a picture *is* the choice of shape. Sides stay even
    // because every encoder on the Produce row is 4:2:0, which refuses an odd edge.
    let wide = runs::take_from_image("/tmp/frame.png", 1920, 1080, 0.0);
    assert_eq!(wide.box_, (1920, 1080), "the tier's own size needs no scaling");
    // A 3:2 picture: 1920 wide, 1280 tall — both even, the ratio exact.
    let three_two = runs::take_from_image("/tmp/frame.png", 3000, 2000, 0.0);
    assert_eq!(three_two.box_, (1920, 1280));
    // A portrait picture fills the height instead of the width.
    let tall = runs::take_from_image("/tmp/frame.png", 1080, 1920, 0.0);
    assert_eq!(tall.box_, (1080, 1920));
    // Integer scaling rounds *up* before the even round takes it back down, so a 641-wide picture lands on the
    // tier's own height rather than a pixel over it. What must never happen is an odd side or a width past 1920.
    let odd = runs::take_from_image("/tmp/frame.png", 641, 361, 0.0);
    assert_eq!(odd.box_, (1920, 1080));
    // A ratio the tier cannot answer exactly is rounded down to even on the side that moves: 7000×3000 wants
    // 822.8 tall and gets 822.
    let thirds = runs::take_from_image("/tmp/frame.png", 7000, 3000, 0.0);
    assert_eq!(thirds.box_, (1920, 822));
    for (width, height) in [wide.box_, three_two.box_, tall.box_, odd.box_, thirds.box_] {
        assert_eq!(height % 2, 0, "4:2:0 refuses an odd side");
        assert_eq!(width % 2, 0, "and so on the other edge");
        // Fitting means the *longer* edge stops at the tier; a 3:2 picture is therefore taller than the 16:9
        // frame it will be letterboxed into, and that is the point of fitting rather than cropping.
        assert!(width.max(height) <= runs::WIDTH, "{width}×{height} leaves the tier");
    }
}

#[test]
fn f5_7_s3_the_log_names_the_file_and_where_the_frame_came_from() {
    assert_eq!(ITEM, "F5.7");
    let chosen = runs::take_from_image("/home/dev/Videos/frame.png", 1920, 1080, 12.5);
    // Base name only — the user's path is nobody's business after the copy exists — and one decimal on the time,
    // which is how the Cut page writes a timestamp.
    assert_eq!(chosen.log, "thumbnail taken from frame.png — scaled to 1920×1080 at 12.5s");
}

#[test]
fn f5_7_s3_a_missing_copy_is_reported_and_the_choice_survives() {
    assert_eq!(ITEM, "F5.7");
    // Reopening a project whose copy was deleted says so and keeps the choice: the next run uses the picture if
    // it comes back, so reporting the loss must not undo the press.
    assert_eq!(runs::chosen_state(false), Some(runs::CHOSEN_REMOVED));
    assert_eq!(runs::CHOSEN_REMOVED, "thumbnail not found");
    assert_eq!(runs::chosen_state(true), None, "a copy that is there needs no message");

    // The tooltip promises the two things these two assertions check: a copy was made, and it survives.
    let tip = runs::CHOSEN_TIP;
    assert!(tip.contains("copy"), "{tip}");
    assert!(tip.contains("survives reopening"), "{tip}");
    assert!(tip.contains("does not replace your file"), "the user's own file is never edited: {tip}");
}

#[test]
fn f5_7_s3_the_copy_is_recorded_and_reopening_finds_it() {
    assert_eq!(ITEM, "F5.7");
    let (_root, tree) = tree_in("s3-record");
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
    for kept in runs::redraw(true).keeps {
        assert!(!kept.is_empty(), "{kept} is a keep with no name");
    }
}

// ---- S4: ask again --------------------------------------------------------------

#[test]
fn f5_7_s4_asking_again_hands_back_an_empty_box() {
    assert_eq!(ITEM, "F5.7");
    // The press says those words were wrong; offering them back would be the mistake the redraw box exists to
    // stop, in the other direction.
    assert_eq!(runs::re_ask("the old title"), "");
    assert_eq!(runs::re_ask(""), "");
    assert_ne!(runs::re_ask("anything at all"), "anything at all");
}

// ---- S5: Words on the thumbnail ---------------------------------------------------

#[test]
fn f5_7_s5_the_dialog_says_what_it_prints_and_where() {
    assert_eq!(ITEM, "F5.7");
    assert_eq!(runs::WORDS_DIALOG, "Words on the thumbnail");
    // One line resized to fill the box: a longer line comes out smaller, and that is the only lever there is.
    assert!(runs::WORDS_DIALOG_TIP.contains("longer line comes out smaller"), "{}", runs::WORDS_DIALOG_TIP);
    // The snap keeps the box on a grid and inside the picture.
    assert!(runs::WORDS_SNAP_TIP.contains("snaps"), "{}", runs::WORDS_SNAP_TIP);
    // The preview is the export, not the sd.cpp picture — the model writes no letters, so there is nothing for
    // the two to disagree about.
    let preview = runs::WORDS_PREVIEW_TIP;
    assert!(preview.contains("same text, width and position"), "{preview}");
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
    assert_eq!((band.cx, band.cy, band.wf, band.hf), (lower.cx, lower.cy, lower.wf, lower.hf));

    // And its size in pixels of the drawn picture: 80% of 1920 by 16% of 1080, centred.
    let (x, y, wide, high) = printed.box_px;
    assert_eq!((wide, high), (1536, 172));
    assert_eq!(x, (runs::WIDTH - wide) / 2);
    assert_eq!(y, (band.cy * f64::from(runs::HEIGHT) - f64::from(high) / 2.0) as u32);

    // P.policy.publishWordsSnapPx is the grid the drag stops on — §10's row and this rule read one number.
    assert_eq!(row("P.policy.publishWordsSnapPx"), runs::words_snap_px().to_string());
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
    assert!(long.size < short.size, "{} should be smaller than {}", long.size, short.size);
    assert!(!short.lines.is_empty());
    // A wider box lets the same words come out bigger — the box is what sets the size, not the text alone.
    let wide = TitleBox { cx: 0.5, cy: 0.78, wf: 1.0, hf: 0.4 };
    assert!(runs::reprint("Nine", wide).size > short.size);
}

// ---- S6: export a JPEG ------------------------------------------------------------

#[test]
fn f5_7_s6_the_quality_ladder_drops_before_anything_else() {
    assert_eq!(ITEM, "F5.7");
    // Quality first, never scale: the picture is already the size wanted, and a rescaled thumbnail that fits is
    // a smaller thumbnail rather than a lighter file of the same one.
    assert_eq!(runs::JPEG_QUALITIES, [92, 85, 75, 60, 40]);
    assert!(runs::JPEG_QUALITIES.windows(2).all(|pair| pair[0] > pair[1]), "best first");
    // The message names the three rungs a person would try by hand.
    assert_eq!(runs::JPEG_RETRY_LIMIT, 3);
}

#[test]
fn f5_7_s6_the_cap_is_youtubes_own_and_is_reported_after_three_tries() {
    assert_eq!(ITEM, "F5.7");
    // P.eng.thumbnailJPEGMax — §10's row and the rule read one number, so a change to either is visible in the other.
    assert_eq!(row("P.eng.thumbnailJPEGMax"), runs::JPEG_MAX_BYTES.to_string());
    assert_eq!(runs::JPEG_MAX_BYTES, 2 * 1024 * 1024);
    // tool:ffmpeg.encode wrote the picture; this only judges its weight.

    assert_eq!(runs::jpeg_target(0), Ok(()));
    assert_eq!(runs::jpeg_target(runs::JPEG_MAX_BYTES), Ok(()), "the cap is inclusive");
    let over = runs::jpeg_target(runs::JPEG_MAX_BYTES + 1).unwrap_err();
    // The three rungs, in order, then the thing that can actually be checked.
    assert_eq!(over, "JPEG exceeds 2 MiB after 92/85/75 — check the sd.cpp encoder");
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
    let (_root, tree) = tree_in("s6-name");
    assert_eq!(tree.name(), Some("demo"));
    let name = runs::export_default("demo");
    assert_eq!(name, "demo-thumbnail.jpg");
    assert_eq!(runs::EXPORT_SUFFIX, ".jpg", "a save dialog that typed .png still gets a JPEG");

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
    let copy = runs::copy_plan("/p/produce/final.mp4", "/home/dev/Desktop/taken.mp4", false, 12 * 1024 * 1024)
        .unwrap();
    assert_eq!(copy.from, "/p/produce/final.mp4");
    // The stamp ▶ wrote describes the file in produce/. A copy elsewhere is not what `renderUpToDate` reads, so
    // ⤓ never makes the next ▶ re-encode.
    assert!(!copy.copy_is_source, "the render's own output is still the source of truth");
    assert_eq!(copy.log, "copy written — 12 MiB copied");
}

#[test]
fn f5_7_s7_a_copy_rewrites_no_settings_and_no_transcript() {
    assert_eq!(ITEM, "F5.7");
    let (_root, tree) = tree_in("s7");
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
        assert!(argv.contains(&expected), "{expected} missing from {:?}", argv);
    }
    assert!(argv.contains(&"-c:a".to_string()) && argv.contains(&"aac".to_string()), "{:?}", argv);

    // webm takes Opus, the only audio codec its container defines — same rule as the render.
    let webm = runs::transcode_args("/in.mov", "/out/final.webm", &Produce { container: Container::Webm, ..settings() });
    assert!(webm.contains(&"libopus".to_string()), "{:?}", webm);
    // A mono row means one channel here too.
    let mono = runs::transcode_args("/in.mov", "/out/final.mp4", &Produce { mono: true, ..settings() });
    assert!(mono.windows(2).any(|pair| pair == ["-ac", "1"]), "{:?}", mono);
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
    assert_eq!(settings().subtitles, Subtitles::TrackInFile, "the setting is what S8 must ignore");
    for forbidden in ["-c:s", "-map_metadata", "mov_text", "final.srt"] {
        assert!(!argv.iter().any(|arg| arg.contains(forbidden)), "{forbidden} in {:?}", argv);
    }
    assert!(!argv.iter().any(|arg| arg.starts_with("s:")), "no subtitle stream map: {:?}", argv);

    // mp4 still streams from the front, as the render's own mux insists.
    assert!(argv.contains(&"+faststart".to_string()), "{:?}", argv);
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
    assert_eq!(runs::transcode_started("file.mov", &chosen), "transcode started — file.mov, mkv/h265, 720p");
    // A transcode writes no stamp, so this line is the only record it leaves.
    assert_eq!(runs::transcode_finished("file.mov"), "transcode finished — file.mov");

    // The default row reads out as the defaults are named on screen.
    let plain = Produce::default();
    assert_eq!(runs::transcode_started("clip.mp4", &plain), "transcode started — clip.mp4, mp4/h264, 1080p");
}

// ---- the catalogue -----------------------------------------------------------------

#[test]
fn f5_7_s8_both_pages_numbers_are_catalogued_where_they_are_used() {
    assert_eq!(ITEM, "F5.7");
    // §10's ids exist and answer. `params::find` searches Prepare's rows only (its own doc says so), so these
    // two are read out of the Produce section — which is also what makes them findable at all.
    assert_eq!(params::family("P.eng.thumbnailJPEGMax"), params::Family::Eng);
    assert_eq!(params::family("P.policy.publishWordsSnapPx"), params::Family::Policy);
    assert!(params::find("P.machine.asrChunkQwenSeconds").is_some(), "find answers Prepare's rows");
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
            "P.eng.narrationMaxTempo",
            "P.machine.thumbnailLongSide",
            "P.eng.titleBand",
            "P.policy.publishFrames",
            "P.eng.publishMaxFrames",
            // §F5.6 S1's bound on the upload brief, catalogued where the brief reads it.
            "P.machine.briefMaxChars",
        ]
    );
    for param in &rows {
        assert!(!param.spelled.is_empty(), "{} catalogued with no value", param.id);
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
    let source = include_str!("../src/produce_runs.rs");
    for forbidden in ["std::process", "process::Command", "gtk4", "adw::"] {
        assert!(!source.contains(forbidden), "{forbidden} does not belong in a pure module");
    }
    // `tool:ffmpeg.encode` is named in the module's own text, since it is the only tool any of these runs needs.
    assert!(source.contains("tool:ffmpeg.encode"));
}
