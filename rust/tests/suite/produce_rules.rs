//! §08-produce#5-rules — `spec/08-produce.md` §5, checked against the modules that hold each rule.
//!
//! §5 is four bullets rather than a set of steps, so one test per bullet (plus a second where a bullet holds
//! rules that can break apart on their own). Most of what it states was implemented by the flows around it —
//! F5.2's encode and join, F5.4's cues, F5.3's stamp, F5.5's tag — so these tests mostly pin *that* the rule
//! still holds when everything around it changes: the frame box on every clip, the format tail on every
//! branch, `-ss` before `-i`, `-t` after every input, whole milliseconds in `adelay`, append-only input slots.
//!
//! Ids used: `P.eng.minClipSeconds` (0.5, the single clip floor), `P.policy.gameVolume` (0.22, the bed whose
//! `volume=` and `adelay=` sit in the audio chain). Tools: `tool:ffmpeg.encode` — every command line here is
//! what that tool is sent.

use std::collections::BTreeMap;

use naivepost::cut::{Fx, Seg};
use naivepost::layout::Tree;
use naivepost::narration::Entry;
use naivepost::produce_data as data;
use naivepost::produce_render as render;
use naivepost::produce_screen as screen;

use naivepost::produce_stamp as stamp;
use naivepost::produce_subtitles as subs;
use naivepost::project::{self, Codec, Container, Produce, Subtitles};

const ITEM: &str = "§08-produce#5-rules";

fn seg(s: f64, e: f64) -> Seg {
    Seg {
        s,
        e,
        ..Default::default()
    }
}

fn no_fx(_: &Seg) -> Vec<Fx> {
    Vec::new()
}

fn settings() -> Produce {
    Produce::default()
}

/// A clip with one spoken line, which is what puts `adelay` and the bed's `volume=` in the chain.
fn clip_with_line(at: f64) -> render::Clip {
    render::Clip {
        no: 1,
        seg_s: 0.0,
        on_screen: 8.0,
        rate: 1.0,
        frame: (1920, 1080),
        lines: vec![render::Placed {
            entry_index: 0,
            at,
            tempo: 1.0,
            extend: 0.0,
            text: String::new(),
            speech: 2.0,
        }],
        ..Default::default()
    }
}

// ---- bullet 1: the join is a stream copy -------------------------------------------------

#[test]
fn sec_08_produce_5_rules_s1_every_clip_comes_out_at_one_size_and_layout() {
    assert_eq!(ITEM, "§08-produce#5-rules");
    // tool:ffmpeg.encode joins the clips with `-c copy`, so nothing may differ between them: not the frame
    // size, not the sample rate, not the channel layout, not whether a limiter ran.
    let clip = render::Clip {
        no: 1,
        seg_s: 0.0,
        on_screen: 8.0,
        rate: 1.0,
        ..Default::default()
    };
    for frame in [(1920, 1080), (1080, 1920)] {
        let boxed = render::Clip {
            frame,
            ..clip.clone()
        };
        let video = render::video_chain(&boxed, &settings(), None).join(",");
        assert!(video.contains("scale="), "{video}");
        assert!(video.contains("pad=") || video.contains("crop="), "{video}");
        assert!(
            video.contains(&format!("{}:{}", frame.0, frame.1)),
            "{video}"
        );
    }

    // Every branch ends at 48 kHz in a named layout — mono when the row says so, stereo otherwise.
    for (mono, layout) in [(false, "stereo"), (true, "mono")] {
        let sound = render::audio_chain(&clip, &Produce { mono, ..settings() }, 0, &[]);
        let last = sound.last().expect("a chain");
        assert!(
            last.starts_with("aformat=sample_fmts=fltp:sample_rates=48000:channel_layouts="),
            "{last}"
        );
        assert!(last.ends_with(layout), "{last}");
        // And the ceiling rides on every clip, not only on the ones that needed it.
        assert!(sound.contains(&render::LIMITER.to_string()), "{sound:?}");
    }
}

