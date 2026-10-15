//! §03-shell §7's link line where a person reads it: inside the shell's own log, painted through the
//! door every flow writes to, and clickable through the one door that opens it.
//!
//! `model_exchange_log_live.rs` pins the recorder and `model_exchange_log.rs` the wording; what nothing
//! reached before is §7's LAST clause — the line is the run's page and §1 makes it the log's only
//! clickable line. These drive the real widgets: a line put in through `ui::note_exchange_line`
//! (the door §7's recorder writes through, the one that also opens the expander a run would have opened)
//! and opened through `ui::log_link::activate_line` with the launcher replaced by the seam the program
//! itself uses (`install_with`, the same door `install` passes its real launcher to).

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::exchanges;
use naivepost::ui;

#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::{fixture_dir, hold_last_window, release_last_window};

/// The one GTK application this binary's widget round needs; the round runs every scenario inside one
/// `activate`, which is also what brings GTK up for the process.
static GTK: std::sync::Once = std::sync::Once::new();

/// One real window, freshly built on the fixture session, with the previous check's window retired
/// first so a widget lookup cannot land in a closed tree.
fn window(app: &adw::Application) -> adw::ApplicationWindow {
    release_last_window();
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();
    hold_last_window(window.clone());
    window
}

/// The log's text view: the expander's own body. The recursive name finder cannot reach it (an expander
/// keeps its child behind an internal widget), which is why `snapshot.rs` and every other reader take
/// this route.
fn log_view(window: &adw::ApplicationWindow) -> gtk::TextView {
    ui::log_view(window).expect("the shell's log has a text view")
}

/// Every line in the log's buffer, one per line, in order.
fn lines(view: &gtk::TextView) -> Vec<String> {
    let buffer = view.buffer();
    let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);
    text.lines().map(str::to_string).collect()
}

/// The lines carrying §7's link tag.
fn tagged(view: &gtk::TextView) -> Vec<i32> {
    let buffer = view.buffer();
    let tag = buffer
        .tag_table()
        .lookup("exchange-link")
        .expect("painting the link line draws its tag");
    (0..buffer.line_count())
        .filter_map(|line| {
            let iter = buffer.iter_at_line(line)?;
            iter.has_tag(&tag).then_some(line)
        })
        .collect()
}

/// Which line is a link, and which line its page sits on: §1 lets exactly one line be clickable, so a
/// second one is a bug no amount of correct wording would catch.
#[test]
fn sec_03_shell_7_the_model_exchange_log_llm_s7_the_run_page_line_is_the_log_s_only_clickable_one() {
    common::pin_private_cwd();
    static RAN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    GTK.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(|app| {
            let window = window(app);
            let view = log_view(&window);
            // What a real call leaves in the log: two lines per call, then the once-per-run link. They
            // go in through §7's own door, not by poking the buffer -- the door is what decides that the
            // log opens for a run's news.
            let page = "llm/0307-142201-Describe.html";
            // A run stands, because that is the state a call's lines are written in (§F0.5 S1 opens the
            // log FOR a run; the door leaves an idle shell's log alone).
            let bar = ui::run_bar(&window).expect("the window carries a run bar");
            naivepost::runqueue::start_run(
                &mut bar.borrow_mut(),
                &mut ui::run_queue(&window).expect("the window carries a queue").borrow_mut(),
                &mut exchanges::RunLog::new(),
                naivepost::run::Step::Prepare,
                naivepost::run::snapshot_sources(&naivepost::project::load(&fixture_dir()).expect("fixture loads")),
            );
            ui::note_exchange_line(">>> Describe: 451.4 kB of text and 0 image(s) went to the LLM");
            ui::note_exchange_line(">>> Describe: 28.6 kB came back in 2m39s");
            ui::note_exchange_line(&exchanges::link_line(page));
            let said = lines(&view);
            // The shell may have logged before this press's own news (the fixture's saved red line puts
            // "line restored to 00:40" first), so the three exchange lines are the LAST three rather than
            // the first three. Counting from the end is what makes this check about §7's recorder and not
            // about whatever the window happened to print on its way up.
            let base = said.len() - 3;
            assert!(
                said[base].ends_with("of text and 0 image(s) went to the LLM"),
                "the send line comes first: {said:?}"
            );
            assert!(
                said[base + 1].starts_with(">>> Describe: ") && said[base + 1].contains("came back in"),
                "then the verdict line: {said:?}"
            );
            assert_eq!(said[base + 2], exchanges::link_line(page), "the link line keeps §7's wording");
            assert_eq!(
                tagged(&view),
                vec![base as i32 + 2],
                "exactly the link line carries §7's link tag: {said:?}"
            );
            // And it is the one line the opener door opens: anything else answers nothing, so no other
            // path in the log can be clicked (§1: "no other path clickable").
            let opened = Rc::new(RefCell::new(Vec::new()));
            let watched = opened.clone();
            let door = move |uri: &str| {
                watched.borrow_mut().push(uri.to_string());
                Ok(())
            };
            assert_eq!(
                ui::log_link::activate_line(&view, base as i32, &door),
                None,
                "the send line is plain text"
            );
            assert_eq!(
                ui::log_link::activate_line(&view, base as i32 + 1, &door),
                None,
                "the verdict line is plain text"
            );
            let opened_uri = ui::log_link::activate_line(&view, base as i32 + 2, &door)
                .expect("the run's page line opens");
            assert!(
                opened_uri.starts_with("file://") && opened_uri.ends_with(page),
                "the link points at the run's page under the session: {opened_uri}"
            );
            assert_eq!(
                *opened.borrow(),
                vec![opened_uri.clone()],
                "and one activation opened exactly one thing"
            );
            let expander = ui::log_expander(&window).expect("the shell carries a log expander");
            assert!(
                expander.is_expanded(),
                "S1: the log is open while its own lines are being written"
            );
            window.close();
            RAN.store(true, std::sync::atomic::Ordering::SeqCst);
            app.quit();
        });
        app.run_with_args::<String>(&[]);
    });
    assert!(RAN.load(std::sync::atomic::Ordering::SeqCst), "the widget round never ran");
}
