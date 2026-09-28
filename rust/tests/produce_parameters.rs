//! §08-produce#4-parameters-used — `spec/08-produce.md` §4, checked against `naivepost::params::produce()`
//! and the owners of every value it names.
//!
//! §4 is a list of ids rather than of steps, so what these tests pin is that the list and the code cannot
//! drift: every id §4 names either has a row built from the constant its rule reads — exercised *through that
//! rule*, not just looked up — or is absent because no rule in this tree reads it yet, which is asserted too,
//! so a number arriving with nothing behind it is caught here rather than shipping. Same shape as
//! §07-narrate#4's and §06-effects#6's tests.
//!
//! Values, each re-read from its owner below: P.eng.minClipSeconds 0.5, P.policy.gameVolume 0.22,
//! P.policy.subtitleBreakSeconds 0.6, P.policy.subtitleRowChars 42, P.policy.subtitleMaxSeconds 6,
//! P.policy.subtitleHoldSeconds 1.2, P.policy.subtitleMinSeconds 0.8, P.machine.translateBatch 150,
//! P.eng.narrationMaxExtendSeconds 4, P.eng.narrationMaxTempo 1.25, P.eng.narrationTailSeconds 0.2,
//! P.eng.loudness `I -14, TP -1.5, LRA 11`, P.eng.clipLimiter `-1 dBFS (0.891)`,
//! P.machine.thumbnailLongSide 1280, P.eng.titleBand `{0.5, 0.25, 1, 0.4}`, P.policy.publishFrames 3,
//! P.eng.publishMaxFrames 8, P.eng.thumbnailJPEGMax 2 MiB. Tools: `tool:ffmpeg.encode` (the encode, the mux
//! and the poster).

use naivepost::params;
use naivepost::produce_embed as embed;
use naivepost::produce_render as render;
use naivepost::produce_runs as runs;
use naivepost::produce_screen as screen;
use naivepost::produce_stamp as stamp;
use naivepost::produce_subtitles as subs;
use naivepost::cut::Seg;
use naivepost::project::{self, Container, Produce, Resolution, TitleBox};

const ITEM: &str = "§08-produce#4-parameters-used";

/// One row of this section's own list.
fn row(id: &str) -> params::Param {
    params::produce().into_iter().find(|param| param.id == id).unwrap_or_else(|| {
        let ids: Vec<&str> = params::produce().iter().map(|param| param.id).collect();
        panic!("{id} is in §08's §4 list and so must be catalogued; this section holds {ids:?}")
    })
}

/// Every id, whichever page's rows it is filed under — `params::find` answers Prepare's list only.
fn anywhere(id: &str) -> Option<params::Param> {
    params::prepare()
        .into_iter()
        .chain(params::cut())
        .chain(params::effects())
        .chain(params::narrate())
        .chain(params::produce())
        .find(|param| param.id == id)
}

/// A number §4 writes with a decimal point, compared as a number: `params::num` trims the trailing zero, so
/// comparing strings would pin a spelling neither side chose.
fn number(id: &str) -> f64 {
    row(id).spelled.parse().unwrap_or_else(|_| panic!("{id} spells a number"))
}

fn settings() -> Produce {
    Produce::default()
}

// ---- S1: the two numbers §4 opens with -------------------------------------------------

#[test]
fn sec_08_produce_4_parameters_used_s1_the_shortest_clip_and_the_bed_under_the_narration() {
    assert_eq!(ITEM, "§08-produce#4-parameters-used");
    // P.eng.minClipSeconds — 0.5, filed under §6 because the speed clamp reads the same number.
    let floor = anywhere("P.eng.minClipSeconds").expect("minClipSeconds is catalogued");
    assert_eq!(floor.spelled, "0.5");
    assert_eq!(naivepost::tools::cutpass::MIN_CLIP_SECONDS, 0.5);

    // And the render really refuses at that floor: a 0.4 s segment is dropped and named.
    let mut short = Seg::default();
    short.s = 10.0;
    short.e = 10.4;
    let (clips, logs) = render::plan_clips(&[short], |_| Vec::new(), "16:9", |_| true, &[], &[], (1920, 1080));
    assert!(clips.is_empty(), "{clips:?}");
    assert_eq!(logs.len(), 1, "{logs:?}");
    assert!(logs[0].contains("too short to render, dropped"), "{}", logs[0]);

    // P.policy.gameVolume — 0.22, the bed's level under a narration that rides at full.
    assert_eq!(number("P.policy.gameVolume"), 0.22);
    assert_eq!(settings().game_volume, 0.22);
    let clip = render::Clip { no: 1, seg_s: 0.0, on_screen: 5.0, rate: 1.0, lines: vec![render::Placed {
        entry_index: 0,
        at: 0.3,
        tempo: 1.0,
        extend: 0.0,
        text: String::new(),
        speech: 2.0,
    }], ..Default::default() };
    let chain = render::audio_chain(&clip, &settings(), 0, &[]).join(",");
    assert!(chain.contains("volume=0.22"), "{chain}");
}

