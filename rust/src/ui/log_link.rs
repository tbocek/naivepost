//! Which log lines are clickable, and what clicking one opens.
//!
//! §7 names the per-run model exchange page as the log's link line; §1 adds "every path the log names is a
//! link to it". A line is clickable when it is §7's page line (tagged whole, as §7's tests pin) or NAMES a
//! path (tag covers that span; clicking opens the first). What counts as a path lives in [`super::log_paths`];
//! this module owns the marking and the opening, through GTK's launcher with an `xdg-open` fallback.

use adw::prelude::*;
use gtk4 as gtk;

use crate::exchanges;

pub type Opener = std::rc::Rc<dyn Fn(&str) -> Result<(), String>>;

/// The line's own prefix, taken from the door that writes it (`exchanges::link_line`) so they cannot drift.
/// The prefix is everything up to the LAST `: `, since a page name may hold more than one.
pub fn link_prefix() -> String {
    let sample = exchanges::link_line("");
    match sample.rfind(": ") {
        Some(at) => sample[..at + ": ".len()].to_string(),
        None => sample,
    }
}

/// The page path a link line names: §7's `llm/<name>.html` and nothing else. Split from [`target_of`] so
/// "is this the link line" needs no session folder.
pub fn page_rel_of(line: &str) -> Option<String> {
    let rel = line.strip_prefix(link_prefix().as_str())?.trim();
    // §7's page lives in `llm/`; anything else the line could name is not this link.
    if !rel.starts_with(&format!("{}/", exchanges::LLM_DIR)) || !rel.ends_with(".html") {
        return None;
    }
    Some(rel.to_string())
}

/// The page a log line points at, as an absolute `file://` URI under `session_dir`. Only §7's link line
/// qualifies (§1 forbids a second clickable path). Resolved at **paint time**: Save as (F0.10) moves the
/// folder mid-session and a frozen path would open nothing.
pub fn target_of(line: &str, session_dir: &std::path::Path) -> Option<String> {
    let rel = page_rel_of(line)?;
    Some(format!("file://{}", session_dir.join(rel).display()))
}

/// Draw one log line into `buffer`: its MESSAGE, tagged with its level (§1) and, when the line names
/// something to open, as a link.
///
/// The shell's ONE log painter (every repaint goes through [`paint_lines`]). Level and stripped text come
/// from [`super::log_level`]. The link tag is created here if the buffer never had §7's door installed, so a
/// line is never painted plain just because nothing connected: the MARK is paint's job, the OPENING needs the door.
///
/// §7's page line is tagged WHOLE (its tests read back exactly one tagged line per run); every other line
/// that names a path is tagged over the path's own span (§1).
pub fn paint_line(buffer: &gtk::TextBuffer, line: &str, session_dir: &std::path::Path) {
    let end_before = buffer.end_iter().offset();
    super::log_level::paint(buffer, line);
    if buffer.tag_table().lookup(LINK_TAG).is_none() {
        buffer.create_tag(Some(LINK_TAG), &[("foreground", &LINK_COLOUR)]);
    }
    let drawn = super::log_level::body_of(line);
    if target_of(line, session_dir).is_some() {
        let start = buffer.iter_at_offset(end_before);
        // The link covers the line as drawn (message without its `>>> ` marker): §7 and §1 meet on the message.
        let end = buffer
            .iter_at_line(start.line() + 1)
            .unwrap_or_else(|| buffer.end_iter());
        buffer.apply_tag_by_name(LINK_TAG, &start, &end);
        return;
    }
    for (from, to) in super::log_paths::path_spans(drawn) {
        let start = buffer.iter_at_offset(end_before + from as i32);
        let end = buffer.iter_at_offset(end_before + to as i32);
        buffer.apply_tag_by_name(LINK_TAG, &start, &end);
    }
}

/// The text of one line of `buffer`, as drawn (the message, without any level marker).
pub fn line_text(buffer: &gtk::TextBuffer, line: i32) -> Option<String> {
    let start = buffer.iter_at_line(line)?;
    let end = buffer
        .iter_at_line(line + 1)
        .unwrap_or_else(|| buffer.end_iter());
    Some(buffer.slice(&start, &end, false).trim_end().to_string())
}

/// The buffer's whole body as drawn, one entry per line (§1's message-alone text). FOR THE TEST BINARIES
/// ONLY: the program asks `window_logs()` for the lines it logged. The trailing empty line a text buffer
/// carries after its last newline is dropped.
pub fn painted_lines_for_test(buffer: &gtk::TextBuffer) -> Vec<String> {
    let mut lines: Vec<String> = (0..buffer.line_count())
        .filter_map(|line| line_text(buffer, line))
        .collect();
    while lines.last().is_some_and(|last| last.is_empty()) {
        lines.pop();
    }
    lines
}

