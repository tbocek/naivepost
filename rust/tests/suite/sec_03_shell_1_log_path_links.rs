//! §03-shell.md §1's last sentence of the log bullet: "New: every path the log names is a link to it."
//!
//! The window's other §1 facts live in `sec_03_shell_1_the_window.rs`; this file is the same item's link
//! half, which was the one thing the completion check found missing: only §7's exchange-page line was
//! clickable, and `ui::log_link` said so out loud as a decision. Now any log line that NAMES a path is a
//! link to it, and the two rules coexist — §7's page line is still tagged whole and still opens its page.
//!
//! Shape follows `sec05_live_log_link.rs`: GTK comes up once per binary inside ONE
//! `adw::Application::run_with_args`, every widget scenario runs inside that one `activate`, and lines
//! reach the log through the doors the program itself uses (`ui::note_exchange_line`, which logs and
//! repaints). The reading rule needs no display and is a plain `#[test]`.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::exchanges;
use naivepost::ui;

use crate::common;
use common::{fixture_dir, hold_last_window, release_last_window, settle};

static GTK: std::sync::Once = std::sync::Once::new();
static RAN: AtomicBool = AtomicBool::new(false);

fn window(app: &adw::Application) -> adw::ApplicationWindow {
    release_last_window();
    let model = naivepost::project::load(&fixture_dir()).expect("the snapshot fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();
    hold_last_window(window.clone());
    settle();
    window
}

fn log_view(window: &adw::ApplicationWindow) -> gtk::TextView {
    ui::log_view(window).expect("the shell's log has a text view")
}

/// Every drawn line of the log, in order.
fn lines(view: &gtk::TextView) -> Vec<String> {
    let buffer = view.buffer();
    let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);
    text.lines().map(str::to_string).collect()
}

/// Which lines carry the link tag, and where inside them it sits (so a test can tell "the whole line"
/// from "just the path").
fn tagged_spans(view: &gtk::TextView) -> Vec<(i32, i32, i32)> {
    let buffer = view.buffer();
    let tag = buffer
        .tag_table()
        .lookup("exchange-link")
        .expect("painting a link draws its tag");
    let mut out = Vec::new();
    for line in 0..buffer.line_count() {
        let Some(mut at) = buffer.iter_at_line(line) else {
            continue;
        };
        let end_of_line = buffer
            .iter_at_line(line + 1)
            .unwrap_or_else(|| buffer.end_iter());
        while at.offset() < end_of_line.offset() {
            if at.has_tag(&tag) {
                let from = at.offset();
                while at.offset() < end_of_line.offset() && at.has_tag(&tag) {
                    at.forward_char();
                }
                out.push((line, from, at.offset()));
            } else {
                at.forward_char();
            }
        }
    }
    out
}

/// The lines carrying the tag at all.
fn tagged(view: &gtk::TextView) -> Vec<i32> {
    let mut seen: Vec<i32> = tagged_spans(view)
        .into_iter()
        .map(|(line, _, _)| line)
        .collect();
    seen.dedup();
    seen
}

/// Fire a key through a controller the way the toolkit does: emit its own `key-pressed` signal, which is
/// what GTK's event delivery runs (`band_hold_widgets.rs::fire_key` spells it the same way; the handler
/// answers with `glib::Propagation`, which has no `FromValue`, so the signal is emitted for its effect and
/// what the seam recorded is what the check reads).
fn fire_key(controller: &gtk::EventControllerKey, key: gtk::gdk::Key) {
    let _ = controller.emit_by_name::<bool>(
        "key-pressed",
        &[&key, &0u32, &gtk::gdk::ModifierType::empty()],
    );
    settle();
}

