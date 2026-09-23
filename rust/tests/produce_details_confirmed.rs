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
use std::path::{Path, PathBuf};

use naivepost::cut::{Fx, Seg};
use naivepost::produce_data as data;
use naivepost::produce_details as details;
use naivepost::produce_embed as embed;
use naivepost::produce_render as render;
use naivepost::produce_screen as screen;
use naivepost::produce_stamp as stamp;
use naivepost::produce_subtitles as subs;
use naivepost::project::{self, Codec, Container, Produce, Subtitles};
use naivepost::render_fx;
use naivepost::roles::Language;

const ITEM: &str = "§08-produce#6-details-confirmed-against-the-code-verification-pass";

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("np-details-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn tree_in(dir: &Path) -> naivepost::layout::Tree {
    naivepost::layout::Tree::new(dir).expect("a project is a folder ending in .naivepost")
}

/// A language row as `SUBTITLE_LANGUAGES` spells it: code, ISO-639-2 tag, name.
fn language(code: &str, tag: &str, name: &str) -> Language {
    Language { code: code.to_string(), tag: tag.to_string(), name: name.to_string() }
}

fn cue(text: &str, s: f64, e: f64) -> subs::Cue {
    subs::Cue { text: text.to_string(), s, e, pos: String::new() }
}

// ---- S1: clip planning refusals and the rules behind them -------------------------

#[test]
fn sec_08_produce_6_details_confirmed_against_the_code_verification_pass_s1_planning_refusals() {
    assert_eq!(ITEM, "§08-produce#6-details-confirmed-against-the-code-verification-pass");
    // Every refusal names its clip and says what happened to it: a person reading this after the video came out
    // short has to find the moment in the timeline, not guess which of twelve clips moved.
    assert_eq!(
        details::copy_in_no_recording_log(3, 412.5),
        "clip 3 copies footage at 412.5 s that falls in no recording — skipped"
    );
    assert_eq!(
        details::copy_past_end_log(4, "media/talk.mkv", 6.251),
        "clip 4 copies past the end of media/talk.mkv — shortened to 6.3 s"
    );
    assert_eq!(
        details::sound_in_no_recording_log(5, 88.0),
        "clip 5 at 88.0 s falls in no recording — its sound has no picture, skipped"
    );
    assert_eq!(
        details::sound_past_end_log(6, "media/talk.mkv", 12.0),
        "clip 6 runs past the end of media/talk.mkv — shortened to 12.0 s"
    );
    // An insert that keeps the sound under it but has none to keep: silent, not an error.
    assert_eq!(details::insert_plays_silent_log(7), "clip 7 keeps the sound under it — it plays silent");

    // A clip uses its recording's first audio stream as its own sound only when that stream is selected; empty
    // means nothing was switched off, which is how a project predating the row reads.
    assert!(details::first_stream_is_own_sound(&[0, 1], 0));
    assert!(!details::first_stream_is_own_sound(&[1, 2], 0));
    assert!(details::first_stream_is_own_sound(&[], 0), "an empty list switched nothing off");
    let source = project::Source { path: "project:media/talk.mkv".into(), tracks: vec![1], ..Default::default() };
    assert!(!details::own_sound_from_source(&source));
    // Switched off at the source means heard through the lane mix instead.
    assert!(details::heard_via_lane_mix(&[1], 0));
    assert!(!details::heard_via_lane_mix(&[0], 0));
}