/// Draw a whole log body, one [`paint_line`] per line. The session folder is read once, at PAINT time (Save
/// as can move it); [`activate_line`] reads it again on click.
pub fn paint_lines(buffer: &gtk::TextBuffer, lines: &[String]) {
    let session = session_now();
    for line in lines {
        paint_line(buffer, line, &session);
    }
}

/// The buffer tag marking the link line, applied by name after every repaint (`paint_progress` rewrites the body).
const LINK_TAG: &str = "exchange-link";

/// GTK cannot paint a `links` span on this build (see [`install_with`]), so the tag carries the link colour itself.
const LINK_COLOUR: &str = "#1c71d8";

/// The folder a log line's page sits under, read at PAINT and at ACTIVATION time, never stored (Save as moves it).
fn session_now() -> std::path::PathBuf {
    crate::startup::open_dir()
}

/// Make `view`'s link line clickable, opening it through [`open`].
pub fn install(view: &gtk::TextView) {
    install_with(view, std::rc::Rc::new(open));
}

/// [`install`] with the opening leg supplied, so a test can watch an activation without launching a browser.
///
/// gtk4-rs 0.11 exposes no reachable link-activation door on `GtkTextView` (no `activate-link` signal; checked
/// against GTK 4.22 through gtk4-sys), so the app draws the link as the tag's own `foreground` and opens it
/// from the keyboard door below. That door and a test both route through [`activate_line`].
pub fn install_with(view: &gtk::TextView, open: Opener) {
    let buffer = view.buffer();
    // `paint_line` makes the tag too, so a mark never depends on this having run; only `install` gives the OPENING door.
    if buffer.tag_table().lookup(LINK_TAG).is_none() {
        buffer.create_tag(Some(LINK_TAG), &[("foreground", &LINK_COLOUR)]);
    }
    // A read-only view still needs focus for a keyboard to reach the link.
    view.set_can_focus(true);
    // Enter on the caret's line is §7's activation.
    let keys = gtk::EventControllerKey::new();
    let key_view = view.clone();
    let key_opener = std::rc::Rc::clone(&open);
    keys.connect_key_pressed(move |_ctrl, key, _code, _mods| {
        if !matches!(key, gtk::gdk::Key::Return | gtk::gdk::Key::KP_Enter) {
            return glib::Propagation::Proceed;
        }
        let buffer = key_view.buffer();
        // The view's OWN insert mark: a freshly built unnamed mark reads offset 0 and would answer "no link" for every line but the first.
        let Some(caret) = buffer.mark("insert") else {
            return glib::Propagation::Proceed;
        };
        let line = buffer.iter_at_mark(&caret).line();
        let held = std::rc::Rc::clone(&key_opener);
        match activate_line(&key_view, line, &|uri| held(uri)) {
            Some(_) => glib::Propagation::Stop,
            None => glib::Propagation::Proceed,
        }
    });
    view.add_controller(keys);
    let click = gtk::GestureClick::new();
    click.set_button(gtk::gdk::BUTTON_PRIMARY);
    let click_view = view.clone();
    click.connect_released(move |gesture, n_press, x, y| {
        if n_press != 1 {
            return;
        }
        let buffer = click_view.buffer();
        if buffer.has_selection() {
            return;
        }
        let (bx, by) =
            click_view.window_to_buffer_coords(gtk::TextWindowType::Widget, x as i32, y as i32);
        let Some(at) = click_view.iter_at_location(bx, by) else {
            return;
        };
        let held = std::rc::Rc::clone(&open);
        if activate_line(&click_view, at.line(), &|uri| held(uri)).is_some() {
            gesture.set_state(gtk::EventSequenceState::Claimed);
        }
    });
    view.add_controller(click);
}

/// Where on `view` a click lands on `line` (the middle of its first character), for driving the click gesture in a test.
pub fn line_point_for_test(view: &gtk::TextView, line: i32) -> Option<(f64, f64)> {
    let iter = view.buffer().iter_at_line(line)?;
    let rect = view.iter_location(&iter);
    let (wx, wy) = view.buffer_to_window_coords(
        gtk::TextWindowType::Widget,
        rect.x() + 2,
        rect.y() + rect.height() / 2,
    );
    Some((f64::from(wx), f64::from(wy)))
}