#[test]
fn sec_08_produce_5_rules_s1_seek_on_input_and_length_on_output() {
    assert_eq!(ITEM, "§08-produce#5-rules");
    let out = std::path::Path::new("produce/clips/c001_ab.mp4");
    let inputs = [
        ("picture", Some("media/talk.mkv")),
        ("sound", Some("media/talk.m4a")),
    ];

    // -ss before -i: the seek happens on input, so a clip 40 minutes in does not decode 40 minutes first.
    let seeked = render::Clip {
        no: 1,
        seg_s: 2400.5,
        on_screen: 8.0,
        rate: 1.0,
        ..Default::default()
    };
    let argv = render::encode_command(&seeked, &settings(), &inputs, None, out);
    let ss = argv.iter().position(|a| a == "-ss").expect("-ss");
    let first_input = argv.iter().position(|a| a == "-i").expect("-i");
    assert!(ss < first_input, "{argv:?}");
    assert_eq!(
        argv[ss + 1],
        "2400.500",
        "the same three decimals as every other time"
    );
    // And footage is read for as long as the clip takes of it, on the input beside the seek: without it a plain
    // 1x clip ran on to the end of its recording (the first rewrite's every footage clip).
    let length = argv
        .iter()
        .position(|a| a == "-t")
        .expect("-t on the input");
    assert!(length < first_input, "{argv:?}");
    assert_eq!(argv[length + 1], "8.000", "{argv:?}");

    // -t on the output: after every input and filter argument, immediately before the file's name.
    let rated = render::Clip {
        no: 1,
        seg_s: 4.0,
        on_screen: 6.5,
        rate: 2.0,
        ..Default::default()
    };
    let argv = render::encode_command(&rated, &settings(), &inputs, None, out);
    let last_input = argv.iter().rposition(|a| a == "-i").expect("the last -i");
    let t = argv
        .iter()
        .rposition(|a| a == "-t")
        .expect("-t on a rated clip");
    assert!(t > last_input, "{argv:?}");
    assert_eq!(argv[t + 1], "6.500");
    assert_eq!(argv.last().unwrap(), &out.display().to_string(), "{argv:?}");

    // An insert too — its asset's own length is not the cut's answer.
    let inserted = render::Clip {
        no: 2,
        seg_s: 0.0,
        on_screen: 3.25,
        rate: 1.0,
        source: "media/card.mp4".to_string(),
        ..Default::default()
    };
    let argv = render::encode_command(&inserted, &settings(), &inputs, None, out);
    assert!(argv.contains(&"-t".to_string()), "{argv:?}");
}

#[test]
fn sec_08_produce_5_rules_s1_which_clips_are_trimmed() {
    assert_eq!(ITEM, "§08-produce#5-rules");
    // §5 names four kinds: inserts, sounds, freezes, rated clips. The first two carry an asset in `source`;
    // a freeze reaches the planner at rate 0 with its still length in on_screen; a rated clip differs from 1.
    let base = render::Clip {
        no: 1,
        seg_s: 0.0,
        on_screen: 4.0,
        rate: 1.0,
        ..Default::default()
    };
    assert_eq!(
        render::output_limit(&base),
        None,
        "footage at 1× runs to the recording's end"
    );

    let insert = render::Clip {
        source: "media/card.mp4".to_string(),
        ..base.clone()
    };
    assert_eq!(render::output_limit(&insert).as_deref(), Some("4.000"));
    let copy = render::Clip {
        source: "copy:12".to_string(),
        ..base.clone()
    };
    assert_eq!(render::output_limit(&copy).as_deref(), Some("4.000"));
    let freeze = render::Clip {
        rate: 0.0,
        ..base.clone()
    };
    assert_eq!(
        render::output_limit(&freeze).as_deref(),
        Some("4.000"),
        "a stop is a still of on_screen s"
    );
    let rated = render::Clip {
        rate: 2.0,
        ..base.clone()
    };
    assert_eq!(render::output_limit(&rated).as_deref(), Some("4.000"));

    // And the trim is not applied to footage merely because it starts partway in: -ss already answers that.
    let seeked = render::Clip {
        seg_s: 90.0,
        ..base
    };
    assert_eq!(render::output_limit(&seeked), None);
}