// ---- S2: the five cue numbers and one batch -------------------------------------------

#[test]
fn sec_08_produce_4_parameters_used_s2_the_cue_bounds_and_the_translation_batch() {
    assert_eq!(ITEM, "§08-produce#4-parameters-used");
    // Each row must read as its module's constant, so a change on one side fails here.
    // P.policy.subtitleBreakSeconds: the pause that ends a cue.
    assert_eq!(number("P.policy.subtitleBreakSeconds"), 0.6);
    assert_eq!(row("P.policy.subtitleBreakSeconds").spelled, subs::SUBBREAK_SECONDS.to_string());
    // P.policy.subtitleRowChars: two rows of this many characters end a cue.
    assert_eq!(row("P.policy.subtitleRowChars").spelled, subs::ROW_CHARS.to_string());
    assert_eq!(row("P.policy.subtitleRowChars").spelled, "42");
    // P.policy.subtitleMaxSeconds, subtitleHoldSeconds, subtitleMinSeconds: the longest cue, the shortest
    // hold after speech, and the floor a cue is never cut below.
    assert_eq!(number("P.policy.subtitleMaxSeconds"), 6.0);
    assert_eq!(row("P.policy.subtitleMaxSeconds").spelled, subs::MAX_SECONDS.to_string());
    assert_eq!(number("P.policy.subtitleHoldSeconds"), 1.2);
    assert_eq!(row("P.policy.subtitleHoldSeconds").spelled, subs::HOLD_SECONDS.to_string());
    assert_eq!(number("P.policy.subtitleMinSeconds"), 0.8);
    assert_eq!(row("P.policy.subtitleMinSeconds").spelled, subs::MIN_SECONDS.to_string());
    // P.machine.translateBatch: numbered lines per translation request.
    assert_eq!(row("P.machine.translateBatch").spelled, subs::BATCH.to_string());
    assert_eq!(row("P.machine.translateBatch").spelled, "150");
}

// ---- S3: F4.3's fitting bounds ---------------------------------------------------------

#[test]
fn sec_08_produce_4_parameters_used_s3_fitting_a_line_to_its_clip() {
    assert_eq!(ITEM, "§08-produce#4-parameters-used");
    // P.eng.narrationMaxExtendSeconds (4) and P.eng.narrationTailSeconds (0.2) are catalogued under §7;
    // P.eng.narrationMaxTempo is this section's row. All three resolve exactly once.
    assert_eq!(number("P.eng.narrationMaxTempo"), 1.25);
    assert_eq!(render::MAX_TEMPO, 1.25);
    assert_eq!(anywhere("P.eng.narrationMaxExtendSeconds").unwrap().spelled, "4");
    assert_eq!(render::MAX_EXTEND_SECONDS, 4.0);
    assert_eq!(anywhere("P.eng.narrationTailSeconds").unwrap().spelled, "0.2");
    assert_eq!(render::TAIL_SECONDS, 0.2);

    // A line over the room grows the clip, and never past the ceiling.
    let (placed, logs) = render::fit(&[(0, 0.3, 5.5)], 2.0);
    assert!(logs.is_empty(), "{logs:?}");
    assert_eq!(placed[0].extend, 4.0, "grown to the ceiling and no further");

    // A hopeless line comes back at the tempo ceiling with a log naming what was left over.
    let (placed, logs) = render::fit(&[(0, 0.3, 60.0)], 2.0);
    assert!(placed[0].extend <= 4.0);
    assert!(placed[0].tempo <= 1.25 + f64::EPSILON, "tempo {}", placed[0].tempo);
    let joined = logs.join("\n");
    assert!(joined.contains("does not fit"), "{joined}");
    assert!(joined.contains("sped up 1.25x"), "{joined}");
}

