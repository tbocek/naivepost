//! F5.6 S3/S6 — the PIXELS behind the image row, as the program handles them.
//!
//! `tests/produce_upload_flow.rs` proves the walk's ORDER with legs that answer for themselves; this file
//! proves the media legs themselves, with REAL pictures: every source image here is a JPEG ffmpeg's lavfi
//! filter drew (a 1440x1080 frame whose shape differs from the video's, so a crop is an observable change),
//! and every number asserted is read back off a file ffmpeg or ffprobe wrote. Nothing is a scripted byte
//! blob, because a blob proves the code copied something, not that it produced the right picture.
//!
//! The two servers arrive as servers: a loopback sd.cpp the SETTINGS name (the seam §02-services#1 gives the
//! program), so the request body asserted here is the one `produce_upload_draw::submit` actually sent.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;

use naivepost::cut::{Cut, Seg};
use naivepost::layout::Tree;
use naivepost::produce_upload as up;
use naivepost::produce_upload_media as media;
use naivepost::project::Publish;
use naivepost::run::Snapshot as SourceSnapshot;
use naivepost::settings::{self, Conf};

/// Held while a scenario points the process-wide settings env at it (the same lock discipline the other
/// wire tests keep: XDG_* and cwd belong to the whole process).
fn scenario_lock() -> &'static std::sync::Mutex<()> {
    static SCENARIO: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    SCENARIO.get_or_init(|| std::sync::Mutex::new(()))
}

struct Fixture {
    root: PathBuf,
    tree: Tree,
    _scenario: std::sync::MutexGuard<'static, ()>,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// A scenario folder: settings (pointed at `sd_url`), data, and a `.naivepost` project with a two-segment
/// 16:9 cut on disk. cwd is pinned inside it, because that is what the page resolves the session from.
fn fixture(tag: &str, sd_url: &str) -> Fixture {
    let scenario = scenario_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let root = std::env::temp_dir().join(format!("naivepost-f56media-{tag}"));
    let _ = std::fs::remove_dir_all(&root);
    let config = root.join("config");
    let data = root.join("data");
    let session = root.join("session.naivepost");
    std::fs::create_dir_all(&session).expect("session folder");
    std::fs::create_dir_all(&config).expect("config folder");
    std::fs::create_dir_all(&data).expect("data folder");
    std::env::set_current_dir(&root).expect("cwd pinned to the scenario root");
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", &config);
        std::env::set_var("XDG_DATA_HOME", &data);
        // A stray exported ffmpeg/ffprobe path would outrank nothing here, but clearing keeps the scenario
        // dialling the real tools rather than something left in the developer's shell.
        std::env::remove_var("NAIVEPOST_FFMPEG");
    }
    let paths = settings::from_environment().expect("settings folder");
    let mut conf = Conf::default();
    conf.model = "f56-media-model".to_string();
    // No LLM in this file's checks: the model leg never dials (the walk is driven directly), and an address
    // that cannot answer would hang a test rather than fail it.
    conf.server = "http://127.0.0.1:1".to_string();
    conf.sd_server = sd_url.to_string();
    settings::save(&paths, &conf).expect("settings written");
    let tree = Tree::new(&session).expect("a .naivepost folder is a project");
    write_cut(&tree);
    Fixture { root, tree, _scenario: scenario }
}

/// Two footage segments, so `starts_of` answers `[0, 20]` and the second clip holds the +1 s moment.
fn write_cut(tree: &Tree) {
    let first = Seg { s: 0.0, e: 9.5, ..Default::default() };
    let second = Seg { s: 10.0, e: 30.0, ..Default::default() };
    let cut = Cut {
        segs: vec![first, second],
        aspect: "16:9".to_string(),
        ..Default::default()
    };
    let file = tree.cut_json();
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, serde_json::to_string_pretty(&cut).unwrap()).unwrap();
    // Prepare writes one marker per recording it frames; the page reads THAT, not the cut, when it decides
    // which folders hold frames, so a scenario that wants its frames found has to leave one behind.
    let marker = tree.frames_dir("room").join(".frames");
    std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
    std::fs::write(&marker, "").unwrap();
}

/// A run carrying that cut and one footage source, which is all the media legs read.
fn run(tree: &Tree) -> naivepost::produce_flow::Run {
    let cut = naivepost::cut::load(tree).unwrap_or_default();
    naivepost::produce_flow::Run {
        cut,
        lines: Vec::new(),
        settings: Default::default(),
        sources: SourceSnapshot {
            footage: vec!["/media/room.mp4".to_string()],
            voice: Vec::new(),
            narrators: Vec::new(),
        },
        publish: None,
        aspect: "16:9".to_string(),
        clips: 2,
        publish_written: false,
        voice: "1".to_string(),
        no_narration: false,
    }
}

// ---- real pictures -------------------------------------------------------------------------------