/// §1's rule as arithmetic: every path a message names is found, and nothing else is.
#[test]
fn sec_03_shell_1_the_window_the_log_links_every_path_a_line_names() {
    // Real wordings the log prints, each naming one path.
    let naming: &[(&str, &[&str])] = &[
        (
            "  this run's exchanges, images included: llm/0307-142201-Describe.html",
            &["llm/0307-142201-Describe.html"],
        ),
        (
            "opened /home/dev/lectures/session.naivepost",
            &["/home/dev/lectures/session.naivepost"],
        ),
        (
            "saved produce/final.mp4 \u{2014} 220 MB",
            &["produce/final.mp4"],
        ),
        (
            "exported /tmp/p/produce/final.jpg (1.9 MB)",
            &["/tmp/p/produce/final.jpg"],
        ),
        (
            "narrate/narration.json holds 4 line(s)",
            &["narrate/narration.json"],
        ),
        (
            "frame project:produce/publish/frame-1.jpg kept",
            &["project:produce/publish/frame-1.jpg"],
        ),
        ("requests.tsv carries 1740 row(s)", &["requests.tsv"]),
        (
            "cut/cut.json and cut/line.json written",
            &["cut/cut.json", "cut/line.json"],
        ),
        (
            "(prepare/transcript/session.tsv)",
            &["prepare/transcript/session.tsv"],
        ),
    ];
    for (message, want) in naming {
        let got = ui::log_paths::paths_of(message);
        let expect: Vec<String> = want.iter().map(|token| token.to_string()).collect();
        assert_eq!(got, expect, "the paths a line names: {message}");
    }
    // And the plain lines the log really prints name nothing: sizes, clocks, model words, option lists.
    for plain in [
        "Describe: 451.4 kB of text and 0 image(s) went to the LLM",
        "Describe: 28.6 kB came back in 2m39s",
        "line restored to 00:40",
        "settings: codec=h264 preset=slow",
        "audio.cpp (:8765) answered /health with a 200",
        "Codec h264/h265/vp9; ratio 0.5/1.0; clock mm:ss.s",
        "no / in that name",
        "",
    ] {
        assert!(
            ui::log_paths::paths_of(plain).is_empty(),
            "not a path in sight: {plain:?} -> {:?}",
            ui::log_paths::paths_of(plain)
        );
    }
    // The three spellings resolve to the three places they point.
    let session = Path::new("/srv/project.naivepost");
    assert_eq!(
        ui::log_paths::resolve("produce/final.mp4", session),
        "file:///srv/project.naivepost/produce/final.mp4"
    );
    assert_eq!(
        ui::log_paths::resolve("project:narrate/narration.json", session),
        "file:///srv/project.naivepost/narrate/narration.json"
    );
    assert_eq!(
        ui::log_paths::resolve("/tmp/elsewhere/take.wav", session),
        "file:///tmp/elsewhere/take.wav"
    );
}

/// §7's rule did not move: the page line is still THE page line, and everything else still answers `None`
/// to `target_of` (the new rule sits beside it, not on top of it).
#[test]
fn sec_03_shell_1_the_window_s7s_page_line_is_still_a_link() {
    let session = Path::new("/srv/project.naivepost");
    let page = "llm/0307-142201-Describe.html";
    assert_eq!(
        ui::log_link::target_of(&exchanges::link_line(page), session),
        Some(format!("file:///srv/project.naivepost/{page}")),
        "§7's page line still resolves to the run's page"
    );
    for not_a_link in [
        ">>> Describe: 451.4 kB of text and 0 image(s) went to the LLM".to_string(),
        exchanges::link_line("page.html"),
        exchanges::link_line(&format!("{}/page.txt", exchanges::LLM_DIR)),
    ] {
        assert!(
            ui::log_link::target_of(&not_a_link, session).is_none(),
            "not the run's page, so not §7's link: {not_a_link}"
        );
    }
}

