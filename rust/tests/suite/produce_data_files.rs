//! §08-produce#3-data — the files `produce/` holds, checked against `naivepost::produce_data`.
//!
//! §3 is one line of paths, so this file is one test per path on that line: each name the spec
//! spells is reached through the accessor that writes it, and each of the line's three groups is
//! pinned to the lifetime §08 gives it (`clips/` cleared every run, the video's neighbours deleted
//! by exact name and left alone when the stamp matches, `publish/` surviving ▶). A rename on either
//! side fails a test here rather than turning up as a file some later run cannot find.
//!
//! Ids cited where they are used: `P.eng.titleBand` (§01 §5's `title_box`, S9), and
//! `tool:ffmpeg.encode` for everything the encodes write — the clips, the join list, the poster
//! (S1, S4, S8).

use std::path::PathBuf;

use crate::common;
use common::temp_dir;
use naivepost::layout::{self, Tree};
use naivepost::produce_data as data;
use naivepost::produce_embed as embed;
use naivepost::produce_render as render;
use naivepost::produce_stamp as stamp;
use naivepost::project::{self, Container};
use naivepost::publish;

const ITEM: &str = "§08-produce#3-data";

/// A project folder inside a throwaway root. Nothing is created under `produce/` — §3's files appear
/// only when a run or a save puts them there, which is what [`data::written`] reports on.
fn tree_in(tag: &str) -> (PathBuf, Tree) {
    let root = temp_dir(tag);
    let dir = root.join("demo.naivepost");
    std::fs::create_dir_all(&dir).unwrap();
    (root, Tree::new(&dir).unwrap())
}

// ---- S1: produce/clips/ ------------------------------------------------------------

// ---- S2: produce/clips/final.srt ---------------------------------------------------

#[test]
fn sec_08_produce_3_data_s2_the_working_cue_sheet_is_not_the_sidecar() {
    assert_eq!(ITEM, "§08-produce#3-data");
    let (_root, tree) = tree_in("s2");
    // §3 lists `clips/final.srt` *and* the sidecars beside the video. They are two files: this is the
    // cue sheet on the produced clock while the clips are still separate; that is what a player and
    // the `<video>` tag read afterwards.
    let working = data::scratch_srt(&tree);
    assert_eq!(working, tree.clips_dir().join("final.srt"));
    assert_ne!(
        working,
        tree.final_srt(None),
        "§3 would not list both if they were one file"
    );

    // Same stem on both sides — spelled from the module that owns the name, so a rename cannot leave
    // the two halves of a run writing to different videos.
    let stem = render::final_srt_name();
    assert_eq!(stem, "final.srt");
    assert!(working.to_string_lossy().ends_with(stem));
    assert!(tree.final_srt(None).to_string_lossy().ends_with(stem));
}

// ---- S3: produce/clips/final.<code>.srt --------------------------------------------

// ---- S4: produce/clips/concat.txt --------------------------------------------------

// ---- S5: produce/final.<ext> --------------------------------------------------------

#[test]
fn sec_08_produce_3_data_s5_the_video_is_named_by_its_container() {
    assert_eq!(ITEM, "§08-produce#3-data");
    let (_root, tree) = tree_in("s5");
    // §3's `<ext>` is the container ticked on the row — the three spellings the Produce page offers.
    for (container, ext) in [
        (Container::Mp4, "mp4"),
        (Container::Mkv, "mkv"),
        (Container::Webm, "webm"),
    ] {
        let video = tree.final_video(ext);
        assert_eq!(video, tree.dir().join(format!("produce/final.{ext}")));
        assert_eq!(embed::paths(&tree, container).0, video);
        // §3's group-2 entries all sit in that one folder beside it.
        assert_eq!(video.parent(), tree.final_jpg().parent());
    }
}

#[test]
fn sec_08_produce_3_data_s5_resolve_names_only_the_single_files() {
    assert_eq!(ITEM, "§08-produce#3-data");
    let (_root, tree) = tree_in("s5-resolve");
    // The entries that name one file answer; the three families (`<encode>`, `<code>`, `<ext>`) are
    // refused rather than guessed at, since guessing would write to a container nobody ticked.
    assert_eq!(data::resolve(&tree, "final.<ext>"), None);
    assert_eq!(data::resolve(&tree, "clips/<encode>"), None);
    assert_eq!(data::resolve(&tree, "final[.<code>].srt"), None);
    assert_eq!(
        data::resolve(&tree, "clips/final.srt"),
        Some(data::scratch_srt(&tree))
    );
    assert_eq!(
        data::resolve(&tree, "clips/concat.txt"),
        Some(data::concat_path(&tree))
    );
    assert_eq!(
        data::resolve(&tree, "final.stamp"),
        Some(tree.final_stamp())
    );
    assert_eq!(
        data::resolve(&tree, "publish/publish.json"),
        Some(tree.publish_json())
    );
}

