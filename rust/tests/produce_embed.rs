//! F5.5 The `<video>` tag — `spec/08-produce.md` §F5.5 (wording in `spec/inventory/produce.md:35`), checked
//! against `naivepost::produce_embed`: the four lines a page needs, which poster sits beside the video, which
//! `.vtt` files become `<track>`s and in what order, and the two notes the log owes a person who just opened
//! the folder in a browser.
//!
//! Tools cited: `tool:ffmpeg.encode` — the poster is ffmpeg's JPEG, not an encoder this app carries (§F5.5's
//! "JPEG 90" becomes `-q:v 2`, ffmpeg's own ladder rung). §F5.5 cites no `P.*` row of §10, so none is asserted
//! here; the only number in the item is the JPEG quality, and it lives in `produce_embed`.

use std::path::{Path, PathBuf};

use naivepost::layout::Tree;
use naivepost::produce_embed as embed;
use naivepost::project::{Codec, Container};
use naivepost::roles::Language;

const ITEM: &str = "F5.5";

fn temp_root(tag: &str) -> PathBuf {
    // Removed here rather than at the end of each test: a case that fails on an assertion never reaches its
    // own cleanup, and a folder left in /tmp is a folder the next reader has to explain.
    let dir = std::env::temp_dir().join(format!("np-f55-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn tree_in(tag: &str) -> (PathBuf, Tree) {
    let root = temp_root(tag);
    let dir = root.join("demo.naivepost");
    std::fs::create_dir_all(&dir).unwrap();
    (root, Tree::new(&dir).unwrap())
}

fn track(code: &str, name: &str, file: &str) -> embed::Track {
    embed::Track { code: code.to_string(), name: name.to_string(), file: file.to_string() }
}

fn language(code: &str, name: &str) -> Language {
    Language { code: code.to_string(), tag: code.to_uppercase(), name: name.to_string() }
}

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|name| name.to_string()).collect()
}

/// The sidecar that carries the video's own language: §A's fixed stem with no code between the dots, spelled
/// from `produce_render::final_srt_name` rather than typed, so a test cannot drift from the name it checks.
fn own_vtt() -> String {
    naivepost::produce_render::final_srt_name().replace(".srt", ".vtt")
}

/// The same name as it appears in an assertion: `&str` rather than the owned `String` a listing holds.
fn own_file() -> &'static str {
    // §A fixes the video's name as `final`, and this is that name with the vtt extension; the assert below
    // keeps it honest against the module that owns the name.
    "final.vtt"
}

// ---- the file itself ---------------------------------------------------------------

#[test]
fn f5_5_s1_the_tag_is_the_one_you_paste_in() {
    assert_eq!(ITEM, "F5.5");
    // §F5.5's body is the answer, character for character: a bare `<video poster controls src preload="none">`
    // and one `<track>` per language, indented two spaces, the file ending on `</video>`.
    let tracks = [track("en", "English", "final.vtt"), track("de", "German", "final.de.vtt")];
    assert_eq!(
        embed::tag("final.mp4", Some("final.jpg"), &tracks),
        "<video poster=\"final.jpg\" controls src=\"final.mp4\" preload=\"none\">\n  \
         <track src=\"final.vtt\" srclang=\"en\" default>\n  \
         <track src=\"final.de.vtt\" srclang=\"de\">\n</video>\n"
    );

    // The four attributes the prose names, and nothing else on the video tag.
    let body = embed::tag("final.mp4", Some("final.jpg"), &tracks);
    for attribute in ["poster=", "controls", "src=", "preload=\"none\""] {
        assert!(body.contains(attribute), "{attribute} missing from:\n{body}");
    }
    assert!(!body.contains("autoplay") && !body.contains("loop"), "a page's own choices: {body}");

    // *Bare* is the spec's word, and its example is the answer: a track carries src, srclang and default. The
    // prototype also wrote label= and kind="subtitles" (recorded in spec/inventory/produce.md:35); a browser
    // labels a track from srclang anyway, so those are left out here on purpose.
    assert!(!body.contains("label="), "{body}");
    assert!(!body.contains("kind="), "{body}");
    assert!(!body.contains("/>"), "the spec's tracks are not self-closed: {body}");
}

