// F2.14 §S2b/§S3 — the MODEL path's rules, asked without a socket.
//
// spec/05-cut.md §F2.14 S2b: the model gets `add_segment`, `remove_segment`, `set_speed`, `cut_status`
// and `finish_cut` (02-services.md §3.6), each answered by `tools::cutpass::Plan`; up
// `P.eng.llmAttempts` = 3 attempts, then "no valid cut after 3 attempts". §S3 then walks the answer
// back — holes closed where somebody talked, edges snapped, marks and dead air out, inserts kept — and
// only that is a suggestion.
//
// Every case hands `cut_suggest_model_run` its own `ask` closure answering scripted `tool_loop::Turn`s,
// which is the arrangement `fix_call` and `narrate_tts::speak_line` are tested behind: no case reaches
// a socket, and no case asserts about a model reply it could not predict.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};

use naivepost::cut::{Cut, Seg};
use naivepost::layout::Tree;
use naivepost::project::Project;
use naivepost::roles::Job;
use naivepost::settings::{self, Paths};
use naivepost::suggest;
use naivepost::textfmt::{self, Retake};
use naivepost::tool_loop::Turn;
use naivepost::tools::{cutpass, Tool};
use naivepost::ui::cut_suggest_model_run as model;
use naivepost::suggest as sug;

use crate::common;
#[allow(dead_code)]
#[allow(unused_imports)] // the widget-case helpers stay imported so this file matches `f2_14_words_mode.rs`'s shape
use common::{fixture_dir, settle, status_text, temp_dir};

static RAN_SNAP: AtomicBool = AtomicBool::new(false);
static RAN_REFUSE_ARGS: AtomicBool = AtomicBool::new(false);
static RAN_STATUS_FINISH: AtomicBool = AtomicBool::new(false);
static RAN_REPAIR: AtomicBool = AtomicBool::new(false);
static RAN_EXHAUSTED: AtomicBool = AtomicBool::new(false);
static RAN_NOTHING_WRITTEN: AtomicBool = AtomicBool::new(false);
static RAN_WALK_BACK: AtomicBool = AtomicBool::new(false);

/// A throwaway `.naivepost` plus the settings folder the press reads, both under one temp root.
struct Scenario {
    root: std::path::PathBuf,
    tree: Tree,
    project: Project,
    paths: Paths,
}

impl Scenario {
    fn fresh(tag: &str) -> Self {
        let root = temp_dir(tag);
        let dir = root.join(format!("{tag}.naivepost"));
        std::fs::create_dir_all(&dir).expect("create the throwaway project");
        std::fs::create_dir_all(Tree::new(&dir).unwrap().session_txt().parent().unwrap())
            .expect("the transcript folder");
        let tree = Tree::new(&dir).expect("a .naivepost folder is a project");
        std::fs::create_dir_all(root.join("config/naivepost")).expect("the settings folder");
        let paths = Paths {
            config_dir: root.join("config/naivepost"),
            data_dir: root.join("data"),
        };
        // The wording the ask reads, through the app's own reader: what lands in the test's settings
        // folder is the shipped text, fence stripped, exactly as the shipped build would send it.
        for key in ["system", "cut"] {
            let shipped = naivepost::prompts::shipped_text(key).expect("shipped prompt text");
            std::fs::create_dir_all(paths.prompts_dir()).expect("the prompts folder");
            std::fs::write(paths.prompt_file(key), shipped).expect("write the wording");
        }
        Scenario {
            root,
            tree,
            project: naivepost::project::load(&fixture_dir()).expect("fixture loads"),
            paths,
        }
    }

    /// `session.txt`: §S1's "there is a timeline" marker and the text the model is shown.
    fn with_timeline(self) -> Self {
        std::fs::write(self.tree.session_txt(), "0:00 hello there\n").expect("write session.txt");
        self
    }

