//! §08-produce#3 Data — what `produce/` holds, checked against the folder the program writes.
//!
//! §3 is one line of paths and §6 adds five more files to `produce/clips/`, so this file pins three things
//! that line can only be honest about by construction:
//!
//! - the catalogue [`naivepost::produce_data::catalogue`] is §3's eleven entries in §3's order followed by
//!   §6's five, and every `clips/` entry is scratch while the video's neighbours are output and the record
//!   under `publish/` is state;
//! - the stream-copy join is named `joined.<container>` (§6) and ONE expression feeds both the command that
//!   writes it and the mux that reads it back, so a container change moves both together;
//! - the files §3 lists actually appear: the cue sheet beside the video, the poster off the drawn thumbnail,
//!   and `publish/publish.json` surviving a render untouched.
//!
//! No ffmpeg runs here (this container has none): the subprocess legs are the program's own injected seams —
//! the walk's spawner records argv and lays down the file each command names, and the poster leg is
//! [`naivepost::produce_tag_page::MakePoster`], injected exactly as `tests/suite/produce_tag_page.rs` injects it.
//!
//! `s1` is the widget leg: the folder is produced by a press on the real ▶ with the scripted spawner loaded,
//! so what is asserted below is read off the disk THAT press wrote, through the same seam
//! (`produce_translate::set_spawn_for_test`) `tests/suite/produce_tag_page.rs` uses.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Mutex;

use adw::prelude::*;
use naivepost::shell::Page;
use naivepost::ui;

/// The poster leg's stand-in type: `Rc` so one leg can be handed to several runs of the same test.
type Poster = Rc<dyn Fn(&[String]) -> Result<(), String>>;

/// The argv the scripted spawner was handed, one entry per spawned command.
type SpawnedArgv = std::sync::Arc<std::sync::Mutex<Vec<Vec<String>>>>;

/// The same record one step deeper: which walk step wrote which argv.
type SpawnedSteps = Rc<RefCell<Vec<(String, Vec<String>)>>>;

use crate::common;
use common::produce_project as project;
use common::session_tree_of as session;
use naivepost::cut::Seg;
use naivepost::layout::Tree;
use naivepost::narration::Entry;
use naivepost::produce_data as data;
use naivepost::produce_details as details;
use naivepost::produce_exec as exec;
use naivepost::produce_flow as flow;
use naivepost::produce_render as render;
use naivepost::produce_subtitles as subs;
use naivepost::project::{Codec, Container, Produce, Publish, Source, Subtitles};

const ITEM: &str = "sec_08_produce_3_data";

// ---- the folder as a press on the real ▶ left it (the widget leg) ---------------------------------

/// What the scripted spawner and the log belong to: `set_spawn_for_test` is thread-local, so the press and
/// the assertions share one thread and one recorded spawner, exactly as `tests/suite/produce_tag_page.rs` does.
static WIDGET_FAIL: Mutex<Option<String>> = Mutex::new(None);

thread_local! {
    static CURRENT_APP: std::cell::RefCell<Option<adw::Application>> = const { std::cell::RefCell::new(None) };
}

fn app_in_round() -> adw::Application {
    CURRENT_APP
        .with(|held| held.borrow().clone())
        .expect("run_round published the application")
}

fn two_segs() -> Vec<Seg> {
    vec![
        Seg {
            s: 0.0,
            e: 9.5,
            cam: 0,
            ..Default::default()
        },
        Seg {
            s: 10.0,
            e: 30.0,
            cam: 0,
            ..Default::default()
        },
    ]
}

/// A Produce window over the fixture project, the way `tests/suite/produce_tag_page.rs` builds one.
fn produce_window(app: &adw::Application) -> adw::ApplicationWindow {
    common::release_last_window();
    let model = naivepost::project::load(&common::fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, Page::Produce.label());
    common::hold_last_window(window.clone());
    window.present();
    common::settle();
    window
}

/// Seed the page's rows so the press hashes real settings rather than an empty state.
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
    ui::produce_page::set_state(page);
}

