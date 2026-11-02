//! §08-produce#6-details-confirmed-against-the-code-verification-pass — 08 §6, checked against the code.
//!
//! §6 is the spec's own audit: the details that were read back out of the Go prototype and found to matter. It
//! has no numbered steps, so its bullets are read here as S1..S10 (clip planning refusals; the planning log
//! lines; what `produce/clips/` also holds; muxing; the legacy `produce` keys; translation; frame boxes;
//! publish; the `<video>` tag; the stamp) and one test per bullet pins them.
//!
//! Most bullets name a rule that already lives in the module owning its flow, so these tests call *that* code
//! through [`naivepost::produce_details`] rather than a copy of it — the point of a verification pass is that
//! the sentence and the shipped function agree, not that a second function says the same thing.
//!
//! Ids used: `tool:ffmpeg.encode` (the mux line, `joined.<container>`, everything in `produce/clips/`),
//! `P.machine.thumbnailLongSide`, `P.policy.gameVolume`, `P.policy.subtitleRowChars`,
//! `P.policy.subtitleHoldSeconds`, `P.policy.subtitleMinSeconds`, `P.policy.publishFrames`,
//! `P.eng.publishMaxFrames`, and the bare ids §6 needs that §10 has no row for (`machine.driftSeconds`,
//! `machine.boundsToleranceSeconds`, `preview.dragMinPx`, `machine.descriptionHeadingChars`,
//! `machine.labelledLinesMax`).

use std::collections::BTreeMap;
use std::path::Path;

use crate::common;
use common::temp_dir_created as temp_dir;
use naivepost::produce_details as details;
use naivepost::produce_render as render;
use naivepost::produce_screen as screen;
use naivepost::produce_stamp as stamp;
use naivepost::produce_subtitles as subs;
use naivepost::project::{self, Codec, Container, Produce, Subtitles};
use naivepost::roles::Language;

const ITEM: &str = "§08-produce#6-details-confirmed-against-the-code-verification-pass";

/// A language row as `SUBTITLE_LANGUAGES` spells it: code, ISO-639-2 tag, name.
fn language(code: &str, tag: &str, name: &str) -> Language {
    Language {
        code: code.to_string(),
        tag: tag.to_string(),
        name: name.to_string(),
    }
}

fn cue(text: &str, s: f64, e: f64) -> subs::Cue {
    subs::Cue {
        text: text.to_string(),
        s,
        e,
        pos: String::new(),
    }
}

// ---- S1: clip planning refusals and the rules behind them -------------------------

// ---- S2: the planning log lines -----------------------------------------------------

// ---- S3: what produce/clips/ also holds ---------------------------------------------

// ---- S4: muxing ---------------------------------------------------------------------

