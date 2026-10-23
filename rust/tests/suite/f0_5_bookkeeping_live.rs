//! F0.5 A run's bookkeeping — the four things `run_bookkeeping.rs` could not pin because they only
//! exist once a press is involved: that a single-step ▶ closes what it opened (§S4), that the queue's
//! joined line reaches the status row over whatever stood there (§2 / S2), that a lucky step boundary
//! really passes its next page (S4's "a lucky run moves on"), and that ⏹'s close takes the idle pulse
//! down with the run (§2).
//!
//! The rules themselves stay proved in `run_bookkeeping.rs` against `naivepost::runqueue`; these are
//! the wires (spec/00-principles.md §5).

use std::cell::RefCell;
use std::io::{Read, Write};
use std::rc::Rc;

use crate::common;
use adw::prelude::*;
use naivepost::project::{Project, Source};
use naivepost::run::{self, RunBar};
use naivepost::runqueue::{self, Queue, TRACK_FRAMES};
use naivepost::shell::Page;
use naivepost::ui;

/// Whether GTK has a display to build widgets against; `gtk4::init` may only be called once, and the
/// widget round's application already did it when that round ran first.
static GTK_UP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn ensure_gtk() {
    use std::sync::atomic::Ordering::SeqCst;
    if GTK_UP.load(SeqCst) {
        return;
    }
    // Safe either way: `is_initialized` says whether anything has brought GTK up yet.
    if !gtk4::is_initialized() {
        let _ = gtk4::init();
    }
    GTK_UP.store(true, SeqCst);
}

/// Stand a server that answers §S5 up for the whole round and remember it -- WITHOUT writing a conf or
/// touching `XDG_CONFIG_HOME`, both of which belong to whichever scenario pins them. A scenario that
/// presses ▶ calls [`point_presses_at_the_answering_server`] after its own pinning, and can read
/// [`press_server_hits`] to see what that press dialled.
fn pressable_audio_server() -> String {
    let url = fake_audio_server_counted();
    PRESS_SERVER.with(|slot| *slot.borrow_mut() = Some(url.clone()));
    url
}

/// The same server with a counter in front of it: the close's unload is one more request on this socket,
/// so a test can prove the hand-off left this process without naming an address it does not own.
/// `/v1/models` names the two models §S5 needs; every other route -- the health check and the unload a
/// close hands over among them -- just 200s, which is what makes this address safe to point a press at.
fn fake_audio_server_counted() -> String {
    use std::io::{BufRead, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().expect("an address").port();
    let hits = PRESS_HITS.with(|cell| cell.clone());
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let hits = hits.clone();
            std::thread::spawn(move || {
                hits.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let mut reader = std::io::BufReader::new(stream.try_clone().expect("clone"));
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 {
                    return;
                }
                loop {
                    let mut header = String::new();
                    if reader.read_line(&mut header).unwrap_or(0) == 0 || header.trim().is_empty() {
                        break;
                    }
                }
                // `/v1/models` names the two models §S5 needs; every other route (the health check and
                // the unload a close hands over among them) just 200s.
                let payload = if line.contains("/v1/models") {
                    r#"{"data":[{"id":"nemotron-asr","task":"asr"},{"id":"sortformer-diar","task":"diar"}]}"#
                } else {
                    "{\"ok\":true}"
                };
                let reply = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{payload}",
                    payload.len()
                );
                let _ = stream.write_all(reply.as_bytes());
                let _ = stream.flush();
            });
        }
    });
    format!("http://127.0.0.1:{port}")
}

thread_local! {
    static PRESS_SERVER: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
    static PRESS_HITS: std::sync::Arc<std::sync::atomic::AtomicUsize> =
        std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
}

/// How many requests the answering server has taken.
fn press_server_hits() -> usize {
    PRESS_HITS.with(|cell| cell.load(std::sync::atomic::Ordering::SeqCst))
}