#[test]
fn sec_08_produce_6_details_confirmed_against_the_code_verification_pass_s1_bounds_drift_and_runs() {
    assert_eq!(ITEM, "§08-produce#6-details-confirmed-against-the-code-verification-pass");
    // machine.boundsToleranceSeconds: a line with no session span (a card, a held frame) is matched by exact
    // bounds, not by overlap — both sides are points, and half of nothing is nothing.
    assert!(details::bounds_match(10.0, 14.0, 10.0, 14.0));
    assert!(details::bounds_match(10.03, 14.02, 10.0, 14.0), "machine.boundsToleranceSeconds = 0.05");
    assert!(!details::bounds_match(10.06, 14.0, 10.0, 14.0), "0.06 s off is a different clip");
    // machine.driftSeconds: the read head opens only where sound would drift audibly.
    assert!(!details::drifts(30.0, 30.04));
    assert!(details::drifts(30.0, 30.05), "machine.driftSeconds = 0.05");
    assert!(details::drifts(30.0, 29.9));

    // The run itself is the render's own arithmetic: only a clip whose speed effect says `own`/`scene` keeps its
    // recording read at 1×, and its head dips in by the lane module's allowance.
    let mut speed = Fx::default();
    speed.kind = "speed".to_string();
    speed.snd = "own".to_string();
    let held: Vec<Fx> = vec![speed.clone()];
    let seg = Seg { s: 20.0, e: 30.0, ..Default::default() };
    assert_eq!(render::read_head_plan(&seg, &held), Some((20.0, 30.0)));
    assert_eq!(render::read_head_plan(&seg, &[]), None, "no speed effect: the read head stays shut");
    let opened = render_fx::read_head((20.0, 30.0));
    assert!(opened.0 > 20.0 && opened.1 < 30.0, "{opened:?}");

    // A card, a held frame or a clip on no recording closes the run: no continuous file under the sound.
    assert!(details::run_closes(false, true), "a card is not footage");
    assert!(details::run_closes(true, false), "footage with no recording behind it");
    assert!(!details::run_closes(true, true));

    // The camera's own refusal names the clip and the rate; §4 caps depth at 10× (render_fx::ZOOM_DEPTH_CAP).
    assert_eq!(
        details::fixed_rate_log("clip 2", 29.97),
        "clip 2: the moving camera needs a fixed frame rate — this clip is 29.97 fps"
    );
    assert_eq!(details::fixed_rate(), render_fx::ZOOM_GRID_FPS);

    // A line with no synthesis holds its cue to the next line, or to the clip's end when it is the last.
    assert_eq!(details::held_cue_end(4.0, Some(9.0), 20.0), 9.0);
    assert_eq!(details::held_cue_end(4.0, None, 12.0), 12.0);
    assert_eq!(details::held_cue_end(11.0, Some(9.0), 12.0), 11.0, "a cue never runs backwards");
    // And the cue survives with the voice missing: cues are built from lines whatever the wav did.
    let lines = vec![{
        let mut entry = naivepost::narration::Entry::default();
        entry.s = 1.0;
        entry.e = 3.0;
        entry.text = "a line nobody synthesised".to_string();
        entry
    }];
    let built = subs::cues(subs::Source::Footage, &lines, &[], 1.0);
    assert_eq!(built.len(), 1, "captioned only still has a cue");
}

// ---- S2: the planning log lines -----------------------------------------------------

#[test]
fn sec_08_produce_6_details_confirmed_against_the_code_verification_pass_s2_planning_log_lines() {
    assert_eq!(ITEM, "§08-produce#6-details-confirmed-against-the-code-verification-pass");
    // The four lines §6 lists beyond the refusals, verbatim.
    assert_eq!(details::stop_in_no_recording_log(125.0), "a stop at 125.0 s falls in no recording — its still is skipped");
    assert_eq!(details::no_entry_keeps_own_audio_log(9, 61.0), "clip 9 at 61.0 s has no narration entry — it keeps its own audio");
    assert_eq!(details::captioned_only_log(9), "clip 9: no synthesis for a line — it is captioned only");
    // The zoom cap is render_fx's line, reached through §6's wording; ffmpeg refuses past 10×.
    assert_eq!(render_fx::depth_log(4), "clip 4: the zoom goes deeper than ffmpeg's 10× — it is rendered at 10×");

    // A too-short clip and a vanished insert are logged by the planner itself, each with its own clip number.
    let mut short = Seg::default();
    short.s = 0.0;
    short.e = 0.4;
    fn no_fx(_: &Seg) -> Vec<Fx> {
        Vec::new()
    }
    let (clips, logs) = render::plan_clips(&[short], no_fx, "16:9", |_| true, &[], &[], (1920, 1080));
    assert!(clips.is_empty());
    assert_eq!(logs.len(), 1, "{logs:?}");
    assert!(logs[0].contains("clip 1"), "{}", logs[0]);

    // S1's four spoken-line steps are the shipped chain: stretched, resampled, panned, delayed whole ms.
    let clip = render::Clip { no: 1, seg_s: 0.0, on_screen: 6.0, rate: 2.0, ..Default::default() };
    let mut spoken = clip.clone();
    spoken.lines = vec![render::Placed { entry_index: 0, at: 0.5, tempo: 1.0, extend: 0.0, text: String::new(), speech: 2.0 }];
    let chain = details::spoken_line_filters(&spoken, &Produce::default(), 1, &[(2.0, 3.0)]).join(",");
    assert!(chain.contains("atempo="), "{chain}");
    let delay = chain.split(',').find(|f| f.starts_with("adelay=")).unwrap_or_else(|| panic!("{chain}"));
    for part in delay.trim_start_matches("adelay=").split('|') {
        let number = part.strip_suffix("ms").unwrap_or_else(|| panic!("{delay}"));
        assert!(number.parse::<i64>().is_ok(), "whole milliseconds only: {delay}");
    }
    // P.policy.gameVolume rides in the same chain as the bed's `volume=`.
    let bed = render::audio_chain(&spoken, &Produce::default(), 1, &[]).join(",");
    assert!(bed.contains("volume=0.22"), "{bed}");
    assert!((Produce::default().game_volume - 0.22).abs() < 1e-9, "P.policy.gameVolume");
}

// ---- S3: what produce/clips/ also holds ---------------------------------------------

