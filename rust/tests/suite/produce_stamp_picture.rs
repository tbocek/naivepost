//! F5.3's other half over the WIRE: `thumbnail.stamp` (§F5.6 S6→S7) is only read and written if the drawn
//! picture's bytes actually land, because the picture's stamp answers "were THESE images and THIS instruction
//! already drawn", and a run that never wrote the picture could neither prove it nor print onto it.
//!
//! Nothing is scripted inside the library here: the drawing server is a loopback fake the settings file names
//! (§02-services#1), and the requests it took are what the checks assert. The shape of its answers is the
//! prototype's: sd.cpp returns the picture inside the job answer as `result.images[0].b64_json`
//! (`go/sdcpp.go:137-150`, read at `sdGenerate` :222-230) — there is no separate picture URL in
//! `spec/02-services.md` §1's four sd.cpp paths.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};

use naivepost::layout::Tree;
use naivepost::produce_stamp::hash_text;
use naivepost::produce_upload as up;
use naivepost::produce_upload_draw as draw;
use naivepost::settings::{self, Conf};

use crate::common;

/// The picture every scripted draw hands back. Any bytes will do for the walk — it writes them and the print
/// leg reads them — but a PNG header keeps a bad decode visible rather than silent.
const PNG_BYTES: &[u8] = &[
    0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, b'r', b'g', b'b',
];

// ---- the fake sd.cpp -----------------------------------------------------------------------------

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

/// A scenario folder whose settings name this fake as the drawing server — the same seam a real settings edit
/// uses, so the program's own submit/poll/save legs are the ones under test.
fn fixture(tag: &str, sd_url: &str) -> Fixture {
    let scenario = common::scenario_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let root = std::env::temp_dir().join(format!("naivepost-f53pic-{tag}"));
    let _ = std::fs::remove_dir_all(&root);
    let config = root.join("config");
    let data = root.join("data");
    let session = root.join("session.naivepost");
    std::fs::create_dir_all(&session).expect("session folder");
    std::env::set_current_dir(&root).expect("cwd pinned to the scenario root");
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", &config);
        std::env::set_var("XDG_DATA_HOME", &data);
        // A leftover SD_SERVER would sit between the settings and the fake.
        std::env::set_var("SD_SERVER", "");
    }
    let paths = settings::from_environment().expect("settings folder");
    let mut conf = Conf::default();
    conf.sd_server = sd_url.to_string();
    settings::save(&paths, &conf).expect("settings written");
    let tree = Tree::new(&session).expect("a .naivepost folder is a project");
    Fixture {
        root,
        tree,
        _scenario: scenario,
    }
}

/// One request the fake took: the method and path it was asked for, and the body when it sent one.
#[derive(Debug)]
struct Seen {
    method: String,
    path: String,
}

#[derive(Default)]
struct Script {
    replies: Vec<String>,
    served: usize,
}

impl Script {
    fn take(&mut self) -> String {
        let index = self.served;
        self.served += 1;
        self.replies
            .get(index)
            .cloned()
            .or_else(|| self.replies.last().cloned())
            .unwrap_or_default()
    }
}

/// A loopback server answering the two job kinds from a script, one reply per request.
struct FakeSd {
    port: u16,
    received: Receiver<Seen>,
    /// Kept so the server's serving thread is not orphaned if a scenario's fake is dropped.
    #[allow(dead_code)]
    script: Arc<Mutex<Script>>,
}

impl FakeSd {
    fn start(replies: Vec<String>) -> FakeSd {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind a loopback port");
        let port = listener.local_addr().expect("a local address").port();
        let (sender, received) = mpsc::channel();
        let script = Arc::new(Mutex::new(Script { replies, served: 0 }));
        let serving = script.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                let serving = serving.clone();
                let sender = sender.clone();
                // One thread per connection: the leg reads to EOF, so a serial accept loop lets a request
                // whose answer was already taken hold its socket and stall every check behind it.
                std::thread::spawn(move || serve(stream, &serving, &sender));
            }
        });
        FakeSd {
            port,
            received,
            script,
        }
    }

    fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    fn next(&self) -> Seen {
        self.received
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("the drawing call reached the fake sd.cpp")
    }

    /// Nothing arrived within a short wait — what proves a second walk asked for no draw at all.
    fn expect_silent(&self) {
        match self
            .received
            .recv_timeout(std::time::Duration::from_millis(400))
        {
            Err(mpsc::RecvTimeoutError::Timeout) => (),
            other => {
                panic!("the drawing server was dialled when nothing should have been: {other:?}")
            }
        }
    }
}

