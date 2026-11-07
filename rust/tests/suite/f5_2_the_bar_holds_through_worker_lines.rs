//! F5.2's bar (spec/03-shell.md §2, spec/08-produce.md): a Produce walk reports its own fraction, and the
//! lines its worker says must not knock the bar back. The shell repaints the bar after every batch of worker
//! lines from the run queue, which Produce leaves empty, so the first worker version showed an empty bar
//! through the whole translation while "writing subtitles and the finished file" stood on the status line.

use std::time::Duration;

use naivepost::project::Project;
use naivepost::shell::Page;
use naivepost::ui;

use crate::common;

fn fraction(window: &adw::ApplicationWindow) -> f64 {
    ui::progress_bar(window)
        .expect("the run bar has a progress bar")
        .fraction()
}

#[test]
fn f5_2_a_worker_line_leaves_the_reported_fraction_on_the_bar() {
    common::pin_private_cwd();
    common::one_app_round(|app| {
        use adw::prelude::*;
        let window = ui::build_window(app, &Project::default(), Page::Produce.label());
        window.present();
        common::settle();
        naivepost::run_stages::forget_shown();
        // A run opens the log, and the live repaint paints a log it can see.
        ui::window::log_expander(&window)
            .expect("the window has a log")
            .set_expanded(true);
        common::settle();

        // The walk reports where it stands, then talks: the translate leg's log lines.
        ui::background::run(
            "produce",
            || {
                naivepost::run_stages::report_progress(
                    "writing subtitles and the finished file",
                    Some(1.0),
                );
                std::thread::sleep(Duration::from_millis(150));
                for n in 0..3 {
                    naivepost::run_stages::report_progress("translating", Some(0.75));
                    let _ = n;
                }
            },
            |()| {},
        );
        assert!(
            ui::background::settle_for_test(Duration::from_secs(10)),
            "the worker answered"
        );
        // A worker line lands: the shell's live repaint.
        ui::run_close::paint_live();
        assert!(
            (fraction(&window) - 0.75).abs() < 1e-9,
            "the bar keeps the walk's own fraction through a repaint: {}",
            fraction(&window)
        );

        // Once the run is over, the bar is the queue's again.
        naivepost::run_stages::forget_shown();
        ui::run_close::paint_live();
        assert_eq!(
            fraction(&window),
            0.0,
            "a closed run leaves no fraction behind"
        );
        window.close();
        common::settle();
    });
}
