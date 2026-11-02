//! F3.9 S4 over a socket: a clip number outside this batch loses the WHOLE reply, the retry goes out
//! for real (it reaches the wire again), and if the second answer is clean the captions land.
//! One scenario per binary (`XDG_CONFIG_HOME`/cwd are process-wide).

#![allow(dead_code)]

#[path = "captions_wire_harness.rs"]
mod harness;

/// The batch this scenario argues about: clips 6 to 10, ten seconds each, so "clip 3" is a clip that
/// exists somewhere but not here.
fn batch() -> Vec<(u32, f64)> {
    (6..=10).map(|n| (n, 10.0)).collect()
}