    /// Words for cam0 that the aligner answered — what `runs_of` needs to call the footage spoken.
    fn with_words(self, source: &str, spoken: &[(&str, f64)]) -> Self {
        let words: Vec<naivepost::requests::Word> = spoken
            .iter()
            .map(|(text, at)| naivepost::requests::Word {
                word: text.to_string(),
                start_sample: (*at * 16000.0) as u64,
                end_sample: ((at + 0.3) * 16000.0) as u64,
            })
            .collect();
        let doc = naivepost::requests::WordsDoc {
            text: spoken.iter().map(|(w, _)| *w).collect::<Vec<&str>>().join(" "),
            words: words.clone(),
        };
        naivepost::requests::write_words(&self.tree, source, &doc).expect("write words.json");
        naivepost::requests::write_aligned(
            &self.tree,
            source,
            &naivepost::requests::AlignedDoc { words },
        )
        .expect("write words.aligned.json");
        self
    }

    fn with_marks(self, marks: &[Retake]) -> Self {
        textfmt::write_retakes(marks, &self.tree.retakes_tsv()).expect("write retakes.tsv");
        self
    }
}

impl Drop for Scenario {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn retake(s: f64, e: f64) -> Retake {
    Retake {
        s,
        e,
        again: 0.0,
        to: e,
        text: String::new(),
        whole: String::new(),
    }
}

/// A cut showing one camera's footage from `s` to `e`.
fn cam_cut(s: f64, e: f64) -> Cut {
    Cut {
        segs: vec![Seg { s, e, cam: 0, ..Default::default() }],
        ..Default::default()
    }
}

fn call(name: &str, args: &str) -> naivepost::exchanges::ToolCall {
    naivepost::exchanges::ToolCall {
        name: name.to_string(),
        args: args.to_string(),
        result: String::new(),
    }
}

fn turn(calls: Vec<naivepost::exchanges::ToolCall>) -> Turn {
    let finish = calls.iter().any(|c| c.name == "finish_cut");
    Turn {
        content: String::new(),
        calls,
        finish,
    }
}

/// An `ask` that answers each scripted turn in order and counts how often it was asked. A turn list that
/// runs out answers a silent finish: the test asked for everything it planned to give.
#[derive(Clone, Default)]
struct Scripted {
    turns: Rc<RefCell<std::collections::VecDeque<Turn>>>,
    asked: Rc<std::cell::Cell<usize>>,
}

impl Scripted {
    fn new(turns: Vec<Turn>) -> Self {
        Scripted {
            turns: Rc::new(RefCell::new(turns.into_iter().collect())),
            asked: Rc::new(std::cell::Cell::new(0)),
        }
    }

    fn times(&self) -> usize {
        self.asked.get()
    }

