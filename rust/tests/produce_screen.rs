//! §08-produce#1-screen — 1. Screen (spec/08-produce.md §1, options/tooltips normative in
//! spec/inventory/produce.md §A), checked against `naivepost::produce_screen`.
//!
//! One test per clause of §1: the encoder settings' rows, options, defaults and tooltips; the two forcing
//! rules that make webm a container of its own; the images row's eight slots and their buttons; the
//! thumbnail's text-overlay editor; the title printed on the thumbnail once only; the Inputs and Outputs
//! readouts.
//!
//! Ids cited: `P.policy.gameVolume` (0.22, the Game audio row) and `tool:ffmpeg.encode` — the presets, CRF
//! range and audio bitrates are ffmpeg's own numbers, which is why this page offers a list rather than a
//! measurement.

use naivepost::layout::Tree;
use naivepost::produce_screen as ps;
use naivepost::project::{self, Codec, Container, Publish, Subtitles, TextMark};
use naivepost::publish;

const ITEM: &str = "sec_08_produce_1_screen";

fn frames(names: &[&str]) -> Vec<String> {
    names.iter().map(|n| format!("project:produce/publish/{n}.jpg")).collect()
}

// ---- s1: the settings scroller's rows and their option lists ----------------------

#[test]
fn sec_08_produce_1_screen_s1_settings_rows_and_options() {
    assert_eq!(ITEM, "sec_08_produce_1_screen");
    // §A's table order is the page's order; 13 rows, nothing else.
    assert_eq!(
        ps::SETTINGS_ROWS,
        [
            "Container",
            "Codec",
            "Preset",
            "Resolution",
            "Frame rate",
            "Audio",
            "Subtitles",
            "Translate",
            "Game audio",
            "Quality (CRF)",
            "Frame timing",
            "Channels",
            "Frame edges"
        ]
    );

    // tool:ffmpeg.encode — every one of these lists is ffmpeg's, not a measurement this app makes.
    assert_eq!(ps::options("Container").unwrap(), &["mp4", "mkv", "webm"]);
    assert_eq!(ps::options("Codec").unwrap(), &["h264", "h265", "vp9"]);
    assert_eq!(
        ps::options("Preset").unwrap(),
        &["ultrafast", "veryfast", "fast", "medium", "slow", "veryslow"]
    );
    assert_eq!(ps::options("Resolution").unwrap(), &["720p", "1080p", "original"]);
    assert_eq!(ps::options("Frame rate").unwrap(), &["source", "60", "30", "24"]);
    assert_eq!(ps::options("Audio").unwrap(), &["128", "192", "256", "320"]);
    assert_eq!(
        ps::options("Subtitles").unwrap(),
        &["burned in", "track in file", "none in the video"]
    );
    assert_eq!(
        ps::options("Translate").unwrap(),
        &["English", "German", "French"]
    );

    // CRF is a range with a mark, not a list: 14..34 and the default marked at 24.
    assert_eq!((ps::CRF_MIN, ps::CRF_MAX, ps::CRF_MARK), (14, 34, 24));
    assert!((ps::CRF_MIN..=ps::CRF_MAX).contains(&ps::CRF_MARK));

    // The three tick rows carry §A's own labels.
    assert_eq!(ps::VFR_TICK, "peak rate (VFR)");
    assert_eq!(ps::MONO_TICK, "mono");
    assert_eq!(ps::EDGES_TICK, "blurred");

    // A row with no list is a slider or a tick — never a guessed one.
    for row in ["Game audio", "Quality (CRF)", "Frame timing", "Channels", "Frame edges"] {
        assert_eq!(ps::options(row), None, "{row} has no option list");
    }
    assert_eq!(ps::options("Whatever"), None);
}

// ---- s2: every row's starting point ---------------------------------------------

