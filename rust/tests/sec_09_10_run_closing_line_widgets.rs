//! §09-llm-and-tools#10-every-request-timed-new — the closing per-service line, at the run's real end.
//!
//! §10: "At the end of every run the log says it in one line per service: `>>> requests: llm 57 in
//! 2h 13m on the wire, 4m waiting for a slot; audio 38 in 6m 10s; web 3 in 12s`." The numbers, the
//! tally and the sentence are pinned in `tests/request_timing.rs` (s7 and s9-s11, through
//! `request_run`). What those cannot show is that the run's own ending SAYS it, so this file drives the
//! door the program closes a run through — [`naivepost::ui::run_close::close_step_run`], the call
//! `window_run_press` makes when a single-step ▶ finishes — and reads the shell log it wrote.
//!
//! # Why one scenario at a time
//!
//! The rows are filed in the session folder beside this process's working directory
//! (`web_request_log::session_tree`), and `cwd` belongs to the process: the scenario holds a lock for as
//! long as it pins it, the same guard `tests/sec_09_llm_and_tools_10_more_doors.rs` uses.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::layout::Tree;
use naivepost::requests::{self, Request, Service};
use naivepost::run::RunBar;
use naivepost::runqueue::Queue;
use naivepost::ui;

#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;

fn scenario_lock() -> &'static std::sync::Mutex<()> {
    static SCENARIO: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    SCENARIO.get_or_init(|| std::sync::Mutex::new(()))
}

/// A temp root whose session folder is the open project, with `cwd` pinned to it for as long as it lives.
struct Scenario {
    tree: Tree,
    _scenario: std::sync::MutexGuard<'static, ()>,
    _root: std::path::PathBuf,
}

fn scenario(tag: &str) -> Scenario {
    let guard = scenario_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let root = std::env::temp_dir().join(format!("naivepost-s0910close-{tag}"));
    let _ = std::fs::remove_dir_all(&root);
    let session = naivepost::startup::session_dir(&root);
    std::fs::create_dir_all(&session).expect("session folder");
    std::env::set_current_dir(&root).expect("cwd pinned to the scenario root");
    Scenario {
        tree: Tree::new(&session).expect("a .naivepost folder is a project"),
        _scenario: guard,
        _root: root,
    }
}

/// A row that spent a known time on the wire, so the closing line has something to say.
fn worked(service: Service, step: &str, kind: &str, on_wire_s: f64) -> Request {
    Request {
        started: "2026-10-18 10:00:00.000".to_string(),
        step: step.to_string(),
        job: kind.to_string(),
        service,
        model: "qwen-vl".to_string(),
        kind: kind.to_string(),
        on_wire_s: Some(on_wire_s),
        ..Default::default()
    }
}

/// One GTK application for the binary (`gio` refuses a second `run_with_args`); the scenario runs inside
/// it because libadwaita widgets may only be built on the main thread.
static RAN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn window_round() {
    common::one_app_round(|app| {
        closing_line_lands_in_the_window_log(app);
        chain_close_says_it_too();
        RAN.store(true, std::sync::atomic::Ordering::SeqCst);
    });
}

/// The project the shell is built with: one footage source, so a ▶ press finds a step to run rather
/// than a project with nothing in it.
fn footage_project() -> naivepost::project::Project {
    naivepost::project::Project {
        sources: vec![naivepost::project::Source {
            path: "project:sources/lecture.mkv".into(),
            footage: true,
            ..Default::default()
        }],
        ..Default::default()
    }
}

