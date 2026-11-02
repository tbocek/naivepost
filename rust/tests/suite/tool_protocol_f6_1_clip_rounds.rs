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

use naivepost::exchanges::{Message, Part};
use naivepost::tool_loop;
use naivepost::tools::clips::Clips;
use naivepost::tools::Tool;

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