// ---- S6: produce/final.stamp --------------------------------------------------------

#[test]
fn sec_08_produce_3_data_s6_the_stamp_sits_beside_the_video_and_decides_the_encode() {
    assert_eq!(ITEM, "§08-produce#3-data");
    let (_root, tree) = tree_in("s6");
    assert_eq!(tree.final_stamp(), tree.dir().join("produce/final.stamp"));
    assert_ne!(tree.final_stamp(), tree.final_video("mp4"));

    // What ▶ reads: a stamp equal to this run's hash skips the encode; anything else encodes.
    assert!(stamp::skip_encode(Some("abc"), "abc"));
    assert!(!stamp::skip_encode(Some("abc"), "abd"));
    // A missing or unreadable file reads as None, and None means encode — the stamp may only ever
    // let a run skip work it can prove was done.
    assert_eq!(stamp::read_stamp(&tree), None);
    assert!(!stamp::skip_encode(
        stamp::read_stamp(&tree).as_deref(),
        "abc"
    ));
    assert!(!stamp::skip_encode(None, ""));

    // Written and read back, it is the same answer.
    stamp::write_stamp(&tree, "abc").unwrap();
    assert_eq!(stamp::read_stamp(&tree).as_deref(), Some("abc"));
    assert!(stamp::skip_encode(
        stamp::read_stamp(&tree).as_deref(),
        "abc"
    ));
    // And it is not the thumbnail's stamp, which lives under publish/ and answers another question.
    assert_ne!(tree.final_stamp(), tree.thumbnail_stamp());
}

// ---- S7: sidecars -------------------------------------------------------------------

#[test]
fn sec_08_produce_3_data_s7_stale_sidecars_are_deleted_by_exact_name() {
    assert_eq!(ITEM, "§08-produce#3-data");
    // §3's `+ sidecars` and the rule that goes with them: ▶ deletes what it is about to rewrite by
    // name, never by a pattern — `*.de.srt` would take a translation of another video's file.
    let stale = render::stale_sidecars("final", &["de"]);
    assert_eq!(
        stale,
        [
            "final.srt".to_string(),
            "final.vtt".to_string(),
            "final.de.srt".to_string(),
            "final.de.vtt".to_string()
        ]
    );
    for name in &stale {
        assert!(!name.contains('*'), "{name} is a pattern, not a name");
        assert!(
            name.starts_with("final."),
            "{name} belongs to another video"
        );
    }
    // The poster and the tag are §3's own entries and never among the deletions: they are written by
    // the other half of ▶ and must survive a run that skipped the encode.
    assert!(!stale.contains(&"final.jpg".to_string()));
    assert!(!stale.contains(&"final.html".to_string()));
}

// ---- S8: produce/final.jpg + final.html ---------------------------------------------

#[test]
fn sec_08_produce_3_data_s8_the_poster_and_the_tag_are_beside_the_video() {
    assert_eq!(ITEM, "§08-produce#3-data");
    let (_root, tree) = tree_in("s8");
    // `produce_embed::paths` answers the three in §3's order: video, poster, tag.
    let (video, jpg, html) = embed::paths(&tree, Container::Mp4);
    assert_eq!(jpg, tree.final_jpg());
    assert_eq!(html, tree.final_html());
    assert_eq!(jpg.parent(), video.parent());
    assert_eq!(html.parent(), video.parent());
    assert_eq!(jpg.file_name().unwrap(), "final.jpg");
    assert_eq!(html.file_name().unwrap(), "final.html");

    // P.eng.posterQuality: §F5.5's "JPEG 90" and the rung ffmpeg takes for it — tool:ffmpeg.encode
    // does the encoding, so both numbers name one requirement at its two levels.
    let (quality, qscale) = data::poster_quality();
    assert_eq!((quality, qscale), (embed::POSTER_JPEG_QUALITY, "2"));

    // §3's two entries resolve to exactly these paths.
    assert_eq!(data::resolve(&tree, "final.jpg"), Some(jpg));
    assert_eq!(data::resolve(&tree, "final.html"), Some(html));
}

// ---- S9: produce/publish/publish.json ------------------------------------------------

