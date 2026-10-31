//! Redrawing §4's list from OUTSIDE the page — the door `--snapshot 03-sources` and a widget test use to
//! put staged rows on screen through the same painter a removal uses.

use adw::prelude::*;
use gtk4 as gtk;

use crate::ui::source_rows::{self, Rows};
use crate::ui::window::{find_status, find_widget_by_name};

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
