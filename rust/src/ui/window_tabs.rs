//! §03-shell.md §1's tab row: the four tab words keep their whole label at the window's minimum size.
//!
//! §1 makes 1240×900 the size at which "every control and lane a page names is on screen at once", and
//! the tabs are named controls (Prepare, Cut, Narrate, Produce). An `adw::ViewSwitcher` in Wide policy
//! ellipsizes its button labels when the centre slot runs short — the shot showed "Prep…", "Narr…",
//! "Prod…" — while badge **4** next to it is spec'd to give way by ellipsizing first ("as the window
//! narrows: path + tab words → file name + tab words"). So the switcher asks for what its words need and
//! never trades them for an ellipsis; the project path, already ellipsizing with a one-character
//! minimum (`header_title`), absorbs whatever width is missing.

use adw::prelude::*;
use gtk4 as gtk;

use crate::ui::window_bench::tab_buttons;

/// Make every label inside this tab row show its whole word.
pub fn keep_labels_whole(switcher: &adw::ViewSwitcher) {
    for button in tab_buttons(switcher.upcast_ref()) {
        for label in labels_of(&button) {
            // No ellipsis, so the button's natural width IS its word; no shrink below that either.
            label.set_ellipsize(gtk::pango::EllipsizeMode::None);
            label.set_max_width_chars(-1);
            label.set_width_chars(-1);
            label.set_hexpand(false);
        }
        // A button that shrinks below its own label would clip it anyway.
        button.set_hexpand(false);
    }
}

/// The tab row's look (§1 badge **5**): the four tabs as one joined group, a frame round the group and a
/// line between neighbours, the shown tab shaded darker. A locked tab keeps libadwaita's dimmed label. The
/// first rewrite drew libadwaita's flat switcher: four loose words with only the shown one shaded.
pub fn style_as_joined(switcher: &adw::ViewSwitcher) {
    switcher.add_css_class(JOINED_CLASS);
    // A group of buttons sitting in the bar, not a strip the bar's height tall.
    switcher.set_valign(gtk::Align::Center);
    static LOADED: std::sync::Once = std::sync::Once::new();
    LOADED.call_once(|| {
        let Some(display) = gtk::gdk::Display::default() else {
            return;
        };
        let provider = gtk::CssProvider::new();
        provider.load_from_string(JOINED_CSS);
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    });
}

/// The class [`style_as_joined`] puts on the switcher, which its CSS is scoped to.
pub const JOINED_CLASS: &str = "naivepost-tabs";

/// One frame round the group; neighbours share a 1 px line; the ends keep the frame's rounding; the shown tab
/// is shaded, a hovered one lightly. The switcher's own padding and the buttons' own shadow are taken off, so
/// the shading meets the one frame line: with them, the shown tab drew a second outline just inside the frame.
const JOINED_CSS: &str = "
viewswitcher.naivepost-tabs {
  border: 1px solid alpha(currentColor, 0.18);
  border-radius: 8px;
  background: alpha(currentColor, 0.03);
  padding: 0;
  border-spacing: 0;
}
viewswitcher.naivepost-tabs > button {
  margin: 0;
  box-shadow: none;
  outline: none;
  min-height: 30px;
  padding: 2px 12px;
  border-radius: 0;
  border: none;
  border-left: 1px solid alpha(currentColor, 0.18);
  background: transparent;
}
viewswitcher.naivepost-tabs > button:first-child {
  border-left: none;
  border-radius: 7px 0 0 7px;
}
viewswitcher.naivepost-tabs > button:last-child {
  border-radius: 0 7px 7px 0;
}
viewswitcher.naivepost-tabs > button box {
  border-spacing: 6px;
}
viewswitcher.naivepost-tabs > button:hover:not(:checked) {
  background: alpha(currentColor, 0.06);
}
viewswitcher.naivepost-tabs > button:checked {
  background: alpha(currentColor, 0.14);
}
";

/// Every label under `root` — a switcher button holds its label inside a box, not directly.
fn labels_of(root: &gtk::Widget) -> Vec<gtk::Label> {
    let mut out = Vec::new();
    for child in root.observe_children().iter::<glib::Object>() {
        let Ok(child) = child else { continue };
        let Ok(widget) = child.downcast::<gtk::Widget>() else {
            continue;
        };
        if let Ok(label) = widget.clone().downcast::<gtk::Label>() {
            out.push(label);
        }
        out.extend(labels_of(&widget));
    }
    out
}