#[test]
fn sec_08_produce_5_rules_s1_delays_in_whole_milliseconds_and_inputs_append_only() {
    assert_eq!(ITEM, "§08-produce#5-rules");
    // P.policy.gameVolume rides with these two: the bed's `volume=0.22` and the line's `adelay` are the same
    // filter pair, and a fractional millisecond would make ffmpeg parse what we meant as an expression.
    let clip = clip_with_line(0.3456);
    let chain = render::voice_chain(&clip.lines[0], &settings()).join(",");
    let delay = chain
        .split(',')
        .find(|filter| filter.starts_with("adelay="))
        .unwrap_or_else(|| panic!("no adelay in {chain}"));
    // `adelay=346ms|346ms` — the unit is spelled once per channel, after the whole number.
    let value = delay.trim_start_matches("adelay=");
    for part in value.split('|') {
        let number = part
            .strip_suffix("ms")
            .unwrap_or_else(|| panic!("{delay} has no ms unit"));
        assert!(!number.contains('.'), "{delay} is not whole milliseconds");
        assert!(number.parse::<i64>().is_ok(), "{delay} is not an integer");
    }
    assert_eq!(value, "346ms|346ms", "rounded, not truncated: {delay}");
    let bed = render::audio_chain(&clip, &settings(), 0, &[]).join(",");
    assert!(bed.contains("volume=0.22"), "{bed}");

    // Input indices append-only: one -i per pair, in the order given, and the `anullsrc` branch takes a slot
    // like any other input — otherwise a later filter's `[N:a]` would point at its neighbour's audio.
    let clip = render::Clip {
        no: 1,
        seg_s: 0.0,
        on_screen: 8.0,
        rate: 1.0,
        ..Default::default()
    };
    let inputs = [
        ("picture", Some("media/talk.mkv")),
        ("sound", None),
        ("voice", Some("narrate/line1.wav")),
        ("mix", Some("lane.wav")),
    ];
    let argv = render::encode_command(
        &clip,
        &settings(),
        &inputs,
        None,
        std::path::Path::new("produce/clips/c.mp4"),
    );
    assert_eq!(argv.iter().filter(|a| **a == "-i").count(), 4, "{argv:?}");
    let paths: Vec<&str> = argv
        .windows(2)
        .filter(|w| w[0] == "-i")
        .map(|w| w[1].as_str())
        .collect();
    assert_eq!(paths[0], "media/talk.mkv", "{paths:?}");
    assert!(
        paths[1].starts_with("anullsrc"),
        "the silent bed still takes a slot: {paths:?}"
    );
    assert_eq!(paths[2], "narrate/line1.wav", "{paths:?}");
    assert_eq!(paths[3], "lane.wav", "{paths:?}");
}

#[test]
fn sec_08_produce_5_rules_s1_the_clip_floor_and_the_log_it_owes() {
    assert_eq!(ITEM, "§08-produce#5-rules");
    // P.eng.minClipSeconds: 0.5 s is the single clip floor, and a dropped clip is always logged — never a
    // video that is quietly shorter than the cut.
    let mut short = seg(0.0, 0.4);
    short.rate = 1.0;
    let mut gone = seg(1.0, 11.0);
    gone.ins = "media/gone.mp4".to_string();
    // An insert's length is its asset's own `dur`, not the segment's span — that is what a spliced clip means.
    gone.dur = 10.0;
    let fine = seg(20.0, 30.0);

    let (clips, logs) = render::plan_clips(
        &[short, gone, fine],
        no_fx,
        "16:9",
        |_| false,
        &[],
        &[],
        (1920, 1080),
        true,
        render::unknown_footage,
    );
    // A missing insert is skipped — dropped from the video — while the too-short clip is dropped by the
    // floor. Both leave the list; each still logs its own number so the log matches the cut's numbering.
    assert_eq!(
        clips.iter().map(|clip| clip.no).collect::<Vec<usize>>(),
        [3],
        "{clips:?}"
    );
    assert_eq!(logs.len(), 2, "{logs:?}");
    // Each names its own clip number, so a person can find it in the cut.
    assert!(logs[0].contains("clip 1"), "{}", logs[0]);
    assert!(logs[1].contains("clip 2"), "{}", logs[1]);
    assert_eq!(logs[0], render::too_short_log(1, 0.0, 0.4, 0));
    assert!(
        logs[0].contains("too short to render, dropped"),
        "{}",
        logs[0]
    );
    assert_eq!(logs[1], render::missing_insert_log(2, "media/gone.mp4"));
    assert!(
        logs[1].contains("is not there any more — skipped"),
        "{}",
        logs[1]
    );

    // The floor is inclusive: exactly 0.5 s renders.
    let edge = seg(0.0, 0.5);
    let (clips, logs) = render::plan_clips(
        &[edge],
        no_fx,
        "16:9",
        |_| true,
        &[],
        &[],
        (1920, 1080),
        true,
        render::unknown_footage,
    );
    assert_eq!(clips.len(), 1, "{logs:?}");
    assert!(logs.is_empty(), "{logs:?}");
}