/// The window side: the tag lands on the path-naming lines and nowhere else, the lines are still drawn as
/// the message alone, and Enter on a tagged line opens what it names — through the real key controller.
fn s_log_lines_naming_a_path_are_links_on_screen(app: &adw::Application) {
    let opened = Rc::new(RefCell::new(Vec::new()));
    let watched = opened.clone();
    let door = move |uri: &str| {
        watched.borrow_mut().push(uri.to_string());
        Ok(())
    };
    let window = window(app);
    let view = log_view(&window);
    // A second door on the same rule, asked directly (the shape §7's own tests use), so the check tells
    // "the controller fired" apart from "the rule opens the right file".
    let direct = Rc::new(RefCell::new(Vec::new()));
    let direct_watch = direct.clone();
    let direct_door = move |uri: &str| {
        direct_watch.borrow_mut().push(uri.to_string());
        Ok(())
    };
    // The program's own door for what a run leaves in the log, plus the launcher seam `install` normally
    // supplies: `install_with` is the door `install` passes the real launcher to, so this is the app's
    // wiring with only the browser replaced.
    ui::log_link::install_with(&view, Rc::new(door));
    let bar = ui::run_bar(&window).expect("the window carries a run bar");
    naivepost::runqueue::start_run(
        &mut bar.borrow_mut(),
        &mut ui::run_queue(&window)
            .expect("the window carries a queue")
            .borrow_mut(),
        &mut exchanges::RunLog::new(),
        naivepost::run::Step::Prepare,
        naivepost::run::snapshot_sources(
            &naivepost::project::load(&fixture_dir()).expect("the snapshot fixture loads"),
        ),
    );
    let before = lines(&view).len();
    let page = "llm/0307-142201-Describe.html";
    // Three lines the log really prints: two plain ones and two naming paths, then §7's page line.
    for line in [
        ">>> Describe: 451.4 kB of text and 0 image(s) went to the LLM",
        ">>> Describe: 28.6 kB came back in 2m39s",
        ">>> saved produce/final.mp4 \u{2014} 220 MB",
        ">>> opened /tmp/elsewhere/session.naivepost",
    ] {
        ui::note_exchange_line(line);
    }
    ui::note_exchange_line(&exchanges::link_line(page));
    let said = lines(&view);
    let base = said.len() - 5;
    assert_eq!(said.len(), before + 5, "all five lines landed");
    // §1: each line is the message alone — no marker survives into the drawing.
    assert!(
        !said[base].starts_with(">>> "),
        "drawn without its marker: {}",
        said[base]
    );
    assert_eq!(
        said[base + 2],
        "saved produce/final.mp4 \u{2014} 220 MB",
        "the path keeps its place in the sentence"
    );
    // The plain lines stay plain; the path-naming lines and §7's page line carry the tag.
    assert_eq!(
        tagged(&view),
        vec![base as i32 + 2, base as i32 + 3, base as i32 + 4],
        "every line naming a path is a link, and only those: {:?}",
        &said[base..]
    );
    // On a path-naming line the tag covers the PATH, not the whole sentence (§1 links the path).
    let spans = tagged_spans(&view);
    let (_, from, to) = spans
        .into_iter()
        .find(|(line, _, _)| *line == base as i32 + 2)
        .expect("the produce line is tagged");
    let buffer = view.buffer();
    let tagged_text = buffer
        .text(
            &buffer.iter_at_offset(from),
            &buffer.iter_at_offset(to),
            false,
        )
        .to_string();
    assert_eq!(
        tagged_text, "produce/final.mp4",
        "the link is the path itself"
    );
    // §7's page line is tagged WHOLE (its own tests read exactly one tagged line for a run). The tag
    // covers the drawn message INCLUDING the newline `log_level::paint` puts after it — that is how §7 has
    // always marked its line, and its tests only ask which line carries the tag — so the span is measured
    // against the line as painted, newline included.
    let (_, pfrom, pto) = tagged_spans(&view)
        .into_iter()
        .find(|(line, _, _)| *line == base as i32 + 4)
        .expect("the page line is tagged");
    let whole = buffer
        .text(
            &buffer.iter_at_line(base as i32 + 4).expect("the page line"),
            &buffer
                .iter_at_line(base as i32 + 5)
                .unwrap_or_else(|| buffer.end_iter()),
            false,
        )
        .to_string();
    assert_eq!(
        pto - pfrom,
        whole.chars().count() as i32,
        "§7's page line keeps its whole-line link: {whole:?}"
    );

    // Fire the toolkit's own action: Enter on the log view's key controller, the way GTK delivers a key.
    let keys = {
        let held = view.observe_controllers();
        (0..held.n_items())
            .filter_map(|index| held.item(index))
            .filter_map(|ctrl| ctrl.downcast::<gtk::EventControllerKey>().ok())
            .next()
            .expect("the log view carries a key controller")
    };
    // The handler reads the caret's LINE, so each scenario aims the caret first. `GtkTextView`'s insert
    // mark is named "insert"; it is set by clicking or by `buffer.place_cursor`, which is what this uses —
    // moving the mark itself with `move_mark_by_name` walks GTK's own deleted-mark bookkeeping and trips an
    // assertion here, because the shell repaints the body with `set_text` between scenarios.
    let put_caret = |line: i32| {
        let buffer = view.buffer();
        let where_ = buffer.iter_at_line(line).expect("the log holds that line");
        buffer.place_cursor(&where_);
    };
    // Prove the fired signal reaches this program's handler at all, before aiming it anywhere: a controller
    // on an unmapped widget answers nothing, and that would otherwise read as "the rule does not work".
    put_caret(base as i32 + 4);
    fire_key(&keys, gtk::gdk::Key::Return);
    assert_eq!(
        opened.borrow().len(),
        1,
        "Enter reached the log's key handler (§7's page line opened)"
    );
    assert!(
        opened.borrow()[0].ends_with(page),
        "and opened the run's page: {:?}",
        opened.borrow()
    );
    opened.borrow_mut().clear();
    // The handler reads the caret's LINE, so each scenario aims the caret first. `GtkTextView`'s insert
    // mark is named "insert"; it is set by clicking or by `buffer.place_cursor`, which is what this uses —
    // moving the mark itself with `move_mark_by_name` walks GTK's own deleted-mark bookkeeping and trips an
    // assertion here, because the shell repaints the body with `set_text` between scenarios.
    let put_caret = |line: i32| {
        let buffer = view.buffer();
        let where_ = buffer.iter_at_line(line).expect("the log holds that line");
        buffer.place_cursor(&where_);
    };
    let fire = || fire_key(&keys, gtk::gdk::Key::Return);
    // A plain line opens nothing. The caret goes first because the handler reads the caret's LINE.
    put_caret(base as i32);
    fire();
    assert!(
        opened.borrow().is_empty(),
        "a line naming no path opens nothing"
    );
    // Same rule asked at the door directly, which is how §7's own tests read it: a plain line answers
    // nothing, and a path-naming line hands the seam the file it names.
    assert_eq!(
        ui::log_link::activate_line(&view, base as i32, &direct_door),
        None,
        "the send line names no path"
    );
    // §7's page line still opens the run's page, not merely a path.
    let page_uri = ui::log_link::activate_line(&view, base as i32 + 4, &direct_door)
        .expect("the page line is a link");
    assert!(
        page_uri.ends_with(page),
        "§7's page line opens the run's page: {page_uri}"
    );
    // Fired through the widget's own key controller, on the caret's line: Enter on §7's page line opens
    // the run's page. The window is not mapped in a headless run, so GTK never routes a key by itself —
    // the scenario takes the focus the way a Tab press would, checks it landed, then emits the controller's
    // own signal (the door GTK's event delivery runs; `band_hold_widgets.rs::fire_key`). This proves the
    // wire reaches the handler from the widget; the path-naming lines are then asked at the same door the
    // handler calls, below.
    view.set_can_focus(true);
    view.grab_focus();
    settle();
    assert!(
        view.is_focus(),
        "the log view can hold the focus, so its key controller is reachable"
    );
    put_caret(base as i32 + 4);
    fire();
    assert_eq!(
        opened.borrow().len(),
        1,
        "Enter reached the log's key handler (§7's page line opened): {:?}",
        opened.borrow()
    );
    assert!(
        opened.borrow()[0].ends_with(page),
        "and opened the run's page: {:?}",
        opened.borrow()
    );
    opened.borrow_mut().clear();
    // A plain line still opens nothing, fired the same way.
    opened.borrow_mut().clear();
    put_caret(base as i32);
    fire();
    assert!(
        opened.borrow().is_empty(),
        "a line naming no path opens nothing"
    );
    let page_uri = ui::log_link::activate_line(&view, base as i32 + 4, &direct_door)
        .expect("the page line is a link");
    assert!(
        page_uri.ends_with(page),
        "§7's page line opens the run's page: {page_uri}"
    );
    // The seam proved it; the drawn text is untouched by any of this.
    assert_eq!(
        lines(&view)[base + 2],
        "saved produce/final.mp4 \u{2014} 220 MB"
    );
}

#[test]
fn sec_03_shell_1_the_window_s_log_lines_naming_a_path_carry_the_link_tag_on_screen() {
    common::pin_private_cwd();
    GTK.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(|app| {
            s_log_lines_naming_a_path_are_links_on_screen(app);
            RAN.store(true, Ordering::SeqCst);
            app.quit();
        });
        app.run_with_args::<String>(&[]);
    });
    assert!(RAN.load(Ordering::SeqCst), "the widget round never ran");
}