#[test]
fn f5_5_s2_only_the_first_track_is_default() {
    assert_eq!(ITEM, "F5.5");
    let three = [
        track("en", "English", "final.vtt"),
        track("de", "German", "final.de.vtt"),
        track("sv", "Swedish", "final.sv.vtt"),
    ];
    let body = embed::tag("final.mp4", Some("final.jpg"), &three);
    // Two defaults is two sets of captions drawn over each other in some players, and nothing on the page says
    // which won — so exactly one, and it is the video's own language because that track leads.
    assert_eq!(body.matches(" default>").count(), 1, "{body}");
    assert!(body.contains("<track src=\"final.vtt\" srclang=\"en\" default>"), "{body}");
    assert!(body.contains("<track src=\"final.sv.vtt\" srclang=\"sv\">\n</video>"), "{body}");

    // One track: it is the default. Zero tracks: still a video, and no stray blank line where its track was.
    let one = embed::tag("final.mp4", None, &[track("en", "English", "final.vtt")]);
    assert_eq!(one.matches("<track").count(), 1);
    assert!(one.contains("default>"), "{one}");
    let none = embed::tag("final.webm", Some("final.jpg"), &[]);
    assert!(!none.contains("<track"), "{none}");
    assert_eq!(
        none,
        "<video poster=\"final.jpg\" controls src=\"final.webm\" preload=\"none\">\n</video>\n"
    );
}

#[test]
fn f5_5_s3_the_poster_is_optional_and_named_by_base_name() {
    assert_eq!(ITEM, "F5.5");
    // No poster is a legal state — the thumbnail is drawn by the other half of ▶ and may not have been drawn
    // yet — and it means no `poster=` attribute rather than a broken image request on the page.
    let body = embed::tag("final.mp4", None, &[]);
    assert_eq!(body, "<video controls src=\"final.mp4\" preload=\"none\">\n</video>\n");
    assert!(!body.contains("poster"), "{body}");

    // The tag lives beside the video, so every name in it is a base name: no folder, and nothing to fix when
    // the person copies produce/ somewhere else. (The file names carry dots; only a `/` would be a path.)
    let tracks = [track("en", "English", "final.vtt")];
    let body = embed::tag("final.mp4", Some("final.jpg"), &tracks);
    let named: Vec<&str> = body.lines().filter(|line| line.contains("<track") || line.contains("<video")).collect();
    assert_eq!(named.len(), 2, "one video line and one track: {body}");
    for line in named {
        assert!(!line.contains("produce/"), "a path would pin the folder: {line}");
        assert!(!line.contains('/'), "{line}");
    }

    // The three files it names are the ones §A fixes, beside each other.
    let (root, tree) = tree_in("s3");
    let (video, poster, html) = embed::paths(&tree, Container::Mp4);
    assert_eq!(video, tree.final_video("mp4"));
    assert_eq!(poster, tree.final_jpg());
    assert_eq!(html, tree.final_html());
    assert_eq!(poster.file_name().unwrap(), "final.jpg");
    assert_eq!(html.file_name().unwrap(), "final.html");
    assert_eq!(poster.parent(), video.parent(), "the poster sits beside the video");
    assert_eq!(html.parent(), video.parent(), "and so does the tag");

    // The container is the only thing that changes, because it is the only thing that differs.
    let (webm, _, _) = embed::paths(&tree, Container::Webm);
    assert_eq!(webm, tree.final_video("webm"));
    assert_eq!(webm.file_name().unwrap(), "final.webm");
    std::fs::remove_dir_all(&root).ok();
}

// ---- which tracks, from what is on disk ------------------------------------------