fn serve(mut stream: TcpStream, script: &Arc<Mutex<Script>>, seen: &mpsc::Sender<Seen>) {
    serve_inner(&mut stream, script, seen);
    let _ = stream.shutdown(std::net::Shutdown::Both);
}

/// Records the request before it answers: the program's next call follows the answer at once, and a
/// record sent after the reply let that call's thread report first, so the checks saw the poll before
/// the submit.
fn serve_inner(
    stream: &mut TcpStream,
    script: &Arc<Mutex<Script>>,
    seen: &mpsc::Sender<Seen>,
) -> Option<()> {
    let mut reader = BufReader::new(stream.try_clone().ok()?);
    let mut request_line = String::new();
    reader.read_line(&mut request_line).ok()?;
    let parts: Vec<String> = request_line
        .split_whitespace()
        .map(|part| part.to_string())
        .collect();
    let mut length = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).ok()? == 0 {
            break;
        }
        let lower = line.to_ascii_lowercase();
        if let Some(rest) = lower.strip_prefix("content-length:") {
            length = rest.trim().parse().unwrap_or(0);
        }
        if line.trim().is_empty() {
            break;
        }
    }
    let mut payload = vec![0u8; length];
    if length > 0 {
        reader.read_exact(&mut payload).ok()?;
    }
    let _ = seen.send(Seen {
        method: parts.first().cloned().unwrap_or_default(),
        path: parts.get(1).cloned().unwrap_or_default(),
    });
    let reply = script.lock().expect("script lock").take();
    let framed = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        reply.len(),
        reply
    );
    let _ = stream.write_all(framed.as_bytes());
    let _ = stream.flush();
    Some(())
}

/// `POST /sdcpp/v1/img_gen` answer: a job id, no picture yet.
fn submitted(id: &str) -> String {
    serde_json::json!({ "id": id, "status": "queued" }).to_string()
}

/// `GET /sdcpp/v1/jobs/<id>` answer: done, carrying the picture exactly where sd.cpp puts it.
fn finished(id: &str, png: &[u8]) -> String {
    serde_json::json!({
        "id": id,
        "status": "completed",
        "result": { "images": [ { "b64_json": naivepost::checks::base64(png) } ] },
    })
    .to_string()
}

// ---- what the walk can ask for -------------------------------------------------------------------

/// The legs the walk drives, every one of them the program's own HTTP leg against the fake, plus a print leg
/// that records being asked. `stored_stamp` is whatever `thumbnail.stamp` holds when the walk starts, which is
/// how S6's skip question is asked in the program too.
struct Run {
    /// The tree the walk wrote into — what each check reads its files back from.
    tree: Tree,
    calls: Vec<String>,
    outcome: up::Outcome,
}

