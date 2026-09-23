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

use naivepost::layout::{self, Tree};
use naivepost::produce_data as data;
use naivepost::produce_embed as embed;
use naivepost::produce_render as render;
use naivepost::produce_stamp as stamp;
use naivepost::project::{self, Container, Publish};
use naivepost::publish;

const ITEM: &str = "§08-produce#3-data";

fn temp_dir(tag: &str) -> PathBuf {
    // Removed here rather than at the end of each test: a case that fails on an assertion never
    // reaches its own cleanup, and a folder left in /tmp is a folder the next reader explains.
    let dir = std::env::temp_dir().join(format!("naivepost-pdata-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// A project folder inside a throwaway root. Nothing is created under `produce/` — §3's files appear
/// only when a run or a save puts them there, which is what [`data::written`] reports on.
fn tree_in(tag: &str) -> (PathBuf, Tree) {
    let root = temp_dir(tag);
    let dir = root.join("demo.naivepost");
    std::fs::create_dir_all(&dir).unwrap();
    (root, Tree::new(&dir).unwrap())
}

/// The scratch folder, made real — a run creates it; §3 lists what lives in it.
fn make_scratch(tree: &Tree) {
    std::fs::create_dir_all(tree.clips_dir()).unwrap();
}

// ---- S1: produce/clips/ ------------------------------------------------------------

#[test]
fn sec_08_produce_3_data_s1_the_scratch_folder_holds_the_encodes() {
    assert_eq!(ITEM, "§08-produce#3-data");
    // tool:ffmpeg.encode writes one file per clip here; §3 names the folder first because it is the
    // only group that exists during a run and never between two of them.
    let (_root, tree) = tree_in("s1");
    assert_eq!(data::SCRATCH, "produce/clips/");
    assert_eq!(tree.clips_dir(), tree.dir().join("produce/clips"));

    // A per-clip encode lands in it, named `c%03d_<stamp>.<container>` by the module that encodes it.
    let clip = data::clip_path(&tree, 7, "17-25-30", "mp4");
    assert_eq!(clip.parent().unwrap(), tree.clips_dir());
    assert_eq!(clip.file_name().unwrap(), format!("{}.mp4", render::clip_stem(7, "17-25-30")).as_str());
    assert_eq!(clip.file_name().unwrap(), "c007_17-25-30.mp4");
    // The number is padded because the join list is read in name order.
    assert_eq!(
        data::clip_path(&tree, 1, "ab", "mp4").file_name().unwrap(),
        "c001_ab.mp4"
    );
    // And the container follows the row, so a webm run leaves no .mp4 behind to join.
    assert!(data::clip_path(&tree, 2, "ab", "webm").to_string_lossy().ends_with(".webm"));
}

#[test]
fn sec_08_produce_3_data_s1_clearing_the_scratch_folder_counts_and_keeps_it() {
    assert_eq!(ITEM, "§08-produce#3-data");
    let (_root, tree) = tree_in("s1-clear");
    // A project that has never rendered has no scratch folder: nothing to clear is not an error.
    assert_eq!(data::clear_scratch(&tree), Ok(0));
    assert!(!tree.clips_dir().exists(), "clearing must not create the folder");

    make_scratch(&tree);
    std::fs::write(data::clip_path(&tree, 1, "ab", "mp4"), b"clip").unwrap();
    std::fs::write(data::concat_path(&tree), b"file 'x'").unwrap();
    assert_eq!(data::clear_scratch(&tree), Ok(2), "every file inside, and the count says so");
    // The folder itself stays: the render writes into it seconds later, and a missing folder is an
    // ffmpeg error with a worse message than this one.
    assert!(tree.clips_dir().is_dir());
    assert_eq!(data::clear_scratch(&tree), Ok(0), "a second clear has nothing to say");
}

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
    assert_ne!(working, tree.final_srt(None), "§3 would not list both if they were one file");

    // Same stem on both sides — spelled from the module that owns the name, so a rename cannot leave
    // the two halves of a run writing to different videos.
    let stem = render::final_srt_name();
    assert_eq!(stem, "final.srt");
    assert!(working.to_string_lossy().ends_with(stem));
    assert!(tree.final_srt(None).to_string_lossy().ends_with(stem));
}

// ---- S3: produce/clips/final.<code>.srt --------------------------------------------

#[test]
fn sec_08_produce_3_data_s3_a_translation_is_drafted_in_the_scratch_folder() {
    assert_eq!(ITEM, "§08-produce#3-data");
    let (_root, tree) = tree_in("s3");
    // The draft beside the cue sheet…
    let drafted = data::scratch_translation_srt(&tree, "de");
    assert_eq!(drafted, tree.clips_dir().join("final.de.srt"));

    // …and the sidecar it becomes beside the video. §3's `+ sidecars` is that second spelling, and
    // produce_render owns the pair of extensions a track writes.
    let beside = tree.final_srt(Some("de"));
    assert_eq!(beside, tree.dir().join("produce/final.de.srt"));
    assert_ne!(drafted, beside);
    assert_eq!(
        render::sidecar_names("final", Some("de")),
        ["final.de.srt".to_string(), "final.de.vtt".to_string()]
    );
}

// ---- S4: produce/clips/concat.txt --------------------------------------------------

#[test]
fn sec_08_produce_3_data_s4_the_join_reads_one_list_from_the_scratch_folder() {
    assert_eq!(ITEM, "§08-produce#3-data");
    let (_root, tree) = tree_in("s4");
    // tool:ffmpeg.encode writes the clips; ffmpeg's own concat demuxer reads this list. It sits with
    // them because it says *which* clips this run made — an old list would join clips this cut lost.
    let list = data::concat_path(&tree);
    assert_eq!(list, tree.clips_dir().join("concat.txt"));
    assert_eq!(data::CONCAT_NAME, "concat.txt");

    // One `file '…'` line per clip, in the order they play — and the paths on those lines are the
    // scratch encodes this module names.
    let clips = [
        data::clip_path(&tree, 1, "ab", "mp4"),
        data::clip_path(&tree, 2, "ab", "mp4"),
    ];
    let stems: Vec<String> = clips.iter().map(|p| p.display().to_string()).collect();
    let text = render::concat_list(&stems);
    assert_eq!(text.lines().count(), 2);
    assert!(text.starts_with("file '"), "{text}");
    assert!(text.contains("c001_ab.mp4") && text.contains("c002_ab.mp4"), "{text}");
    assert!(
        text.find("c001").unwrap() < text.find("c002").unwrap(),
        "the join is in list order: {text}"
    );

    // And the command that reads it back: a stream copy, no re-encode.
    let out = tree.final_video("mp4");
    let argv = render::join_command(&list, &out);
    let joined = argv.join(" ");
    assert!(joined.contains("-f concat"), "{joined}");
    assert!(joined.contains("-safe 0"), "{joined}");
    assert!(joined.contains(&format!("-i {}", list.display())), "{joined}");
    assert!(joined.contains("-c copy"), "{joined}");
    assert!(joined.ends_with(&out.display().to_string()), "{joined}");
}

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
    assert_eq!(data::resolve(&tree, "clips/final.srt"), Some(data::scratch_srt(&tree)));
    assert_eq!(data::resolve(&tree, "clips/concat.txt"), Some(data::concat_path(&tree)));
    assert_eq!(data::resolve(&tree, "final.stamp"), Some(tree.final_stamp()));
    assert_eq!(data::resolve(&tree, "publish/publish.json"), Some(tree.publish_json()));
}

// ---- S6: produce/final.stamp --------------------------------------------------------

#[test]
fn sec_08_produce_3_data_s6_the_stamp_sits_beside_the_video_and_decides_the_encode() {
    assert_eq!(ITEM, "§08-produce#3-data");
    let (_root, tree) = tree_in("s6");
    // One path, two modules: §3 lists the file and F5.3 owns what is in it.
    assert_eq!(data::stamp_path(&tree), tree.final_stamp());
    assert_eq!(data::stamp_path(&tree), tree.dir().join("produce/final.stamp"));
    assert_ne!(data::stamp_path(&tree), tree.final_video("mp4"));

    // What ▶ reads: a stamp equal to this run's hash skips the encode; anything else encodes.
    assert!(data::video_is_up_to_date(Some("abc"), "abc"));
    assert!(!data::video_is_up_to_date(Some("abc"), "abd"));
    // A missing or unreadable file reads as None, and None means encode — the stamp may only ever
    // let a run skip work it can prove was done.
    assert_eq!(stamp::read_stamp(&tree), None);
    assert!(!data::video_is_up_to_date(stamp::read_stamp(&tree).as_deref(), "abc"));
    assert!(!stamp::skip_encode(None, ""));

    // Written and read back, it is the same answer.
    stamp::write_stamp(&tree, "abc").unwrap();
    assert_eq!(stamp::read_stamp(&tree).as_deref(), Some("abc"));
    assert!(data::video_is_up_to_date(stamp::read_stamp(&tree).as_deref(), "abc"));
    // And it is not the thumbnail's stamp, which lives under publish/ and answers another question.
    assert_ne!(data::stamp_path(&tree), tree.thumbnail_stamp());
}

// ---- S7: sidecars -------------------------------------------------------------------

#[test]
fn sec_08_produce_3_data_s7_the_sidecars_are_two_files_per_track() {
    assert_eq!(ITEM, "§08-produce#3-data");
    let (_root, tree) = tree_in("s7");
    // The session's own language carries no code between the dots; a translation names its own.
    assert_eq!(
        data::sidecar_files("final", None),
        ["final.srt".to_string(), "final.vtt".to_string()]
    );
    assert_eq!(
        data::sidecar_files("final", Some("de")),
        ["final.de.srt".to_string(), "final.de.vtt".to_string()]
    );
    // Both spellings land beside the video, and both are readable through Tree.
    assert_eq!(tree.final_srt(None).parent(), tree.final_vtt(Some("de")).parent());
    assert_eq!(tree.final_srt(None).file_name().unwrap(), data::sidecar_files("final", None)[0].as_str());
    assert_eq!(
        tree.final_vtt(Some("de")).file_name().unwrap(),
        data::sidecar_files("final", Some("de"))[1].as_str()
    );
}

#[test]
fn sec_08_produce_3_data_s7_stale_sidecars_are_deleted_by_exact_name() {
    assert_eq!(ITEM, "§08-produce#3-data");
    // §3's `+ sidecars` and the rule that goes with them: ▶ deletes what it is about to rewrite by
    // name, never by a pattern — `*.de.srt` would take a translation of another video's file.
    let stale = data::stale_files("final", &["de"]);
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
        assert!(name.starts_with("final."), "{name} belongs to another video");
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
fn sec_08_produce_3_data_s9_the_record_exists_or_it_does_not() {
    assert_eq!(ITEM, "§08-produce#3-data");
    let (_root, tree) = tree_in("s9-exists");
    // §5: "an existing publish.json means the upload text is written" — the file's existence is the
    // whole flag, so there is no boolean beside it to keep in step.
    assert!(!publish::is_written(&tree));
    assert_eq!(data::written(&tree), Vec::<&str>::new(), "a fresh project has none of §3's groups");

    let mut record = Publish::default();
    record.title = "a title".to_string();
    publish::save(&record, &tree).unwrap();
    assert!(publish::is_written(&tree));
    assert_eq!(data::written(&tree), [data::DATA_FILES[10]]);
    assert_eq!(data::publish_folder(&tree), tree.publish_dir());
    assert_eq!(
        data::resolve(&tree, "publish/publish.json"),
        Some(tree.publish_json())
    );

    // §5: "deleting the folder starts it over" — and that is the only way.
    publish::start_over(&tree).unwrap();
    assert!(!publish::is_written(&tree));
    assert_eq!(data::written(&tree), Vec::<&str>::new());
}

#[test]
fn sec_08_produce_3_data_s9_the_first_frame_is_the_base_and_the_rest_are_references() {
    assert_eq!(ITEM, "§08-produce#3-data");
    let (_root, tree) = tree_in("s9-frames");
    // §5: "The first frame is the base the image model edits; the rest are references." Order is the
    // whole answer, which is why no index travels beside the list.
    let mut record = Publish::default();
    record.frames = vec!["project:a.jpg".to_string(), "project:b.jpg".to_string(), "project:c.jpg".into()];
    publish::save(&record, &tree).unwrap();
    let read = publish::load(&tree).unwrap();
    assert_eq!(publish::base(&read), Some("project:a.jpg"));
    assert_eq!(publish::references(&read), ["project:b.jpg", "project:c.jpg"]);

    // A chosen frame rather than a drawn one is the `own` flag, and it is the only kind of thumbnail
    // that costs no model call.
    assert!(!read.own);
    let mut chosen = read.clone();
    chosen.own = true;
    publish::save(&chosen, &tree).unwrap();
    assert!(publish::load(&tree).unwrap().own);

    // And the base can be moved without editing the list by hand.
    let mut moved = read.clone();
    assert!(publish::set_base(&mut moved, "project:c.jpg"));
    assert_eq!(publish::base(&moved), Some("project:c.jpg"));
    assert!(!publish::set_base(&mut moved, "project:gone.jpg"));
}

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
    assert_eq!(resolved, tree.dir().join("prepare/inputs/frames/a/2026-09-16_17-25-30.jpg"));
    // And the round trip out writes the same spelling back.
    assert_eq!(layout::write_path(tree.dir(), tree.dir(), &resolved), *stored);

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
        assert!(!entry.starts_with("produce/"), "{entry} repeats the folder §3 is about");
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
        assert_eq!(data::kind(entry), Kind::Output, "{entry} sits beside the video");
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
fn sec_08_produce_3_data_s10_every_path_stays_inside_the_project() {
    assert_eq!(ITEM, "§08-produce#3-data");
    let (_root, tree) = tree_in("s10-paths");
    make_scratch(&tree);
    // The whole §3 set as this project's paths: all ten under `produce/`, none escaping the folder.
    let paths = [
        data::clip_path(&tree, 1, "ab", "mp4"),
        data::scratch_srt(&tree),
        data::scratch_translation_srt(&tree, "de"),
        data::concat_path(&tree),
        tree.final_video("mp4"),
        data::stamp_path(&tree),
        tree.final_srt(Some("de")),
        tree.final_vtt(None),
        tree.final_jpg(),
        tree.final_html(),
        tree.publish_json(),
    ];
    assert_eq!(paths.len(), 11, "one path per entry of §3's line");
    for path in &paths {
        assert!(path.starts_with(tree.dir()), "{} leaves the project", path.display());
        assert!(data::in_produce(&tree, path), "{} is not under produce/", path.display());
        // And nothing here is a directory: §3 lists files.
        assert!(path.extension().is_some(), "{} has no extension", path.display());
    }
}

#[test]
fn sec_08_produce_3_data_s10_the_module_runs_nothing_itself() {
    assert_eq!(ITEM, "§08-produce#3-data");
    // This module names files and classifies them; the encodes that fill them belong to ffmpeg
    // (`tool:ffmpeg.encode`) and are spawned elsewhere.
    let source = include_str!("../src/produce_data.rs");
    for forbidden in ["std::process", "process::Command", "gtk4", "adw::"] {
        assert!(!source.contains(forbidden), "{forbidden} does not belong in a pure module");
    }
    assert!(source.contains("tool:ffmpeg.encode"));
}