#[test]
fn sec_08_produce_6_details_confirmed_against_the_code_verification_pass_s3_scratch_contents() {
    assert_eq!(ITEM, "§08-produce#6-details-confirmed-against-the-code-verification-pass");
    // tool:ffmpeg.encode writes all of this between its two passes and reads none of it back next time.
    assert_eq!(details::clip_cues(7, "a1b2c3"), "c007_a1b2c3.srt");
    assert_eq!(details::cue_title(7, 2), "c007_t02.svg");
    // A parameterised card is written out with its query resolved: ffmpeg reads a plain file.
    let copy = details::card_copy("assets/quote.svg?text=hello");
    assert!(copy.starts_with("quote") && copy.ends_with(".svg"), "{copy}");
    assert_ne!(copy, "quote.svg", "the copy is not the template");
    assert_eq!(details::animation_frames("loading.svg"), "loading.frames/");
    assert_eq!(details::joined("mp4"), "joined.mp4");
    assert_eq!(details::joined("webm"), "joined.webm");

    // Every kind lives under the scratch folder, whose name is §3's `clips/` group.
    assert_eq!(details::SCRATCH, "produce/clips/");
    assert_eq!(details::SCRATCH_KINDS.len(), 6, "{:?}", details::SCRATCH_KINDS);
    for kind in details::SCRATCH_KINDS {
        assert!(!kind.starts_with('/') && !kind.contains("final"), "{kind} is scratch, not output");
    }

    // Cleared at the start of every render — files go, the folder stays, because the planner already handed
    // ffmpeg paths inside it.
    let dir = temp_dir("scratch");
    let clips = dir.join("produce/clips");
    std::fs::create_dir_all(&clips).unwrap();
    std::fs::write(clips.join("c001.mp4"), "x").unwrap();
    std::fs::write(clips.join("c001_a1b2c3.srt"), "x").unwrap();
    std::fs::create_dir_all(clips.join("loading.frames")).unwrap();
    assert_eq!(details::clear(&clips).unwrap(), 2, "files only");
    assert!(clips.is_dir(), "the folder itself is not the sweep's business");
    assert_eq!(std::fs::read_dir(&clips).unwrap().count(), 1, "only the frames folder is left");
    let _ = std::fs::remove_dir_all(&dir);
}

// ---- S4: muxing ---------------------------------------------------------------------

#[test]
fn sec_08_produce_6_details_confirmed_against_the_code_verification_pass_s4_mux_and_stored_defaults() {
    assert_eq!(ITEM, "§08-produce#6-details-confirmed-against-the-code-verification-pass");
    let list = [language("de", "deu", "Deutsch"), language("fr", "fra", "Français")];
    // A listed language carries its ISO-639-2 tag and its own name, so a player's menu reads what the person ticked.
    assert_eq!(details::language_tag("de", &list), ("deu".to_string(), "Deutsch".to_string()));
    // An unlisted one has no tag to carry: `und`, named by its upper-cased code so it still reads as a label.
    assert_eq!(details::language_tag("nl", &list), ("und".to_string(), "NL".to_string()));
    assert_eq!(details::UNDETERMINED_TAG, "und");

    // tool:ffmpeg.encode — one input per track, each mapped to its own stream and tagged. Indices start after
    // the two streams, which is why an append-only list matters.
    let dir = temp_dir("mux");
    let (video, audio, out) = (dir.join("v.mp4"), dir.join("a.m4a"), dir.join("final.mp4"));
    let tracks = [("de", Path::new("produce/clips/final.de.srt")), ("nl", Path::new("produce/clips/final.nl.srt"))];
    let mut settings = Produce::default();
    settings.subtitles = Subtitles::TrackInFile;
    let argv = details::mux(&video, &audio, &tracks, &settings, &out);
    let line = argv.join(" ");
    // Two streams plus one input per track: indices append-only, so a later filter's [N:a] still points at its
    // own file.
    assert_eq!(argv.iter().filter(|a| a.as_str() == "-i").count(), 4, "{line}");
    let tagged: Vec<&str> = argv.iter().filter(|a| a.starts_with("s:") && a.contains("language=")).map(String::as_str).collect();
    assert_eq!(tagged, ["s:0:language=0", "s:1:language=1"], "{argv:?}");
    // Burned-in asks for nothing in the container, and §5's rule holds whatever the caller handed as tracks.
    let burned = details::mux(&video, &audio, &tracks, &Produce::default(), &out).join(" ");
    assert!(!burned.contains("-map s:"), "burned-in muxes no track: {burned}");

    // webm carries no subtitle stream whatever the tick says, and §6 wants that said out loud afterwards.
    let mut webm = Produce::default();
    webm.container = Container::Webm;
    webm.subtitles = Subtitles::TrackInFile;
    let argv = details::mux(&video, &audio, &tracks, &webm, &dir.join("final.webm"));
    assert!(!argv.join(" ").contains("-map s:"), "no track survives a webm mux: {}", argv.join(" "));
    assert!(!argv.join(" ").contains("-c:s"), "and no subtitle codec is even asked for");
    assert_eq!(details::WEBM_SRT_LOG, "webm cannot carry an srt track — the subtitles are the files beside the video");
    // Webm never reaches the codec question: the container rule has already answered "no tracks", which is why
    // the mux above maps nothing. mp4 takes mov_text, mkv takes srt.
    let (_, webm_tracks) = screen::apply_container_rules(Container::Webm, Codec::Vp9, Subtitles::TrackInFile);
    assert_eq!(webm_tracks, Subtitles::None);
    assert_eq!(render::subtitle_codec(Container::Mp4), "mov_text");
    assert_eq!(render::subtitle_codec(Container::Mkv), "srt");

    // An unknown stored subtitle mode reads as none in the video: guessing burned-in would letter a picture
    // nobody asked to letter. A stored resolution no longer offered falls back to the row's default.
    assert_eq!(details::stored_subtitle_mode(false), Subtitles::None);
    assert_eq!(details::stored_subtitle_mode(true), Subtitles::TrackInFile);
    assert_eq!(details::stored_resolution(false), Produce::default().resolution);
    assert_eq!(details::stored_resolution(true), project::Resolution::P1080);
    let _ = std::fs::remove_dir_all(&dir);
}

