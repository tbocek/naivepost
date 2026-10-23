//! §3's content repair on the wire: an answer this app cannot use is asked for again, repaired.
//!
//! [`crate::llm_retry`] holds §3's four repairs as pure decisions — `noAnswer`, `cutOff`, `thinkAgain`,
//! `retryTurn` — and [`crate::llm_retry_leg`] runs §3's *patience*, which retries a call that failed on the
//! wire. Neither touched an answer that arrived whole and was unusable, so until now such a reply was resent
//! **unchanged**: same messages, same thinking, same request, guaranteed same answer. §3 says otherwise — the
//! correction goes back into the conversation (`retryTurn`), an empty answer turns thinking off for the retry
//! (`thinkAgain`), and it happens ONCE, because "the first failure only" is what keeps a stubborn model from
//! costing two requests per batch forever.
//!
//! # Why the door takes an answer already in hand
//!
//! One door, [`ask_repair_after`], and it does not send the first request: every chat ask in the app dials its
//! own first round through [`crate::llm_tool_refusal_leg::dial_honouring_refusal`], because that leg owns §2 S5's
//! refusal memory and the flow counts that request once in `requests.tsv`. A door that dialled the first ask too
//! would have no caller — so this one takes the reply the flow got and spends exactly ONE more request on it.
//!
//! # Why the door takes a parse, not a status
//!
//! The failure this repairs is not an HTTP failure: the server answered 200 and wrote prose where JSON belongs,
//! or wrote its reasoning and nothing else. So the caller hands in `problem` — its own verdict on the answer,
//! already worded as the correction §3 wants sent (`llm_retry::repair` builds that wording) — and this module
//! decides only what §3 decides: whether one more ask is owed, how to rebuild the conversation, and whether the
//! retry may still think.
//!
//! # Tools
//!
//! §3's last line — "with tools these become tool results" — is [`llm_retry::retry_turn`]'s `tools_active`
//! flag: a model that is mid-round expects its calls answered, so the correction arrives as a `tool` message
//! rather than a new instruction from the user. A flow passes true when its request offered tools; the shape of
//! the message list is the flow's business, this only picks the role.

use crate::exchanges::Message;
use crate::llm_request;
use crate::llm_retry;

/// One repaired ask: the reply that came back, whether the retry happened, and what was said about it.
#[derive(Debug)]
pub struct Repaired {
    /// The chat envelope of the LAST reply seen — the repaired one when there was one. A caller parses its own
    /// answer out of this exactly as it parsed the first.
    pub body: String,
    pub content_type: String,
    /// Whether a second request went out. §3's repair is once-only, so this is also "the repair was spent".
    pub retried: bool,
}

impl Repaired {
    /// Read the reply the way the caller reads any answer.
    pub fn reply(&self) -> Result<llm_request::Reply, String> {
        llm_request::reply_from(&self.content_type, &self.body)
    }
}

/// How a flow sends one body and gets back what the server wrote: `(content_type, body)`. Named because clippy
/// is right that the inline type is hard to read, and both doors below take it.
pub type Dialer<'a> = &'a dyn Fn(&str) -> Result<(String, String), String>;

/// How a flow builds the body for a message list and a thinking choice. `false` is what
/// [`llm_retry::think_again`] turning thinking off means (§1: the switch follows the mode, so that is
/// `Mode::Execute`).
pub type Builder<'a> = &'a dyn Fn(&[Message], bool) -> Result<String, String>;

/// The flow's verdict on an answer: `Some(sentence)` = unusable, and that sentence is the correction §3 sends;
/// `None` = usable. Only the flow can say this — it owns the parse (this step's document reader, a tool schema,
/// whatever the job checks) — while this module only knows how to ask again.
pub type Verdict<'a> = &'a dyn Fn(&llm_request::Reply) -> Option<String>;

/// Repair an answer the flow has ALREADY received: no first dial here, because the flow made it, and made it ONCE
/// — which is what §F3.9 counts in `requests.tsv`, under §2 S5's refusal rule.
///
/// * `verdict` — the flow's read of the answer it got: `Some(sentence)` = unusable, and that sentence is what the
///   model is told.
/// * `build` — build the body for a message list and a thinking choice. `Mode::Execute` is what
///   [`llm_retry::think_again`] turning false means (§1: the switch follows the mode).
/// * `dial` — send one body, get `(content_type, body)` back. The flow owns the wire; this owns the repair.
/// * `tools_active` — §3's "with tools these become tool results".
///
/// The wait is not slept here: §3's 2 s short retry belongs to [`crate::llm_retry_leg::chat`], which every flow's
/// `dial` already rides, so a repair costs one extra request and no second patience ladder.
///
/// `verdict(first)` returning `None` is a GUARD, not a path: a flow reaches this door only after its own read of
/// that reply failed, so a `None` here means the caller contradicts itself, and the honest answer is an error
/// rather than a silent resend of a question that was already answered.
#[allow(clippy::too_many_arguments)]
pub fn ask_repair_after(
    step: &str,
    log: &dyn Fn(&str),
    messages: Vec<Message>,
    thinking: bool,
    tools_active: bool,
    verdict: Verdict<'_>,
    first: &llm_request::Reply,
    dial: Dialer<'_>,
    build: Builder<'_>,
) -> Result<Repaired, String> {
    let Some(problem) = verdict(first) else {
        return Err(format!("{step}: the answer was usable, nothing to repair"));
    };
    let answer = first.answer();
    // §3's `thinkAgain`: an EMPTY answer means the thinking budget was spent with no words, and asking the same
    // question the same way answers it the same way, so that retry sends thinking off. A malformed answer keeps
    // thinking — that model is writing and getting it wrong.
    let next_thinking = llm_retry::think_again(thinking, answer);
    // §3's `retryTurn`: the rejected answer goes back as an assistant turn UNLESS it was empty (some servers
    // refuse a conversation holding a blank assistant turn), then the correction — as a `tool` result when the
    // flow is mid-round, as a user turn otherwise.
    let grown = llm_retry::retry_turn(messages, answer, &problem, tools_active);
    log(&repair_log(step, &problem));
    let second = build(&grown, next_thinking)?;
    let (second_type, second_body) = dial(&second)?;
    Ok(Repaired {
        body: second_body,
        content_type: second_type,
        retried: true,
    })
}

/// §3's repair line: what the answer was found to be, and that it was asked for again. The prototype logged
/// nothing here at all, which is why a repaired batch looked identical to a normal one in the log; the shape
/// follows [`crate::llm_retry::waited_log`] (`>>> <step>: …`) rather than inventing a third prefix.
pub fn repair_log(step: &str, problem: &str) -> String {
    format!(">>> {step}: the answer could not be used ({problem}) -- asked again")
}
