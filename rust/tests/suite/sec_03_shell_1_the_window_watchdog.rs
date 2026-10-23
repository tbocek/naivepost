//! §03-shell.md §1's last bullet — the GUI hang watchdog: "dumps every goroutine's stack after 3 s
//! without a GUI heartbeat (engineering constant)", spelled out by `spec/inventory/shell.md`
//! §"Hang watchdog": "Heartbeat every 200 ms on the GTK thread; a goroutine dumps every goroutine's
//! stack after 3 s of silence to stderr and `<config>/hang-MMDD-HHMMSS.txt` (0600), once per hang."
//!
//! The rules are pure (`silent_ms`, `stalled`, `dump_body`) so they can be checked against injected
//! times; the two live checks drive the real watcher thread and the real GTK timer through the same
//! doors the program uses ([`naivepost::watchdog::start_watch`] from the launch,
//! [`naivepost::watchdog::arm`] from `build_window`), only with a shorter limit than §1's 3 s.
//!
//! The live check of a window that keeps beating is in `sec_03_shell_1_the_window_watchdog_window.rs`:
//! the watcher starts once per process, so the check that hangs on purpose and the one that must stay
//! quiet each need a process, and so a file, of their own. Every test gets its own `$XDG_CONFIG_HOME`,
//! so no dump lands in the real `~/.config/naivepost`.

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::Duration;

use naivepost::watchdog;

use crate::common;
use common::wait_until;

const SHORT_LIMIT: Duration = common::WATCHDOG_TEST_LIMIT;

/// The names a hang dump must carry, checked against one file's text.
fn assert_dump_names(path: &Path, text: &str) {
    assert!(
        path.file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("hang-"),
        "§7 names the file `hang-*.txt`: {}",
        path.display()
    );
    assert!(
        text.contains(watchdog::COMPONENT),
        "the dump names what stalled: {text}"
    );
    assert!(
        text.contains(&format!("pid {}", std::process::id())),
        "the dump names the pid of the process that hung: {text}"
    );
    assert!(text.contains("stack:"), "the dump carries stacks:\n{text}");
}

/// The two engineering constants §1 and §10's prose row name ("hang watchdog 200 ms / 3 s"), and the
/// catalogue rows built from them. Same shape as `run_bookkeeping.rs`'s check of the checkpoint poll.
#[test]
fn sec_03_shell_1_the_window_s10_the_heartbeat_is_200ms_and_the_silence_3s() {
    use naivepost::params;
    assert_eq!(watchdog::HEARTBEAT, Duration::from_millis(200));
    assert_eq!(watchdog::SILENCE_LIMIT, Duration::from_millis(3000));
    assert_eq!(
        params::find("machine.hangWatchdogBeatMs").map(|row| row.spelled),
        Some("200 ms".to_string()),
        "// machine.hangWatchdogBeatMs is catalogued from watchdog::HEARTBEAT"
    );
    assert_eq!(
        params::find("machine.hangWatchdogSilenceSeconds").map(|row| row.spelled),
        Some("3 s".to_string()),
        "// machine.hangWatchdogSilenceSeconds is catalogued from watchdog::SILENCE_LIMIT"
    );
    // What the dump calls the stalled thing: the GUI thread, since this app has one and Rust has no
    // runtime-wide stack dump (see the module doc).
    assert!(
        watchdog::COMPONENT.contains("gtk"),
        "the dump names the GUI thread: {}",
        watchdog::COMPONENT
    );
}

/// The silence rule, over injected times: the gap is measured against the limit, and a beat resets it.
#[test]
fn sec_03_shell_1_the_window_s10_the_silence_rule_measures_the_gap_against_the_limit() {
    let now = Duration::from_secs(10);
    // Nothing said yet (last beat at the epoch) reads as the whole clock of silence.
    assert_eq!(watchdog::silent_ms(now, Duration::ZERO), 10_000);
    assert_eq!(watchdog::silent_ms(now, now), 0);
    assert_eq!(
        watchdog::silent_ms(Duration::from_secs(1), now),
        0,
        "a beat stamped after `now` is no silence, not a negative one"
    );
    // Exactly at the limit has NOT yet passed it; one ms more has.
    assert!(!watchdog::stalled(
        now,
        now - watchdog::SILENCE_LIMIT,
        watchdog::SILENCE_LIMIT
    ));
    assert!(watchdog::stalled(
        now,
        now - watchdog::SILENCE_LIMIT - Duration::from_millis(1),
        watchdog::SILENCE_LIMIT
    ));
    // A beat in the middle of the silence resets the count: it is the GAP that matters, not the total.
    assert!(!watchdog::stalled(
        now,
        now - Duration::from_millis(500),
        watchdog::SILENCE_LIMIT
    ));
    // And the same pair against a shorter limit is a hang — which is what lets the live checks below
    // watch a hang happen without sleeping for §1's three seconds.
    assert!(watchdog::stalled(
        now,
        now - Duration::from_millis(500),
        SHORT_LIMIT
    ));
}

