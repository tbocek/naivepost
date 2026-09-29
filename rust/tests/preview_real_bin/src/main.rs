//! F4.5's real legs driven OUTSIDE the test binary, so a SIGKILL here is survivable: the process
//! running the preview is not the process reporting that it died. This is the S5 "reaped on the way
//! out" check with the reaper outside the corpse, over a real session folder and the real legs.
//!
//! argv[1] = session .naivepost folder to preview.
//! Prints `child <pid>` per player it spawns (the pids the test must find dead afterwards),
//! `alive` partway through the run, and `stopped ...` on normal exit.
//!
//! Build once per round with the helper script next to this file; the test skips cleanly when the
//! binary is absent (no network, no cargo in the sandbox).

use naivepost::cut::{Cut, Fx, Seg};
use naivepost::narration::Entry;
use naivepost::narrate_preview_leg::{self as leg, Spawn};

fn entry(s: f64, e: f64, text: &str) -> Entry {
    Entry { s, e, at: 0.5, text: text.into(), emotion: String::new(), pos: String::new(), roll: 0 }
}

fn main() {
    let Some(session) = std::env::args().nth(1) else {
        eprintln!("usage: preview_real <session.naivepost>");
        std::process::exit(2);
    };
    let tree = naivepost::layout::Tree::new(std::path::Path::new(&session)).expect("session tree");

    let segs = vec![
        Seg { s: 0.0, e: 10.0, ins: "a.mp4".into(), dur: 10.0, ..Default::default() },
        Seg { s: 10.0, e: 20.0, ins: "b.mp4".into(), dur: 10.0, ..Default::default() },
    ];
    // A stop at t=10.0 so the tick has a gap to skip forward over (S2).
    let fx = vec![Fx { kind: "speed".into(), t: 10.0, ..Default::default() }];
    let cut = leg::cut_of(segs, fx);
    let entries = vec![
        entry(0.0, 10.0, "first line"),
        entry(10.0, 20.0, "second line"),
        entry(20.0, 30.0, "third line"),
    ];

    let mut preview = leg::Preview::default();
    let mut spawn_fn = |prog: &str, args: &[String]| -> Result<std::process::Child, String> {
        leg::spawn_player(prog, args)
    };
    let spawn: &mut Spawn = &mut spawn_fn;
    match preview.start(&tree, &cut, &entries, 0.0, spawn) {
        Ok(msg) => {
            for child in preview.children() {
                println!("child {child}");
            }
            println!("started {msg}");
        }
        Err(why) => {
            println!("refused {why}");
            std::process::exit(1);
        }
    }
    // Step until stopped or the wall runs out. Each step is one tick of the picture.
    for i in 0..300 {
        let now = i as f64 * 0.05;
        let _ = leg::step(&mut preview, &tree, &cut, &entries, now, spawn);
        if i == 100 {
            println!("alive");
        }
    }
    // Normal exit: stop the run, which should reap.
    let (stopped, pids) = leg::stop_running();
    println!("stopped {stopped:?} {pids:?}");
}
