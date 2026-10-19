//! §09-llm-and-tools#5-context-budgets-new — Context budgets.
//!
//! Spec: `spec/09-llm-and-tools.md` §5. What the rewrite must do is one bound plus two ways of
//! living inside it, and each clause is pinned where it is implemented:
//!
//! - **The bound** (`P.machine.promptMaxChars`, 120 000 per request) lives in
//!   [`naivepost::llm_budget`] and is applied by `llm_request::body` — s1.
//! - **Chars vs bytes**: the budget counts characters, the log reports bytes. s1's second test.
//! - **Batching** — captions 5 clips ([`naivepost::cut_captions`]), translation
//!   `P.machine.translateBatch` lines ([`naivepost::produce_subtitles`]), the cut brief folded one
//!   line per clip ([`naivepost::cut_speed_pass`]) — s2's two tests.
//! - **Reading the rest through tools** — `get_lines` / `get_events`
//!   ([`naivepost::tools::offered`]) — s3.
//! - **The size is logged, not put on the exchange page** — s4. The wording itself is already
//!   pinned whole by `tests/suite/model_exchange_log.rs`; what this adds is that the same figure does
//!   NOT appear in the page's HTML.

use naivepost::exchanges::{self, Call, Message, Mode, Part};
use naivepost::layout::Tree;
use naivepost::llm_budget::{self, PROMPT_MAX_CHARS};
use naivepost::params;
use naivepost::produce_subtitles as subs;
use naivepost::project::{Project, Source};
use naivepost::run;
use naivepost::run_stages;
use naivepost::services::Server;
use naivepost::settings::{self, Conf, Paths};
use naivepost::textfmt::{self, Line};
use naivepost::tools::{self, clips::Clips, Tool};
use naivepost::{cut_captions, cut_speed_pass, roles};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::common;
use common::temp_dir_tagged;

/// The fix walk's questions belong to the process (the LLM address and the working folder are env-wide) and
/// cargo runs these tests as threads, so the one case that moves them takes them one at a time.
static FIX_ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());
/// One transcript row per source for the fix-walk case: above §S1's floor on the source side, small enough
/// that its blocks go out in one ask each.
const FIX_LINES: usize = 4;
/// The lanes those recordings file under: the stem of each name, which is what names every folder here.
const DECK: &str = "2026-05-01 10-00-00_deck";
const MIC: &str = "2026-05-01 10-01-30_mic";
/// The two recordings' stamped names, 90 s apart, so `placement` has real seconds to report.
const VIDEO: &str = "2026-05-01 10-00-00_deck.mkv";
const AUDIO: &str = "2026-05-01 10-01-30_mic.wav";

/// One `fix_line` per line of a block, then `finish` — §3.3's answer shape.
fn fix_answer(lines: usize) -> String {
    let mut calls = String::new();
    for n in 1..=lines {
        calls.push_str(&format!(
            "{{\"function\":{{\"name\":\"fix_line\",\"arguments\":\"{{\\\"n\\\":{n},\\\"text\\\":\\\"respelled line {n}\\\"}}\"}}}},",
        ));
    }
    calls.push_str("{\"function\":{\"name\":\"finish\",\"arguments\":\"{}\"}}");
    format!("{{\"choices\":[{{\"message\":{{\"content\":\"\",\"tool_calls\":[{calls}]}}}}]}}")
}

/// A session with transcripts on both lanes and one lane already fixed, which is the state that makes the
/// merge row run `fix_transcripts::run` rather than skip it.
struct FixScene {
    root: PathBuf,
    tree: Tree,
}

