//! §03-shell and §04-prepare, every press through its REAL widget to its real effect: the controls the button
//! inventory found tested only below the widget, or not at all. One window round (one GTK main loop per test
//! process), each check named after the item it proves:
//!
//! - §4 the 🎥 footage toggle, clicked: the session's flag and the status line follow, and a second click undoes it;
//! - §4 the track menu's ticks on a file with two audio streams: one out, the face counts it; the last one cannot
//!   come out and springs back with the reason;
//! - F0.12 S2 the "copy into project" tick: unticked, an added file is referenced where it lies; ticked, copied in;
//! - F0.14 S4 a profile's choice dropdown and its choice+ picks (named, so they can be pressed) store their value;
//! - §1/§7 a log line naming a path opens on a CLICK, through the real opener (the first rewrite opened links from
//!   Enter alone);
//! - §5 closing the Settings dialog leaves it open no more (the first rewrite never took it off its list);
//! - F0.8 S3 New's question: Cancel changes nothing, Start new… puts up the naming sheet.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::common;
use adw::prelude::*;
use gtk4 as gtk;
use naivepost::project::{Project, Source};
use naivepost::ui;

/// Walk down from `node` for the widget named `name`.
fn find_in(node: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
    if node.widget_name() == name {
        return Some(node.clone());
    }
    let mut child = node.first_child();
    while let Some(current) = child {
        if let Some(found) = find_in(&current, name) {
            return Some(found);
        }
        child = current.next_sibling();
    }
    None
}

/// The widget named `name` in `window`, as `W`.
fn named<W: IsA<gtk::Widget>>(window: &impl IsA<gtk::Widget>, name: &str) -> W {
    find_in(window.upcast_ref(), name)
        .unwrap_or_else(|| panic!("no widget named {name}"))
        .downcast::<W>()
        .unwrap_or_else(|_| panic!("{name} is not the widget kind asked for"))
}

/// Turn the main loop until `done`, or `wait` passes.
fn turn_until(wait: Duration, done: impl Fn() -> bool) -> bool {
    let until = Instant::now() + wait;
    let context = glib::MainContext::default();
    while Instant::now() < until {
        if done() {
            return true;
        }
        context.iteration(false);
        std::thread::sleep(Duration::from_millis(5));
    }
    done()
}

/// A two-second test video with `voices` audio streams, written with ffmpeg.
fn video(at: &std::path::Path, voices: usize) {
    let mut command = std::process::Command::new("ffmpeg");
    command.args([
        "-v",
        "error",
        "-y",
        "-f",
        "lavfi",
        "-i",
        "testsrc=size=160x120:rate=5:duration=2",
    ]);
    for n in 0..voices {
        command.args([
            "-f",
            "lavfi",
            "-i",
            &format!("sine=frequency={}:duration=2", 440 + 220 * n),
        ]);
    }
    for n in 0..=voices {
        command.args(["-map", &n.to_string()]);
    }
    let made = command
        .arg("-shortest")
        .arg(at)
        .status()
        .expect("ffmpeg runs");
    assert!(
        made.success(),
        "the test video {} was written",
        at.display()
    );
}

fn status_of(window: &adw::ApplicationWindow) -> String {
    ui::state(window).status
}

/// §4: the 🎥 toggle, clicked as a person clicks it.
fn check_footage(window: &adw::ApplicationWindow) {
    let footage: gtk::ToggleButton = named(window, "footage-0");
    assert!(footage.is_active(), "the first row starts as footage");
    footage.emit_clicked();
    assert!(!footage.is_active());
    assert!(
        !ui::session_handle(window).borrow().sources[0].footage,
        "the session follows the click"
    );
    assert!(
        status_of(window).ends_with("not footage"),
        "{}",
        status_of(window)
    );
    footage.emit_clicked();
    assert!(ui::session_handle(window).borrow().sources[0].footage);
}

