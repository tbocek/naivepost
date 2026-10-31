//! §03-shell.md §1 badge **4** — the header's project name/path, and how it narrows.

use std::path::Path;

use adw::prelude::*;
use gtk4 as gtk;

/// §1's ladder, one rung per amount of header room.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fit {
    Roomy,
    Tight,
    Tightest,
}

/// §1's badge **4**.
pub const WIDGET_NAME: &str = "project-title";

/// The separator §1's ladder reads with ("path + tab words"): a middle dot, the app's usual joiner.
const JOIN: &str = " · ";

/// The label's text for one folder, one tab and one width stage.
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

/// The label's natural ask, in characters — see the note on [`build_label`]'s `max-width-chars`.
pub const TITLE_NATURAL_CHARS: i32 = 14;

/// Build §1's badge **4** and pack it into the header after Save.
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
    label.set_max_width_chars(-1);
    // EXPANDS, but never asks for more than it must: with the header's title widget held dead centre
    // (`CenteringPolicy::Strict` in `window.rs`) the START group's natural width is what decides where the
    // centre lands — measured, an uncapped path pulled the tab row's centre from 615 to 736 px in a 1240 px
    // window.
    label.set_max_width_chars(TITLE_NATURAL_CHARS);
    label.set_width_chars(1);
    label.set_single_line_mode(true);
    label.set_hexpand(true);
    // Packed after Save, not centred: the path starts where §1's screenshot puts it and gives way from
    // its own end as the bar narrows.
    label.set_halign(gtk::Align::Start);
    label
}

/// Which stage fits: the widest ladder rung whose natural text still fits `width`.
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

/// Rough width of one label character, the value the ladder measures against.
pub const APPROX_CHAR_PX: i32 = 8;

/// Draw the header label for this folder and tab, at whatever stage its current width allows.
pub fn paint(label: &gtk::Label, dir: &Path, tab: &str) {
    // An unallocated label (width 0 — which is every label before the first size allocation) keeps the
    // widest rung, so a window that has been built but not yet laid out still says something true.
    let stage = stage_for(&label_for(dir, tab, Fit::Roomy), natural_room(label));
    label.set_text(&label_for(dir, tab, stage));
}

/// The room the label has to draw in.
fn natural_room(label: &gtk::Label) -> i32 {
    match label.width() {
        0 => i32::MAX / 4,
        width => width,
    }
}
