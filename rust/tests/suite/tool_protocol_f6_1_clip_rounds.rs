//! §F6.1's round leg for the cut's own jobs: a model that ANSWERS BY CALLING A TOOL gets its call run, the round
//! line is printed, and the call's payload comes back as the pass's own answer.
//!
//! Until this round the three per-clip passes each made ONE chat call and read `tool_calls` off the single reply,
//! so §2's rounds existed only in other flows. Each scenario here scripts a DIFFERENT reply per request (the
//! shared fake's point), which is the only way to show a conversation happened: a round line can only print if a
//! further request left the program.
//!
//! Three tests in one binary on purpose: they drive [`naivepost::clip_rules_tools_leg`] directly rather than a
//! pass's own `ask`, so they touch no process-wide settings and cannot race each other.

#![allow(dead_code)]

#[path = "tool_protocol_f6_1_clip_rounds_shared.rs"]
mod shared;

use naivepost::clip_rules_tools_leg;
use naivepost::exchanges::{Message, Part};
use naivepost::tool_loop;
use naivepost::tools::clips::Clips;
use naivepost::tools::Tool;
use serde_json::json;
use shared::*;

/// The two messages every §3.7 ask carries.
fn ask_messages() -> Vec<Message> {
    vec![
        Message {
            role: "system".to_string(),
            parts: vec![Part::Text("the rules".to_string())],
        },
        Message {
            role: "user".to_string(),
            parts: vec![Part::Text("CLIP 1: 10.0 s long".to_string())],
        },
    ]
}

/// The batch: two clips of ten seconds each, in clip-relative space (what `batch_of` builds from `(n, length)`).
fn two_clips() -> Clips {
    Clips::new(&[(1, 0.0, 10.0), (2, 10.0, 10.0)])
}

/// Drive one conversation: the loop's `ask` closure answers from a scripted list, one reply per round, and counts
/// the rounds it was asked for.
struct Scripted {
    replies: Vec<tool_loop::Turn>,
    index: usize,
    rounds: usize,
    /// Every request's message list as the loop handed it over, so a test can see the conversation grow.
    seen: Vec<Vec<String>>,
}

impl Scripted {
    fn new(replies: Vec<tool_loop::Turn>) -> Scripted {
        Scripted {
            replies,
            index: 0,
            rounds: 0,
            seen: Vec::new(),
        }
    }

    fn ask(&mut self) -> impl FnMut(&[Message], &[Tool]) -> Result<tool_loop::Turn, String> + '_ {
        move |conversation: &[Message], _tools: &[Tool]| {
            self.rounds += 1;
            self.seen.push(
                conversation
                    .iter()
                    .map(|message| message.role.clone())
                    .collect::<Vec<String>>(),
            );
            let turn = self
                .replies
                .get(self.index)
                .cloned()
                .unwrap_or_else(|| tool_loop::Turn {
                    content: String::new(),
                    calls: Vec::new(),
                    finish: false,
                });
            self.index += 1;
            Ok(turn)
        }
    }
}

