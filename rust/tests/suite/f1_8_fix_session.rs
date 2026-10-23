//! F1.8 Fix the transcripts — the session halves, walked as ▶ walks them, spec/04-prepare.md.
//!
//! `tests/suite/fix_transcripts.rs` proves [`naivepost::fix_transcripts`]'s rules with closures and no
//! server, and `tests/suite/fix_call.rs` proves one source's request leg through `fix_call::stage_chunk`.
//! Neither could see the two rows this round added to the same stage: S1's placement (offsets before any
//! block goes out) and S5–S7's merge (session.tsv, the marking pass, session.txt). This file drives
//! `run_stages::plan(Step::Prepare)` filtered to the `fix` stage through `run_stages::drive_walk`, so
//! what is asserted is the files a run leaves behind — read off the tree, never from a plan line.
//!
//! The parameter the flow runs on is pinned after its token too: `P.machine.fixBlockLines`.

use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use crate::common;
use common::temp_dir_tagged;
use naivepost::layout::Tree;
use naivepost::project::{MarkingPass, Origin, Project, Source};
use naivepost::settings::{self, Conf, Paths};
use naivepost::textfmt::{self, FrameEvent, Line, Retake};
use naivepost::{fix_transcripts, params, run, run_stages};

/// The env and the working folder belong to the PROCESS and cargo runs these tests as threads, so the
/// tests that move them take them one at a time and put them back before asserting.
static ENV: Mutex<()> = Mutex::new(());
static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// The two recordings' stamped names, 90 s apart, so `placement` has real seconds to report and
/// `offsets.tsv` has a non-zero row to hold.
const VIDEO: &str = "2026-05-01 10-00-00_deck.mkv";
const AUDIO: &str = "2026-05-01 10-01-30_mic.wav";
/// The lanes those names file under: the stem of each name, which is what names every folder here.
const DECK: &str = "2026-05-01 10-00-00_deck";
const MIC: &str = "2026-05-01 10-01-30_mic";

/// One `fix_line` per line of a block, then `finish` — §3.3's answer shape, built for however many lines
/// the block actually holds (the last block of thirty lines at twenty-five a block is a block of FIVE).
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

/// An answer short by one line: `fix_block` refuses it ("n in the block, no tab?"). Kept for the
/// validation rule's own tests, which live in `tests/suite/fix_transcripts.rs`; this file's S3 case goes
/// through the wire instead, where a refusal never arrives because nothing answers.
#[allow(dead_code)]
fn short_answer(lines: usize) -> String {
    fix_answer(lines.saturating_sub(1))
}

/// A fake LLM answering every chat ask with the held reply, recording how many asks arrived.
struct Fake {
    url: String,
    asks: std::sync::Arc<Mutex<Vec<ChatAsk>>>,
    /// Held so the reply outlives the connection threads that read it; no test here changes it.
    #[allow(dead_code)]
    answer: std::sync::Arc<Mutex<String>>,
}

use common::{serve_chat as serve, ChatAsk};

impl Fake {
    fn start(answer: String) -> Fake {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let port = listener.local_addr().expect("an address").port();
        let asks = std::sync::Arc::new(Mutex::new(Vec::new()));
        let recorder = asks.clone();
        let held = std::sync::Arc::new(Mutex::new(answer));
        let replies = held.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                let recorder = recorder.clone();
                let replies = replies.clone();
                std::thread::spawn(move || {
                    let answer = replies.lock().expect("the reply lock").clone();
                    if let Some(ask) = serve(stream, &answer) {
                        if let Ok(mut row) = recorder.lock() {
                            row.push(ask);
                        }
                    }
                });
            }
        });
        Fake {
            url: format!("http://127.0.0.1:{port}"),
            asks,
            answer: held,
        }
    }

    /// The asks recorded so far; recording happens on the connection's own thread, so callers wait.
    fn waited(&self, count: usize) -> Vec<ChatAsk> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            let seen = self.asks.lock().expect("the record lock").clone();
            if seen.len() >= count || std::time::Instant::now() > deadline {
                return seen;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    /// How many asks arrived, without waiting.
    #[allow(dead_code)]
    fn count(&self) -> usize {
        self.asks.lock().expect("the record lock").len()
    }

    /// Change what every later ask is answered with — the retry case answers the first ask badly.
    #[allow(dead_code)]
    fn set_answer(&self, answer: String) {
        *self.answer.lock().expect("the reply lock") = answer;
    }
}

