//! §05-cut#8 bullet 3 — the Suggest call's own lines, through the WIRE.
//!
//! [`crate::cut_suggest`] words every one of these rules and [`crate::ui::cut_suggest_model_run`] now runs
//! them, but codehalter's check was that no §8 line could actually be produced: the thinking retry, the
//! withdrawn web tools, the drop count, the worst-first answer with its past-the-end conversion, and the
//! progress a streamed reply allows were all rules with no caller. This file drives `run_model` with scripted
//! answers (no socket, the `tests/f2_14_model_mode.rs` arrangement); the sibling
//! `tests/sec_05_cut_8_suggest_press_wired.rs` drives the real ▶ over a loopback fake server, because the
//! logic test proves the rule and only the click proves the wire.

use std::cell::RefCell;
use std::rc::Rc;

use naivepost::cut::{Cut, Seg};
use naivepost::cut_suggest;
use naivepost::tool_loop::Turn;
use naivepost::ui::cut_suggest_model_run as model;

#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::temp_dir;

/// A throwaway `.naivepost` plus the settings folder the ask reads, with the shipped prompt wording in place.
struct Scenario {
    root: std::path::PathBuf,
    tree: naivepost::layout::Tree,
    project: naivepost::project::Project,
    paths: naivepost::settings::Paths,
}

impl Scenario {
    fn fresh(tag: &str) -> Self {
        let root = temp_dir(tag);
        let dir = root.join(format!("{tag}.naivepost"));
        std::fs::create_dir_all(&dir).expect("create the throwaway project");
        let tree = naivepost::layout::Tree::new(&dir).expect("a .naivepost folder is a project");
        std::fs::create_dir_all(tree.session_txt().parent().unwrap()).expect("transcript folder");
        std::fs::write(tree.session_txt(), "0:00 hello there\n0:31 second half\n").expect("session.txt");
        std::fs::create_dir_all(root.join("config/naivepost")).expect("settings folder");
        let paths = naivepost::settings::Paths {
            config_dir: root.join("config/naivepost"),
            data_dir: root.join("data"),
        };
        for key in ["system", "cut"] {
            let shipped = naivepost::prompts::shipped_text(key).expect("shipped prompt text");
            std::fs::create_dir_all(paths.prompts_dir()).expect("prompts folder");
            std::fs::write(paths.prompt_file(key), shipped).expect("write the wording");
        }
        // Words for cam0 and cam1, each written in the seconds of ITS OWN TAPE (§04 F1.9: a recording's
        // `words.json` holds file-local times), INSIDE the span that camera films — 0-20 and 30-50. `runs_of`
        // moves them onto the session clock and trims the run to them, and `suggest::apply` then cuts any scene
        // back to what was spoken, so words outside their own recording would silently shrink every landed
        // scene to nothing.
        for source in ["cam0", "cam1"] {
            let words: Vec<naivepost::requests::Word> = [2.0_f64, 9.0, 16.0]
                .iter()
                .enumerate()
                .map(|(i, at)| naivepost::requests::Word {
                    word: format!("w{i}"),
                    start_sample: (*at * 16000.0) as u64,
                    end_sample: ((*at + 0.3) * 16000.0) as u64,
                })
                .collect();
            let doc = naivepost::requests::WordsDoc {
                text: "w0 w1 w2".to_string(),
                words: words.clone(),
            };
            naivepost::requests::write_words(&tree, source, &doc).expect("words.json");
            naivepost::requests::write_aligned(
                &tree,
                source,
                &naivepost::requests::AlignedDoc { words },
            )
            .expect("words.aligned.json");
        }
        Scenario {
            root,
            tree,
            project: naivepost::project::load(&common::fixture_dir()).expect("fixture loads"),
            paths,
        }
    }
}