// ---- S4: the final mix's two targets -----------------------------------------------------

#[test]
fn sec_08_produce_4_parameters_used_s4_the_loudness_target_and_the_ceiling() {
    assert_eq!(ITEM, "§08-produce#4-parameters-used");
    // tool:ffmpeg.encode applies both. §10 spells them; the rows are read out of the filter strings, so a
    // change to what ffmpeg is sent moves the row with it.
    assert!(render::LOUDNORM.contains("loudnorm=I=-14:TP=-1.5:LRA=11"), "{}", render::LOUDNORM);
    assert_eq!(row("P.eng.loudness").spelled, "I -14, TP -1.5, LRA 11");
    assert_eq!(render::LIMITER, "alimiter=limit=0.891:level=disabled");
    assert_eq!(row("P.eng.clipLimiter").spelled, "-1 dBFS (0.891)");

    // Order is part of the claim: the limiter is the last thing before the format, and it sits after the
    // narration's bed — a ceiling applied before the mix would let the mix clip instead.
    let clip = render::Clip { no: 1, seg_s: 0.0, on_screen: 5.0, rate: 1.0, lines: vec![render::Placed {
        entry_index: 0,
        at: 0.3,
        tempo: 1.0,
        extend: 0.0,
        text: String::new(),
        speech: 2.0,
    }], ..Default::default() };
    let chain = render::audio_chain(&clip, &settings(), 0, &[]);
    let bed = chain.iter().position(|f| f == "volume=0.22").expect("the bed");
    let limiter = chain.iter().position(|f| f == render::LIMITER).expect("the limiter");
    let format = chain.iter().position(|f| f.starts_with("format=sample_fmts=fltp:sample_rates=48000")).expect("the format");
    assert!(bed < limiter && limiter < format, "{chain:?}");
}

// ---- S5: the thumbnail's size, band and frame row -------------------------------------

#[test]
fn sec_08_produce_4_parameters_used_s5_the_thumbnails_size_and_band() {
    assert_eq!(ITEM, "§08-produce#4-parameters-used");
    // P.machine.thumbnailLongSide — 1280 on the LONG edge, so a tall picture is not 1280 turned sideways.
    assert_eq!(number("P.machine.thumbnailLongSide"), 1280.0);
    assert_eq!(screen::THUMB_LONG_SIDE, 1280);
    // No aspect → the footage's own shape; §2's default when even that is unknown is 16:9.
    assert_eq!(screen::thumb_box("", 0, 0), (1280, 720));
    assert_eq!(screen::thumb_box("source", 0, 0), (1280, 720));
    // A shape given is the picture's own ratio, capped on its longer edge.
    assert_eq!(screen::thumb_box("9:16", 1080, 1920), (720, 1280));
    assert_eq!(screen::thumb_box("", 4000, 3000), (1280, 960));
    for box_ in [
        screen::thumb_box("", 0, 0),
        screen::thumb_box("9:16", 1080, 1920),
        screen::thumb_box("", 4000, 3000),
        screen::thumb_box("1:1", 500, 500),
    ] {
        assert_eq!(box_.0 % 2, 0, "{box_:?} has an odd edge");
        assert_eq!(box_.1 % 2, 0, "{box_:?} has an odd edge");
        assert!(box_.0.max(box_.1) <= screen::THUMB_LONG_SIDE, "{box_:?} leaves the tier");
    }

    // P.eng.titleBand — §01 §5's example box, spelled once by project.
    assert_eq!(row("P.eng.titleBand").spelled, "{0.5, 0.25, 1, 0.4}");
    assert_eq!(TitleBox::default(), TitleBox { cx: 0.5, cy: 0.25, wf: 1.0, hf: 0.4 });
}