/// The scripted spawner, plus the argv it was handed: no ffmpeg here, and §3's claim about the join is only
/// checkable if the commands the walk issued are remembered.
fn scripted_spawn(seen: SpawnedArgv) -> naivepost::produce_translate::SpawnSlot {
    std::sync::Arc::new(move |command: &naivepost::produce_exec::Command| {
        seen.lock().unwrap().push(command.argv.clone());
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

/// One run through the real ▶: what §3's catalogue says the folder now holds, checked off that disk.
fn check_press_writes_what_the_catalogue_lists(root: &Path) {
    let tree = session(root);
    // A cut on disk, because §08 reads the cut the Cut page saved when no live review cut stands.
    let cut = naivepost::cut::Cut {
        segs: two_segs(),
        aspect: "16:9".to_string(),
        ..Default::default()
    };
    std::fs::create_dir_all(tree.cut_json().parent().unwrap()).unwrap();
    std::fs::write(tree.cut_json(), serde_json::to_string_pretty(&cut).unwrap()).unwrap();
    // Narration the render can speak without a server: `wav_of` finds these and needs no ffprobe to size them.
    naivepost::narration::save(
        &naivepost::narration::Narration {
            entries: vec![
                Entry {
                    s: 0.0,
                    e: 4.0,
                    at: 0.5,
                    text: "one spoken line".into(),
                    ..Default::default()
                },
                Entry {
                    s: 5.0,
                    e: 9.0,
                    at: 5.5,
                    text: "a second spoken line".into(),
                    ..Default::default()
                },
            ],
            silent: vec![],
        },
        &tree,
    )
    .expect("two lines for the run to speak");
    // A fake take wav per line, so the render's sizing leg has a real file to weigh (no ffprobe, no server):
    // `produce_stamp::wav_of` names each take, so these are the paths the run itself would look for.
    for (index, entry) in ["one spoken line", "a second spoken line"]
        .iter()
        .enumerate()
    {
        let line = Entry {
            s: index as f64 * 5.0,
            e: index as f64 * 5.0 + 4.0,
            at: 0.5,
            text: (*entry).into(),
            ..Default::default()
        };
        let take = naivepost::produce_stamp::wav_of(&tree, &line, Some("1"), 0)
            .expect("a line has a take path");
        std::fs::create_dir_all(take.parent().unwrap()).unwrap();
        std::fs::write(&take, b"RIFF fake wav bytes, long enough to weigh").unwrap();
    }
    // The scratch folder carries another render's leftovers, so the sweep §6 asks for is proved rather than
    // assumed: the file below can only be gone if the press really cleared the folder first.
    let stale = tree.clips_dir().join("joined.mp4");
    std::fs::create_dir_all(tree.clips_dir()).unwrap();
    std::fs::write(&stale, b"an older render's join").unwrap();
    let seen: SpawnedArgv = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    naivepost::produce_translate::set_spawn_for_test(scripted_spawn(seen.clone()));

    let window = produce_window(&app_in_round());
    ui::seed_review_cut(&window, &cut);
    seed_page(&window);
    let button =
        ui::line_step_button(&window, "play-button").expect("the run bar drew a play-button");
    button.emit_by_name::<()>("clicked", &[]);
    // The press walks on a worker (§03-shell §1): its answer lands before anything is read.
    assert!(
        naivepost::ui::background::settle_for_test(std::time::Duration::from_secs(120)),
        "the press's worker answered"
    );
    common::settle();

    let logs = ui::window::window_logs();
    assert!(
        logs.iter()
            .any(|l| l.starts_with(">>> transcoding ") || l.starts_with(">>> producing ")),
        "the press reached the encode: {:?}",
        logs.iter()
            .filter(|l| l.contains("produce"))
            .collect::<Vec<&String>>()
    );

    // §3's video, stamp, page and cue sheet, read back off the folder the PRESS wrote.
    let video = tree.final_video("mp4");
    assert!(
        video.is_file(),
        "the run wrote the video at {}",
        video.display()
    );
    assert!(
        naivepost::produce_stamp::stamp_path(&tree).is_file(),
        "and the stamp beside it"
    );
    assert!(
        tree.final_srt(None).is_file(),
        "§3's `final.srt` for the cues it spoke"
    );
    assert!(
        tree.final_vtt(None).is_file(),
        "and the webvtt the <video> tag reads"
    );
    assert!(tree.final_html().is_file(), "and the tag page");
    // The scratch sheet lives under clips/ (§3) and the join carries the container (§6).
    assert!(
        data::scratch_srt(&tree).is_file(),
        "{} is §3's scratch sheet",
        data::scratch_srt(&tree).display()
    );
    let join = tree
        .clips_dir()
        .join(naivepost::produce_details::joined("mp4"));
    assert_eq!(
        stale, join,
        "the planted leftover is exactly the file this container's run writes"
    );
    assert!(
        join.is_file(),
        "the stream-copy join §6 names: {}",
        join.display()
    );
    assert_ne!(
        std::fs::read(&join).unwrap(),
        b"an older render's join",
        "§6's sweep cleared the leftover before the run wrote its own join"
    );
    let commands = seen.lock().unwrap();
    assert!(
        commands
            .iter()
            .any(|argv| argv.last().is_some_and(|out| out == join.to_str().unwrap())),
        "and it is a command's own output, not a hopeful path: {:#?}",
        *commands
    );
    // The catalogue's `resolve` agrees with every one of those paths — the reason §3's spelling is load-bearing.
    assert_eq!(data::resolve(&tree, "final.html"), Some(tree.final_html()));
    assert_eq!(
        data::resolve(&tree, "clips/final.srt"),
        Some(data::scratch_srt(&tree))
    );
    // And §6's sweep already ran: the log line names the scratch groups it cleared.
    let groups = data::scratch_groups();
    assert!(
        logs.iter()
            .any(|l| l.contains("cleared") && l.contains(&format!("{groups} kinds"))),
        "the S1 sweep ran before the encodes: {:?}",
        logs.iter()
            .filter(|l| l.contains("clips"))
            .collect::<Vec<&String>>()
    );
    // `publish.json` was never written by this run: §3's state entry stays absent, so §5's flag is honest.
    assert!(
        !naivepost::publish::is_written(&tree),
        "a render does not write the upload record"
    );
    window.close();
    common::settle();
}

/// One press on the shell's ▶ fills the folder the way §3 describes it: the video, its stamp, both cue
/// sheets beside it, the tag page, §3's scratch sheet and §6's container-named join under `clips/` — every
/// path the catalogue spells, read off the disk the press wrote, with no live ffmpeg and no sleeping.
#[test]
fn sec_08_produce_3_data_s1_a_press_fills_the_folder_the_catalogue_lists() {
    let root = std::env::temp_dir().join(format!("np-sec083-data-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("temp root created");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");

    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(run_widget_round);
        app.run_with_args(&["naivepost"]);
    });

    let failure = WIDGET_FAIL.lock().unwrap().take();
    std::fs::remove_dir_all(&root).ok();
    assert!(failure.is_none(), "{}", failure.unwrap_or_default());
}

/// The activate round: pin cwd to this check's own folder (the page resolves the session from cwd), run the
/// press, and hand any panic back as a message rather than an abort through the C trampoline.
fn run_widget_round(app: &adw::Application) {
    CURRENT_APP.with(|held| *held.borrow_mut() = Some(app.clone()));
    let base = std::env::current_dir().expect("cwd pinned before the run");
    let folder = base.join("press");
    std::fs::create_dir_all(&folder).unwrap();
    std::env::set_current_dir(&folder).unwrap();
    let owned = folder.clone();
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        check_press_writes_what_the_catalogue_lists(&owned);
    }));
    if outcome.is_err() {
        *WIDGET_FAIL.lock().unwrap() =
            Some("press-fills-the-folder: the press's folder did not match §3".into());
    }
    let app = app.clone();
    glib::idle_add_local(move || {
        app.quit();
        glib::ControlFlow::Break
    });
}

