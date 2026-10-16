//! §12-decisions.md · **Cut and effects** — the decisions the cut pass makes behind the model, checked on
//! the live path rather than in prose.
//!
//! Each test is named for the numbered entry it proves. Entry 13 is the dead-air rule: the app cuts long
//! silences out of kept clips AFTER the model has answered, so §12 requires the number to be STATED in the
//! cut brief — a model that never hears it cannot keep a deliberate long beat by asking. The wire message
//! is what these assert, because that is what the model reads.

use std::cell::RefCell;
use std::rc::Rc;

use naivepost::cut::{Cut, Fx, Seg};
use naivepost::{cut_clamp, cut_clamp_line, policy, suggest};
use naivepost::suggest::{DEAD_AIR_KEEP_SECONDS, DEAD_AIR_MAX_SECONDS};

#[allow(dead_code)] // every test binary compiles this whole module; an unused helper here is not a warning
mod common;
use common::{fx, seg};

fn cut_of(segs: &[Seg], effects: &[Fx]) -> Cut {
    Cut { segs: segs.to_vec(), fx: effects.to_vec(), ..Default::default() }
}

/// Entry 19: "Effects are clamped to the cut as applied and dropped under a second of survivor; only a
/// count reaches the log → stays after `finish` (the cut moved under them), but the count becomes a line the
/// user can act on." The count row stays byte for byte what §F3.12 quotes — it is pinned verbatim elsewhere —
/// and the names go in the row right after it, so both facts reach the log without either being weakened.
#[test]
fn sec_12_decisions_cut_and_effects_s19_the_clamp_names_which_effects_the_cut_dropped() {
    // Two ways an effect loses its home: a zoom on footage no kept scene covers (a point in the hole), and a
    // band whose surviving piece is under `MIN_SURVIVING_SECONDS`. Shapes copied from the clamp's own tests.
    let segs = [seg(0.0, 10.0), seg(20.0, 30.0)];
    let effects = [
        fx("zoom", 15.0, 0.0),   // a point in the hole between the two kept scenes: goes
        fx("volume", 29.5, 3.0), // 0.5 s of survivor, under the 1 s floor: goes
        fx("zoom", 2.0, 4.0),    // inside the first scene: stays
    ];
    let out = cut_clamp::clamp_to_cut(&cut_of(&segs, &effects));
    assert_eq!(out.dropped, 2, "two went: {:?}", out.kept);
    assert_eq!(out.kept.len(), 1, "the one inside footage stands");

    // The notes name each by the kind word the dropdown uses and the second it was at.
    assert_eq!(out.dropped_lines.len(), 2, "{:?}", out.dropped_lines);
    assert_eq!(out.dropped_lines[0], "zoom at 00:15", "{:?}", out.dropped_lines);
    assert_eq!(out.dropped_lines[1], "volume at 00:29", "{:?}", out.dropped_lines);

    // The count row is unchanged (§F3.12 quotes it whole) and the names follow it.
    let count = cut_clamp_line::dropped_log(&out).expect("something went, so there is a line");
    assert_eq!(count, cut_clamp::log_line(2), "the count row stays §F3.12's own sentence: {count}");
    let names = cut_clamp_line::dropped_names(&out).expect("and the names beside it");
    assert!(names.contains("zoom at 00:15"), "{names}");
    assert!(names.contains("volume at 00:29"), "{names}");
    // One row, comma-separated: the log interleaves the whole run and six separate rows would read as six
    // unrelated events.
    assert!(!names.contains('\n'), "one row: {names}");

    // Nothing dropped: no line at all, so a clean clamp stays quiet.
    let clean = cut_clamp::clamp_to_cut(&cut_of(&segs, &[fx("zoom", 2.0, 4.0)]));
    assert_eq!(clean.dropped, 0);
    assert!(cut_clamp_line::dropped_log(&clean).is_none(), "a clean clamp says nothing");
    assert!(cut_clamp_line::dropped_names(&clean).is_none());

    // A long list is capped rather than allowed to bury the rest of the log: the remainder is counted.
    let many: Vec<Fx> = (0..10).map(|i| fx("text", 100.0 + i as f64, 0.0)).collect();
    let flooded = cut_clamp::clamp_to_cut(&cut_of(&segs, &many));
    assert_eq!(flooded.dropped, 10, "{:?}", flooded.dropped_lines);
    let listed = cut_clamp_line::dropped_names(&flooded).expect("ten drops still get a row");
    assert!(listed.contains("... and 4 more"), "the tail is counted, not listed: {listed}");
    assert_eq!(listed.matches(",").count(), 6, "six named (five separators) then the counted tail: {listed}");

    // A stop is named as a stop, which is the word §9 refuses it with, not as a speed.
    let stopped = cut_clamp::clamp_to_cut(&cut_of(
        &segs,
        &[Fx { kind: "stop".into(), t: 15.0, dur: 0.0, rate: 0.0, ..Default::default() }],
    ));
    assert_eq!(stopped.dropped, 1);
    assert_eq!(stopped.dropped_lines[0], "stop at 00:15", "{:?}", stopped.dropped_lines);
}

