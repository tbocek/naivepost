//! Which log lines are clickable, and what clicking one opens.
//!
//! Two rules meet here. §7 names the per-run model exchange page as the log's link line and spells its
//! wording; §1 closes its own log bullet with "New: every path the log names is a link to it", which is
//! the rewrite's rule over the prototype's "only one log line is a link" (the same `Prototype:`/`New:`
//! pairing the chapter uses elsewhere, where `New:` replaces what came before it). So a line is clickable
//! when it is §7's page line — tagged whole, exactly as §7's tests pin it — or when it NAMES a path, in
//! which case the tag covers just that path's span and clicking opens the first path the line names. The
//! reading rule (what counts as a path, and where a token points) lives in [`super::log_paths`], free of
//! widgets; this module owns only the marking and the opening, through GTK's launcher with an `xdg-open`
//! fallback.
//!
//! Split out of `window.rs`, which is past its size budget; the shell only calls [`install`] once and
//! [`paint_line`] per line.

use adw::prelude::*;
use gtk4 as gtk;

use crate::exchanges;

/// The opening leg, injected so a test can watch an activation without launching a browser.
pub type Opener = std::rc::Rc<dyn Fn(&str) -> Result<(), String>>;

/// The line's own prefix, taken from the door that writes it so the reader and the writer cannot drift:
/// The line's own prefix, taken from the door that writes it so the reader and the writer cannot drift:
/// if §7's wording changes, `exchanges::link_line` changes first and this follows it. The page name is
/// the tail of the line, so the prefix is everything up to the LAST `: ` — not the first, because §7's
/// own sentence contains a comma but a page name may hold more, and the tail split is the one that
/// cannot eat part of the wording.
pub fn link_prefix() -> String {
    let sample = exchanges::link_line("");
    match sample.rfind(": ") {
        Some(at) => sample[..at + ": ".len()].to_string(),
        None => sample,
    }
}

/// The page path a link line names — §7's `llm/<name>.html` and nothing else. Split from [`target_of`]
/// so the "is this the link line" question can be asked without a session folder to resolve against.
pub fn page_rel_of(line: &str) -> Option<String> {
    let rel = line.strip_prefix(link_prefix().as_str())?.trim();
    // §7's page lives in `llm/`; anything else the line could name is not this link.
    if !rel.starts_with(&format!("{}/", exchanges::LLM_DIR)) || !rel.ends_with(".html") {
        return None;
    }
    Some(rel.to_string())
}

/// The page a log line points at, as an absolute `file://` URI under `session_dir`.
///
/// Only §7's link line qualifies — §1 forbids a second clickable path, so a line that merely names a
/// file answers `None`. The relative name is resolved against the session folder **at paint time**, not
/// when the line was written: Save as (F0.10) moves the folder mid-session, and a link frozen to the old
/// path would open nothing.
pub fn target_of(line: &str, session_dir: &std::path::Path) -> Option<String> {
    let rel = page_rel_of(line)?;
    Some(format!("file://{}", session_dir.join(rel).display()))
}