/// §10 through the shell's own close: a run that sent one LLM request and one audio request ends with
/// the sentence naming both, in the shell log the person reads.
fn closing_line_lands_in_the_window_log(app: &adw::Application) {
    let scene = scenario("window");
    let model = footage_project();
    let window = ui::build_window(app, &model, "Prepare");
    window.present();
    naivepost::ui::run_close::clear_log_for_test();

    // The program's own start door, which is also what marks the run's rows: ▶ presses exactly this.
    let bar = ui::run_bar(&window).expect("the shell carries a run bar");
    let queue = ui::run_queue(&window).expect("the shell carries a queue");
    naivepost::runqueue::start_run(
        &mut bar.borrow_mut(),
        &mut queue.borrow_mut(),
        &mut naivepost::exchanges::RunLog::new(),
        naivepost::run::Step::Prepare,
        naivepost::run::Snapshot::default(),
    );
    // The two requests the run "sent": written through the same recorder every leg uses.
    requests::record(&scene.tree, &worked(Service::Llm, "Prepare", "chat", 20.0)).unwrap();
    requests::record(&scene.tree, &worked(Service::Audio, "Prepare", "asr", 6.0)).unwrap();

    // The program's own close door — the call a finished single-step ▶ makes, same handles.
    let status = ui::find_status(window.upcast_ref()).expect("the shell has a status line");
    let progress = ui::progress_bar(&window).expect("the run bar has a progress widget");
    let log = ui::log_expander(&window).expect("the shell has a log expander");
    let view = log
        .child()
        .and_then(|child| child.downcast::<gtk::TextView>().ok())
        .expect("the expander's child is the log body");
    ui::run_close::close_step_run(
        &bar,
        &queue,
        &Rc::new(RefCell::new(naivepost::exchanges::RunLog::new())),
        &status,
        &progress,
        &log,
        &view,
        None,
    );

    let lines = ui::window_logs();
    let said = lines
        .iter()
        .find(|line| line.starts_with(">>> requests:"))
        .unwrap_or_else(|| panic!("the close wrote no closing line, log was {lines:?}"));
    assert!(said.contains("llm 1 in 20s on the wire"), "{said}");
    assert!(said.contains("; audio 1 in 6s"), "{said}");
    assert!(!said.contains("image"), "an untouched service says nothing: {said}");
    window.close();
}

/// The chain's ending says it too, over handles with no window: `close_chain_end` is the door
/// `lucky_run::press` closes the gears' run through.
/// A chain is standing when `close_chain_end` is the door that runs, so the state is stood up the way
/// the driver leaves it: a run open on the bar, and — §2 — the status row lent to the run, which is
/// what lets the close put the queue's own line there.
fn chain_close_says_it_too() {
    // A chain must be standing for its ending to run: `close_chain_step` is the door `lucky_run` uses at
    // every step boundary, and with a next page it hands back the run rather than closing it (§2 wants
    // ⏸/⏹ acting for the whole chain). So a chain with nowhere left to go ends through the same door.
    let scene = scenario("chain");
    naivepost::ui::run_close::clear_log_for_test();
    let handles = naivepost::ui::lucky_run::Handles {
        bar: Rc::new(RefCell::new(RunBar::default())),
        shell: Rc::new(RefCell::new(naivepost::shell::Shell::default())),
        stack: adw::ViewStack::new(),
        guard: Rc::new(RefCell::new(false)),
        status: gtk::Label::new(None),
        progress: gtk::ProgressBar::new(),
        log: gtk::Expander::new(None),
        log_view: gtk::TextView::new(),
        queue: Rc::new(RefCell::new(Queue::new())),
        exchange_log: Rc::new(RefCell::new(naivepost::exchanges::RunLog::new())),
        procs: Rc::new(RefCell::new(naivepost::run::Subprocesses::default())),
    };
    naivepost::runqueue::start_run(
        &mut handles.bar.borrow_mut(),
        &mut handles.queue.borrow_mut(),
        &mut naivepost::exchanges::RunLog::new(),
        naivepost::run::Step::Prepare,
        naivepost::run::Snapshot::default(),
    );
    naivepost::ui::run_close::mark_chain_row(Some(&handles.status));
    requests::record(&scene.tree, &worked(Service::Web, "narrate", "search", 3.0)).unwrap();
    naivepost::ui::run_close::close_chain_step(&handles, None);

    // The chain's ending said the run's own sentence, in §10's spelling and once: read back from the buffer the
    // chain's close writes to, since a chain has no log pane of its own (§2 gives the window log to the run, and
    // a chain's housekeeping answers after it).
    let said = naivepost::ui::run_close::chain_close_lines_for_test()
        .into_iter()
        .filter(|line| line.starts_with(">>> requests:"))
        .collect::<Vec<String>>();
    assert_eq!(said, vec![">>> requests: web 1 in 3s on the wire".to_string()], "one closing line, spelled as every other run ending spells it");
    // §2's rule the close is written under: the queue's line goes on the row, and the queue is emptied
    // for whatever press comes next.
    assert_eq!(
        handles.queue.borrow().text(),
        "",
        "the close reset the queue, so the chain left nothing queued"
    );
    assert!(
        handles.bar.borrow().running.is_none(),
        "§S4: the chain's close ended the press's run"
    );
}

#[test]
fn sec_09_llm_and_tools_10_every_request_timed_new_s12_the_run_close_logs_one_line_per_service() {
    common::pin_private_cwd();
    window_round();
    assert!(
        RAN.load(std::sync::atomic::Ordering::SeqCst),
        "the closing-line round never ran"
    );
}
