//! F5.5 The `<video>` tag — the two gaps the last round left, pinned against a real session folder on disk.
//!
//! `tests/suite/produce_tag_page.rs` pins the walk's shape (bare tag, notes, unwritable page) and
//! `tests/suite/produce_embed.rs` pins the rules. What neither could catch is what this round fixed in the program:
//! - **`src=` named an mp4 whatever was rendered.** The walk threw away the video `embed::paths` had already
//!   resolved for the chosen container and rebuilt the name as `final.mp4`, so an mkv or webm run published a
//!   page pointing at a file that does not exist.
//! - **The poster could never happen live.** §F5.6's draw leg writes `publish/thumbnail-plain.png` and its
//!   print leg writes `publish/thumbnail.png` only from that plain file, so a run whose print pass did not
//!   happen left the tag with no `poster=` at all even though a picture was on disk.
//!
//! Every test here drives `produce_tag_page::build_and_write` — the door `ui::produce_page`'s finish calls
//! after both halves of ▶ — against a scratch `.naivepost` folder, and reads back the `.html` the program
//! actually wrote. The poster leg is injected through the seam `build_and_write` already takes (the same
//! injection `narrate_tts::speak_line` uses for its network legs): the encoder is the program's spawner, and
//! a test replaces ONLY the spawner, never the work.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use naivepost::layout::Tree;
use naivepost::produce_embed as embed;
use naivepost::produce_tag_page as tag;
use naivepost::project::{Codec, Container};
use naivepost::roles::Language;

/// A project folder of our own: nothing lands in a real project or another test's tree.
fn tree_in(tag_name: &str) -> (PathBuf, Tree) {
    let dir = std::env::temp_dir().join(format!("np-f55live-{}-{tag_name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("demo.naivepost")).unwrap();
    let project = dir.join("demo.naivepost");
    (dir, Tree::new(&project).unwrap())
}

fn langs() -> Vec<Language> {
    vec![
        Language {
            code: "en".into(),
            tag: "en-US".into(),
            name: "English".into(),
        },
        Language {
            code: "de".into(),
            tag: "de-DE".into(),
            name: "German".into(),
        },
        Language {
            code: "fr".into(),
            tag: "fr-FR".into(),
            name: "French".into(),
        },
    ]
}

/// Bytes where the render would have put its finished video, so a `src=` naming it names something real.
fn write_video(tree: &Tree, container: Container) {
    let word = match container {
        Container::Mp4 => "mp4",
        Container::Mkv => "mkv",
        Container::Webm => "webm",
    };
    let path = tree.final_video(word);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"not really a video, but a file all the same").unwrap();
    assert!(
        path.is_file(),
        "the test's own video landed: {}",
        path.display()
    );
}

fn write_vtt(tree: &Tree, code: Option<&str>) {
    let path = tree.final_vtt(code);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        &path,
        "WEBVTT\n\n00:00:00.000 --> 00:00:02.000\na caption\n",
    )
    .unwrap();
}

/// A PNG the size header a JPEG conversion would accept, written where each pass of §F5.6 puts its picture.
fn write_png(path: &Path) {
    let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, 64, 36).expect("a surface");
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).unwrap();
    }
    let mut bytes = Vec::new();
    surface
        .write_to_png(&mut bytes)
        .expect("the scratch png writes");
    std::fs::write(path, bytes).expect("the scratch png lands");
    assert!(path.is_file(), "{} exists", path.display());
}

/// A poster leg that records the argv it was handed and writes the file that argv names last, which is what
/// makes `page.poster` mean "a jpeg is on disk" rather than "a job was sent".
#[derive(Clone, Default)]
struct Legs {
    calls: Rc<RefCell<Vec<Vec<String>>>>,
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

    fn last(&self) -> Vec<String> {
        self.calls.borrow().last().cloned().unwrap_or_default()
    }
}

/// Run the walk through the door the page runs, reading its log into a list a test can search.
fn run(
    tree: &Tree,
    container: Container,
    codec: Codec,
    own_language: &str,
    legs: &Legs,
) -> (tag::TagPage, Vec<String>) {
    let log = RefCell::new(Vec::new());
    let page = tag::build_and_write(
        tree,
        container,
        codec,
        own_language,
        &langs(),
        &*legs.ok(),
        |line| log.borrow_mut().push(line.to_string()),
    );
    (page, log.into_inner())
}

fn read_html(tree: &Tree) -> String {
    std::fs::read_to_string(tree.final_html()).expect("the page reads back")
}