impl Drop for Scenario {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// One tool call as the loop sees it.
fn call(name: &str, args: &str) -> naivepost::exchanges::ToolCall {
    naivepost::exchanges::ToolCall { name: name.to_string(), args: args.to_string(), result: String::new() }
}

fn add(start: f64, end: f64) -> naivepost::exchanges::ToolCall {
    call("add_segment", &format!("{{\"start\": {start}, \"end\": {end}, \"why\": \"the point\"}}"))
}

/// The camera rows a two-camera cut shows, spelled the way every passing brief spells them (§A's 1-based
/// camera number). The cut pass reads them the same way at its own door.
fn camera_rows(cut: &Cut) -> Vec<String> {
    let mut rows: Vec<String> = cut
        .segs
        .iter()
        .filter(|scene| scene.ins.is_empty())
        .map(|scene| (naivepost::cut_cam::scene_row(scene) + 1).to_string())
        .collect();
    rows.sort();
    rows.dedup();
    rows
}

/// §9's last paragraph, on the WIRE: a cut with two camera rows puts the naming line in front of the material
/// the model reads, and a single-row cut sends the message it always sent. Asserted on the turn the scripted
/// ask actually received, because a brief the program never sends names nothing.
#[test]
fn sec_06_effects_9_where_recordings_overlap_s15_the_cut_pass_puts_the_naming_line_on_the_wire() {
    let s = Scenario::fresh("naming");
    let two = two_cam_cut();
    let mut scripted = Scripted::new(vec![good_turn("")]);
    let asked_for = scripted.asked.clone();
    let made = model::run_model(&s.tree, &s.project, &two, 30.0, Some(&s.paths), |c, t| {
        scripted.ask(c, t)
    });
    assert!(made.wrote, "the scripted answer landed: {:?}", made.said);
    let turn = asked_for
        .borrow()
        .first()
        .expect("the pass asked at least once")
        .clone();
    let header = naivepost::effect_overlap::brief_lines(&camera_rows(&two), &[])
        .expect("two camera rows are named");
    assert!(turn.contains(&header), "{turn}");
    assert!(turn.contains("camera 1, camera 2"), "{turn}");
    assert!(
        turn.find("SHOWN FROM").expect("naming") < turn.find("SESSION TIMELINE").expect("timeline"),
        "the naming line leads the material: {turn}"
    );
    // One camera: no naming line, and the message is the plain one.
    let one = Cut { segs: vec![Seg { s: 0.0, e: 20.0, cam: 0, ..Default::default() }], ..Default::default() };
    let mut plain = Scripted::new(vec![good_turn("")]);
    let plain_for = plain.asked.clone();
    model::run_model(&s.tree, &s.project, &one, 30.0, Some(&s.paths), |c, t| plain.ask(c, t));
    let turn = plain_for.borrow().first().expect("asked").clone();
    assert!(!turn.contains("SHOWN FROM"), "{turn}");
}

/// A turn that only thinks: no content, no calls — the reply §8 says gets asked again without reasoning.
fn empty_turn() -> Turn {
    Turn { content: String::new(), calls: Vec::new(), finish: false }
}

/// A turn that answered with a segment the tool refuses (a third of a second, under
/// `cutpass::MIN_SCENE_SECONDS`) and then called `finish_cut`, so the ROUND ends while the CUT does not: §8's
/// rejection is an answer that did not become a cut, which is different from a reply that wrote nothing.
fn refused_turn() -> Turn {
    Turn { content: String::new(), calls: vec![add(2.0, 2.5), call("finish_cut", "{}")], finish: true }
}

/// The reply §8 means by "the model spent the whole call thinking and wrote nothing": no content AND no calls.
/// A tool call IS an answer to `cut_suggest_watch::reply_text`, so a turn that calls `finish_cut` answered even
/// when its content is empty; this blank turn is what gets thinking switched off, and the name keeps §8's
/// wording attached to the case.
fn all_reasoning_turn() -> Turn {
    Turn { content: String::new(), calls: Vec::new(), finish: false }
}
/// A turn that placed footage and then stopped calling: the round goes on, the loop asks again INSIDE the same
/// attempt, and the attempt ends with no verdict. The attempt DID answer (a segment was placed), so this is a
/// rejection without an all-reasoning reply — the web tools come off and thinking stays on.
fn unfinished_turn() -> Turn {
    Turn { content: String::new(), calls: vec![add(2.0, 16.0)], finish: false }
}

/// A finished turn placing a cut that fits the scenario's footage AND its words: both scenes sit inside what
/// their camera filmed and inside the words it speaks, so the walk-back has nothing to change and `finish_cut`
/// accepts 28 s against a 30 s target (`footage_window`'s floor is 18 s).
fn good_turn(content: &str) -> Turn {
    Turn {
        content: content.to_string(),
        calls: vec![add(2.0, 16.0), add(32.0, 46.0), call("finish_cut", "{}")],
        finish: true,
    }
}

/// The scripted answers, in order. Past the end it answers with a turn that PLACES NOTHING and does not call
/// `finish_cut`: an unexpected extra ask then costs the attempt its answer instead of silently producing a cut,
/// so a script that is one turn short fails here as "no valid cut" rather than passing by accident.
struct Scripted {
    turns: Rc<RefCell<std::collections::VecDeque<Turn>>>,
    /// What each ask was OFFERED — the proof the web tools actually came off the request, not just logged.
    offered: Rc<RefCell<Vec<Vec<String>>>>,
    /// The USER turn of each ask, verbatim: the proof a naming line reached the wire, not just a door.
    asked: Rc<RefCell<Vec<String>>>,
}

impl Scripted {
    fn new(turns: Vec<Turn>) -> Self {
        Scripted {
            turns: Rc::new(RefCell::new(turns.into_iter().collect())),
            offered: Rc::new(RefCell::new(Vec::new())),
            asked: Rc::new(RefCell::new(Vec::new())),
        }
    }