// ---- the catalogue -------------------------------------------------------------------------------

/// §3's line, then §6's additions: the catalogue is the two spec lines concatenated, in their own orders.
#[test]
fn sec_08_produce_3_data_s1_catalogue_is_spec_3_then_spec_6() {
    assert_eq!(ITEM, "sec_08_produce_3_data");
    let list = data::catalogue();
    assert_eq!(
        list.len(),
        data::DATA_FILES.len() + data::CLIPS_EXTRA.len(),
        "§3's {} entries then §6's {}: {list:?}",
        data::DATA_FILES.len(),
        data::CLIPS_EXTRA.len()
    );
    // §3's order is kept exactly: the four under clips/, the five beside the video, then the record.
    assert_eq!(&list[..data::DATA_FILES.len()], &data::DATA_FILES[..]);
    for extra in data::CLIPS_EXTRA {
        assert!(
            list.contains(&extra),
            "{extra} is missing from the catalogue: {list:?}"
        );
    }
    // §6 puts its extras in `clips/`, which is what makes them scratch like §3's four.
    for extra in data::CLIPS_EXTRA {
        assert!(
            extra.starts_with("clips/"),
            "{extra} does not live in the scratch folder"
        );
    }
}

/// The three groups §3's grouping means: what happens to the file on the next ▶.
#[test]
fn sec_08_produce_3_data_s2_kind_sorts_every_entry_into_its_group() {
    let list = data::catalogue();
    for entry in list.iter().filter(|e| e.starts_with("clips/")) {
        assert_eq!(
            data::kind(entry),
            data::Kind::Scratch,
            "{entry} is cleared by every run"
        );
    }
    for entry in list.iter().filter(|e| e.starts_with("final.")) {
        assert_eq!(
            data::Kind::Output,
            data::kind(entry),
            "{entry} stands beside the video"
        );
    }
    assert_eq!(
        data::kind("publish/publish.json"),
        data::Kind::State,
        "the upload record survives ▶ entirely"
    );
    // Every entry lands in exactly one group, so nothing is left unclassified by accident.
    let groups = [data::Kind::Scratch, data::Kind::Output, data::Kind::State];
    for entry in list.iter() {
        assert!(groups.contains(&data::kind(entry)), "{entry} has no group");
    }
}

