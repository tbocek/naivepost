//! §7's link line as the log's ONE clickable line.
//!
//! §1 of the same chapter is explicit about which log line may be clicked: "Only one log line is a
//! link — the per-run model exchange page, opened in a browser (GTK's file launcher, `xdg-open`
//! fallback); no other path clickable." So this module answers exactly two questions — which line is
//! the link, and where does it point — and opens it through GTK's launcher with a subprocess fallback.
//! Everything else the log prints stays plain text, including lines that mention a path.
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

/// Draw one log line into `buffer`, tagging it as a link when [`target_of`] finds one.
///
/// The tag is created here if this buffer has never had §7's door installed, so a line can never be
/// painted plain merely because nothing connected to the view: the MARK is paint's job, and only the
/// OPENING needs the door. Applied to the whole line because §7 links the line, not a word in it.
pub fn paint_line(buffer: &gtk::TextBuffer, line: &str, session_dir: &std::path::Path) {
    let end_before = buffer.end_iter().offset();
    buffer.insert(&mut buffer.end_iter(), line);
    buffer.insert(&mut buffer.end_iter(), "\n");
    if target_of(line, session_dir).is_some() {
        if buffer.tag_table().lookup(LINK_TAG).is_none() {
            buffer.create_tag(Some(LINK_TAG), &[("foreground", &LINK_COLOUR)]);
        }
        let start = buffer.iter_at_offset(end_before);
        let end = buffer.end_iter();
        buffer.apply_tag_by_name(LINK_TAG, &start, &end);
    }
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
    crate::startup::session_dir(&std::env::current_dir().unwrap_or_default())
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
        let caret = gtk::TextMark::new(Some("insert"), false);
        let line = buffer.iter_at_mark(&caret).line();
        let held = std::rc::Rc::clone(&key_opener);
        match activate_line(&key_view, line, &|uri| held(uri)) {
            Some(_) => glib::Propagation::Stop,
            None => glib::Propagation::Proceed,
        }
    });
    view.add_controller(keys);
}

/// Open §7's page if `line` of `view`'s buffer is THE link line, and report what was opened.
///
/// The single door every activation goes through — the click handler, the Enter handler, and a test that
/// cannot synthesise a mouse press. Its rule: a line is the link line exactly when it carries
/// [`LINK_TAG`], which only [`paint_line`] puts there; nothing is opened for any other line, so §1's "no
/// other path clickable" holds even if some other line names a file. The target comes from the CURRENT
/// session folder on every call and is never stored, so Save as (F0.10) moving the folder mid-session
/// cannot leave the link pointing at a folder that no longer exists.
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
    if !start.has_tag(&tag) {
        return None;
    }
    let end = buffer
        .iter_at_line(line + 1)
        .unwrap_or_else(|| buffer.end_iter());
    let text = buffer.slice(&start, &end, false).to_string();
    let uri = target_of(text.trim_end(), &session_now())?;
    if let Err(why) = open(&uri) {
        let name = uri.rsplit('/').next().unwrap_or(&uri);
        super::window::log_line(&format!("!!! could not open {name}: {why}"));
    }
    Some(uri)
}

/// Open one page: GTK's launcher first, `xdg-open` as §1's named fallback.
///
/// Never panics and never propagates into the call that logged the line (§7: "recording never fails the
/// call"): failures come back as `Err` and the handler turns them into a log line.
pub fn open(uri: &str) -> Result<(), String> {
    let launcher = gtk::UriLauncher::new(uri);
    // The completion is a no-op on purpose: §1 asks for the page to be opened, not for the browser's
    // verdict, and a desktop portal reports success late or not at all. A launcher that cannot even
    // start is the failure worth reporting, and `launch` reports that by panicking out of a callback
    // context, so it is caught here and handed to the fallback.
    let started = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        launcher.launch(None as Option<&gtk::Window>, None as Option<&gio::Cancellable>, |_result| {});
    }));
    if started.is_ok() {
        return Ok(());
    }
    open_fallback(uri)
}

/// §1's named fallback, run the way every other subprocess here is run: logged as a shell would take it,
/// before it runs.
fn open_fallback(uri: &str) -> Result<(), String> {
    let path = uri.strip_prefix("file://").unwrap_or(uri);
    crate::subprocess::run("xdg-open", &[path], super::window::log_line).map(|_| ())
}