// ---- S5: legacy `produce` keys -------------------------------------------------------

#[test]
fn sec_08_produce_6_details_confirmed_against_the_code_verification_pass_s5_legacy_produce_keys() {
    assert_eq!(ITEM, "§08-produce#6-details-confirmed-against-the-code-verification-pass");
    // `subs: "sidecar"` was "nothing in the video, the files beside it do the work" — today's none. An .srt is
    // written beside the video whatever the row says, so nothing is lost by reading it as silence in the file.
    assert_eq!(details::legacy_subtitles("sidecar"), None);
    assert_eq!(details::legacy_subtitles("burned"), Some(Subtitles::BurnedIn));
    assert_eq!(details::legacy_subtitles("track"), Some(Subtitles::TrackInFile));
    // A word this app has never written is also none: an unknown answer must not letter somebody's picture.
    assert_eq!(details::legacy_subtitles("karaoke"), None);

    // `subs_from` is dropped, not migrated: where a translation starts is recomputed from what the file still says.
    assert_eq!(details::DROPPED_PRODUCE_KEYS, ["subs_from"]);

    // Absent `game_vol` is the default bed (P.policy.gameVolume = 0.22); a stored 0 is silence. The two must not
    // collapse into each other, which is why the input is an Option and not a plain f64.
    assert!((details::legacy_game_volume(None) - 0.22).abs() < 1e-9);
    assert_eq!(details::legacy_game_volume(Some(0.0)), 0.0, "stored silence stays silent");
    assert!((details::legacy_game_volume(Some(0.6)) - 0.6).abs() < 1e-9);

    // `bare` was "no blurred edges" before the tick was inverted; it reads once as its negation.
    assert!(details::legacy_bare(false));
    assert!(!details::legacy_bare(true));
}

// ---- S6: translation -----------------------------------------------------------------

#[test]
fn sec_08_produce_6_details_confirmed_against_the_code_verification_pass_s6_translation() {
    assert_eq!(ITEM, "§08-produce#6-details-confirmed-against-the-code-verification-pass");
    // A cue's line breaks go out as " / " so one cue is one numbered entry, and come back as breaks.
    let two_lines = cue("first row\nsecond row", 0.0, 2.0);
    assert_eq!(subs::sent_text(&two_lines), "first row / second row");
    assert_eq!(subs::keep_breaks("erste zeile\nzweite zeile"), "erste zeile\nzweite zeile");

    // A cue with no words is never counted missing nor re-asked: nothing was sent, so nothing can come back.
    let empty = cue("", 0.0, 2.0);
    assert!(subs::sent_text(&empty).is_empty());
    let original = vec![(1usize, "one".to_string()), (2, "two".to_string())];
    let mut batch = subs::Batch::new((0, 2));
    batch.translate_line(1, "eins").unwrap();
    assert_eq!(batch.missing(), vec![2], "only the line that was asked for and not answered");

    // Answers are read back by number — tab, space, dot or colon after it — an unreadable line is skipped
    // rather than shifted up, and the first answer per number wins.
    assert_eq!(details::answer_number("1\teins"), Some((1, "eins")));
    assert_eq!(details::answer_number("2 zwei"), Some((2, "zwei")));
    assert_eq!(details::answer_number("3. drei"), Some((3, "drei")));
    assert_eq!(details::answer_number("4: vier"), Some((4, "vier")));
    assert_eq!(details::answer_number("this line has no number"), None, "skipped, never shifted");
    let answers = details::read_answers("1 eins\nzwei without a number\n2 zwei later\n1 first wins");
    assert_eq!(answers.get(&1).map(String::as_str), Some("eins"));
    assert_eq!(answers.get(&2).map(String::as_str), Some("zwei later"));

    // Missing lines are named by number, and a track is never dropped for one bad line.
    let mut got = BTreeMap::new();
    got.insert(1, "eins".to_string());
    let merged = subs::merge("de", &original, &got);
    assert_eq!(merged.still_missing, vec![2]);
    assert_eq!(merged.lines.len(), 2);
    let warning = merged.warning.expect("the log names what stayed untranslated");
    assert_eq!(warning, "!!! subtitles: de: line(s) 2 stayed in the original");

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
        assert!(row.chars().count() <= subs::ROW_CHARS, "row over the width: {row}");
    }
    assert_eq!(kept, long.split_whitespace().collect::<Vec<&str>>(), "no word lost");
    // A break the model chose is kept, not re-wrapped.
    assert_eq!(subs::wrap("one\n two"), "one\n two");

    // The track's last cue is extended to the readable minimum (P.policy.subtitleMinSeconds = 0.8), and the
    // breath between two cues is closed to P.policy.subtitleHoldSeconds = 1.2.
    let tail = subs::tidy(&[cue("the last line", 30.0, 30.2)]);
    assert_eq!(tail.len(), 1);
    assert!((tail[0].e - tail[0].s - subs::MIN_SECONDS).abs() < 1e-9, "{:?}", tail[0]);
    let near = subs::tidy(&[cue("one", 0.0, 1.0), cue("two", 2.0, 3.0)]);
    assert_eq!(near[0].e, 2.0, "a gap under the hold is not a blank screen");
    let far = subs::tidy(&[cue("one", 0.0, 1.0), cue("two", 5.0, 6.0)]);
    assert_eq!(far[0].e, 1.0, "4 s of gap is a breath and stays one");
}