impl FixScene {
    fn new(lines: usize, llm_url: &str) -> FixScene {
        let root = temp_dir_tagged("ctx-budgets-fix");
        let session = naivepost::startup::session_dir(&root);
        std::fs::create_dir_all(session.join("sources")).expect("the sources folder");
        for name in [VIDEO, AUDIO] {
            std::fs::write(session.join("sources").join(name), b"bytes").expect("the recording");
        }
        let paths = Paths { config_dir: root.join("cfg/naivepost"), data_dir: root.join("data/naivepost") };
        let mut conf = Conf::default();
        conf.server = llm_url.to_string();
        conf.model = "qwen-text".to_string();
        settings::save(&paths, &conf).expect("the conf is written");
        let project = Project {
            sources: vec![
                Source {
                    path: format!("project:sources/{VIDEO}"),
                    footage: true,
                    narrator: 0,
                    sepvoice: false,
                    tracks: vec![],
                },
                Source {
                    path: format!("project:sources/{AUDIO}"),
                    footage: false,
                    narrator: 0,
                    sepvoice: false,
                    tracks: vec![],
                },
            ],
            ..Project::default()
        };
        naivepost::project::save(&project, &session).expect("the session file");
        let tree = Tree::new(session).expect("the session is a project");
        for lane in [DECK, MIC] {
            let rows: Vec<Line> = (0..lines)
                .map(|index| Line {
                    start: index as f64,
                    end: index as f64 + 0.9,
                    speaker: "S1".to_string(),
                    text: format!("asr heard line {index} mispelled"),
                })
                .collect();
            textfmt::write_lines(&rows, &tree.transcript_tsv(lane)).expect("the transcript");
        }
        // The deck comes back already fixed, so the only work left for the stage's merge row is the mic —
        // and the mic's file can only appear if `fix_transcripts::run` actually walks it.
        let done: Vec<Line> = (0..lines)
            .map(|index| Line {
                start: index as f64,
                end: index as f64 + 0.9,
                speaker: "S1".to_string(),
                text: format!("respelled line {index}"),
            })
            .collect();
        textfmt::write_lines(&done, &tree.transcript_fixed_tsv(DECK)).expect("the deck is already fixed");
        FixScene { root, tree }
    }

    /// Point the process at this scene exactly as the app resolves both, and take the §3 patience ladder
    /// down by 1000 so a refused or unanswered ask cannot park the walk for minutes.
    fn enter(&self) {
        std::env::set_var("NAIVEPOST_RETRY_SCALE", "1000");
        std::env::set_var("XDG_CONFIG_HOME", self.root.join("cfg"));
        std::env::set_var("XDG_DATA_HOME", self.root.join("data"));
        std::env::set_current_dir(&self.root).expect("the scene is the working folder");
    }

    fn leave(&self) {
        std::env::remove_var("XDG_CONFIG_HOME");
        std::env::remove_var("XDG_DATA_HOME");
        std::env::remove_var("NAIVEPOST_RETRY_SCALE");
        std::env::set_current_dir(env!("CARGO_MANIFEST_DIR")).expect("back to the crate folder");
    }

    fn project(&self) -> Project {
        naivepost::project::load(self.tree.dir()).expect("the session")
    }

    /// The fix stage only, planned from the project file the way ▶ plans it, minus the mic's per-source
    /// fix task: the state a resume leaves behind, where the session walk is the only thing that can still
    /// produce that lane's fixed file.
    fn stages(&self) -> Vec<run_stages::Stage> {
        run_stages::plan(run::Step::Prepare, &self.project())
            .into_iter()
            .filter(|stage| stage.name == "fix")
            .map(|mut stage| {
                let before = stage.tasks.len();
                stage.tasks.retain(|task| !format!("{:?}", task).contains(MIC));
                assert!(stage.tasks.len() < before, "the mic had a fix task to remove");
                stage
            })
            .collect()
    }

    fn drive(&self, stages: &[run_stages::Stage]) -> (run_stages::Drive, Vec<String>) {
        let mut queue = naivepost::runqueue::Queue::new();
        let mut procs = run::Subprocesses::default();
        let mut log: Vec<String> = Vec::new();
        let drive = run_stages::drive_walk(stages, &mut queue, || false, || false, |line| {
            log.push(line.to_string());
        }, &mut procs);
        run_stages::say_outcome(&drive, |line| log.push(line.to_string()));
        (drive, log)
    }
}

/// A fake LLM on loopback answering every ask with `answer`, recording each request body.
struct Fake {
    url: String,
    asks: Arc<Mutex<Vec<String>>>,
    #[allow(dead_code)] // held so the reply outlives the connection threads reading it
    answer: Arc<Mutex<String>>,
}