#[test]
fn sec_08_produce_1_screen_s2_defaults() {
    assert_eq!(ITEM, "sec_08_produce_1_screen");
    // §A's Default column. The preset is the spec's `slow`; the prototype shipped veryslow (§A notes it).
    let defaults = ps::defaults();
    let of = |row: &str| {
        defaults
            .iter()
            .find(|(name, _)| *name == row)
            .map(|(_, value)| *value)
    };
    assert_eq!(of("Container"), Some("mp4"));
    assert_eq!(of("Codec"), Some("h264"));
    assert_eq!(of("Preset"), Some("slow"));
    assert_eq!(of("Resolution"), Some("1080p"));
    assert_eq!(of("Frame rate"), Some("30"));
    assert_eq!(of("Audio"), Some("128"));
    assert_eq!(of("Subtitles"), Some("none in the video"));
    assert_eq!(of("Translate"), Some(""));
    assert_eq!(of("Game audio"), Some("0.22"));
    assert_eq!(of("Quality (CRF)"), Some("24"));
    assert_eq!(of("Frame timing"), Some("off"));
    assert_eq!(of("Channels"), Some("off"));
    assert_eq!(of("Frame edges"), Some("on"));

    // The same thirteen answers as the stored record's default — one source of truth for both.
    let settings = project::Produce::default();
    assert_eq!(settings.container, Container::Mp4);
    assert_eq!(settings.codec, Codec::H264);
    assert_eq!(settings.preset, project::Preset::Slow);
    assert_eq!(settings.resolution, project::Resolution::P1080);
    assert_eq!(settings.frame_rate, project::FrameRate::F30);
    assert_eq!(settings.audio_kbps, 128);
    assert_eq!(settings.subtitles, Subtitles::None);
    assert!(settings.translate.is_empty());
    assert_eq!(settings.crf, 24);
    assert!(!settings.vfr);
    assert!(!settings.mono);
    assert!(settings.blurred_edges);

    // P.policy.gameVolume: the Game audio row's default is §10's number, not a second copy of it. The row
    // lives in the project record (params::narrate_preview says so), so the value this page shows is read
    // from there rather than catalogued twice.
    assert_eq!(settings.game_volume, 0.22);
    assert_eq!(of("Game audio"), Some("0.22"));
}

// ---- s3: each row's tooltip says why ---------------------------------------------

#[test]
fn sec_08_produce_1_screen_s3_tooltips_name_the_rule() {
    assert_eq!(ITEM, "sec_08_produce_1_screen");
    // Every row has a sentence; none of them restates the row's own label.
    for row in ps::SETTINGS_ROWS {
        assert!(!ps::tooltip(row).is_empty(), "{row} has no tooltip");
        assert!(
            !ps::tooltip(row).eq_ignore_ascii_case(row),
            "{row}'s tooltip only repeats its label"
        );
    }

    // §A's wording, spot-checked exactly where the sentence carries a rule. tool:ffmpeg.encode: the encode
    // rows' tooltips are the reason for ffmpeg's own choices.
    assert!(ps::tooltip("Container").contains("webm forces VP9 + Opus"), "{}", ps::tooltip("Container"));
    assert!(ps::tooltip("Resolution").contains("the short side of the frame"));
    assert!(ps::tooltip("Frame rate").contains("a ceiling rather than a target with VFR on"));
    assert!(ps::tooltip("Quality (CRF)").contains("lower is better and bigger"));
    assert!(ps::tooltip("Frame edges").contains("blurred copy of the picture itself"));
    assert!(ps::tooltip("Game audio").contains("original game audio sits under the narration"));
    assert!(ps::tooltip("Subtitles").contains(".srt is written beside the video either way"));
    assert!(ps::tooltip("Channels").contains("single channel"));

    // The two Transcode buttons (§A's heading row) and the thumbnail's two (§1 item 6).
    assert_eq!(
        ps::TRANSCODE_TIP,
        "Encode the video again from the cut and these settings — no model call, and the thumbnail and the upload text are left alone"
    );
    assert_eq!(
        ps::SAVE_VIDEO_TIP,
        "Save the finished video somewhere else — a copy; produce/final stays where it is"
    );
    assert!(ps::EXPORT_TIP.contains("JPEG") && ps::EXPORT_TIP.contains("2 MB"), "{}", ps::EXPORT_TIP);
    assert!(ps::REDRAW_TIP.contains("Draw the thumbnail again"), "{}", ps::REDRAW_TIP);

    // An unknown row says nothing rather than something wrong.
    assert_eq!(ps::tooltip("Whatever"), "");
}