/// Draw one JPEG with ffmpeg's own lavfi sources: `shape` is its pixel size, and `testsrc2` (colour bars over
/// a moving counter) is busy enough that a JPEG ladder actually has something to throw away. A failure here is
/// the TEST's environment, so it is stated as such rather than swallowed.
fn make_jpeg(path: &Path, wide: u32, high: u32) {
    std::fs::create_dir_all(path.parent().unwrap()).expect("the frame folder exists");
    let source = format!("testsrc2=size={wide}x{high}:rate=25:duration=1");
    let out = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-y"])
        .args(["-f", "lavfi", "-i", &source])
        .args(["-frames:v", "1"])
        .arg(path)
        .output()
        .expect("ffmpeg runs in this container");
    assert!(
        out.status.success(),
        "lavfi could not draw a {wide}x{high} frame: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(path.is_file(), "ffmpeg wrote no frame at {}", path.display());
}

/// One extracted frame, filed the way Prepare files them: `tree.frames_dir(stem)/<clock name>.jpg`.
fn frame_on_disk(tree: &Tree, stem: &str, seconds: f64, wide: u32, high: u32) -> PathBuf {
    let path = tree
        .frames_dir(stem)
        .join(format!("{}.jpg", naivepost::clock::frame_name(seconds)));
    make_jpeg(&path, wide, high);
    path
}

/// Write one JSON picture to a scratch file so ffprobe (the only reader this repo has) can size it. The name
/// carries a per-call token for the same reason `flat_png`'s does: these checks share one process.
fn size_of_bytes(bytes: &[u8]) -> Option<(i32, i32)> {
    let scratch = scratch_path(&format!("probe-{}.png", counter::next()));
    std::fs::write(&scratch, bytes).expect("scratch is writable");
    let size = naivepost::produce_probe::size_of(&scratch);
    let _ = std::fs::remove_file(&scratch);
    size
}

/// The `(moment, file)` pairs a media leg is handed, exactly as `produce_languages::frame_images` builds
/// them: each frame's seconds inside a kept segment, moved onto the produced clock.
fn frame_files(tree: &Tree, listed: &[(f64, PathBuf)]) -> Vec<(f64, PathBuf)> {
    let run = run(tree);
    let mut out: Vec<(f64, PathBuf)> = Vec::new();
    let mut at = 0.0;
    for seg in &run.cut.segs {
        for (seconds, path) in listed {
            if *seconds >= seg.s && *seconds < seg.e {
                out.push((at + (seconds - seg.s), path.clone()));
            }
        }
        at += seg.e - seg.s;
    }
    out
}

/// Decode one `data:image/…;base64,` URL back to (mime, bytes) through the program's own base64 reader.
fn decode_data_url(url: &str) -> (String, Vec<u8>) {
    let (_head, payload) = url
        .split_once(";base64,")
        .unwrap_or_else(|| panic!("not a data url: {url}"));
    let mime = url
        .strip_prefix("data:")
        .and_then(|rest| rest.split_once(';').map(|(mime, _)| mime))
        .unwrap_or_default()
        .to_string();
    let parsed = serde_json::json!({ "images": [ { "b64_json": payload } ] });
    (mime, naivepost::produce_upload_draw::picture_of(&parsed))
}

// ---- the fake sd.cpp ------------------------------------------------------------------------------

/// A loopback sd.cpp answering the two job kinds and handing each request's body back, so the assertion is
/// about what the program SENT rather than about what a leg was told to say.
struct FakeSd {
    url: String,
    received: std::sync::mpsc::Receiver<String>,
}

impl FakeSd {
    fn start(replies: Vec<String>) -> FakeSd {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind a loopback port");
        let port = listener.local_addr().expect("a local address").port();
        let (sender, received) = std::sync::mpsc::channel();
        let script = std::sync::Arc::new(std::sync::Mutex::new(replies));
        let serving = script.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                let serving = serving.clone();
                let sender = sender.clone();
                std::thread::spawn(move || {
                    if let Some(body) = serve(stream, &serving) {
                        let _ = sender.send(body);
                    }
                });
            }
        });
        FakeSd {
            url: format!("http://127.0.0.1:{port}"),
            received,
        }
    }

    fn next_body(&self) -> serde_json::Value {
        let raw = self
            .received
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("the drawing call reached the fake sd.cpp");
        serde_json::from_str(&raw).expect("the request body is JSON")
    }

    /// A body if one arrived, without waiting for a call that should never happen: a refused draw must send
    /// nothing, and `next_body` would turn that correct silence into a timeout failure.
    fn try_next_body(&self) -> Option<serde_json::Value> {
        self.received
            .recv_timeout(std::time::Duration::from_millis(150))
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
    }
}

/// Answer one request with the next scripted reply, then close: the client reads to EOF, so a kept-open
/// connection makes every later round pay a timeout.
fn serve(mut stream: TcpStream, script: &std::sync::Arc<std::sync::Mutex<Vec<String>>>) -> Option<String> {
    let seen = serve_inner(&mut stream, script);
    let _ = stream.shutdown(std::net::Shutdown::Both);
    seen
}