/// Draw one log line into `buffer`: its MESSAGE, tagged with its level (§1) and, when the line names
/// something to open, as a link.
///
/// This is the shell's ONE log painter — every repaint of the log body goes through [`paint_lines`] — so
/// §1's "each line is the message alone" holds everywhere the log is drawn, not in one caller. The level
/// and the stripped text come from [`super::log_level`], which answers them with no widget in sight.
///
/// The link tag is created here if this buffer has never had §7's door installed, so a line can never be
/// painted plain merely because nothing connected to the view: the MARK is paint's job, and only the
/// OPENING needs the door. Applied to the whole line because §7 links the line, not a word in it.
///
/// §7's page line is tagged WHOLE, as its own tests (`sec_03_shell_7_…_s7/s8`, which read back exactly one
/// tagged line for a run's three) pin it: its path is the tail of the line, so tagging the token span and
/// tagging the line would differ by the words "this run's exchanges, images included:" — and §7 links the
/// LINE, not a word in it. Every other line that names a path is tagged over the path's own span, which is
/// what §1's "every path the log names is a link to it" asks for while leaving the rest of the sentence as
/// plain text.
pub fn paint_line(buffer: &gtk::TextBuffer, line: &str, session_dir: &std::path::Path) {
    let end_before = buffer.end_iter().offset();
    super::log_level::paint(buffer, line);
    if buffer.tag_table().lookup(LINK_TAG).is_none() {
        buffer.create_tag(Some(LINK_TAG), &[("foreground", &LINK_COLOUR)]);
    }
    let drawn = super::log_level::body_of(line);
    if target_of(line, session_dir).is_some() {
        let start = buffer.iter_at_offset(end_before);
        // The link covers the line as drawn, i.e. the message without its `>>> ` marker: §7 links the
        // line, and §1 draws the line without the marker, so the two rules meet on the message.
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

/// The text of one line of `buffer`, as drawn (so: the message, without any level marker).
pub fn line_text(buffer: &gtk::TextBuffer, line: i32) -> Option<String> {
    let start = buffer.iter_at_line(line)?;
    let end = buffer
        .iter_at_line(line + 1)
        .unwrap_or_else(|| buffer.end_iter());
    Some(buffer.slice(&start, &end, false).trim_end().to_string())
}

/// The buffer's whole body as drawn, one entry per line — §1's message-alone text. FOR THE TEST BINARIES
/// ONLY: the program never asks what the view drew, it asks `window_logs()` for the lines it logged (in
/// §7's marked wording, which is what the sinks and the exchange recorder keep). A test that must show §1's
/// "each line is the message alone" has to read the drawing rather than the sink, and this is that read.
///
/// The trailing empty line a text buffer always carries after its last newline is dropped: it is GTK's
/// insertion point, not a logged message.
pub fn painted_lines_for_test(buffer: &gtk::TextBuffer) -> Vec<String> {
    let mut lines: Vec<String> = (0..buffer.line_count())
        .filter_map(|line| line_text(buffer, line))
        .collect();
    while lines.last().is_some_and(|last| last.is_empty()) {
        lines.pop();
    }
    lines
}

/// Draw a whole log body, one [`paint_line`] per line. The session folder is read once here, at PAINT
/// time, because Save as (F0.10) can move it mid-session; [`activate_line`] reads it again when the line
/// is clicked, so neither moment freezes the other's answer.
pub fn paint_lines(buffer: &gtk::TextBuffer, lines: &[String]) {
    let session = session_now();
    for line in lines {
        paint_line(buffer, line, &session);
    }
}

/// The buffer tag marking the link line. Named so [`paint_line`] can apply it by name after every
/// repaint, since `paint_progress` rewrites the body whole.
const LINK_TAG: &str = "exchange-link";

/// How the link line is drawn: GTK cannot be asked to paint a `links` span on this GTK build (see
/// [`install_with`]), so the tag carries the link colour itself — the shell's log has no other link.
const LINK_COLOUR: &str = "#1c71d8";

/// The folder a log line's page sits under, read at PAINT time and again at ACTIVATION time (never
/// stored): Save as (F0.10) moves the session mid-session, and either moment freezing the answer would
/// leave the link pointing at a folder that no longer exists.
fn session_now() -> std::path::PathBuf {
    crate::startup::open_dir()
}

/// Make `view`'s link line clickable, opening it through [`open`].
pub fn install(view: &gtk::TextView) {
    install_with(view, std::rc::Rc::new(open));
}

/// [`install`] with the opening leg supplied — the seam the program itself uses, so a test can watch an
/// activation without launching a browser (`narrate_tts::speak_line` passes its network legs the same
/// way). `install` is this with the real launcher.
///
/// gtk4-rs 0.11 exposes no reachable link-activation door on `GtkTextView` — no `activate-link`
/// signal and no class slot for one (checked against the installed GTK 4.22 through gtk4-sys) — so the
/// app draws the link itself, as the tag's own `foreground`, and opens it from the keyboard door below.
/// Both that door and a test route through [`activate_line`], which is where the rule lives.
pub fn install_with(view: &gtk::TextView, open: Opener) {
    let buffer = view.buffer();
    // The tag `paint_line` marks the link line with — made there too, so a mark never depends on this
    // having run; what only `install` can give the view is the door that OPENS the line.
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
        // The view's OWN insert mark: the caret a click, a tab to the view, or `buffer.place_cursor`
        // leaves behind. A freshly built unnamed mark reads offset 0 whatever the buffer holds, which would
        // answer "no link" for every line but the first.
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
    // A click on a link line opens it too (§1 "every path the log names is a link to it", §7's page line):
    // the first rewrite opened links from Enter alone, so a person clicking the blue line got nothing. One
    // primary-button click on the line, released where it was pressed; a drag that selects text is not one.
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

/// Where on `view` a click lands on `line` (the middle of its first character), in the view's own
/// coordinates: what a test needs to drive the click gesture at a real line.
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

/// Open a log line, and report what was opened.
///
/// The single door every activation goes through — the Enter handler and a test that cannot synthesise a
/// mouse press. Two rules, in this order:
///
/// * §7's page line opens its run's `llm/` page. That branch is unchanged, so `target_of` keeps §7's
///   meaning ("is this THE page line") and §7's tests stay green for the right reason.
/// * any other line that NAMES a path ([`super::log_paths::paths_of`]) opens its first named path — §1's
///   "every path the log names is a link to it". The activation door carries a LINE and no caret column,
///   so a line naming several paths opens the first one it names; which one that was is on screen, tagged.
///
/// A target is derived from the CURRENT session folder on every call and never stored, so Save as (F0.10)
/// moving the folder mid-session cannot leave a link pointing at a folder that no longer exists.
///
/// A failure to open is logged, not returned: §7 says recording never fails the call, and neither may
/// opening what was recorded. The `Some(uri)` still comes back, because the click WAS the link's.
pub fn activate_line(
    view: &gtk::TextView,
    line: i32,
    open: &dyn Fn(&str) -> Result<(), String>,
) -> Option<String> {
    let buffer = view.buffer();
    let table = buffer.tag_table();
    let tag = table.lookup(LINK_TAG)?;
    let start = buffer.iter_at_line(line)?;
    // A link anywhere on the line: §7's page line is tagged whole, but a named path only over the path, and a
    // path in mid-line (`wrote /…/prepare`) starts untagged. The first rewrite asked only the line's first
    // character, so such a link never opened, by Enter or by click.
    let mut toggle = start.clone();
    let linked =
        start.has_tag(&tag) || (toggle.forward_to_tag_toggle(Some(&tag)) && toggle.line() == line);
    if !linked {
        return None;
    }
    // Read the line as DRAWN (no `>>> ` marker, §1) and put the marker back before asking §7's rule:
    // `target_of` is written against the wording §7 quotes, and the two must not have to agree about
    // whether the marker is on the line. Re-derived from [`super::log_level`] rather than typed, so the
    // two rules cannot drift apart: a line is the link line exactly when the marker + the drawn text is.
    let drawn = line_text(&buffer, line).unwrap_or_default();
    let marked = restore_mark(&drawn);
    let session = session_now();
    let uri = match target_of(&marked, &session) {
        Some(page) => page,
        None => {
            // Not §7's page line: then whatever path it names, if any. A line with no path in it is plain
            // text and nothing opens, which is what keeps an unrelated "h264/vp9" sentence inert.
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

/// Put a level marker back in front of a drawn line, so a rule written against the recorded wording
/// (`target_of`, which matches §7's quoted `">>>   this run's exchanges…"`) can be asked about what the
/// log actually shows. The marker is recovered from the level rule itself, never retyped.
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

/// How many opens this test binary has refused, for the test that pins the refusal.
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

/// Open one page: GTK's launcher first, `xdg-open` as §1's named fallback.
///
/// Never panics and never propagates into the call that logged the line (§7: "recording never fails the
/// call"): failures come back as `Err` and the handler turns them into a log line.
pub fn open(uri: &str) -> Result<(), String> {
    // A test presses the buttons that open things, and the launcher reaches the real desktop whatever display the
    // test window is on: a test run opened the user's files in their session. A test binary opens nothing.
    if running_as_test() {
        BLOCKED_OPENS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        return Ok(());
    }
    // A local file or folder goes through GTK's FILE launcher (the URI launcher is for web pages, and a
    // `file://` folder handed to it through the desktop portal opened nothing and said nothing). A launch the
    // desktop refuses lands in the completion with an error: then the named fallback, and if that fails too, a
    // log line. The first rewrite ignored the completion, so a refused launch was silent.
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
    // The completion is a no-op on purpose: §1 asks for the page to be opened, not for the browser's
    // verdict, and a desktop portal reports success late or not at all. A launcher that cannot even
    // start is the failure worth reporting, and `launch` reports that by panicking out of a callback
    // context, so it is caught here and handed to the fallback.
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

/// §1's named fallback, logged as a shell would take it before it runs. Spawned and not waited on: the press is
/// on the window thread, which never waits (§03-shell §1), and the file manager outlives the call anyway.
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
