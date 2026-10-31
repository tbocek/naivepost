//! §03-shell §1, "The window thread never waits": every piece of work that can block (a subprocess, a server
//! call, a sleep, a join, a walk over media folders) runs on a worker thread, and the window thread only takes
//! its answers. This is the one door for that: [`run`] hands `work` a thread of its own and `done` the answer
//! back on the window thread; [`run_with_notes`] does the same for work that reports as it goes (a run's log
//! lines, its queue, the stage it is in).

use std::cell::Cell;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, TryRecvError};
use std::sync::Mutex;
use std::time::Duration;

/// How often the window thread looks for a worker's lines and answer: often enough that a log line or the
/// progress bar moves while the person watches, rarely enough to cost nothing.
pub const POLL: Duration = Duration::from_millis(50);

/// What a worker left for the window thread: a line for the log, or a call that has to run there (a paint
/// reading the window's own state). One list, so the two keep the order they were said in.
enum Held {
    Line(String),
    Call(Box<dyn FnOnce() + Send>),
}

/// Lines and calls said on a worker thread, waiting for the window thread.
static WORKER_LOGS: Mutex<Vec<Held>> = Mutex::new(Vec::new());

/// Workers whose answer the window thread has not taken yet. A test waits for this to reach zero.
static IN_FLIGHT: AtomicUsize = AtomicUsize::new(0);

/// A worker died without answering while a run may have been leaning on it: the run is closed once no worker
/// is left in flight, whichever worker is the last to land ([`close_an_orphaned_run`]).
static ORPHANED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn close_an_orphaned_run() {
    if IN_FLIGHT.load(Ordering::SeqCst) == 0
        && ORPHANED.swap(false, Ordering::SeqCst)
        && crate::ui::run_close::run_open()
    {
        crate::ui::run_close::close_window_run();
    }
}

thread_local! {
    /// Set on a thread [`run`] started: its log lines go to [`WORKER_LOGS`] instead of a thread-local
    /// nobody reads.
    static ON_WORKER: Cell<bool> = const { Cell::new(false) };
}

/// Whether this thread is a worker [`run`] started.
pub(crate) fn on_worker() -> bool {
    ON_WORKER.with(Cell::get)
}

/// A line said on a worker, held for the window thread.
pub(crate) fn hold_line(line: &str) {
    if let Ok(mut held) = WORKER_LOGS.lock() {
        held.push(Held::Line(line.to_string()));
    }
}

pub(crate) fn on_window_thread(call: impl FnOnce() + Send + 'static) {
    if !on_worker() {
        call();
        return;
    }
    if let Ok(mut held) = WORKER_LOGS.lock() {
        held.push(Held::Call(Box::new(call)));
    }
}

/// Every held worker line and call, taken off the list.
fn take_lines() -> Vec<Held> {
    WORKER_LOGS
        .lock()
        .map(|mut held| std::mem::take(&mut *held))
        .unwrap_or_default()
}

/// Put lines in the window's log and run the held calls, oldest first.
fn say(lines: Vec<Held>) {
    for held in lines {
        match held {
            Held::Line(line) => crate::ui::window::log_line(&line),
            Held::Call(call) => call(),
        }
    }
}

/// What a worker sends: the lines it said since its last message travel with each note and with the
/// answer, so the window puts them in the log BEFORE the note is applied, never after it.
enum Message<T, N> {
    Note(Vec<Held>, N),
    Answer(Vec<Held>, T),
}

/// Run `work` on a thread of its own and hand its answer to `done` on the window thread.
pub fn run<T: Send + 'static>(
    name: &str,
    work: impl FnOnce() -> T + Send + 'static,
    done: impl FnOnce(T) + 'static,
) {
    run_with_notes(name, move |_: &dyn Fn(())| work(), |_| {}, done);
}

pub fn run_with_notes<T: Send + 'static, N: Send + 'static>(
    name: &str,
    work: impl FnOnce(&dyn Fn(N)) -> T + Send + 'static,
    mut on_note: impl FnMut(N) + 'static,
    done: impl FnOnce(T) + 'static,
) {
    let (tell, told) = mpsc::channel::<Message<T, N>>();
    IN_FLIGHT.fetch_add(1, Ordering::SeqCst);
    let named = format!("naivepost:{name}");
    let spawned = std::thread::Builder::new().name(named).spawn(move || {
        ON_WORKER.with(|flag| flag.set(true));
        let note_tell = tell.clone();
        let note = move |note: N| {
            let _ = note_tell.send(Message::Note(take_lines(), note));
        };
        let answer = work(&note);
        let _ = tell.send(Message::Answer(take_lines(), answer));
    });
    let what = name.to_string();
    if let Err(why) = spawned {
        IN_FLIGHT.fetch_sub(1, Ordering::SeqCst);
        crate::ui::window::log_line(&format!(
            "!!! {what}: could not start a worker thread -- {why}"
        ));
        return;
    }
    let mut done = Some(done);
    glib::timeout_add_local(POLL, move || loop {
        match told.try_recv() {
            Ok(Message::Note(lines, note)) => {
                say(lines);
                on_note(note);
                crate::ui::run_close::paint_live();
            }
            Ok(Message::Answer(lines, answer)) => {
                say(lines);
                IN_FLIGHT.fetch_sub(1, Ordering::SeqCst);
                if let Some(done) = done.take() {
                    done(answer);
                }
                close_an_orphaned_run();
                return glib::ControlFlow::Break;
            }
            Err(TryRecvError::Empty) => {
                let lines = take_lines();
                if !lines.is_empty() {
                    say(lines);
                    crate::ui::run_close::paint_live();
                }
                return glib::ControlFlow::Continue;
            }
            Err(TryRecvError::Disconnected) => {
                say(take_lines());
                IN_FLIGHT.fetch_sub(1, Ordering::SeqCst);
                crate::ui::window::log_line(&format!(
                    "!!! {what}: the worker ended without an answer (it panicked) -- see the terminal"
                ));
                ORPHANED.store(true, Ordering::SeqCst);
                close_an_orphaned_run();
                crate::ui::run_close::paint_live();
                return glib::ControlFlow::Break;
            }
        }
    });
}

pub fn settle_for_test(limit: Duration) -> bool {
    let until = std::time::Instant::now() + limit;
    let context = glib::MainContext::default();
    while IN_FLIGHT.load(Ordering::SeqCst) > 0 {
        if std::time::Instant::now() > until {
            return false;
        }
        if !context.iteration(false) {
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    while context.iteration(false) {}
    true
}