/// One dump's body, which goes to stderr AND to the file, so the two can never disagree: the component,
/// how long it had been unresponsive, the pid, then the stacks.
#[test]
fn sec_03_shell_1_the_window_s10_a_dump_names_the_stalled_thread_its_pid_and_its_stacks() {
    let body = watchdog::dump_body(
        Duration::from_millis(3200),
        4242,
        "#0 fake_frame at fake.rs:1\n",
    );
    assert!(body.contains(watchdog::COMPONENT), "{body}");
    assert!(body.contains("3.2s"), "how late it was: {body}");
    assert!(body.contains("pid 4242"), "{body}");
    assert!(
        body.contains("stack:\n#0 fake_frame"),
        "the stacks come under a heading:\n{body}"
    );
    // Whole seconds print plain, the way §7's other dump spells them.
    assert!(watchdog::dump_body(Duration::from_secs(4), 1, "").contains("4s"));
    // The real capture is not empty: a dump of this process has frames in it.
    assert!(
        watchdog::stacks().len() > 20,
        "a captured backtrace has something in it"
    );
}

/// The live half: a thread that stops beating IS dumped, through the program's own door, into the
/// config folder §7 names, 0600, with the stamp §7 names.
#[test]
fn sec_03_shell_1_the_window_s10_a_live_watch_dumps_after_the_silence_passes() {
    let paths = common::watchdog_paths("hang");
    // One hang per test process, and this is the only check that hangs one on purpose. It owns the dump
    // the other live check must NOT see: `start_watch` is once per process (one GUI thread, one watcher),
    // so both checks share the watcher this call starts and hand it the same short limit.
    watchdog::start_watch(Some(paths.clone()), SHORT_LIMIT);
    let beats_before = watchdog::beats();
    // One beat establishes that the thread is alive; then nothing more is said, and the silence is the
    // hang. Beating on the test's own clock is the marker the GTK timer uses.
    watchdog::beat(watchdog::clock().as_millis() as u64);
    wait_until("the watchdog wrote its hang dump", || {
        !common::hang_dumps(&paths).is_empty()
    });
    let files = common::hang_dumps(&paths);
    assert_eq!(files.len(), 1, "one hang is one dump: {files:?}");
    let path = &files[0];
    assert_eq!(
        path.parent().unwrap(),
        paths.config_dir,
        "the dump lands beside the settings (§7)"
    );
    let stamp = path
        .file_name()
        .unwrap()
        .to_string_lossy()
        .trim_start_matches("hang-")
        .trim_end_matches(".txt")
        .to_string();
    assert!(
        paths.has_hang_dump(&stamp),
        "`hang-{stamp}.txt` is the file §7 names, and the reader agrees it exists"
    );
    let text = std::fs::read_to_string(path).expect("the dump is readable");
    assert_dump_names(path, &text);
    assert_eq!(
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
        0o600,
        "§1's exception list: a watchdog dump is 0600"
    );
    // Once per hang: keep looking for longer than the limit and the folder still holds the ONE file it
    // wrote. What this cannot pin from outside is the SECOND half of that rule — a hang AFTER a
    // recovered thread earning its own file — because a test cannot stall the watcher's clock twice in
    // one process.
    //
    // Beating again DOES clear the flag, but the beat cannot be rung from here: `LAST_BEAT_MS` follows
    // `clock()`, so a test's extra `beat()` lands at "now" and reads as no silence at all. Only a real
    // stall followed by a real recovery can drive that half, which is what the app does on its own.
    wait_until("the watcher had looked again", || {
        watchdog::clock() > SHORT_LIMIT * 4
    });
    assert_eq!(
        common::hang_dumps(&paths).len(),
        1,
        "one hang earns exactly one file"
    );
    assert!(
        watchdog::beats() > beats_before,
        "the beat the test rang is counted: {} -> {}",
        beats_before,
        watchdog::beats()
    );
}