#[test]
fn sec_08_produce_4_parameters_used_s5_three_frames_to_start_and_eight_at_most() {
    assert_eq!(ITEM, "§08-produce#4-parameters-used");
    // P.policy.publishFrames — the first run hands the model three frames, the earliest being the base.
    assert_eq!(row("P.policy.publishFrames").spelled, "3");
    assert_eq!(screen::FIRST_IMAGES, 3);
    let available: Vec<String> = (0..6).map(|n| format!("project:frame{n}.jpg")).collect();
    let seeded = screen::seed_frames(&available);
    assert_eq!(seeded.len(), 3);
    assert_eq!(seeded, available[..3], "in the order offered");
    assert!(screen::seed_frames(&[]).is_empty());
    // Fewer frames available than three is not a shortage: all of them are taken.
    let two = vec![available[0].clone(), available[1].clone()];
    assert_eq!(screen::seed_frames(&two), two);
    // The first of them is the base the image model edits.
    let mut record = project::Publish::default();
    record.frames = seeded;
    assert_eq!(naivepost::publish::base(&record), Some("project:frame0.jpg"));

    // P.eng.publishMaxFrames — the row's ceiling, and the ninth is refused rather than dropped silently.
    assert_eq!(row("P.eng.publishMaxFrames").spelled, "8");
    assert_eq!(screen::MAX_IMAGES, 8);
    let mut frames: Vec<String> = (0..8).map(|n| format!("f{n}")).collect();
    assert!(!screen::add_image(&mut frames, "f8"));
    assert_eq!(frames.len(), 8);
    // The cap and the first-run count are one rule, not two numbers: seeding never overfills the row.
    assert!(screen::FIRST_IMAGES <= screen::MAX_IMAGES);
}

// ---- S6: the settings are the project's own -------------------------------------------

#[test]
fn sec_08_produce_4_parameters_used_s6_the_encoder_settings_are_stored_state() {
    assert_eq!(ITEM, "§08-produce#4-parameters-used");
    // §4 lists these under "Project", not under parameters: they are stored per project (§A's row), so none
    // of them has a `P.*` id and none is catalogued. What is pinned is that the row's defaults are what the
    // page shows and what the encoder is sent.
    let settings = settings();
    assert_eq!(settings.container, Container::Mp4);
    assert_eq!(settings.codec, project::Codec::H264);
    assert_eq!(settings.crf, 24);
    assert_eq!(render::preset_name(settings.preset), "slow");
    assert_eq!(settings.resolution, Resolution::P1080);
    assert_eq!(render::frame_rate_name(settings.frame_rate), "30");
    assert_eq!(settings.audio_kbps, 128);
    // The page's rows and the settings' fields are the same thirteen.
    assert_eq!(screen::SETTINGS_ROWS.len(), 13);

    let argv = render::codec_args(&settings).join(" ");
    assert!(argv.contains("libx264"), "{argv}");
    assert!(argv.contains("-crf 24"), "{argv}");
    assert!(argv.contains("-preset slow"), "{argv}");

    // And a setting flipped is a video that is no longer up to date: the settings are one of the stamp's
    // seven groups, hashed in full.
    let segs = vec![Seg::default()];
    let sources = vec![project::Source::default()];
    let aspect = String::new();
    let voice = String::new();
    let first = stamp::Input {
        settings: &settings,
        segs: &segs,
        lines: &[],
        sources: &sources,
        aspect: &aspect,
        voice: &voice,
        no_narration: false,
    };
    let mut slower = settings.clone();
    slower.crf = 23;
    let second = stamp::Input {
        settings: &slower,
        segs: &segs,
        lines: &[],
        sources: &sources,
        aspect: &aspect,
        voice: &voice,
        no_narration: false,
    };
    fn facts(_: &std::path::Path) -> (u64, u64) {
        (10, 20)
    }
    fn wav(_: &naivepost::narration::Entry) -> Option<std::path::PathBuf> {
        None
    }
    let facts_fn = facts;
    let wav_fn = wav;
    assert_ne!(first.stamp_of(&wav_fn, &facts_fn), second.stamp_of(&wav_fn, &facts_fn));
    let named = stamp::parts(&first, &wav_fn, &facts_fn).into_iter().map(|(name, _)| name).collect::<Vec<_>>();
    assert!(named.contains(&"settings"), "{named:?}");
}

// ---- S7: the engineering sentence -------------------------------------------------------

