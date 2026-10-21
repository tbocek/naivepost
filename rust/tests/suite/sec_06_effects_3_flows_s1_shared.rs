//! §06-effects §3 Flows, S1 of the answered spec question on this item: the three per-clip passes keep their
//! tools ON THE WIRE, and a reply that carries a plain JSON document instead of calling a tool is UNREADABLE --
//! the app asks again rather than parsing the document, and a second such answer is the pass's failure.
//!
//! Everything here runs over the loopback fake through the settings address (the same seam a person types an
//! endpoint into in Settings), so no live server is dialled and no canned reply stands where the program
//! telephones. `sec_06_effects_3_flows_order.rs` proves the tool rules with turns handed in; the `f3_9_*`
//! siblings prove the batch, the cache and the out-of-batch retry. What is proven here is the offer on the wire
//! and the answer shape the wire must come back in.
//!
//! Helpers only: ONE scenario per binary, because `fixture()` pins `XDG_CONFIG_HOME`, `XDG_DATA_HOME` and the
//! cwd, which are process-wide. The three cases live in `sec_06_effects_3_flows_s1_offer.rs`, `…_calls.rs` and
//! `…_plain.rs`, each including this file by `#[path]` -- the shape `captions_wire_harness.rs` is shared by.

#![allow(dead_code)] // every scenario binary compiles this whole module; a helper it does not call is not a warning

#[path = "captions_wire_harness.rs"]
mod harness;

pub use harness::*;

/// The batch: two clips of ten seconds, so a caption at 1..4 sits inside the clip and over the floor.
pub fn clips() -> Vec<(u32, f64)> {
    vec![(1u32, 10.0), (2u32, 10.0)]
}

/// Rows this step paid for on the wire, excluding §09 §10's `cache` lines: one row per REQUEST. The fake
/// answers the streaming request `llm_request::body` asks for (§1) as plain JSON, and `llm_retry_leg`'s §3
/// patience treats a stream read failure as retryable -- so EVERY ask in these scenarios costs two requests,
/// identical bodies, and that constant is not what this item is about. Counting asks whose reply the fake
/// actually served (`Seen::reply`) is what distinguishes one answer from two.
pub fn rows_paid(fixture: &Fixture) -> usize {
    read(&fixture.tree.requests_tsv())
        .lines()
        .filter(|line| {
            let fields: Vec<&str> = line.split('\t').collect();
            fields.contains(&"captions") && fields.contains(&"chat") && !fields.contains(&"cache")
        })
        .count()
}

/// Whether anything under this step's cache folder holds `needle`.
pub fn kept(fixture: &Fixture, needle: &str) -> bool {
    let mut stack = vec![fixture.root.join("session.naivepost/cache/llm/captions")];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if std::fs::read_to_string(&path).is_ok_and(|text| text.contains(needle)) {
                return true;
            }
        }
    }
    false
}

/// The window's log sink, read the way the other live tests read it.
pub fn log_said(says: impl Fn(&str) -> bool) -> bool {
    naivepost::ui::window_logs().iter().any(|line| says(line))
}