fn walk_once(tree: &Tree, instruction: &str, images: &[String], own: bool) -> Run {
    // Every leg is a `&'a dyn Fn` inside `Materials`, so nothing they touch may borrow the caller's arguments:
    // the tree is owned here and the state the walk reads is built before the legs are.
    let tree = tree.clone();
    let recorded = Arc::new(Mutex::new(Vec::<String>::new()));
    // Each leg records into its own clone of the recorder, moved in with `move` so it outlives the borrow.
    let mark_draw = recorded.clone();
    let mark_poll = recorded.clone();
    let mark_picture = recorded.clone();
    let mark_print = recorded.clone();
    let mark_stamp = recorded.clone();
    // A title AND one marked text: those are the two print passes S7 orders, so the walk has words of both
    // kinds to lay down and the check sees `print` twice rather than a pass skipped for emptiness.
    let publish = naivepost::project::Publish {
        prompt: instruction.to_string(),
        negative: "no words".to_string(),
        frames: images.to_vec(),
        own,
        title: "a title".to_string(),
        texts: vec![naivepost::project::TextMark {
            cx: 0.5,
            cy: 0.25,
            wf: 1.0,
            hf: 0.4,
            text: "one lecture at a time".to_string(),
        }],
        ..Default::default()
    };
    let answers = up::Answers::default();
    let ask_model = |_brief: &str| Ok(String::new());
    let ask_draw = {
        let tree = tree.clone();
        move |_body: &serde_json::Value| {
            mark_draw
                .lock()
                .expect("call lock")
                .push("draw".to_string());
            draw::submit(&tree, _body)
        }
    };
    let poll = {
        let tree = tree.clone();
        move |job: &up::DrawnJob| {
            mark_poll
                .lock()
                .expect("call lock")
                .push("poll".to_string());
            draw::poll(&tree, job)
        }
    };
    let save_picture = {
        let tree = tree.clone();
        move |job: &up::DrawnJob, png: &[u8]| {
            mark_picture
                .lock()
                .expect("call lock")
                .push("picture".to_string());
            save_plain(&tree, job, png)
        }
    };
    let print = move |_kind: &str, _words: &[String]| {
        mark_print
            .lock()
            .expect("call lock")
            .push("print".to_string());
        Ok(())
    };
    let nearest = |_at: f64| Some(1.0f64);
    let spread = || vec!["frame:5.0".to_string()];
    let stored = read_picture_stamp(&tree);
    let write_stamp = {
        let tree = tree.clone();
        move |hash: &str| {
            mark_stamp
                .lock()
                .expect("call lock")
                .push("stamp".to_string());
            write_picture_stamp(&tree, hash)
        }
    };
    let save_publish = {
        let tree = tree.clone();
        move |next: &naivepost::project::Publish| naivepost::publish::save(next, &tree)
    };
    let mats = up::Materials {
        save_publish: &save_publish,
        ask_model: &ask_model,
        model_asked: false,
        ask_draw: &ask_draw,
        poll: &poll,
        save_picture: &save_picture,
        print: &print,
        nearest_frame: &nearest,
        spread_frames: &spread,
        stored_stamp: stored,
        write_stamp: &write_stamp,
        edited_by_hand: false,
        // S3/S6: the media legs. This file asks the STAMP's question, not the crop's, so the legs stand for
        // "a session with a real folder" without an ffmpeg: the row's entries arrive as URLs of their own
        // bytes, and a chosen frame is written as the plain picture from whatever is on disk already.
        request_images: &|publish: &naivepost::project::Publish| {
            naivepost::produce_upload_media::Sent {
                base: (!publish.frames.is_empty()).then(|| "data:image/png;base64,".to_string()),
                urls: if !publish.frames.is_empty() {
                    vec!["data:image/png;base64,".to_string()]
                } else {
                    Default::default()
                },
                base_name: publish.frames.first().cloned().unwrap_or_default(),
                crop_percent: 100,
                centre: (0.5, 0.5),
                skipped: Vec::new(),
            }
        },
        own_picture: &|_publish: &naivepost::project::Publish| Ok(()),
    };
    let outcome = up::walk(
        "THE BRIEF",
        // The record the model's answers produced: a title is part of every check here, and `finish` refuses a
        // run with none (S8 keeps the previous value, so a record with nothing to keep says so).
        &publish,
        &up::Answers {
            title: Some("a title".to_string()),
            ..Default::default()
        },
        &mats,
        |_| {},
    );
    let _ = answers;
    Run {
        tree,
        calls: match Arc::try_unwrap(recorded) {
            Ok(owned) => owned
                .into_inner()
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
            Err(shared) => shared.lock().expect("call lock").clone(),
        },
        outcome,
    }
}

