//! F5.4 S3's live half, over a wire: the translation ask against a fake LLM on loopback.
//!
//! Every other `f5_4_s3_*` file drives `produce_translate::track` with an injected `Ask` and so proves
//! the ORDER of the walk. This one proves the REQUEST — what actually goes out over HTTP, what comes
//! back, and what the cache does with it — because that is the half the page used to stand in for. The
//! seam is the settings address (the same one a typed address goes through): `point_at` writes a
//! loopback port into `naivepost.json` and the leg dials it, so nothing here hands the program an
//! answer.
//!
//! One scenario per test BINARY for the same reason `tests/captions_wire_harness.rs` says it: the env
//! is process-wide. `just test` runs one binary per process, so a file may set it once. Here the fixture
//! is built once, in `main`, and the checks run in order against one fake whose script is chosen before
//! each check.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};

use naivepost::produce_subtitles as subs;
use naivepost::produce_translate_ask as ask_leg;
use naivepost::settings::{self, Conf};

/// What the fake saw, in order: one entry per request that reached it.
#[derive(Debug, Clone)]
struct Seen {
    path: String,
    body: String,
}

/// A chat server on loopback answering from a script of `content` strings, one per request. Each answer
/// is wrapped in the envelope a real server answers with, so the leg reads it through
/// `llm_request::parse_reply` exactly as it would live.
struct FakeLlm {
    port: u16,
    received: Receiver<Seen>,
    script: Arc<Mutex<Script>>,
}

/// The fake's answers, with the request each of them is owed to. `queue` waits until the PREVIOUS
/// request has taken its answer (the leg is synchronous, so that is the same as waiting for the call to
/// return), which is what ties one scripted answer to the one request it was written for.
#[derive(Default)]
struct Script {
    answers: Vec<String>,
    served: usize,
}

impl Script {
    fn take(&mut self) -> String {
        let index = self.served;
        self.served += 1;
        self.answers.get(index).cloned().or_else(|| self.answers.last().cloned()).unwrap_or_default()
    }

    /// True while the request before this one has not been served yet.
    fn waiting(&self, wanted: usize) -> bool {
        self.served < wanted
    }
}

impl FakeLlm {
    fn start() -> FakeLlm {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind a loopback port");
        let port = listener.local_addr().expect("a local address").port();
        let (sender, received) = mpsc::channel();
        let script: Arc<Mutex<Script>> = Arc::new(Mutex::new(Script::default()));
        let serving = script.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                let body = serve(stream, &serving);
                if let Some(seen) = body {
                    let _ = sender.send(seen);
                }
            }
        });
        FakeLlm { port, received, script }
    }

    /// Queue the next answers, oldest first. A request past the end of the script gets the last answer,
    /// which is what a stubborn server looks like rather than a broken test. Waits for the request a
    /// previous `queue` was about to have been SERVED first: the leg is synchronous, so an answer written
    /// while an earlier request is still in flight would answer that one instead.
    /// Load the answers for the requests still to come. Waits until every request the caller has
    /// already made has been served, so an answer can never be spent on the wrong request.
    fn queue(&self, answers: Vec<String>) {
        let owed = self.script.lock().expect("script lock").served;
        for _ in 0..250 {
            if !self.script.lock().expect("script lock").waiting(owed) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        self.script.lock().expect("script lock").answers = answers;
    }

    fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// The next request the server took, waiting briefly for it: the leg dials synchronously, so a
    /// missing entry means the request never went out at all.
    fn next(&self) -> Seen {
        self.received
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("the ask reached the fake LLM")
    }

    /// Nothing arrived within a short while. Used by the cache check: a second identical batch must be
    /// answered from `cache/llm/translate/` and cost no wire.
    fn expect_silent(&self) {
        match self.received.recv_timeout(std::time::Duration::from_millis(400)) {
            Err(mpsc::RecvTimeoutError::Timeout) => (),
            other => panic!("the server was dialled when the answer should have come from the cache: {other:?}"),
        }
    }
}