#[test]
fn sec_08_produce_3_data_s9_the_specs_file_round_trips_and_paths_stay_relative() {
    assert_eq!(ITEM, "§08-produce#3-data");
    let (_root, tree) = tree_in("s9-spec");
    // §5's example, verbatim.
    let spec = r#"{"frames": ["project:prepare/inputs/frames/a/2026-09-16_17-25-30.jpg"], "crop": {"x": 0.5, "y": 0.5}, "own": false,
 "title_box": {"cx": 0.5, "cy": 0.25, "wf": 1, "hf": 0.4}, "thumb_title": "…", "title_seeded": true,
 "texts": [{"cx": 0.3, "cy": 0.8, "wf": 0.4, "hf": 0.1, "text": "…"}],
 "title": "…", "prompt": "…", "negative": "…", "description": "…"}"#;
    std::fs::create_dir_all(tree.publish_dir()).unwrap();
    std::fs::write(tree.publish_json(), spec).unwrap();

    let read = publish::load(&tree).expect("§5's own file loads");
    assert_eq!(read.frames.len(), 1);
    // P.eng.titleBand: the band's default is §5's example, spelled once in project.
    assert_eq!(read.title_box, Some(project::TitleBox::default()));
    assert_eq!(read.texts.len(), 1);
    assert!(read.title_seeded);

    // A frame is stored root-relative with the `project:` prefix and read back as a real path (§1).
    let stored = &read.frames[0];
    assert!(stored.starts_with("project:"), "{stored}");
    let resolved = layout::resolve(tree.dir(), tree.dir(), stored);
    assert_eq!(
        resolved,
        tree.dir()
            .join("prepare/inputs/frames/a/2026-09-16_17-25-30.jpg")
    );
    // And the round trip out writes the same spelling back.
    assert_eq!(
        layout::write_path(tree.dir(), tree.dir(), &resolved),
        *stored
    );

    // Saving what was read changes nothing about where the file lives.
    publish::save(&read, &tree).unwrap();
    assert_eq!(publish::load(&tree).unwrap(), read);
}

// ---- S10: §3's line as data ----------------------------------------------------------

#[test]
fn sec_08_produce_3_data_s10_the_spec_line_is_every_file_in_three_groups() {
    assert_eq!(ITEM, "§08-produce#3-data");
    // §3's one line, in its order: four under clips/, five beside the video, one record.
    assert_eq!(
        data::DATA_FILES,
        [
            "clips/<encode>",
            "clips/final.srt",
            "clips/final.<code>.srt",
            "clips/concat.txt",
            "final.<ext>",
            "final.stamp",
            "final[.<code>].srt",
            "final[.<code>].vtt",
            "final.jpg",
            "final.html",
            "publish/publish.json",
        ]
    );
    for entry in data::DATA_FILES {
        assert!(!entry.is_empty(), "an empty entry would name nothing");
        assert!(
            !entry.starts_with("produce/"),
            "{entry} repeats the folder §3 is about"
        );
        assert!(!entry.contains(".."), "{entry} leaves the project");
    }
}

#[test]
fn sec_08_produce_3_data_s10_each_group_has_the_lifetime_the_spec_gives_it() {
    assert_eq!(ITEM, "§08-produce#3-data");
    use data::Kind;
    // The three groups are three folders, and the folder is what decides what the next ▶ does.
    for entry in &data::DATA_FILES[..4] {
        assert_eq!(data::kind(entry), Kind::Scratch, "{entry} is inside clips/");
    }
    for entry in &data::DATA_FILES[4..10] {
        assert_eq!(
            data::kind(entry),
            Kind::Output,
            "{entry} sits beside the video"
        );
    }
    assert_eq!(data::kind(data::DATA_FILES[10]), Kind::State);

    // The counts are the spec's own grouping: four scratch, six output, one state.
    let kinds: Vec<Kind> = data::DATA_FILES.iter().map(|e| data::kind(e)).collect();
    assert_eq!(kinds.iter().filter(|k| **k == Kind::Scratch).count(), 4);
    assert_eq!(kinds.iter().filter(|k| **k == Kind::Output).count(), 6);
    assert_eq!(kinds.iter().filter(|k| **k == Kind::State).count(), 1);

    // A name nobody recognises is treated as output: leaving a file alone beats deleting it as
    // scratch, which is the irreversible direction.
    assert_eq!(data::kind("something-else.txt"), Kind::Output);
}

#[test]
fn sec_08_produce_3_data_s10_the_module_runs_nothing_itself() {
    assert_eq!(ITEM, "§08-produce#3-data");
    // This module names files and classifies them; the encodes that fill them belong to ffmpeg
    // (`tool:ffmpeg.encode`) and are spawned elsewhere.
    let source = include_str!("../../src/produce_data.rs");
    for forbidden in ["std::process", "process::Command", "gtk4", "adw::"] {
        assert!(
            !source.contains(forbidden),
            "{forbidden} does not belong in a pure module"
        );
    }
}