/// §6 spells its five extras in `produce_details`; the catalogue must agree with those spellings rather
/// than restate them, or the two lists drift the first time a name changes.
#[test]
fn sec_08_produce_3_data_s3_catalogue_agrees_with_details_scratch_names() {
    // Same placeholders, same folder: `details::SCRATCH_KINDS` is §6's own words without the prefix.
    assert_eq!(
        details::SCRATCH_KINDS.len(),
        data::CLIPS_EXTRA.len() + 1,
        "one encode entry each"
    );
    let spelled = [
        details::clip_cues(3, "ab12"),
        details::cue_title(3, 7),
        details::card_copy("produce/cards/ranks.svg"),
        details::animation_frames("loop.svg"),
        details::joined("mp4"),
    ];
    let expected = [
        "c003_ab12.srt",
        "c003_t07.svg",
        "ranks-filled.svg",
        "loop.frames/",
        "joined.mp4",
    ];
    for (name, want) in spelled.iter().zip(expected) {
        assert_eq!(name, want, "a §6 name changed spelling: {name}");
    }
    // Each spelled name sits under the catalogue's matching entry's placeholder family.
    for (entry, name) in data::CLIPS_EXTRA[1..].iter().zip(&spelled[1..]) {
        let placeholder = entry.trim_start_matches("clips/");
        let stem = placeholder.split(['<', '%']).next().unwrap_or(placeholder);
        assert!(
            stem.is_empty() || name.starts_with(stem),
            "{name} does not match the catalogue entry {entry}"
        );
    }
}

// ---- the join's name -----------------------------------------------------------------------------

/// One run per container: the scratch join carries the container this run writes, and the SAME path appears
/// in the join command and as the mux's input.
#[test]
fn sec_08_produce_3_data_s4_the_join_is_named_after_the_container_and_shared_by_both_commands() {
    for (container, code) in [
        (Container::Mp4, "mp4"),
        (Container::Webm, "webm"),
        (Container::Mkv, "mkv"),
    ] {
        let (_root, tree) = common::tree_in(&format!("join-{code}"));
        let settings = Produce {
            container,
            ..Default::default()
        };
        let (tree2, run) = run_over(
            &tree,
            vec![common::seg(0.0, 10.0)],
            Vec::new(),
            settings.clone(),
        );
        let plan = exec::plan(&run, &tree2, &mats(&[], exec::Cues::default()));
        let want = tree.clips_dir().join(format!("joined.{code}"));
        assert_eq!(want, tree.clips_dir().join(details::joined(code)));
        let join = plan
            .join
            .clone()
            .unwrap_or_else(|| panic!("{code}: a run with a clip joins"));
        assert_eq!(
            Some(want.display().to_string()),
            join.argv.last().map(|s| s.to_string()),
            "the join writes {code}'s stream copy: {:?}",
            join.argv
        );
        // The mux reads exactly that file back: one name, two commands.
        assert!(
            plan.mux
                .argv
                .iter()
                .any(|a| *a == want.display().to_string()),
            "the mux's input is the join it just wrote: {:?}",
            plan.mux.argv
        );
    }
}

/// A webm run does not keep an mp4-shaped scratch name around: the join follows the container ticked.
#[test]
fn sec_08_produce_3_data_s5_no_run_leaves_a_bin_named_join_behind() {
    let (_root, tree) = common::tree_in("join-no-bin");
    let (tree2, run) = run_over(
        &tree,
        vec![common::seg(0.0, 10.0)],
        Vec::new(),
        Produce {
            container: Container::Mkv,
            ..Default::default()
        },
    );
    let spy = Spy::new();
    spy.run(&run, &tree2, &mats(&[], exec::Cues::default()));
    let all = spy.joined();
    assert!(
        !all.contains("joined.bin"),
        "no run writes a `.bin` join:\n{all}"
    );
    assert!(
        all.contains("joined.mkv"),
        "it writes the mkv-named join:\n{all}"
    );
}

// ---- the folder the run writes -------------------------------------------------------------------