fn serve_inner(stream: &mut TcpStream, script: &std::sync::Arc<std::sync::Mutex<Vec<String>>>) -> Option<String> {
    let mut reader = BufReader::new(stream.try_clone().ok()?);
    let mut request_line = String::new();
    reader.read_line(&mut request_line).ok()?;
    let mut length = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).ok()? == 0 {
            break;
        }
        if let Some(rest) = line.to_ascii_lowercase().strip_prefix("content-length:") {
            length = rest.trim().parse().unwrap_or(0);
        }
        if line.trim().is_empty() {
            break;
        }
    }
    let mut payload = vec![0u8; length];
    if length > 0 && reader.read_exact(&mut payload).is_err() {
        return None;
    }
    let _ = request_line;
    let reply = {
        let mut script = script.lock().expect("script lock");
        if script.len() > 1 {
            script.remove(0)
        } else {
            script.first().cloned().unwrap_or_default()
        }
    };
    let framed = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        reply.len(),
        reply
    );
    let _ = stream.write_all(framed.as_bytes());
    let _ = stream.flush();
    Some(String::from_utf8_lossy(&payload).to_string())
}

/// The job answer that reports done and carries `png` as the drawn picture, base64 as sd.cpp returns it.
fn done_reply(png: &[u8]) -> String {
    serde_json::json!({
        "status": "done",
        "result": { "images": [ { "b64_json": naivepost::checks::base64(png) } ] },
    })
    .to_string()
}

/// A flat PNG of the draw's own size, made in this process's own scratch folder and read back as bytes.
/// EVERY path a check hands the program is absolute: a scenario pins the process's cwd inside its own folder,
/// and a relative scratch name would be resolved from there and then be gone when the folder is tidied.
fn drawn_png(wide: u32, high: u32) -> Vec<u8> {
    flat_png(wide, high)
}

/// The same picture as bytes, through cairo (the encoder the program itself uses for a PNG).
///
/// Written to a name unique to the CALL, not to the size: `cargo test` runs the four checks in this file as
/// threads of ONE process, so two of them asking for the same 1280x720 picture at the same moment would have
/// one delete the other's file between its write and its read.
fn flat_png(wide: u32, high: u32) -> Vec<u8> {
    let scratch = scratch_path(&format!("drawn-{}-{wide}x{high}.png", counter::next()));
    naivepost::produce_upload_print::flat_png_for_test(&scratch, wide, high, 0.3).expect("a flat png");
    let bytes = std::fs::read(&scratch).expect("the flat png is readable");
    let _ = std::fs::remove_file(&scratch);
    bytes
}

/// A per-call counter, so every scratch name in this process is distinct even when two threads ask for the
/// same size at the same moment.
mod counter {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    pub fn next() -> u64 {
        NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    }
}