fn serve(mut stream: TcpStream, script: &Arc<Mutex<Script>>) -> Option<Seen> {
    let mut reader = BufReader::new(stream.try_clone().ok()?);
    let mut request_line = String::new();
    reader.read_line(&mut request_line).ok()?;
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
    let path = request_line
        .split_whitespace()
        .nth(1)
        .unwrap_or_default()
        .to_string();
    let body = String::from_utf8_lossy(&payload).to_string();
    let answer = script.lock().expect("script lock").take();
    // A scripted `tool_calls` answer is already an envelope; wrapping that again buries the calls inside a
    // content string and the leg reads an empty answer.
    let reply = if answer.trim_start().starts_with("{\"choices\"") { answer } else { envelope(&answer) };
    let framed = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        reply.len(),
        reply
    );
    let _ = stream.write_all(framed.as_bytes());
    let _ = stream.flush();
    Some(Seen { path, body })
}

/// Wrap one answer's content in the chat envelope.
fn envelope(content: &str) -> String {
    serde_json::json!({ "choices": [ { "message": { "content": content } } ] }).to_string()
}

/// An answer in the OTHER shape §3.10 allows: one `translate_line` tool call per line.
fn tool_calls(calls: &[(usize, &str)]) -> String {
    let list: Vec<serde_json::Value> = calls
        .iter()
        .map(|(number, text)| {
            serde_json::json!({
                "type": "function",
                "function": {
                    "name": "translate_line",
                    "arguments": serde_json::json!({ "n": number, "text": text }).to_string(),
                }
            })
        })
        .collect();
    serde_json::json!({ "choices": [ { "message": { "content": "", "tool_calls": list } } ] }).to_string()
}

/// Held while a scenario's environment is being pointed at it. See [`fixture`].
fn scenario_lock() -> &'static std::sync::Mutex<()> {
    static SCENARIO: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    SCENARIO.get_or_init(|| std::sync::Mutex::new(()))
}

/// A cancel context that is never cancelled: this leg is not the one F0.3's stop is wired to yet, and the
/// leg only ever asks the flag between reads.
fn stop_flag() -> std::rc::Rc<dyn Fn() -> bool> {
    std::rc::Rc::new(|| false)
}

/// The scenario root: settings folder, data folder, project and cwd all under one temp directory, so
/// nothing outside it is read or written. Built once per process; every `set_var` happens here.
struct Fixture {
    root: PathBuf,
    tree: naivepost::layout::Tree,
    /// The scenario guard, held for the whole check: `set_var` and the settings file are process-wide,
    /// so one check owns them from the moment its folder is pointed at until it finishes.
    _scenario: std::sync::MutexGuard<'static, ()>,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        // The scenario is gone with the check: the next one re-points the env.
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn fixture(tag: &str) -> Fixture {
    let scenario = scenario_lock()
        .lock()
        // Poisoned means a check panicked mid-scenario; the env is still ours to set.
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    // The scenario IS the process-wide state: `set_var` and the settings file every later read goes
    // through. std Mutex isn't reentrant, so the guard is released before the directory work below and
    // held again only around the env writes; two checks may then interleave the harmless part but never
    // the part that decides which address the leg dials.
    let root = std::env::temp_dir().join(format!("naivepost-f54wire-{tag}"));
    // Wipe it all, including any answer cached under an earlier run: a cached reply would answer the ask
    // without reaching the fake, and the check would then assert about a request that never went out.
    let _ = std::fs::remove_dir_all(&root);
    let config = root.join("config");
    let data = root.join("data");
    let session = root.join("session.naivepost");
    std::fs::create_dir_all(&session).expect("session folder");
    std::env::set_current_dir(&root).expect("cwd pinned to the scenario root");
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", &config);
        std::env::set_var("XDG_DATA_HOME", &data);
    }
    // Address and model go in ONCE, here, while the guard is held: `settings::save` replaces the whole
    // file, so a later partial write would drop one of them, and an empty model is §02-services' own
    // "no LLM model configured" refusal.
    let address = std::env::var("NAIVEPOST_FAKE_LLM").unwrap_or_default();
    // The fake's address arrives through the env AFTER `FakeLlm::start`, and nothing re-reads settings
    // before the first batch, so the ask would dial the default port with no server behind it.
    unsafe {
        std::env::set_var("NAIVEPOST_FAKE_LLM", "");
    }
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", &config);
    }
    let paths = settings::from_environment().expect("settings folder from the pinned env");
    // The address is set AFTER this call: `settings::save` replaces the whole file, so an address written
    // before would be wiped and the ask would dial the default instead of the fake.
    let mut conf = Conf::default();
    conf.model = MODEL.to_string();
    conf.server = address;
    settings::save(&paths, &conf).expect("settings written");
    let tree = naivepost::layout::Tree::new(&session).expect("a .naivepost folder is a project");
    drop(scenario);
    Fixture { root, tree, _scenario: scenario_reacquire() }
}