/// Open a log line, and report what was opened. The single door every activation goes through (the Enter
/// handler and a test). In order:
///
/// * §7's page line opens its run's `llm/` page (`target_of` keeps §7's meaning).
/// * any other line that NAMES a path ([`super::log_paths::paths_of`]) opens the first one it names (§1).
///
/// The target is derived from the CURRENT session folder on every call (Save as moves it). A failure to open
/// is logged, not returned (§7: recording never fails the call); `Some(uri)` still comes back.
pub fn activate_line(
    view: &gtk::TextView,
    line: i32,
    open: &dyn Fn(&str) -> Result<(), String>,
) -> Option<String> {
    let buffer = view.buffer();
    let table = buffer.tag_table();
    let tag = table.lookup(LINK_TAG)?;
    let start = buffer.iter_at_line(line)?;
    let mut toggle = start;
    let linked =
        start.has_tag(&tag) || (toggle.forward_to_tag_toggle(Some(&tag)) && toggle.line() == line);
    if !linked {
        return None;
    }
    // Read the line as DRAWN (no `>>> ` marker, §1) and put the marker back (re-derived from [`super::log_level`])
    // before asking §7's rule, which is written against the quoted wording.
    let drawn = line_text(&buffer, line).unwrap_or_default();
    let marked = restore_mark(&drawn);
    let session = session_now();
    let uri = match target_of(&marked, &session) {
        Some(page) => page,
        None => {
            // Not §7's page line: then whatever path it names; a line with no path stays inert ("h264/vp9").
            let first = super::log_paths::paths_of(&drawn).into_iter().next()?;
            super::log_paths::resolve(&first, &session)
        }
    };
    if let Err(why) = open(&uri) {
        let name = uri.rsplit('/').next().unwrap_or(&uri);
        super::window::log_line(&format!("!!! could not open {name}: {why}"));
    }
    Some(uri)
}

/// Put a level marker back in front of a drawn line so a rule written against the recorded wording
/// (`target_of`) can be asked about what the log shows; recovered from the level rule, never retyped.
fn restore_mark(drawn: &str) -> String {
    for (mark, level) in [
        (">>> ", super::log_level::Level::Progress),
        ("!!! ", super::log_level::Level::Failure),
        ("    ", super::log_level::Level::Detail),
    ] {
        if super::log_level::level_of(&format!("{mark}{drawn}")) == level {
            return format!("{mark}{drawn}");
        }
    }
    drawn.to_string()
}

static BLOCKED_OPENS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// How many opens this test binary has refused.
pub fn blocked_opens_for_test() -> usize {
    BLOCKED_OPENS.load(std::sync::atomic::Ordering::SeqCst)
}

/// Whether this process is a cargo test binary: those, and only those, live in a `deps` folder under `target`.
fn running_as_test() -> bool {
    std::env::current_exe().is_ok_and(|exe| {
        exe.parent()
            .and_then(|dir| dir.file_name())
            .is_some_and(|dir| dir == "deps")
    })
}

/// Open one page: GTK's launcher first, `xdg-open` as §1's named fallback. Never panics or propagates
/// into the call that logged the line (§7); failures come back as `Err` for the handler to log.
pub fn open(uri: &str) -> Result<(), String> {
    // A test binary opens nothing: the launcher reaches the real desktop whatever display the test window is on.
    if running_as_test() {
        BLOCKED_OPENS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        return Ok(());
    }
    if let Ok((path, _)) = glib::filename_from_uri(uri) {
        let owned = uri.to_string();
        gtk::FileLauncher::new(Some(&gio::File::for_path(&path))).launch(
            None as Option<&gtk::Window>,
            None as Option<&gio::Cancellable>,
            move |result| {
                if let Err(why) = result {
                    if let Err(fallback) = open_fallback(&owned) {
                        super::window::log_line(&format!(
                            "!!! could not open {owned}: {why}; {fallback}"
                        ));
                    }
                }
            },
        );
        return Ok(());
    }
    let launcher = gtk::UriLauncher::new(uri);
    // The completion is a no-op on purpose (§1 asks for the page to be opened; a portal reports late or never).
    // A launcher that cannot start panics out of a callback context, so it is caught and handed to the fallback.
    let started = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        launcher.launch(
            None as Option<&gtk::Window>,
            None as Option<&gio::Cancellable>,
            {
                let owned = uri.to_string();
                move |result| {
                    if let Err(why) = result {
                        if let Err(fallback) = open_fallback(&owned) {
                            super::window::log_line(&format!(
                                "!!! could not open {owned}: {why}; {fallback}"
                            ));
                        }
                    }
                }
            },
        );
    }));
    if started.is_ok() {
        return Ok(());
    }
    open_fallback(uri)
}

/// §1's named fallback. Spawned and not waited on: the window thread never waits (§03-shell §1).
fn open_fallback(uri: &str) -> Result<(), String> {
    // The path itself, decoded (a folder with a space in its name is `%20` in its URI).
    let decoded = glib::filename_from_uri(uri)
        .map(|(path, _)| path.display().to_string())
        .unwrap_or_else(|_| uri.to_string());
    let path = decoded.as_str();
    super::window::log_line(&crate::subprocess::log_line("xdg-open", &[path]));
    let mut child = std::process::Command::new("xdg-open")
        .arg(path)
        .stdin(std::process::Stdio::null())
        .spawn()
        .map_err(|err| {
            format!(
                "{} failed: {err}",
                crate::subprocess::shown("xdg-open", &[path])
            )
        })?;
    // Reaped on a thread of its own, so no zombie is left behind.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}