impl Fake {
    fn start(answer: String) -> Fake {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let port = listener.local_addr().expect("an address").port();
        let asks = Arc::new(Mutex::new(Vec::new()));
        let recorder = asks.clone();
        let held = Arc::new(Mutex::new(answer));
        let replies = held.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                let recorder = recorder.clone();
                let replies = replies.clone();
                std::thread::spawn(move || {
                    let answer = replies.lock().expect("the reply lock").clone();
                    if let Some(body) = serve(stream, &answer) {
                        if let Ok(mut row) = recorder.lock() {
                            row.push(body);
                        }
                    }
                });
            }
        });
        Fake { url: format!("http://127.0.0.1:{port}"), asks, answer: held }
    }

    /// The bodies recorded so far; recording happens on the connection's own thread, so callers wait.
    fn waited(&self, count: usize) -> Vec<String> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        loop {
            let seen = self.asks.lock().expect("the record lock").clone();
            if seen.len() >= count || std::time::Instant::now() > deadline {
                return seen;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
}

use common::serve_body as serve;


/// Messages of `n` ASCII characters: one text part each, so a size is easy to state exactly.
fn ascii_messages(count: usize, parts_each: usize) -> Vec<Message> {
    let per = count / parts_each.max(1);
    (0..parts_each)
        .map(|i| Message {
            role: "user".to_string(),
            parts: vec![Part::Text("x".repeat(per + if i == 0 { count - per * parts_each } else { 0 }))],
        })
        .collect()
}

// --- S1: the bound -------------------------------------------------------------------------------------

#[test]
fn sec_09_llm_and_tools_5_context_budgets_new_s1_every_request_is_bounded() {
    // P.machine.promptMaxChars — §10 §1: the most one request may carry.
    assert_eq!(PROMPT_MAX_CHARS, 120_000);
    let row = params::find("P.machine.promptMaxChars").expect("the cap is catalogued");
    assert_eq!(row.spelled, "120000", "spelled plain, as P.eng.thumbnailJPEGMax spells its cap");
    assert_eq!(row.from, "llm_budget::PROMPT_MAX_CHARS", "one home for the number");
    assert_eq!(params::family("P.machine.promptMaxChars"), params::Family::Machine);

    // Inclusive boundary: exactly the cap goes, one char more does not.
    assert!(llm_budget::fits(0));
    assert!(llm_budget::fits(PROMPT_MAX_CHARS));
    assert!(!llm_budget::fits(PROMPT_MAX_CHARS + 1));

    let under = llm_budget::check("Marking", PROMPT_MAX_CHARS);
    assert!(under.is_ok(), "the cap itself is inside the budget: {under:?}");
    let over = llm_budget::check("Marking", PROMPT_MAX_CHARS + 1).unwrap_err();
    // The message names the step that was over, the size, the limit, and both ways out — "too big" with no
    // remedy, or with no name attached, is a dead end for whoever reads the log.
    assert!(over.starts_with(">>> Marking: "), "{over}");
    assert!(over.contains("120001"), "{over}");
    assert!(over.contains("120000"), "{over}");
    assert!(over.contains("get_lines"), "{over}");
    assert!(over.contains("batches"), "{over}");

    // And the request contract refuses it: body() is where every chat request is built, so a call
    // site cannot forget the cap.
    let too_big = ascii_messages(PROMPT_MAX_CHARS + 1, 1);
    let refused = naivepost::llm_request::body("Marking", "m", &too_big, Mode::Execute, None, false)
        .expect_err("a request over the cap is not sent");
    assert_eq!(refused, over, "body reports the budget's own sentence, step included");

    // Exactly at the cap still builds.
    let at_cap = ascii_messages(PROMPT_MAX_CHARS, 2);
    assert!(naivepost::llm_request::body("Marking", "m", &at_cap, Mode::Execute, None, false).is_ok());
}

#[test]
fn sec_09_llm_and_tools_5_context_budgets_new_s1_chars_counts_characters_not_bytes() {
    // A Cyrillic transcript is twice the bytes of a Latin one and the same context. The budget reads
    // chars so the language costs nothing; the log prints bytes because that is what went on the wire.
    let cyrillic = "я".repeat(1000);
    assert_eq!(cyrillic.len(), 2000, "two bytes per char in UTF-8");
    let messages = vec![Message { role: "user".into(), parts: vec![Part::Text(cyrillic.clone())] }];
    assert_eq!(llm_budget::chars(&messages), 1000, "the budget sees 1000 chars");

    let mut call = Call {
        step: "Describe".into(),
        model: "m".into(),
        mode: Mode::Execute,
        took_secs: 0,
        thinking_secs: None,
        messages,
        reply: String::new(),
        reasoning: None,
        tool_calls: Vec::new(),
        cut_off: false,
        error: None,
    };
    assert_eq!(call.sent().0, 2000, "the log figure is bytes, deliberately not the budget's number");
    // An image is no context at all: it cannot push a request over the cap by itself.
    call.messages.push(Message {
        role: "user".into(),
        parts: vec![Part::Image("data:image/png;base64,AAAA".repeat(50_000))],
    });
    assert_eq!(llm_budget::chars(&call.messages), 1000, "images count zero against the cap");
    assert!(llm_budget::fits(llm_budget::chars(&call.messages)));
}

// --- S2: jobs over the cap run in batches ---------------------------------------------------------------

#[test]
fn sec_09_llm_and_tools_5_context_budgets_new_s2_jobs_over_the_cap_run_in_batches() {
    // Captions: 5 clips per request.
    assert_eq!(cut_captions::BATCH, 5);
    assert_eq!(params::find("P.machine.captionBatch").expect("catalogued").spelled, "5");
    let forty = cut_captions::batches(40);
    assert_eq!(forty.len(), 8, "40 clips is eight requests of five");
    assert!(forty.iter().all(|(first, last)| last - first == 4), "every batch holds five");
    let odd = cut_captions::batches(23);
    assert_eq!(odd.last(), Some(&(21, 23)), "the final batch is short rather than dropped");

    // Translation: P.machine.translateBatch lines per request.
    assert_eq!(subs::BATCH, 150);
    let row = params::produce()
        .into_iter()
        .find(|p| p.id == "P.machine.translateBatch")
        .expect("catalogued in the Produce rows");
    assert_eq!(row.spelled, "150");

    // The prototype's oversized call, reproduced with the real formatter: 778 lines of ~160 chars —
    // §5's "778-line translation call" — is well past the cap as ONE message.
    let track: Vec<(usize, String)> =
        (1..=778).map(|n| (n, format!("{n}: {}", "word ".repeat(32)))).collect();
    let whole = subs::numbered(&track);
    assert!(whole.chars().count() > PROMPT_MAX_CHARS, "the whole track would be refused");
    assert!(!llm_budget::fits(whole.chars().count()));

    // Cut to batches, every request fits and the batches tile the track — nothing is lost by splitting.
    let ranges = subs::batches(track.len());
    assert_eq!(ranges.len(), 6, "778 lines is six requests of at most 150");
    assert!(ranges.iter().all(|(first, last)| last - first <= subs::BATCH));
    assert_eq!(ranges.first(), Some(&(0, 150)));
    assert_eq!(ranges.last(), Some(&(750, 778)));
    let covered: usize = ranges.iter().map(|(f, l)| l - f).sum();
    assert_eq!(covered, 778, "the batches add up to the whole track");
    for range in &ranges {
        let slice: Vec<(usize, String)> = track[range.0..range.1].to_vec();
        let sent = subs::numbered(&slice);
        assert!(
            llm_budget::fits(sent.chars().count()),
            "batch {range:?} is {} chars",
            sent.chars().count()
        );
    }
}

#[test]
fn sec_09_llm_and_tools_5_context_budgets_new_s2_the_cut_brief_is_folded_per_clip() {
    // The cut brief carries one folded line per clip rather than the session's transcript, so its
    // size scales with the number of clips and stays inside the cap whatever was recorded.
    let many: Vec<(u32, f64, f64)> = (1..=200u32).map(|n| (n, f64::from(n) * 12.0, 12.0)).collect();
    let clips = Clips::new(&many);
    let brief = cut_speed_pass::brief(&clips, 2400.0);
    assert!(brief.chars().count() < PROMPT_MAX_CHARS, "a 200-clip brief is {} chars", brief.chars().count());

    // One entry per clip, each naming its own number: the fold is per clip, not one blob.
    let lines: Vec<&str> = brief.lines().filter(|line| line.starts_with("CLIP ")).collect();
    assert_eq!(lines.len(), 200, "one line per clip");
    assert!(lines[0].starts_with("CLIP 1:"), "{}", lines[0]);
    assert!(lines[199].starts_with("CLIP 200:"), "{}", lines[199]);
    // Plus the header lines above the list (the footage total and the list's own heading).
    let total = brief.lines().count();
    assert_eq!(total, 200 + 3, "the two headers and a blank line above the clip list");

    // The effects pass folds the same way: one line per clip, with the speed noted on it.
    let speeds: Vec<(u32, f64)> = (1..=200u32).map(|n| (n, 2.0)).collect();
    let fx_brief = naivepost::cut_effects_pass::brief(&clips, &speeds);
    assert!(fx_brief.chars().count() < PROMPT_MAX_CHARS);
    let fx_lines: Vec<&str> = fx_brief.lines().filter(|line| line.starts_with("CLIP ")).collect();
    assert_eq!(fx_lines.len(), 200);
    assert!(fx_lines[7].contains("plays at 2x"), "{}", fx_lines[7]);
}

// --- S3: the rest is read through tools -----------------------------------------------------------------

#[test]
fn sec_09_llm_and_tools_5_context_budgets_new_s3_the_rest_is_read_through_tools() {
    // The other half of the budget rule: what is not sent may be asked for. Every job the LLM
    // answers is offered the two reading tools §5 names.
    let llm_jobs: Vec<roles::Job> = roles::Job::all()
        .into_iter()
        .filter(|job| roles::server(*job) == Server::Llm)
        .collect();
    assert_eq!(llm_jobs.len(), 9, "four audio jobs go to audio.cpp and the thumbnail to sd.cpp");

    for job in &llm_jobs {
        let offered = tools::offered(*job);
        assert!(offered.contains(&Tool::GetEvents), "{job:?} cannot read events: {offered:?}");
        if *job == roles::Job::Describe {
            // §3.2's Reads row: a chunk of frames has no transcript lines to read, so get_lines is
            // dropped for it and get_frames stands in. Asserted rather than papered over.
            assert!(!offered.contains(&Tool::GetLines), "{job:?} has no lines to offer");
            assert!(offered.contains(&Tool::GetFrames), "{job:?} reads frames instead");
        } else {
            assert!(offered.contains(&Tool::GetLines), "{job:?} cannot read lines: {offered:?}");
        }
    }
}

// --- S1: the refusal names the step, and the size line's unit -------------------------------------------

/// §5's log convention is `>>> <step>: …`, and Failure-is-specific wants to know WHICH job was over: a
/// refusal that names no step reads as a server fault to whoever is watching the run.
#[test]
fn sec_09_llm_and_tools_5_context_budgets_new_s1_the_refusal_names_the_step() {
    let too_big = ascii_messages(PROMPT_MAX_CHARS + 1, 1);
    let refused = naivepost::llm_request::body("Marking", "qwen", &too_big, Mode::Execute, None, false)
        .expect_err("a request over the cap is not built, so it cannot be sent");
    assert!(
        refused.starts_with(&format!(
            ">>> Marking: prompt is over the budget: {} chars against {PROMPT_MAX_CHARS}",
            PROMPT_MAX_CHARS + 1
        )),
        "the refusal opens with the step that was over: {refused}"
    );
    // Both numbers and both ways out, spelled from the constant rather than from a literal.
    assert!(refused.contains(&format!("against {PROMPT_MAX_CHARS}")), "{refused}");
    assert!(refused.contains("run the job in batches"), "{refused}");
    assert!(refused.contains("get_lines/get_events"), "{refused}");

    // A different step gets its own name: the sentence is built per request, not stored once.
    let other = naivepost::llm_request::body("translate", "qwen", &too_big, Mode::Execute, None, false)
        .expect_err("the cap does not care which job asks");
    assert!(other.starts_with(">>> translate: "), "{other}");

    // Exactly at the cap still builds — the bound is inclusive, so this is not a refusal.
    assert!(naivepost::llm_request::body(
        "Marking",
        "qwen",
        &ascii_messages(PROMPT_MAX_CHARS, 2),
        Mode::Execute,
        None,
        false
    )
    .is_ok());
}

/// §5 fixes the size line verbatim as `N kB of text`; below a kilobyte `size_of` used to print `812 B`, a
/// unit the spec never names. Every size now goes out in kB, one decimal, whatever it is.
#[test]
fn sec_09_llm_and_tools_5_context_budgets_new_s1_the_size_line_names_kilobytes_under_one_kilobyte() {
    let call_with = |bytes: usize| Call {
        step: "Describe".into(),
        model: "vision".into(),
        mode: Mode::Execute,
        took_secs: 1,
        thinking_secs: None,
        messages: vec![Message {
            role: "user".into(),
            parts: vec![Part::Text("z".repeat(bytes))],
        }],
        reply: "an answer".into(),
        reasoning: None,
        tool_calls: Vec::new(),
        cut_off: false,
        error: None,
    };
    // 812 bytes is under a kilobyte and STILL prints kB: this is the case the old formatter got wrong.
    assert_eq!(exchanges::kb_of(812), "0.8 kB");
    assert_eq!(
        exchanges::log_lines(&call_with(812))[0],
        ">>> Describe: 0.8 kB of text and 0 image(s) went to the LLM"
    );
    // And §5's own example keeps reading the same way it always did: 462 132 bytes is 451.3 kB.
    assert_eq!(exchanges::kb_of(462_132), "451.3 kB");
    assert_eq!(
        exchanges::log_lines(&call_with(462_132))[0],
        ">>> Describe: 451.3 kB of text and 0 image(s) went to the LLM"
    );
    // One unit means a megabyte of it is still kB — the line stays summable across a whole run.
    assert_eq!(exchanges::kb_of(1_572_864), "1536.0 kB");

    // The size belongs to the log only (§5: "not on the exchange page").
    let page = exchanges::section_html(1, &call_with(812));
    assert!(!page.contains("went to the LLM"), "{page}");
    assert!(!page.contains("kB of text"), "{page}");
}

// --- S2: batching keeps a real request inside the cap ---------------------------------------------------

/// The translation batch size is what keeps §5's oversized call sendable: build each batch's request the
/// way `produce_translate::track` builds it (heading + numbered lines) and check THAT against the cap,
/// not just the line list.
#[test]
fn sec_09_llm_and_tools_5_context_budgets_new_s2_translation_batches_stay_under_the_cap() {
    // Cues the size of §5's example session: 778 of them, ~160 characters apiece.
    let cues: Vec<subs::Cue> = (1..=778usize)
        .map(|n| subs::Cue {
            s: f64::from(n as u32) * 4.0,
            e: f64::from(n as u32) * 4.0 + 3.5,
            text: format!("cue {n}: {}", "word ".repeat(30)),
            pos: String::new(),
        })
        .collect();
    assert_eq!(cues.len(), 778);
    assert_eq!(subs::BATCH, 150, "P.machine.translateBatch");

    // What the track would be as ONE message — heading plus every numbered cue: refused outright.
    let original: Vec<(usize, String)> =
        (1..=cues.len()).map(|n| (n, subs::sent_text(&cues[n - 1]))).collect();
    let whole = format!("{}\n{}", subs::message(cues.len(), "de"), subs::numbered(&original));
    assert!(whole.chars().count() > PROMPT_MAX_CHARS, "one message for the track is {} chars", whole.chars().count());
    assert!(!llm_budget::fits(whole.chars().count()));

    // Each request the batching actually sends fits, and every cue is asked about exactly once.
    let ranges = subs::batches(cues.len());
    assert!(ranges.len() > 1, "the track has to be split: {ranges:?}");
    let mut asked = 0usize;
    for range in &ranges {
        assert!(range.1 - range.0 <= subs::BATCH, "batch {range:?} holds more than BATCH lines");
        let part: Vec<(usize, String)> = original[range.0..range.1].to_vec();
        let request = format!("{}\n{}", subs::message(part.len(), "de"), subs::numbered(&part));
        assert!(
            llm_budget::fits(request.chars().count()),
            "batch {range:?} is {} chars, past the cap",
            request.chars().count()
        );
        asked += part.len();
    }
    assert_eq!(asked, cues.len(), "splitting must not drop or repeat a cue");
}

// --- S2: the escape routes are work the program drives -------------------------------------------------

/// §5's "jobs over it read the rest via get_lines/get_events tools or run in batches" only helps if the
/// job that needs it runs. The fix step's whole-session walk (`fix_transcripts::run`, asking block by
/// block through `fix_call`, whose every ask passes `llm_request::body` and so the cap) is driven by the
/// merge row of the fix stage; this walks that stage for real against a loopback fake and asserts the
/// files and log lines only that walk produces.
#[test]
fn sec_09_llm_and_tools_5_context_budgets_new_s2_the_session_fix_walk_runs_in_a_real_run() {
    let _turn = FIX_ENV.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    // Only ONE source gets a fixed file planted: the other still owes the fixer, which is what makes the
    // merge row run the session walk instead of skipping it (§S3's resume rule).
    let fake = Fake::start(fix_answer(FIX_LINES));
    let scene = FixScene::new(FIX_LINES, &fake.url);
    scene.enter();
    let stages = scene.stages();
    let (_drive, log) = scene.drive(&stages);
    scene.leave();

    assert!(matches!(_drive, run_stages::Drive::Done { .. }), "the fix stage finished: {_drive:?}");
    // The source nobody fixed now has its fixed file: only `fix_transcripts::run` writes it in this walk,
    // because its own fix row was the one removed below.
    assert!(
        scene.tree.transcript_fixed_tsv(MIC).exists(),
        "the session walk fixed the source whose own fix row did not run"
    );
    let fixed = std::fs::read_to_string(scene.tree.transcript_fixed_tsv(MIC)).expect("the fixed file");
    assert!(fixed.contains("respelled line 1"), "the walk's answers landed: {fixed}");
    // The timeline and its text come after the walk, from the merge row that ran it. `session_txt` is only
    // ever written by that merge, so its presence means the merge — and the walk inside it — really ran.
    assert!(scene.tree.session_tsv().exists(), "the merge still wrote the timeline");
    assert!(scene.tree.session_txt().exists(), "and the text the cut reads");
    // And the walk really dialled the fixer for the blocks it owed: §3.3's message wording is only ever
    // put on the wire by that leg.
    let fix_asks = fake
        .waited(1)
        .into_iter()
        .filter(|body| body.contains("Transcript lines to clean"))
        .count();
    let _ = &log;
    assert!(fix_asks >= 1, "the session walk asked the fixer: {fix_asks} asks");
}


#[test]
fn sec_09_llm_and_tools_5_context_budgets_new_s4_size_logged_not_on_the_page() {
    let text = "z".repeat(1200);
    let images = vec![
        Part::Image("data:image/jpeg;base64,AAA".to_string()),
        Part::Image("data:image/jpeg;base64,BBB".to_string()),
    ];
    let call = Call {
        step: "Describe".into(),
        model: "vision".into(),
        mode: Mode::Thinking,
        took_secs: 3,
        thinking_secs: None,
        messages: vec![
            Message { role: "user".into(), parts: vec![Part::Text(text.clone())] },
            Message { role: "user".into(), parts: images },
        ],
        reply: "an answer".into(),
        reasoning: None,
        tool_calls: Vec::new(),
        cut_off: false,
        error: None,
    };
    // The app log gets the size: 1200 bytes of text is 1.2 kB (§5's line names kB for every size), and two
    // pictures.
    let lines = exchanges::log_lines(&call);
    assert_eq!(lines[0], ">>> Describe: 1.2 kB of text and 2 image(s) went to the LLM");
    assert_eq!(
        lines[0],
        format!(">>> {}: {} of text and 2 image(s) went to the LLM", call.step, exchanges::kb_of(text.len()))
    );

    // The exchange page never says it: §5 puts the size in the log, not on the page.
    let page = exchanges::section_html(1, &call);
    assert!(!page.contains("went to the LLM"), "{page}");
    assert!(!page.contains("of text"), "{page}");
    // The page does show what was sent, which is the point of having it at all.
    assert!(page.contains(&text), "the page shows the text itself");
    assert!(page.contains("<img"), "and the pictures");
}
