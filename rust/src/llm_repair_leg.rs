//! §3's content repair on the wire: an answer this app cannot use is asked for again, repaired.

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

pub type Builder<'a> = &'a dyn Fn(&[Message], bool) -> Result<String, String>;

pub type Verdict<'a> = &'a dyn Fn(&llm_request::Reply) -> Option<String>;

/// Repair an answer the flow has ALREADY received: no first dial here, because the flow made it, and made it ONCE
/// — which is what §F3.9 counts in `requests.tsv`, under §2 S5's refusal rule.
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

pub fn repair_log(step: &str, problem: &str) -> String {
    format!(">>> {step}: the answer could not be used ({problem}) -- asked again")
}