// ---- S7: frame boxes -----------------------------------------------------------------

#[test]
fn sec_08_produce_6_details_confirmed_against_the_code_verification_pass_s7_frame_boxes() {
    assert_eq!(ITEM, "§08-produce#6-details-confirmed-against-the-code-verification-pass");
    // No aspect: 16:9 at long side 1280 (P.machine.thumbnailLongSide).
    assert_eq!(details::thumbnail_box_without_aspect(), (1280, 720));
    assert_eq!(screen::THUMB_LONG_SIDE, 1280);
    // A tall picture is 1280 the other way, not 1280 turned sideways.
    assert_eq!(screen::thumb_box("9:16", 0, 0), (720, 1280));

    // Nothing derivable at all: the chosen height at 16:9, and 1080 when even that is unset.
    assert_eq!(details::frame_box_from_nothing(0), (1920, 1080));
    assert_eq!(details::frame_box_from_nothing(720), (1280, 720));
    assert_eq!(render_fx::TIER_SHORT_SIDE, 1080);

    // Both sides always even — 4:2:0 refuses a chroma sample on an odd edge.
    for (wide, high) in [(1u32, 1u32), (1919, 1079), (1080, 1920), (3, 3)] {
        let box_ = screen::thumb_box(&format!("{wide}:{high}"), 0, 0);
        assert_eq!(box_.0 % 2, 0, "{wide}:{high} -> {box_:?}");
        assert_eq!(box_.1 % 2, 0, "{wide}:{high} -> {box_:?}");
    }
    for side in [1i32, 7, 1079, 1080, 1920] {
        let box_ = details::frame_box_from_nothing(side);
        assert_eq!(box_.0 % 2, 0, "{side} -> {box_:?}");
        assert_eq!(box_.1 % 2, 0, "{side} -> {box_:?}");
    }
    // The render's own box keeps the footage's shape when no aspect is set.
    assert_eq!(render_fx::frame_box("", 1920, 1080), (1920, 1080));
}

// ---- S8: publish ---------------------------------------------------------------------