/// An absolute scratch path inside this process's own folder under `/tmp` — outside every scenario, so a
/// scenario's tidy-up cannot delete a file another check is still reading.
fn scratch_path(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("naivepost-f56-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("/tmp is writable");
    dir.join(name)
}

fn silent(_line: &str) {}

// ---- S6: the base cropped, the references raw ------------------------------------------------------

/// S6: the request carries PIXELS, not file names — the base cropped to the video's shape, every reference
/// sent raw. Asserted off the body the fake sd.cpp actually received.
#[test]
fn f5_6_s6_the_base_goes_cropped_and_the_references_raw() {
    let sd = FakeSd::start(vec![
        serde_json::json!({ "id": "job-1" }).to_string(),
        done_reply(&drawn_png(1280, 720)),
    ]);
    let scene = fixture("base-cropped", &sd.url);
    // A 21:9 source frame against a 16:9 video: the crop has something to take away (it keeps 75% of the
    // width), so "cropped" is a measurable difference rather than a claim.
    let base = frame_on_disk(&scene.tree, "room", 1.0, 2560, 1080);
    let reference = frame_on_disk(&scene.tree, "room", 25.0, 2560, 1080);
    let listed = vec![(1.0, base.clone()), (25.0, reference.clone())];
    let files = frame_files(&scene.tree, &listed);
    assert_eq!(files.len(), 2, "both frames survive into the video");

    let publish = Publish {
        frames: vec!["frame:1.000".to_string(), "frame:25.000".to_string()],
        prompt: "brighten it".to_string(),
        thumb_title: "a title".to_string(),
        ..Default::default()
    };
    let sent = media::request_images(
        &scene.tree,
        &publish,
        &files,
        1280,
        720,
        silent,
    );
    assert_eq!(sent.urls.len(), 2, "base plus one reference: {:?}", sent.urls);
    assert!(sent.skipped.is_empty(), "nothing was skipped: {:?}", sent.skipped);

    let (base_mime, base_bytes) = decode_data_url(sent.urls.first().expect("a base"));
    assert_eq!(base_mime, "image/png", "the crop is written as a PNG: {base_mime}");
    assert!(
        base_bytes != std::fs::read(&base).expect("the source frame"),
        "the base went out as its uncropped bytes"
    );
    // The crop to 16:9 lands at the draw's size, which is the whole point of cropping before sending.
    assert_eq!(
        size_of_bytes(&base_bytes),
        Some((1280, 720)),
        "the base arrived at the video's shape"
    );
    let (ref_mime, ref_bytes) = decode_data_url(&sent.urls[1]);
    assert_eq!(ref_mime, "image/jpeg", "a reference is the frame's own type");
    // Compared as base64 text, not as a byte array: a failure that prints two megabytes of JPEG buries the
    // assertion under it, and the fact being pinned is "the same bytes", which the encoding shows directly.
    let encoded = crate_helper_b64(&ref_bytes);
    let on_disk = crate_helper_b64(&std::fs::read(&reference).expect("the reference frame"));
    assert_eq!(encoded, on_disk, "S6: references go RAW — the same bytes, not a re-encode");
    // And the crop the log prints is a fact about that source: a 4:3 frame cropped to 16:9 keeps 75% of
    // its width, not the 100% a shape-only rule would have claimed.
    assert_eq!(sent.crop_percent, 75, "the crop line tells the truth: {}", sent.crop_percent);
}

/// S6 with nothing behind the row: the walk must SAY it has no base, in the sentence §S6 spells, and send no
/// `ref_images` at all rather than a phantom one.
#[test]
fn f5_6_s6_no_base_says_drawn_from_the_instruction_alone() {
    let sd = FakeSd::start(vec![
        serde_json::json!({ "id": "job-1" }).to_string(),
        done_reply(&drawn_png(1280, 720)),
    ]);
    let scene = fixture("no-base", &sd.url);
    let logs = collected();
    // The row names frames; none of them exists on disk.
    let publish = Publish {
        frames: vec!["frame:5.000".to_string()],
        prompt: "brighten it".to_string(),
        thumb_title: "a title".to_string(),
        description: "a description".to_string(),
        ..Default::default()
    };
    let sent = media::request_images(&scene.tree, &publish, &[], 1280, 720, silent);
    assert!(sent.base.is_none(), "nothing resolved to a picture");
    assert!(sent.urls.is_empty(), "no phantom reference: {:?}", sent.urls);

    // Through the walk, so the sentence is the program's and not the test's.
    let answers = up::Answers {
        title: Some("a title".into()),
        instruction: Some("brighten it".into()),
        negative: None,
        description: Some("a description".into()),
        picked_frame: None,
    };
    let outcome = walk_through_page_legs(&scene, &publish, &answers, &logs);
    assert!(outcome.published, "the text still lands ahead of the draw");
    assert_eq!(outcome.error, None, "the draw was refused, not failed: {:?}", outcome.error);
    let logs = logs.lock().expect("log lock").clone();
    assert!(
        logs.iter().any(|l| l.contains(naivepost::produce_details::REQUEST_NO_IMAGES_LOG)),
        "S6's own line is missing: {:?}",
        logs.iter().filter(|l| l.contains("publish")).collect::<Vec<_>>()
    );
    let body = sd.next_body();
    assert!(
        body.get("ref_images").is_none(),
        "an empty row sends no references at all: {body}"
    );
}

// ---- S3: the picked frame becomes the picture ------------------------------------------------------

/// S3: a picked frame IS the thumbnail — no model, no GPU — so it must land as the plain picture at the
/// video's shape AND be printed onto, which is what makes the stamp writable at all.
#[test]
fn f5_6_s3_a_picked_frame_becomes_the_picture() {
    let sd = FakeSd::start(vec![serde_json::json!({ "id": "job-1" }).to_string()]);
    let scene = fixture("picked", &sd.url);
    // One frame inside the SECOND clip: `clip 2 +1` is 1 s into it, which is where this frame sits.
    let taken = frame_on_disk(&scene.tree, "room", 11.0, 1440, 1080);
    let listed = vec![(11.0, taken.clone())];
    let files = frame_files(&scene.tree, &listed);
    let logs = collected();
    let publish = Publish {
        frames: vec!["frame:1.000".to_string()],
        prompt: "an instruction of my own".to_string(),
        thumb_title: "an old title".to_string(),
        ..Default::default()
    };
    let answers = up::Answers {
        title: Some("a picked-frame title".into()),
        instruction: None,
        negative: None,
        description: Some("a description".into()),
        picked_frame: Some(21.0),
    };
    let outcome = walk_with_frames(&scene, &publish, &answers, &files, true, &logs);
    assert!(outcome.own, "S3: the answer says the picture is a chosen frame");
    assert_eq!(outcome.error, None, "the walk finished: {:?}", outcome.error);
    // An OWN picture is not reprinted when the words changed (decision 34): this scenario's row holds a frame
    // AND an instruction of its own, so `edited_by_hand` is true and the title change costs no lettering.
    // That is the rule under test, so the plain picture is what this check measures — and it only exists at
    // all because the S3 leg wrote it (§F5.6 S3: the frame IS the thumbnail, no model, no GPU).
    assert!(!outcome.reprinted, "an own picture is not re-lettered by a title change");
    let plain = scene.tree.thumbnail_plain_png();
    assert!(plain.is_file(), "the chosen frame landed as the plain picture");
    assert_eq!(
        naivepost::produce_probe::size_of(&plain),
        Some((1280, 720)),
        "the plain picture is at the video's shape, not the frame's"
    );
    assert!(
        scene.tree.thumbnail_stamp().is_file(),
        "a picture that exists can carry a stamp"
    );
    // The row holds that frame only (§S3: "own = true, the row holds that frame only").
    let saved = naivepost::publish::load(&scene.tree).expect("publish.json reads back");
    assert!(saved.own, "the record says the picture is one's own");
    assert!(
        saved.frames.len() == 1,
        "the row holds ONE frame: {:?}",
        saved.frames
    );
    // The entry names the moment on the PRODUCED clock, which is where `files` put it — not the number the
    // model answered in `clip n +s` form, and not the source file's own seconds.
    assert_eq!(
        saved.frames,
        vec![format!("frame:{:.1}", files[0].0)],
        "and it is the frame that was picked"
    );
    let _ = sd;
}

// ---- S7: the words half leaves a real picture behind ----------------------------------------------

/// S6→S7 through the page's own legs with a fake sd.cpp: the drawn bytes land, the words print over them,
/// and the stamp is written — five files, each one the reason the next step could run.
#[test]
fn f5_6_s7_the_words_half_lands_a_real_picture() {
    let drawn = drawn_png(1280, 720);
    let sd = FakeSd::start(vec![
        serde_json::json!({ "id": "job-9" }).to_string(),
        done_reply(&drawn),
    ]);
    let scene = fixture("words-half", &sd.url);
    let frame = frame_on_disk(&scene.tree, "room", 2.0, 1440, 1080);
    let listed = vec![(2.0, frame)];
    let files = frame_files(&scene.tree, &listed);
    let logs = collected();
    let publish = Publish {
        frames: vec!["frame:2.000".to_string()],
        thumb_title: String::new(),
        prompt: String::new(),
        description: String::new(),
        ..Default::default()
    };
    let answers = up::Answers {
        title: Some("The comeback nobody saw coming".into()),
        instruction: Some("brighten the final scoreboard".into()),
        negative: Some("no blur".into()),
        description: Some("It was close until the last round.".into()),
        picked_frame: None,
    };
    let outcome = walk_with_frames(&scene, &publish, &answers, &files, false, &logs);
    assert_eq!(outcome.error, None, "the words half finished: {:?}", outcome.error);
    assert!(outcome.reprinted, "S7 printed the title, so the walk says it did");
    assert!(outcome.published, "publish.json + description.txt landed");
    for (what, path) in [
        ("publish.json", scene.tree.publish_json()),
        ("description.txt", scene.tree.description_txt()),
        ("thumbnail-plain.png", scene.tree.thumbnail_plain_png()),
        ("thumbnail.png", scene.tree.thumbnail_png()),
        ("thumbnail.stamp", scene.tree.thumbnail_stamp()),
    ] {
        assert!(path.is_file(), "{what} is missing at {}", path.display());
    }
    let plain = std::fs::read(scene.tree.thumbnail_plain_png()).expect("the plain picture");
    let printed = std::fs::read(scene.tree.thumbnail_png()).expect("the printed picture");
    assert!(
        plain != printed,
        "the title was not printed: both pictures are {} bytes",
        printed.len()
    );
    // The picture that landed is the one the server drew, not a placeholder the leg invented.
    assert_eq!(
        size_of_bytes(&plain),
        Some((1280, 720)),
        "the drawn bytes are the job's own picture"
    );
    let stamp = std::fs::read_to_string(scene.tree.thumbnail_stamp()).expect("the stamp reads back");
    assert_eq!(stamp.trim().len(), 16, "the stamp is a hash: {stamp}");
    // The request the fake took carried the cropped base — the pixels, not the row's name.
    let body = sd.next_body();
    let refs = body["ref_images"].as_array().expect("references went out");
    assert!(!refs.is_empty(), "an image row of one frame is a base: {body}");
    let (mime, bytes) = decode_data_url(refs[0].as_str().expect("a url"));
    assert_eq!(mime, "image/png", "the base is the crop, not the JPEG: {mime}");
    assert_eq!(
        size_of_bytes(&bytes),
        Some((1280, 720)),
        "the base went out at the video's shape"
    );
    let _ = listed;
}

// ---- the legs, driven the way the page drives them -------------------------------------------------

/// Drive `walk` through the PAGE's media legs with a row that resolves to real files. The page's own
/// `upload_legs` closes over a frame list read from disk; these checks hand it a list they laid down, so the
/// legs are built through `produce_languages` only where the run's cut can see the frames.
/// Base64 for a comparison message: short enough to read when it differs, unlike a raw JPEG dump.
fn crate_helper_b64(bytes: &[u8]) -> String {
    naivepost::checks::base64(bytes)
}

/// The walk's log for one check: `window::log_line` is a thread-local only the window's own flows may write
/// to, so a program-path test keeps its own copy and says so when an assertion needs a line from it.
fn collected() -> std::sync::Arc<std::sync::Mutex<Vec<String>>> {
    std::sync::Arc::new(std::sync::Mutex::new(Vec::new()))
}

fn walk_with_frames(
    scene: &Fixture,
    publish: &Publish,
    answers: &up::Answers,
    files: &[(f64, PathBuf)],
    own_frame: bool,
    logs: &std::sync::Arc<std::sync::Mutex<Vec<String>>>,
) -> up::Outcome {
    let tree = scene.tree.clone();
    let frames = files.to_vec();
    let (wide, high) = up::frame_size_from_aspect("16:9");
    let draw = draw_leg();
    let poll = poll_leg();
    let print = print_leg();
    let save_picture = save_picture_leg();
    let save_publish = save_publish_leg();
    let write_stamp = stamp_writer();
    let mats = up::Materials {
        save_publish: &save_publish,
        // No model call in these checks: the answers are what a model would have answered, handed in.
        ask_model: &|_brief: &str| Ok(String::new()),
        ask_draw: &draw,
        poll: &poll,
        save_picture: &save_picture,
        print: &print,
        nearest_frame: &|at| frames.iter().map(|(moment, _)| *moment).min_by(|a, b| {
            (a - at).abs().partial_cmp(&(b - at).abs()).unwrap_or(std::cmp::Ordering::Equal)
        }),
        spread_frames: &|| vec!["frame:1.000".to_string()],
        stored_stamp: None,
        write_stamp: &write_stamp,
        edited_by_hand: own_frame,
        request_images: &|record: &Publish| {
            media::request_images(&tree, record, &frames, wide, high, silent)
        },
        own_picture: &|record: &Publish| {
            media::write_own_picture(&tree, record, &frames, wide, high, silent)
        },
    };
    up::walk("THE FINISHED VIDEO: 2 clips, 00:30 long.", publish, answers, &mats, |line| {
        logs.lock().expect("log lock").push(line.to_string());
    })
}

fn walk_through_page_legs(
    scene: &Fixture,
    publish: &Publish,
    answers: &up::Answers,
    logs: &std::sync::Arc<std::sync::Mutex<Vec<String>>>,
) -> up::Outcome {
    let tree = scene.tree.clone();
    let legs = naivepost::ui::produce_languages::upload_legs(&tree, &publish_run(&scene.tree, publish));
    let mats = up::Materials {
        save_publish: &legs.save_publish,
        ask_model: &legs.ask_model,
        ask_draw: &legs.ask_draw,
        poll: &legs.poll,
        save_picture: &legs.save_picture,
        print: &legs.print,
        nearest_frame: &legs.nearest_frame,
        spread_frames: &legs.spread_frames,
        stored_stamp: legs.stored_stamp.clone(),
        write_stamp: &legs.write_stamp,
        edited_by_hand: legs.edited_by_hand,
        request_images: &legs.request_images,
        own_picture: &legs.own_picture,
    };
    up::walk("THE FINISHED VIDEO: 2 clips, 00:30 long.", publish, answers, &mats, |line| {
        logs.lock().expect("log lock").push(line.to_string());
    })
}

/// A run whose cut is the scenario's and whose publish state is the record under test — which is exactly
/// what the page's snapshot hands the words half at a press.
fn publish_run(tree: &Tree, publish: &Publish) -> naivepost::produce_flow::Run {
    naivepost::produce_flow::Run { publish: Some(publish.clone()), ..run(tree) }
}

/// The draw and poll legs the fixture's fake sd.cpp answers: unscripted, so these are the program's own.
fn draw_leg() -> Box<up::AskDraw> {
    let tree = naivepost::ui::produce_languages::session_tree();
    Box::new(move |body| naivepost::produce_upload_draw::submit(&tree, body))
}

fn poll_leg() -> Box<up::Poll> {
    let tree = naivepost::ui::produce_languages::session_tree();
    Box::new(move |job| naivepost::produce_upload_draw::wait_until_done(&tree, job, &|| false, silent))
}

fn save_picture_leg() -> Box<up::SavePicture> {
    Box::new(|_job, png| {
        let path = naivepost::ui::produce_languages::session_tree().thumbnail_plain_png();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|why| format!("{why}"))?;
        }
        std::fs::write(&path, png).map_err(|why| format!("{why}"))
    })
}