/// The picture's stamp as the page's own door reads it. Copied here rather than called from `src`'s private
/// helpers: these checks need to READ what the program wrote, and the reading rule is one `fs::read_to_string`.
fn read_picture_stamp(tree: &Tree) -> Option<String> {
    std::fs::read_to_string(tree.thumbnail_stamp())
        .ok()
        .map(|text| text.trim().to_string())
        .filter(|hash| !hash.is_empty())
}

fn write_picture_stamp(tree: &Tree, hash: &str) -> Result<(), String> {
    let path = tree.thumbnail_stamp();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|why| format!("{why}"))?;
    }
    std::fs::write(&path, format!("{hash}\n")).map_err(|why| format!("{why}"))
}

/// The program's own write, reached through the same shape `ui::produce_languages::save_drawn` uses: bytes to
/// the publish folder, creating it for a project that has never drawn.
fn save_plain(tree: &Tree, _job: &up::DrawnJob, png: &[u8]) -> Result<(), String> {
    let path = tree.thumbnail_plain_png();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|why| format!("{why}"))?;
    }
    std::fs::write(&path, png).map_err(|why| format!("{why}"))
}

/// The stamp S6/S7 say this state's picture carries: the hash the walk compares and the hash it writes.
fn expected_stamp(images: &[String], instruction: &str) -> String {
    hash_text(&up::thumbnail_stamp_text(
        images,
        instruction,
        "no words",
        up::DEFAULT_ASPECT,
        false,
    ))
}

fn silent(_line: &str) {}

// ---- the checks -----------------------------------------------------------------------------------

/// §F5.3 / §F5.6 S6→S7: the drawn bytes land, the words print onto them, and the picture's stamp is written —
/// every step over the real wire, asserted off the requests the fake took and the files the program wrote.
#[test]
fn f5_3_s15_the_drawn_picture_lands_and_its_stamp_is_written() {
    let sd = FakeSd::start(vec![submitted("job-9"), finished("job-9", PNG_BYTES)]);
    let scene = fixture("draw-and-stamp", &sd.url());
    let images = vec!["project:produce/publish/frame-1.jpg".to_string()];
    let run = walk_once(&scene.tree, "brighten the room", &images, false);

    assert_eq!(
        run.outcome.error, None,
        "the half failed: {:?}",
        run.outcome.error
    );
    assert_eq!(
        run.calls.iter().filter(|call| **call == "draw").count(),
        1,
        "one submit, got {:?}",
        run.calls
    );
    let submit = sd.next();
    assert_eq!(submit.method, "POST");
    assert_eq!(submit.path, "/sdcpp/v1/img_gen", "§3.5's submit path");
    let asked = sd.next();
    assert_eq!(
        asked.path, "/sdcpp/v1/jobs/job-9",
        "the poll asks about the job it was given"
    );

    // The bytes the job handed back are the plain picture, byte for byte.
    let plain = std::fs::read(run.tree.thumbnail_plain_png())
        .expect("the drawn picture landed where every re-print starts from");
    assert_eq!(
        plain, PNG_BYTES,
        "the file holds the server's bytes, not a re-encoding"
    );

    // And the stamp now says what was drawn from.
    let stamp = read_picture_stamp(&run.tree).expect("thumbnail.stamp was written");
    assert_eq!(
        stamp,
        expected_stamp(&images, "brighten the room"),
        "the stamp names this draw"
    );

    // The ORDER the spec draws: submit → poll → picture → the two print passes → stamp. Words go onto bytes
    // that exist, and S7's order is texts first then title.
    let order: Vec<&str> = run.calls.iter().map(String::as_str).collect();
    assert_eq!(
        order,
        vec!["draw", "poll", "picture", "print", "print", "stamp"],
        "S6→S7's order: {order:?}"
    );
    assert!(
        run.outcome.calls.contains(&"picture".to_string()),
        "{:?}",
        run.outcome.calls
    );
    assert!(
        run.outcome.calls.contains(&"stamp".to_string()),
        "{:?}",
        run.outcome.calls
    );
}