#[test]
fn sec_08_produce_6_details_confirmed_against_the_code_verification_pass_s4_mux_and_stored_defaults(
) {
    assert_eq!(
        ITEM,
        "§08-produce#6-details-confirmed-against-the-code-verification-pass"
    );
    let list = [
        language("de", "deu", "Deutsch"),
        language("fr", "fra", "Français"),
    ];
    // A listed language carries its ISO-639-2 tag and its own name, so a player's menu reads what the person ticked.
    assert_eq!(
        details::language_tag("de", &list),
        ("deu".to_string(), "Deutsch".to_string())
    );
    // An unlisted one has no tag to carry: `und`, named by its upper-cased code so it still reads as a label.
    assert_eq!(
        details::language_tag("nl", &list),
        ("und".to_string(), "NL".to_string())
    );
    assert_eq!(details::UNDETERMINED_TAG, "und");

    // tool:ffmpeg.encode — one input per track, each mapped to its own stream and tagged. Indices start after
    // the two streams, which is why an append-only list matters.
    let dir = temp_dir("mux");
    let (video, audio, out) = (dir.join("v.mp4"), dir.join("a.m4a"), dir.join("final.mp4"));
    let tracks = [
        ("de", Path::new("produce/clips/final.de.srt")),
        ("nl", Path::new("produce/clips/final.nl.srt")),
    ];
    let mut settings = Produce::default();
    settings.subtitles = Subtitles::TrackInFile;
    let argv = render::mux_command(&video, &audio, &tracks, &settings, &out, &list);
    let line = argv.join(" ");
    // Two streams plus one input per track: indices append-only, so a later filter's [N:a] still points at its
    // own file.
    assert_eq!(
        argv.iter().filter(|a| a.as_str() == "-i").count(),
        4,
        "{line}"
    );
    // Each track is tagged with its ISO-639-2 code and its name (§6: "so players name them"), never with the
    // stream's own index; an unlisted language falls back to `und` named by its upper-cased code.
    // Per-stream metadata: `-metadata:s:s:<n>` names the subtitle stream, its value follows.
    let tagged: Vec<String> = argv
        .windows(2)
        .filter(|w| w[0].starts_with("-metadata:s:s:") && w[1].starts_with("language="))
        .map(|w| format!("{}:{}", w[0].trim_start_matches("-metadata:"), w[1]))
        .collect();
    assert_eq!(
        tagged,
        ["s:s:0:language=deu", "s:s:1:language=und"],
        "{argv:?}"
    );
    // Per-stream metadata: `-metadata:s:s:<n>` names the subtitle stream, its value follows.
    let named: Vec<String> = argv
        .windows(2)
        .filter(|w| w[0].starts_with("-metadata:s:s:") && w[1].starts_with("title="))
        .map(|w| format!("{}:{}", w[0].trim_start_matches("-metadata:"), w[1]))
        .collect();
    assert_eq!(named, ["s:s:0:title=Deutsch", "s:s:1:title=NL"], "{argv:?}");
    // Burned-in asks for nothing in the container, and §5's rule holds whatever the caller handed as tracks.
    let burned =
        render::mux_command(&video, &audio, &tracks, &Produce::default(), &out, &list).join(" ");
    assert!(
        !burned.contains("-c:s"),
        "burned-in muxes no track: {burned}"
    );

    // webm carries no subtitle stream whatever the tick says, and §6 wants that said out loud afterwards.
    let webm = Produce {
        container: Container::Webm,
        subtitles: Subtitles::TrackInFile,
        ..Default::default()
    };
    let argv = render::mux_command(
        &video,
        &audio,
        &tracks,
        &webm,
        &dir.join("final.webm"),
        &list,
    );
    assert!(
        !argv.join(" ").contains("-c:s"),
        "no track survives a webm mux: {}",
        argv.join(" ")
    );
    assert!(
        !argv.join(" ").contains("-c:s"),
        "and no subtitle codec is even asked for"
    );
    assert_eq!(
        details::WEBM_SRT_LOG,
        "webm cannot carry an srt track — the subtitles are the files beside the video"
    );
    // Webm never reaches the codec question: the container rule has already answered "no tracks", which is why
    // the mux above maps nothing. mp4 takes mov_text, mkv takes srt.
    let (_, webm_tracks) =
        screen::apply_container_rules(Container::Webm, Codec::Vp9, Subtitles::TrackInFile);
    assert_eq!(webm_tracks, Subtitles::None);
    assert_eq!(render::subtitle_codec(Container::Mp4), "mov_text");
    assert_eq!(render::subtitle_codec(Container::Mkv), "srt");

    // An unknown stored subtitle mode reads as none in the video: guessing burned-in would letter a picture
    // nobody asked to letter. A stored resolution no longer offered falls back to the row's default.
    assert_eq!(details::stored_subtitle_mode(false), Subtitles::None);
    assert_eq!(details::stored_subtitle_mode(true), Subtitles::TrackInFile);
    assert_eq!(
        details::stored_resolution(false),
        Produce::default().resolution
    );
    assert_eq!(details::stored_resolution(true), project::Resolution::P1080);
    let _ = std::fs::remove_dir_all(&dir);
}

// ---- S5: legacy `produce` keys -------------------------------------------------------

// ---- S6: translation -----------------------------------------------------------------