#[test]
fn sec_08_produce_6_details_confirmed_against_the_code_verification_pass_s8_publish_legacy_and_gate() {
    assert_eq!(ITEM, "§08-produce#6-details-confirmed-against-the-code-verification-pass");
    // The old base-by-index answer becomes the frame list's order: that frame moves to the front, because the
    // first frame is the one the image model edits. An index naming no *other* frame moves nothing.
    let frames = vec!["a".to_string(), "b".to_string(), "c".to_string()];
    assert_eq!(details::apply_legacy_base(&frames, 2), ["c", "a", "b"]);
    assert_eq!(details::apply_legacy_base(&frames, 0), frames, "0 already names the front");
    assert_eq!(details::apply_legacy_base(&frames, 9), frames, "past the end names nothing");

    // `title_off` is read once and then retires: reading it twice would let a retired key overrule an edit.
    assert_eq!(details::title_off_reads_once(Some(true), false), Some(true));
    assert_eq!(details::title_off_reads_once(Some(true), true), None);

    // A one-line `frame: …` instruction drew nothing and said nothing, so it is cleared on load.
    assert_eq!(details::clear_frame_only_instruction("frame: clip 2 +4"), "");
    assert_eq!(details::clear_frame_only_instruction("make it look like a lecture hall"), "make it look like a lecture hall");

    // An older `<project>/publish/` folder wins for as long as the new one does not exist — and nothing is
    // migrated, because a person's own folder is not this app's to move.
    let dir = temp_dir("publish");
    let project = dir.join("demo.naivepost");
    std::fs::create_dir_all(project.join("publish")).unwrap();
    let tree = tree_in(&project);
    assert_eq!(tree.publish_dir(), project.join("publish"));
    assert!(details::publish_folder_wins(true, false));
    assert!(!details::publish_folder_wins(true, true), "the new place wins once it exists");
    std::fs::create_dir_all(project.join("produce/publish")).unwrap();
    assert_eq!(tree.publish_dir(), project.join("produce/publish"));
    let _ = std::fs::remove_dir_all(&dir);

    // The deadlock breaker: text gate closed with no picture, no images and no instruction can never open by
    // itself, so it asks again rather than sitting shut forever.
    assert!(details::asks_for_text_again(false, 0, "   "));
    assert!(!details::asks_for_text_again(true, 0, ""), "a picture is what the gate waits for");
    assert!(!details::asks_for_text_again(false, 1, ""), "one image is enough to draw from");
    assert_eq!(
        details::DEADLOCK_LOG,
        "    publish: no picture, no images and no instruction — asking for the upload text again"
    );
}

#[test]
fn sec_08_produce_6_details_confirmed_against_the_code_verification_pass_s8_publish_frame_and_draw() {
    assert_eq!(ITEM, "§08-produce#6-details-confirmed-against-the-code-verification-pass");
    // A chosen frame says which moment it is and promises no model and no GPU.
    assert_eq!(
        details::chosen_frame_log(75.0),
        "    publish: the thumbnail is the frame at 01:15, as it is — no model, no GPU"
    );
    assert_eq!(details::chosen_frame_failed_log("the copy is gone"), "    publish: the copy is gone -- the thumbnail is drawn instead");

    // The prototype's `frame:` line: a clip plus an offset, or a bare second / mm:ss. An unknown clip or a
    // negative sum names no frame — never "take the first one", because a wrong picture on a video's page is
    // worse than a missing one.
    let starts = [0.0, 30.0, 61.5];
    let clip = details::frame_line("frame: clip 3 +12.5").expect("a clip reference");
    assert_eq!(details::frame_moment(clip, &starts), Some(74.0));
    assert_eq!(details::frame_moment(details::frame_line("frame: 12").unwrap(), &starts), Some(12.0));
    assert_eq!(details::frame_moment(details::frame_line("frame: 1:02").unwrap(), &starts), Some(62.0));
    assert!(details::frame_line("frame: clip 9 +1").is_some());
    assert_eq!(details::frame_moment(details::frame_line("frame: clip 9 +1").unwrap(), &starts), None, "no such clip");
    assert!(details::frame_line("frame: clip 2 -5").is_none(), "a negative sum names nothing");
    assert!(details::frame_line("a bright room, wide").is_none(), "not a frame reference at all");

    // A `THUMBNAIL: frame: …` line is a frame and its instruction is discarded.
    assert!(details::thumbnail_line_is_frame("THUMBNAIL: frame: clip 1 +3"));
    assert!(!details::thumbnail_line_is_frame("THUMBNAIL: a bright room, wide"));

    // Labelled lines peel in either order, up to three (machine.labelledLinesMax), quotes stripped, and a
    // whole-reply fence is removed before any of it.
    let (labels, rest) = details::peel_labels("TITLE: \"Dust II\"\nTEXT: hold the frame\nmore prose here");
    assert_eq!(labels, ["Dust II", "hold the frame"]);
    assert_eq!(rest, "more prose here");
    let (flipped, _) = details::peel_labels("TEXT: second\nTITLE: first");
    assert_eq!(flipped, ["second", "first"], "either order");
    let (capped, leftover) = details::peel_labels("AAAA: one\nBBBB: two\nCCCC: three\nDDDD: four");
    assert_eq!(capped.len(), 3, "machine.labelledLinesMax = 3");
    assert_eq!(leftover, "DDDD: four", "a fourth label is prose by then");
    let (fenced, _) = details::peel_labels("```\nTITLE: fenced answer\n```");
    assert_eq!(fenced, ["fenced answer"], "the fence goes first, or it reads as a label");

    // A first description line under 40 chars ending in ":" is a heading the model wrote for itself.
    let short_heading = "Chapters:";
    assert!(short_heading.len() < details::DESCRIPTION_HEADING_CHARS);
    assert!(details::drops_description_heading(short_heading));
    let exact = format!("{}:", "x".repeat(details::DESCRIPTION_HEADING_CHARS - 1));
    assert_eq!(exact.chars().count(), details::DESCRIPTION_HEADING_CHARS, "40 chars is a sentence");
    assert!(!details::drops_description_heading(&exact));
    assert!(!details::drops_description_heading("Chapters of this session"));

    // Candidate frames: the middles of three equal bands over the kept footage — never a band's first or last
    // frame, and never a moment an insert covers. P.policy.publishFrames = 3 is the same count, and
    // P.eng.publishMaxFrames stays the row's ceiling above it.
    assert_eq!(screen::FIRST_IMAGES, 3, "P.policy.publishFrames");
    assert_eq!(screen::MAX_IMAGES, 8, "P.eng.publishMaxFrames");
    let kept = details::candidate_frames(&[(0.0, 30.0)], &[1.0, 2.0]);
    assert_eq!(kept.len(), 3, "{kept:?}");
    for at in &kept {
        assert!(*at > 0.0 && *at < 30.0, "never the first or last frame: {at}");
    }
    assert!(details::under_insert(12.0, &[(10.0, 20.0)]));
    assert!(!details::under_insert(9.0, &[(10.0, 20.0)]));
    // Too small a kept pool hands the whole extracted set back rather than nothing.
    assert_eq!(details::candidate_frames(&[], &[3.0, 4.0]), vec![3.0, 4.0]);
    assert_eq!(details::NO_FRAMES_LOG, "    publish: no frames extracted either — drawing from the instruction alone");

    // Drawing: a missing base is an error, a missing reference is skipped and logged.
    assert_eq!(details::base_gone_error("/tmp/base.png"), "the base image is gone: /tmp/base.png");
    assert_eq!(details::reference_skipped_log("/tmp/ref.png"), "    publish: /tmp/ref.png is not there — sent without it");

    // The request line carries everything needed to tell "it ignored my crop" from "the crop went wrong".
    let line = details::request_log(1280, 720, "/tmp/base.png", 3, 60, 0.5, 0.4);
    assert_eq!(line, "    publish: 1280x720 editing /tmp/base.png, 3 image(s) sent, base cropped to 60% of its width around 0.50,0.40");
    assert_eq!(details::REQUEST_NO_IMAGES_LOG, "    publish: drawn from the instruction alone, no images");

    // "Already drawn" needs both thumbnail files *and* a stamp match; one file is half a picture.
    assert!(details::already_drawn(true, true));
    assert!(!details::already_drawn(false, true), "the plain file is what printing failed back to");
    assert!(!details::already_drawn(true, false), "something the draw reads moved");

    // The chosen-picture branch prints its words before the text-only gate, so ↻ Suggest picking a frame still
    // gets its title; a printing failure keeps the plain picture rather than losing the frame too.
    assert!(details::prints_words_first(true, false));
    assert!(!details::prints_words_first(true, true), "no chosen frame means the gate applies");
    assert!(details::print_failed_keeps_plain(true));

    // The title band is a box only while the picture carries words; Remove keeps it off for good; and the
    // YouTube title never re-prints the picture.
    assert!(details::title_band_is_box(true));
    assert!(!details::title_band_is_box(false));
    assert!(details::REMOVE_IS_PERMANENT.contains("off for good"), "{}", details::REMOVE_IS_PERMANENT);
    assert!(!details::youtube_title_redraws_picture());
}