/// The gap: `src=` must name the container the run wrote, because that is the file that exists.
#[test]
fn f5_5_s1_src_names_the_container_actually_written() {
    for (container, word) in [
        (Container::Mp4, "mp4"),
        (Container::Mkv, "mkv"),
        (Container::Webm, "webm"),
    ] {
        let (_root, tree) = tree_in(&format!("src-{word}"));
        write_video(&tree, container);
        let legs = Legs::default();
        let (page, _) = run(&tree, container, Codec::H264, "en", &legs);
        let body = read_html(&tree);
        assert!(page.written, "{word}: the page landed: {page:?}");
        assert!(
            body.contains(&format!("src=\"final.{word}\"")),
            "{word}: the tag names the rendered file: {body}"
        );
        if word != "mp4" {
            assert!(
                !body.contains("final.mp4"),
                "{word}: no leftover mp4 in a {word} page: {body}"
            );
        }
    }

    // And §F5.5's first note rides along with the container the browser will not play.
    let (_root, tree) = tree_in("src-mkv-note");
    write_video(&tree, Container::Mkv);
    let legs = Legs::default();
    let (_, log) = run(&tree, Container::Mkv, Codec::H264, "en", &legs);
    assert!(
        log.iter().any(|line| line.contains("Matroska")),
        "an mkv page says the container is not web-playable: {log:?}"
    );
}

/// The live poster case: whichever thumbnail file the run left behind becomes the JPEG, and only having
/// NEITHER counts as no poster.
#[test]
fn f5_5_s2_poster_made_from_the_picture_on_disk() {
    // (a) the printed picture, which wins when both exist.
    let (_root, tree) = tree_in("poster-printed");
    write_png(&tree.thumbnail_png());
    write_png(&tree.thumbnail_plain_png());
    let legs = Legs::default();
    let (page, _) = run(&tree, Container::Mp4, Codec::H264, "en", &legs);
    let argv = legs.last();
    assert_eq!(
        argv.iter()
            .position(|a| a == "-i")
            .map(|i| argv[i + 1].clone()),
        Some(tree.thumbnail_png().display().to_string()),
        "the printed picture is the source when it exists: {argv:?}"
    );
    assert!(
        page.poster && tree.final_jpg().is_file(),
        "and the jpeg is there to point at: {page:?}"
    );
    assert!(
        read_html(&tree).contains("poster=\"final.jpg\""),
        "the tag says so"
    );

    // (b) ONLY the plain draw — the case that used to lose the poster entirely.
    let (_root, tree) = tree_in("poster-plain");
    write_png(&tree.thumbnail_plain_png());
    assert!(
        !tree.thumbnail_png().is_file(),
        "nothing was printed in this session"
    );
    let legs = Legs::default();
    let (page, log) = run(&tree, Container::Mp4, Codec::H264, "en", &legs);
    let argv = legs.last();
    assert_eq!(
        argv.iter()
            .position(|a| a == "-i")
            .map(|i| argv[i + 1].clone()),
        Some(tree.thumbnail_plain_png().display().to_string()),
        "the plain picture is a legitimate poster source: {argv:?}"
    );
    assert!(
        page.poster,
        "the poster happened without a print pass: {page:?}"
    );
    assert!(
        read_html(&tree).contains("poster=\"final.jpg\""),
        "so the tag carries one: {}",
        read_html(&tree)
    );
    assert!(
        !log.iter()
            .any(|line| line.contains(embed::no_poster_reason())),
        "nothing is missing, so nothing apologises: {log:?}"
    );

    // (c) neither: said quietly, and the tag goes out with no `poster=`.
    let (_root, tree) = tree_in("poster-none");
    let legs = Legs::default();
    let (page, log) = run(&tree, Container::Mp4, Codec::H264, "en", &legs);
    assert!(!page.poster, "nothing on disk to convert: {page:?}");
    assert!(
        legs.calls.borrow().is_empty(),
        "no job was sent for a missing picture"
    );
    assert!(
        log.iter()
            .any(|line| line == &embed::no_poster_log(embed::no_poster_reason())),
        "the reason was said in the program's own words: {log:?}"
    );
    assert!(
        !read_html(&tree).contains("poster="),
        "and the tag omits it: {}",
        read_html(&tree)
    );
}

