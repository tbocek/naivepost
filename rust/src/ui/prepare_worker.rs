//! The Prepare ▶ on a worker (§03-shell §1, "The window thread never waits"). The press opens the run's bar on
//! the window thread and hands everything that can block to [`work`]: the policy derivation the ▶ door owes
//! (F0.7), F1.1's start (the save, the stale outputs cleared), the audio.cpp preflight (S5), the describe plan,
//! and the walk itself. Lines go to the log as they are said; the queue and the stage come back as [`Note`]s;
//! the window applies the [`Answer`] when it lands. The first rewrite ran all of it inside the click handler.

use crate::layout::Tree;
use crate::prepare_run;
use crate::project::{Policy, Project};
use crate::run_stages::Drive;
use crate::runqueue::Queue;

/// What the worker is handed: the session as ▶ snapshotted it, and the derivation to make first, if one is
/// owed (the context to derive from and the policy the derivation reads as "live").
pub(crate) struct Ask {
    pub asked: Project,
    pub derive: Option<(String, Policy)>,
}

/// What the walk reports while it runs.
pub(crate) enum Note {
    /// The ▶ door's derivation landed: apply it to the session before anything else is said.
    Policy(Policy, crate::policy::Derived, String),
    /// The run's queue after a change, and the stage the walk is in.
    Queue(Queue, String),
}

/// How the press ended.
pub(crate) enum Answer {
    /// F1.1's start or the preflight refused: the sentence for the status line (its log line is said).
    Refused(String),
    /// The walk ran (to its end, to a stop, or to a failure) over this session.
    Walked(Drive, Project),
}

/// The whole press, on the worker. The run's `llm/` page is this thread's (§7 keeps one per thread), so the
/// worker opens and closes it around the work.
pub(crate) fn work(ask: Ask, queue: Queue, note: &dyn Fn(Note)) -> Answer {
    crate::exchange_leg::note_run_started();
    let answer = press(ask, queue, note);
    crate::exchange_leg::note_run_ended();
    answer
}

fn press(ask: Ask, queue: Queue, note: &dyn Fn(Note)) -> Answer {
    let log = crate::ui::window::log_line;
    let mut asked = ask.asked;
    // F0.7's second door: the context nobody derived from yet is derived before the step reads it.
    if let Some((context, live)) = ask.derive {
        crate::policy_call::set_live_policy(&live);
        let mut policy = asked.policy.clone();
        let derived = crate::policy::derive(&mut policy, &context, &mut crate::policy_call::answer);
        asked.policy = policy.clone();
        note(Note::Policy(policy, derived, context));
    }
    // F1.1 S1-S4 against the project's own folder. No folder yet: nothing to clear or save into, and the run
    // walks the empty session like any other page does.
    if let Ok(tree) = Tree::new(crate::startup::open_dir()) {
        match prepare_run::begin(&asked, &tree) {
            prepare_run::Start::Refused { log: line, status } => {
                log(&line);
                return Answer::Refused(status);
            }
            prepare_run::Start::SaveFailed { error } => {
                log(&format!("!!! could not save the project -- {error}"));
                return Answer::Refused(prepare_run::failed_status(&error));
            }
            prepare_run::Start::Started { lines, .. } => {
                for line in &lines {
                    log(line);
                }
                // S5: audio.cpp is asked before any stage runs; a server that cannot serve Prepare refuses. The
                // ask is said before it goes and when it lands: the server can take minutes (a router loading a
                // model answers when it is done), and a silent wait read as a run that had hung.
                log(&asking_audio_line());
                let asked_at = std::time::Instant::now();
                let refused = crate::ui::prepare_guard::why_not(&asked);
                log(&answered_audio_line(asked_at.elapsed()));
                if let Some(why) = refused {
                    log(&format!("!!! {why}"));
                    return Answer::Refused(why);
                }
                say_describe_plan(&tree, &asked);
                // §4's bill: which models this step is about to ask and how many times. It counts the frames
                // on disk, which is why it is said here and not by the press.
                let bill =
                    crate::ui::run_bill::bill_for(crate::run::Step::Prepare, &asked, Some(&tree));
                if bill.asked() > 0 {
                    log(&format!("{}: {}", bill.stage.label(), bill.one_line()));
                }
            }
        }
    }
    let drive = crate::run_stages::drive_worker(
        crate::run::Step::Prepare,
        &asked,
        queue,
        &mut |queue, stage| note(Note::Queue(queue.clone(), stage.to_string())),
    );
    Answer::Walked(drive, asked)
}

/// F1.7's plan, said as the run starts: one forwarder per footage source, what would be sent.
fn say_describe_plan(tree: &Tree, asked: &Project) {
    for source in asked.sources.iter().filter(|source| source.footage) {
        // The lane Describe files a source's work under is its file name minus the extension, the same
        // rule `prepare_run::lane` and `frames.rs` use.
        let lane = source
            .path
            .rsplit('/')
            .next()
            .unwrap_or(&source.path)
            .split_once('.')
            .map(|(stem, _)| stem.to_string())
            .unwrap_or_else(|| source.path.clone());
        for line in crate::describe::stage_log_for(tree, &lane, &[]) {
            crate::ui::window::log_line(&line);
        }
    }
}

/// F1.1 S5's first line: which audio.cpp is asked, and that ⏹ ends the wait.
fn asking_audio_line() -> String {
    let url = crate::settings::from_environment()
        .and_then(|paths| crate::settings::read(&paths).ok())
        .map(|conf| crate::services::audio_endpoint(&conf, |name| std::env::var(name).ok()).url)
        .unwrap_or_default();
    format!(
        ">>> prepare: asking audio.cpp at {url} whether it is up and serves what Prepare needs \
         (a server loading a model can take minutes; \u{23f9} stops the wait)"
    )
}

/// F1.1 S5's second line: how long audio.cpp took to answer.
fn answered_audio_line(took: std::time::Duration) -> String {
    format!(
        ">>> prepare: audio.cpp answered in {:.1} s",
        took.as_secs_f64()
    )
}