/// §3's `final.srt` beside the video: written when there are cues, absent-with-a-said-reason when there
/// are none, and the scratch sheet appears in `clips/` either way only when it has something to say.
#[test]
fn sec_08_produce_3_data_s6_the_cue_sheet_lands_beside_the_video_when_there_are_cues() {
    let line = Entry {
        s: 0.0,
        e: 5.0,
        at: 0.0,
        text: "a spoken line".into(),
        ..Default::default()
    };
    let (tree, run) = run_over(
        &common::tree_in("sheet-yes").1,
        vec![common::seg(0.0, 10.0)],
        vec![line.clone()],
        Produce {
            subtitles: Subtitles::TrackInFile,
            ..Default::default()
        },
    );
    let spy = Spy::new();
    let rendered = spy.run(
        &run,
        &tree,
        &mats(
            &[],
            exec::Cues {
                languages: vec![],
                ..Default::default()
            },
        ),
    );
    assert!(
        rendered.ok,
        "the render half reported failure: {rendered:?}"
    );
    assert!(
        tree.final_srt(None).is_file(),
        "{} should exist after a run with cues",
        tree.final_srt(None).display()
    );
    assert!(
        data::scratch_srt(&tree).is_file(),
        "and the scratch sheet §3 lists is there too"
    );
    // The catalogue's scratch entry resolves to the file the run wrote, not to a hand-typed name.
    assert_eq!(
        data::resolve(&tree, "clips/final.srt"),
        Some(data::scratch_srt(&tree)),
        "the catalogue's entry and the writer agree"
    );

    // No cues at all: nothing beside the video, and the reason is said.
    let (_root2, tree2) = common::tree_in("sheet-no");
    let (tree3, run2) = run_over(
        &tree2,
        vec![common::seg(0.0, 10.0)],
        Vec::new(),
        Produce {
            subtitles: Subtitles::TrackInFile,
            ..Default::default()
        },
    );
    let quiet = Spy::new();
    quiet.run(&run2, &tree3, &mats(&[], exec::Cues::default()));
    assert!(
        !tree3.final_srt(None).is_file(),
        "with no cue there is no track to write: {}",
        tree3.final_srt(None).display()
    );
    assert!(
        quiet
            .logs_vec()
            .iter()
            .any(|l| l.contains(render::no_subtitle_cue_log())),
        "and the absence is logged: {:?}",
        quiet.logs_vec()
    );
}

/// §3's `final.jpg`: off `produce/publish/thumbnail.png` when that exists, else off `thumbnail-plain.png`
/// (the frame before any lettering, §F5.6 S6), and skipped-with-a-log when neither exists. The tag page
/// goes out in every case — a page with no picture still plays.
#[test]
fn sec_08_produce_3_data_s7_the_poster_is_made_from_the_thumbnail_that_exists() {
    let (_root, tree) = common::tree_in("poster");
    std::fs::create_dir_all(tree.final_jpg().parent().unwrap()).unwrap();
    std::fs::write(tree.final_video("mp4"), b"a video").unwrap();
    let calls: Rc<RefCell<Vec<Vec<String>>>> = Rc::new(RefCell::new(Vec::new()));
    let leg_calls = calls.clone();
    let leg: Box<naivepost::produce_tag_page::MakePoster> = Box::new(move |argv: &[String]| {
        leg_calls.borrow_mut().push(argv.to_vec());
        if let Some(out) = argv.last() {
            let _ = std::fs::write(Path::new(out), b"a jpeg poster");
        }
        Ok(())
    });
    let logs = Rc::new(RefCell::new(Vec::new()));
    let log_store = logs.clone();

    // Nothing drawn yet: no poster, the reason said, the page still written.
    let page = naivepost::produce_tag_page::build_and_write(
        &tree,
        Container::Mp4,
        Codec::H264,
        "en",
        &[],
        &*leg,
        |line| log_store.borrow_mut().push(line.to_string()),
    );
    assert!(
        page.written,
        "the tag page is written even with no poster: {page:?}"
    );
    assert!(
        calls.borrow().is_empty(),
        "no poster was asked for: {:?}",
        calls.borrow()
    );
    assert!(
        logs.borrow()
            .iter()
            .any(|l| l.contains("no thumbnail has been drawn yet")),
        "and that is said: {:?}",
        logs.borrow()
    );
    assert!(
        !tree.final_jpg().is_file(),
        "so §3's final.jpg is simply absent"
    );

    // Now draw one (the printed picture) and run again: the poster is JPEG 90 off it, at §3's path.
    std::fs::create_dir_all(tree.publish_dir()).unwrap();
    std::fs::write(tree.thumbnail_png(), b"the printed thumbnail").unwrap();
    // A plain frame standing too does not win: the printed picture is the one §F5.5 names.
    std::fs::write(tree.thumbnail_plain_png(), b"the same frame, unlettered").unwrap();
    let page = naivepost::produce_tag_page::build_and_write(
        &tree,
        Container::Mp4,
        Codec::H264,
        "en",
        &[],
        &*leg,
        |line| log_store.borrow_mut().push(line.to_string()),
    );
    assert!(page.written);
    let argv = calls.borrow().last().cloned().unwrap_or_default();
    assert!(
        argv.iter().any(|a| a.ends_with("thumbnail.png")),
        "the source is the drawn thumbnail: {argv:?}"
    );
    assert_eq!(
        Some(&tree.final_jpg().display().to_string()),
        argv.last(),
        "and the output is §3's jpg"
    );
    assert!(tree.final_jpg().is_file(), "which is really on disk now");
    let (quality, rung) = data::poster_quality();
    assert!(
        argv.windows(2).any(|w| w[0] == "-q:v" && w[1] == rung),
        "the ladder rung §F5.5 asks for ({quality}): {argv:?}"
    );

    // Only the plain frame: still a poster, photographed off the unlettered frame.
    std::fs::remove_file(tree.thumbnail_png()).unwrap();
    let page = naivepost::produce_tag_page::build_and_write(
        &tree,
        Container::Mp4,
        Codec::H264,
        "en",
        &[],
        &*leg,
        |line| log_store.borrow_mut().push(line.to_string()),
    );
    assert!(
        page.poster,
        "the plain frame is a legitimate poster: {page:?}"
    );
    let argv = calls.borrow().last().cloned().unwrap_or_default();
    assert!(
        argv.iter().any(|a| a.ends_with("thumbnail-plain.png")),
        "photographed off the unlettered frame: {argv:?}"
    );

    // Neither file: no poster, the reason said, and §3's jpg is simply absent.
    std::fs::remove_file(tree.thumbnail_plain_png()).unwrap();
    std::fs::remove_file(tree.final_jpg()).unwrap();
    let page = naivepost::produce_tag_page::build_and_write(
        &tree,
        Container::Mp4,
        Codec::H264,
        "en",
        &[],
        &*leg,
        |line| log_store.borrow_mut().push(line.to_string()),
    );
    assert!(
        !page.poster && page.written,
        "the page survives its picture's absence: {page:?}"
    );
    assert!(!tree.final_jpg().is_file(), "and nothing invented one");
}