/// The §3 patience ladder divided by 1000 (5 s → 5 ms). A walk that dials a server which never answers
/// sits through the whole ladder on every ask, which is minutes of sleeping inside a test whose subject
/// is the FILE the walk writes afterwards. `llm_retry_leg::nap_duration` reads this from the process env,
/// and the env is exactly what this file's `ENV` guard already serialises — so every walk here takes it
/// through `enter`, and no case can hang for four minutes on a wait nobody is testing.
fn scale_the_waits() {
    std::env::set_var("NAIVEPOST_RETRY_SCALE", "1000");
}

/// One scene: a session whose project holds the deck and the microphone, transcripts planted for both,
/// events for the deck, and a settings folder naming its own fake LLM.
struct Scene {
    root: PathBuf,
    /// The scene's own settings folder, kept so a test could re-point the conf if it needed to.
    #[allow(dead_code)]
    paths: Paths,
    tree: Tree,
}

impl Scene {
    /// `lines` transcript rows per source, spaced a second apart from each source's own second nought.
    fn new(lines: usize, llm_url: &str) -> Scene {
        let root = temp_dir_tagged(&format!(
            "f18-run-{}",
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        let session = naivepost::startup::session_dir(&root);
        std::fs::create_dir_all(session.join("sources")).expect("the sources folder");
        for name in [VIDEO, AUDIO] {
            std::fs::write(session.join("sources").join(name), b"bytes").expect("the recording");
        }
        let paths = Paths {
            config_dir: root.join("cfg/naivepost"),
            data_dir: root.join("data/naivepost"),
        };
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
        // The deck's event log: two picture events, which become EVENT rows on the timeline (S5).
        let events = vec![
            FrameEvent {
                start: 1.0,
                end: 4.0,
                text: "a slide appears".to_string(),
            },
            FrameEvent {
                start: 6.0,
                end: 8.0,
                text: "the speaker writes".to_string(),
            },
        ];
        textfmt::write_events(&events, &tree.events_tsv(DECK)).expect("events.tsv");
        Scene { root, paths, tree }
    }

    /// Name the microphone the narrator's row: its words go to `commentary.fixed.tsv` (S4) and its lane
    /// is the label `session.txt` prints (§S7's narrator).
    fn make_narrator(&mut self) {
        let mut project = self.project();
        project.sources[1].narrator = 1;
        naivepost::project::save(&project, self.tree.dir()).expect("the session saves");
    }

    /// Plant marks by hand — `retakes.tsv` is where both marking passes leave their stretches, and this
    /// is how a case puts one there without a model proposing it. A walk under a NAMED retake pass
    /// overwrites the file with whatever that pass answers, so only a `none`-policy walk keeps them.
    fn plant_marks(&self, marks: &[Retake]) {
        textfmt::write_retakes(marks, &self.tree.retakes_tsv()).expect("retakes.tsv");
    }

    fn project(&self) -> Project {
        naivepost::project::load(self.tree.dir()).expect("the session")
    }

    /// Point the process at this scene exactly as the app resolves both.
    fn enter(&self) {
        scale_the_waits();
        std::env::set_var("XDG_CONFIG_HOME", self.root.join("cfg"));
        std::env::set_var("XDG_DATA_HOME", self.root.join("data"));
        std::env::set_current_dir(&self.root).expect("the scene is the working folder");
    }

    fn leave(&self) {
        std::env::remove_var("XDG_CONFIG_HOME");
        std::env::remove_var("XDG_DATA_HOME");
        std::env::set_current_dir(env!("CARGO_MANIFEST_DIR")).expect("back to the crate folder");
    }

    /// The fix stage only, planned from the project file the way ▶ plans it: placement row, one row per
    /// source, merge row — the order §F1.8's flowchart draws.
    fn stages(&self) -> Vec<run_stages::Stage> {
        self.stages_named(MarkingPass::None)
    }

    /// The fix stage under a named marking policy, planned the way ▶ plans it: placement row, one row per
    /// source, merge row — the order §F1.8's flowchart draws.
    fn stages_named(&self, pass: MarkingPass) -> Vec<run_stages::Stage> {
        let mut project = self.project();
        project.policy.marking_pass = naivepost::project::Field {
            value: pass,
            origin: Origin::User,
            because: None,
        };
        naivepost::project::save(&project, self.tree.dir()).expect("the pass is set");
        run_stages::plan(run::Step::Prepare, &project)
            .into_iter()
            .filter(|stage| stage.name == "fix")
            .collect()
    }

    /// Walk it, naming the marking pass the policy should carry (§P.policy.markingPass).
    ///
    /// NO ENV guard here or in [`Scene::walk`]: the env is one process-wide resource and the guard belongs
    /// to the FREE helpers below, which take it exactly once per test. A method that locked it too would
    /// re-lock a `std::sync::Mutex` the same thread already holds — which blocks forever, not an error.
    fn walk_in(&self, pass: MarkingPass) -> (run_stages::Drive, Vec<String>) {
        self.enter();
        // The pass and its inputs must be SEEN by the run being walked: `plan` reads the project it is
        // handed, and a stage list built before the pass was named would carry the old policy.
        let stages = self.stages_named(pass);
        let drive = self.drive(&stages);
        self.leave();
        drive
    }

    /// Walk it under no marking policy: these cases pin the fix rows and the merge, and the policy stays
    /// `none`, which keeps the retake job's THREE pooled runs off a fake that only answers the fixer.
    fn walk(&self) -> (run_stages::Drive, Vec<String>) {
        self.enter();
        let stages = self.stages();
        let drive = self.drive(&stages);
        self.leave();
        drive
    }

    /// One walk of the given stages, collecting every line it speaks. Shared by both walkers above.
    fn drive(&self, stages: &[run_stages::Stage]) -> (run_stages::Drive, Vec<String>) {
        let mut queue = naivepost::runqueue::Queue::new();
        let mut procs = run::Subprocesses::default();
        let mut log: Vec<String> = Vec::new();
        let drive = run_stages::drive_walk(
            stages,
            &mut queue,
            || false,
            || false,
            |line| {
                log.push(line.to_string());
            },
            &mut procs,
        );
        run_stages::say_outcome(&drive, |line| log.push(line.to_string()));
        (drive, log)
    }

    fn offsets(&self) -> String {
        std::fs::read_to_string(self.tree.offsets_tsv()).unwrap_or_default()
    }

    fn fixed(&self, lane: &str) -> Vec<Line> {
        let path = self.tree.transcript_fixed_tsv(lane);
        textfmt::read_lines(&path).unwrap_or_default()
    }

    fn commentary(&self, lane: &str) -> Vec<Line> {
        let path = self.tree.commentary_fixed_tsv(lane);
        textfmt::read_lines(&path).unwrap_or_default()
    }

    fn session_rows(&self) -> Vec<textfmt::SessionLine> {
        textfmt::read_session(&self.tree.session_tsv()).expect("session.tsv is readable")
    }

    fn session_txt(&self) -> String {
        std::fs::read_to_string(self.tree.session_txt()).unwrap_or_default()
    }
}

/// Walk a scene under the one guard the env needs, and hand back what the walk came to. THE ONLY OWNER
/// of the ENV guard for an unmarked walk: `Scene::walk` assumes the caller holds it.
fn walk(scene: &Scene) -> (run_stages::Drive, Vec<String>) {
    let _guard = ENV.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    scene.walk()
}

/// The same guard for a walk that NAMES a marking pass (§P.policy.markingPass), which is the one that
/// dials the retake job and so must hold the env for its whole walk.
fn walk_marked(scene: &Scene, pass: MarkingPass) -> (run_stages::Drive, Vec<String>) {
    let _guard = ENV.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    scene.walk_in(pass)
}

// ---- S1 ---------------------------------------------------------------------

/// S1: every source is placed on the session clock and `offsets.tsv` is written BEFORE any block goes
/// out — the seconds a block's context names have to be ones the timeline on disk already agrees with.
#[test]
fn f1_8_s1_offsets_tsv_is_written_before_any_block() {
    let fake = Fake::start(fix_answer(4));
    let scene = Scene::new(4, &fake.url);
    let (drive, log) = walk(&scene);
    assert!(
        matches!(drive, run_stages::Drive::Done { .. }),
        "the stage walked: {drive:?} / {log:?}"
    );
    let offsets = scene.offsets();
    let rows: Vec<&str> = offsets.lines().filter(|line| !line.is_empty()).collect();
    assert_eq!(rows.len(), 1, "one video against one audio: {offsets}");
    let fields: Vec<&str> = rows[0].split('\t').collect();
    assert_eq!(fields.len(), 3, "video, audio, offset: {offsets}");
    assert_eq!(fields[0], DECK, "the video is named by its base: {offsets}");
    assert_eq!(fields[1], MIC, "and so is the audio: {offsets}");
    // The names are 90 s apart, and the row says how far the video starts into the audio.
    assert_eq!(
        fields[2].parse::<f64>().expect("the offset is a number"),
        -90.0,
        "the deck starts 90 s BEFORE the mic: {offsets}"
    );
    // Order: the offset sentence lands before the first block's checkpoint.
    let offset_line = log
        .iter()
        .position(|line| line.starts_with(">>> offset:"))
        .expect("S1 said the pairing");
    let first_block = log
        .iter()
        .position(|line| line.starts_with(">>> fixing "))
        .expect("a block went out");
    assert!(
        offset_line < first_block,
        "offsets first, blocks after: {log:?}"
    );
    fake.waited(1);
}

// ---- S2 ---------------------------------------------------------------------

/// S2: a block is `P.machine.fixBlockLines` lines and the message says so, with the grounding around it.
#[test]
fn f1_8_s2_a_block_is_25_lines_and_the_message_names_its_context() {
    let fake = Fake::start(fix_answer(25));
    let scene = Scene::new(30, &fake.url);
    let (_drive, log) = walk(&scene);
    // Thirty lines at twenty-five a block is two blocks per source. The second source's first block is
    // the identical question -- same times, same words -- so §S3's cache answers it and only three FIX
    // asks go out; what this test pins is the BLOCKING and the message, which the four checkpoints show.
    // A Prepare press now dials other jobs too (§F1.9's pooled retake runs ask the same server), so the
    // count is taken over the fix job's own asks only — recognisable by the wording only §3.3's message
    // carries — rather than over everything that reached the socket.
    let fix_asks: Vec<ChatAsk> = fake
        .waited(6)
        .into_iter()
        .filter(|ask| ask.body.contains("Transcript lines to clean"))
        .collect();
    assert_eq!(
        fix_asks.len(),
        3,
        "the unchanged repeat block costs no fix ask: {log:?}"
    );
    let blocks: Vec<&String> = log
        .iter()
        .filter(|line| line.starts_with(">>> fixing "))
        .collect();
    assert_eq!(blocks.len(), 4, "two blocks per source: {blocks:?}");
    assert!(
        blocks[0].contains("block 1/2") && blocks[1].contains("block 2/2"),
        "thirty lines are two blocks: {blocks:?}"
    );
    let body = &fix_asks[0].body;
    assert!(
        body.contains("Transcript lines to clean (25 lines, return exactly 25)"),
        "the block counts itself out for the model: {}",
        &body[..body.len().min(400)]
    );
    assert!(
        body.contains("Context around these lines:"),
        "and grounds it: {body}"
    );
    assert_eq!(
        params::find("P.machine.fixBlockLines")
            .expect("the row is catalogued")
            .spelled,
        "25",
        "P.machine.fixBlockLines"
    );
    // The tail block of the same source is the short one, and it says ITS count, not the first's.
    let second = &fix_asks[1].body;
    assert!(
        second.contains("Transcript lines to clean (5 lines, return exactly 5)"),
        "the tail block counts its own five lines: {}",
        &second[..second.len().min(400)]
    );
}

// ---- S3 ---------------------------------------------------------------------

/// S3: a valid answer lands — the fixed file holds the respelling while every time and speaker stands.
#[test]
fn f1_8_s3_a_fix_lands_on_disk() {
    let fake = Fake::start(fix_answer(4));
    let scene = Scene::new(4, &fake.url);
    let (_drive, log) = walk(&scene);
    fake.waited(2);
    let fixed = scene.fixed(DECK);
    assert_eq!(
        fixed.len(),
        4,
        "every line survives the fix: {fixed:?} / {log:?}"
    );
    assert_eq!(
        fixed[0].text, "respelled line 1",
        "the model's words are what landed"
    );
    assert_eq!(
        fixed[0].speaker, "S1",
        "the speaker is untouched: {fixed:?}"
    );
    assert_eq!(fixed[0].start, 0.0);
    assert_eq!(fixed[0].end, 0.9);
    assert!(
        !scene.fixed(MIC).is_empty(),
        "the other source was fixed too: {:?}",
        scene.fixed(MIC)
    );
}

/// S3's other branch, through the real wire: a fixer that is DOWN. A refusal of an answer is retried,
/// but a server that does not answer is reported per source rather than re-asked -- and either way the
/// block keeps the ASR's own lines (originals kept on failure) instead of losing its words.
#[test]
fn f1_8_s3_a_refused_block_keeps_the_original_lines() {
    // Nothing listens on this port: every ask fails at the socket (§Failure-is-specific: name the
    // reason where it failed, carry on elsewhere).
    let dead = {
        let probe = TcpListener::bind("127.0.0.1:0").expect("a port to name");
        let port = probe.local_addr().expect("its address").port();
        drop(probe);
        format!("http://127.0.0.1:{port}")
    };
    let scene = Scene::new(4, &dead);
    let (_drive, log) = walk(&scene);
    assert!(
        log.iter()
            .any(|line| line.contains("block 1/1") && line.contains("deck")),
        "each source walked its one block: {log:?}"
    );
    let fixed = scene.fixed(DECK);
    assert_eq!(fixed.len(), 4, "the block still owes its lines: {fixed:?}");
    assert_eq!(
        fixed[0].text, "asr heard line 0 mispelled",
        "an unanswered block keeps what the ASR heard: {fixed:?}"
    );
    assert!(
        !scene.tree.subtitles_srt(DECK).exists()
            || std::fs::read_to_string(scene.tree.subtitles_srt(DECK))
                .unwrap_or_default()
                .contains("mispelled"),
        "and its subtitles show those originals, not an invented fix"
    );
}

// ---- S4 ---------------------------------------------------------------------

/// S4: a video gets `transcript.fixed.tsv` AND `subtitles.srt` showing the fixed text; a narrator's
/// microphone gets `commentary.fixed.tsv` and no subtitles, since there is nothing to caption over it.
#[test]
fn f1_8_s4_a_video_gets_subtitles_and_a_commentator_gets_none() {
    let fake = Fake::start(fix_answer(4));
    let mut scene = Scene::new(4, &fake.url);
    scene.make_narrator();
    let (drive, log) = walk(&scene);
    fake.waited(2);
    assert!(
        matches!(drive, run_stages::Drive::Done { .. }),
        "the stage walked: {drive:?} / {log:?}"
    );
    // The deck: both files, and the srt carries the respelling rather than the ASR's guess.
    assert!(
        scene.tree.transcript_fixed_tsv(DECK).exists(),
        "the video's fixed transcript"
    );
    let srt =
        std::fs::read_to_string(scene.tree.subtitles_srt(DECK)).expect("the video's subtitles");
    assert!(
        srt.contains("respelled line 1"),
        "subtitles show the FIXED text: {srt}"
    );
    assert!(!srt.contains("mispelled"), "not the ASR's words: {srt}");
    // The narrator: commentary instead, and no subtitles at all.
    assert_eq!(
        scene.commentary(MIC).len(),
        4,
        "the recorder's words are commentary: {log:?}"
    );
    assert!(
        !scene.tree.transcript_fixed_tsv(MIC).exists(),
        "a commentator does not also get a plain fixed transcript"
    );
    assert!(
        !scene.tree.subtitles_srt(MIC).exists(),
        "and nothing to caption over an audio file"
    );
}

// ---- S5 ---------------------------------------------------------------------

/// S5: everything lands on one timeline — both sources' rows plus the deck's events labelled EVENT,
/// sorted by start — and the log counts what it merged.
#[test]
fn f1_8_s5_session_tsv_merges_both_sources_and_the_log_counts_them() {
    let fake = Fake::start(fix_answer(4));
    let scene = Scene::new(4, &fake.url);
    let (_drive, log) = walk(&scene);
    fake.waited(2);
    let rows = scene.session_rows();
    // Four + four lines + two events.
    assert_eq!(rows.len(), 10, "both transcripts and the events: {rows:?}");
    let mut starts: Vec<f64> = rows.iter().map(|row| row.start).collect();
    let sorted = {
        starts.sort_by(|a, b| a.partial_cmp(b).unwrap());
        starts
    };
    assert_eq!(
        rows.iter().map(|row| row.start).collect::<Vec<_>>(),
        sorted,
        "sorted by start: {rows:?}"
    );
    let events: Vec<&textfmt::SessionLine> = rows.iter().filter(|row| row.who == "EVENT").collect();
    assert_eq!(events.len(), 2, "the picture events are rows too: {rows:?}");
    assert!(
        events.iter().all(|row| row.source == DECK),
        "and come from the deck: {events:?}"
    );
    let sources: std::collections::BTreeSet<&str> =
        rows.iter().map(|row| row.source.as_str()).collect();
    assert_eq!(
        sources.len(),
        2,
        "both recordings made the timeline: {sources:?}"
    );
    assert!(
        log.contains(&fix_transcripts::timeline_log(rows.len(), sources.len())),
        "the log says what it merged: {log:?}"
    );
}

// ---- S6 ---------------------------------------------------------------------

/// S6 with the policy saying none: nothing is asked of any marking pass and nothing is folded away.
#[test]
fn f1_8_s6_marking_none_leaves_the_timeline_unmarked() {
    let fake = Fake::start(fix_answer(4));
    let scene = Scene::new(4, &fake.url);
    // A mark sitting there unclaimed, written in by hand: `none` means the fold asks for nothing, so it
    // stays unfolded. Planting it is honest here — the case is about the FOLD ignoring a file, and no
    // pass is dialled to have to agree with it.
    scene.plant_marks(&[Retake {
        s: 1.0,
        e: 2.5,
        again: 0.0,
        to: 2.5,
        text: String::new(),
        whole: String::new(),
    }]);
    let (_drive, log) = walk_marked(&scene, MarkingPass::None);
    fake.waited(2);
    assert!(
        scene.tree.session_tsv().exists(),
        "the timeline still stands: {log:?}"
    );
    let text = scene.session_txt();
    assert!(
        !text.contains("abandoned attempt"),
        "nothing is folded: {text}"
    );
    assert!(
        !log.iter()
            .any(|line| line.contains("the timeline stands unmarked")),
        "`none` is a policy, not a failure: {log:?}"
    );
    assert!(!text.is_empty(), "and the text is still written: {text:?}");
}

/// S6's other branch: the pass runs and marks nothing, and the timeline still pays nothing for it. A
/// three-line session is below §S1's floor on the PASS's side too — `retakes_ask::run_pass` counts the
/// spoken lines itself and writes an empty `retakes.tsv` before anyone is dialled, so no ask goes out and
/// the fold reads an empty mark list. What this pins is that the timeline is written FIRST and stands
/// complete: losing the fixes to a marking pass that found nothing would be the worse error.
///
/// The "-- the timeline stands unmarked" sentence belongs to `fix_transcripts::marking_log` and is pinned
/// verbatim in `tests/suite/fix_transcripts.rs`, where the failing pass is a closure; a real pass cannot be made
/// to fail here without taking the LLM's address away from the fix rows that share the stage.
#[test]
fn f1_8_s6_a_marking_pass_trouble_never_costs_the_timeline() {
    let fake = Fake::start(fix_answer(3));
    let scene = Scene::new(3, &fake.url);
    let (_drive, log) = walk_marked(&scene, MarkingPass::Retakes);
    // The fixes are not lost to a pass that marked nothing, and the pass said so out loud (§S1's own
    // "nothing to mark" sentence).
    assert!(
        scene.tree.session_tsv().exists(),
        "the fixes are not lost to a marking problem: {log:?}"
    );
    assert!(
        log.iter().any(|line| line.contains("retakes:")),
        "the pass reported itself before the fold: {log:?}"
    );
    assert_eq!(
        scene.session_txt().lines().count(),
        scene.session_rows().len(),
        "unmarked means every row still gets its own line: {log:?}"
    );
}

// ---- S7 ---------------------------------------------------------------------

/// S7: `session.txt` is the text the cut reads — one line per row when nothing is marked, stamped the
/// way `labelled` stamps, and the narrator's lane named as the narrator.
#[test]
fn f1_8_s7_session_txt_is_the_text_the_cut_reads() {
    let fake = Fake::start(fix_answer(4));
    let scene = Scene::new(4, &fake.url);
    let (_drive, log) = walk(&scene);
    fake.waited(2);
    let text = scene.session_txt();
    let rows = scene.session_rows();
    assert!(
        scene.tree.session_txt().exists(),
        "the file the cut opens: {log:?}"
    );
    assert_eq!(
        text.lines().count(),
        rows.len(),
        "one line per row with nothing marked: {text}"
    );
    // Stamped like `labelled`: "[start-end | mm:ss] SPEAKER: text".
    let first = text.lines().next().expect("a first line");
    assert!(
        first.contains('[') && first.contains(']') && first.contains(": "),
        "stamped: {first}"
    );
    assert!(
        first.contains("respelled line"),
        "and carrying the fixed words: {first}"
    );
    assert!(
        text.contains("EVENT: a slide appears"),
        "an event is labelled as one, not as a person: {text}"
    );
}

/// S7's fold: a stretch a marking pass left in `retakes.tsv` contributes ONE line to the text the cut
/// reads, however many rows fall inside it. Reading the stored marks back is all §S6 does with them, so
/// this calls that read-and-fold pair (`fix_session::marks_to_fix` into `fix_transcripts::session_text`)
/// over the rows a real walk wrote. Where a mark COMES FROM is §F1.9's own question and belongs to
/// `tests/suite/retakes_flow.rs`, not here.
#[test]
fn f1_8_s7_a_marked_stretch_folds_into_one_line() {
    let fake = Fake::start(fix_answer(4));
    let scene = Scene::new(4, &fake.url);
    let (_drive, log) = walk(&scene);
    fake.waited(2);
    let rows = scene.session_rows();
    let marks = naivepost::fix_session::marks_to_fix(&[Retake {
        s: 0.5,
        e: 2.5,
        again: 3.0,
        to: 2.5,
        text: String::new(),
        whole: String::new(),
    }]);
    let folded = fix_transcripts::session_text(&rows, "", &marks);
    assert!(
        folded.contains("abandoned attempt"),
        "the mark is folded into one line: {folded}"
    );
    assert!(
        folded.contains("said again at"),
        "and says where it was said again: {folded}"
    );
    assert!(
        folded.lines().count() < rows.len(),
        "fewer lines than rows, because the marked ones contribute one between them: {folded} / {log:?}"
    );
}
