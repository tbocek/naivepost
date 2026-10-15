//! §2 S5's live half, shared: one dial that honours a server's refusal of the `tools` field.
//!
//! Four flows already honoured it (narrate, upload text, joins, policy derivation), each with its own copy of
//! "the first round failed and it names tools → log [`crate::tool_loop::refused_log`] once, rebuild the identical
//! body without the field, redial". The steps that had no copy — the transcript fixer, the retake pool and the
//! cut's own ask — lost the job on such a server, and re-offered the field on every retry, which is precisely the
//! prototype behaviour §2 says MUST NOT be kept ("a step retrying with tools re-offers the field and re-logs the
//! line each time"). This module is that rule in one place so a new flow cannot miss it.
//!
//! Two closures rather than a body string because the two halves belong to different layers: only the `tools`
//! field changes on a refusal, while everything else about the request (its `Mode`, its message list, its model)
//! belongs to the caller. So the caller supplies how to BUILD a body for a given offer and how to DIAL one, and
//! this door only decides what to build and whether to try again.

use crate::llm_tools_offer;
use crate::tool_loop;
use crate::tools::Tool;

/// Dial one chat request, honouring §2's refusal rule: a non-stop error on the FIRST round that names the
/// `tools` field means the identical request goes out again WITHOUT the field, said once.
///
/// * `build(offer)` — the body for this offer; `None` means "no `tools` key at all", which is what a refused
///   server gets. Called twice on a refusal, with `Some(offered)` then `None`, so the second body differs by
///   exactly that field.
/// * `dial(body)` — send it. Generic so `llm_retry_leg::chat`'s `Sent` and any flow's own reply shape both fit.
/// * `log(line)` — where the refusal line goes. Every flow already carries its own sink (the run bar, the press's
///   log vector, the window's log), and §2 wants the sentence in the same place as the step's other lines.
///
/// A remembered refusal is silent: [`crate::llm_tools_offer::tools_for`] hands back `None` before the first dial,
/// so a step's second, third and nth attempt neither re-offer the field nor repeat the line. A dead server is not
/// a refusal — that judgement stays [`crate::tool_loop::retry_without_tools`]'s, reached through
/// [`crate::llm_tools_offer::honour_refusal`].
/// How a flow builds the body for one offer: `None` means "no `tools` key at all". Named because clippy is right
/// that the inline type is hard to read, and every caller writes it.
pub type BodyBuilder<'a> = &'a dyn Fn(Option<&[Tool]>) -> Result<String, String>;

/// How a flow sends one body. Generic in the reply so `llm_retry_leg::chat`'s `Sent` and any flow's own shape fit.
pub type Dialer<'a, S> = &'a dyn Fn(&str) -> Result<S, String>;

pub fn dial_honouring_refusal<S>(
    step: &str,
    server: &str,
    offered: Option<&[Tool]>,
    build: BodyBuilder<'_>,
    dial: Dialer<'_, S>,
    log: &dyn Fn(&str),
) -> Result<S, String> {
    // The offer as this SERVER allows it: nothing at all once it has said no to the field.
    let offer = offered.and_then(|tools| llm_tools_offer::tools_for(server, tools));
    let body = build(offer.as_deref())?;
    match dial(&body) {
        Ok(sent) => Ok(sent),
        Err(why) => {
            if !llm_tools_offer::honour_refusal(server, &why, 1) {
                return Err(why);
            }
            log(&tool_loop::refused_log(step, &why));
            let bare = build(None)?;
            dial(&bare)
        }
    }
}