#[test]
fn f5_5_s4_tracks_come_from_the_files_on_disk() {
    assert_eq!(ITEM, "F5.5");
    // The session's own language arrives in `known` like any other: it is a code until something names it.
    let known = [language("en", "English"), language("de", "German"), language("sv", "Swedish")];
    let found = embed::tracks(
        &names(&[
            "final.de.vtt",
            "final.sv.vtt",
            own_file(),
            // An .srt is the desktop player's file: a <track> would fetch it, fail to parse it and show
            // nothing at all, so it never belongs in this tag.
            "final.srt",
            "final.de.srt",
            // Another video's sidecar, sitting in the same folder.
            "other.vtt",
            "notes.txt",
        ]),
        "en",
        &known,
    );
    let codes: Vec<(&str, &str, &str)> = found
        .iter()
        .map(|track| (track.code.as_str(), track.name.as_str(), track.file.as_str()))
        .collect();

    // The video's own language first and named by the session, the translations after it by name — not by
    // code, because the menu shows the name. `final.vtt` carries no code in its name; that is what says "this
    // video's own language".
    assert_eq!(
        codes,
        vec![
            ("en", "English", "final.vtt"),
            ("de", "German", "final.de.vtt"),
            ("sv", "Swedish", "final.sv.vtt"),
        ]
    );

    // A code nobody knows still gets a track, labelled with itself rather than with nothing — including the
    // session's own language when the settings list carries no entry for it.
    let bare = [language("de", "German"), language("sv", "Swedish")];
    let odd = embed::tracks(&names(&[own_file(), "final.pl.vtt"]), "en", &bare);
    assert_eq!(odd[1].code, "pl");
    assert_eq!(odd[1].name, "pl", "a code is better than an empty label");

    // Nothing on disk is not an error: a video with no captions still gets its tag.
    assert!(embed::tracks(&[], "en", &known).is_empty());
    assert!(embed::tracks(&names(&["final.srt"]), "en", &known).is_empty());
    // A sidecar named after a different video is not this one's track, even though it is a .vtt.
    assert!(embed::tracks(&names(&["other.vtt"]), "en", &known).is_empty());

    // The listing IS the disk check, and an unreadable folder reads as empty — which is what lets a ▶ that
    // skipped the encode still write its tag rather than failing on a missing folder.
    assert!(embed::list_vtt(Path::new("/no/such/produce")).is_empty());

    let (root, tree) = tree_in("s4");
    let html = tree.final_html();
    let dir = html.parent().unwrap();
    std::fs::create_dir_all(dir).unwrap();
    assert_eq!(own_vtt(), own_file(), "the stem §A fixes is what this reads");
    for name in [own_file(), "final.de.vtt", "final.sv.vtt", "final.srt"] {
        std::fs::write(dir.join(name), "WEBVTT\n\n").unwrap();
    }
    let listed = embed::list_vtt(dir);
    // The listing is alphabetical — it is `tracks` that decides which language a player is offered first.
    assert_eq!(listed, names(&["final.de.vtt", "final.sv.vtt", own_file()]));
    let on_disk = embed::tracks(&listed, "en", &known);
    assert_eq!(on_disk[0].file, own_file());
    assert_eq!(on_disk[0].code, "en");
    // A name in the folder that is not a file is not a track either.
    std::fs::create_dir(dir.join("final.fr.vtt")).unwrap();
    assert!(!embed::list_vtt(dir).contains(&"final.fr.vtt".to_string()));
    std::fs::remove_dir_all(&root).ok();
}

// ---- the poster -------------------------------------------------------------------

#[test]
fn f5_5_s5_the_poster_is_the_thumbnail_as_a_jpeg() {
    assert_eq!(ITEM, "F5.5");
    // §F5.5's "JPEG 90" is the requirement; ffmpeg's ladder rung is how it is met, because this app carries no
    // JPEG encoder of its own (tool:ffmpeg.encode).
    assert_eq!(embed::POSTER_JPEG_QUALITY, 90);
    assert_eq!(embed::POSTER_QSCALE, "2");

    let argv = embed::poster_command(
        Path::new("/p/produce/publish/thumbnail.png"),
        Path::new("/p/produce/final.jpg"),
    );
    assert_eq!(argv[0], "ffmpeg", "the encoder is ffmpeg, invoked as an argv list");
    // Built as an argv list, never as one string for a shell to interpret: no flag carries a path the user
    // typed into it, and no separator survives to be re-split.
    assert!(argv.iter().all(|part| !part.contains(' ')), "one argument per part: {argv:?}");
    assert!(!argv.join("").contains(";"), "{argv:?}");
    let at = |flag: &str| argv.iter().position(|a| a == flag).unwrap();
    assert_eq!(argv[at("-i") + 1], "/p/produce/publish/thumbnail.png");
    assert_eq!(argv[at("-q:v") + 1], embed::POSTER_QSCALE);
    assert_eq!(*argv.last().unwrap(), "/p/produce/final.jpg".to_string());

    // The poster is made from the thumbnail the project already has, and lands beside the video.
    let (root, tree) = tree_in("s5");
    let (video, poster, _) = embed::paths(&tree, Container::Mp4);
    let argv = embed::poster_command(&tree.thumbnail_png(), &poster);
    assert!(argv.contains(&tree.thumbnail_png().display().to_string()));
    assert!(argv.contains(&poster.display().to_string()));
    assert_eq!(poster.with_extension("mp4"), video);
    std::fs::remove_dir_all(&root).ok();
}

// ---- the two notes, and what a failure means --------------------------------------

