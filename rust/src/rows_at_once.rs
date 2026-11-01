//! §09 §4: a stage of independent LLM rows, run side by side up to the slots the server says it has. Each recording has its
//! own frames, its own `textscenes.tsv` and its own `events.tsv` and `state.txt`; only the chunks inside one recording chain.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use crate::run_stages::{Stage, Task, VISION_TASK};
use crate::runqueue::Queue;
use crate::services::{Kind, Server};

/// How often the walk reads ⏸ and ⏹ while the rows work.
const POLL: Duration = Duration::from_millis(100);

/// What a stage of rows came to.
pub enum Walked {
    Done { ran: usize, skipped: usize },
    Stopped { ran: usize, left: usize },
    Failed(String),
}

enum Said {
    Line(String),
    Taken,
    Within {
        row: usize,
        fraction: f64,
        /// Requests answered or judges asked by this row so far.
        count: usize,
        /// The requests this row plans; a judge row cannot know how many it will ask.
        planned: Option<usize>,
    },
    Finished {
        row: usize,
        worked: Result<(), String>,
    },
}

/// How many of `stage`'s rows may run at once; `None` when they run in turn: rows that are not independent LLM asks, fewer
/// than two to run, or a server that says it has one slot or says nothing.
pub fn at_once_for(stage: &Stage) -> Option<usize> {
    let independent = |task: &Task| {
        task.program == crate::text_gate::TEXT_GATE_TASK || task.program == VISION_TASK
    };
    if !stage.tasks.iter().all(independent)
        || stage.tasks.iter().filter(|task| !task.already_done).count() < 2
    {
        return None;
    }
    let slots = llm_slots()?;
    (slots > 1).then_some(slots as usize)
}

/// The LLM server's `/health` `slots`, within the gate's range.
fn llm_slots() -> Option<u32> {
    let paths = crate::settings::from_environment()?;
    let conf = crate::settings::read(&paths).ok()?;
    let tree = crate::layout::Tree::new(crate::startup::open_dir()).ok()?;
    let endpoint = crate::services::llm_endpoint(&conf);
    let row =
        crate::server_leg::base_row(Server::Llm, &conf.decider(), "GET", "", "slots", "slots");
    let cancelled = crate::cancel_leg::cancel_check_now();
    let (_, reply) = crate::server_leg::call_dialed(
        &tree,
        &row,
        &crate::server_leg::Dial {
            server: Server::Llm,
            endpoint: &endpoint,
            kind: Kind::Health,
            job: None,
            body: None,
            filename: None,
            cancelled: &cancelled,
            liveness: None,
            waited_ms: 0,
            delta: None,
            budget: None,
            attempt: 1,
        },
    );
    let reply = reply.ok()?;
    let health: serde_json::Value = serde_json::from_str(&reply.body).ok()?;
    let slots = u32::try_from(health.get("slots")?.as_u64()?).ok()?;
    Some(crate::llm_liveness::clamped_slots(slots))
}

/// One row on this worker's thread, with a page of its own (a page holds one open call).
fn run_row(row: usize, task: &Task, tell: &mpsc::Sender<Said>) -> Result<(), String> {
    let _page = crate::exchange_leg::RowPage::open(&format!("r{}", row + 1));
    let mut log = |line: &str| {
        let _ = tell.send(Said::Line(line.to_string()));
    };
    if task.program == VISION_TASK {
        let mut reached = |answered: usize, total: usize| {
            let fraction = if total > 0 {
                answered as f64 / total as f64
            } else {
                1.0
            };
            let _ = tell.send(Said::Within {
                row,
                fraction,
                count: answered,
                planned: Some(total),
            });
        };
        crate::describe_call::run_task(task, &mut log, &mut reached)
    } else {
        let mut reached = |at: crate::text_gate::Reached| {
            let _ = tell.send(Said::Within {
                row,
                fraction: at.fraction(),
                count: at.judges,
                planned: None,
            });
        };
        crate::text_gate::run_task(task, &mut log, &mut reached)
    }
}