// ---- s4: webm forces its own answer; the output is never asked -------------------

#[test]
fn sec_08_produce_1_screen_s4_container_forces_and_output() {
    assert_eq!(ITEM, "sec_08_produce_1_screen");
    // §A: webm forces vp9 and turns "track in file" into "none".
    assert_eq!(
        ps::apply_container_rules(Container::Webm, Codec::H264, Subtitles::TrackInFile),
        (Codec::Vp9, Subtitles::None)
    );
    // Every other container leaves both choices alone — the forcing is webm's, not a general rule.
    assert_eq!(
        ps::apply_container_rules(Container::Mp4, Codec::H264, Subtitles::TrackInFile),
        (Codec::H264, Subtitles::TrackInFile)
    );
    assert_eq!(
        ps::apply_container_rules(Container::Mkv, Codec::H265, Subtitles::BurnedIn),
        (Codec::H265, Subtitles::BurnedIn)
    );
    // webm with the answers it would force is unchanged by the rule.
    assert_eq!(
        ps::apply_container_rules(Container::Webm, Codec::Vp9, Subtitles::None),
        (Codec::Vp9, Subtitles::None)
    );

    // §A: the output file is not a question — produce/final.<container>.
    assert_eq!(ps::output_name(Container::Mp4), "final.mp4");
    assert_eq!(ps::output_name(Container::Mkv), "final.mkv");
    assert_eq!(ps::output_name(Container::Webm), "final.webm");

    // The name is the file layout writes, so the page cannot point at a video elsewhere.
    let root = std::env::temp_dir().join("np-produce-screen-s4");
    let dir = root.join("demo.naivepost");
    std::fs::create_dir_all(&dir).unwrap();
    let tree = Tree::new(&dir).unwrap();
    for (container, stem) in [
        (Container::Mp4, "mp4"),
        (Container::Mkv, "mkv"),
        (Container::Webm, "webm"),
    ] {
        assert_eq!(ps::output_name(container), format!("final.{stem}"));
        let path = tree.final_video(stem);
        assert_eq!(path.file_name().unwrap().to_str().unwrap(), ps::output_name(container));
        assert!(path.starts_with(tree.dir().join("produce")), "{path:?}");
    }
    std::fs::remove_dir_all(&root).ok();
}

// ---- s5: the session's own language is never offered ------------------------------

#[test]
fn sec_08_produce_1_screen_s5_translate_never_offers_own_language() {
    assert_eq!(ITEM, "sec_08_produce_1_screen");
    // §1: per-language ticks in the settings list; the session's own language is not among them.
    assert_eq!(ps::translate_options("English"), ["German", "French"]);
    assert_eq!(ps::translate_options("German"), ["English", "French"]);
    assert_eq!(ps::translate_options("French"), ["English", "German"]);
    // A language the app cannot translate into removes nothing, and so does an unset one.
    assert_eq!(ps::translate_options("Klingonian"), ["English", "German", "French"]);
    assert_eq!(ps::translate_options(""), ["English", "German", "French"]);
    // The menu is not case-sensitive: the session's language arrives from a settings field.
    assert_eq!(ps::translate_options("english"), ["German", "French"]);

    // Nothing is ticked until somebody ticks it, and the tick labels are §A's three names.
    assert!(project::Produce::default().translate.is_empty());
    assert_eq!(ps::TRANSLATE_LANGUAGES, ["English", "German", "French"]);
}

// ---- s6: up to eight pictures, the first is the base -----------------------------