#[test]
fn sec_08_produce_6_details_confirmed_against_the_code_verification_pass_s8_publish_text_boxes() {
    assert_eq!(ITEM, "§08-produce#6-details-confirmed-against-the-code-verification-pass");
    // Only the ✎ chip opens the dialog, and its hint is §A's own sentence.
    assert!(details::opens_text_dialog(true));
    assert!(!details::opens_text_dialog(false));
    assert_eq!(
        details::TEXT_CHIP_HINT,
        "Printed to fill the box you marked — a longer line comes out smaller, and Enter starts a new line."
    );

    // A drag under 8 px (preview.dragMinPx) marks nothing, so tapping inside a box you just drew keeps it.
    assert!(!details::drag_marks(7.9));
    assert!(details::drag_marks(8.0));
    assert_eq!(details::DRAG_MIN_PX, 8.0);

    // A box saved empty is not created, and an existing one emptied is removed: an empty text effect would be a
    // row that draws nothing and has to be hunted down before the row is usable again.
    assert_eq!(details::box_saved_empty(false), details::BoxSave::NotCreated);
    assert_eq!(details::box_saved_empty(true), details::BoxSave::Removed);

    // Nothing to print means the plain bytes go through untouched — no re-encode, no trace of an empty box.
    assert!(details::copies_plain_bytes("   "));
    assert!(!details::copies_plain_bytes("a line"));
}

// ---- S9: the <video> tag -------------------------------------------------------------

