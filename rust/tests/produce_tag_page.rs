//! F5.5 S5 — the `<video>` tag page reached from the real ▶ (`spec/08-produce.md` §F5.1 S5, §F5.5).
//!
//! `tests/produce_embed.rs` pins the RULES (the tag's text, which files count as tracks, the notes) over
//! `produce_embed`. This file pins the WALK that makes them reachable: `produce_tag_page::build_and_write`
//! gathers what is on disk, asks for the poster through an injected leg, and writes `final.html`; and the
//! Produce page calls it at both doors of a run — after both halves, and on the up-to-date skip too, which
//! is why this item came back once before ("a flow that cannot be reached from the UI").
//!
//! The poster leg is injected exactly as `narrate_tts::speak_line` injects its network legs: no test here
//! needs ffmpeg (this container has none), and the argv the walk built is recorded so `-q:v 2` — the ladder
//! rung for the JPEG 90 §F5.5 asks for — is checked rather than assumed.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Mutex;

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut::Seg;
use naivepost::layout::Tree;
use naivepost::produce_embed as embed;
use naivepost::produce_tag_page as tag;
use naivepost::project::{Codec, Container};
use naivepost::roles::Language;
use naivepost::shell::Page;
use naivepost::ui;
#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::{fixture_dir, hold_last_window, release_last_window, settle};

// ---- logic helpers -----------------------------------------------------------------

fn tree_in(tag_name: &str) -> (PathBuf, Tree) {
    let dir = std::env::temp_dir().join(format!("np-f55w-{}-{tag_name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let project = dir.join("demo.naivepost");
    std::fs::create_dir_all(&project).unwrap();
    (dir, Tree::new(&project).unwrap())
}

fn langs() -> Vec<Language> {
    vec![
        Language { code: "en".into(), tag: "en-US".into(), name: "English".into() },
        Language { code: "de".into(), tag: "de-DE".into(), name: "German".into() },
    ]
}

/// Writes a WebVTT body into the produce folder so there is something to list.
fn write_vtt(tree: &Tree, code: Option<&str>) {
    let path = tree.final_vtt(code);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        &path,
        "WEBVTT\n\n00:00:00.000 --> 00:00:02.000\na caption\n",
    )
    .unwrap();
}

fn write_thumbnail(tree: &Tree) {
    let path = tree.thumbnail_png();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"a drawn thumbnail").unwrap();
}

/// A poster leg that records every argv it was handed and "writes" the file it names last.
#[derive(Clone, Default)]
struct Legs {
    calls: Rc<RefCell<Vec<Vec<String>>>>,
    fail: bool,
}

impl Legs {
    fn ok(&self) -> Box<tag::MakePoster> {
        let calls = self.calls.clone();
        Box::new(move |argv: &[String]| {
            calls.borrow_mut().push(argv.to_vec());
            if let Some(out) = argv.last() {
                let path = Path::new(out);
                if let Some(dir) = path.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                let _ = std::fs::write(path, b"a jpeg poster");
            }
            Ok(())
        })
    }

    fn failing(&self) -> Box<tag::MakePoster> {
        let calls = self.calls.clone();
        Box::new(move |argv: &[String]| {
            calls.borrow_mut().push(argv.to_vec());
            Err("no ffmpeg here".to_string())
        })
    }

    fn last(&self) -> Vec<String> {
        self.calls
            .borrow()
            .last()
            .cloned()
            .unwrap_or_default()
    }
}

/// Run the walk with mp4/h264 defaults and read the page back.
fn run_walk(
    tree: &Tree,
    container: Container,
    codec: Codec,
    legs: &Legs,
    log: &Rc<RefCell<Vec<String>>>,
) -> tag::TagPage {
    tag::build_and_write(
        tree,
        container,
        codec,
        "en",
        &langs(),
        &*legs.ok(),
        |line| log.borrow_mut().push(line.to_string()),
    )
}

fn read_html(tree: &Tree) -> String {
    std::fs::read_to_string(tree.final_html()).expect("the page reads back")
}

// ---- the logic tests ----------------------------------------------------------------