fn print_leg() -> Box<up::PrintWords> {
    Box::new(|_kind, words| {
        let tree = naivepost::ui::produce_languages::session_tree();
        let plain = tree.thumbnail_plain_png();
        if !plain.is_file() {
            return Err("nothing has been drawn to print onto".to_string());
        }
        let band = naivepost::publish::load(&tree)
            .ok()
            .and_then(|record| record.title_box)
            .unwrap_or_default();
        naivepost::produce_upload_print::print_words(&plain, &tree.thumbnail_png(), words, &band)
    })
}

// ---- this round: what the PAGE left, not what the file recorded ---------------------------------

/// The page's rows are what §S3 snapshots at the press, and three of them have no `publish.json` spelling of
/// their own until a run writes one: the title box dragged on the picture, the marked texts typed onto it,
/// and the edit instruction. A words half that read the FILE instead would draw from an empty instruction,
/// letter the title across the wrong band, and print none of the boxes — so these checks hand the walk a
/// publish state that exists only in the page and assert the pixels show it.
#[test]
fn f5_6_s7_the_page_s_band_and_marked_texts_are_what_get_printed() {
    let sd = FakeSd::start(vec![
        serde_json::json!({ "id": "job-band" }).to_string(),
        done_reply(&drawn_png(1280, 720)),
    ]);
    let scene = fixture("page-band", &sd.url);
    // One frame so the row has a base the crop can open; the shape differs from the video's on purpose.
    let frame = frame_on_disk(&scene.tree, "room", 2.0, 1440, 1080);
    let files = frame_files(&scene.tree, &[(2.0, frame)]);
    assert_eq!(files.len(), 1, "the scenario's one frame resolves to one entry");
    let logs = collected();
    // The page's state: a title box over the LOWER part of the picture (the app default is 0.25, the upper
    // third), one marked text, and an instruction. None of this is on disk — nothing has been saved yet.
    // `thumb_title` differs from `title` so decision 34 says the words changed: with an instruction already
    // written by hand the picture is the user's own, and only a CHANGED title earns a reprint at all.
    let publish = Publish {
        frames: vec!["frame:2.000".to_string()],
        prompt: String::new(),
        title: "a title from the page".to_string(),
        thumb_title: "an older printed line".to_string(),
        description: "a description".to_string(),
        title_box: Some(naivepost::project::TitleBox { cx: 0.5, cy: 0.8, wf: 0.9, hf: 0.3 }),
        texts: vec![naivepost::project::TextMark {
            cx: 0.5,
            cy: 0.5,
            wf: 0.6,
            hf: 0.2,
            text: "a box typed on the picture".to_string(),
        }],
        ..Default::default()
    };
    assert!(
        !scene.tree.publish_json().exists(),
        "the scenario starts with no record at all: {}",
        scene.tree.publish_json().display()
    );
    let answers = up::Answers {
        title: Some("a title from the page".into()),
        instruction: Some("brighten the final scoreboard".into()),
        negative: None,
        description: Some("a description".into()),
        picked_frame: None,
    };
    // Decision 34's own rule, asserted on the way in: `walk_through_page_legs` takes "has the user edited the
    // picture by hand" from the legs it builds, and this scenario wants the reprint to be the page's business,
    // so the walk is driven with the same materials over a record whose printed line differs from its title.
    assert!(
        naivepost::audit_gaps::reprint_title_on_change(&publish.thumb_title, "a title from the page", false),
        "the scenario does not actually change the printed line"
    );
    let outcome = walk_through_page_legs(&scene, &publish, &answers, &logs);
    assert_eq!(outcome.error, None, "the words half finished: {:?}", outcome.error);
    assert!(outcome.published, "the text landed before the draw (S5)");
    assert!(outcome.reprinted, "a new title prints (decision 34)");
    // S7 printed SOMETHING: the two pictures differ, and both exist.
    let plain = std::fs::read(scene.tree.thumbnail_plain_png()).expect("the drawn plain picture");
    let printed = std::fs::read(scene.tree.thumbnail_png()).expect("the printed picture");
    assert_ne!(plain, printed, "the words were not printed: both are {} bytes", printed.len());
    // The band the letters went into is measurable: a box at cy 0.8 puts ink in the BOTTOM quarter and none
    // in the top one, which is the opposite of the app default's 0.25. Read back off the PNG cairo wrote.
    let (top, bottom) = ink_halves(&printed);
    assert!(bottom > 0, "nothing was printed in the lower band the page asked for: {outcome:?}");
    assert_eq!(top, 0, "ink landed above the page's box: top={top} bottom={bottom}");
    // And the instruction the page holds reached the server — a fresh session reading the file would have had
    // nothing to send at all.
    let body = sd.next_body();
    let sent_prompt = body["prompt"].as_str().unwrap_or_default().to_string();
    assert!(
        sent_prompt.contains("brighten the final scoreboard"),
        "the page's instruction is not in the request: {sent_prompt}"
    );
    assert!(
        body["ref_images"].as_array().is_some_and(|a| a.len() == 1),
        "the cropped base went out as the one image: {body}"
    );
    let _ = sd;
}

