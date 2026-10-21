//! Redrawing §4's list from OUTSIDE the page — the door `--snapshot 03-sources` and a widget test use to
//! put staged rows on screen through the same painter a removal uses.
//!
//! It lives in its own file rather than in [`crate::ui::source_rows`] because that module is drawn once at
//! build time and repaints only when a row leaves; this adds nothing to the row rules, it only finds the
//! three handles a [`crate::ui::source_rows::Rows`] needs and hands them to the existing
//! [`crate::ui::source_rows::repaint`]. A second, simpler row painter would let a snapshot show controls
//! the live page never draws.

use adw::prelude::*;
use gtk4 as gtk;

use crate::ui::source_rows::{self, Rows};
use crate::ui::window::{find_status, find_widget_by_name};

/// Repaint the window's sources list from the session it now holds. Returns whether the page has a list
/// to repaint (a window built on another page does not), so a caller can say what it actually drew.
pub fn redraw(window: &adw::ApplicationWindow) -> bool {
    let Some(list) = find_widget_by_name(window.upcast_ref(), "sources-list") else {
        return false;
    };
    let Some(status) = find_status(window.upcast_ref()) else {
        return false;
    };
    let Ok(list) = list.downcast::<gtk::ListBox>() else {
        return false;
    };
    // The same handle every page reads, so what is drawn is what the session holds — not a copy of it.
    source_rows::repaint(&Rows {
        session: crate::ui::session_handle(window),
        status,
        list,
    });
    true
}

/// How many rows the page is drawing right now — counted off the widget tree, which is where the truth is
/// after a [`redraw`], not off the project a caller believes it staged.
pub fn drawn_rows(window: &adw::ApplicationWindow) -> usize {
    let Some(list) = find_widget_by_name(window.upcast_ref(), "sources-list") else {
        return 0;
    };
    let Some(list) = list.downcast::<gtk::ListBox>().ok() else {
        return 0;
    };
    list.observe_children()
        .iter::<glib::Object>()
        .filter_map(|child| child.ok())
        .filter(|child| child.downcast_ref::<gtk::ListBoxRow>().is_some())
        .count()
}