#[test]
fn sec_08_produce_6_details_confirmed_against_the_code_verification_pass_s6_translation() {
    assert_eq!(
        ITEM,
        "§08-produce#6-details-confirmed-against-the-code-verification-pass"
    );
    // A cue's line breaks go out as " / " so one cue is one numbered entry, and come back as breaks.
    let two_lines = cue("first row\nsecond row", 0.0, 2.0);
    assert_eq!(subs::sent_text(&two_lines), "first row / second row");
    assert_eq!(
        subs::keep_breaks("erste zeile\nzweite zeile"),
        "erste zeile\nzweite zeile"
    );

    // A cue with no words is never counted missing nor re-asked: nothing was sent, so nothing can come back.
    let empty = cue("", 0.0, 2.0);
    assert!(subs::sent_text(&empty).is_empty());
    let original = vec![(1usize, "one".to_string()), (2, "two".to_string())];
    let mut batch = subs::Batch::new((0, 2));
    batch.translate_line(1, "eins").unwrap();
    assert_eq!(
        batch.missing(),
        vec![2],
        "only the line that was asked for and not answered"
    );

    // Answers are read back by number — tab, space, dot or colon after it — an unreadable line is skipped
    // rather than shifted up, and the first answer per number wins.
    assert_eq!(details::answer_number("1\teins"), Some((1, "eins")));
    assert_eq!(details::answer_number("2 zwei"), Some((2, "zwei")));
    assert_eq!(details::answer_number("3. drei"), Some((3, "drei")));
    assert_eq!(details::answer_number("4: vier"), Some((4, "vier")));
    assert_eq!(
        details::answer_number("this line has no number"),
        None,
        "skipped, never shifted"
    );
    let answers =
        details::read_answers("1 eins\nzwei without a number\n2 zwei later\n1 first wins");
    assert_eq!(answers.get(&1).map(String::as_str), Some("eins"));
    assert_eq!(answers.get(&2).map(String::as_str), Some("zwei later"));

    // Missing lines are named by number, and a track is never dropped for one bad line.
    let mut got = BTreeMap::new();
    got.insert(1, "eins".to_string());
    let merged = subs::merge("de", &original, &got);
    assert_eq!(merged.still_missing, vec![2]);
    assert_eq!(merged.lines.len(), 2);
    let warning = merged
        .warning
        .expect("the log names what stayed untranslated");
    // The wording is §6's, spelled by `produce_subtitles::left_in_original_log` and pinned there and in
    // `tests/suite/produce_rules.rs`; this check is that the merge warns at all and names the number.
    assert!(warning.contains("subtitles: de"), "{warning}");
    assert!(warning.contains('2'), "the line number is named: {warning}");

    // Only a complete track is cached: a partial answer cached is a partial answer forever.
    assert!(subs::cached(true));
    assert!(!subs::cached(false));

    // Wrapping rebalances at the middle and never drops a word (P.policy.subtitleRowChars = 42).
    let long = "the quick brown fox jumps over the lazy dog and then keeps running for a while";
    let wrapped = subs::wrap(long);
    assert_eq!(wrapped.lines().count(), 2, "{wrapped}");
    let mut kept: Vec<&str> = Vec::new();
    for row in wrapped.lines() {
        kept.extend(row.split_whitespace());
        assert!(
            row.chars().count() <= subs::ROW_CHARS,
            "row over the width: {row}"
        );
    }
    assert_eq!(
        kept,
        long.split_whitespace().collect::<Vec<&str>>(),
        "no word lost"
    );
    // A break the model chose is kept, not re-wrapped.
    assert_eq!(subs::wrap("one\n two"), "one\n two");

    // The track's last cue is extended to the readable minimum (P.policy.subtitleMinSeconds = 0.8), and the
    // breath between two cues is closed to P.policy.subtitleHoldSeconds = 1.2.
    let tail = subs::tidy(&[cue("the last line", 30.0, 30.2)]);
    assert_eq!(tail.len(), 1);
    assert!(
        (tail[0].e - tail[0].s - subs::MIN_SECONDS).abs() < 1e-9,
        "{:?}",
        tail[0]
    );
    let near = subs::tidy(&[cue("one", 0.0, 1.0), cue("two", 2.0, 3.0)]);
    assert_eq!(near[0].e, 2.0, "a gap under the hold is not a blank screen");
    let far = subs::tidy(&[cue("one", 0.0, 1.0), cue("two", 5.0, 6.0)]);
    assert_eq!(far[0].e, 1.0, "4 s of gap is a breath and stays one");
}

// ---- S7: frame boxes -----------------------------------------------------------------

// ---- S8: publish ---------------------------------------------------------------------

// ---- S9: the <video> tag -------------------------------------------------------------

// ---- S10: the stamp ------------------------------------------------------------------

// ---- this round: the three §6 rules that were only in a test before ---------------------