/// §2 S2 for captions: a round that CALLS is a round, so the line exists, the args are quoted raw, and the call
/// survives as the caption the pass would place.
#[test]
fn f6_1_s2_a_captions_round_that_calls_is_logged_and_its_call_survives() {
    naivepost::ui::window::clear_logs_for_test();
    let caption =
        json!({"clip": 2, "start": 1.0, "end": 4.0, "text": "the line the model asked for"});
    let mut scripted = Scripted::new(vec![
        // Round 1: look first, then ask for the caption — §3.7's "so a zoom can be aimed" read, plus the work.
        tool_loop::Turn {
            content: String::new(),
            calls: vec![
                naivepost::exchanges::ToolCall {
                    name: "get_frames".to_string(),
                    args: json!({"clip": 2, "at": 2.0}).to_string(),
                    result: String::new(),
                },
                naivepost::exchanges::ToolCall {
                    name: "add_caption".to_string(),
                    args: caption.to_string(),
                    result: String::new(),
                },
            ],
            finish: false,
        },
        // Round 2: nothing more to ask.
        tool_loop::Turn {
            content: String::new(),
            calls: Vec::new(),
            finish: true,
        },
    ]);
    let mut batch = two_clips();
    let run = clip_rules_tools_leg::run_over_batch(
        "captions",
        ask_messages(),
        &caption_tools(),
        &mut batch,
        &mut scripted.ask(),
    );
    // S7: a flow hands these lines to its own log sink; the check reads them off the run, which is where they
    // were put before any sink saw them, so nothing here depends on a window existing.
    let said_lines: Vec<String> = run.run.logs.clone();
    let lines_with = |needle: &str| -> Vec<String> {
        said_lines
            .iter()
            .filter(|line| line.contains(needle))
            .cloned()
            .collect::<Vec<String>>()
    };
    assert_eq!(
        scripted.rounds, 2,
        "a round that called cost exactly one more round"
    );

    // §2 S2's own wording, as the program writes it (an em dash, `round i of 8`, name directly followed by the
    // raw arguments JSON).
    let rounds = lines_with("round 1 of");
    assert_eq!(rounds.len(), 1, "one round line: {rounds:?}");
    assert!(
        rounds[0].starts_with(">>> captions: round 1 of 8 \u{2014} the model asked for "),
        "the round line names the step, the round and the budget: {}",
        rounds[0]
    );
    assert!(
        rounds[0].contains("get_frames{"),
        "the call is named as `name{{args}}`: {}",
        rounds[0]
    );
    assert!(
        rounds[0].contains("add_caption{"),
        "both calls of the round, joined: {}",
        rounds[0]
    );
    assert!(
        rounds[0].contains("\"text\":\"the line the model a"),
        "the raw arguments ride in the line, cut at 60 bytes: {}",
        rounds[0]
    );
    assert!(
        rounds[0].contains('\u{2026}'),
        "the cut is marked, so a reader knows the args were truncated: {}",
        rounds[0]
    );
    // S7: each call's result is noted too, so the exchange page can show what the tool answered.
    let notes = lines_with("[tool call get_frames(");
    assert_eq!(notes.len(), 1, "the read is noted: {notes:?}");
    assert!(
        notes[0].contains("frames"),
        "the note carries the tool's answer, which is the frames: {}",
        notes[0]
    );

    // The read was ANSWERED about the clip the model named, not refused: clip 2's frames are clip 2's own seconds.
    let said = lines_with("[tool call get_frames(");
    assert!(
        !said[0].contains("\"error\""),
        "a `get_frames` for a clip in the batch is never an error: {}",
        said[0]
    );

    // The caption call came back as the pass's own call shape, and it is in the batch the pass places from.
    let asked: Vec<naivepost::cut_captions::Call> = run
        .job_calls(&[Tool::AddCaption])
        .iter()
        .filter_map(|call| {
            let args = serde_json::from_str::<serde_json::Value>(&call.args).ok()?;
            naivepost::captions_ask::call_of_args(&args)
        })
        .collect();
    assert_eq!(asked.len(), 1, "one caption call survived: {asked:?}");
    assert_eq!(asked[0].clip, 2, "for the clip the model named");
    assert_eq!(
        asked[0].text, "the line the model asked for",
        "with the model's own words"
    );
    let placed = naivepost::cut_captions::place(&[(1, 10.0), (2, 10.0)], &asked);
    assert!(
        matches!(placed, naivepost::cut_captions::Reply::Accepted(_)),
        "and it is placeable: {placed:?}"
    );
    assert_eq!(
        run.run.outcome,
        tool_loop::Outcome::Finished {
            asked_finish: false
        },
        "the flow ended on finish"
    );
}