/// The stage's rows, `at_once` at a time. Workers only ask and tell; the bar, the log and ⏸ ⏹ stay on the walk's thread.
/// ⏹ ends what is on the wire through the dial's own check, and no row starts after it or after a failure.
#[allow(clippy::too_many_arguments)] // the walk's own state, passed through
pub fn walk(
    stage: &Stage,
    at_once: usize,
    queue: &mut Queue,
    paused: &dyn Fn() -> bool,
    stopped: &dyn Fn() -> bool,
    log: &mut dyn FnMut(&str),
    on_queue: &mut dyn FnMut(&Queue, &str),
) -> Walked {
    let total = stage.tasks.len();
    let endpoint = crate::settings::from_environment()
        .and_then(|paths| crate::settings::read(&paths).ok())
        .map(|conf| crate::services::llm_endpoint(&conf).url)
        .unwrap_or_default();
    let _opened = crate::slot_gate::open_to(&endpoint, at_once as u32);
    log(&format!(
        ">>> {}: {total} recording(s), {at_once} at once -- the LLM server has the slots",
        stage.name
    ));

    let rows: Vec<usize> = (0..total)
        .filter(|row| !stage.tasks[*row].already_done)
        .collect();
    let skipped = total - rows.len();
    let mut done = skipped;
    for _ in 0..skipped {
        queue.take(stage.track);
    }
    let next = AtomicUsize::new(0);
    let hold = AtomicBool::new(false);
    let leave = AtomicBool::new(false);
    let (tell, told) = mpsc::channel();
    let mut ran = 0usize;
    let mut failure: Option<String> = None;
    let mut within = vec![0.0f64; total];
    let mut counts = vec![0usize; total];
    // The stage's whole count, the row's own being a number nobody can read progress from: each row's plan, summed
    // before the first one runs, and corrected by a row whose plan comes out different.
    let mut planned: Vec<usize> = stage
        .tasks
        .iter()
        .map(|task| {
            if task.program == VISION_TASK {
                crate::describe_call::planned_requests(task)
            } else {
                0
            }
        })
        .collect();
    let describes = stage.tasks.iter().any(|task| task.program == VISION_TASK);
    let label = format!("{at_once} at once");
    let words = |done: usize, counts: &[usize], planned: &[usize]| {
        let count: usize = counts.iter().sum();
        if describes {
            let of: usize = planned.iter().sum();
            format!("{done} of {total} recordings, {count} of {of} requests")
        } else {
            format!("{done} of {total} recordings, {count} judge(s)")
        }
    };

    std::thread::scope(|scope| {
        for _ in 0..at_once.min(rows.len()) {
            let tell = tell.clone();
            let (rows, next, hold, leave) = (&rows, &next, &hold, &leave);
            scope.spawn(move || loop {
                while hold.load(Ordering::SeqCst) && !leave.load(Ordering::SeqCst) {
                    std::thread::sleep(POLL);
                }
                if leave.load(Ordering::SeqCst) {
                    return;
                }
                let Some(&row) = rows.get(next.fetch_add(1, Ordering::SeqCst)) else {
                    return;
                };
                let _ = tell.send(Said::Taken);
                let worked = run_row(row, &stage.tasks[row], &tell);
                let _ = tell.send(Said::Finished { row, worked });
            });
        }
        drop(tell);
        loop {
            hold.store(paused(), Ordering::SeqCst);
            if stopped() {
                leave.store(true, Ordering::SeqCst);
            }
            match told.recv_timeout(POLL) {
                Ok(Said::Line(line)) => log(&line),
                Ok(Said::Taken) => queue.take(stage.track),
                Ok(Said::Within {
                    row,
                    fraction,
                    count,
                    planned: plan,
                }) => {
                    within[row] = fraction;
                    counts[row] = count;
                    if let Some(plan) = plan {
                        planned[row] = plan;
                    }
                    let share: f64 = within.iter().sum();
                    crate::run_stages::report_within(
                        queue,
                        on_queue,
                        stage,
                        &label,
                        done,
                        share.min(0.99),
                        &words(done, &counts, &planned),
                    );
                }
                Ok(Said::Finished { row, worked }) => {
                    within[row] = 0.0;
                    done += 1;
                    match worked {
                        Ok(()) => ran += 1,
                        Err(error) => {
                            failure.get_or_insert(error);
                            leave.store(true, Ordering::SeqCst);
                        }
                    }
                    crate::run_stages::report_within(
                        queue,
                        on_queue,
                        stage,
                        &label,
                        done,
                        within.iter().sum::<f64>().min(0.99),
                        &words(done, &counts, &planned),
                    );
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
    });

    if let Some(error) = failure.filter(|_| !stopped()) {
        return Walked::Failed(error);
    }
    if done < total || stopped() {
        return Walked::Stopped {
            ran,
            left: total - ran - skipped,
        };
    }
    Walked::Done { ran, skipped }
}