/// A `produce_exec::Materials` with nothing behind it: no lanes, no wav, no words, no translation reply.
/// Built the way `tests/suite/produce_render_flow.rs` builds its one door — the closures and the list they read are
/// leaked, because a per-test local cannot outlive the borrow `Materials` holds.
fn bare_materials(
    cues: naivepost::produce_exec::Cues,
) -> naivepost::produce_exec::Materials<'static> {
    static LANES: &[naivepost::cut::Lane] = &[];
    let exists: &'static dyn Fn(&str) -> bool = Box::leak(Box::new(|_file: &str| true));
    let fx_for: &'static dyn Fn(&naivepost::cut::Seg) -> Vec<naivepost::cut::Fx> =
        Box::leak(Box::new(|_seg: &naivepost::cut::Seg| Vec::new()));
    let wav_of: &'static dyn Fn(&naivepost::narration::Entry) -> Option<std::path::PathBuf> =
        Box::leak(Box::new(|_line: &naivepost::narration::Entry| None));
    let wav_seconds: &'static dyn Fn(&naivepost::narration::Entry) -> Option<f64> =
        Box::leak(Box::new(|_line: &naivepost::narration::Entry| Some(2.5)));
    let source_file: &'static dyn Fn(&naivepost::produce_render::Clip) -> Option<(String, f64)> =
        Box::leak(Box::new(|clip: &naivepost::produce_render::Clip| {
            Some(("/media/take.mp4".to_string(), clip.seg_s))
        }));
    let base_of: &'static dyn Fn(&naivepost::cut::Seg) -> String =
        Box::leak(Box::new(|_seg: &naivepost::cut::Seg| String::new()));
    let words: &'static dyn Fn(&naivepost::produce_render::Clip) -> Vec<subs::Word> =
        Box::leak(Box::new(|_clip: &naivepost::produce_render::Clip| {
            Vec::new()
        }));
    let translate: &'static naivepost::produce_translate::Ask = Box::leak(Box::new(
        |_lang: &str, wanted: &[usize]| -> Result<BTreeMap<usize, String>, String> {
            Ok(wanted
                .iter()
                .map(|number| (*number, format!("translated {number}")))
                .collect())
        },
    ));
    let sources: &'static dyn Fn() -> Vec<project::Source> = Box::leak(Box::new(Vec::new));
    let probe: &'static dyn Fn(&std::path::Path) -> Option<(i32, i32)> =
        Box::leak(Box::new(|_file: &std::path::Path| None));
    naivepost::produce_exec::Materials {
        exists,
        fx_for,
        lanes: LANES,
        src_shape: (1920, 1080),
        wav_of,
        source_file,
        on_recording: true,
        footage_left: &naivepost::produce_render::unknown_footage,
        filmed_runs: naivepost::produce_plan_logs::NO_TAPE,
        base_of,
        wav_seconds,
        cues,
        words,
        translate,
        sources,
        probe,
    }
}

/// The run a plan is asked over: one 10-second clip on a real tree, with whatever narration lines the caller
/// gives it (the cues `plan` builds come from those, not from the `Cues` it is handed).
fn plan_run(
    tree: &naivepost::layout::Tree,
    settings: Produce,
    lines: Vec<naivepost::narration::Entry>,
) -> naivepost::produce_flow::Run {
    naivepost::produce_flow::snapshot(
        Some(naivepost::cut::Cut {
            segs: vec![naivepost::cut::Seg {
                s: 0.0,
                e: 10.0,
                ..Default::default()
            }],
            aspect: "16:9".into(),
            ..Default::default()
        }),
        tree,
        lines,
        settings,
        &project::Project::default(),
        None,
        naivepost::narrate_screen::CAPTIONS.to_string(),
    )
}