/// §3's sidecars (`.srt`/`.vtt` beside the video) and the tag page's track list agree: the bare-stem pair is
/// what a cue sheet writes, the per-language pair only exists if a language was ticked, and the tracks the
/// page advertises are read off the folder rather than out of the settings.
#[test]
fn sec_08_produce_3_data_s10_the_sidecars_and_the_track_list_agree() {
    let line = Entry {
        s: 0.0,
        e: 5.0,
        at: 0.0,
        text: "a spoken line".into(),
        ..Default::default()
    };
    let (_root, tree) = common::tree_in("sidecars");
    let (tree, run) = run_over(
        &tree,
        vec![common::seg(0.0, 10.0)],
        vec![line],
        Produce {
            subtitles: Subtitles::TrackInFile,
            ..Default::default()
        },
    );
    // No language ticked: the video's own track is the bare-stem pair, and §3's `<code>` entry is the one
    // that stays empty here.
    Spy::new().run(
        &run,
        &tree,
        &mats(
            &[],
            exec::Cues {
                languages: vec![],
                ..Default::default()
            },
        ),
    );
    assert!(
        tree.final_srt(None).is_file(),
        "{} never written",
        tree.final_srt(None).display()
    );
    assert!(
        tree.final_vtt(None).is_file(),
        "{} never written",
        tree.final_vtt(None).display()
    );
    assert!(
        !tree.final_srt(Some("en")).is_file(),
        "no ticked language, no `final.en.srt`"
    );
    // The catalogue resolves §3's wildcard to the pair `layout` writes, so no test hand-types a stem.
    let sidecars = data::sidecar_files("final", Some("en"));
    assert_eq!(
        sidecars,
        ["final.en.srt".to_string(), "final.en.vtt".to_string()]
    );
    assert_eq!(
        data::sidecar_files("final", None),
        ["final.srt".to_string(), "final.vtt".to_string()],
        "and the bare pair is the one this run wrote"
    );
    // And the tag page reads the tracks back off that folder rather than out of the settings.
    let page = naivepost::produce_tag_page::build_and_write(
        &tree,
        Container::Mp4,
        Codec::H264,
        "en",
        &[],
        &*no_poster(),
        |_| {},
    );
    assert_eq!(
        page.tracks, 1,
        "the one .vtt beside the video became one track: {page:?}"
    );

    // Empty cue sheet: no sidecars, so the page has nothing to advertise either.
    let (_root2, quiet) = common::tree_in("sidecars-none");
    std::fs::create_dir_all(quiet.final_video("mp4").parent().unwrap()).unwrap();
    std::fs::write(quiet.final_video("mp4"), b"a video").unwrap();
    let (quiet, run2) = run_over(
        &quiet,
        vec![common::seg(0.0, 10.0)],
        Vec::new(),
        Produce {
            subtitles: Subtitles::TrackInFile,
            ..Default::default()
        },
    );
    Spy::new().run(&run2, &quiet, &mats(&[], exec::Cues::default()));
    assert!(
        !quiet.final_vtt(None).is_file(),
        "no cue, no track: {}",
        quiet.final_vtt(None).display()
    );
    let page = naivepost::produce_tag_page::build_and_write(
        &quiet,
        Container::Mp4,
        Codec::H264,
        "en",
        &[],
        &*no_poster(),
        |_| {},
    );
    assert_eq!(
        page.tracks, 0,
        "and the page advertises what is actually there: {page:?}"
    );
}