/// §F3.11 / §2 S2 for decorations: the model asks `get_frames` to aim a zoom, is shown the frames, and then asks
/// for the zoom — which lands through `cut_effects_pass::place`.
#[test]
fn f6_1_s2_a_decoration_round_reads_the_frames_it_needs_then_asks() {
    naivepost::ui::window::clear_logs_for_test();
    let mut scripted = Scripted::new(vec![
        tool_loop::Turn {
            content: String::new(),
            calls: vec![naivepost::exchanges::ToolCall {
                name: "get_frames".to_string(),
                args: json!({"clip": 2}).to_string(),
                result: String::new(),
            }],
            finish: false,
        },
        tool_loop::Turn {
            content: String::new(),
            calls: vec![naivepost::exchanges::ToolCall {
                name: "add_effect".to_string(),
                args: json!({"clip": 2, "kind": "zoom", "start": 1.0, "end": 3.0}).to_string(),
                result: String::new(),
            }],
            finish: false,
        },
        tool_loop::Turn {
            content: String::new(),
            calls: Vec::new(),
            finish: true,
        },
    ]);
    let mut batch = two_clips();
    let run = clip_rules_tools_leg::run_over_batch(
        "effects",
        ask_messages(),
        &effect_tools(),
        &mut batch,
        &mut scripted.ask(),
    );
    // S7: a flow hands these lines to its own log sink; the check reads them off the run, which is where they
    // were put before any sink saw them, so nothing here depends on a window existing.
    let said_lines: Vec<String> = run.run.logs.clone();
    let lines_with = |needle: &str| -> Vec<String> {
        said_lines
            .iter()
            .filter(|line| line.contains(needle))
            .cloned()
            .collect::<Vec<String>>()
    };
    assert_eq!(scripted.rounds, 3, "read, ask, then finish: three rounds");

    // The read went to clip 2, and its answer named clip 2 — the model was shown what it asked to see.
    let read = lines_with("[tool call get_frames(");
    assert_eq!(read.len(), 1, "the read is noted: {read:?}");
    assert!(
        read[0].contains("\"clip\":2"),
        "the frames answer names the clip it read: {}",
        read[0]
    );
    assert!(
        !read[0].contains("\"error\""),
        "clip 2 is in the batch, so the read is not refused: {}",
        read[0]
    );
    // The second round's line exists because there WAS a second round.
    let second = lines_with("round 2 of");
    assert_eq!(
        second.len(),
        1,
        "the round after the read is logged: {second:?}"
    );
    assert!(
        second[0].contains("add_effect{"),
        "and it names the decoration asked for: {}",
        second[0]
    );

    // The decoration is the pass's own call shape and places with the app's defaults, not the model's numbers.
    let calls: Vec<naivepost::cut_effects_pass::Call> = run
        .job_calls(&[Tool::AddEffect])
        .iter()
        .filter_map(|call| {
            let args = serde_json::from_str::<serde_json::Value>(&call.args).ok()?;
            naivepost::effects_ask::call_of_args(&args)
        })
        .collect();
    assert_eq!(calls.len(), 1, "one decoration survived: {calls:?}");
    let placed = naivepost::cut_effects_pass::place(&two_clips(), &calls);
    assert!(
        placed.usable(),
        "a decoration on clip 2 places: {:?}",
        placed.fault
    );
    assert_eq!(
        placed.effects.len(),
        1,
        "one effect landed: {:?}",
        placed.effects
    );
    assert_eq!(
        placed.effects[0].kind, "zoom",
        "the zoom the model asked for"
    );
}