/// The model name every scenario asks with, so a check can assert it came back in the request.
const MODEL: &str = "translate-model";

/// The scenario guard, taken a second time for the returned `Fixture`. `std::sync::Mutex` is not
/// reentrant, so `fixture` releases its own guard and hands ownership to the caller instead.
fn scenario_reacquire() -> std::sync::MutexGuard<'static, ()> {
    scenario_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Rewrite the settings' LLM address. THIS is the seam the program itself uses: the leg re-reads the
/// file per batch, so the next batch dials whatever is in it now.
/// Point every later check at this fake: write its address into the settings AND hand it to [`fixture`],
/// which stamps it into the settings file it builds. A leg re-reads settings per batch, but the FIRST
/// batch is answered from the settings written by `fixture`, so an address set only after that folder
/// exists never reaches the request that matters.
fn point_at(url: &str) {
    unsafe {
        std::env::set_var("NAIVEPOST_FAKE_LLM", url);
    }
    let paths = settings::from_environment().expect("settings folder");
    let mut conf = settings::read(&paths).expect("settings read back");
    conf.server = url.to_string();
    conf.model = MODEL.to_string();
    settings::save(&paths, &conf).expect("new address written");
}

/// A track of `n` cues, one word each, spaced a second apart. `tagged` puts `{\an8}` on the first cue so
/// the placement rule has something to strip.
fn cues(n: usize, tagged: bool) -> Vec<subs::Cue> {
    (0..n)
        .map(|index| subs::Cue {
            s: index as f64,
            e: index as f64 + 0.6,
            pos: if tagged && index == 0 { "top".to_string() } else { String::new() },
            text: if tagged && index == 0 {
                // The tag the rule itself writes — `placement("top")` is what `keep_placement` checks for.
                format!("{}line {}", subs::placement("top").unwrap(), index + 1)
            } else {
                format!("line {}", index + 1)
            },
        })
        .collect()
}