/// A poster leg that lays nothing down is a skipped poster, not a failed run: `poster=` only appears when
/// the file is really there, whatever the leg answered.
/// The poster leg's stand-in: answers `Ok` and lays nothing down, so a run that asks for a poster still
/// gets `poster=` false (the file has to exist for the tag to claim it).
fn no_poster() -> Poster {
    Rc::new(|_argv: &[String]| Ok(()))
}

/// §5's rule that makes `publish/publish.json` §3's odd one out: ▶ never touches it.
#[test]
fn sec_08_produce_3_data_s8_publish_json_survives_a_render() {
    let (_root, tree) = common::tree_in("publish-survives");
    let record = Publish {
        title: "kept by the person".into(),
        own: true,
        ..Default::default()
    };
    naivepost::publish::save(&record, &tree).expect("the record is written");
    let before = std::fs::read(tree.publish_json()).unwrap();
    assert!(
        data::written(&tree).contains(&data::DATA_FILES[10]),
        "and the catalogue reports it"
    );

    let (tree2, run) = run_over(
        &tree,
        vec![common::seg(0.0, 10.0)],
        vec![Entry {
            s: 0.0,
            e: 4.0,
            at: 0.0,
            text: "a line".into(),
            ..Default::default()
        }],
        Produce::default(),
    );
    let spy = Spy::new();
    spy.run(&run, &tree2, &mats(&[], exec::Cues::default()));
    assert_eq!(
        std::fs::read(tree.publish_json()).unwrap(),
        before,
        "the render rewrote the upload record"
    );
    assert_eq!(
        naivepost::publish::load(&tree).unwrap().title,
        "kept by the person"
    );
}

/// §6's "cleared at the start of every render": everything in `clips/` goes, the folder stays, and the log
/// line says how many kinds of scratch file that sweep covers.
#[test]
fn sec_08_produce_3_data_s9_the_scratch_sweep_clears_every_clips_kind() {
    let (_root, tree) = common::tree_in("sweep");
    let scratch = tree.clips_dir();
    std::fs::create_dir_all(scratch.join("leftover.frames")).unwrap();
    for name in [
        "c000_ab12.mp4",
        "c000_ab12.srt",
        "c000_t01.svg",
        "ranks-filled.svg",
        "joined.mp4",
    ] {
        std::fs::write(scratch.join(name), b"an older render's leftovers").unwrap();
    }
    let (tree2, run) = run_over(
        &tree,
        vec![common::seg(0.0, 10.0)],
        Vec::new(),
        Produce::default(),
    );
    let spy = Spy::new();
    spy.run(&run, &tree2, &mats(&[], exec::Cues::default()));
    // The sweep is S1, so the leftovers are judged as the run SAW them: read the log's own count and check
    // that every §6 name the test planted is gone from what this run did NOT write.
    let left: Vec<String> = std::fs::read_dir(&scratch)
        .map(|d| {
            d.flatten()
                .map(|e| e.path())
                .filter(|p| p.is_file())
                .map(|p| p.display().to_string())
                .collect()
        })
        .unwrap_or_default();
    for stale in ["c000_ab12.srt", "c000_t01.svg", "ranks-filled.svg"] {
        let path = scratch.join(stale);
        assert!(
            !left.contains(&path.display().to_string()),
            "{stale} survived the sweep: {left:?}"
        );
    }
    // `joined.mp4` was planted AND written by this run (the fake spawner lays down its own output), so the
    // honest claim about it is that the sweep counted it: the planted bytes are gone, replaced by the run's.
    assert_eq!(
        std::fs::read(scratch.join("joined.mp4")).ok(),
        Some(b"a rendered file of some length".to_vec()),
        "the planted join was cleared and rewritten by this run"
    );
    assert!(
        scratch.is_dir(),
        "the folder itself stays: the run writes into it seconds later"
    );
    let groups = data::scratch_groups();
    assert_eq!(
        groups,
        data::catalogue()
            .iter()
            .filter(|e| data::kind(e) == data::Kind::Scratch)
            .count(),
        "the count the log quotes is the catalogue's own"
    );
    assert!(
        spy.logs_vec()
            .iter()
            .any(|l| l.contains("cleared") && l.contains(&format!("{groups} kinds"))),
        "the S1 line names the sweep it made: {:?}",
        spy.logs_vec()
    );
    // And the sweep is `render::clear_scratch`'s own contract, taken straight: files go, folder stays.
    let (_root2, tree2) = common::tree_in("sweep-direct");
    let bare = tree2.clips_dir();
    std::fs::create_dir_all(&bare).unwrap();
    std::fs::write(bare.join("stray.txt"), b"left by a run that was cancelled").unwrap();
    assert_eq!(
        1,
        data::clear_scratch(&tree2).expect("the sweep reports"),
        "one file went"
    );
    assert!(bare.is_dir(), "the folder itself stays");
    assert_eq!(
        0,
        std::fs::read_dir(&bare).unwrap().count(),
        "and nothing is inside it"
    );
}

