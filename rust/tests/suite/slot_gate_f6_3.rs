//! F6.3 §4's gate, seen through the doors the program actually dials.
//!
//! The item's rules are about a WAIT, so every check here holds a real request on a real socket until it
//! chooses to let it go, and asks a SECOND one in the middle: the waiting line, the ⏹ that ends a wait, the
//! wait's number in `requests.tsv`, and one request at a time per SERVER (02 §1). Nothing here drives
//! `llm_liveness::Gate` directly — `tests/suite/llm_liveness_f6_3.rs` already pins those rules on the pure gate;
//! these prove the wiring (`slot_gate::hold` from `llm_retry_leg`/`audio_task`, and the TTS and image doors).
//!
//! Process-wide on purpose, so each test has its own loopback port: the gate registry is one app's set of
//! servers, and cargo runs these as threads of one binary.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use naivepost::audio_task::{self, Ask};
use naivepost::bodies::{self, ServerPath};
use naivepost::produce_upload as upload;
use naivepost::produce_upload_draw as draw;
use naivepost::settings::{self, Conf, Paths};
use naivepost::slot_gate;
use naivepost::ui;

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// A scenario: its own temp settings folder (pointing the audio and image boxes at one fake server) and its
/// own session folder, so `requests.tsv` has one writer.
struct Scene {
    paths: Paths,
    project: PathBuf,
    root: PathBuf,
}

/// A scenario whose settings are the process's own. `cwd` and the two `XDG_*` variables are process-wide and
/// every door re-reads them from there, so a scenario must be PINNED — merely holding one would have its conf
/// ignored and its dials would resolve the built-in loopback defaults.
///
/// The pin lasts until the scenario is dropped, which every test does with `let _pin = ...pin();` at the top:
/// a scenario's dials log after they return, and the wait must be observable while they run.
struct SceneGuard {
    scene: Scene,
    _pin: Pin,
}

impl std::ops::Deref for SceneGuard {
    type Target = Scene;
    fn deref(&self) -> &Scene {
        &self.scene
    }
}

static SCENARIO: std::sync::OnceLock<Mutex<()>> = std::sync::OnceLock::new();