/// Write the conf that makes a ▶ press answerable: the §S5 server, into whatever root `XDG_CONFIG_HOME`
/// names at the moment it is called -- so it never fights another scenario's pin, it just fills it.
fn point_presses_at_the_answering_server() {
    let Some(url) = PRESS_SERVER.with(|slot| slot.borrow().clone()) else {
        return;
    };
    let paths =
        naivepost::settings::from_environment().expect("settings folder from the pinned env");
    let mut conf = naivepost::settings::Conf::default();
    conf.audio_server = url;
    naivepost::settings::save(&paths, &conf).expect("the press dials a server that answers §S5");
}
/// One GTK application for the whole binary (`gio` refuses a second `run_with_args`), every scenario
/// inside one `activate` — the shape every widget test here uses. It also brings GTK up for the two
/// window-free checks below, which is why they call `window_round()` before building any widget. The
/// application the round runs under is handed to each scenario by `connect_activate`; nothing here needs
/// a global copy of it.
fn window_round() {
    common::one_app_round(|app| {
        // §S5 asks audio.cpp before any stage runs, so the round's presses need a server that
        // answers. It is stood up here; each scenario points at it AFTER its own pinning, so no
        // scenario loses the conf it asserts against.
        pressable_audio_server();
        point_presses_at_the_answering_server();
        single_step_closes(app);
        line_overwrites(app);
        unload_handed_over(app);
        lucky_moves_on();
        the_pulse_belongs_to_the_run();
        RAN.store(true, std::sync::atomic::Ordering::SeqCst);
    });
}