/// The seconds as the brief spells them: whole, no decimals (`SESSION LENGTH` does the same).
fn stated_seconds() -> String {
    (DEAD_AIR_MAX_SECONDS as i64).to_string()
}

/// Entry 13: "Silences over eight seconds are cut out of kept clips, on the stated assumption the model
/// will not → … The cut brief states the number, so the model can keep a beat by asking."
#[test]
fn sec_12_decisions_cut_and_effects_s13_the_cut_brief_states_the_dead_air_number() {
    let msg = suggest::message(1200.0, 300.0, DEAD_AIR_MAX_SECONDS, "[0.0s-5.0s] A: hi");

    // The number is on the wire, not only in the code that applies it after the answer.
    assert!(
        msg.contains(&format!("SILENCES longer than {} seconds", stated_seconds())),
        "the brief names the dead-air ceiling: {msg}"
    );
    // And what the model can do about it: say so, and the beat stays. That half is the whole point of
    // stating it — a bare number would still leave a deliberate pause at the mercy of the walk-back.
    assert!(msg.contains("cut out afterwards"), "the rule is said: {msg}");
    assert!(msg.contains("deliberate"), "a deliberate beat is the model's to claim: {msg}");
    assert!(msg.contains("WHY"), "and it claims it in the field it already answers with: {msg}");

    // Order: length, target, dead air, timeline. The dead-air line sits with the other rules about WHAT
    // survives, before the material the model has to read; `prompt_assembly` puts the person's context in
    // front of all of it, which `suggest_flow` pins separately.
    let len = msg.find("SESSION LENGTH").expect("length present");
    let tgt = msg.find("KEEP between").expect("target present");
    let air = msg.find("SILENCES longer than").expect("dead air present");
    let head = msg.find("SESSION TIMELINE:").expect("timeline heading present");
    let body = msg.find("[0.0s-5.0s] A: hi").expect("timeline body present");
    assert!(len < tgt, "the finished length comes first: {msg}");
    assert!(tgt < air, "then what may survive inside a clip: {msg}");
    assert!(air < head, "and both before the material: {msg}");
    assert!(head < body, "then the timeline itself: {msg}");
    // Said once: a second copy would be a second home for the sentence.
    assert_eq!(msg.matches("SILENCES longer than").count(), 1, "{msg}");

    // No target changes nothing about this line: the ceiling holds whatever the length gate says.
    let no_target = suggest::message(1200.0, 0.0, DEAD_AIR_MAX_SECONDS, "x");
    assert!(
        no_target.contains(&format!("SILENCES longer than {} seconds", stated_seconds())),
        "the ceiling is stated even with no target: {no_target}"
    );
    assert!(no_target.contains("NO TARGET LENGTH"), "{no_target}");

    // And the rule the brief describes is still the one the walk-back applies: over the ceiling goes,
    // exactly the ceiling is a deliberate beat, under it costs nothing.
    let (kept, removed) = suggest::remove_dead_air(
        &[(10.0, 40.0)],
        &[(20.0, 20.0 + DEAD_AIR_MAX_SECONDS + 1.0)],
        DEAD_AIR_MAX_SECONDS,
        DEAD_AIR_KEEP_SECONDS,
    );
    assert_eq!(
        kept,
        vec![(10.0, 20.0 + DEAD_AIR_KEEP_SECONDS), (20.0 + DEAD_AIR_MAX_SECONDS + 1.0, 40.0)],
        "a silence past the stated ceiling loses everything but the beat"
    );
    assert!(removed > 0.0, "{removed}");

    let (_, exact) = suggest::remove_dead_air(
        &[(10.0, 40.0)],
        &[(20.0, 20.0 + DEAD_AIR_MAX_SECONDS)],
        DEAD_AIR_MAX_SECONDS,
        DEAD_AIR_KEEP_SECONDS,
    );
    assert_eq!(exact, 0.0, "exactly the stated ceiling is kept -- the brief's number is a ceiling");

    let (_, short) = suggest::remove_dead_air(
        &[(10.0, 40.0)],
        &[(20.0, 26.0)],
        DEAD_AIR_MAX_SECONDS,
        DEAD_AIR_KEEP_SECONDS,
    );
    assert_eq!(short, 0.0, "under the stated ceiling costs nothing");
}