/// The process-wide scenario pin, held by name so its owner is visible at the `let` rather than as a guard
/// whose only job is to exist until dropped (a pinned scenario's dials finish on their own threads, after the
/// pin was taken, which is why the pin cannot simply be released at the end of `pin`).
struct Pin(#[allow(dead_code)] std::sync::MutexGuard<'static, ()>);

impl Scene {
    fn new(label: &str, url: &str) -> Scene {
        Self::new_with_sd(label, url, url)
    }

    /// The same scenario with the DRAW box pointed somewhere of its own (two models, one machine).
    fn new_with_sd(label: &str, url: &str, sd_url: &str) -> Scene {
        let root = std::env::temp_dir().join(format!(
            "np-f63gate-{label}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("cfg/naivepost")).expect("config folder");
        std::fs::create_dir_all(root.join("data/naivepost")).expect("data folder");
        let project = root.join("session.naivepost");
        std::fs::create_dir_all(&project).expect("session folder");
        let paths = Paths {
            config_dir: root.join("cfg/naivepost"),
            data_dir: root.join("data/naivepost"),
        };
        let mut conf = Conf::default();
        conf.audio_server = url.to_string();
        conf.sd_server = sd_url.to_string();
        settings::save(&paths, &conf).expect("the conf");
        Scene {
            paths,
            project,
            root,
        }
    }

    /// Point the process at this scenario's settings, and keep the process-wide pin for as long as it lives.
    fn pin(self) -> SceneGuard {
        let guard = SCENARIO
            .get_or_init(|| Mutex::new(()))
            .lock()
            // A scenario that failed mid-run is over; its lock is still ours to hand on. Panicking here would
            // turn the first failure into five and hide which rule actually broke.
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        std::env::set_current_dir(&self.root).expect("cwd pinned to the scenario root");
        unsafe {
            // `settings::from_environment()` appends `naivepost` to these, so the pin names the PARENT of
            // `Scene::paths`' folders: pointing them at `cfg/naivepost` would make every door that resolves its
            // own paths read `cfg/naivepost/naivepost/llm.conf`, miss, and fall back to the shipped defaults.
            std::env::set_var("XDG_CONFIG_HOME", self.root.join("cfg"));
            std::env::set_var("XDG_DATA_HOME", self.root.join("data"));
        }
        SceneGuard {
            scene: self,
            _pin: Pin(guard),
        }
    }

    fn tree(&self) -> naivepost::layout::Tree {
        naivepost::layout::Tree::new(&self.project).expect("a .naivepost folder is a project")
    }
}

/// Clear the server overrides once, so each scenario's own conf box is what every door resolves (an
/// inherited `SD_SERVER` would outrank the conf and point a draw at a port nothing serves).
fn pin_env() {
    std::env::remove_var("NAIVEPOST_TTS_URL");
    // Cleared rather than removed: an exported value outranks the conf by §1's precedence, and `""` is what
    // every other draw test writes so a stray dev value cannot answer instead of the fake (§1).
    std::env::set_var("AUDIOCPP_SERVER", "");
    std::env::set_var("SD_SERVER", "");
    // The image door resolves its server from XDG_CONFIG_HOME, exactly like the audio doors; HOME is cleared
    // of any inherited value too so nothing outside the scenario can outrank the conf it just wrote (§1).
    std::env::set_var("HOME", std::env::temp_dir());
}

/// A fake server that reads each request and then says NOTHING until the test releases it — the state a
/// busy model is in, and the only state a wait can be tested against.
///
/// Releases are PER REQUEST, not one global flag: a scenario holds one call, lets a second through while
/// the first is still on the wire, and must be able to free them in any order — which is also the order the
/// gate hands slots back in.
struct Held {
    permits: Arc<Mutex<usize>>,
    arrived: Arc<std::sync::atomic::AtomicBool>,
    queue: Arc<std::sync::atomic::AtomicUsize>,
}

impl Held {
    /// Let ONE held request answer. Each call to [`held_server`] admits one more, so a test releases as many
    /// dials as it started.
    fn release(&self) {
        let mut permits = self.permits.lock().expect("the permit count");
        *permits += 1;
    }
    /// Block until the NEXT request lands, marking the queue's depth on the way in so a waiter — which never
    /// reaches the socket at all — is visible as "one dial queued, none arrived" rather than as a silence.
    fn wait_arrived(&self, arrived: &mut bool) {
        *arrived = false;
        self.queue.fetch_add(1, Ordering::SeqCst);
        let ok = wait_for(Duration::from_secs(15), || {
            self.arrived.load(Ordering::SeqCst)
        });
        self.queue.fetch_sub(1, Ordering::SeqCst);
        self.arrived.store(false, Ordering::SeqCst);
        *arrived = ok;
    }
}

impl Drop for Held {
    fn drop(&mut self) {
        self.release();
    }
}

/// How many requests the fake has been handed and not yet answered: the number a test compares with the slot
/// count to tell "its second dial is parked in the gate" from "its second dial reached the wire".
#[derive(Default, Clone)]
struct Tally {
    live: Arc<std::sync::atomic::AtomicUsize>,
}

impl Tally {
    fn here(&self) -> usize {
        self.live.load(Ordering::SeqCst)
    }
}

/// Start the held fake. `answer` is what each released request gets, chosen by path so one server can serve
/// the ASR task, the speech POST and the draw submit at once.
fn held_server(answer: fn(&str) -> (String, Vec<u8>)) -> (String, Held, Tally) {
    let tally = Tally::default();
    let live = tally.live.clone();
    let done = tally.live.clone();
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().expect("a local address").port();
    let outstanding = Arc::new(Mutex::new(0usize));
    let permits = outstanding.clone();
    let arrived = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let noted = arrived.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            let permits = permits.clone();
            let noted = noted.clone();
            let live = live.clone();
            let done = done.clone();
            std::thread::spawn(move || {
                let path = drain(&stream);
                noted.store(true, Ordering::SeqCst);
                // This request is now ON THE WIRE and unanswered: the count a test reads to see how many of
                // its dials the server actually got.
                live.fetch_add(1, Ordering::SeqCst);
                let deadline = Instant::now() + Duration::from_secs(30);
                loop {
                    let mut open = permits.lock().expect("the permit count");
                    if *open > 0 {
                        *open -= 1;
                        drop(open);
                        break;
                    }
                    drop(open);
                    if Instant::now() > deadline {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
                let (content_type, body) = answer(&path);
                let framed = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let mut stream = stream;
                let _ = stream.write_all(framed.as_bytes());
                let _ = stream.write_all(&body);
                let _ = stream.flush();
                let _ = stream.shutdown(std::net::Shutdown::Both);
                done.fetch_sub(1, Ordering::SeqCst);
            });
        }
    });
    (
        format!("http://127.0.0.1:{port}"),
        Held {
            permits: outstanding,
            arrived,
            queue: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        },
        tally,
    )
}

/// The ASR-shaped answer, for the audio door.
fn asr_answer(_path: &str) -> (String, Vec<u8>) {
    (
        "application/json".to_string(),
        br#"{"text":"hello there","words":[]}"#.to_vec(),
    )
}

/// A speech answer big enough to be a take (§F4.4 S5's floor is 1000 bytes of wav).
fn speech_answer(_path: &str) -> (String, Vec<u8>) {
    ("audio/wav".to_string(), vec![0u8; 1600])
}

/// Read one request far enough that the client is not left waiting to finish writing, and hand back its path.
fn drain(stream: &TcpStream) -> String {
    let mut reader = BufReader::new(stream.try_clone().expect("clone the socket"));
    let mut request_line = String::new();
    let _ = reader.read_line(&mut request_line);
    let path = request_line
        .split_whitespace()
        .nth(1)
        .unwrap_or("/")
        .to_string();
    let mut length = 0usize;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).unwrap_or(0) == 0 || header.trim().is_empty() {
            break;
        }
        if header.to_ascii_lowercase().starts_with("content-length:") {
            length = header
                .split(':')
                .nth(1)
                .unwrap_or("0")
                .trim()
                .parse()
                .unwrap_or(0);
        }
    }
    let mut body = vec![0u8; length];
    if length > 0 {
        let _ = reader.read_exact(&mut body);
    }
    path
}

