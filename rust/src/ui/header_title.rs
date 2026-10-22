//! §03-shell.md §1 badge **4** — the header's project name/path, and how it narrows.
//!
//! §1: "project name/path (as the window narrows: path + tab words → file name + tab words → file name +
//! icons only)". Three readings of the same fact, chosen by how much room the header leaves: the full
//! path while there is room, then the folder's own name, then the name alone with the tab words gone
//! (the icons are the header's own buttons and always stay).
//!
//! The rule is a pure function of (folder, tab, fit) so a test can pin the ladder without a window; the
//! widget half only picks the stage and draws it.
//!
//! Split out of `window.rs`, which is past its size budget.

use std::path::Path;

use adw::prelude::*;
use gtk4 as gtk;

/// §1's ladder, one rung per amount of header room. Named for what they hold, per §1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fit {
    /// Path + tab words.
    Roomy,
    /// File name + tab words.
    Tight,
    /// File name only.
    Tightest,
}

/// §1's badge **4**.
pub const WIDGET_NAME: &str = "project-title";

/// The separator §1's ladder reads with ("path + tab words"): a middle dot, the app's usual joiner.
const JOIN: &str = " · ";

/// The label's text for one folder, one tab and one width stage.
///
/// An empty half is left out rather than joined: a window that knows no folder yet shows the tab word
/// alone, and a tab word that is missing shows the path alone. Never a stray separator.
pub fn label_for(dir: &Path, tab: &str, fit: Fit) -> String {
    let name = dir
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let path = dir.to_string_lossy().into_owned();
    let head = match fit {
        Fit::Roomy if !path.is_empty() => path,
        _ => name,
    };
    let tail = if fit == Fit::Tightest { "" } else { tab };
    match (head.as_str(), tail) {
        ("", "") => String::new(),
        ("", _) => tail.to_string(),
        (_, "") => head,
        (head, tail) => format!("{head}{JOIN}{tail}"),
    }
}

/// The label's own width floor, in characters: below this the label has no readable content at all,
/// which is what makes it the thing that pins the window's minimum width.
pub const TITLE_MIN_CHARS: i32 = 1;

/// The label's natural ask, in characters — see the note on [`build_label`]'s `max-width-chars`. Long enough to
/// read a real project path, short enough that the start group leaves the header's two sides balanced for the
/// tab row to sit in its centre at §1's 1240 px (measured: 40 chars keeps the tabs centred, no cap does not).
pub const TITLE_NATURAL_CHARS: i32 = 40;

/// Build §1's badge **4** and pack it into the header after Save. §1: "project name/path packed after
/// Save", with the tab row taking the centre slot — so this goes on the bar's start side, where it sits
/// right behind New/Open/Save and gives way from its own end as the window narrows. Installed here rather
/// than by the caller so the name, the dimming and the ellipsis cannot be half-done.
pub fn install(header: &adw::HeaderBar) -> gtk::Label {
    let label = build_label();
    // §1 packs badge **4** after Save, so it belongs on the bar's start side rather than in the centre.
    header.pack_start(&label);
    label
}

/// The label itself, without a place in the bar: one construction so the name, the dimming and the
/// ellipsis cannot be half-done whichever side the caller packs it on.
fn build_label() -> gtk::Label {
    let label = gtk::Label::new(None);
    label.set_widget_name(WIDGET_NAME);
    label.add_css_class("dim-label");
    label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    // The label must never be the thing that holds the window wide (§1: what no longer fits gives way
    // instead of pinning the shell), so its MINIMUM is one character. But a `gtk::Label` with a small
    // `width-chars` also caps how much it asks for, which is why the natural width is asked for here and
    // the ellipsis only ever trims it: without this the header showed "…" at any window size.
    label.set_max_width_chars(-1);
    // EXPANDS, but never asks for more than it must: with the header's title widget held dead centre
    // (`CenteringPolicy::Strict` in `window.rs`) the START group's natural width is what decides where the
    // centre lands — measured, an uncapped path pulled the tab row's centre from 615 to 736 px in a 1240 px
    // window. `max-width-chars` caps the NATURAL ask only; the minimum stays one character (`width_chars(1)`),
    // so a narrow window still ellipsizes this label instead of being held wide.
    label.set_max_width_chars(TITLE_NATURAL_CHARS);
    label.set_width_chars(1);
    label.set_single_line_mode(true);
    label.set_hexpand(true);
    // Packed after Save, not centred: the path starts where §1's screenshot puts it and gives way from
    // its own end as the bar narrows.
    label.set_halign(gtk::Align::Start);
    label
}

/// Which stage fits: the widest ladder rung whose natural text still fits `width`. Measured against the
/// text's own natural width rather than a fixed character count — §10's prototype numbers (`headSlack`
/// 90, name 28 chars) are what this replaces with something that follows the actual header. A width of
/// 0 (never allocated yet) keeps the widest rung, which is what §1's minimum-size 1240×900 window shows.
pub fn stage_for(text: &str, width: i32) -> Fit {
    if width <= 0 {
        return Fit::Roomy;
    }
    let natural = text.chars().count() as i32 * APPROX_CHAR_PX;
    if natural <= width {
        Fit::Roomy
    } else if natural / 2 <= width {
        Fit::Tight
    } else {
        Fit::Tightest
    }
}

/// Rough width of one label character, the value the ladder measures against. Chosen so the rung that
/// survives is the one §1's screenshot shows at 1240 px (path + tab words), and so a path roughly twice
/// that width drops to the name — the prototype's own slack, expressed per character.
pub const APPROX_CHAR_PX: i32 = 8;

/// Draw the header label for this folder and tab, at whatever stage its current width allows.
pub fn paint(label: &gtk::Label, dir: &Path, tab: &str) {
    // An unallocated label (width 0 — which is every label before the first size allocation) keeps the
    // widest rung, so a window that has been built but not yet laid out still says something true.
    let stage = stage_for(&label_for(dir, tab, Fit::Roomy), natural_room(label));
    label.set_text(&label_for(dir, tab, stage));
}

/// The room the label has to draw in. `gtk::Label` reports 0 until it has been allocated, and its own
/// natural width afterwards; before any allocation there is nothing to compare against, so `stage_for`
/// gets a width big enough to keep the widest rung.
fn natural_room(label: &gtk::Label) -> i32 {
    match label.width() {
        0 => i32::MAX / 4,
        width => width,
    }
}
