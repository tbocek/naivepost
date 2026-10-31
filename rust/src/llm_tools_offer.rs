//! F6.1 S5's live half: the `tools` field a server refused, remembered per server.

use std::sync::{Mutex, OnceLock};

use crate::tool_loop::Refused;
use crate::tools::Tool;

fn refused() -> &'static Mutex<Refused> {
    static REFUSED: OnceLock<Mutex<Refused>> = OnceLock::new();
    REFUSED.get_or_init(|| Mutex::new(Refused::new()))
}

/// The tools to offer this request: the job's own list, or nothing at all once this server has refused the field.
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
