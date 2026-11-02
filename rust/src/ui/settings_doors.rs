//! Reading the open Settings dialog from outside: its window, buttons, badges, entries and log.

use crate::ui::settings::*;
use adw::prelude::*;
use gtk4 as gtk;
use std::rc::Rc;

/// The ids the open dialog's model list currently offers, newest first by fetch order.
pub fn listed_models(_window: &adw::ApplicationWindow) -> Vec<String> {
    let Some(list) = find_named("model-list")
        .and_then(|w| w.downcast::<gtk::DropDown>().ok())
        .or_else(|| {
            last_dialog()
                .and_then(|d| {
                    crate::ui::window::find_widget_by_name(d.window.upcast_ref(), "model-list")
                })
                .and_then(|w| w.downcast::<gtk::DropDown>().ok())
        })
    else {
        return Vec::new();
    };
    let Some(model) = list.model() else {
        return Vec::new();
    };
    let strings = match model.downcast::<gtk::StringList>() {
        Ok(strings) => strings,
        Err(_) => return Vec::new(),
    };
    (0..strings.n_items())
        .filter_map(|i| strings.string(i).map(|text| text.to_string()))
        .collect()
}

pub fn open_settings_window() -> Option<adw::Window> {
    last_dialog().map(|dialog| dialog.window.clone())
}

/// One row's Test button, by key (`test-<key>-button`).
pub fn test_button(window: &adw::ApplicationWindow, key: &str) -> Option<gtk::Button> {
    find_named(&format!("test-{key}-button"))?
        .downcast()
        .ok()
        .or_else(|| {
            crate::ui::window::find_widget_by_name(
                window.upcast_ref(),
                &format!("test-{key}-button"),
            )?
            .downcast()
            .ok()
        })
}

/// One row's badge, by key (`badge-<key>`).
pub fn badge(window: &adw::ApplicationWindow, key: &str) -> Option<gtk::Label> {
    last_dialog()
        .and_then(|dialog| dialog.badge(key))
        .or_else(|| {
            crate::ui::window::find_widget_by_name(window.upcast_ref(), &format!("badge-{key}"))?
                .downcast()
                .ok()
        })
}

/// One row's value box, by key (`entry-<key>`) — the box a Test press reads what was typed from.
pub fn entry(window: &adw::ApplicationWindow, key: &str) -> Option<gtk::Entry> {
    last_dialog()
        .and_then(|dialog| dialog.entry(key))
        .or_else(|| {
            crate::ui::window::find_widget_by_name(window.upcast_ref(), &format!("entry-{key}"))?
                .downcast()
                .ok()
        })
}

/// The dialog's own log text, which is what its expander shows.
pub fn dialog_log() -> String {
    if let Some(dialog) = last_dialog() {
        return buffer_text(&dialog.log_view);
    }
    String::new()
}

pub fn dialog_log_open() -> bool {
    if let Some(dialog) = last_dialog() {
        return dialog.log_expander.is_expanded();
    }
    false
}

/// The open dialog's boxes as typed, `(key, value)`, which is what a Test reads and the save writes.
pub fn typed_values() -> Vec<(String, String)> {
    last_dialog()
        .map(|dialog| dialog.typed_all())
        .unwrap_or_default()
}

pub(crate) fn last_dialog() -> Option<Rc<Dialog>> {
    OPEN.with(|open| open.borrow().last().cloned())
}

/// A widget by name inside the open dialog.
pub(crate) fn find_named(name: &str) -> Option<gtk::Widget> {
    let dialog = last_dialog()?;
    crate::ui::window::find_widget_by_name(dialog.window.upcast_ref(), name)
}

pub(crate) fn buffer_text(view: &gtk::TextView) -> String {
    let buffer = view.buffer();
    let start = buffer.start_iter();
    let end = buffer.end_iter();
    buffer.text(&start, &end, false).to_string()
}
