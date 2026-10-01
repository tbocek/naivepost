//! F0.5 A run's bookkeeping — the four things `run_bookkeeping.rs` could not pin because they only
//! exist once a press is involved: that a single-step ▶ closes what it opened (§S4), that the queue's
//! joined line reaches the status row over whatever stood there (§2 / S2), that a lucky step boundary
//! really passes its next page (S4's "a lucky run moves on"), and that ⏹'s close takes the idle pulse
//! down with the run (§2).
//!
//! The rules themselves stay proved in `run_bookkeeping.rs` against `naivepost::runqueue`; these are
//! the wires (spec/00-principles.md §5).

use std::cell::RefCell;
use std::rc::Rc;

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

/// One GTK application for the whole binary (`gio` refuses a second `run_with_args`), every scenario
/// inside one `activate` — the shape every widget test here uses. It also brings GTK up for the two
/// window-free checks below, which is why they call `window_round()` before building any widget.
fn window_round() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        GTK_UP.store(true, std::sync::atomic::Ordering::SeqCst);
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(|app| {
            single_step_closes(app);
            line_overwrites(app);
            lucky_moves_on();
            the_pulse_belongs_to_the_run();
            RAN.store(true, std::sync::atomic::Ordering::SeqCst);
            app.quit();
        });
        app.run_with_args::<String>(&[]);
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
    let window = ui::build_window(app, &project, "Prepare");
    window.present();

    let play = ui::play_button(&window).expect("the run bar has a ▶");
    play.emit_by_name::<()>("clicked", &[]);

    let bar = ui::run_bar(&window).expect("the window carries a run bar");
    assert!(
        bar.borrow().running.is_none(),
        "S4: the step's own run closed when the press ended"
    );
    // And the audio unload was handed over rather than skipped: `end_run` answers with it every time,
    // so the pair the close hands to `fire_unload` is pinned at the source too.
    let (timeout, best_effort) = runqueue::audio_unload_is_bounded();
    assert_eq!(timeout, std::time::Duration::from_secs(20), "the unload is bounded");
    assert!(best_effort, "and it never fails the run that made it");
    // A bar with no open run cannot answer the next press as busy: ▶'s S1 pause sentence is impossible.
    play.emit_by_name::<()>("clicked", &[]);
    assert_ne!(
        ui::state(&window).status,
        run::PAUSING,
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

    let status = ui::find_status(window.upcast_ref()).expect("the log header carries the status line");
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
    assert!(!joined.is_empty(), "a reported track has a line to show: {joined:?}");

    // The program's own close door puts it there: same call the shell makes when a run ends. The log
    // body is read through the expander's own child, which is where the shell put it.
    let progress = ui::progress_bar(&window).expect("the run bar has a progress widget");
    let log = ui::log_expander(&window).expect("the shell has a log expander");
    use gtk4::prelude::*;
    let view = log
        .child()
        .and_then(|child| child.downcast::<gtk4::TextView>().ok())
        .expect("the expander's child is the log body");
    naivepost::ui::run_close::close_step_run(
        &bar, &queue, &Rc::new(RefCell::new(naivepost::exchanges::RunLog::new())),
        &status, &progress, &log, &view, None,
    );
    assert_eq!(
        ui::state(&window).status,
        joined,
        "§2: the joined track line IS the status line now, sentinel gone"
    );
    assert_ne!(ui::state(&window).status, "something said before the run", "the run overwrote it");
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
    assert!(bar.borrow().running.is_none(), "S4: with nowhere to move on, the run ends");
}

/// §2's idle pulse, also inside the round: arming stands a timer while the run holds nothing
/// countable, and the close takes it down rather than leaving one ticking on an idle bar. Nothing
/// pumps the loop between the two reads, so a standing timer can only be the arming's doing.
fn the_pulse_belongs_to_the_run() {
    let handles = handles();
    assert!(!ui::shell_beats::pulse_standing_for_test(), "an idle process holds no pulse timer");
    ui::shell_beats::arm_pulse(&handles.progress, &handles.bar, &handles.queue);
    assert!(
        ui::shell_beats::pulse_standing_for_test(),
        "§2: a run with nothing countable pulses the bar"
    );
    // The rule the timer runs on, and the close that ends it.
    assert!(ui::shell_beats::should_pulse(true, 0.0), "a thinking model pulses");
    assert!(!ui::shell_beats::should_pulse(true, 0.25), "once something counts, the needle owns the bar");
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
    assert!(RAN.load(std::sync::atomic::Ordering::SeqCst), "the widget rounds never ran");
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
        guard: Rc::new(RefCell::new(false)),
        status: gtk4::Label::new(None),
        progress: gtk4::ProgressBar::new(),
        log: gtk4::Expander::new(None),
        log_view: gtk4::TextView::new(),
        queue: Rc::new(RefCell::new(Queue::new())),
        exchange_log: Rc::new(RefCell::new(naivepost::exchanges::RunLog::new())),
        procs: Rc::new(RefCell::new(run::Subprocesses::default())),
    }
}