#[test]
fn sec_08_produce_4_parameters_used_s7_input_order_and_the_faststart_flags() {
    assert_eq!(ITEM, "§08-produce#4-parameters-used");
    // tool:ffmpeg.encode. Input ordering: the picture's seek goes before its -i, so a clip starting at 40
    // minutes does not decode 40 minutes first.
    let clip = render::Clip { no: 1, seg_s: 12.5, on_screen: 5.0, rate: 1.0, ..Default::default() };
    let argv = render::encode_command(&clip, &settings(), &[("picture", Some("media/talk.mkv"))], None, std::path::Path::new("produce/final.mp4"));
    let ss = argv.iter().position(|a| a == "-ss").expect("-ss");
    let input = argv.iter().position(|a| a == "-i").expect("-i");
    assert!(ss < input, "{argv:?}");

    // The faststart flags are mp4's only: they exist to stop that container writing a negative-CTS start.
    let video = std::path::Path::new("produce/video.mp4");
    let audio = std::path::Path::new("produce/audio.m4a");
    let out = std::path::Path::new("produce/final.mp4");
    let mp4 = render::mux_command(video, audio, &[], &settings(), out).join(" ");
    for flag in ["+faststart", "-use_editlist", "0", "-negative_cts_offsets"] {
        assert!(mp4.contains(flag), "{flag} missing from {mp4}");
    }
    for container in [Container::Mkv, Container::Webm] {
        let mut other = settings();
        other.container = container;
        let line = render::mux_command(video, audio, &[], &other, out).join(" ");
        assert!(!line.contains("faststart"), "{container:?} should carry none: {line}");
        assert!(!line.contains("use_editlist"), "{line}");
    }
}

#[test]
fn sec_08_produce_4_parameters_used_s7_the_servers_deadlines_the_poster_and_the_ladder() {
    assert_eq!(ITEM, "§08-produce#4-parameters-used");
    // sd.cpp's timeouts (15/60/30/1/10 s) are §10 prose with no `P.` row, so nothing is catalogued under an
    // sd id — and inventing one would be a number with no rule behind it.
    for section in [params::prepare(), params::cut(), params::effects(), params::narrate(), params::produce()] {
        for param in section {
            assert!(
                !(param.id.starts_with("P.eng.sd") || param.id.starts_with("P.machine.sd")),
                "{} is a row nothing reads",
                param.id
            );
        }
    }
    // The only deadline the app itself sets on a server is audio.cpp's unload — 20 s in services, and §10
    // lists it as `P.eng.audiocppUnloadSeconds` without a row here (its home is the settings' prose), so what
    // is pinned is that the number in the code is the one §10 writes.
    assert_eq!(naivepost::services::UNLOAD_TIMEOUT.as_secs(), 20);
    assert_eq!(naivepost::services::SD_PORT, 1234);

    // Poster quality: §F5.5's "JPEG 90" and the rung ffmpeg takes for it. tool:ffmpeg.encode does the job.
    assert_eq!(embed::POSTER_JPEG_QUALITY, 90);
    assert_eq!(embed::POSTER_QSCALE, "2");
    let argv = embed::poster_command(std::path::Path::new("publish/thumbnail.png"), std::path::Path::new("produce/final.jpg")).join(" ");
    assert!(argv.contains("-q:v 2"), "{argv}");

    // P.eng.thumbnailJPEGMax — the cap an upload puts on the picture, and the ladder it is tried against.
    assert_eq!(number("P.eng.thumbnailJPEGMax"), 2_097_152.0);
    assert_eq!(runs::JPEG_MAX_BYTES, 2 * 1024 * 1024);
    assert_eq!(runs::JPEG_QUALITIES, [92, 85, 75, 60, 40]);
    assert!(runs::JPEG_QUALITIES.windows(2).all(|pair| pair[0] > pair[1]), "best first");
    let over = runs::jpeg_target(runs::JPEG_MAX_BYTES + 1).unwrap_err();
    assert_eq!(over, "JPEG exceeds 2 MiB after 92/85/75 — check the sd.cpp encoder");
}

// ---- S8: what §4 names that has no row ----------------------------------------------------