#[test]
fn sec_08_produce_1_screen_s6_images_row_eight_slots() {
    assert_eq!(ITEM, "sec_08_produce_1_screen");
    assert_eq!(ps::MAX_IMAGES, 8);

    // Adding fills the row; a ninth is refused rather than dropping one.
    let mut list = frames(&["a", "b", "c", "d", "e", "f", "g", "h"]);
    assert_eq!(list.len(), ps::MAX_IMAGES);
    assert!(!ps::add_image(&mut list, "project:produce/publish/i.jpg"));
    assert_eq!(list.len(), 8);

    let mut few = frames(&["a"]);
    assert!(ps::add_image(&mut few, "project:produce/publish/b.jpg"));
    assert_eq!(few.len(), 2);

    // Order is the whole answer, and publish.rs answers it: first = base, rest = references.
    let list = frames(&["first", "second", "third"]);
    assert_eq!(publish::base(&Publish { frames: list.clone(), ..Default::default() }), Some("project:produce/publish/first.jpg"));
    let with_refs = Publish { frames: list, ..Default::default() };
    assert_eq!(publish::references(&with_refs).len(), 2);
    assert_eq!(publish::references(&with_refs)[0], "project:produce/publish/second.jpg");

    // Make base on a reference: it goes to the front and nothing is lost. Never on the base itself.
    let mut list = frames(&["first", "second", "third"]);
    assert!(ps::make_base(&mut list, 2));
    assert_eq!(
        list,
        frames(&["third", "first", "second"])
    );
    assert!(!ps::make_base(&mut list, 0));
    assert!(!ps::make_base(&mut list, 9));
    assert_eq!(list.len(), 3);

    // Change… keeps the slot's place; remove takes it out of the row.
    let mut list = frames(&["first", "second"]);
    assert!(ps::change_image(&mut list, 1, "project:produce/publish/other.jpg"));
    assert_eq!(list[1], "project:produce/publish/other.jpg");
    assert!(!ps::change_image(&mut list, 5, "project:produce/publish/x.jpg"));
    assert!(ps::remove_image(&mut list, 0));
    assert_eq!(list, vec!["project:produce/publish/other.jpg".to_string()]);
    assert!(!ps::remove_image(&mut list, 4));

    // A reference offers Make base; the base does not. §1's item 3.
    let base_row = ps::slot_actions(0, 3);
    assert!(!base_row.contains(&"Make base"), "{base_row:?}");
    assert!(base_row.contains(&"Set Thumbnail") && base_row.contains(&"Change…") && base_row.contains(&"remove"));
    let reference = ps::slot_actions(1, 3);
    assert_eq!(reference, &["Make base", "Set Thumbnail", "Change…", "remove"]);

    // Empty state (§A's own sentence) and the crop box that only a differently-shaped base wears.
    assert_eq!(ps::EMPTY_IMAGES, "No image — the thumbnail will be drawn from the instruction alone.");
    assert!(ps::EMPTY_IMAGES_HINT.starts_with("Add image…"));
    assert_eq!(publish::base(&Publish::default()), None);
    assert!(ps::shows_crop_box(Some((1920, 1080)), (1080, 1920)));
    assert!(!ps::shows_crop_box(Some((1920, 1080)), (1920, 1080)));
    assert!(!ps::shows_crop_box(None, (1080, 1920)));
}

// ---- s7: the thumbnail is a text-overlay editor ----------------------------------