/// §2 S4: a model that keeps calling never has its last round's work thrown away — the loop STOPS at the budget,
/// says §2's line, and this call answers nothing.
#[test]
fn f6_1_s4_a_loop_that_never_stops_costs_exactly_the_round_budget() {
    naivepost::ui::window::clear_logs_for_test();
    // A scripted endless loop must actually be endless: one reply per round, or the list running out hands back an
    // empty turn and §3 finishes the flow instead of §4 exhausting it.
    let budget = tool_rounds();
    let endless = vec![
        tool_loop::Turn {
            content: String::new(),
            calls: vec![naivepost::exchanges::ToolCall {
                name: "get_frames".to_string(),
                args: json!({"clip": 1}).to_string(),
                result: String::new(),
            }],
            finish: false,
        };
        (budget as usize) + 4
    ];
    let mut scripted = Scripted::new(endless);
    let mut batch = two_clips();
    let run = clip_rules_tools_leg::run_over_batch(
        "effects",
        ask_messages(),
        &effect_tools(),
        &mut batch,
        &mut scripted.ask(),
    );
    // S7: a flow hands these lines to its own log sink; the check reads them off the run, which is where they
    // were put before any sink saw them, so nothing here depends on a window existing.
    let said_lines: Vec<String> = run.run.logs.clone();
    let lines_with = |needle: &str| -> Vec<String> {
        said_lines
            .iter()
            .filter(|line| line.contains(needle))
            .cloned()
            .collect::<Vec<String>>()
    };
    assert_eq!(
        run.run.outcome,
        tool_loop::Outcome::Exhausted,
        "the budget ended the flow"
    );
    assert_eq!(
        scripted.rounds as u32,
        budget + 1,
        "P.eng.llmToolRounds rounds ran their tools, and the budget round was asked for but delivered nothing: \
         the prototype ran the last round's calls and threw the results away, which is the defect this guards"
    );
    // §2 S4's line, in the program's own wording.
    let said = lines_with("still calling tools after");
    assert_eq!(said.len(), 1, "the exhaustion line is said once: {said:?}");
    assert!(
        said[0].starts_with("!!! effects:"),
        "it names the step: {}",
        said[0]
    );
    assert!(
        said[0].contains("the step gets no answer from this call"),
        "and §2's ending: {}",
        said[0]
    );
    // The step gets NO answer: nothing was placed, and the last round's calls were not run.
    assert!(
        run.job_calls(&[Tool::AddEffect]).is_empty(),
        "a loop that never stops answers nothing"
    );
    // And it has no PROSE either: `prose` is how a caller reads back a §2 answer in words, so an exhausted loop
    // must hand back None rather than some earlier round's half-sentence.
    assert_eq!(
        run.prose(),
        None,
        "an exhausted conversation ends without a prose answer"
    );
    // The other shape of the same helper: a round that CALLS and then answers in WORDS leaves that document
    // readable, which is what F3.9/F3.10/F3.11 read back when a server looks first and writes after.
    let mut prose_round = Scripted::new(vec![tool_loop::Turn {
        content: "[[0,1,\"ok\"]]".to_string(),
        calls: vec![naivepost::exchanges::ToolCall {
            name: "get_frames".to_string(),
            args: json!({"clip": 1}).to_string(),
            result: String::new(),
        }],
        finish: true,
    }]);
    let mut batch = two_clips();
    let said = clip_rules_tools_leg::run_over_batch(
        "effects",
        ask_messages(),
        &effect_tools(),
        &mut batch,
        &mut prose_round.ask(),
    );
    assert_eq!(
        said.prose().as_deref(),
        Some("[[0,1,\"ok\"]]"),
        "the round's own words are the answer, not the silence around its call"
    );
    let placed = naivepost::cut_effects_pass::place(&two_clips(), &[]);
    assert!(placed.effects.is_empty(), "so the cut gains no decoration");
    // The budget is spent BEFORE the last round's work: the run only carries `budget` round lines, and the round
    // after the budget never ran a tool.
    assert_eq!(
        lines_with("round ").len() as u32,
        budget,
        "one line per round, no more"
    );
}

/// The mode the §3.7 asks send: captions thinks, speed and decorations do not — pinned here so a change to the
/// loop's body builder cannot quietly flip it.
#[test]
fn f6_1_s2_the_modes_the_clip_passes_ask_in_are_the_ones_their_prompts_were_written_for() {
    let effects = naivepost::effects_ask::chat_body("m", "s", "u").expect("an effects body");
    assert!(
        effects.contains("\"enable_thinking\":false"),
        "decorations ask without thinking"
    );
    let speeds = naivepost::speeds_ask::chat_body("m", "s", "u").expect("a speeds body");
    assert!(
        speeds.contains("\"enable_thinking\":false"),
        "speeds ask without thinking"
    );
    // The captions ask builds its body inside the ask, so its flag is read from the settings-reading path by
    // `tests/f3_9_s3_a_…`; here the check is that the two Execute-mode passes did not drift to thinking.
}