/// The press path is what has to put the number on the wire, not a test calling `message` directly:
/// `user_turn` is the door `run_model` assembles the request with.
#[test]
fn sec_12_decisions_cut_and_effects_s13_the_press_path_puts_the_number_on_the_wire() {
    let turn = naivepost::ui::cut_suggest_model_run::user_turn(
        "a lecture about routers",
        1200.0,
        300.0,
        "[0.0s-5.0s] A: hi",
    );
    assert!(
        turn.contains(&format!("SILENCES longer than {} seconds", stated_seconds())),
        "the assembled request carries the ceiling: {turn}"
    );
    // Still one copy, and still after the person's own words.
    assert_eq!(turn.matches("SILENCES longer than").count(), 1, "{turn}");
    assert!(
        turn.find("a lecture about routers").expect("context")
            < turn.find("SILENCES longer than").expect("dead air"),
        "the person's context leads the request: {turn}"
    );
}

/// Entry 20: "The thinking-off retry and the web-tool withdrawal are logged → stays, AND a rewrite tells the
/// model its tools are gone." The log lines were already there; what was missing is the sentence in the NEXT
/// request. `Call::note_to_model` spells it, `opening` puts it in the conversation, and the scripted-`ask`
/// drive below proves the second attempt's messages really carry it — no socket.
#[test]
fn sec_12_decisions_cut_and_effects_s20_the_rewrite_tells_the_model_its_tools_are_gone() {
    use naivepost::ui::cut_suggest_model_run as model;
    use naivepost::ui::cut_suggest_watch::Call;

    // Nothing has happened yet: a first attempt says nothing about terms that never changed.
    let mut first = Call::new();
    assert_eq!(first.note_to_model(), None, "a first attempt announces nothing");

    // A rejection took the web tools away (the same count `offer()` acts on).
    let mut after_rejection = Call::new();
    after_rejection.note_rejected();
    let (_tools, withdrawn) = after_rejection.offer();
    assert!(withdrawn.is_some(), "the withdrawal still logs (§F3.12's line stays)");
    let said = after_rejection.note_to_model().expect("and now it is also announced");
    assert!(said.contains("web"), "{said}");
    assert!(!said.contains("thinking"), "a plain rejection does not change thinking: {said}");
    // ...and only once: the rounds inside that same attempt are asked with the same terms and need no repeat.
    assert_eq!(after_rejection.note_to_model(), None, "the sentence is said once per change");

    // An all-reasoning reply switched thinking off.
    let mut after_thinking = Call::new();
    let note = after_thinking.note_reply_text("");
    assert_eq!(note.as_deref(), Some(naivepost::cut_suggest::THINKING_RETRY_LOG));
    let said = after_thinking.note_to_model().expect("the switch is announced too");
    assert!(said.contains("no thinking"), "{said}");
    assert!(!said.contains("web"), "thinking alone does not touch the tools: {said}");

    // Both at once reads as one sentence with both halves.
    let mut both = Call::new();
    both.note_reply_text("");
    both.note_rejected();
    let _ = both.offer();
    let said = both.note_to_model().expect("both changes");
    assert!(said.contains("web") && said.contains("no thinking"), "{said}");

    // `opening` puts the announcement in the conversation, before the complaint it accompanies.
    let messages = model::opening("SYS", "USER", Some("COMPLAINT"), Some("NOTE"));
    assert_eq!(messages.len(), 4, "system, user, note, complaint");
    let text_of = |m: &naivepost::exchanges::Message| match &m.parts[0] {
        naivepost::exchanges::Part::Text(t) => t.clone(),
        other => panic!("expected a text part, got {other:?}"),
    };
    assert_eq!(text_of(&messages[2]), "NOTE");
    assert_eq!(text_of(&messages[3]), "COMPLAINT");
    // With nothing to announce, the conversation is exactly what it was before this rule existed.
    assert_eq!(model::opening("SYS", "USER", Some("COMPLAINT"), None).len(), 3);
    assert_eq!(model::opening("SYS", "USER", None, None).len(), 2);
}

/// The same proof from the far end: a scripted `ask` that records every message it is handed, so the sentence
/// is seen in the SECOND attempt's request rather than in the function meant to build it.
struct Recording {
    turns: std::collections::VecDeque<naivepost::tool_loop::Turn>,
    /// One entry per ask: the text of every message in the order the model reads them.
    conversations: Rc<RefCell<Vec<Vec<String>>>>,
}