#[test]
fn sec_08_produce_1_screen_s7_thumbnail_text_overlay_editor() {
    assert_eq!(ITEM, "sec_08_produce_1_screen");
    // §A: the title band defaults across the top of the picture, on its own.
    assert_eq!(ps::TITLE_BOX_DEFAULT, (0.5, 0.25, 1.0, 0.4));
    let mut publish = Publish::default();
    assert_eq!(publish.title_box, None);
    let (cx, cy, wf, hf) = ps::TITLE_BOX_DEFAULT;
    publish.title_box = Some(project::TitleBox { cx, cy, wf, hf });
    let box_ = publish.title_box.unwrap();
    assert_eq!((box_.cx, box_.cy, box_.wf, box_.hf), (0.5, 0.25, 1.0, 0.4));

    // §A's overlay geometry: 200 px picture, min box 28, snap 10, ✎ chip 22.
    assert_eq!(ps::OVERLAY_EDGE_PX, 200.0);
    assert_eq!((ps::OVERLAY_MIN_PX, ps::OVERLAY_SNAP_PX, ps::OVERLAY_CHIP_PX), (28.0, 10.0, 22.0));
    assert!(ps::box_is_too_small(27.0, 100.0));
    assert!(ps::box_is_too_small(100.0, 27.0));
    assert!(!ps::box_is_too_small(28.0, 28.0));

    // Snap to a picture edge or middle within 10 px; a far value stays where the hand left it.
    let targets = [0.0, 100.0, 200.0];
    assert_eq!(ps::snap(6.0, &targets), 0.0);
    assert_eq!(ps::snap(194.0, &targets), 200.0);
    assert_eq!(ps::snap(105.0, &targets), 100.0);
    assert_eq!(ps::snap(50.0, &targets), 50.0);
    assert_eq!(ps::snap(30.0, &[]), 30.0);

    // Drag a box → words printed to fill it; the geometry is fractions of the picture (project::TextMark).
    let mut mark = TextMark::default();
    ps::place_words(&mut mark, 0.5, 0.8, 0.6, 0.12, "  ninety percent  ");
    assert_eq!((mark.cx, mark.cy, mark.wf, mark.hf), (0.5, 0.8, 0.6, 0.12));
    assert_eq!(mark.text, "ninety percent");
    let text = serde_json::to_string(&mark).unwrap();
    assert_eq!(serde_json::from_str::<TextMark>(&text).unwrap(), mark);

    // ✎ reword keeps the box; ✎ remove takes it out and leaves the others in order.
    let mut marks = vec![mark.clone()];
    let mut other = TextMark::default();
    ps::place_words(&mut other, 0.2, 0.2, 0.3, 0.1, "second box");
    marks.push(other);
    ps::set_words(&mut marks[0], "new words");
    assert_eq!(marks[0].text, "new words");
    assert_eq!((marks[0].cx, marks[0].wf), (0.5, 0.6));
    assert!(ps::remove_words(&mut marks, 0));
    assert_eq!(marks.len(), 1);
    assert_eq!(marks[0].text, "second box");
    assert!(!ps::remove_words(&mut marks, 3));
}

// ---- s8: the title is printed across the thumbnail once only ---------------------

#[test]
fn sec_08_produce_1_screen_s8_title_is_printed_once() {
    assert_eq!(ITEM, "sec_08_produce_1_screen");
    // §1: printed across the thumbnail the first time only; then separate.
    let mut publish = Publish { title: "A title for the video".to_string(), ..Default::default() };
    assert!(ps::seed_title(&mut publish));
    assert_eq!(publish.thumb_title, "A title for the video");
    assert!(publish.title_seeded);

    // Retitled afterwards: the picture keeps the line it was drawn with.
    publish.title = "Retitled for YouTube".to_string();
    assert!(!ps::seed_title(&mut publish));
    assert_eq!(publish.thumb_title, "A title for the video");

    // Nothing to print yet: no title, so nothing is seeded and the page can still seed later.
    let mut empty = Publish::default();
    assert!(!ps::seed_title(&mut empty));
    assert!(!empty.title_seeded);

    // The written column's boxes and their sentences (§A).
    assert_eq!(ps::DESCRIPTION_LINES, 8);
    assert_eq!((ps::INSTRUCTION_LINES, ps::NEGATIVE_LINES), (4, 2));
    assert!(ps::SUGGEST_TIP.contains("the only thing that does"), "{}", ps::SUGGEST_TIP);
    assert!(ps::SUGGEST_TIP.contains("fresh title, thumbnail instruction and description"));
    assert!(ps::TITLE_TIP.contains("Four to seven words"), "{}", ps::TITLE_TIP);
    assert!(ps::TITLE_PLACEHOLDER.contains("also printed on the thumbnail"));
    assert!(ps::INSTRUCTION_TIP.contains("No words"));
    assert!(ps::NEGATIVE_TIP.contains("watermarks, logos, lettering, extra limbs"));
}

// ---- s9: the Inputs readout -------------------------------------------------------

