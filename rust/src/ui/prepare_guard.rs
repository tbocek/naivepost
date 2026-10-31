//! F1.1 S5's door on the ▶ Prepare press: ask audio.cpp before any stage is allowed to run.

use adw::prelude::*;

use crate::{layout, prepare_run, project::Project, settings};

/// Why ▶ Prepare must not start, or `None` when audio.cpp is ready for it. Why ▶ Prepare must not
/// start, or `None` when audio.cpp is ready for it.
pub fn why_not(project: &Project) -> Option<String> {
    let paths = settings::from_environment()?;
    let dir = crate::startup::open_dir();
    let tree = layout::Tree::new(&dir).ok();
    prepare_run::preflight(&paths, &tree, project)
}

/// §S7 + §S8: a walked Prepare press ends here.
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
/// to hold.
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
/// §S8's refresh.
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
    // §S7: a finished run says it with the same number the Outputs row is about to show, and its
    // bar ends at 1.
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
        // moment the finished number still exists to draw.
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
    // §S8: the page, the gates and the Cut page see what the run wrote.
    crate::ui::window::paint_readouts_for(window, project, dir);
    crate::ui::window::paint_tabs_for(window, project);
    let (scan_tree, cut, _narration) = crate::ui::window::session_reads();
    if scan_tree.is_some() {
        crate::ui::window::seed_review_cut(&cut);
        crate::ui::cut_cameras::refresh_camera_rows(window);
        crate::ui::window::refresh_effects_lane(window);
    }
}