#[test]
fn sec_08_produce_6_details_confirmed_against_the_code_verification_pass_s9_video_tag() {
    assert_eq!(ITEM, "§08-produce#6-details-confirmed-against-the-code-verification-pass");
    let list = [language("de", "deu", "Deutsch"), language("fr", "fra", "Français")];
    // Tracks come off disk, so the folder's contents are the page's track list.
    let dir = temp_dir("tag");
    std::fs::write(dir.join("final.vtt"), "WEBVTT\n").unwrap();
    std::fs::write(dir.join("final.de.vtt"), "WEBVTT\n").unwrap();
    std::fs::write(dir.join("final.fr.vtt"), "WEBVTT\n").unwrap();
    std::fs::write(dir.join("final.srt"), "").unwrap();
    let found = details::tracks_from_disk(&dir);
    assert_eq!(found.len(), 3, "only .vtt files: {found:?}");

    // Own language first with `default` on the first only; the rest by name, because that is what a player's
    // menu shows and an alphabetical file sort would offer German as the default.
    let built = embed::tracks(&found, "en", &list);
    assert_eq!(built[0].code, "en", "{:?}", built[0]);
    let names: Vec<&str> = built.iter().skip(1).map(|track| track.name.as_str()).collect();
    assert_eq!(names, ["Deutsch", "Français"], "{names:?}");
    let page = embed::tag("final.mp4", Some("final.jpg"), &built);
    assert_eq!(page.matches(" default").count(), 1, "{page}");
    assert!(page.starts_with("<video poster=\"final.jpg\" controls src=\"final.mp4\" preload=\"none\">"), "{page}");

    // A caption file deleted since the last run drops out of the page instead of shipping a broken <track>.
    std::fs::remove_file(dir.join("final.de.vtt")).unwrap();
    let after = embed::tracks(&details::tracks_from_disk(&dir), "en", &list);
    assert_eq!(after.len(), 2, "{after:?}");
    // And the tag is rewritten even when the encode was skipped: it lists what survived on disk.
    assert!(data::tag_rewritten(true));

    // Both web notes, verbatim — each names the browser to blame and the format to move to.
    assert_eq!(details::web_note(Container::Mkv, Codec::H264), Some("no browser plays Matroska — render to mp4 or webm for a page"));
    let h265 = details::web_note(Container::Mp4, Codec::H265).expect("h265 is a note of its own");
    assert!(h265.starts_with("Firefox plays no h265 at all"), "{h265}");
    assert!(h265.contains("h264 is the one that plays everywhere"), "{h265}");
    assert_eq!(details::web_note(Container::Mp4, Codec::H264), None, "a page's own formats say nothing");

    // An unwritable poster is logged and the tag written without one: a page with no picture still plays.
    let without = embed::tag("final.mp4", None, &built);
    assert!(!without.contains("poster="), "{without}");
    assert!(details::poster_missing_reason().len() > 8, "the log gives a reason");
    assert!(details::poster_unwritable_log("/tmp/final.jpg").contains("/tmp/final.jpg"));

    // The page lives beside the video (tool:ffmpeg.encode's own folder).
    let tree = tree_in(&dir.join("demo.naivepost"));
    assert_eq!(details::page_path(&tree, Container::Mp4).file_name().unwrap(), "final.html");
    let _ = std::fs::remove_dir_all(&dir);
}

// ---- S10: the stamp ------------------------------------------------------------------

#[test]
fn sec_08_produce_6_details_confirmed_against_the_code_verification_pass_s10_stamp() {
    assert_eq!(ITEM, "§08-produce#6-details-confirmed-against-the-code-verification-pass");
    // A stamp only ever lets a run skip work it can prove was done: uncomputable or unwritable both read as
    // *not up to date*, which costs one encode and never hides a change.
    assert!(!details::stamp_matches(false, Some("abc"), "abc"), "uncomputable never matches");
    assert!(!details::stamp_matches(true, None, "abc"), "never produced");
    assert!(!details::stamp_matches(true, Some("abd"), "abc"));
    assert!(details::stamp_matches(true, Some("abc"), "abc"));

    // The write failure is a log line, not a failure: the encode it describes already succeeded.
    assert_eq!(
        details::stamp_unwritable_log("Read-only file system"),
        "    produce: could not write the render stamp (Read-only file system) — the next ▶ will encode again"
    );
    assert!(!details::stamp_failure_fails_run());

    // An unwritable folder returns Err from the writer and is reported, never panicked on and never swallowed:
    // a caller that cannot store the verdict simply encodes again next time.
    let dir = temp_dir("stamp");
    let tree = tree_in(&dir.join("demo.naivepost"));
    stamp::write_stamp(&tree, "abc").expect("the folder is writable here");
    assert_eq!(stamp::read_stamp(&tree).as_deref(), Some("abc"));
    std::fs::write(stamp::stamp_path(&tree), "\n").unwrap();
    assert_eq!(stamp::read_stamp(&tree), None, "an empty stamp proves nothing");
    assert!(!details::stamp_matches(true, stamp::read_stamp(&tree).as_deref(), "abc"));
    let _ = std::fs::remove_dir_all(&dir);
}