impl Recording {
    fn new(turns: Vec<naivepost::tool_loop::Turn>) -> Self {
        Self { turns: turns.into_iter().collect(), conversations: Rc::new(RefCell::new(Vec::new())) }
    }

    fn ask(&mut self, messages: &[naivepost::exchanges::Message]) -> Result<naivepost::tool_loop::Turn, String> {
        self.conversations.borrow_mut().push(
            messages
                .iter()
                .map(|m| match &m.parts[0] {
                    naivepost::exchanges::Part::Text(t) => t.clone(),
                    other => format!("{other:?}"),
                })
                .collect(),
        );
        Ok(self.turns.pop_front().unwrap_or_else(|| naivepost::tool_loop::Turn {
            content: String::new(),
            calls: Vec::new(),
            finish: false,
        }))
    }
}

/// A throwaway `.naivepost` with words for both cameras, plus the settings folder holding the shipped prompt
/// wording — the shape `run_model` needs to build a real request. Copied from `tests/sec_05_cut_8_suggest_wired.rs`.
struct Wire {
    root: std::path::PathBuf,
    tree: naivepost::layout::Tree,
    project: naivepost::project::Project,
    paths: naivepost::settings::Paths,
}

impl Wire {
    fn fresh(tag: &str) -> Self {
        let root = common::temp_dir(tag);
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
            naivepost::requests::write_words(
                &tree,
                source,
                &naivepost::requests::WordsDoc { text: "w0 w1 w2".to_string(), words: words.clone() },
            )
            .expect("words.json");
            naivepost::requests::write_aligned(
                &tree,
                source,
                &naivepost::requests::AlignedDoc { words },
            )
            .expect("words.aligned.json");
        }
        Self { root, tree, project: naivepost::project::load(&common::fixture_dir()).expect("fixture loads"), paths }
    }
}