/// Poll `check` until it holds, failing loudly rather than hanging the suite when it never does.
fn wait_until(check: impl Fn() -> bool, why: &str) {
    assert!(wait_for(Duration::from_secs(15), check), "timed out: {why}");
}

/// How long a dial waits for its own turn in the gate before a test stops calling it "parked". Deliberately
/// much shorter than [`wait_until`]'s budget: this number judges a waiter, that one judges a hand-off.
const PARKED_MS: u64 = 3000;

/// Wait until `check` holds or `budget` runs out, reporting which happened instead of asserting.
fn wait_for(budget: Duration, check: impl Fn() -> bool) -> bool {
    let deadline = Instant::now() + budget;
    while !check() {
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    true
}

/// A dial whose slot has not come round yet: still running after [`PARKED_MS`]. Every dial in this file
/// answers within 100 ms of reaching the fake, so a thread alive that long is parked in the gate, not on the
/// socket — the only honest meaning of "it is waiting", and something a plain sleep-then-`is_finished` cannot
/// say (the OS thread may not even have been scheduled yet, and GTK's main loop runs no ticks here).
fn parked(handle: &std::thread::JoinHandle<impl Send>) -> bool {
    wait_for(Duration::from_millis(PARKED_MS), || handle.is_finished())
}

/// A dial that got its slot and reached the fake: the only event that proves it is NOT parked in the gate.
fn on_the_wire(held: &Held) -> bool {
    let mut landed = false;
    held.wait_arrived(&mut landed);
    landed
}

/// A dial that has had [`PARKED_MS`] and is still running: parked in the gate. `why` is the test's own words,
/// echoed into the failure so a hung thread reads as the rule it broke.
fn still_parked(handle: &std::thread::JoinHandle<impl Send>, why: &str) {
    assert!(!parked(handle), "{why}");
}

/// One dial of the real audio door (ASR task), against the conf's box.
fn dial_asr(scene: &Scene, step: &str, model: &str) -> Result<String, String> {
    let audio = ServerPath::from_upload("/uploads/c00.wav").expect("absolute");
    audio_task::dial(
        &Some(scene.tree()),
        &scene.paths,
        Ask {
            step,
            job: "c00.wav",
            task: "asr",
        },
        model,
        &bodies::asr_body(model, &audio, "en"),
    )
}

/// The log lines a worker thread wrote since `from`.
///
/// The sink is THREAD-LOCAL (`ui::window_logs` reads this thread's list), so a dial that ran on another
/// thread logged into a list this one cannot see. Every check here therefore keeps the log by hand: the
/// worker copies its own lines after its dial returns and hands them back with the result. Same state the
/// program writes, read through the same door — the thread-local sink is the app's own, not a test's copy.
fn take_logs(from: usize) -> Vec<String> {
    ui::window_logs()[from..].to_vec()
}

/// A dial's outcome plus the log lines it wrote, as one hand-back across the thread boundary.
struct Dial {
    ok: bool,
    err: Option<String>,
    logs: Vec<String>,
}

/// Start a dial on its own thread, reporting its outcome and its log lines when it returns.
fn dial_thread(
    paths: Paths,
    project: PathBuf,
    root: PathBuf,
    step: &'static str,
    model: &'static str,
) -> std::thread::JoinHandle<Dial> {
    std::thread::spawn(move || {
        let from = ui::window_logs().len();
        let scene = Scene {
            paths,
            project,
            root,
        };
        let outcome = dial_asr(&scene, step, model);
        Dial {
            ok: outcome.is_ok(),
            err: outcome.err(),
            logs: take_logs(from),
        }
    })
}

/// The audio door on a thread that can log on its own thread — the shape every check below uses.
fn dial_async(
    scene: &Scene,
    step: &'static str,
    model: &'static str,
) -> std::thread::JoinHandle<Dial> {
    dial_thread(
        scene.paths.clone(),
        scene.project.clone(),
        scene.root.clone(),
        step,
        model,
    )
}

/// A speech dial on its own thread, so it can take its slot (and wait for one) off the main thread.
fn speak_thread(
    scene: &Scene,
    voice: &'static str,
    text: &'static str,
    seed: i64,
) -> std::thread::JoinHandle<(bool, Vec<String>)> {
    std::thread::spawn({
        let scene = Scene {
            paths: scene.paths.clone(),
            project: scene.project.clone(),
            root: scene.root.clone(),
        };
        move || {
            let from = ui::window_logs().len();
            let tree = scene.tree();
            let conf = settings::read(&scene.paths).expect("conf");
            let endpoint = naivepost::services::audio_endpoint(&conf, |_| None);
            let body = bodies::speech_body(
                voice,
                text,
                &ServerPath::from_upload("/v.wav").expect("absolute"),
                "en",
                &[0.0; 8],
                seed,
            );
            let ok = naivepost::speak_leg::speech_call(&tree, &endpoint, &body).is_ok();
            (ok, take_logs(from))
        }
    })
}

/// A draw submit on its own thread, for the same reason.
///
/// It dials `produce_upload_draw::submit`, the program's own door, so the address it uses is the one §1 gives:
/// the scenario conf's draw box, read through the process environment this scenario pinned. Nothing here hands
/// an endpoint in — the gate, the row and the socket are reached exactly as a real draw reaches them.
fn draw_thread(scene: &Scene) -> std::thread::JoinHandle<(bool, String)> {
    let scene = Scene {
        paths: scene.paths.clone(),
        project: scene.project.clone(),
        root: scene.root.clone(),
    };
    std::thread::spawn(move || {
        let tree = scene.tree();
        let body = upload::request_body(
            &upload::draw_prompt("brighten it", upload::Band::Upper),
            None,
            1280,
            720,
            &[],
        );
        match draw::submit(&tree, &body) {
            Ok(job) => (true, job.id),
            Err(why) => (false, why),
        }
    })
}

/// §4's waiting line, said ONCE for a queueing spell, and the second request goes through on the release.
#[test]
fn f6_3_s2_a_waiting_step_says_the_line_once_and_first_come_first_served_holds() {
    pin_env();
    let (url, held, tally) = held_server(asr_answer);
    let _pin = Scene::new("once", &url).pin();
    let scene = &_pin;
    let model = "f63-once-asr";

    let first = dial_async(scene, "Prepare", model);
    assert!(on_the_wire(&held), "the first dial reached the server");
    let holders = slot_gate::holders(&url);
    assert_eq!(holders.len(), 1, "one slot, one holder: {holders:?}");
    let busy = holders[0].clone();

    let second = dial_async(scene, "Marking", model);
    // The wait has to be VISIBLE: §4's sentence, in §4's words, naming the holder that has the slot. The
    // line is this thread's own to read, so it is pulled from the dial's hand-back once it returns. The
    // waiter never reaches the socket, so the only proof it is queued is that it is still alive after
    // PARKED_MS while the fake saw nothing new.
    still_parked(
        &second,
        "the second request waits rather than going out: the voice has ONE slot",
    );
    assert_eq!(
        tally.here(),
        1,
        "the waiter is parked in the gate, not on the wire: {}",
        tally.here()
    );

    held.release();
    // One held request per release: the FIRST dial answers and gives its slot back, then the waiter's turn
    // arrives and the server answers it from its own allowance. Releasing twice up front would answer both
    // at once and prove nothing about the hand-off.
    let first_done = first.join().expect("first ends");
    assert!(
        first_done.ok,
        "the first dial succeeded: {:?}",
        first_done.err
    );
    // The waiter is admitted only now, so its request reaches the fake only now: it needs its own release.
    held.release();
    wait_until(
        || second.is_finished(),
        "the freed slot went to whoever waited first",
    );
    let second_done = second.join().expect("second ends");
    assert!(second_done.ok, "the second succeeded after the wait");

    let said: Vec<String> = second_done
        .logs
        .iter()
        .filter(|line| line.contains("waiting for") && line.contains("busy with"))
        .cloned()
        .collect();
    assert_eq!(
        said.len(),
        1,
        "the line is said ONCE per queueing spell, not per poll: {said:?}"
    );
    // §4's line names the STEP, the server it waits for and the holder; the holder carries the per-ask `#N`
    // suffix `slot_gate::hold` adds (two asks of one step are distinct queue members).
    assert!(
        said[0].starts_with(">>> Marking")
            && said[0].contains(&format!("waiting for {url} -- busy with {busy}")),
        "§4 spells the step, the server and the holder: {}",
        said[0]
    );
    wait_until(
        || slot_gate::holders(&url).is_empty(),
        "both slots came back",
    );
}

/// ⏹ ends a wait, and a step that quits leaves no place in the queue behind.
#[test]
fn f6_3_s2_the_wait_is_cancellable_by_stop() {
    pin_env();
    let (url, held, _tally) = held_server(asr_answer);
    let _pin = Scene::new("stop", &url).pin();
    let scene = &_pin;
    let model = "f63-stop-asr";

    let first = dial_async(scene, "Prepare", model);
    assert!(
        on_the_wire(&held),
        "the first dial is on the wire with the only slot"
    );
    let queued = dial_async(scene, "Marking", model);
    // Let the second ask join the queue before pulling the stop, so this is a cancelled WAIT and not a
    // dial that never asked: `parked` spends PARKED_MS proving the thread is alive and the wire is quiet.
    still_parked(&queued, "the second ask was queued before the stop came");
    naivepost::cancel_leg::cancel_now();
    wait_until(|| queued.is_finished(), "⏹ ended the wait");
    let outcome = queued.join().expect("the queued dial returned");
    assert!(
        !outcome.ok,
        "a request that waited and was stopped reports a refusal, not a reply"
    );
    assert!(
        outcome.logs.iter().any(|line| line.contains("waiting for")),
        "it really was waiting when the stop came: {:?}",
        outcome.logs
    );
    naivepost::cancel_leg::clear();
    held.release();
    let _ = first.join();
    wait_until(
        || slot_gate::holders(&url).is_empty(),
        "the held slot came back with its call",
    );
}

/// §4's queue sentence for a step that waited behind ANOTHER step's call, and the prototype's silence when
/// a step waits behind its own earlier one.
#[test]
fn f6_3_s4_a_queued_step_names_the_step_it_waited_behind() {
    pin_env();
    let (url, held, tally) = held_server(asr_answer);
    let _pin = Scene::new("names", &url).pin();
    let scene = &_pin;

    // (a) behind ANOTHER step: the sentence, naming that step.
    let model_a = "f63-names-other";
    let first = dial_async(scene, "Prepare", model_a);
    assert!(on_the_wire(&held), "the first dial holds the slot");
    let second = dial_async(scene, "Marking", model_a);
    // Only ONE request has reached the wire so far: the waiter is parked in the gate, not on the socket.
    still_parked(&second, "the waiter never reached the server");
    assert_eq!(
        tally.here(),
        1,
        "the waiting request never reached the server: {}",
        tally.here()
    );
    held.release();
    assert!(first.join().expect("first done").ok);
    held.release();
    held.release();
    wait_until(|| second.is_finished(), "the waiter went in");
    let second_done = second.join().expect("second done");
    assert!(second_done.ok);
    assert!(
        second_done
            .logs
            .iter()
            // §4's line names the step it queued behind; the holder carries the gate's ask `#N`.
            .any(|line| line.contains(">>> Marking") && line.contains("busy with Prepare")),
        "§4's prototype line names the step it queued behind: {:?}",
        second_done.logs
    );
}

/// The other half of §4's queue sentence: a step that waits behind its OWN earlier call says nothing.
/// Its own `#[test]` because a scenario pins the process-wide cwd and `XDG_*` for as long as it lives — two
/// scenarios cannot be pinned at once, so each gets its own test rather than a second pin mid-body.
#[test]
fn f6_3_s4_a_step_behind_its_own_earlier_call_waits_silently() {
    pin_env();
    let (url2, held2, _tally2) = held_server(asr_answer);
    let _pin2 = Scene::new("names-self", &url2).pin();
    let scene2 = &_pin2;
    let model_b = "f63-names-self";
    let one = dial_async(scene2, "Publish", model_b);
    assert!(on_the_wire(&held2), "the first of the two is on the wire");
    let two = dial_async(scene2, "Publish", model_b);
    still_parked(&two, "the second of the pair is queued behind the first");
    held2.release();
    assert!(one.join().expect("one done").ok);
    held2.release();
    wait_until(|| two.is_finished(), "the second of the pair went in");
    let two_done = two.join().expect("two done");
    assert!(two_done.ok);
    // A step queued behind its OWN earlier call waits silently: §4's line names ANOTHER step's slot, and
    // "publish waited for publish" says nothing the two requests in flight do not already say. The holder
    // named in brackets is therefore this step's own ask — proof of what it queued behind — while the
    // sentence around it stays unwritten.
    assert!(
        two_done
            .logs
            .iter()
            .all(|line| !line.contains("waited for the LLM") && !line.contains("busy with Publish")),
        "a step behind its own earlier call says nothing about whom it waited for: {:?}",
        two_done.logs
    );
}

/// The wait is a number, in the column §10 gives it.
#[test]
fn f6_3_s2_a_slot_wait_lands_in_the_requests_row() {
    pin_env();
    let (url, held, _tally) = held_server(asr_answer);
    let _pin = Scene::new("row", &url).pin();
    let scene = &_pin;
    let model = "f63-row-asr";

    let first = dial_async(scene, "Prepare", model);
    assert!(on_the_wire(&held), "the first dial holds the slot");
    let second = dial_async(scene, "Marking", model);
    still_parked(&second, "the second ask is queued behind the first");
    held.release();
    assert!(first.join().expect("first").ok);
    held.release();
    wait_until(|| second.is_finished(), "the waiter went in");
    held.release();
    assert!(second.join().expect("second").ok);

    let rows = std::fs::read_to_string(scene.tree().requests_tsv()).expect("requests.tsv");
    let row = rows.lines().last().expect("a row");
    let fields: Vec<&str> = row.split('\t').collect();
    let wait_field = fields.get(12).copied().unwrap_or_default();
    assert!(
        !wait_field.is_empty(),
        "the waiting row records its wait: {row}"
    );
    let secs: f64 = wait_field.parse().expect("the wait column is seconds");
    assert!(secs > 0.0, "and it is a positive number: {row}");
}

/// One request at a time per server, and different servers side by side (02 §1): while the voice holds the
/// audio server, a draw on the drawing server goes straight through, and an ASR request to the audio server
/// waits its turn behind the voice like a second line does.
#[test]
fn f6_3_s3_each_server_takes_one_request_and_servers_run_side_by_side() {
    pin_env();
    let (url, held, _tally) = held_server(|path| match path {
        p if p.contains("audio/speech") => speech_answer(p),
        other => asr_answer(other),
    });
    let (image_url, image_held, _image_tally) = held_server(|_path| {
        (
            "application/json".to_string(),
            br#"{"id":"job-f63"}"#.to_vec(),
        )
    });
    let _pin = Scene::new_with_sd("two-servers", &url, &image_url).pin();
    let scene = &_pin;

    // One speech call holds the audio server.
    let speaking = speak_thread(scene, "f63-tts-voice", "one line", 1);
    assert!(on_the_wire(&held), "the speech POST reached the server");
    assert_eq!(
        slot_gate::holders(&url).len(),
        1,
        "the voice holds the audio server"
    );

    // A second line, and an ASR request to the SAME server, both wait: one request at a time per server.
    let second_speech = speak_thread(scene, "f63-tts-voice", "another line", 2);
    still_parked(&second_speech, "a second line waits behind the first");
    let listening = dial_async(scene, "Prepare", "f63-asr");
    still_parked(&listening, "an ASR request to the same server waits too");

    // A draw on its OWN server goes straight through while the audio server is still held.
    let drew = draw_thread(scene);
    image_held.release();
    assert!(
        wait_for(Duration::from_secs(5), || drew.is_finished()),
        "the draw was not held by the audio server"
    );
    let (drew_ok, why) = drew.join().expect("the draw returned");
    assert!(
        drew_ok,
        "a draw on its own server is not held by the voice: {why}"
    );
    assert_eq!(
        slot_gate::holders(&image_url).len(),
        0,
        "the draw gave its server back"
    );

    // The audio server's queue, first come first served: the first line, then the second, then the ASR.
    held.release();
    let (spoke_ok, _) = speaking.join().expect("speech ended");
    assert!(spoke_ok, "the first line was spoken");
    held.release();
    wait_until(
        || second_speech.is_finished(),
        "the second line went in next",
    );
    let (second_ok, second_logs) = second_speech.join().expect("second speech ended");
    assert!(
        second_ok,
        "the queued line was spoken after the wait: {second_logs:?}"
    );
    held.release();
    wait_until(|| listening.is_finished(), "the ASR request went in last");
    assert!(
        listening.join().expect("asr ended").ok,
        "the ASR request was answered"
    );
}