/// Where the ink landed: counts of pixels that differ from the flat plate in the TOP and BOTTOM quarters of
/// the picture. The letters are white over a mid-grey plate (the drawn PNG is flat), so "changed" is exactly
/// measurable, and which quarter holds them says which band the print leg used.
fn ink_halves(png: &[u8]) -> (usize, usize) {
    let mut surface = cairo::ImageSurface::create_from_png(&mut std::io::Cursor::new(png))
        .expect("cairo can read the picture the program wrote");
    let (wide, high) = (surface.width(), surface.height());
    let stride = surface.stride() as usize;
    let data = surface.data().expect("the surface is readable").to_vec();
    // The plate's own colour: the top-left corner, which no band covers under either box tested here.
    let corner = &data[..4];
    let same = |pixel: &[u8]| pixel[..3] == corner[..3];
    let mut top = 0usize;
    let mut bottom = 0usize;
    for y in 0..high as usize {
        for x in 0..wide as usize {
            let pixel = &data[y * stride + x * 4..][..4];
            if same(pixel) {
                continue;
            }
            if y < high as usize / 4 {
                top += 1;
            } else if y >= 3 * high as usize / 4 {
                bottom += 1;
            }
        }
    }
    (top, bottom)
}

/// S6's refusal, reached from the PAGE's emptiness rather than the file's: a session with a frame in the row
/// but no instruction anywhere must answer with the spec's own sentence, because §S6 refuses before a call.
#[test]
fn f5_6_s6_an_empty_page_instruction_refuses_before_any_call() {
    let sd = FakeSd::start(vec![]);
    let scene = fixture("no-page-instruction", &sd.url);
    let frame = frame_on_disk(&scene.tree, "room", 2.0, 1440, 1080);
    let files = frame_files(&scene.tree, &[(2.0, frame)]);
    let logs = collected();
    let publish = Publish {
        frames: vec!["frame:2.000".to_string()],
        title: "a title with no instruction under it".to_string(),
        description: "a description".to_string(),
        ..Default::default()
    };
    let answers = up::Answers {
        title: None,
        instruction: None,
        negative: None,
        description: Some("a description".into()),
        picked_frame: None,
    };
    let outcome = walk_with_frames(&scene, &publish, &answers, &files, false, &logs);
    assert_eq!(
        outcome.error.as_deref(),
        Some(up::NO_INSTRUCTION),
        "S6's own sentence, not a substitute: {:?}",
        outcome.error
    );
    assert!(outcome.published, "the text still lands ahead of the refusal (S5)");
    assert!(
        !scene.tree.thumbnail_plain_png().exists(),
        "a refused draw leaves no picture behind"
    );
    assert!(
        sd.try_next_body().is_none(),
        "nothing was asked of the drawing server"
    );
}

