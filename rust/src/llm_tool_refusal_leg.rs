//! §2 S5's live half, shared: one dial that honours a server's refusal of the `tools` field.

use crate::llm_tools_offer;
use crate::tool_loop;
use crate::tools::Tool;

/// Dial one chat request, honouring §2's refusal rule: a non-stop error on the FIRST round that names the
/// `tools` field means the identical request goes out again WITHOUT the field, said once.
pub type BodyBuilder<'a> = &'a dyn Fn(Option<&[Tool]>) -> Result<String, String>;

/// How a flow sends one body.
pub type Dialer<'a, S> = &'a dyn Fn(&str) -> Result<S, String>;

pub fn dial_honouring_refusal<S>(
    step: &str,
    server: &str,
    offered: Option<&[Tool]>,
    build: BodyBuilder<'_>,
    dial: Dialer<'_, S>,
    log: &dyn Fn(&str),
) -> Result<S, String> {
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
