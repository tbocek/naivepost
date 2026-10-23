//! F1.1 S5's door on the ▶ Prepare press: ask audio.cpp before any stage is allowed to run.
//!
//! §S5 sits between the opening lines and the first phase, so the press has to make the call and then
//! either go on or say why it will not. The rule itself — which of health, ASR, diarization and
//! separation-when-asked is missing, and in what order — is `prepare_run::preflight`'s; this file only
//! finds the settings folder and the session tree the window works in, because `window.rs` is past its
//! size budget and a guard written there would be an unreadable twelve-line arm inside a handler that
//! already has four.

use adw::prelude::*;

use crate::{layout, prepare_run, project::Project, settings};

/// Where the press asks about: the machine's settings, if this process has a config home at all.
fn paths() -> Option<settings::Paths> {
    settings::from_environment()
}

/// Why ▶ Prepare must not start, or `None` when audio.cpp is ready for it.
/// Why ▶ Prepare must not start, or `None` when audio.cpp is ready for it.
///
/// Called after S1-S4 (the refusals and the save come first — a session with no sources should hear
/// that, not "audio.cpp is down") and before the bookkeeping opens a run, so a refused preflight never
/// becomes a run at all. No settings folder means nothing to ask about: the same absence that makes
/// every other audio call resolve to the loopback default leaves the preflight to report the server it
/// would dial, rather than passing silently because nothing was configured.
/// Why ▶ Prepare must not start, or `None` when audio.cpp is ready for it.
///
/// Called after S1-S4 (the refusals and the save come first — a session with no sources should hear
/// that, not "audio.cpp is down") and before the bookkeeping opens a run, so a refused preflight never
/// becomes a run at all. No settings folder means nothing to ask about: the same absence that makes
/// every other audio call resolve to the loopback default leaves the preflight to report the server it
/// would dial, rather than passing silently because nothing was configured.
pub fn why_not(project: &Project) -> Option<String> {
    let paths = paths()?;
    let dir = crate::startup::session_dir(&std::env::current_dir().unwrap_or_default());
    let tree = layout::Tree::new(&dir).ok();
    prepare_run::preflight(&paths, &tree, project)
}

/// §S7 + §S8: a walked Prepare press ends here.
///
/// Three things in the spec's order, none of which fits inside the ▶ handler:
///
/// * the ending's own log lines, [`prepare_run::ending_lines`];
/// * the status sentence, [`prepare_run::ending_status`], written into `bar.status` (the caller's
///   `status.set_text(&bar.borrow().status)` is what lands it on the row) and onto `status` directly
///   as well, AFTER [`crate::ui::run_close::close_step_run`] has had its turn — §S7 makes the ending
///   the run's last word, and that close writes the queue's joined track line over whatever is there;
/// * §S8's refresh, through the SAME doors ⟳ Rescan uses (`window::paint_readouts_for` for the
///   Inputs/Outputs readouts, `window::paint_tabs_for` for the tab gates, and the Cut page's own two
///   doors when the run left a cut worth reading) — no second refresh path is invented here.
///
/// `queue` gets the understand phase's share on both tracks so the bar finishes at 1 (§S7's
/// "fraction 1") before the caller repaints it.
pub fn say_ending(
    drive: &crate::run_stages::Drive,
    window: &adw::ApplicationWindow,
    project: &Project,
    bar: &std::rc::Rc<std::cell::RefCell<crate::run::RunBar>>,
    queue: &std::rc::Rc<std::cell::RefCell<crate::runqueue::Queue>>,
) {
    let dir = crate::ui::save_apply::save_target();
    let Some(tree) = layout::Tree::new(&dir).ok() else {
        return;
    };
    finish(
        &prepare_run::ending_of(drive, &tree),
        window,
        project,
        bar,
        queue,
        &dir,
    );
}

/// §S7: the same ending for a session whose tree is known, not the one the window's folder happens
/// to hold. TEST DOOR ONLY — the program's own folder comes from `save_apply::save_target()` inside
/// [`say_ending`]; this variant takes the folder so a test can point the file count at a folder it
/// planted itself. `dir` is used for the whole call so the readouts, the tab gates and the Cut page
/// are repainted from the same folder the count came from.
pub fn say_ending_in_for_test(
    drive: &crate::run_stages::Drive,
    window: &adw::ApplicationWindow,
    project: &Project,
    dir: &std::path::Path,
) {
    let Some(tree) = layout::Tree::new(dir).ok() else {
        return;
    };
    let Some(bar) = crate::ui::shell_beats::run_bar(window) else {
        return;
    };
    let Some(queue) = crate::ui::window::run_queue(window) else {
        return;
    };
    finish(
        &prepare_run::ending_of(drive, &tree),
        window,
        project,
        &bar,
        &queue,
        dir,
    );
}

/// The one place an ending is painted: bar to 1 when it finished, log lines, status sentence, then
/// §S8's refresh. Both doors above hand their [`Ending`] here so the two cannot drift.
fn finish(
    ending: &prepare_run::Ending,
    window: &adw::ApplicationWindow,
    project: &Project,
    bar: &std::rc::Rc<std::cell::RefCell<crate::run::RunBar>>,
    queue: &std::rc::Rc<std::cell::RefCell<crate::runqueue::Queue>>,
    dir: &std::path::Path,
) {
    let Some(status) = crate::ui::window::find_status(window.upcast_ref()) else {
        return;
    };
    // §S7: a finished run says it with the same number the Outputs row is about to show, and its bar
    // ends at 1 — both tracks through the understand phase's top, where §S6 puts the end.
    if matches!(ending, prepare_run::Ending::Prepared { .. }) {
        let share = prepare_run::share(
            prepare_run::Phase::Understand,
            prepare_run::separation_asked(project),
        )
        .1;
        let mut q = queue.borrow_mut();
        q.phase(0.0, share.max(0.0));
        for track in [crate::runqueue::TRACK_STT, crate::runqueue::TRACK_FRAMES] {
            q.done(track, 1.0);
        }
        // §S7's "fraction 1" is only true on screen if the widget is repainted from that queue: the
        // shell's own paint runs after `close_step_run`, which resets the queue, so this is the one
        // moment the finished number still exists to draw. The `drop` is load-bearing — borrowing the
        // queue again while `q` lives panics `RefCell already mutably borrowed`.
        let fraction = q.fraction();
        drop(q);
        if let Some(progress) = crate::ui::window::progress_bar(window) {
            progress.set_fraction(fraction);
        }
    }
    for line in prepare_run::ending_lines(ending) {
        crate::ui::window::log_line(&line);
    }
    let sentence = prepare_run::ending_status(ending);
    bar.borrow_mut().status = sentence.clone();
    status.set_text(&sentence);
    // §S8: the page, the gates and the Cut page see what the run wrote — the doors Rescan already
    // uses, aimed at `dir` so the readouts count the folder the ending counted. A run's end and a ⟳
    // must not drift into two different repaints of one window.
    crate::ui::window::paint_readouts_for(window, project, dir);
    crate::ui::window::paint_tabs_for(window, project);
    let (scan_tree, cut, _narration) = crate::ui::window::session_reads();
    if scan_tree.is_some() {
        crate::ui::window::seed_review_cut(window, &cut);
        crate::ui::cut_cameras::refresh_camera_rows(window);
        crate::ui::window::refresh_effects_lane(window);
    }
}