/// §F5.5's track rule, off files on disk: the video's own language first and `default`, everything else
/// after it without, and only the video's own sidecars counted at all.
#[test]
fn f5_5_s3_tracks_from_disk_own_language_first() {
    let (_root, tree) = tree_in("tracks");
    write_vtt(&tree, None);
    write_vtt(&tree, Some("de"));
    write_vtt(&tree, Some("fr"));
    // Not the video's sidecars: a stranger's vtt and the desktop-player srt a <track> could not read.
    let folder = tree.final_vtt(None).parent().unwrap().to_path_buf();
    std::fs::write(folder.join("other.vtt"), "WEBVTT\n").unwrap();
    std::fs::write(
        folder.join("final.srt"),
        "1\n00:00:00,000 --> 00:00:02,000\nx\n",
    )
    .unwrap();

    // This video speaks German, so `final.vtt` IS the German track and must lead with `default`.
    let legs = Legs::default();
    let (page, log) = run(&tree, Container::Mp4, Codec::H264, "de", &legs);
    let body = read_html(&tree);
    assert_eq!(
        page.tracks, 3,
        "three of the folder's files are the video's tracks: {page:?}"
    );
    let de = body
        .find("<track src=\"final.vtt\" srclang=\"de\" default>")
        .unwrap_or_else(|| panic!("the own track must lead and be default: {body}"));
    let en = body
        .find("<track src=\"final.de.vtt\" srclang=\"de\">")
        .or_else(|| body.find("srclang=\"en\""))
        .expect("the translations follow the own track");
    assert!(de < en, "own language first: {body}");
    assert_eq!(
        body.matches(" default>").count(),
        1,
        "exactly one default, or two caption sets fight: {body}"
    );
    assert!(
        !body.contains("other.vtt"),
        "a stranger's vtt is not this video's: {body}"
    );
    assert!(
        !body.contains(".srt"),
        "a <track> parses WebVTT only: {body}"
    );
    assert!(
        log.iter().any(|line| line.contains("file://")),
        "with tracks present the http note is owed: {log:?}"
    );
}

/// The page is exactly the four lines §F5.5 shows, plus the log line that announces where they are.
#[test]
fn f5_5_s4_the_page_is_the_bare_tag() {
    let (_root, tree) = tree_in("bare");
    write_video(&tree, Container::Mp4);
    write_png(&tree.thumbnail_png());
    write_vtt(&tree, None);
    write_vtt(&tree, Some("de"));
    let legs = Legs::default();
    let (page, log) = run(&tree, Container::Mp4, Codec::H264, "en", &legs);
    let body = read_html(&tree);

    assert_eq!(
        body.lines().next().unwrap(),
        "<video poster=\"final.jpg\" controls src=\"final.mp4\" preload=\"none\">",
        "the opening line is §F5.5's, attribute for attribute"
    );
    assert_eq!(
        body.lines()
            .filter(|line| line.starts_with("  <track "))
            .count(),
        2
    );
    assert_eq!(
        body.lines()
            .filter(|line| line.starts_with("  <track "))
            .filter(|l| l.ends_with('>'))
            .count(),
        2
    );
    assert_eq!(body.lines().last().unwrap(), "</video>");
    assert_eq!(
        body.matches("<video").count(),
        1,
        "one tag, nothing wrapped around it: {body}"
    );
    for banned in ["label=", "kind=", "<html", "<script"] {
        assert!(
            !body.contains(banned),
            "the page stays bare, no {banned}: {body}"
        );
    }
    assert!(
        page.path.is_file(),
        "the answer names a file that exists: {:?}",
        page.path
    );
    assert!(
        log.iter()
            .any(|line| *line == embed::wrote_tag_log(&page.path.display().to_string())),
        "the log points at the page it wrote: {log:?}"
    );
}

/// JPEG 90 is ffmpeg's `-q:v` ladder rung, and the encoder leg is the program's spawner: this test replaces
/// only the spawner (the seam `build_and_write` takes) and checks the argv the program built for it, rather
/// than assuming the conversion happens some other way.
#[test]
fn f5_5_s5_the_poster_argv_is_jpeg_quality_90() {
    assert_eq!(embed::POSTER_JPEG_QUALITY, 90, "§F5.5 spells the quality");
    let (_root, tree) = tree_in("argv");
    write_png(&tree.thumbnail_png());
    let legs = Legs::default();
    let (page, _) = run(&tree, Container::Mp4, Codec::H264, "en", &legs);
    let argv = legs.last();
    assert_eq!(argv.first().map(String::as_str), Some("ffmpeg"), "{argv:?}");
    let q = argv
        .iter()
        .position(|a| a == "-q:v")
        .unwrap_or_else(|| panic!("no -q:v in the poster command: {argv:?}"));
    assert_eq!(
        argv[q + 1],
        embed::POSTER_QSCALE,
        "the nearest rung to 90 on ffmpeg's ladder"
    );
    assert_eq!(
        argv.last().unwrap(),
        &tree.final_jpg().display().to_string(),
        "{argv:?}"
    );
    assert!(
        page.poster,
        "and the walk trusts the file, not the exit code: {page:?}"
    );
}