// ---- bullet 2: cues and sidecars ---------------------------------------------------------

fn line(text: &str) -> Entry {
    Entry {
        s: 1.0,
        e: 3.0,
        text: text.to_string(),
        ..Default::default()
    }
}

fn word(text: &str, s: f64, e: f64) -> subs::Word {
    subs::Word {
        text: text.to_string(),
        s,
        e,
    }
}

#[test]
fn sec_08_produce_5_rules_s2_cues_are_built_whatever_the_subtitle_mode_is() {
    assert_eq!(ITEM, "§08-produce#5-rules");
    // `subs::cues` takes no mode at all — that is how the rule holds: the cue list is built once and the mode
    // only decides where it goes (burned in, a track, or the sidecar beside the video). Three modes, one call.
    for mode in [Subtitles::BurnedIn, Subtitles::TrackInFile, Subtitles::None] {
        let lines = vec![line("what was said")];
        let built = subs::cues(subs::Source::Footage, &lines, &[], 1.0);
        assert_eq!(built.len(), 1, "{mode:?}");
        assert_eq!(built[0].text, "what was said", "{mode:?}");
    }

    // Never an empty cue: a line that says nothing contributes nothing rather than a blank caption.
    let blank = vec![line("   ")];
    assert!(subs::cues(subs::Source::Footage, &blank, &[], 1.0).is_empty());
    // With no lines, footage captions its own speech and an insert or freeze does not.
    let spoken = subs::cues(subs::Source::Footage, &[], &[word("hello", 0.2, 0.9)], 1.0);
    assert_eq!(spoken.len(), 1);
    assert!(subs::cues(subs::Source::Insert, &[], &[word("ding", 0.2, 0.9)], 1.0).is_empty());
    // And no cues at all is the one case §5 gives a log line for.
    assert_eq!(
        render::no_subtitle_cue_log(),
        "!!! nothing to put in a subtitle: no narration, and no speech in the clips"
    );
}

#[test]
fn sec_08_produce_5_rules_s2_sidecars_are_deleted_by_name_and_gaps_stay_in_the_original() {
    assert_eq!(ITEM, "§08-produce#5-rules");
    // Stale ones deleted first, by exact name — never a pattern that would take another video's file.
    let stale = render::stale_sidecars("final", &["de"]);
    assert_eq!(
        stale,
        ["final.srt", "final.vtt", "final.de.srt", "final.de.vtt"].map(String::from)
    );
    for name in &stale {
        assert!(!name.contains('*'), "{name} is a pattern");
    }
    assert_eq!(
        render::sidecar_names("final", None),
        ["final.srt".to_string(), "final.vtt".to_string()]
    );

    // Translations are read back by number; a gap is left in the original with a warning naming it, and the
    // track still ships. `subs::merge` owns that rule — this pins that one missing line is not a lost track.
    let original = vec![(1usize, "one".to_string()), (2, "two".to_string())];
    let mut got = BTreeMap::new();
    got.insert(1, "eins".to_string());
    let merged = subs::merge("de", &original, &got);
    assert_eq!(merged.lines[0], "eins");
    assert_eq!(merged.still_missing, vec![2]);
    assert!(
        merged.warning.is_some(),
        "the log has to name what stayed untranslated"
    );
    assert_eq!(
        merged.lines.len(),
        2,
        "a track is never dropped for one bad line"
    );

    // The session's own language is never translated into itself.
    assert_eq!(
        subs::track_languages("en", &["en".to_string(), "de".to_string()]),
        ["de".to_string()]
    );

    // webm carries no subtitle track whatever the tick says; mp4 and mkv keep theirs.
    let (_, webm) =
        screen::apply_container_rules(Container::Webm, Codec::Vp9, Subtitles::TrackInFile);
    assert_eq!(webm, Subtitles::None);
    for container in [Container::Mp4, Container::Mkv] {
        let (codec, kept) =
            screen::apply_container_rules(container, Codec::H264, Subtitles::TrackInFile);
        assert_eq!(kept, Subtitles::TrackInFile, "{container:?}");
        assert_eq!(codec, Codec::H264, "{container:?} keeps its codec");
    }
}

