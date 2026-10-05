//! F2.2 S5 — the spare pipeline, the swap at the join, and the three "while playing" answers, as REAL
//! PROCESSES rather than a number in a slot.
//!
//! The spec's S5 says "next jump preloaded three seconds ahead in a spare pipeline so the join does not
//! freeze", and `spec/inventory/cut.md` §D spells out what that means mechanically: a pipeline taken to
//! PAUSED at exactly the next second, and `PlaySegment` SWAPPING it in when the head matches within 0.01 s.
//! This app's media leg is a subprocess (`play_back`, F2.1), so the two states are two processes and every
//! claim here is checked on a real one: a preroll is a process `ps` reports as stopped, a swap is that
//! process running while the one it replaced is gone. No test helper is consulted — the fake player is the
//! program's own seam (`play_back::player_program` reads `NAIVEPOST_PLAYER`, the same shape as pointing a
//! server URL at a loopback fake).
//!
//! Everything runs in ONE `#[test]` in this binary: the leg's slots are process-wide (one running player,
//! one spare — `gui/player_spare.go`'s "one spare, not one per row") and the player seam is one env var, so
//! the cases must run in sequence, the arrangement `tests/f2_1_recording_play_widgets.rs` uses.

use std::cell::Cell;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use naivepost::{cut, cut_play_leg, cut_screen, preview};
use naivepost::cut_play_leg::CutPreviewPage;