impl Drop for Wire {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn tool(name: &str, args: &str) -> naivepost::exchanges::ToolCall {
    naivepost::exchanges::ToolCall { name: name.to_string(), args: args.to_string(), result: String::new() }
}

fn add_seg(start: f64, end: f64) -> naivepost::exchanges::ToolCall {
    tool("add_segment", &format!("{{\"start\": {start}, \"end\": {end}, \"why\": \"the point\"}}"))
}

/// Entry 20 again from the far end of the wire: the SECOND attempt's messages carry the announcement, and it
/// arrives BEFORE the complaint. Scripted answers, no socket (the `sec_05_cut_8_suggest_wired.rs` arrangement):
/// attempt 1 places footage but never finishes (an answer that did not become a cut — §8's rejection), so the
/// web tools come off and attempt 2 is asked with the sentence in it.
#[test]
fn sec_12_decisions_cut_and_effects_s20_the_second_attempt_carries_the_announcement_on_the_wire() {
    use naivepost::ui::cut_suggest_model_run as model;
    let w = Wire::fresh("announce");
    let mut script = Recording::new(vec![
        // Attempt 1: placed a segment too short to be a scene (under `cutpass::MIN_SCENE_SECONDS`) and called
        // `finish_cut`, so the attempt ENDS with an answer that is not a cut — §8's rejection, which is what
        // withdraws the web tools for the attempt after it.
        naivepost::tool_loop::Turn {
            content: String::new(),
            calls: vec![add_seg(2.0, 2.3), tool("finish_cut", "{}")],
            finish: true,
        },
        // Attempt 2's first round places a segment the tool REFUSES (under `cutpass::MIN_SCENE_SECONDS`), so
        // the refusal round that follows inside this attempt is the one that must carry the announcement.
        naivepost::tool_loop::Turn {
            content: String::new(),
            calls: vec![add_seg(2.0, 2.3)],
            finish: false,
        },
        // Attempt 2 then answers with a cut that fits and finishes.
        naivepost::tool_loop::Turn {
            content: String::new(),
            calls: vec![add_seg(2.0, 16.0), add_seg(32.0, 46.0), tool("finish_cut", "{}")],
            finish: true,
        },
    ]);
    let asks = Rc::clone(&script.conversations);
    let cut = Cut {
        segs: vec![seg(0.0, 20.0), seg(30.0, 50.0)],
        ..Default::default()
    };
    let made = model::run_model(&w.tree, &w.project, &cut, 30.0, Some(&w.paths), |messages, _tools| {
        script.ask(messages)
    });
    assert!(
        made.log.iter().any(|line| *line == naivepost::cut_suggest::WEB_TOOLS_WITHDRAWN_LOG),
        "the withdrawal logged, as before: {:?}",
        made.log
    );
    // Every ask INSIDE an attempt is asked with that attempt's terms, so collapse repeats before looking.
    let by_attempt: Vec<Vec<String>> = {
        let mut out: Vec<Vec<String>> = Vec::new();
        for conversation in asks.borrow().iter() {
            if out.last().map(|last| last != conversation).unwrap_or(true) {
                out.push(conversation.clone());
            }
        }
        out
    };
    assert!(by_attempt.len() >= 2, "there was a second attempt to announce to: {by_attempt:?}");
    let later = by_attempt.iter().skip(1).find(|conversation| {
        conversation.iter().any(|part| part.contains("no longer available to you"))
    });
    let said = later.unwrap_or_else(|| {
        panic!("every later attempt should tell the model its tools are gone: {by_attempt:?}")
    });
    // And it goes BEFORE the complaint it belongs with, in the same attempt's message list. §8 orders the
    // faults worst first — a segment count is cheaper to fix than a length — so the shortest-count fault leads
    // the sentence `correction_from` wraps.
    let complaint_at = said.iter().rposition(|part| part.contains("segments is under the"));
    assert!(complaint_at.is_some(), "the refused answer came back as a correction: {said:?}");
    let note_at = said
        .iter()
        .rposition(|part| part.contains("no longer available to you"))
        .expect("the note is there");
    assert!(note_at < complaint_at.unwrap(), "the terms of the call precede the complaint about the last one: {said:?}");
}

/// Entry 21: "The target length is a regex over free text with no fallback: 'half an hour' silently becomes
/// no target, changing the prompt, both count gates and the length gate at once → `P.policy.targetLengthSeconds`,
/// derived by F0.7's `set_policy`, shown in the policy form." The string cases: what used to be a silent 0.
#[test]
fn sec_12_decisions_cut_and_effects_s21_target_length_is_derived_and_shown() {
    use naivepost::policy_length::{self, Reading};
    let read = |words: &str| policy_length::target_length_of(words);

    // The §12 example itself, and the plain digit forms the old regex missed by spacing.
    assert_eq!(read("in half an hour").seconds, 1800.0);
    assert_eq!(read("Keep it to 45 min").seconds, 2700.0);
    assert_eq!(read("90 seconds max").seconds, 90.0);
    assert_eq!(read("about twenty minutes of talk").seconds, 1200.0);
    assert_eq!(read("2 hours of lecture").seconds, 7200.0);
    assert_eq!(read("cut it to 1h").seconds, 3600.0);
    assert_eq!(read("roughly 30m").seconds, 1800.0);
    assert_eq!(read("make it 1h20").seconds, 4800.0);
    assert_eq!(read("an hour and a half").seconds, 5400.0);

    // The §12 sentence itself: "no length mentioned" still leaves no target — the word "length" is a mention
    // of duration, so `mentioned` is true and the log will say it went unread, but the number stays 0 = none.
    assert_eq!(read("no length mentioned").seconds, 0.0);
    // A context with no duration vocabulary at all is the honest `NONE`: nothing claimed, nothing to report.
    let none = read("punchy jump cuts, no intro");
    assert_eq!(none, Reading { seconds: 0.0, mentioned: false }, "{none:?}");
    // A length that is mentioned but not spelled in seconds is the case that must NOT read as "no target":
    // it is a real gap, and §12's complaint is that a gap like this was invisible.
    let vague = read("make it the length of a train journey");
    assert_eq!(vague.seconds, 0.0);
    assert!(vague.mentioned, "a duration was mentioned, so the log has to say it went unread");

    // The field it lands in goes through `policy::apply`, so the hand-set rule cannot be bypassed.
    let mut policy = naivepost::project::Policy::default();
    assert_eq!(policy.target_length_seconds.value, 0.0, "§10's default is none");
    let derived = policy::read_target_length(&mut policy, "keep it to 45 min");
    assert!(derived.target_read, "the read landed: {:?}", derived.applied);
    assert_eq!(policy.target_length_seconds.value, 2700.0);
    assert_eq!(
        policy.target_length_seconds.origin,
        naivepost::project::Origin::Model,
        "it is a derivation, so a later hand-set value outranks it"
    );
    // A person's own number survives the next read, exactly as S4 holds for a model's proposal.
    policy.target_length_seconds.origin = naivepost::project::Origin::User;
    let again = policy::read_target_length(&mut policy, "make it an hour");
    assert_eq!(
        again.applied.first().map(|(_, outcome)| outcome.clone()),
        Some(naivepost::policy::Applied::KeptUser)
    );
    assert_eq!(policy.target_length_seconds.value, 2700.0, "the person's length stands");
}