/// §4: the track menu of a file holding two audio streams.
fn check_tracks(window: &adw::ApplicationWindow) {
    assert!(
        turn_until(Duration::from_secs(30), || find_in(
            window.upcast_ref(),
            "track-0"
        )
        .is_some()),
        "the two-stream row grows its track menu once its probe lands"
    );
    let menu: gtk::MenuButton = named(window, "track-0");
    // 01 §2: a row that lists no tracks hears its first stream alone (the first rewrite drew it "0/2").
    assert_eq!(
        menu.label().as_deref(),
        Some("1/2"),
        "an untouched row hears its first stream"
    );
    let list: gtk::Box = named(window, "track-popover-0");
    let mut ticks: Vec<gtk::CheckButton> = Vec::new();
    let mut child = list.first_child();
    while let Some(widget) = child {
        if let Ok(tick) = widget.clone().downcast::<gtk::CheckButton>() {
            ticks.push(tick);
        }
        child = widget.next_sibling();
    }
    assert_eq!(ticks.len(), 2, "one tick per stream");
    assert!(
        ticks[0].is_active() && !ticks[1].is_active(),
        "the first ticked, the second not"
    );
    let heard = || {
        ui::session_handle(window).borrow().sources[0]
            .tracks
            .clone()
    };
    ticks[1].set_active(true);
    assert_eq!(
        heard(),
        vec![0, 1],
        "both streams in, 0-based as 01 §2 stores them"
    );
    assert_eq!(menu.label().as_deref(), Some("2/2"), "the face counts it");
    ticks[0].set_active(false);
    assert_eq!(heard(), vec![1], "the first out");
    assert_eq!(menu.label().as_deref(), Some("1/2"));
    ticks[1].set_active(false);
    assert!(
        ticks[1].is_active(),
        "the last ticked track cannot come out: it springs back"
    );
    assert_eq!(heard(), vec![1]);
    assert!(
        status_of(window).contains("track"),
        "and the status says why: {}",
        status_of(window)
    );
    ticks[0].set_active(true);
}

/// F0.12 S2: the tick decides, at the chooser's answer, whether a file is copied in or referenced.
fn check_copy_tick(
    window: &adw::ApplicationWindow,
    root: &std::path::Path,
    session: &std::path::Path,
) {
    let outside = root.join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    video(&outside.join("2026-10-25 11-00-00.mkv"), 1);
    video(&outside.join("2026-10-25 11-05-00.mkv"), 1);
    let tick = ui::copy_into_project(window).expect("the copy tick");
    tick.set_active(false);
    // The chooser's own response body (`add_files` is what its Accept runs).
    assert!(ui::window::add_files(
        window,
        &[outside.join("2026-10-25 11-00-00.mkv")]
    ));
    common::settle_workers();
    let referenced = ui::session_handle(window)
        .borrow()
        .sources
        .iter()
        .any(|source| {
            source.path.ends_with("11-00-00.mkv") && !source.path.starts_with("project:")
        });
    assert!(referenced, "unticked: the file is referenced where it lies");
    assert!(
        !session.join("sources/2026-10-25 11-00-00.mkv").exists(),
        "and not copied"
    );
    tick.set_active(true);
    assert!(ui::window::add_files(
        window,
        &[outside.join("2026-10-25 11-05-00.mkv")]
    ));
    common::settle_workers();
    assert!(
        session.join("sources/2026-10-25 11-05-00.mkv").exists(),
        "ticked: copied into the project's sources/"
    );
    assert!(ui::session_handle(window)
        .borrow()
        .sources
        .iter()
        .any(|source| source.path == "project:sources/2026-10-25 11-05-00.mkv"));
}

/// F0.14 S4: a choice field's dropdown and a choice+ field's offered values, pressed through the widgets.
fn check_profile_choices(window: &adw::ApplicationWindow) {
    let paths = naivepost::settings::from_environment().expect("a settings folder");
    naivepost::profile::save(
        &paths,
        "Choices",
        "---\nkind: choice \"Kind\" = talk | demo\ntone: choice+ \"Tone\" = calm | loud\n---\nA {{kind}}, {{tone}}.\n",
    )
    .expect("the profile is written");
    ui::profile_form::refresh_all();
    common::settle();
    let picker: gtk::DropDown = named(window, "profile-picker");
    let model = picker.model().expect("the picker lists the profiles");
    let at = (0..model.n_items())
        .find(|i| {
            model
                .item(*i)
                .and_downcast::<gtk::StringObject>()
                .is_some_and(|item| item.string() == "Choices")
        })
        .expect("Choices is offered");
    picker.set_selected(at);
    assert!(
        turn_until(Duration::from_secs(5), || find_in(
            window.upcast_ref(),
            "profile-field-kind"
        )
        .is_some()),
        "picking the profile draws its fields"
    );
    let kind: gtk::DropDown = named(window, "profile-field-kind");
    kind.set_selected(1);
    let value = |field: &str| {
        ui::session_handle(window)
            .borrow()
            .profile
            .as_ref()
            .and_then(|used| used.values.get(field).cloned())
            .unwrap_or_default()
    };
    assert_eq!(
        value("kind"),
        "demo",
        "the dropdown's pick is the field's value"
    );
    let pick: gtk::Button = named(window, "profile-field-tone-value-1");
    pick.emit_clicked();
    let tone: gtk::Entry = named(window, "profile-field-tone");
    assert_eq!(tone.text(), "loud", "the offered value lands in the entry");
    assert_eq!(value("tone"), "loud", "and is the field's value");
}