/// §F5.6 S6's own skip: with the stamp on disk saying these images and this instruction were already drawn,
/// a second walk asks the server NOTHING and writes nothing — which is only reachable because the first walk
/// wrote both the picture and the stamp.
#[test]
fn f5_3_s16_the_picture_stamp_makes_the_second_walk_skip_the_draw() {
    let sd = FakeSd::start(vec![submitted("job-9"), finished("job-9", PNG_BYTES)]);
    let scene = fixture("stamp-skips", &sd.url());
    let images = vec!["project:produce/publish/frame-1.jpg".to_string()];
    let first = walk_once(&scene.tree, "brighten the room", &images, false);
    assert_eq!(first.outcome.error, None, "{:?}", first.outcome.error);
    sd.next();
    sd.next();

    let second = walk_once(&scene.tree, "brighten the room", &images, false);
    assert_eq!(second.outcome.error, None, "{:?}", second.outcome.error);
    assert!(
        second.outcome.already_drawn,
        "the stamp says it was drawn, so the walk says it was"
    );
    assert!(
        !second.calls.contains(&"draw".to_string()) && !second.calls.contains(&"poll".to_string()),
        "a skipped draw dials the server NOTHING: {:?}",
        second.calls
    );
    assert!(
        !second.calls.contains(&"picture".to_string()),
        "and writes no new picture: {:?}",
        second.calls
    );
    // What it does instead is reprint onto the picture that already stands — the plain file is kept, the words
    // are laid again, and the stamp it writes says the same thing it said before.
    assert_eq!(
        second.calls,
        vec!["print", "print", "stamp"],
        "{:?}",
        second.calls
    );
    sd.expect_silent();
    // The stamp still holds the first walk's answer, unread and unwritten by the second.
    assert_eq!(
        read_picture_stamp(&scene.tree).unwrap(),
        expected_stamp(&images, "brighten the room")
    );
}

/// The stamp is a claim about THESE images and THIS instruction: change the instruction and the stamp no
/// longer matches, so the walk draws again and replaces the stamp with the new claim.
#[test]
fn f5_3_s17_a_stamp_that_no_longer_matches_draws_again() {
    let sd = FakeSd::start(vec![
        submitted("job-9"),
        finished("job-9", PNG_BYTES),
        submitted("job-10"),
        finished("job-10", b"second picture bytes"),
    ]);
    let scene = fixture("stamp-redraws", &sd.url());
    let images = vec!["project:produce/publish/frame-1.jpg".to_string()];
    walk_once(&scene.tree, "brighten the room", &images, false);
    sd.next();
    sd.next();

    let again = walk_once(&scene.tree, "and put the ship behind them", &images, false);
    assert_eq!(again.outcome.error, None, "{:?}", again.outcome.error);
    assert!(
        !again.outcome.already_drawn,
        "a different instruction is a different picture"
    );
    assert_eq!(
        again.calls,
        vec!["draw", "poll", "picture", "print", "print", "stamp"],
        "and the whole hand-off runs again: {:?}",
        again.calls
    );
    assert_eq!(
        sd.next().path,
        "/sdcpp/v1/img_gen",
        "the server was asked again"
    );
    assert_eq!(
        read_picture_stamp(&again.tree).unwrap(),
        expected_stamp(&images, "and put the ship behind them")
    );
    let plain =
        std::fs::read(again.tree.thumbnail_plain_png()).expect("the new picture is on disk");
    assert_eq!(
        plain,
        b"second picture bytes".to_vec(),
        "the newer draw's bytes stand"
    );
}

/// The decode is the wire's, not a test's: a base64 blob wrapped at 76 columns — what a proxy in front of
/// sd.cpp answers with — has to decode to the same picture as one that is not.
#[test]
fn f5_3_s15_a_wrapped_base64_blob_decodes_to_the_same_picture() {
    let wrapped = naivepost::checks::base64(PNG_BYTES)
        .as_bytes()
        .chunks(6)
        .map(|chunk| String::from_utf8_lossy(chunk).to_string())
        .collect::<Vec<String>>()
        .join("\n");
    let parsed: serde_json::Value = serde_json::from_str(
        &serde_json::json!({ "status": "completed", "result": { "images": [ { "b64_json": wrapped } ] } })
            .to_string(),
    )
    .expect("the fake's own JSON");
    let state = draw::state_of(&parsed);
    assert!(
        state.done,
        "a job that already handed back its image is done"
    );
    assert_eq!(
        state.png,
        PNG_BYTES.to_vec(),
        "line breaks are not bytes of the picture"
    );
}

