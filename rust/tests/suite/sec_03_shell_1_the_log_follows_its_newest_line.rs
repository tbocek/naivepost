//! §03-shell §1: the log keeps its newest line in view while the person is at its end, keeps their place when
//! they scroll up to read, and follows again once they are back at the end. The first rewrite asked for the end
//! right after replacing the text, before GTK had measured it, so the newest lines ran off the bottom.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::common;
use adw::prelude::*;
use naivepost::ui;

/// Turn the main loop for a while, so the new text is measured and the scroller has moved.
fn turn(for_: Duration) {
    let until = Instant::now() + for_;
    let context = glib::MainContext::default();
    while Instant::now() < until {
        context.iteration(false);
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Write `n` lines and repaint the log through the program's own live door.
fn say(n: usize, tag: &str) {
    let lines: Vec<String> = (0..n).map(|i| format!("{tag} line {i}")).collect();
    naivepost::ui::window::note_startup(&lines);
    naivepost::ui::run_close::paint_live();
    turn(Duration::from_millis(400));
}

#[test]
fn sec_03_shell_1_the_log_follows_its_newest_line() {
    static RAN: AtomicBool = AtomicBool::new(false);
    common::one_app_round(|app| {
        let model = naivepost::project::Project::default();
        let window = ui::build_window(app, &model, "Prepare");
        window.present();
        ui::window::log_expander(&window)
            .expect("the log expander")
            .set_expanded(true);
        let view = naivepost::ui::log_paint::log_view(&window).expect("the log view");
        let adj = view.vadjustment().expect("the log scrolls");
        let at_end = |adj: &gtk4::Adjustment| adj.value() + adj.page_size() >= adj.upper() - 2.0;

        say(200, "first");
        assert!(
            adj.upper() > adj.page_size(),
            "the log is longer than its view"
        );
        assert!(
            at_end(&adj),
            "the newest line is in view: {} + {} of {}",
            adj.value(),
            adj.page_size(),
            adj.upper()
        );

        // The person scrolls up to read, with a real wheel: their place is kept while lines keep coming.
        let scroller = view.parent().expect("the log's scroller");
        crate::common::real_input::real_scroll(&scroller, 20.0, 20.0, -5, &[]);
        turn(Duration::from_millis(100));
        let left = adj.value();
        assert!(
            !at_end(&adj),
            "the wheel took the view up: {left} of {}",
            adj.upper()
        );
        say(50, "second");
        assert!(
            (adj.value() - left).abs() < 2.0,
            "the place they scrolled to is kept: {} (was {left})",
            adj.value()
        );
        assert!(!naivepost::ui::log_paint::follows_end_for_test(&view));

        // Back at the end: it follows again.
        adj.set_value(adj.upper() - adj.page_size());
        turn(Duration::from_millis(100));
        say(50, "third");
        assert!(
            at_end(&adj),
            "following again: {} + {} of {}",
            adj.value(),
            adj.page_size(),
            adj.upper()
        );

        // Folded while a run writes, then opened with a real click on its header: the newest line is what it
        // shows. The first rewrite did not repaint a folded log, and opening it showed the old text (or none).
        let expander = ui::window::log_expander(&window).expect("the log expander");
        expander.set_expanded(false);
        turn(Duration::from_millis(100));
        say(200, "folded");
        crate::common::real_input::real_click(&expander, 12.0, 8.0, 1);
        turn(Duration::from_millis(400));
        assert!(expander.is_expanded(), "the click opened the log");
        let buffer = view.buffer();
        let shown = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);
        assert!(
            shown.contains("folded line 199"),
            "the lines written while it was folded are in it"
        );
        assert!(
            at_end(&adj),
            "opened on its newest line: {} + {} of {}",
            adj.value(),
            adj.page_size(),
            adj.upper()
        );

        // §1: a log that has only grown is appended to, the lines on screen left alone. Emptying and refilling
        // the buffer at every repaint made the view move itself back near the top a moment after the follow put it
        // at the end (traced on the user's machine, 2026-10-26: 57 jumps, never unpinned).
        naivepost::ui::window::clear_logs_for_test();
        say(20, "first");
        let buffer = view.buffer();
        let line5 = buffer.iter_at_line(5).expect("a fifth line");
        let probe = buffer.create_mark(Some("probe"), &line5, true);
        say(20, "more");
        assert_eq!(
            buffer.iter_at_mark(&probe).line(),
            5,
            "the lines already on screen were not replaced"
        );
        let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);
        assert!(text.contains("more line 19"), "the new lines were added");
        assert_eq!(
            text.matches("first line 0").count(),
            1,
            "nothing was painted twice"
        );

        // A log that is not the one on screen plus more (cleared, a new run) is painted whole.
        naivepost::ui::window::clear_logs_for_test();
        say(3, "fresh");
        let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);
        assert!(
            !text.contains("first line"),
            "the old lines are gone: {text}"
        );
        assert!(text.contains("fresh line 2"), "{text}");
        RAN.store(true, Ordering::SeqCst);
        app.quit();
    });
    assert!(RAN.load(Ordering::SeqCst), "the round never ran");
}