    fn ask(&mut self, _: &[naivepost::exchanges::Message], _: &[Tool]) -> Result<Turn, String> {
        self.asked.set(self.asked.get() + 1);
        let next = self.turns.borrow_mut().pop_front();
        Ok(next.unwrap_or_else(|| turn(vec![])))
    }
}

/// What one `add_segment` for `start`/`end` costs: the tool's own answer, so a case can read the edges
/// back out of the JSON the model would have read.
fn add(start: f64, end: f64) -> naivepost::exchanges::ToolCall {
    call(
        "add_segment",
        &format!("{{\"start\": {start}, \"end\": {end}, \"why\": \"the point\"}}"),
    )
}

#[test]
fn f2_14_s2b_a_segment_lands_on_a_snap_point_not_the_models_number() {
    let mut plan = cutpass::Plan::new(600.0, 60.0);
    // A word edge 1 s before the number the model typed, inside `SNAP_TOLERANCE_SECONDS`.
    let mut walk = model::walk_of(&Default::default());
    walk.snap_points = vec![(19.0, naivepost::cut_effect_decisions::Candidate::WordEdge)];
    plan = plan.with_snap_points(walk.snap_points.iter().map(|(p, _)| *p).collect());
    let answer = model::answer_call(&mut plan, &add(20.0, 40.0));
    assert!(
        !answer.contains("error"),
        "an in-range request is placed: {answer}"
    );
    assert_eq!(plan.segments().len(), 1, "the model asked for one segment");
    assert_eq!(
        plan.segments()[0].start, 19.0,
        "the edge moved to the word edge, so the model's own 20 is not a cut"
    );
    assert_eq!(plan.segments()[0].why, "the point", "the model's reason rides along");
    // No candidate near a stamp leaves it exactly where the model put it: `suggest::apply` later applies
    // the same rule, so a number with nothing beside it survives unchanged rather than drifting.
    let mut bare = cutpass::Plan::new(600.0, 60.0);
    model::answer_call(&mut bare, &add(20.0, 40.0));
    assert_eq!(bare.segments()[0].start, 20.0, "nothing to snap to, nothing moved");
    RAN_SNAP.store(true, Ordering::SeqCst);
}

#[test]
fn f2_14_s2b_a_bad_request_is_answered_not_defaulted() {
    let mut plan = cutpass::Plan::new(100.0, 0.0);
    // Past the footage: the tool's own sentence, naming the model's own number in mm:ss.
    let past = model::answer_call(&mut plan, &add(90.0, 200.0));
    assert!(past.contains("error"), "past the end is an error: {past}");
    assert!(past.contains("03:20"), "the model's own end is quoted back: {past}");
    assert!(past.contains("01:40"), "and the footage's own end: {past}");
    assert!(plan.segments().is_empty(), "a refused segment is not placed");
    // A missing `start` names `start`, and nothing was placed at 0 — the default that would silently cut
    // the head of the session.
    let missing = model::answer_call(&mut plan, &call("add_segment", r#"{"end": 30.0}"#));
    assert!(missing.contains("start"), "the absent argument is named: {missing}");
    assert!(!missing.contains("end"), "an argument it DID send is not named: {missing}");
    assert!(plan.segments().is_empty(), "no zero-second segment was invented");
    // A non-numeric one, and an argument blob that is not JSON at all: both answered, neither panics.
    let junk = model::answer_call(&mut plan, &call("add_segment", r#"{"start":"soon","end":9.0}"#));
    assert!(junk.contains("start"), "a non-number is the argument that is wrong: {junk}");
    let not_json = model::answer_call(&mut plan, &call("set_speed", "start=5 rate=2"));
    assert!(not_json.contains("start"), "an unreadable blob names its arguments: {not_json}");
    assert!(plan.segments().is_empty());
    // An unknown tool name is answered too: §S2b offers five, and a model that invents a sixth must be
    // told, not silently ignored.
    let unknown = model::answer_call(&mut plan, &call("add_perfect_cut", "{}"));
    assert!(unknown.contains("add_perfect_cut"), "the name comes back: {unknown}");
    RAN_REFUSE_ARGS.store(true, Ordering::SeqCst);
}

#[test]
fn f2_14_s2b_status_and_finish_come_from_the_tools() {
    let mut plan = cutpass::Plan::new(600.0, 60.0);
    let status = model::answer_call(&mut plan, &call("cut_status", "{}"));
    assert_eq!(
        status,
        plan.status_json(),
        "cut_status is the plan's own reading, not a second one"
    );
    assert!(status.contains("\"segments\""), "the reading names the count: {status}");
    // Short of the target window, `finish_cut` answers the problem list §S2 S6 sends back.
    let empty = model::answer_call(&mut plan, &call("finish_cut", "{}"));
    assert!(empty.contains("error"), "nothing placed is not a cut: {empty}");
    assert!(model::answer_call(&mut plan, &add(0.0, 25.0)).contains("span"));
    assert!(model::answer_call(&mut plan, &add(30.0, 55.0)).contains("span"));
    assert!(model::answer_call(&mut plan, &add(60.0, 78.0)).contains("span"));
    assert!(model::answer_call(&mut plan, &add(80.0, 95.0)).contains("span"));
    let enough = model::answer_call(&mut plan, &call("finish_cut", "{}"));
    assert!(
        !enough.contains("error") && enough.contains("footage"),
        "inside the window, finish_cut says ok: {enough}"
    );
    assert!(model::acceptable(&plan, 60.0), "and this module agrees with it");
    RAN_STATUS_FINISH.store(true, Ordering::SeqCst);
}

#[test]
fn f2_14_s2b_a_first_answer_that_is_not_a_cut_gets_one_correction() {
    let s = Scenario::fresh("repair").with_timeline().with_words("cam0", &[("hi", 2.0), ("yo", 30.0)]);
    let cut = cam_cut(0.0, 60.0);
    // Round 1 places one short stretch and calls it done; §S2 S6's complaint goes back; round 2 fills
    // the window and finishes. The attempts are counted here, so the script must cover both.
    let short = turn(vec![
        add(2.0, 5.0),
        call("finish_cut", "{}"),
    ]);
    let filled = turn(vec![
        add(2.0, 20.0),
        add(25.0, 45.0),
        add(46.0, 60.0),
        call("finish_cut", "{}"),
    ]);
    let mut scripted = Scripted::new(vec![short, filled]);
    let made = model::run_model(
        &s.tree,
        &s.project,
        &cut,
        60.0,
        Some(&s.paths),
        |conversation, tools| scripted.ask(conversation, tools),
    );
    assert!(made.wrote, "the second answer landed: {:?}", made.said);
    assert!(!made.segs.is_empty(), "a cut of something");
    assert_eq!(
        made.log
            .iter()
            .filter(|line| line.starts_with(">>> cut: round "))
            .count(),
        2,
        "two rounds, one per attempt: {:?}",
        made.log
    );
    assert_eq!(scripted.times(), 2, "the repair round was asked, and only that one");
    // §S3's own pair rides on `applied.log` — the two lines the flowchart prints for a landed
    // suggestion — which is where `landed_log` reads the status sentence from.
    assert!(
        made.applied
            .log
            .first()
            .is_some_and(|line| line.starts_with(">>> suggested ")),
        "§S3's line leads the walk-back's log: {:?}",
        made.applied.log
    );
    assert_eq!(
        made.said,
        made.applied.log[0].trim_start_matches(">>> "),
        "and the status row says that line, nothing invented"
    );
    RAN_REPAIR.store(true, Ordering::SeqCst);
}

#[test]
fn f2_14_s2b_three_attempts_and_it_says_so() {
    let s = Scenario::fresh("exhaust").with_timeline().with_words("cam0", &[("hi", 2.0)]);
    let cut = cam_cut(0.0, 60.0);
    // Every attempt finishes on a cut that fails the whole-check: too short for a 10-minute target.
    let never = || {
        Scripted::new(vec![turn(vec![add(2.0, 4.0), call("finish_cut", "{}")])])
    };
    let mut scripted = never();
    let made = model::run_model(
        &s.tree,
        &s.project,
        &cut,
        600.0,
        Some(&s.paths),
        |conversation, tools| scripted.ask(conversation, tools),
    );
    assert_eq!(made.said, sug::NO_VALID_CUT, "S6's sentence, unaltered");
    assert!(!made.wrote, "a cut that never passed lands nothing");
    assert_eq!(
        scripted.times(),
        naivepost::roles::LLM_ATTEMPTS as usize,
        "`P.eng.llmAttempts` attempts and no more"
    );
    assert!(
        sug::attempts_exhausted(naivepost::roles::LLM_ATTEMPTS),
        "and the bound is the same one `suggest` answers with"
    );
    RAN_EXHAUSTED.store(true, Ordering::SeqCst);
}

#[test]
fn f2_14_s2b_a_failed_suggestion_touches_nothing() {
    let s = Scenario::fresh("untouched")
        .with_timeline()
        .with_words("cam0", &[("hi", 2.0)])
        .with_marks(&[retake(4.0, 5.0)]);
    let before = cam_cut(0.0, 60.0);
    naivepost::cut::save(&before, &s.tree).expect("write the cut the press was handed");
    let mut scripted = Scripted::new(vec![turn(vec![add(2.0, 4.0), call("finish_cut", "{}")])]);
    let made = model::run_model(
        &s.tree,
        &s.project,
        &before,
        600.0,
        Some(&s.paths),
        |conversation, tools| scripted.ask(conversation, tools),
    );
    assert!(!made.wrote);
    assert_eq!(
        naivepost::cut::load(&s.tree).expect("the file still loads"),
        before,
        "the project's cut file is byte-for-byte what the press was handed"
    );
    // And the flowchart's story is in the log: the rounds, then the reason it stopped.
    assert!(
        made.log.last().is_some_and(|line| line.contains(sug::NO_VALID_CUT)),
        "the last line is the reason: {:?}",
        made.log
    );
    RAN_NOTHING_WRITTEN.store(true, Ordering::SeqCst);
}

#[test]
fn f2_14_s2b_the_answer_is_walked_back_before_it_is_a_suggestion() {
    let s = Scenario::fresh("walkback").with_timeline().with_words("cam0", &[("hi", 2.0), ("yo", 40.0)]);
    let mut cut = cam_cut(0.0, 60.0);
    // One hand-placed card: §S3 keeps it, and says so by handing it back rather than re-proposing it.
    cut.segs.push(Seg {
        s: 0.0,
        e: 0.0,
        dur: 3.0,
        ins: "card.png".to_string(),
        cam: 0,
        ..Default::default()
    });
    // The model keeps two stretches with a 1 s hole between them, and somebody talks in that hole:
    // §S3 joins them (P.eng.seamMaxSeconds is 1.5, so 1.0 is inside it).
    let mut scripted = Scripted::new(vec![turn(vec![
        add(2.0, 20.0),
        add(21.0, 45.0),
        call("finish_cut", "{}"),
    ])]);
    let made = model::run_model(
        &s.tree,
        &s.project,
        &cut,
        0.0,
        Some(&s.paths),
        |conversation, tools| scripted.ask(conversation, tools),
    );
    assert!(made.wrote, "a no-target cut has no length complaint: {:?}", made.said);
    assert_eq!(
        made.applied.kept_inserts.len(),
        1,
        "the card survives, untrimmed"
    );
    assert!(made.applied.kept_inserts[0].is_insert());
    let joined = made.segs.iter().any(|(a, b)| a <= &2.0 && b >= &45.0);
    // The edges then move to the word edges the words path trusts, so the joined stretch is at least as
    // wide as the model's own; a hole that was NOT talked in would still be there.
    assert!(
        joined || made.segs.len() == 1,
        "the talked-in 1 s hole closed: {:?}",
        made.segs
    );
    // Same answer, same walk-back, spelled directly: `close_holes` is the rule the run went through.
    let walk = model::walk_of(&naivepost::cut_suggest_words_run::inputs(&s.tree, &s.project, &cut));
    let direct = suggest::close_holes(&[(2.0, 20.0), (21.0, 45.0)], &walk.talked, suggest::SEAM_MAX_SECONDS);
    assert_eq!(direct.len(), 1, "and the pure rule agrees: {direct:?}");
    RAN_WALK_BACK.store(true, Ordering::SeqCst);
}

/// The job's own tools, so a case can name what the model was offered without a hard-coded list.
#[allow(dead_code)]
fn offered_names() -> Vec<&'static str> {
    naivepost::tools::offered(Job::ModelCut)
        .iter()
        .map(|tool| tool.name())
        .collect()
}

/// `settings::from_environment()` reads the process environment, which a test must not move; every
/// case here passes its own `Paths`, so this only pins that the module reads them per call.
#[allow(dead_code)]
fn env_paths() -> Option<Paths> {
    settings::from_environment()
}