/// §1/§7: a log line naming a path opens on a click, through the window's own (real) opener.
fn check_log_link_click(window: &adw::ApplicationWindow, session: &std::path::Path) {
    ui::window::note_startup(&[format!(">>> wrote {}", session.join("prepare").display())]);
    let expander = ui::window::log_expander(window).expect("the log");
    expander.set_expanded(true);
    ui::run_close::paint_live();
    common::settle();
    let view = ui::log_paint::log_view(window).expect("the log view");
    let buffer = view.buffer();
    let shown = buffer
        .text(&buffer.start_iter(), &buffer.end_iter(), false)
        .to_string();
    let line = shown
        .lines()
        .position(|text| text.contains("wrote") && text.contains("prepare"))
        .expect("the line naming the path is on screen") as i32;
    let (x, y) = ui::log_link::line_point_for_test(&view, line).expect("the line has a place");
    let click = view
        .observe_controllers()
        .iter::<glib::Object>()
        .flatten()
        .find_map(|controller| controller.downcast::<gtk::GestureClick>().ok())
        .expect("the log view answers clicks");
    let before = ui::log_link::blocked_opens_for_test();
    click.emit_by_name::<()>("released", &[&1i32, &x, &y]);
    assert!(
        turn_until(Duration::from_secs(10), || {
            ui::log_link::blocked_opens_for_test() > before
        }),
        "the click reached the opener, which a test binary only counts"
    );
}

/// §5: the gear opens the dialog, and closing it leaves it open no more.
fn check_settings_close(window: &adw::ApplicationWindow) {
    ui::settings_button(window)
        .expect("the gear")
        .emit_clicked();
    assert!(ui::dialog_open(window), "the gear opened Settings");
    let dialog = gtk::Window::list_toplevels()
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk::Window>().ok())
        .find(|top| top.title().as_deref() == Some("Settings") && top.is_visible())
        .expect("the Settings window is up");
    dialog.close();
    common::settle();
    assert!(!ui::dialog_open(window), "closed is not open");
}

/// The visible message dialog asking `question`, if one is up.
fn asking(question: &str) -> Option<adw::MessageDialog> {
    gtk::Window::list_toplevels()
        .into_iter()
        .filter_map(|widget| widget.downcast::<adw::MessageDialog>().ok())
        .find(|dialog| dialog.is_visible() && dialog.heading().as_deref() == Some(question))
}

/// F0.8 S3: New with something to lose asks first; Cancel changes nothing, Start new… asks for the name.
fn check_new_confirm(window: &adw::ApplicationWindow) {
    let question = naivepost::new_project::QUESTION;
    let new_ = ui::new_button(window).expect("the New button");
    let before = ui::window::last_ask();
    new_.emit_clicked();
    let first = asking(question).expect("New asks before it empties anything");
    first.response("cancel");
    common::settle();
    let after_cancel = ui::window::last_ask();
    assert_eq!(
        after_cancel.as_ref().map(|d| d.as_ptr() as usize),
        before.as_ref().map(|d| d.as_ptr() as usize),
        "Cancel put up no naming sheet"
    );
    assert!(
        !ui::session_handle(window).borrow().sources.is_empty(),
        "and emptied nothing"
    );
    new_.emit_clicked();
    let second = asking(question).expect("asked again");
    second.response("start");
    common::settle();
    let named_sheet = ui::window::last_ask().expect("Start new… puts up the naming sheet");
    assert_ne!(
        Some(named_sheet.as_ptr() as usize),
        before.as_ref().map(|d| d.as_ptr() as usize),
        "a new sheet, not an old one"
    );
}

#[test]
fn sec_03_shell_every_press_reaches_its_effect() {
    static RAN: AtomicBool = AtomicBool::new(false);
    common::one_app_round(|app| {
        let root = common::temp_dir_created("every-press");
        let session = root.join("press.naivepost");
        std::fs::create_dir_all(session.join("sources")).expect("sources folder");
        std::env::set_current_dir(&root).expect("cwd pinned");
        video(&session.join("sources/2026-10-25 10-00-00.mkv"), 2);
        video(&session.join("sources/2026-10-25 10-10-00.mkv"), 1);
        let model = Project {
            sources: ["2026-10-25 10-00-00.mkv", "2026-10-25 10-10-00.mkv"]
                .iter()
                .map(|name| Source {
                    path: format!("project:sources/{name}"),
                    footage: true,
                    ..Source::default()
                })
                .collect(),
            ..Project::default()
        };
        naivepost::project::save(&model, &session).expect("the project file");
        naivepost::startup::set_open_dir(&session);
        let window = ui::window::build_window_at(app, &model, "Prepare", &session);
        window.present();
        common::settle_workers();
        check_footage(&window);
        check_tracks(&window);
        check_copy_tick(&window, &root, &session);
        check_profile_choices(&window);
        check_log_link_click(&window, &session);
        check_settings_close(&window);
        check_new_confirm(&window);
        RAN.store(true, Ordering::SeqCst);
        app.quit();
    });
    assert!(RAN.load(Ordering::SeqCst), "the round never ran");
}