#[test]
fn f5_5_s6_the_notes_the_log_has_to_make() {
    assert_eq!(ITEM, "F5.5");
    // Container or codec no browser plays. mp4 + h264 and webm + vp9 are a page's own formats and say nothing
    // — a note that fires on the good case stops being read.
    assert!(embed::web_unplayable(Container::Mp4, Codec::H264).is_none());
    assert!(embed::web_unplayable(Container::Webm, Codec::Vp9).is_none());

    let mkv = embed::web_unplayable(Container::Mkv, Codec::H264).expect("an mkv is offered to a page");
    assert!(mkv.contains("Matroska"), "{mkv}");
    assert!(mkv.contains("mp4") && mkv.contains("webm"), "it names what to render instead: {mkv}");

    let h265 = embed::web_unplayable(Container::Mp4, Codec::H265).expect("h265 is offered to a page");
    assert!(h265.contains("Firefox") && h265.contains("h264"), "{h265}");
    // The codec is the problem whatever it was put in.
    assert!(embed::web_unplayable(Container::Webm, Codec::H265).is_some());

    // Subtitles need http, not file:// — and only worth saying when there is a track to refuse.
    let none: [embed::Track; 0] = [];
    assert_eq!(embed::note_http(&none), None);
    let note = embed::note_http(&[track("en", "English", "final.vtt")]).expect("a page needs this said");
    assert!(note.contains("http") && note.contains("file://"), "{note}");
    assert_eq!(note, embed::needs_http_log());

    // The line that says where the four lines are.
    assert_eq!(embed::wrote_tag_log("produce/final.html"), ">>> the <video> tag for it: produce/final.html");
    assert!(embed::no_poster_log(embed::no_poster_reason()).contains("poster"), "the poster is what failed");
    assert!(embed::unwritable_log("produce/final.html").contains("produce/final.html"));

    // And the rule that keeps this a bonus: the render's verdict is the run's verdict. A tag that could not be
    // written leaves a finished video and its subtitles on disk, so it never turns a good run bad — and never
    // turns a failed one good either.
    // `true` = the run failed.
    assert!(!embed::render_verdict(false, false), "a good render is not a failure");
    assert!(embed::render_verdict(true, false), "a failed render is a failed run");
    assert!(!embed::render_verdict(false, true), "an unwritable tag is not a failed render");
    assert!(embed::render_verdict(true, true), "nor does it rescue one that failed anyway");
}

// ---- escaping, and writing the file -----------------------------------------------

#[test]
fn f5_5_s7_escaping_and_paths() {
    assert_eq!(ITEM, "F5.5");
    // A name carrying markup characters must not end its own attribute: one raw quote in a filename would
    // otherwise split the tag in two and silently break the page.
    let tracks = [track("e&n", "English", "a\"b.vtt")];
    let body = embed::tag("tom & jerry<mkv>.mp4", Some("q\"q.jpg"), &tracks);
    assert!(body.contains("src=\"tom &amp; jerry&lt;mkv&gt;.mp4\""), "{body}");
    assert!(body.contains("poster=\"q&quot;q.jpg\""), "{body}");
    assert!(body.contains("src=\"a&quot;b.vtt\""), "{body}");
    assert!(body.contains("srclang=\"e&amp;n\""), "{body}");
    // Escaped means the attribute still closes exactly once per value.
    let first = body.lines().next().unwrap();
    assert_eq!(first.matches('"').count() % 2, 0, "unbalanced quotes: {first}");
    assert!(first.ends_with("preload=\"none\">"), "{first}");

    // write_tag creates the produce/ folder (a first render has never had one) and writes the body verbatim.
    let (root, tree) = tree_in("s7");
    assert!(!tree.final_html().exists());
    let body = embed::tag("final.mp4", None, &[track("en", "English", "final.vtt")]);
    embed::write_tag(&tree, &body).unwrap();
    assert_eq!(std::fs::read_to_string(tree.final_html()).unwrap(), body);

    // Overwritten whole rather than appended to: the second ▶ describes a different video.
    let again = embed::tag("final.webm", None, &[]);
    embed::write_tag(&tree, &again).unwrap();
    assert_eq!(std::fs::read_to_string(tree.final_html()).unwrap(), again);

    // A path that cannot be written is an Err naming the path — a directory where the file belongs is enough,
    // and it must not panic.
    std::fs::remove_file(tree.final_html()).unwrap();
    std::fs::create_dir(tree.final_html()).unwrap();
    let err = embed::write_tag(&tree, "x").expect_err("a folder cannot be overwritten by a file");
    assert!(err.contains("final.html"), "{err}");
    std::fs::remove_dir_all(&root).ok();
}