/// How long a stopped process stays a process: `SIGSTOP` is not immediate relative to a script's own lines,
/// and a signal to a pid that has already exited is no longer a signal to it. Poll a short wait rather than
/// trusting one `ps`.
fn wait_until(what: impl Fn() -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(4);
    while Instant::now() < deadline {
        if what() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    what()
}

/// A player that answers `SIGTERM` (so a replacement really dies) and stays alive until then, the way
/// `tests/f2_1_recording_play_widgets.rs`'s fake player does. The `kill -0` in the trap is not decoration:
/// a shell defers a trapped signal until the foreground command returns, so without it a `SIGTERM` sent while
/// `sleep` is running would not be acted on for a twentieth of a second — long enough for a test to read the
/// process it just replaced as still there.
fn fake_player(root: &Path) -> PathBuf {
    let script = root.join("spare-player.sh");
    std::fs::write(
        &script,
        "#!/bin/sh\ntrap 'kill -0 $$; exit 0' TERM INT\nwhile :; do sleep 0.05; done\n",
    )
    .expect("the player stub");
    std::fs::set_permissions(&script, PermissionsExt::from_mode(0o755)).expect("it runs");
    std::env::set_var("NAIVEPOST_PLAYER", &script);
    script
}

fn alive(pid: u32) -> bool {
    // `kill(pid, 0)` answers whether the process exists and is signallable by us — but a process the fake
    // player script left behind is a CHILD of this test process, and an exited one stays a zombie until
    // someone reaps it, where `kill(pid, 0)` still says "there". `ps -o stat=` is the outside answer: a
    // zombie shows `Z`, a live process anything else, and a reaped one is no line at all.
    let out = std::process::Command::new("ps")
        .args(["-o", "stat=", "-p", &pid.to_string()])
        .output();
    match out {
        Ok(o) if o.status.success() => {
            let state = String::from_utf8_lossy(&o.stdout).trim().to_string();
            !state.is_empty() && !state.starts_with('Z')
        }
        _ => false,
    }
}

/// A cut with one removed stretch: kept 0..10 and 20..30, so the jump to 20 is the join to preroll for and
/// `P.eng.preloadLeadSeconds` (3 s) says the spare should be waiting by second 17.
fn cut_with_one_hole() -> cut::Cut {
    cut::Cut {
        segs: vec![
            cut::Seg { s: 0.0, e: 10.0, ..Default::default() },
            cut::Seg { s: 20.0, e: 30.0, ..Default::default() },
        ],
        ..Default::default()
    }
}

fn files(one: &str, two: &str) -> Vec<String> {
    vec![one.to_string(), two.to_string()]
}

/// S5's spare, on real processes: armed at the jump's second, held stopped, swapped in at the join.
fn s5_spare_prerolls_holds_and_swaps(_root: &Path) {
    let both = files("take-a.mp4", "take-b.mp4");
    cut_play_leg::clear_spare_arms_for_test();

    // The rule that decides WHEN: three seconds ahead of the jump, per `P.eng.preloadLeadSeconds`.
    let cut_ = cut_with_one_hole();
    assert_eq!(
        naivepost::cut_play::preload_ahead(&cut_.segs, 17.0),
        Some(20.0),
        "the lead is exactly P.eng.preloadLeadSeconds = {}",
        preview::PRELOAD_LEAD_SECONDS
    );

    // Play the first clip: one process, running (not stopped).
    let first = naivepost::cut_play_back::cue_clips(&both, 0.0, &|_| {})
        .expect("the player starts");
    assert!(alive(first), "the running player is a real process");
    // A player that is playing is one `ps` reports as running/sleeping, never stopped.
    assert!(
        !naivepost::cut_play_back::is_stopped_for_test(first),
        "a player that is playing is not stopped"
    );
    assert_eq!(
        naivepost::cut_play_back::spare_pid_for_test(),
        None,
        "S5 arms a spare for a jump; the press itself never leaves one over"
    );

    // Arm the spare for the join at 20. A preroll costs a process held STOPPED, silent, on the second the
    // line is about to land on.
    let spare = naivepost::cut_play_back::preroll(&both, 20.0, &|_| {}).expect("the spare prerolls");
    assert_ne!(spare, first, "the spare is a SECOND process, not the running one");
    assert_eq!(naivepost::cut_play_back::spare_pid_for_test(), Some(spare));
    assert!(
        wait_until(|| naivepost::cut_play_back::is_stopped_for_test(spare)),
        "the spare sits at the jump's second frozen (ps shows T), which is the preroll"
    );
    assert!(alive(first), "arming a spare leaves the playing process alone");

    // The join arrives. The spare was waiting, so the swap is a signal pair, not a pipeline build.
    let now = naivepost::cut_play_back::swap_spare_in(&|_| {}).expect("the spare was there to swap");
    assert_eq!(now, spare, "the swapped-in process IS the spare");
    assert_eq!(
        naivepost::cut_play_back::playing_pid(),
        Some(spare),
        "the leg now calls the spare the running player"
    );
    assert_eq!(naivepost::cut_play_back::spare_pid_for_test(), None, "swapping consumes the spare: one slot");
    assert!(
        wait_until(|| !naivepost::cut_play_back::is_stopped_for_test(spare)),
        "the swapped-in process is released and running"
    );
    let replaced = first;
    assert!(
        wait_until(|| !alive(replaced)),
        "the process the join left behind was stopped, not leaked"
    );

    // A swap with nothing armed answers "nothing to swap" — the caller then cues, which is the reload the
    // spare exists to avoid, and the leg must be able to tell the two apart.
    assert_eq!(naivepost::cut_play_back::swap_spare_in(&|_| {}), None);

    // Pausing releases BOTH: a spare kept alive after a pause is a process waiting for a jump that will
    // never come. `halt` is that pause door.
    assert!(naivepost::cut_play_back::halt(), "a player was running to stop");
    assert_eq!(naivepost::cut_play_back::playing_pid(), None);
    assert_eq!(naivepost::cut_play_back::spare_pid_for_test(), None, "and the spare went with it");
    assert!(!naivepost::cut_play_back::halt(), "a second pause has nothing left to stop");
}

/// The tick loop's own bookkeeping: the arm log records a preroll BEFORE the head got there, one per new
/// second, and the swap is folded into the same entry.
fn s5_the_tick_arms_once_per_join_and_records_it(root: &Path) {
    let _ = root;
    let cut_ = cut_with_one_hole();
    // `last_armed` is the leg's memory of what it armed last; a previous case in this binary may have armed
    // the same second, and the leg's own "only arm a NEW second" rule would then rightly skip the preroll.
    // Forget the leg's memory of the second armed last, so this case measures only its own arms. `arm_spare`
    // is the arithmetic-only arm (it starts no process); it clears the slot when nothing is worth arming.
    cut_play_leg::arm_spare(&cut_, 0.0);
    cut_play_leg::clear_spare_arms_for_test();

    let page = WatchPage::new(cut_.clone());
    // Tick 0, the one that matters: the line is three seconds short of the join, so THIS arm must preroll.
    // Every tick after it finds the same second armed and must not build a second pipeline.
    cut_play_leg::arm_spare_page(&cut_, 17.0, &page);
    // The tick loop's own shape: the swap first (the slot is still this tick's own), the move, then the arm
    // for the NEXT jump. `head` starts inside the lead window: 17 s is exactly
    // `P.eng.preloadLeadSeconds` before the jump at 20 s, so the very first arm should preroll.
    let mut head = 17.0;
    for _ in 0..40 {
        let armed_now = cut_play_leg::peek_spare();
        let step = cut_play_leg::tick(&cut_, head, preview::TICK_MS);
        if step.jumped || cut_play_leg::reached_spare(armed_now, step.head) {
            let found = page.cross_into_spare();
            cut_play_leg::note_spare_swapped(found);
            if found {
                cut_play_leg::clear_spare(&page);
            }
        }
        head = step.head;
        let jumped = step.jumped;
        cut_play_leg::arm_spare_page(&cut_, head, &page);
        if jumped {
            break;
        }
    }
    let arms = cut_play_leg::spare_arms_for_test();
    let prerolled: Vec<Option<f64>> = arms.iter().filter(|arm| arm.prerolled).map(|arm| arm.at).collect();
    assert_eq!(
        prerolled,
        vec![Some(20.0)],
        "one preroll, at the jump's second, armed before the line arrived — not one per tick"
    );
    let swapped = arms.iter().filter(|arm| arm.swapped == Some(true)).count();
    assert_eq!(swapped, 1, "the join that landed on the spare records exactly one swap");
    assert!(
        arms.iter().any(|arm| arm.swapped == Some(true) && !arm.prerolled),
        "the swap is recorded on the tick that jumped, whose own arm had nothing new to open"
    );
}

/// S5 "stops show their still": a stop under the line freezes the real process; leaving it releases it.
fn s5_a_stop_freezes_the_running_process(_root: &Path) {
    let both = files("take-a.mp4", "take-b.mp4");
    let running = naivepost::cut_play_back::cue_clips(&both, 0.0, &|_| {}).expect("a player to freeze");
    assert!(
        wait_until(|| !naivepost::cut_play_back::is_stopped_for_test(running)),
        "a fresh player runs, so a stop has a moving picture to hold"
    );

    let stopped = naivepost::cut_play_back::hold_still(true);
    assert!(stopped, "a stop under the line finds the player to freeze");
    assert!(
        wait_until(|| naivepost::cut_play_back::is_stopped_for_test(running)),
        "the process is frozen on its last frame — that IS the still, nothing is drawn"
    );

    // The seconds the stop covers end: the picture moves again.
    assert!(naivepost::cut_play_back::hold_still(false));
    assert!(
        wait_until(|| !naivepost::cut_play_back::is_stopped_for_test(running)),
        "releasing the stop runs the same process on, with no rebuild"
    );
    naivepost::cut_play_back::halt();

    // And the rule itself, through the leg's one read of a frame: a stop is a speed effect at rate 0.
    let stopped_cut = cut::Cut {
        segs: vec![cut::Seg { s: 0.0, e: 10.0, ..Default::default() }],
        fx: vec![cut::Fx {
            kind: "speed".to_string(),
            t: 4.0,
            dur: naivepost::cut_speed::STOP_SECONDS,
            rate: 0.0,
            ..Default::default()
        }],
        ..Default::default()
    };
    let plain = cut::Seg { s: 0.0, e: 10.0, ..Default::default() };
    assert!(
        cut_play_leg::frame(&stopped_cut, Some(&plain), 4.5).still,
        "inside the stop's seconds the frame answer is a still"
    );
    assert!(
        !cut_play_leg::frame(&stopped_cut, Some(&plain), 3.9).still,
        "before it, the picture moves"
    );
}

/// S5 "speed effects at their flat rate" — what the player's argv can honestly do, and what it cannot.
fn s5_rate_answers_honestly(_root: &Path) {
    let both = files("take-a.mp4", "take-b.mp4");
    // Nothing running: no process to hand a rate to, and no fake success.
    naivepost::cut_play_back::halt();
    assert!(
        !naivepost::cut_play_back::set_rate(&both, 1.0, 5.0, &|_| {}),
        "with no player there is nothing to cue — the answer is no"
    );

    let before = naivepost::cut_play_back::cue_clips(&both, 0.0, &|_| {}).expect("a player");
    // The flat rate the preview is asked to hold: the process is rebuilt at the head so no earlier stretch's
    // clock can leak into the flat seconds. A NEW process is the honest shape of "re-cued".
    assert!(naivepost::cut_play_back::set_rate(&both, 1.0, 7.5, &|_| {}));
    let after = naivepost::cut_play_back::playing_pid().expect("still a player");
    assert_ne!(after, before, "a flat-rate re-cue replaces the process");
    assert!(
        wait_until(|| !alive(before)),
        "and the one it replaced is gone, not left playing"
    );

    // A rate that is NOT flat cannot be asked of this player at all (see the module doc's flag list), and the
    // leg says so rather than claiming a success: the process it has is left alone.
    let kept = after;
    assert!(
        !naivepost::cut_play_back::set_rate(&both, 2.0, 9.0, &|_| {}),
        "a non-flat rate is answered honestly: not expressible through gst-play-1.0's argv"
    );
    assert_eq!(naivepost::cut_play_back::playing_pid(), Some(kept), "and nothing was swapped out");
    naivepost::cut_play_back::halt();
}

/// S5 "volume effects apply": the gain the tick routes is the slider's own number untouched over a plain
/// stretch, and the cut's own over an effect — one multiplication rule, `cut_hear::mix_gain`.
fn s5_the_routed_gain_follows_the_volume_effects() {
    let plain = cut_with_one_hole();
    // No slider touched: the preview's one volume is full, so these seconds' gain is the cut's own = 1.
    let routed = cut_play_leg::route(&plain, 5.0, naivepost::cut_hear::VOLUME_DEFAULT);
    assert!(!routed.still, "no stop here, so no still asked for");
    assert_eq!(routed.rate, naivepost::cut_play::FLAT_RATE, "a plain clip plays at the flat rate");
    assert!(
        (routed.gain - 1.0).abs() < 1e-9,
        "over footage with no volume effect the gain is exactly the slider's, and the slider is full"
    );

    // A volume effect at half the slider's number: the routed gain is the product, the same rule F2.5's
    // page asks. (The slider is NOT moved by this: a gain goes to the player, never back into the slider.)
    let ducked = cut::Cut {
        segs: plain.segs.clone(),
        fx: vec![cut::Fx {
            kind: "volume".to_string(),
            t: 0.0,
            dur: 8.0,
            gain: 0.5,
            ..Default::default()
        }],
        ..Default::default()
    };
    let ducked_gain = cut_play_leg::route(&ducked, 5.0, 1.0).gain;
    assert!(
        (ducked_gain - 0.5).abs() < 1e-9,
        "a volume effect at 0.5 over full slider routes 0.5, not 1.0: {ducked_gain}"
    );

    // A rate written on the seg the line is on is the rate handed to the player — which is what makes the
    // honest refusal in `set_rate` matter rather than look theoretical.
    let sped = cut::Cut {
        segs: vec![cut::Seg { s: 0.0, e: 10.0, rate: 2.0, ..Default::default() }],
        ..Default::default()
    };
    assert_eq!(cut_play_leg::route(&sped, 5.0, 1.0).rate, 2.0);
    // The line in a REMOVED stretch has no clip of its own: flat rate, no still, so no re-cue is owed there.
    assert_eq!(cut_play_leg::route(&plain, 15.0, 1.0).rate, naivepost::cut_play::FLAT_RATE);
    assert!(!cut_play_leg::route(&plain, 15.0, 1.0).still);
}

/// A page that answers what a window does, without a window: named processes through the leg, and counters
/// for what the leg itself records.
struct WatchPage {
    cut_: cut::Cut,
    head: Cell<f64>,
}

impl WatchPage {
    fn new(cut_: cut::Cut) -> Self {
        WatchPage { cut_, head: Cell::new(0.0) }
    }
}

impl cut_play_leg::CutPreviewPage for WatchPage {
    fn cut_only(&self) -> bool { true }
    fn playhead(&self) -> Option<f64> { Some(self.head.get()) }
    fn set_playhead(&self, at: f64) { self.head.set(at); }
    fn cut(&self) -> cut::Cut { self.cut_.clone() }
    fn say_clock(&self, _clock: f64, _ended: bool) {}
    fn volume(&self) -> f64 { naivepost::cut_hear::VOLUME_DEFAULT }
    // A real preroll, exactly as the window's page does it: the files are the session's, the leg owns the
    // process. `cut_screen::can_play_cut` is the S1 gate a page checks before it starts anything.
    fn arm_spare_at(&self, at: f64) -> bool {
        if !cut_screen::can_play_cut(&self.cut_) {
            return false;
        }
        naivepost::cut_play_back::preroll(
            &["take-a.mp4".to_string(), "take-b.mp4".to_string()],
            at,
            &|_| {},
        )
        .is_ok()
    }
    fn cross_into_spare(&self) -> bool { naivepost::cut_play_back::swap_spare_in(&|_| {}).is_some() }
    fn drop_spare(&self) { naivepost::cut_play_back::drop_spare(); }
    fn hold_still(&self, hold: bool) -> bool { naivepost::cut_play_back::hold_still(hold) }
    fn play_rate(&self, rate: f64, at: f64) -> bool {
        naivepost::cut_play_back::set_rate(&["take-a.mp4".to_string()], rate, at, &|_| {})
    }
    fn apply_gain(&self, _gain: f64) {}
}

/// One binary, one test: the leg's slots and the player seam are process-wide.
#[test]
fn f2_2_s5_the_spare_and_the_frame_answers_are_real_processes() {
    let root = std::env::temp_dir().join("naivepost-f2-2-spare");
    std::fs::create_dir_all(&root).expect("a scratch session folder");
    for name in ["take-a.mp4", "take-b.mp4"] {
        std::fs::write(root.join(name), b"not really a video").expect("a footage file to name");
    }
    std::env::set_current_dir(&root).expect("cwd pinned to the scratch session");
    fake_player(&root);

    s5_spare_prerolls_holds_and_swaps(&root);
    s5_the_tick_arms_once_per_join_and_records_it(&root);
    s5_a_stop_freezes_the_running_process(&root);
    s5_rate_answers_honestly(&root);
    s5_the_routed_gain_follows_the_volume_effects();
}
