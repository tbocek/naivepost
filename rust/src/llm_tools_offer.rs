//! F6.1 S5's live half: the `tools` field a server refused, remembered per server.
//!
//! [`crate::tool_loop`] holds §2's rules as pure functions and is tested with two closures; nothing on the wire
//! ever honoured them. Every chat ask built its body with `Some(tools)` and gave up on the first error, so a
//! server that answers 400 "does not support the tools field" lost the step, and narrate's three attempts each
//! re-offered the field and each re-logged the same refusal — which is exactly what §2 says the rewrite must not
//! keep ("the refusal is not remembered, so a step retrying with tools re-offers the field and re-logs the line
//! each time").
//!
//! Two doors, both called from an ask closure at the moment the decision is made:
//!
//! * [`tools_for`] before the body is built — offer the field unless this server has already said no.
//! * [`honour_refusal`] when a round comes back with an error — is this a refusal of the field, and has it not
//!   been honoured yet? A yes remembers the server and the caller logs [`crate::tool_loop::refused_log`] once.
//!
//! The memory is process-wide and keyed on the endpoint URL because refusals are a property of the SERVER, not
//! of one call: the LLM address can be edited mid-run (§02-services#1), and a different box gets a clean slate.
//! It is deliberately never cleared — a server that stopped supporting tools in the middle of a session is rarer
//! than the repeated log line this removes, and restarting the app resets it.

use std::sync::{Mutex, OnceLock};

use crate::tool_loop::Refused;
use crate::tools::Tool;

/// The refusals seen since the app started. One table for every flow, so narrate's second and third attempt and
/// any later step share what the first attempt learned.
fn refused() -> &'static Mutex<Refused> {
    static REFUSED: OnceLock<Mutex<Refused>> = OnceLock::new();
    REFUSED.get_or_init(|| Mutex::new(Refused::new()))
}

/// The tools to offer this request: the job's own list, or nothing at all once this server has refused the field.
///
/// `None` means "build the body without a `tools` key" — the identical request minus the field, which is what S5
/// resends. Nothing is logged here: a server already known to refuse is simply asked without the field, silently,
/// which is the whole point of remembering.
pub fn tools_for(server: &str, offered: &[Tool]) -> Option<Vec<Tool>> {
    if refused()
        .lock()
        .map(|seen| seen.server(server))
        .unwrap_or(false)
    {
        return None;
    }
    Some(offered.to_vec())
}

/// Has this error just now become a refusal of the `tools` field, on the round where honouring it is still worth
/// doing? True means: remember this server (so no later request offers the field again) and say the refusal once
/// — the caller writes [`crate::tool_loop::refused_log`] and resends the same body without the field.
///
/// "Non-stop error on round 0", "not already honoured", and "a dead server is not a refusal" are all
/// [`crate::tool_loop::retry_without_tools`]'s judgement, reused rather than restated. `round` is the loop's
/// round number, counting from 1.
pub fn honour_refusal(server: &str, error: &str, round: u32) -> bool {
    let already = refused()
        .lock()
        .map(|seen| seen.server(server))
        .unwrap_or(false);
    if !crate::tool_loop::retry_without_tools(error, round, already) {
        return false;
    }
    if let Ok(mut seen) = refused().lock() {
        seen.remember(server);
    }
    true
}