/// S3 again, but measured on the CROPPED SHAPE: the source frame is 4:3, the video is 16:9, and the picture
/// that stands as the thumbnail must be the video's shape at the thumbnail tier — not the frame's own edges.
#[test]
fn f5_6_s3_the_chosen_picture_is_the_videos_shape_not_the_frames() {
    let sd = FakeSd::start(vec![serde_json::json!({ "id": "job-shape" }).to_string()]);
    let scene = fixture("own-shape", &sd.url);
    let frame = frame_on_disk(&scene.tree, "room", 11.0, 1440, 1080);
    let listed = vec![(11.0, frame)];
    let files = frame_files(&scene.tree, &listed);
    let logs = collected();
    let publish = Publish {
        frames: vec!["frame:1.000".to_string()],
        prompt: "an instruction of my own".to_string(),
        thumb_title: "an old title".to_string(),
        ..Default::default()
    };
    let answers = up::Answers {
        title: Some("a picked-frame title".into()),
        instruction: None,
        negative: None,
        description: Some("a description".into()),
        picked_frame: Some(21.0),
    };
    let outcome = walk_with_frames(&scene, &publish, &answers, &files, true, &logs);
    assert_eq!(outcome.error, None, "the walk finished: {:?}", outcome.error);
    assert!(outcome.own, "S3: the picture is a chosen frame");
    let plain = scene.tree.thumbnail_plain_png();
    let (wide, high) = naivepost::produce_probe::size_of(&plain).expect("ffprobe can size the picture");
    let (want_wide, want_high) = up::frame_size_from_aspect("16:9");
    assert_eq!(
        (wide, high),
        (want_wide as i32, want_high as i32),
        "the chosen picture is the video's shape at the draw tier, not the 1440x1080 frame's"
    );
    let saved = naivepost::publish::load(&scene.tree).expect("publish.json reads back");
    assert!(saved.own, "and the record says so too");
    assert_eq!(saved.frames.len(), 1, "the row holds that frame only: {:?}", saved.frames);
    let _ = sd;
}

/// The publish leg the walk is handed: the program's own `publish::save`, so a check reads back the same file
/// a press writes.
fn save_publish_leg() -> Box<up::SavePublish> {
    Box::new(|record| naivepost::publish::save(record, &naivepost::ui::produce_languages::session_tree()))
}

/// The stamp writer's shape, named because `Box<dyn Fn …>` inline reads as noise (clippy's `type_complexity`).
type StampWrite = dyn Fn(&str) -> Result<(), String>;

fn stamp_writer() -> Box<StampWrite> {
    Box::new(|hash| {
        let path = naivepost::ui::produce_languages::session_tree().thumbnail_stamp();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|why| format!("{why}"))?;
        }
        std::fs::write(&path, format!("{hash}\n")).map_err(|why| format!("{why}"))
    })
}