/// §6 muxing, through the §6-named door: the known language is tagged with its ISO-639-2 code and named with
/// its proper name, the unlisted one with `und` and its upper-cased code, and never with the stream index.
#[test]
fn sec_08_produce_6_details_confirmed_against_the_code_verification_pass_s_mux_tags_each_track_by_code_and_name(
) {
    assert_eq!(
        ITEM,
        "§08-produce#6-details-confirmed-against-the-code-verification-pass"
    );
    let list = [
        language("de", "deu", "Deutsch"),
        language("fr", "fra", "Français"),
    ];
    let dir = temp_dir("mux-tags");
    let (video, audio, out) = (dir.join("v.mp4"), dir.join("a.m4a"), dir.join("final.mp4"));
    let tracks = [
        ("de", Path::new("produce/clips/final.de.srt")),
        ("nl", Path::new("produce/clips/final.nl.srt")),
    ];
    let settings = Produce {
        subtitles: Subtitles::TrackInFile,
        ..Default::default()
    };
    let argv = render::mux_command(&video, &audio, &tracks, &settings, &out, &list);
    // Per-stream metadata: `-metadata:s:s:<n>` names the subtitle stream, its value follows.
    let tagged: Vec<String> = argv
        .windows(2)
        .filter(|w| w[0].starts_with("-metadata:s:s:") && w[1].starts_with("language="))
        .map(|w| format!("{}:{}", w[0].trim_start_matches("-metadata:"), w[1]))
        .collect();
    // Per-stream metadata: `-metadata:s:s:<n>` names the subtitle stream, its value follows.
    let named: Vec<String> = argv
        .windows(2)
        .filter(|w| w[0].starts_with("-metadata:s:s:") && w[1].starts_with("title="))
        .map(|w| format!("{}:{}", w[0].trim_start_matches("-metadata:"), w[1]))
        .collect();
    // Asserted against `language_tag`'s own answers where the seam can be reused, so the mux and the rule the
    // inventory quotes cannot drift apart silently.
    let (de_tag, de_name) = details::language_tag("de", &list);
    let (nl_tag, nl_name) = details::language_tag("nl", &list);
    // A language spelled by its name is the same language: the page ticks names, the policy holds codes.
    assert_eq!(
        details::language_tag("German", &list),
        details::language_tag("de", &list),
        "a name and its code tag alike"
    );
    assert_eq!(de_tag, "deu");
    assert_eq!(de_name, "Deutsch");
    assert_eq!(nl_tag, details::UNDETERMINED_TAG);
    assert_eq!(nl_name, "NL");
    assert_eq!(
        tagged,
        [
            format!("s:s:0:language={de_tag}"),
            format!("s:s:1:language={nl_tag}")
        ],
        "{argv:?}"
    );
    assert_eq!(
        named,
        [
            format!("s:s:0:title={de_name}"),
            format!("s:s:1:title={nl_name}")
        ],
        "{argv:?}"
    );
    // The old shape, which said nothing: the stream's own index as its language.
    for index in 0..tracks.len() {
        assert!(
            !argv
                .iter()
                .any(|a| *a == format!("s:{index}:language={index}")),
            "a track tagged with its index names no language: {argv:?}"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// §6's "each subtitle track is its own input", asked of the live planner rather than of `mux_command`: a run
/// with an own-language sheet and two translations muxes three inputs, own language first, each mapped to its
/// own stream and tagged.
#[test]
fn sec_08_produce_6_details_confirmed_against_the_code_verification_pass_s_mux_one_input_per_subtitle_track(
) {
    assert_eq!(
        ITEM,
        "§08-produce#6-details-confirmed-against-the-code-verification-pass"
    );
    let (root, tree) = common::tree_in("mux-tracks");
    let settings = Produce {
        subtitles: Subtitles::TrackInFile,
        ..Default::default()
    };
    // The cue list a real press hands `plan`: §S4 builds it from the clip's own lines, so the run carries one
    // narration line inside its single 10-second clip and `plan` answers with one cue to put on the sheet.
    let run = plan_run(
        &tree,
        settings.clone(),
        vec![naivepost::narration::Entry {
            s: 1.0,
            e: 3.0,
            text: "a thing said".to_string(),
            ..Default::default()
        }],
    );
    let cues = naivepost::produce_exec::Cues {
        all: vec![cue("a thing said", 1.0, 3.0)],
        languages: vec!["de".to_string(), "fr".to_string()],
        session_language: "en".to_string(),
        ..Default::default()
    };
    let plan = naivepost::produce_exec::plan(&run, &tree, &bare_materials(cues));
    assert!(
        !plan.cues.all.is_empty(),
        "the run's own line reached the sheet, or there is nothing to mux"
    );
    // This container has no `SUBTITLE_LANGUAGES` to read, so every track lands on §6's fallback: `und`, named
    // by its upper-cased code. A listed language would carry its ISO-639-2 tag instead — pinned by the tagging
    // test through `produce_render::mux_command`, where the list is an argument.
    let argv = plan.mux.argv.join(" ");
    // The joined file once (its picture and its sound are one input) + one input per track.
    assert_eq!(
        plan.mux.argv.iter().filter(|a| a.as_str() == "-i").count(),
        4,
        "the joined file plus own language plus two translations: {argv}"
    );
    // Each track mapped from its own input, which follows the joined file's: input 1 + its index.
    for index in 0..3 {
        assert!(
            plan.mux
                .argv
                .windows(2)
                .any(|w| w == ["-map".to_string(), format!("{}:0", index + 1)]),
            "track {index} mapped: {argv}"
        );
    }
    // Own language first: the bare-stem sheet in the scratch folder, then `.de`, then `.fr` beside the video —
    // the same three files, in the same order, as the sidecars this run writes.
    let positions: Vec<usize> = ["final.srt", "final.de.srt", "final.fr.srt"]
        .iter()
        .map(|name| {
            plan.mux
                .argv
                .iter()
                .position(|a| a.ends_with(name))
                .unwrap_or_else(|| panic!("{name} is not an input of the mux: {argv}"))
        })
        .collect();
    assert_eq!(
        positions,
        {
            let mut sorted = positions.clone();
            sorted.sort();
            sorted
        },
        "the session's own track comes first: {argv}"
    );
    // And each of them is tagged, not left to a player to call "Subtitle 1".
    assert_eq!(
        plan.mux
            .argv
            .iter()
            .filter(|a| a.starts_with("language="))
            .count(),
        3,
        "every track carries a language tag: {argv}"
    );
    assert_eq!(
        plan.mux
            .argv
            .iter()
            .filter(|a| a.starts_with("title="))
            .count(),
        3,
        "and every track carries a name: {argv}"
    );
    // The translations carry their ISO-639-2 tag and name: with no `SUBTITLE_LANGUAGES` written, the app's own
    // three answer (the prototype's `subLangs`); the first rewrite tagged every one `und`.
    assert!(
        plan.mux
            .argv
            .windows(2)
            .any(|w| w == ["-metadata:s:s:1", "language=deu"]),
        "{argv}"
    );
    assert!(
        plan.mux
            .argv
            .windows(2)
            .any(|w| w == ["-metadata:s:s:1", "title=German"]),
        "{argv}"
    );
    assert!(
        plan.mux
            .argv
            .windows(2)
            .any(|w| w == ["-metadata:s:s:2", "title=French"]),
        "{argv}"
    );
    // The same tracks are the sidecars beside the video: one fact, two places.
    assert_eq!(
        plan.sidecars.len(),
        3,
        "{:?}",
        plan.sidecars
            .iter()
            .map(|s| &s.language)
            .collect::<Vec<_>>()
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// §6's webm half, on the live plan: the container keeps no text track, so the mux carries none whatever the
/// tick says, and the run says where the subtitles are instead.
#[test]
fn sec_08_produce_6_details_confirmed_against_the_code_verification_pass_s_webm_never_gets_a_text_track(
) {
    assert_eq!(
        ITEM,
        "§08-produce#6-details-confirmed-against-the-code-verification-pass"
    );
    let (root, tree) = common::tree_in("mux-webm");
    let settings = Produce {
        container: Container::Webm,
        subtitles: Subtitles::TrackInFile,
        ..Default::default()
    };
    let run = plan_run(
        &tree,
        settings.clone(),
        vec![naivepost::narration::Entry {
            s: 1.0,
            e: 3.0,
            text: "a thing said".to_string(),
            ..Default::default()
        }],
    );
    let cues = naivepost::produce_exec::Cues {
        all: vec![cue("a thing said", 1.0, 3.0)],
        languages: vec!["de".to_string()],
        session_language: "en".to_string(),
        ..Default::default()
    };
    let plan = naivepost::produce_exec::plan(&run, &tree, &bare_materials(cues));
    assert!(
        !plan.cues.all.is_empty(),
        "the run has something to say, which is what makes the note owed"
    );
    let argv = plan.mux.argv.join(" ");
    assert!(
        !argv.contains("-c:s"),
        "no track survives a webm mux: {argv}"
    );
    assert!(
        !argv.contains("-c:s"),
        "and no subtitle codec is even asked for: {argv}"
    );
    assert!(!argv.contains("final.de.srt"), "{argv}");
    assert!(
        plan.webm_without_track,
        "and the run knows to say why afterwards"
    );
    assert_eq!(
        details::WEBM_SRT_LOG,
        "webm cannot carry an srt track — the subtitles are the files beside the video"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// §6's stamp half: an unwritable stamp is said in §6's own words and never fails the run, while a writable one
/// still says the hash it wrote.
#[test]
fn sec_08_produce_6_details_confirmed_against_the_code_verification_pass_s_unwritable_stamp_says_so_and_never_fails_the_run(
) {
    assert_eq!(
        ITEM,
        "§08-produce#6-details-confirmed-against-the-code-verification-pass"
    );
    let clean_ran = naivepost::produce_exec::Ran {
        muxed: true,
        encoded: vec!["c000_x".into()],
        ..Default::default()
    };
    assert!(
        clean_ran.clean(),
        "a run that muxed and failed nothing may write its stamp"
    );

    // The writable case first, so the failure below cannot be confused with a broken argument.
    let (root, tree) = common::tree_in("stamp-ok");
    let plan = naivepost::produce_exec::Plan {
        stamp: "abc123".to_string(),
        ..plan_materials_plan(&tree)
    };
    let mut lines: Vec<String> = Vec::new();
    let wrote = naivepost::produce_exec::write_stamp(&plan, &tree, &clean_ran, |line| {
        lines.push(line.to_string())
    });
    assert!(wrote, "a writable stamp is written");
    assert!(
        lines.iter().any(|l| *l == stamp::wrote_stamp_log("abc123")),
        "the success line names the hash: {lines:?}"
    );
    assert_eq!(stamp::read_stamp(&tree).as_deref(), Some("abc123"));

    // Unwritable: the stamp path is taken by a DIRECTORY, so `fs::write` cannot answer with a file. Works for
    // any user, unlike a chmod, which root would simply ignore.
    std::fs::remove_file(tree.final_stamp()).unwrap();
    std::fs::create_dir_all(tree.final_stamp()).unwrap();
    let mut said: Vec<String> = Vec::new();
    let ok = naivepost::produce_exec::write_stamp(&plan, &tree, &clean_ran, |line| {
        said.push(line.to_string())
    });
    assert!(!ok, "an unwritable stamp is not a written stamp");
    assert!(
        said.iter().any(
            |l| l.starts_with("    produce: could not write the render stamp (")
                && l.ends_with("\u{2014} the next \u{25b6} will encode again")
        ),
        "the failure is said in §6's words: {said:?}"
    );
    assert!(
        said.iter()
            .any(|l| *l == details::stamp_unwritable_log(&path_error(&tree))),
        "and the sentence is the one §6 quotes, carrying the write's own error: {said:?}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// The message `fs::write` gives when the stamp path is a directory — the same string `write_stamp` forwards.
fn path_error(tree: &naivepost::layout::Tree) -> String {
    match std::fs::write(tree.final_stamp(), b"x") {
        Ok(()) => panic!("the stamp path was supposed to be occupied by a directory"),
        Err(why) => format!("{}: {why}", tree.final_stamp().display()),
    }
}

/// A `Plan` with only what `write_stamp` reads: the stamp hash, the video and its scratch folder.
fn plan_materials_plan(tree: &naivepost::layout::Tree) -> naivepost::produce_exec::Plan {
    naivepost::produce_exec::Plan {
        scratch_files: Vec::new(),
        speaking: false,
        captions_warning: None,
        speech: Vec::new(),
        clips: Vec::new(),
        plan_logs: Vec::new(),
        cues: naivepost::produce_exec::Cues::default(),
        burns: Vec::new(),
        encodes: Vec::new(),
        join: None,
        mux: naivepost::produce_exec::Command {
            step: "muxing",
            log: String::new(),
            argv: Vec::new(),
        },
        translated: Vec::new(),
        sidecars: Vec::new(),
        webm_without_track: false,
        stamp: String::new(),
        video: tree.final_video("mp4"),
        scratch: tree.clips_dir(),
        commands: Vec::new(),
    }
}

/// what §6 says and every OTHER stored row keeps its own value.
#[test]
fn sec_08_produce_6_details_confirmed_against_the_code_verification_pass_s_stored_subtitle_mode_and_resolution_fall_back_one_row_at_a_time(
) {
    assert_eq!(
        ITEM,
        "§08-produce#6-details-confirmed-against-the-code-verification-pass"
    );
    let (root, _tree) = common::tree_in("stored-words");

    // (i) + (ii) + (iii): a misspelt subtitle mode, with the rest of the block stated.
    std::fs::write(
        root.join("naivepost.json"),
        r#"{"produce": {"container": "mkv", "codec": "h265", "subtitles": "sidcar", "crf": 19, "game_volume": 0.4}}"#,
    )
    .unwrap();
    let (loaded, report) =
        project::load_report(&root).expect("a stale word does not stop the load");
    assert_eq!(
        loaded.produce.subtitles,
        Subtitles::None,
        "§6: an unknown mode reads as none in the video"
    );
    assert_eq!(
        loaded.produce.container,
        Container::Mkv,
        "the container row kept its value"
    );
    assert_eq!(loaded.produce.codec, Codec::H265, "and the codec's");
    assert_eq!(loaded.produce.crf, 19, "and the crf's");
    assert!(
        (loaded.produce.game_volume - 0.4).abs() < 1e-9,
        "and the bed's: {}",
        loaded.produce.game_volume
    );
    assert_eq!(
        loaded.produce.resolution,
        project::Resolution::P1080,
        "a row the file never named keeps the default"
    );
    assert!(
        report
            .iter()
            .any(|l| l.contains("\"sidcar\"") && l.contains("none in the video")),
        "the report names the stale word and what it became: {report:?}"
    );

    // An unoffered resolution, with a KNOWN subtitle mode beside it: only the resolution falls back.
    std::fs::write(
        root.join("naivepost.json"),
        r#"{"produce": {"container": "mkv", "subtitles": "trackinfile", "resolution": "4k", "crf": 19, "translate": ["de", "fr"], "vfr": true}}"#,
    )
    .unwrap();
    let (loaded, report) =
        project::load_report(&root).expect("a stale resolution does not stop the load");
    assert_eq!(
        loaded.produce.resolution,
        project::Resolution::P1080,
        "the row's default"
    );
    assert_eq!(
        loaded.produce.subtitles,
        Subtitles::TrackInFile,
        "and the good row is untouched"
    );
    assert_eq!(loaded.produce.crf, 19);
    assert_eq!(
        loaded.produce.translate,
        vec!["de".to_string(), "fr".to_string()]
    );
    assert!(
        loaded.produce.vfr,
        "the ticks survive a stale neighbour too"
    );
    assert!(
        report
            .iter()
            .any(|l| l.contains("\"4k\"") && l.contains("1080p")),
        "{report:?}"
    );

    // A current file: nothing fell back, so nothing is said.
    std::fs::write(
        root.join("naivepost.json"),
        r#"{"produce": {"container": "mkv", "subtitles": "trackinfile", "resolution": "720p", "crf": 21}}"#,
    )
    .unwrap();
    let (loaded, report) = project::load_report(&root).expect("a current project loads");
    assert_eq!(
        loaded.produce,
        Produce {
            container: Container::Mkv,
            subtitles: Subtitles::TrackInFile,
            resolution: project::Resolution::P720,
            crf: 21,
            ..Default::default()
        },
        "read exactly as written"
    );
    assert!(
        report.is_empty(),
        "a project that says nothing stale gets no notes: {report:?}"
    );

    // §6's legacy half, unchanged by any of this.
    std::fs::write(
        root.join("naivepost.json"),
        r#"{"produce": {"subs": "sidecar", "subs_from": "transcript", "bare": true, "game_vol": 0.0, "container": "webm"}}"#,
    )
    .unwrap();
    let (loaded, report) = project::load_report(&root).expect("a legacy project loads");
    assert_eq!(
        loaded.produce.subtitles,
        Subtitles::None,
        "subs: \"sidecar\" is none in the video"
    );
    assert!(
        (loaded.produce.game_volume - 0.0).abs() < 1e-9,
        "a stored 0 is silence"
    );
    assert!(
        !loaded.produce.blurred_edges,
        "bare is the negation of the blurred tick"
    );
    assert_eq!(loaded.produce.container, Container::Webm);
    assert!(
        report.iter().any(|l| l.contains("subs_from")),
        "subs_from is reported as dropped: {report:?}"
    );
    assert!(
        !report.iter().any(|l| l.contains("read row by row")),
        "no fallback was needed here: {report:?}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// The seam tie: what §6's two fallback functions answer IS what the loader applied, so a rule checked only in
/// isolation cannot pass this file.
#[test]
fn sec_08_produce_6_details_confirmed_against_the_code_verification_pass_s_stored_mode_is_asked_on_a_live_read(
) {
    assert_eq!(
        ITEM,
        "§08-produce#6-details-confirmed-against-the-code-verification-pass"
    );
    // The logic seam, spelled once.
    assert_eq!(details::stored_subtitle_mode(false), Subtitles::None);
    assert_eq!(details::stored_subtitle_mode(true), Subtitles::TrackInFile);
    assert_eq!(
        details::stored_resolution(false),
        project::Produce::default().resolution
    );
    let (root, _tree) = common::tree_in("stored-seam");
    std::fs::write(
        root.join("naivepost.json"),
        r#"{"produce": {"subtitles": "burnedin-but-spelt-wrong"}}"#,
    )
    .unwrap();
    let (loaded, _) = project::load_report(&root).expect("loads");
    assert_eq!(
        loaded.produce.subtitles,
        details::stored_subtitle_mode(false),
        "the loader applied the same door this test calls"
    );
    std::fs::write(
        root.join("naivepost.json"),
        r#"{"produce": {"resolution": "8k"}}"#,
    )
    .unwrap();
    let (loaded, _) = project::load_report(&root).expect("loads");
    assert_eq!(
        loaded.produce.resolution,
        details::stored_resolution(false),
        "and so for the resolution"
    );
    let _ = std::fs::remove_dir_all(&root);
}