#[test]
fn sec_08_produce_4_parameters_used_s8_the_brief_bound_and_the_one_row_per_parameter() {
    assert_eq!(ITEM, "§08-produce#4-parameters-used");
    // P.machine.briefMaxChars (120 kB) bounds F5.6 S1's upload brief, and the brief now exists in
    // `produce_upload`, so its row must exist with it: the two are asserted together rather than one being
    // absent because nothing read it.
    let sources = [
        include_str!("../src/produce_upload.rs"),
        include_str!("../src/produce_screen.rs"),
        include_str!("../src/publish.rs"),
    ];
    assert!(
        sources.iter().any(|text| text.contains("THE FINISHED VIDEO")),
        "the upload brief is built, so this test must keep watching its bound"
    );
    assert_eq!(number("P.machine.briefMaxChars"), 120_000.0);
    let rows = params::produce()
        .into_iter()
        .filter(|p| p.id == "P.machine.briefMaxChars")
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), 1, "exactly one row for the brief bound");
    assert_eq!(rows[0].from, "produce_upload::BRIEF_MAX_CHARS");

    // One row per parameter: the two fitting numbers §4 borrows from F4.3 live in §7 and are not repeated here.
    let in_produce = params::produce().into_iter().filter(|p| p.id == "P.eng.narrationTailSeconds").count();
    assert_eq!(in_produce, 0, "its home is narrate()");
    assert_eq!(params::narrate().into_iter().filter(|p| p.id == "P.eng.narrationTailSeconds").count(), 1);
    let in_produce = params::produce().into_iter().filter(|p| p.id == "P.eng.narrationMaxExtendSeconds").count();
    assert_eq!(in_produce, 0);
    assert_eq!(params::narrate().into_iter().filter(|p| p.id == "P.eng.narrationMaxExtendSeconds").count(), 1);
}

// ---- S9: the section's own shape ------------------------------------------------------------

#[test]
fn sec_08_produce_4_parameters_used_s9_the_sections_rows_are_exactly_these() {
    assert_eq!(ITEM, "§08-produce#4-parameters-used");
    let ids: Vec<&str> = params::produce().iter().map(|param| param.id).collect();
    assert_eq!(
        ids,
        [
            // §F5.4's cue building and one translation request's size.
            "P.policy.subtitleBreakSeconds",
            "P.policy.subtitleRowChars",
            "P.policy.subtitleMaxSeconds",
            "P.policy.subtitleHoldSeconds",
            "P.policy.subtitleMinSeconds",
            "P.machine.translateBatch",
            // §F5.7's page runs.
            "P.eng.thumbnailJPEGMax",
            "P.policy.publishWordsSnapPx",
            // §4's mix, fitting bound and thumbnail — this round.
            "P.policy.gameVolume",
            "P.eng.loudness",
            "P.eng.clipLimiter",
            // §4's frame-edge blur.
            "P.eng.blurSigma",
            "P.eng.narrationMaxTempo",
            "P.machine.thumbnailLongSide",
            "P.eng.titleBand",
            "P.policy.publishFrames",
            "P.eng.publishMaxFrames",
        // F5.6 S1's upload-brief bound, catalogued where the brief reads it.
        "P.machine.briefMaxChars",
        ]
    );
    // §10's spelling won: the F5.7 round's invented id is gone rather than kept beside it.
    assert!(!ids.contains(&"P.eng.jpegMaxBytes"), "{ids:?}");
    let unique: std::collections::HashSet<&str> = ids.iter().copied().collect();
    assert_eq!(unique.len(), ids.len(), "one row per parameter");

    // Every row names where its value lives, and that name is really in the module it points at.
    let files: &[(&str, &str)] = &[
        ("produce_subtitles", include_str!("../src/produce_subtitles.rs")),
        ("produce_runs", include_str!("../src/produce_runs.rs")),
        ("produce_render", include_str!("../src/produce_render.rs")),
        ("produce_screen", include_str!("../src/produce_screen.rs")),
    ];
    for param in params::produce() {
        assert!(!param.spelled.is_empty(), "{} catalogued with no value", param.id);
        let Some((module, name)) = param.from.split_once("::") else {
            // A row built from a default's field (`project::Produce::default — game_volume`) is allowed to
            // name the field rather than a constant; it still has to say which field.
            assert!(param.from.contains(' '), "{} names no home", param.id);
            continue;
        };
        let Some((_, text)) = files.iter().find(|(file, _)| *file == module) else {
            continue; // project:: and tools:: live outside this section's own files
        };
        assert!(text.contains(name), "{} names {} which {} does not hold", param.id, name, module);
    }
}