// ---- bullet 3: the expensive work runs once ----------------------------------------------

/// A project folder on disk, so `publish.json` and `final.stamp` can be real files.
fn tree_in(dir: &std::path::Path) -> Tree {
    std::fs::create_dir_all(dir).expect("project folder");
    Tree::new(dir).expect("tree")
}

#[test]
fn sec_08_produce_5_rules_s3_a_run_leaves_an_up_to_date_video_alone() {
    assert_eq!(ITEM, "§08-produce#5-rules");
    let dir = std::env::temp_dir().join(format!("np-rules-encode-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let tree = tree_in(&dir.join("p.naivepost"));

    // F5.3's hash decides it, and the answer is a pair of numbers: nothing changed means ▶ encodes nothing.
    let settings = Produce::default();
    let input = stamp::Input {
        settings: &settings,
        segs: &[],
        lines: &[],
        sources: &[],
        aspect: "16:9",
        voice: "1",
        fx: &[],
        lanes: &[],
        shape: (1920, 1080),
    };
    let current = stamp::stamp(&input, |_| None, |_| (0, 0));
    assert!(
        !stamp::skip_encode(stamp::read_stamp(&tree).as_deref(), &current),
        "never produced"
    );

    stamp::write_stamp(&tree, &current).expect("stamp");
    assert!(stamp::skip_encode(Some(&current), &current), "{current}");

    // A change to anything the render reads asks for the encode again: a codec is one keystroke on §A.
    let retuned = Produce {
        codec: project::Codec::Vp9,
        ..Produce::default()
    };
    let retuned = stamp::Input {
        settings: &retuned,
        segs: &[],
        lines: &[],
        sources: &[],
        ..input
    };
    let next = stamp::stamp(&retuned, |_| None, |_| (0, 0));
    assert_ne!(next, current);
    assert!(!stamp::skip_encode(Some(&current), &next));

    // And a stamp is written only when an encode actually ran and did not fail: writing one on a failure would
    // let the next ▶ skip work that was never done.
    assert!(render::stamp_written(true, false));
    assert!(
        !render::stamp_written(false, false),
        "a skipped encode writes no stamp of its own"
    );
    assert!(
        !render::stamp_written(true, true),
        "a failed encode leaves the old verdict standing"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn sec_08_produce_5_rules_s3_publish_json_goes_down_before_the_picture() {
    assert_eq!(ITEM, "§08-produce#5-rules");
    // The order is fixed by §5 and each step's reason is in it: a draw that dies leaves the record, so the next
    // ▶ finishes the job instead of rewriting text nobody asked for.
    assert_eq!(
        data::WRITE_ORDER,
        [
            "publish/publish.json",
            "publish/thumbnail.png",
            "final.html"
        ]
    );
    let json = std::path::Path::new(data::WRITE_ORDER[0]);
    let picture = std::path::Path::new(data::WRITE_ORDER[1]);
    assert_eq!(json.file_name().unwrap(), "publish.json");
    assert_eq!(
        picture.parent().unwrap(),
        json.parent().unwrap(),
        "both under publish/"
    );

    // The thumbnail keeps its own stamp, which is why redrawing a picture never invalidates the encode: it is
    // `publish/thumbnail.stamp`, not `final.stamp` beside the video.
    let dir = std::env::temp_dir().join(format!("np-rules-order-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let tree = tree_in(&dir.join("p.naivepost"));
    let video_stamp = tree.final_stamp();
    assert_eq!(video_stamp.file_name().unwrap(), "final.stamp");
    assert_ne!(
        video_stamp,
        tree.thumbnail_stamp(),
        "two stamps, two questions"
    );
    assert_eq!(tree.thumbnail_stamp().parent().unwrap(), tree.publish_dir());
    let _ = std::fs::remove_dir_all(&dir);
}

// ---- bullet 4: nothing shared touches the same file --------------------------------------