static RAN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// S4 through the widget: the run a single-step ▶ opened is CLOSED by the time the press is over —
/// `running` off, nothing left holding the bar busy. Before this wiring a ▶ left the bar showing a
/// finished run forever, so the next press answered as though work were still going.
fn single_step_closes(app: &adw::Application) {
    let project = footage_project();
    let dir = scratch("close");
    std::env::set_current_dir(&dir).expect("cwd pinned to the temp root");
    // Re-point AFTER this scenario pins its own cwd: `window_round` wrote the conf into whatever root
    // stood there BEFORE any scenario moved, so on a busy machine the press could dial an address that
    // is not the answering server's and take a different leg of the handler.
    point_presses_at_the_answering_server();
    let window = ui::build_window(app, &project, "Prepare");
    window.present();

    let play = ui::play_button(&window).expect("the run bar has a ▶");
    // The one door to every live bar (`shell_beats::run_bar` answers `WINDOW_BAR`'s newest), and THIS
    // window's bar IS the newest: `single_step_closes` built it last, so waiting on it is waiting on the
    // bar this press will act for. A ▶ over a run that is still standing PAUSES (F1.12/S1) instead of
    // starting-and-closing, so if an earlier scenario's close were still in flight this press would be a
    // pause and S4's assertion below would be measuring the wrong press. Wait for the condition instead
    // of assuming the previous scenario is done: same helper as everywhere else here, wall-clock ceiling.
    assert!(
        wait_until(
            || ui::run_bar(&window).is_some_and(|b| b.borrow().running.is_none()),
            "no run was open on this window's bar before the press"
        ),
        "a run from an earlier scenario never closed, so ▶ here would be a pause, not a step"
    );
    play.emit_by_name::<()>("clicked", &[]);

    let bar = ui::run_bar(&window).expect("the window carries a run bar");
    // The close runs inside the press, but the unload it hands over is a thread and the repaint lands on
    // the main context: wait for `running` to actually go off rather than reading it once, at whatever
    // point the scheduler happened to deliver the emission.
    assert!(
        wait_until(
            || bar.borrow().running.is_none(),
            "S4: the step's own run closing when the press ended"
        ),
        "S4: the step's own run closed when the press ended"
    );
    // And the audio unload was handed over rather than skipped: `end_run` answers with it every time,
    // so the pair the close hands to `fire_unload` is pinned at the source too.
    let (timeout, best_effort) = runqueue::audio_unload_is_bounded();
    assert_eq!(
        timeout,
        std::time::Duration::from_secs(20),
        "the unload is bounded"
    );
    assert!(best_effort, "and it never fails the run that made it");
    // A bar with no open run cannot answer the next press as busy: ▶'s S1 pause sentence is impossible.
    // Wait for the row to settle rather than reading it the instant after the emission: on a loaded
    // machine the refusal and the queue's line land on the main context, not inside `emit_by_name`.
    play.emit_by_name::<()>("clicked", &[]);
    assert!(
        wait_until(
            || ui::state(&window).status != run::PAUSING,
            "the second ▶ answering as a step, not a pause"
        ),
        "nothing was under way, so the second press is not a pause"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// §2 / S2: the queue's JOINED track line lands on the status row and overwrites whatever was last
/// there — asserted against the very string `Queue::text_and_tip().0` returns, so the row and the
/// joined line cannot drift into two different values.
fn line_overwrites(app: &adw::Application) {
    let project = footage_project();
    let dir = scratch("line");
    std::env::set_current_dir(&dir).expect("cwd pinned to the temp root");
    let window = ui::build_window(app, &project, "Prepare");
    window.present();

    let status =
        ui::find_status(window.upcast_ref()).expect("the log header carries the status line");
    status.set_text("something said before the run");
    let queue = ui::run_queue(&window).expect("the window carries a run queue");
    let bar = ui::run_bar(&window).expect("the window carries a run bar");
    runqueue::start_run(
        &mut bar.borrow_mut(),
        &mut queue.borrow_mut(),
        &mut naivepost::exchanges::RunLog::new(),
        run::Step::Prepare,
        run::snapshot_sources(&project),
    );
    report_frames(&queue);
    let joined = queue.borrow().text_and_tip().0;
    assert!(
        !joined.is_empty(),
        "a reported track has a line to show: {joined:?}"
    );

    // The program's own close door puts it there: same call the shell makes when a run ends. The log
    // body is read through `ui::log_view`, the shell's own door for it (the expander keeps its body
    // behind the log's scroller, so the test must not re-derive the walk).
    let progress = ui::progress_bar(&window).expect("the run bar has a progress widget");
    let log = ui::log_expander(&window).expect("the shell has a log expander");
    use gtk4::prelude::*;
    let view = ui::log_view(&window).expect("the expander holds the log body");
    naivepost::ui::run_close::close_step_run(
        &bar,
        &queue,
        &Rc::new(RefCell::new(naivepost::exchanges::RunLog::new())),
        &status,
        &progress,
        &log,
        &view,
        None,
    );
    assert_eq!(
        ui::state(&window).status,
        joined,
        "§2: the joined track line IS the status line now, sentinel gone"
    );
    assert_ne!(
        ui::state(&window).status,
        "something said before the run",
        "the run overwrote it"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// S4's "a lucky run moves to its next step": the chain's step boundary passes the page it is moving
/// to, which is the ONLY place in the program where `end_run` gets a `Some(page)` — without it
/// `EndRun::chain_moves_on` would be true only in tests. No window needed: the driver's handles hold
/// widgets, and their types are all GTK needs.
#[test]
fn f0_5_s4_a_lucky_run_moves_on() {
    // libadwaita widgets may only be built on the main thread, and `run_with_args` runs its `activate`
    // there -- so the window-free scenarios run inside the same round the widget checks use.
    window_round();
}

/// §2: the idle pulse belongs to the press — ▶ stands it while the run holds nothing countable, and
/// the close takes it down rather than leaving a timer ticking on an idle bar. Nothing pumps the main
/// loop between the two assertions, so a standing timer can only be the press's doing.
#[test]
fn f0_5_s2_the_press_stands_a_pulse() {
    window_round();
}

/// S4's "a lucky run moves to its next step", run inside the round so its widgets are on the main
/// thread: the chain's step boundary passes the page it is moving to, which is the ONLY place in the
/// program where `end_run` gets a `Some(page)` -- without it `EndRun::chain_moves_on` would be true
/// only in tests.
fn lucky_moves_on() {
    let handles = handles();
    let bar = handles.bar.clone();
    bar.borrow_mut().running = Some(run::Run {
        step: run::Step::Prepare,
        paused: false,
        log_expanded: true,
        sources: run::Snapshot::default(),
    });
    report_frames(&handles.queue);
    let joined = handles.queue.borrow().text_and_tip().0;

    naivepost::ui::run_close::close_chain_step(&handles, Some(Page::Cut));

    // The move happened: a boundary with a next page reports that the chain moves on...
    assert!(
        runqueue::end_run(&mut RunBar::default(), Some(Page::Cut)).chain_moves_on,
        "S4: a boundary with a next page moves on"
    );
    // ...and the press's ONE run stays open across it, because §2 wants ⏸/⏹ acting for the whole chain.
    assert!(
        bar.borrow().running.is_some(),
        "S4: the chain keeps its run open between steps (F0.4 S1 must still see it as busy)"
    );
    // Its queue's line went to the status row and the queue was reset for the next step's counting.
    assert_eq!(
        handles.status.text().as_str(),
        joined.as_str(),
        "§2: the step's line reached the row"
    );
    assert_eq!(
        handles.queue.borrow().text_and_tip().0,
        "",
        "S1: the next step counts from an empty queue"
    );

    // The chain's END is the other half: no next page, so the run really closes.
    naivepost::ui::run_close::close_chain_step(&handles, None);
    assert!(
        bar.borrow().running.is_none(),
        "S4: with nowhere to move on, the run ends"
    );
}

/// §2's idle pulse, also inside the round: arming stands a timer while the run holds nothing
/// countable, and the close takes it down rather than leaving one ticking on an idle bar. Nothing
/// pumps the loop between the two reads, so a standing timer can only be the arming's doing.
fn the_pulse_belongs_to_the_run() {
    let handles = handles();
    assert!(
        !ui::shell_beats::pulse_standing_for_test(),
        "an idle process holds no pulse timer"
    );
    ui::shell_beats::arm_pulse(&handles.progress, &handles.bar, &handles.queue);
    assert!(
        ui::shell_beats::pulse_standing_for_test(),
        "§2: a run with nothing countable pulses the bar"
    );
    // The rule the timer runs on, and the close that ends it.
    assert!(
        ui::shell_beats::should_pulse(true, 0.0),
        "a thinking model pulses"
    );
    assert!(
        !ui::shell_beats::should_pulse(true, 0.25),
        "once something counts, the needle owns the bar"
    );
    ui::shell_beats::arm_pulse(&handles.progress, &handles.bar, &handles.queue);
    naivepost::ui::run_close::close_chain_step(&handles, None);
    assert!(
        !ui::shell_beats::pulse_standing_for_test(),
        "S4: closing the run takes the pulse down with it"
    );
}

#[test]
fn f0_5_bookkeeping_reaches_the_shell() {
    window_round();
    assert!(
        RAN.load(std::sync::atomic::Ordering::SeqCst),
        "the widget rounds never ran"
    );
}

#[test]
fn f0_5_s4_the_chain_close_hands_over_the_unload() {
    window_round();
}

/// §S4's "audio models unloaded off-thread", at the door the chain closes through, over a REAL socket.
/// The seam is the settings file: `fire_unload` reads audio.cpp's address out of it on every run (§1's
/// re-read-per-request rule), so pointing that one box at a loopback fake makes the check about bytes
/// that crossed a socket rather than about a stub. `close_chain_step` hands the unload over at each step
/// boundary; the program's own bounded wait (`settings_probe::drain_unload`) then retires the leg, and
/// the thread's report — written by nothing else in the program — lands in the log.
fn unload_handed_over(app: &adw::Application) {
    // FIRST in the round: this is the one scenario that pins `XDG_CONFIG_HOME` (process-wide), and it
    // has to write its conf before any other scenario's press dials the default loopback address.
    let fake = FakeAudioCpp::start();
    // Its `/v1/models` answers `{"ok":true}` -- no model list -- so §S5 would refuse a ▶ press pointed
    // here. The presses in this binary need models named; they go to the dedicated fake instead, whose
    // conf lives in its own root (`serve_audio_for_the_presses`). This conf stays the one the unload
    // assertions are made against.
    point_settings_at(&fake.url());
    // §S5 now asks this same address before a ▶ press may run, and this fake declares no models, so a
    // press made while its conf stands would be refused. The press below belongs to the UNLOAD check,
    // which wants exactly this address dialled -- so the answering server is pointed at for the length
    // of the press and this conf written back before the unload is waited on.
    naivepost::ui::run_close::clear_log_for_test();
    let handles = handles();
    handles.bar.borrow_mut().running = Some(run::Run {
        step: run::Step::Prepare,
        paused: false,
        log_expanded: true,
        sources: run::Snapshot::default(),
    });

    // §2 at the hand-off: with a chain running the row is the run's, so what the close writes there is
    // the step's own track line and NOT whatever stood there before -- `lucky_run::press` lends the row
    // to the run for its length through `mark_chain_row`, and §S4's housekeeping answers into it.
    handles.status.set_text("a sentence from before the chain");
    naivepost::ui::run_close::mark_chain_row(Some(&handles.status));
    report_frames(&handles.queue);
    let joined = handles.queue.borrow().text_and_tip().0;
    naivepost::ui::run_close::close_chain_step(&handles, Some(Page::Cut));
    // The hand-off is counted while it dials (`cancel_leg::leg_started` inside `fire_unload`'s thread).
    naivepost::settings_probe::drain_unload(std::time::Duration::from_secs(5));
    let dialed = wait_until(|| fake.hits() > 0, "the unload reached audio.cpp");
    assert!(dialed, "S4: the unload really telephoned audio.cpp");
    let said = wait_until(
        || naivepost::ui::run_close::unload_line_for_test().is_some(),
        "the run recorded its own housekeeping",
    )
    .then(|| naivepost::ui::run_close::unload_line_for_test().unwrap())
    .expect("the run recorded its own housekeeping");
    assert_eq!(
        fake.last_path().as_deref(),
        Some("/v1/tasks/unload_all_models"),
        "and asked the endpoint Kind::UnloadAll names, not some other route (log said: {said})"
    );
    assert!(
        said.starts_with(naivepost::ui::run_close::UNLOAD_PREFIX),
        "on the log line every run leaves: {said}"
    );
    assert!(
        said.contains("models unloaded"),
        "answered servers say so, rather than the run staying quiet: {said}"
    );
    // §2 at the hand-off: the row now carries the step's own line, not the sentence that was there when
    // the press started. Read through a short wait because the unload answers from its OWN thread (§S4)
    // and may still be dialling: if it has answered by now, its news is what the row holds instead.
    let on_row = wait_until(
        || {
            let row = handles.status.text().to_string();
            row == joined || row == said.replacen(naivepost::ui::run_close::UNLOAD_PREFIX, "", 1)
        },
        "the row to take the run's words (the step's line, or the housekeeping news once it lands)",
    );
    assert!(
        on_row,
        "§2: while the chain runs the row belongs to the run, not to what stood there before (row said {:?}, step line {joined:?}, log said {said:?})",
        handles.status.text(),
    );
    // And the pair §S4 pins together: the move happened WITHOUT ending the press's one run.
    assert!(
        handles.bar.borrow().running.is_some(),
        "S4: the chain keeps ONE run open across a step boundary (F0.4 S1 still reads it as busy)"
    );
    // The other close door, on the same socket: a single-step ▶ must hand the unload over too.
    single_step_close_dials(app);
}

#[test]
fn f0_5_s4_the_single_step_close_dials_the_unload() {
    window_round();
}

/// §S4's unload at the OTHER door: a single-step ▶. Same fake, same settings seam as
/// [`unload_handed_over`] (one scenario per binary may pin `XDG_CONFIG_HOME`, so this runs inside it and
/// shares its fake), but reached through the real button: `ui::play_button(...).emit_by_name("clicked")`
/// walks Prepare's step and closes what it opened, and the close must hand the audio unload over exactly
/// as the chain's boundary does — which before this wiring it never did at all.
fn single_step_close_dials(app: &adw::Application) {
    let project = footage_project();
    let window = ui::build_window(app, &project, "Prepare");
    window.present();
    naivepost::ui::run_close::clear_log_for_test();
    // §S5 asks audio.cpp before the step may run, and this scenario's fake declares no models -- a press
    // pointed at it would be refused and nothing would open, close or unload. So THIS press dials the
    // answering server (same re-read-per-request conf seam), and its own close hands the unload over to
    // THAT address: `hits` counts every route it serves, including the unload, so the assertions below
    // are made against the server the press really used.
    point_presses_at_the_answering_server();
    let dialled = press_server_hits();

    ui::play_button(&window)
        .expect("the run bar has a ▶")
        .emit_by_name::<()>("clicked", &[]);

    // The press closed its own bookkeeping (§S4), so the bar is idle again -- the same state the logic
    // check in `single_step_closes` asserts, read here off the widget press rather than off a call.
    // Waited for, not read once: an earlier scenario's close can still be in flight when this press runs
    // (one process-wide bar list), and then this press is a pause whose close lands on the main context
    // later than the emission returns -- which is how this assert went red on one machine and green on
    // another. The condition is what S4 claims; the deadline keeps a genuinely stuck run loud.
    let bar = ui::run_bar(&window).expect("the window carries a run bar");
    assert!(
        wait_until(
            || bar.borrow().running.is_none(),
            "S4: the ▶ press closing the run it opened"
        ),
        "S4: the ▶ press closed the run it opened"
    );
    naivepost::settings_probe::drain_unload(std::time::Duration::from_secs(5));
    assert!(
        wait_until(
            || press_server_hits() > dialled,
            "the single-step close dialled audio.cpp"
        ),
        "S4: a lone step's close hands the audio unload over too, not only the chain's"
    );
    let said = naivepost::ui::run_close::unload_line_for_test()
        .expect("the press recorded its own housekeeping");
    assert!(
        said.contains("models unloaded"),
        "and the answer came back from the fake: {said}"
    );
    window.close();
}

/// Poll `ask` until it answers true, or fail with `what` after ~5 s. The unload runs on its own thread
/// by design (§S4 "off-thread"), so a check for its effect must wait for a thread rather than assume
/// one has finished; the ceiling is well under §1's 20 s so a hung leg still fails the test loudly.
fn wait_until(ask: impl Fn() -> bool, what: &str) -> bool {
    for _ in 0..100 {
        if ask() {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    eprintln!("timed out waiting for {what}");
    false
}

/// A loopback audio.cpp for this binary only: answers 200 to everything, counts requests, remembers the
/// last request line's path. Lives here because only the test needs a stand-in server; the program dials
/// whatever the settings file says and knows nothing about it.
struct FakeAudioCpp {
    address: std::net::SocketAddr,
    hits: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    last_path: std::sync::Arc<std::sync::Mutex<Option<String>>>,
}

impl FakeAudioCpp {
    fn start() -> Self {
        let listener =
            std::net::TcpListener::bind("127.0.0.1:0").expect("a loopback port to fake audio.cpp");
        let hits = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let last_path = std::sync::Arc::new(std::sync::Mutex::new(None));
        let counted = hits.clone();
        let remembered = last_path.clone();
        let address = listener
            .local_addr()
            .expect("the fake server knows its own address");
        std::thread::spawn(move || {
            for incoming in listener.incoming() {
                let Ok(mut stream) = incoming else { continue };
                // Read the request line and headers (one byte at a time: no buffering to deadlock on),
                // note the path, then answer.
                let mut request = Vec::new();
                let mut byte = [0u8; 1];
                let mut blanks = 0;
                while stream.read(&mut byte).unwrap_or(0) == 1 {
                    request.push(byte[0]);
                    // End the header block at its blank line. The unload is a GET *with* a body, so
                    // stopping here (rather than reading to EOF) is what lets the reply go out before the
                    // client finishes writing — waiting for EOF on a keep-alive connection would hang.
                    blanks = if byte[0] == b'\r' || byte[0] == b'\n' {
                        blanks + 1
                    } else {
                        0
                    };
                    if blanks >= 2 || request.len() > 64 * 1024 {
                        break;
                    }
                }
                let head = String::from_utf8_lossy(&request);
                let path = head
                    .split_whitespace()
                    .nth(1)
                    .map(|path| path.to_string())
                    .unwrap_or_default();
                // A GET sent with a body (`Kind::UnloadAll`'s method) arrives as one request; anything
                // else that reaches this socket is counted too, but only a real route is remembered, so
                // the assertion below can name the endpoint the program asked for.
                if !path.is_empty() && path.starts_with('/') {
                    *remembered.lock().unwrap() = Some(path);
                }
                counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                // Answer EVERY request on this connection: the unload is a GET sent with a body, so one
                // connection can carry the request line and then the body as a second "request" if the
                // client's bytes are read in more than one piece. A 200 each time is what audio.cpp
                // itself would do for a route it knows, and it keeps the reply from depending on how the
                // bytes happened to be split.
                let body = "{\"ok\":true}";
                let _ = stream.write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                        body.len()
                    )
                    .as_bytes(),
                );
                let _ = stream.flush();
                drop(stream);
            }
        });
        Self {
            address,
            hits,
            last_path,
        }
    }

    /// The address to write into `AUDIOCPP_SERVER`.
    fn url(&self) -> String {
        format!("http://{}", self.address)
    }

    fn hits(&self) -> usize {
        self.hits.load(std::sync::atomic::Ordering::SeqCst)
    }

    fn last_path(&self) -> Option<String> {
        self.last_path.lock().unwrap().clone()
    }
}

/// Write the audio.cpp address the way the Settings dialog does — into the conf file the program re-reads
/// per request. `XDG_CONFIG_HOME` and cwd are process-wide, so this runs in exactly one scenario here.
fn point_settings_at(url: &str) {
    let root = scratch("unload");
    std::fs::create_dir_all(root.join("config")).expect("config home");
    std::env::set_current_dir(&root).expect("cwd pinned to the scenario root");
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
        std::env::set_var("XDG_DATA_HOME", root.join("data"));
    }
    let paths =
        naivepost::settings::from_environment().expect("settings folder from the pinned env");
    let mut conf = naivepost::settings::Conf::default();
    conf.audio_server = url.to_string();
    naivepost::settings::save(&paths, &conf).expect("audio address written");
}

/// One frame track, half done: enough for the queue to have a joined line of its own.
fn report_frames(queue: &Rc<RefCell<Queue>>) {
    let mut q = queue.borrow_mut();
    q.job(TRACK_FRAMES, "frames", 1, 2);
    q.push(TRACK_FRAMES, 4, "frame");
    q.prog(TRACK_FRAMES, 0.5, "frame 2/4");
}

/// A project whose Prepare step has work, so ▶ starts rather than refuses.
fn footage_project() -> Project {
    Project {
        sources: vec![Source {
            path: "project:sources/lecture.mkv".into(),
            footage: true,
            ..Default::default()
        }],
        ..Default::default()
    }
}

fn scratch(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("naivepost-f05-live-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch folder");
    dir
}

/// The driver's handles. GTK is initialised by the application the widget round runs, so these tests
/// only build widgets -- calling `gtk4::init()` a second time after `adw::Application::run` crashes
/// GDK, which is why the round runs first and `ensure_gtk` only fills in when there was no app.
fn handles() -> naivepost::ui::lucky_run::Handles {
    ensure_gtk();
    naivepost::ui::lucky_run::Handles {
        bar: Rc::new(RefCell::new(RunBar::default())),
        shell: Rc::new(RefCell::new(naivepost::shell::Shell::default())),
        stack: adw::ViewStack::new(),
        guard: Rc::new(std::cell::Cell::new(false)),
        status: gtk4::Label::new(None),
        progress: gtk4::ProgressBar::new(),
        log: gtk4::Expander::new(None),
        log_view: gtk4::TextView::new(),
        queue: Rc::new(RefCell::new(Queue::new())),
        exchange_log: Rc::new(RefCell::new(naivepost::exchanges::RunLog::new())),
        procs: Rc::new(RefCell::new(run::Subprocesses::default())),
    }
}