// ---- the plumbing every test above drives through --------------------------------------------------

fn run_over(
    tree: &Tree,
    segs: Vec<Seg>,
    lines: Vec<Entry>,
    settings: Produce,
) -> (Tree, flow::Run) {
    let run = flow::snapshot(
        Some(naivepost::cut::Cut {
            segs,
            aspect: "16:9".into(),
            ..Default::default()
        }),
        tree,
        lines,
        settings,
        &project(),
        Some(Publish::default()),
        "1".to_string(),
    );
    (tree.clone(), run)
}

fn no_fx(_: &Seg) -> Vec<naivepost::cut::Fx> {
    Vec::new()
}
fn any_source(_: &render::Clip) -> Option<String> {
    Some("/media/take.mp4".to_string())
}
fn no_wav(_: &Entry) -> Option<PathBuf> {
    None
}
fn seconds_2_5(_: &Entry) -> Option<f64> {
    Some(2.5)
}
fn no_words(_: &render::Clip) -> Vec<subs::Word> {
    Vec::new()
}
fn sources() -> Vec<Source> {
    project().sources
}
fn missing_is_gone(_: &str) -> bool {
    true
}
fn no_translate(
    _m: &str,
    n: &[usize],
) -> Result<std::collections::BTreeMap<usize, String>, String> {
    Ok(n.iter().map(|i| (*i, format!("translated {i}"))).collect())
}

/// THE Materials constructor: one door, nothing to mangle (same shape `tests/suite/produce_render_exec.rs` uses).
fn mats<'a>(lanes: &'a [naivepost::cut::Lane], cues: exec::Cues) -> exec::Materials<'a> {
    exec::Materials {
        exists: &missing_is_gone,
        fx_for: &no_fx,
        lanes,
        src_shape: (1920, 1080),
        wav_of: &no_wav,
        source_file: &any_source,
        on_recording: true,
        footage_left: &render::unknown_footage,
        filmed_runs: naivepost::produce_plan_logs::NO_TAPE,
        base_of: &|_seg| String::new(),
        wav_seconds: &seconds_2_5,
        cues,
        words: &no_words,
        translate: &no_translate,
        sources: &sources,
        probe: &|_file| None,
    }
}

/// One walk's record: every argv spawned and every line logged. The spawner writes the file each command
/// names last, so existence and size claims below are about real files.
#[derive(Clone, Default)]
struct Spy {
    argv: SpawnedSteps,
    logs: Rc<RefCell<Vec<String>>>,
}

impl Spy {
    fn new() -> Spy {
        Spy::default()
    }

    fn run(&self, run: &flow::Run, tree: &Tree, materials: &exec::Materials) -> flow::Rendered {
        let argv = self.argv.clone();
        let logs = self.logs.clone();
        exec::run_render(
            run,
            tree,
            materials,
            |line| logs.borrow_mut().push(line.to_string()),
            |_line| Ok(()),
            |_at, _which| true,
            |command: &exec::Command| {
                argv.borrow_mut()
                    .push((command.step.to_string(), command.argv.clone()));
                if let Some(out) = command.argv.last() {
                    let path = Path::new(out);
                    if let Some(dir) = path.parent() {
                        let _ = std::fs::create_dir_all(dir);
                    }
                    let _ = std::fs::write(path, b"a rendered file of some length");
                }
                Ok(())
            },
        )
    }

    fn joined(&self) -> String {
        self.argv
            .borrow()
            .iter()
            .map(|(_, args)| args.join(" "))
            .collect::<Vec<String>>()
            .join("\n")
    }

    fn logs_vec(&self) -> Vec<String> {
        self.logs.borrow().clone()
    }
}