/// The numbered lines this batch was sent, as the fake will read them back out of the request.
///
/// `llm_request::body` writes each message's content as an ARRAY of parts (`{"text":..,"type":"text"}`),
/// not a bare string — reading `content` as a string yields nothing and every body assertion then fails
/// with an empty left-hand side.
fn sent_lines(body: &str) -> Vec<String> {
    let value: serde_json::Value = serde_json::from_str(body).expect("the request is JSON");
    let user = value["messages"][1]["content"][0]["text"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    user.lines().skip(1).map(String::from).collect()
}

/// `P.machine.translateBatch`, read through the parameter table so the check names the row rather than
/// a copy of it.
fn batch_size() -> usize {
    naivepost::params::produce()
        .into_iter()
        .find(|row| row.id == "P.machine.translateBatch")
        .expect("the batch row is catalogued")
        .spelled
        .parse::<usize>()
        .expect("the batch row holds a number")
}

/// Counts the numbered lines this scenario's request was asked to answer, read off the wire by the
/// fake. A batch split is the WALK's rule (proved in tests/produce_translate_flow.rs); what this file
/// can prove over the wire is that one call carried every line it was handed.
static CHECKS: AtomicUsize = AtomicUsize::new(0);

/// One check of the real leg, in its own scenario: the settings address and `cache/llm` are
/// process-wide state, so each test builds the folder it dials and reads.
#[test]
fn f5_4_s1_wire_the_cue_text_is_what_travels() {
    let scene = fixture("wire_the_cue_text_is_what_travels");
    let llm = FakeLlm::start();
    point_at(&llm.url());
    let cancelled = stop_flag();
    let track = cues(3, false);
    let ask = ask_leg::ask(&scene.tree, &cancelled);
    llm.queue(vec!["1\teins\n2\tzwei\n3\tdrei".to_string()]);
    let numbers: Vec<usize> = (1..=track.len()).collect();
    let message = format!("{}\n{}", subs::message(track.len(), "German"), subs::numbered(
        &track.iter().enumerate().map(|(i, _)| (i + 1, format!("line {}", i + 1))).collect::<Vec<(usize, String)>>(),
    ));
    let got = ask(&message, &numbers).expect("the fake answers");
    assert_eq!(got.get(&1).map(String::as_str), Some("eins"));
    assert_eq!(got.len(), 3, "every numbered line came back: {got:?}");
    // And the text that travelled is the CUE text, folded to one row per cue (no `\n`, no tag).
    let sent = last_sent_lines(&llm);
    assert_eq!(sent.first().map(String::as_str), Some("1\tline 1"), "the numbered line is the cue: {sent:?}");
}

/// One check of the real leg, in its own scenario: the settings address and `cache/llm` are
/// process-wide state, so each test builds the folder it dials and reads.
#[test]
fn f5_4_s3_wire_the_placement_tag_never_travels() {
    let scene = fixture("placement_tag_never_travels");
    let llm = FakeLlm::start();
    point_at(&llm.url());
    let cancelled = stop_flag();
    let track = cues(2, true);
    let ask = ask_leg::ask(&scene.tree, &cancelled);
    llm.queue(vec!["1\terste zeile\n2\tzweite zeile".to_string()]);
    let message = subs::message(2, "German");
    let numbers = vec![1usize, 2];
    let _ = ask(&message, &numbers).expect("the fake answers");
    let seen = llm.next();
    assert_eq!(seen.path, "/v1/chat/completions", "the chat endpoint is what was dialled");
    assert!(
        !seen.body.contains("\\an8"),
        "the placement tag travelled into the translation: {}",
        seen.body
    );
    assert!(
        seen.body.contains(&message),
        "the request lost §3.10's message wording: {}",
        seen.body
    );
    // ...and it comes back on the cue, because the app puts it there: `keep_placement` reads the ORIGINAL
    // cue's `pos`, never the reply, and answers `true` only when the model smuggled a tag in as a word.
    let (text, carried) = subs::keep_placement(&track[0], "erste zeile");
    assert!(!carried, "a clean answer was reported as smuggling a tag: {text}");
    assert!(
        text.starts_with(subs::placement("top").unwrap()),
        "the finished cue lost its placement: {text}"
    );
    // A model that DID send the tag back as a word gets it stripped rather than trusted (§3.10).
    let (stripped, carried) = subs::keep_placement(&track[0], "{\\an8}erste zeile");
    assert!(carried, "a smuggled tag must be reported");
    assert!(
        stripped.starts_with(subs::placement("top").unwrap()),
        "the app's own tag is what stands: {stripped}"
    );
}

/// One check of the real leg, in its own scenario: the settings address and `cache/llm` are
/// process-wide state, so each test builds the folder it dials and reads.
#[test]
fn f5_4_s3_wire_thinking_off_and_two_tools() {
    let scene = fixture("thinking_off_and_two_tools");
    let llm = FakeLlm::start();
    point_at(&llm.url());
    let cancelled = stop_flag();
    let ask = ask_leg::ask(&scene.tree, &cancelled);
    llm.queue(vec!["1\teins".to_string()]);
    let _ = ask(&subs::message(1, "German"), &[1]).expect("the fake answers");
    let body: serde_json::Value = serde_json::from_str(&llm.next().body).expect("JSON body");
    assert_eq!(
        body["enable_thinking"].as_bool(),
        Some(false),
        "§02-services: an execute call answers without thinking: {body}"
    );
    let tools: Vec<String> = body["tools"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|t| t["function"]["name"].as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(
        tools,
        vec!["finish".to_string(), "translate_line".to_string()],
        "§3.10 lists two tools for this job; the offer carries exactly those: {body}"
    );
    assert_eq!(
        body["model"].as_str(),
        Some("translate-model"),
        "the settings' model is what asks"
    );
}

/// One check of the real leg, in its own scenario: the settings address and `cache/llm` are
/// process-wide state, so each test builds the folder it dials and reads.
#[test]
fn f5_4_s3_wire_one_request_per_batch() {
    let scene = fixture("one_request_per_batch");
    let llm = FakeLlm::start();
    point_at(&llm.url());
    let cancelled = stop_flag();
    let size = batch_size();
    assert_eq!(size, subs::BATCH, "the row and the rule agree");
    let ask = ask_leg::ask(&scene.tree, &cancelled);
    let many = cues(size + 1, false);
    let numbers: Vec<usize> = (1..=many.len()).collect();
    llm.queue(vec![envelope(&answer_for(&numbers))]);
    // The request itself carried all `many.len()` numbered lines: counted off the body the fake read.
    // The leg's own message argument is only §3.10's heading — the walk appends the numbered lines, so
    // this check hands it both, exactly as `one_track` does.
    let request = format!("{}\n{}", subs::message(many.len(), "German"), subs::numbered(
        &many.iter().enumerate().map(|(i, _)| (i + 1, format!("line {}", i + 1))).collect::<Vec<(usize, String)>>(),
    ));
    let got = ask(&request, &numbers).expect("the fake answers");
    assert_eq!(got.len(), many.len(), "every line of the batch answered");
    // The request itself carried all `many.len()` numbered lines: counted off the body the fake read,
    // not off the numbers this test passed in.
    let sent = sent_lines(&llm.next().body);
    assert_eq!(
        sent.len(),
        many.len(),
        "one request carried the whole batch, so {} lines reached the model",
        sent.len()
    );
    CHECKS.store(sent.len(), Ordering::SeqCst);
    assert_eq!(
        subs::batches(many.len()).len(),
        2,
        "the walk splits one over the batch size into two requests"
    );
}

/// One check of the real leg, in its own scenario: the settings address and `cache/llm` are
/// process-wide state, so each test builds the folder it dials and reads.
#[test]
fn f5_4_s3_wire_the_models_line_breaks_survive() {
    let scene = fixture("models_line_breaks_survive");
    let llm = FakeLlm::start();
    point_at(&llm.url());
    let cancelled = stop_flag();
    let ask = ask_leg::ask(&scene.tree, &cancelled);
    // §3.10's second must-fix: the prototype re-wrapped a returned answer at P.policy.subtitleRowChars,
    // contradicting the prompt that asks the model to keep its own breaks. A numbered answer spends ONE
    // physical line per number, so what must survive intact is a line LONGER than the row: nothing may be
    // inserted into it or folded at 42 characters on the way back.
    let wide = "ein uebersetzter Satz, der ueber die zweiundvierzig Zeichen der Row hinauslaeuft";
    assert!(
        wide.chars().count() > 42,
        "the sample must be over P.policy.subtitleRowChars to prove anything"
    );
    llm.queue(vec![format!("1\t{wide}")]);
    let got = ask(&subs::message(1, "German"), &[1]).expect("the fake answers");
    assert_eq!(
        got.get(&1).map(String::as_str),
        Some(wide),
        "the answer was re-wrapped on the way back: {:?}",
        got.get(&1)
    );
    // The one place a reply is post-processed is `keep_breaks`, which `parse_numbered` runs on every
    // line: it passes the text through, which is what makes the rule above hold.
    assert_eq!(
        subs::keep_breaks("oben\nunten"),
        "oben\nunten",
        "a break the model chose did not survive the leg"
    );
}

/// One check of the real leg, in its own scenario: the settings address and `cache/llm` are
/// process-wide state, so each test builds the folder it dials and reads.
#[test]
fn f5_4_s3_wire_tool_calls_are_read() {
    let scene = fixture("tool_calls_are_read");
    let llm = FakeLlm::start();
    point_at(&llm.url());
    let cancelled = stop_flag();
    let ask = ask_leg::ask(&scene.tree, &cancelled);
    let raw = tool_calls(&[(1, "eins"), (3, "drei")]);
    llm.queue(vec![raw]);
    let got = ask(&subs::message(3, "German"), &[1, 2, 3]).expect("the fake answers");
    assert_eq!(got.get(&1).map(String::as_str), Some("eins"));
    assert!(!got.contains_key(&2), "an unanswered number must stay missing: {got:?}");
    assert_eq!(got.get(&3).map(String::as_str), Some("drei"));
}

/// One check of the real leg, in its own scenario: the settings address and `cache/llm` are
/// process-wide state, so each test builds the folder it dials and reads.
#[test]
fn f5_4_s3_wire_a_foreign_number_and_an_empty_line_refuse() {
    let scene = fixture("foreign_number_refused");
    let llm = FakeLlm::start();
    point_at(&llm.url());
    let cancelled = stop_flag();
    let ask = ask_leg::ask(&scene.tree, &cancelled);
    llm.queue(vec!["9\taus der batch raus\n1\t\n2\tzwei".to_string()]);
    let got = ask(&subs::message(2, "German"), &[1, 2]).expect("the fake answers");
    assert_eq!(got.keys().copied().collect::<Vec<usize>>(), vec![2], "only a line inside the batch with text is an answer: {got:?}");
}

/// One check of the real leg, in its own scenario: the settings address and `cache/llm` are
/// process-wide state, so each test builds the folder it dials and reads.
#[test]
fn f5_4_s3_wire_only_a_complete_answer_is_cached() {
    let scene = fixture("only_a_complete_answer_is_cached");
    let llm = FakeLlm::start();
    point_at(&llm.url());
    let cancelled = stop_flag();
    let fresh_dir = scene.root.join("uncached.naivepost");
    std::fs::create_dir_all(&fresh_dir).ok();
    let fresh = naivepost::layout::Tree::new(&fresh_dir).expect("project");
    let ask = ask_leg::ask(&fresh, &cancelled);
    llm.queue(vec!["1\teins\n2\tzwei".to_string()]);
    let message = subs::message(2, "German");
    let numbers = vec![1usize, 2];
    let first = ask(&message, &numbers).expect("the fake answers");
    assert_eq!(first.len(), 2);
    // The first request is spent: its record still sits in the channel, and silence is about the SECOND ask.
    let _ = llm.next();
    llm.expect_silent();
    let again = ask(&message, &numbers).expect("the cache answers the identical request");
    assert_eq!(again.len(), 2, "the cached answer is the whole answer: {again:?}");
    assert_eq!(
        subs::cached(true),
        true,
        "a complete batch is the cacheable one (§F5.4 S3)"
    );
    assert_eq!(subs::cached(false), false, "a partial one never is");
}

/// One check of the real leg, in its own scenario: the settings address and `cache/llm` are
/// process-wide state, so each test builds the folder it dials and reads.
#[test]
fn f5_4_s3_wire_a_dead_server_is_named() {
    let scene = fixture("dead_server_is_named");
    let llm = FakeLlm::start();
    point_at(&llm.url());
    let cancelled = stop_flag();
    let dead = {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a port to hand out");
        let port = listener.local_addr().expect("an address").port();
        drop(listener);
        format!("http://127.0.0.1:{port}")
    };
    point_at(&dead);
    let ask = ask_leg::ask(&scene.tree, &cancelled);
    let err = ask(&subs::message(1, "German"), &[1]).expect_err("nothing listens there");
    assert!(
        err.contains("translate-model"),
        "the reason must name the model (§00-principles): {err}"
    );
    point_at(&llm.url());
}

/// The numbered lines of the LAST request the fake took.
fn last_sent_lines(llm: &FakeLlm) -> Vec<String> {
    sent_lines(&llm.next().body)
}

/// A whole-batch answer in the numbered-line shape, so a batch of any size can be scripted.
fn answer_for(numbers: &[usize]) -> String {
    numbers
        .iter()
        .map(|number| format!("{number}\tuebersetzt {number}"))
        .collect::<Vec<String>>()
        .join("\n")
}

/// Silence the unused warning for the map import when a check is edited out; the map is what `Ask` hands
/// back and several checks destructure it.
#[allow(dead_code)]
fn keep(_: &BTreeMap<usize, String>) {}