#[test]
fn sec_08_produce_1_screen_s9_inputs_readout() {
    assert_eq!(ITEM, "sec_08_produce_1_screen");
    // §1: "N clip(s) · mm:ss[ · no narration | · N to speak][ · no upload text]".
    assert_eq!(ps::inputs_line(4, 95.0, false, 3, true), "4 clip(s) \u{b7} 01:35 \u{b7} 3 to speak");
    // The two narration states are alternatives, never both.
    assert_eq!(ps::inputs_line(4, 95.0, true, 3, true), "4 clip(s) \u{b7} 01:35 \u{b7} no narration");
    // With narration on and nothing to speak, neither clause is a lie.
    assert_eq!(ps::inputs_line(4, 95.0, false, 0, true), "4 clip(s) \u{b7} 01:35");
    assert_eq!(
        ps::inputs_line(4, 95.0, false, 3, false),
        "4 clip(s) \u{b7} 01:35 \u{b7} 3 to speak \u{b7} no upload text"
    );
    // No cut: the page says it in the other pages' words.
    assert_eq!(ps::inputs_line(0, 0.0, false, 0, false), ps::NO_CUT_LINE);
    assert_eq!(ps::NO_CUT_LINE, "no cut yet \u{2014} build one on the Cut step");

    // The tooltip: what will be spoken first, which recordings are mixed, the voice, publish.json.
    let mixed = vec!["rec-a.mp4".to_string(), "rec-b.mp4".to_string()];
    let tip = ps::inputs_tooltip("cut/cut.json \u{2014} 4 clips", Some("the first line"), &mixed, Some("Mira"), true);
    let rows: Vec<&str> = tip.split("\n\n").collect();
    assert_eq!(rows[0], "cut/cut.json \u{2014} 4 clips");
    assert_eq!(rows[1], "Spoken first: \u{201c}the first line\u{201d}");
    assert_eq!(rows[2], "Mixed recordings: rec-a.mp4, rec-b.mp4");
    assert_eq!(rows[3], "Voice: Mira");
    assert!(rows[4].contains("publish.json is written"), "{tip}");

    // Nothing to speak and no publish.json: the rows say so instead of being left out silently.
    let bare = ps::inputs_tooltip("", None, &[], None, false);
    assert_eq!(bare.split("\n\n").count(), 2, "{bare}");
    assert!(bare.contains("nothing will be spoken"), "{bare}");
    assert!(bare.contains("no publish.json yet"), "{bare}");

    // The shell's own Produce row is the same readout (§8-produce.md §1). Its base shape is what this has
    // to agree with; the shell writes "N clip(s)" from a count and adds "· no narration" off the record,
    // exactly as inputs_line does — so the two are equal for the shapes they share.
    assert_eq!(
        format!("{} clip(s) \u{b7} {}", 4, naivepost::tools::mm_ss(95.0)),
        "4 clip(s) \u{b7} 01:35"
    );
    let shell_shape = ps::inputs_line(4, 95.0, true, 0, false);
    assert!(shell_shape.starts_with("4 clip(s) \u{b7} 01:35 \u{b7} no narration"), "{shell_shape}");
    assert!(shell_shape.ends_with("no upload text"), "{shell_shape}");
}

// ---- s10: the Outputs row counts, and only counts --------------------------------

#[test]
fn sec_08_produce_1_screen_s10_outputs_count_only() {
    assert_eq!(ITEM, "sec_08_produce_1_screen");
    // §1: count only — "61 files, 220 MB".
    let summary = ps::outputs_summary(61, 220 * 1024 * 1024);
    assert_eq!(summary, "61 files, 220 MB");
    assert!(summary.contains("61 files") && summary.contains("220 MB"));

    // An empty folder is not "0 MB" of nothing; a small one reads in KB.
    assert_eq!(ps::outputs_summary(0, 0), "0 files");
    assert_eq!(ps::outputs_summary(1, 512 * 1024), "1 files, 512 KB");
    assert_eq!(ps::outputs_summary(3, 1024 * 1024), "3 files, 1 MB");

    // The folder button's tooltip names the four things this step writes.
    assert_eq!(
        ps::FOLDER_TIP,
        "produce/ \u{2014} the finished video, the per-clip encodes, the thumbnail and the upload text"
    );
}