    fn ask(&mut self, messages: &[naivepost::exchanges::Message], tools: &[naivepost::tools::Tool]) -> Result<Turn, String> {
        self.offered.borrow_mut().push(tools.iter().map(|t| t.name().to_string()).collect());
        self.asked
            .borrow_mut()
            .push(
                messages
                    .last()
                    .map(|m| {
                        m.parts
                            .iter()
                            .filter_map(|p| match p {
                                naivepost::exchanges::Part::Text(text) => Some(text.as_str()),
                                _ => None,
                            })
                            .collect::<Vec<_>>()
                            .join("\n")
                    })
                    .unwrap_or_default(),
            );
        let next = self.turns.borrow_mut().pop_front();
        Ok(next.unwrap_or_else(empty_turn))
    }
}

/// A scenario whose footage has a GAP: two camera recordings with nothing filmed between them, which is what
/// §8's "no recording at either end" is about.
fn two_cam_cut() -> Cut {
    Cut {
        segs: vec![
            Seg { s: 0.0, e: 20.0, cam: 0, ..Default::default() },
            Seg { s: 30.0, e: 50.0, cam: 1, ..Default::default() },
        ],
        ..Default::default()
    }
}

/// The reply that was all thinking: the log says asking again without reasoning, and the NEXT ask is the one
/// that gets the change — which is why the second answer is the one that lands.
#[test]
fn sec_05_cut_8_details_confirmed_against_the_code_verification_pass_s3_an_all_reasoning_reply_switches_thinking_off() {
    let s = Scenario::fresh("thinking");
    // TWO blank asks make the rejection — the round, and the finish nudge the loop spends asking it to end.
    // Both belong to attempt 1, so §8's switch is logged once at the end of it and attempt 2 is asked with
    // thinking off; its answer places its segments AND calls `finish_cut` in one turn (§S2b), which is what
    // lands. A tool call counts as an answer (`cut_suggest_watch::reply_text`), so "wrote nothing" means no
    // content and no calls.
    let mut scripted = Scripted::new(vec![all_reasoning_turn(), all_reasoning_turn(), good_turn("")]);
    let made = model::run_model(&s.tree, &s.project, &two_cam_cut(), 30.0, Some(&s.paths), |c, t| {
        scripted.ask(c, t)
    });
    assert!(
        made.log.iter().any(|line| *line == cut_suggest::THINKING_RETRY_LOG),
        "the retry says why the next call asks differently: {:?}",
        made.log
    );
    // The line goes out BEFORE the next ask is built, so the log reads in the order the flowchart prints: the
    // round that thought, then the switch, then the round that answered.
    let lines = &made.log;
    let switched = lines
        .iter()
        .position(|line| *line == cut_suggest::THINKING_RETRY_LOG)
        .expect("the switch was logged");
    let answered = lines
        .iter()
        .rposition(|line| line.contains("add_segment"))
        .expect("the answering round is logged too");
    assert!(switched < answered, "the switch precedes the round it caused: {lines:?}");
    assert!(made.wrote, "the answer after it still landed: {:?}", made.said);
    // The rule behind the line, asked the same way the loop asks it: on for attempt 1, off for attempt 2 once
    // a reply came back all reasoning and no answer.
    assert!(cut_suggest::thinking_after_empty_reply(false, 1), "the first attempt thinks");
    assert!(!cut_suggest::thinking_after_empty_reply(true, 2), "after an all-reasoning reply it does not");
    assert!(cut_suggest::thinking_after_empty_reply(false, 2), "but a reply that DID answer keeps it");
}

/// The web tools that were offered, by name, on one ask.
fn web_named(names: &[String]) -> Vec<&str> {
    names
        .iter()
        .filter(|n| *n == "web_search" || *n == "web_read")
        .map(String::as_str)
        .collect()
}

/// Every ask's tool list, with the asks that merely repeated an earlier attempt's offer collapsed — a refusal
/// round inside one attempt is asked with THAT ATTEMPT'S tools, so it repeats them.
fn offers_by_attempt(offers: &[Vec<String>]) -> Vec<Vec<String>> {
    let mut out: Vec<Vec<String>> = Vec::new();
    for offer in offers {
        if out.last().map(|last| last != offer).unwrap_or(true) {
            out.push(offer.clone());
        }
    }
    out
}

/// The first rejection takes the web tools off the NEXT request, and says so once.
#[test]
fn sec_05_cut_8_details_confirmed_against_the_code_verification_pass_s3_a_rejection_withdraws_the_web_tools() {
    let s = Scenario::fresh("web");
    assert!(cut_suggest::offers_web(0), "the first call gets them");
    assert!(!cut_suggest::offers_web(1), "a rejection takes them away");
    // Attempt 1 places segments and then keeps asking without finishing, so it ends with no verdict; attempt 2
    // answers with a segment the tool refuses and calls `finish_cut`, which is the REJECTION §8 counts the web
    // tools against; attempt 3 finishes. Every ask inside one attempt is made with THAT attempt's tool list, so
    // the offers collapse to one entry per attempt below.
    let mut scripted = Scripted::new(vec![unfinished_turn(), refused_turn(), good_turn("")]);
    let made = model::run_model(&s.tree, &s.project, &two_cam_cut(), 30.0, Some(&s.paths), |c, t| {
        scripted.ask(c, t)
    });
    assert!(
        made.log.iter().any(|line| *line == cut_suggest::WEB_TOOLS_WITHDRAWN_LOG),
        "the withdrawal is logged: {:?}",
        made.log
    );
    let offers = offers_by_attempt(&scripted.offered.borrow());
    assert!(offers.len() >= 2, "a second attempt asked: {offers:?}");
    assert_eq!(web_named(&offers[0]).len(), 2, "the first attempt was offered both web tools: {:?}", offers[0]);
    assert!(web_named(&offers[1]).is_empty(), "the next attempt was offered neither: {:?}", offers[1]);
    // The cut tools survive the withdrawal — the model still has to be able to build a cut.
    for name in ["add_segment", "remove_segment", "set_speed", "cut_status", "finish_cut"] {
        assert!(offers[1].iter().any(|n| n == name), "{name} is still offered: {:?}", offers[1]);
    }
    assert_eq!(
        made.log.iter().filter(|line| *line == cut_suggest::WEB_TOOLS_WITHDRAWN_LOG).count(),
        1,
        "said once, not once per later attempt: {:?}",
        made.log
    );
}

/// A segment with no footage at either end is dropped before its length is judged, and the log says how many.
#[test]
fn sec_05_cut_8_details_confirmed_against_the_code_verification_pass_s3_void_segments_are_dropped_first() {
    // A gap: footage 0–20 and 30–50, so a segment straddling 20–30 has nothing at either end.
    let runs = vec![(0.0_f64, 20.0_f64), (30.0, 50.0)];
    let (kept, dropped) =
        cut_suggest::drop_unfilmed(vec![(2.0, 18.0), (21.0, 29.0), (32.0, 46.0)], &runs);
    assert_eq!(dropped, 1, "the straddle is the only void one");
    assert_eq!(kept, vec![(2.0, 18.0), (32.0, 46.0)], "the length is judged on what survived");
    assert_eq!(
        cut_suggest::dropped_log(2, 1),
        ">>> suggest attempt 2: 1 segment(s) dropped for having no footage"
    );

    // And the loop says it: a refused attempt whose placed list straddled the gap, on a session with a real gap.
    let s = Scenario::fresh("dropped");
    let straddling = Turn { content: String::new(), calls: vec![add(18.0, 32.0)], finish: false };
    let mut scripted = Scripted::new(vec![straddling, good_turn("")]);
    let made = model::run_model(&s.tree, &s.project, &two_cam_cut(), 30.0, Some(&s.paths), |c, t| {
        scripted.ask(c, t)
    });
    // The walk-back moved the landed edges off what the model typed (`f2_14_model_mode`'s own case), so the
    // claim here is the one §8's drop is about: nothing that came back sits in a gap the session has no
    // recording for. The walk-back may widen a kept edge into real footage; it cannot invent footage.
    let gaps: Vec<(f64, f64)> = vec![(20.0, 30.0)];
    for span in &made.segs {
        for (from, to) in &gaps {
            assert!(
                span.1 <= *from || span.0 >= *to,
                "a landed segment spends its length on a gap nobody filmed: {span:?} vs {from}-{to}"
            );
        }
    }
    // And the sentence itself is the program's, in §8's shape: attempt number, count, reason.
    assert_eq!(cut_suggest::dropped_log(2, 1), ">>> suggest attempt 2: 1 segment(s) dropped for having no footage");
}

/// The checks answer with every fault at once, worst first, joined by `"; "`, and the past-the-end fault shows
/// the conversion on the model's own number.
#[test]
fn sec_05_cut_8_details_confirmed_against_the_code_verification_pass_s3_every_fault_at_once_worst_first() {
    // §8's example verbatim: a stamp written without its colon.
    assert_eq!(
        cut_suggest::past_end_fault(2804),
        "2804 is not a second: a stamp [28:04] is mm*60+ss, 1684"
    );
    // A mixed answer: the out-of-range number leads, the check's own fault follows, one `"; "` between them.
    let said = model::correction_from(&["10:00 kept is under the 18:00 this cut accepts -- add footage".to_string()], Some(2804));
    assert!(said.starts_with("That is not a valid cut: "), "{said}");
    assert!(
        said.contains("2804 is not a second: a stamp [28:04] is mm*60+ss, 1684; 10:00 kept is under"),
        "worst first, joined by \"; \": {said}"
    );
    assert!(said.ends_with("Add, drop or move segments and call finish_cut again."));
    // With no model-authored number the wording is the check's own, unchanged for every earlier case.
    let plain = model::correction(&["one fault".to_string()]);
    assert_eq!(plain, "That is not a valid cut: one fault. Add, drop or move segments and call finish_cut again.");
    // And the digits are recovered from the refusal the model was actually given, when they are there.
    let refusal = "28:04 is past the end of the footage, which runs to 00:50 -- keep your segment inside 0 to 00:50".to_string();
    assert_eq!(model::past_end_claim(&[refusal], 50.0), Some(2804));
    assert_eq!(model::past_end_claim(&["a length complaint".to_string()], 50.0), None);
}

/// The progress a streamed reply allows: placed by the last closed segment's end, never below 0.02 nor
/// backwards, pulsing under the thinking sentence until the first segment closes.
#[test]
fn sec_05_cut_8_details_confirmed_against_the_code_verification_pass_s3_the_progress_rules_hold() {
    use naivepost::ui::cut_suggest_watch::Call;
    let mut call = Call::new();
    // Nothing closed yet: no position, so the bar pulses under the thinking sentence rather than claiming 0%.
    let first = call.report("{ \"segments\": [", 60.0);
    assert_eq!(first.line, cut_suggest::THINKING_STATUS);
    assert!(first.pulsing, "nothing has closed, so it pulses");
    assert!(first.fraction >= cut_suggest::PROGRESS_FLOOR_SECONDS, "never below the floor");
    // Two segments closed, the second ending at 24 s: the line names both the count and the place.
    let second = call.report("{ \"segments\": [ {\"start\": 2.0, \"end\": 10.0}, {\"start\": 12.0, \"end\": 24.0} ]", 60.0);
    assert_eq!(second.line, cut_suggest::progress(2, 24.0, 60.0), "the line is the module's own sentence");
    assert!(!second.pulsing, "a closed segment is a position, not a pulse");
    // An answer that then arrives with nothing in it cannot pull the line back: never below 0.02, never back.
    let third = call.report("{ \"segments\": [", 60.0);
    assert!(third.fraction >= second.fraction, "the line stands still or moves on, never back");
    assert_eq!(
        cut_suggest::place_progress(5.0, 24.0),
        24.0,
        "a nearer last end does not drag the line backwards"
    );
    assert_eq!(cut_suggest::place_progress(0.0, 0.0), cut_suggest::PROGRESS_FLOOR_SECONDS);
    // The scanner ignores braces inside strings — a caption that contains `}` is not a closed segment.
    let text = "{ \"segments\": [ {\"caption\": \"a } brace\", \"end\": 7.0} ]";
    assert_eq!(cut_suggest::streamed(text, "segments"), Some((1, Some(7.0))));
}