/// §F5.5: `<stem>.html` is the bare tag plus one `<track>` per `.vtt` on disk, the video's own language
/// first and `default`, and nothing else on the track line.
#[test]
fn f5_5_s5_the_page_is_the_bare_tag_with_every_vtt_on_disk() {
    let (_root, tree) = tree_in("bare");
    write_vtt(&tree, None);
    write_vtt(&tree, Some("de"));
    write_thumbnail(&tree);
    let legs = Legs::default();
    let log = Rc::new(RefCell::new(Vec::new()));
    let page = run_walk(&tree, Container::Mp4, Codec::H264, &legs, &log);

    assert!(page.written, "the page landed");
    assert_eq!(page.tracks, 2, "two vtt files, two tracks: {page:?}");
    let body = read_html(&tree);
    let own_at = body
        .find(r#"<track src="final.vtt" srclang="en" default>"#)
        .unwrap_or_else(|| panic!("the video's own track is missing or not default: {body}"));
    let de_at = body
        .find(r#"<track src="final.de.vtt" srclang="de">"#)
        .unwrap_or_else(|| panic!("the German track is missing: {body}"));
    assert!(own_at < de_at, "own language first: {body}");
    assert!(body.starts_with(r#"<video poster="final.jpg" controls src="final.mp4" preload="none">"#), "{body}");
    assert!(!body.contains("label="), "the tag stays bare, no label=: {body}");
    assert!(!body.contains("kind="), "…and no kind=: {body}");
    assert!(body.ends_with("</video>\n"), "{body}");
    // It equals what the rule module itself would have written for the same inputs.
    let expected = embed::tag(
        "final.mp4",
        Some("final.jpg"),
        &embed::tracks(
            &embed::list_vtt(tree.final_jpg().parent().unwrap()),
            "en",
            &langs(),
        ),
    );
    assert_eq!(body, expected, "the page is `embed::tag`'s answer, unchanged");
}

/// §F5.5's "JPEG 90": the poster is asked for as ffmpeg's `-q:v 2` off `publish/thumbnail.png`, into
/// `produce/final.jpg`.
#[test]
fn f5_5_s5_the_poster_is_asked_as_jpeg_90_from_the_thumbnail() {
    let (_root, tree) = tree_in("poster");
    write_thumbnail(&tree);
    let legs = Legs::default();
    let log = Rc::new(RefCell::new(Vec::new()));
    let page = run_walk(&tree, Container::Mp4, Codec::H264, &legs, &log);

    let argv = legs.last();
    assert_eq!(argv.first().map(String::as_str), Some("ffmpeg"), "{argv:?}");
    let q = argv
        .iter()
        .position(|a| a == "-q:v")
        .unwrap_or_else(|| panic!("no -q:v in the poster command: {argv:?}"));
    assert_eq!(argv[q + 1], embed::POSTER_QSCALE, "the ladder rung for JPEG 90");
    let i = argv.iter().position(|a| a == "-i").expect("-i present");
    assert_eq!(argv[i + 1], tree.thumbnail_png().display().to_string(), "{argv:?}");
    assert_eq!(argv.last().unwrap(), &tree.final_jpg().display().to_string(), "{argv:?}");
    assert!(page.poster, "the poster is there to point at: {page:?}");
}

/// No thumbnail yet: said quietly with the reason, and the tag goes out with no `poster=` at all.
#[test]
fn f5_5_s5_no_thumbnail_is_said_and_the_tag_has_no_poster() {
    let (_root, tree) = tree_in("noposter");
    write_vtt(&tree, None);
    let legs = Legs::default();
    let log = Rc::new(RefCell::new(Vec::new()));
    let page = run_walk(&tree, Container::Mp4, Codec::H264, &legs, &log);

    assert!(!page.poster, "nothing to point at: {page:?}");
    assert!(legs.calls.borrow().is_empty(), "no poster job was sent without a thumbnail");
    let body = read_html(&tree);
    assert!(!body.contains("poster="), "no poster attribute: {body}");
    assert!(body.starts_with("<video controls src=\"final.mp4\""), "{body}");
    let logged = log.borrow();
    assert!(
        logged.iter().any(|l| l.contains(embed::no_poster_reason())),
        "the reason was said: {logged:?}"
    );
    assert!(page.written, "the page is still written: {page:?}");
}

/// A caption-less video still gets a page, and the http note is withheld: there is nothing to refuse.
#[test]
fn f5_5_s5_a_video_with_no_captions_still_gets_a_page_and_no_http_note() {
    let (_root, tree) = tree_in("nocaptions");
    write_thumbnail(&tree);
    let legs = Legs::default();
    let log = Rc::new(RefCell::new(Vec::new()));
    let page = run_walk(&tree, Container::Mp4, Codec::H264, &legs, &log);

    assert_eq!(page.tracks, 0, "no vtt on disk: {page:?}");
    assert!(page.written, "still a page: {page:?}");
    let body = read_html(&tree);
    assert!(!body.contains("<track"), "no track lines: {body}");
    let logged = log.borrow();
    assert!(
        !logged.iter().any(|l| l.contains("http")),
        "nothing to withhold, so no http note: {logged:?}"
    );
}

/// §F5.5's first note: mkv and h265 are not web-playable; mp4+h264 says nothing.
#[test]
fn f5_5_s5_the_web_notes_reach_the_log() {
    let (_root, tree) = tree_in("notes");
    write_vtt(&tree, None);
    let legs = Legs::default();

    let log = Rc::new(RefCell::new(Vec::new()));
    run_walk(&tree, Container::Mkv, Codec::H264, &legs, &log);
    let logged = log.borrow();
    assert!(
        logged.iter().any(|l| l.contains("Matroska")),
        "mkv names the container nobody plays: {logged:?}"
    );
    drop(logged);

    let log = Rc::new(RefCell::new(Vec::new()));
    run_walk(&tree, Container::Mp4, Codec::H265, &legs, &log);
    let logged = log.borrow();
    assert!(
        logged.iter().any(|l| l.contains("h265")),
        "h265 names the codec Firefox will not decode: {logged:?}"
    );
    drop(logged);

    let log = Rc::new(RefCell::new(Vec::new()));
    run_walk(&tree, Container::Mp4, Codec::H264, &legs, &log);
    let logged = log.borrow();
    assert!(
        !logged.iter().any(|l| l.contains("Matroska") || l.contains("h265")),
        "mp4/h264 is a page's own format and says nothing: {logged:?}"
    );
    // And with a track present, the http-vs-file:// note is there in every case.
    assert!(
        logged.iter().any(|l| l.contains("file://")),
        "the http note rides along when there is a track: {logged:?}"
    );
}

/// An unwritable page is said and changes nothing else: the answers about poster and tracks still come back.
#[test]
fn f5_5_s5_an_unwritable_page_is_said_and_changes_nothing_else() {
    let (_root, tree) = tree_in("unwritable");
    write_vtt(&tree, None);
    write_vtt(&tree, Some("de"));
    write_thumbnail(&tree);
    // A DIRECTORY where the page should be: the write cannot land.
    std::fs::create_dir_all(tree.final_html()).unwrap();
    let legs = Legs::default();
    let log = Rc::new(RefCell::new(Vec::new()));
    let page = run_walk(&tree, Container::Mp4, Codec::H264, &legs, &log);

    assert!(!page.written, "the write failed: {page:?}");
    assert_eq!(page.tracks, 2, "the tracks were still counted: {page:?}");
    assert!(page.poster, "the poster was still made: {page:?}");
    let logged = log.borrow();
    assert!(
        logged.iter().any(|l| l.contains("could not write")),
        "the failure was said: {logged:?}"
    );
    assert!(
        !logged.iter().any(|l| l.starts_with(">>> the <video> tag")),
        "a page that did not land is not announced: {logged:?}"
    );
}

/// A poster job that fails costs the poster only: the tag is written, without a `poster=`.
#[test]
fn f5_5_s5_a_poster_job_that_fails_still_writes_the_tag_without_a_poster() {
    let (_root, tree) = tree_in("posterfail");
    write_vtt(&tree, None);
    write_thumbnail(&tree);
    let legs = Legs::default();
    let log = Rc::new(RefCell::new(Vec::new()));
    let page = tag::build_and_write(
        &tree,
        Container::Mp4,
        Codec::H264,
        "en",
        &langs(),
        &*legs.failing(),
        |line| log.borrow_mut().push(line.to_string()),
    );

    assert!(!page.poster, "the job failed, so there is no poster: {page:?}");
    assert!(page.written, "the page still got written: {page:?}");
    let body = read_html(&tree);
    assert!(!body.contains("poster="), "and says so by omission: {body}");
    assert!(!legs.calls.borrow().is_empty(), "the job was still attempted");
}

// ---- the widget wire ---------------------------------------------------------------

static TAG_FAIL: Mutex<Option<String>> = Mutex::new(None);
static SKIP_TAG_FAIL: Mutex<Option<String>> = Mutex::new(None);

/// Run one check so a panic becomes a recorded message rather than an abort through the C trampoline.
fn record(slot: &Mutex<Option<String>>, label: &str, body: impl FnOnce() -> Result<(), String> + std::panic::UnwindSafe) {
    let outcome = std::panic::catch_unwind(body);
    let failure = match outcome {
        Ok(Ok(())) => None,
        Ok(Err(why)) => Some(why),
        Err(payload) => Some(
            payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "<non-string panic>".to_string()),
        ),
    };
    if let Some(why) = failure {
        *slot.lock().unwrap() = Some(format!("{label}: {why}"));
    }
}

thread_local! {
    static CURRENT_APP: std::cell::RefCell<Option<adw::Application>> = const { std::cell::RefCell::new(None) };
}

fn app_in_round() -> adw::Application {
    CURRENT_APP
        .with(|held| held.borrow().clone())
        .expect("run_round published the application")
}

fn session(root: &Path) -> Tree {
    let dir = naivepost::startup::session_dir(root);
    std::fs::create_dir_all(&dir).expect("session folder created");
    Tree::new(&dir).expect("the session folder is a project tree")
}

fn two_segs() -> Vec<Seg> {
    let mut first = Seg::default();
    first.s = 0.0;
    first.e = 9.5;
    let mut second = Seg::default();
    second.s = 10.0;
    second.e = 30.0;
    vec![first, second]
}

fn write_cut(tree: &Tree) {
    let cut = naivepost::cut::Cut {
        segs: two_segs(),
        aspect: "16:9".to_string(),
        ..Default::default()
    };
    let file = tree.cut_json();
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, serde_json::to_string_pretty(&cut).unwrap()).unwrap();
}

fn produce_window(app: &adw::Application) -> adw::ApplicationWindow {
    release_last_window();
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, Page::Produce.label());
    hold_last_window(window.clone());
    window.present();
    settle();
    window
}

/// Seed the page's rows the way `tests/produce_stamp_widgets.rs` does, so the press hashes real settings.
fn seed_page(window: &adw::ApplicationWindow) {
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

/// The scripted spawner: no ffmpeg here, so each command's own output file is written here.
fn scripted_spawn() -> naivepost::produce_translate::SpawnSlot {
    Rc::new(|command: &naivepost::produce_exec::Command| {
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

fn press_play(window: &adw::ApplicationWindow) -> String {
    let button = ui::line_step_button(window, "play-button").expect("the run bar drew a play-button");
    button.emit_by_name::<()>("clicked", &[]);
    settle();
    ui::find_status(window.upcast_ref())
        .expect("the window has a status line")
        .text()
        .to_string()
}

/// Check A: the real ▶ reaches S5 and `produce/final.html` comes back with both tracks.
fn check_press_writes_the_tag_page(root: &Path) {
    let tree = session(root);
    write_cut(&tree);
    write_vtt(&tree, None);
    write_vtt(&tree, Some("de"));
    naivepost::produce_translate::set_spawn_for_test(scripted_spawn());

    let window = produce_window(&app_in_round());
    ui::seed_review_cut(
        &window,
        &naivepost::cut::Cut {
            segs: two_segs(),
            aspect: "16:9".to_string(),
            ..Default::default()
        },
    );
    seed_page(&window);
    let said = press_play(&window);
    assert_ne!(said, naivepost::cut::NO_CUT_YET, "refused for want of a cut: {said:?}");

    let html = tree.final_html();
    assert!(html.exists(), "S5 wrote the page at {}", html.display());
    let body = std::fs::read_to_string(&html).expect("it reads back");
    let own_at = body
        .find(r#"<track src="final.vtt" srclang="en" default>"#)
        .unwrap_or_else(|| panic!("the own-language default track is missing: {body}"));
    let de_at = body
        .find(r#"<track src="final.de.vtt" srclang="de">"#)
        .unwrap_or_else(|| panic!("the German track is missing: {body}"));
    assert!(own_at < de_at, "the video's own language comes first: {body}");
    let logs = ui::window::window_logs();
    assert!(
        logs.iter()
            .any(|l| l == &embed::wrote_tag_log(&html.display().to_string())),
        "the run announced the page it wrote: {:?}",
        logs.iter().filter(|l| l.contains("video")).collect::<Vec<&String>>()
    );
    window.close();
    settle();
}

/// Check B: an encode skipped by the stamp still rewrites the page (§F5.5's own sentence).
fn check_skipped_encode_still_rewrites_the_page(root: &Path) {
    let tree = session(root);
    write_cut(&tree);
    write_vtt(&tree, None);
    write_vtt(&tree, Some("de"));
    // Up to date: the video stands there and the stamp answers the current hash.
    let video = tree.final_video("mp4");
    std::fs::create_dir_all(video.parent().unwrap()).unwrap();
    std::fs::write(&video, b"a finished video of some length").unwrap();
    let settings = naivepost::project::Produce::default();
    let project = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let current = naivepost::produce_stamp::gate_stamp(
        &tree,
        &settings,
        &two_segs(),
        &project.sources,
        "16:9",
        Some("1"),
        project.no_narration,
    );
    naivepost::produce_stamp::write_stamp(&tree, &current).expect("stamp written");
    // A sentinel page: if S5 does not run on the skip path, this survives.
    std::fs::write(tree.final_html(), "<<<OLD>>>\n").unwrap();
    naivepost::produce_translate::set_spawn_for_test(scripted_spawn());

    let window = produce_window(&app_in_round());
    ui::seed_review_cut(
        &window,
        &naivepost::cut::Cut {
            segs: two_segs(),
            aspect: "16:9".to_string(),
            ..Default::default()
        },
    );
    seed_page(&window);
    let said = press_play(&window);
    assert_eq!(said, naivepost::produce_flow::SKIP_LOG, "the press skipped the encode: {said:?}");
    let body = std::fs::read_to_string(tree.final_html()).expect("the page is there");
    assert!(!body.contains("<<<OLD>>>"), "the sentinel survived — S5 did not run on the skip path: {body}");
    assert!(
        body.contains(r#"srclang="en" default"#),
        "the rewritten page carries the own-language track: {body}"
    );
    window.close();
    settle();
}

fn run_round(app: &adw::Application) {
    CURRENT_APP.with(|held| *held.borrow_mut() = Some(app.clone()));

    let base = std::env::current_dir().expect("cwd pinned before the run");
    let encode_root = base.join("encode");
    std::fs::create_dir_all(&encode_root).unwrap();
    std::env::set_current_dir(&encode_root).unwrap();
    let owned = encode_root.clone();
    record(
        &TAG_FAIL,
        "press-writes-tag-page",
        std::panic::AssertUnwindSafe(move || {
            check_press_writes_the_tag_page(&owned);
            Ok(())
        }),
    );

    let skip_root = base.join("skip");
    std::fs::create_dir_all(&skip_root).unwrap();
    std::env::set_current_dir(&skip_root).unwrap();
    let owned = skip_root.clone();
    record(
        &SKIP_TAG_FAIL,
        "skipped-encode-rewrites-page",
        std::panic::AssertUnwindSafe(move || {
            check_skipped_encode_still_rewrites_the_page(&owned);
            Ok(())
        }),
    );

    let app = app.clone();
    glib::idle_add_local(move || {
        app.quit();
        glib::ControlFlow::Break
    });
}

#[test]
fn f5_5_s5_pressing_produce_writes_the_tag_page_through_the_real_button() {
    let root = std::env::temp_dir().join(format!("np-f55-tag-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("temp root created");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");

    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(run_round);
        app.run_with_args(&["naivepost"]);
    });

    let a = TAG_FAIL.lock().unwrap().take();
    let b = SKIP_TAG_FAIL.lock().unwrap().take();
    std::fs::remove_dir_all(&root).ok();
    assert!(a.is_none(), "{}", a.unwrap_or_default());
    assert!(b.is_none(), "{}", b.unwrap_or_default());
}