/// A job that answers `done` with no image is said, not silently skipped: the alternative is a print leg
/// refusing for a reason one file and one step away from the truth.
#[test]
fn f5_3_s15_a_finished_job_with_no_picture_is_named() {
    let sd = FakeSd::start(vec![
        submitted("job-11"),
        serde_json::json!({ "id": "job-11", "status": "completed", "result": { "images": [] } })
            .to_string(),
    ]);
    let scene = fixture("no-picture", &sd.url());
    let run = walk_once(
        &scene.tree,
        "brighten the room",
        &["project:produce/publish/frame-1.jpg".to_string()],
        false,
    );
    let why = run.outcome.error.unwrap_or_default();
    assert!(
        why.contains("no image"),
        "the reason names what is missing: {why}"
    );
    assert!(
        !run.calls.contains(&"picture".to_string()),
        "{:?}",
        run.calls
    );
    assert!(
        !scene.tree.thumbnail_plain_png().exists(),
        "nothing was written as the plain picture"
    );
    assert!(
        read_picture_stamp(&scene.tree).is_none(),
        "and no stamp claims a draw that never landed"
    );
    silent("");
}

/// The endpoint is read from the settings file per request (§02-services#1), so the fake this file points at
/// is the one the program dialled — the seam, proved from the wire side.
#[test]
fn f5_3_s15_the_settings_file_names_the_server_the_program_dials() {
    let sd = FakeSd::start(vec![submitted("job-12"), finished("job-12", PNG_BYTES)]);
    let scene = fixture("settings-seam", &sd.url());
    let paths = settings::from_environment().expect("settings folder");
    let conf = settings::read(&paths).expect("settings read back");
    assert_eq!(
        conf.sd_server,
        sd.url(),
        "the fake is the address the settings hold"
    );
    let (_conf, endpoint) = endpoint_of(&conf);
    assert_eq!(endpoint.url, sd.url());
    // And the submit leg, pointed there, takes the job id out of the reply it gets back.
    let job = draw::submit(&scene.tree, &serde_json::json!({ "prompt": "x" }))
        .expect("the fake answered");
    assert_eq!(job.id, "job-12");
    let state = draw::poll(&scene.tree, &job).expect("the poll answered");
    assert!(state.done && state.png == PNG_BYTES, "{state:?}");
}

/// The same resolution the draw module uses, called here so a check can say WHICH address got dialled.
fn endpoint_of(conf: &Conf) -> (Conf, naivepost::services::Endpoint) {
    let endpoint = naivepost::services::image_endpoint(conf, |key| std::env::var(key).ok());
    (conf.clone(), endpoint)
}

/// The publish folder is the picture's home; asserting it once here keeps a path change visible in a wire test
/// as well as in the file-layout checks.
#[test]
fn f5_3_s15_the_picture_and_its_stamp_sit_beside_the_upload_text() {
    let root = std::env::temp_dir().join(format!("np-f53pic-layout-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("session.naivepost")).unwrap();
    let tree = Tree::new(root.join("session.naivepost")).expect("a project");
    let dir = tree.thumbnail_png().parent().unwrap().to_path_buf();
    assert_eq!(
        dir,
        tree.publish_dir(),
        "the picture lives with publish.json"
    );
    assert_eq!(tree.thumbnail_stamp().parent().unwrap(), dir);
    let plain_parent = tree.thumbnail_plain_png().parent().unwrap().to_path_buf();
    assert_eq!(plain_parent, dir);
    std::fs::remove_dir_all(&root).ok();
}
