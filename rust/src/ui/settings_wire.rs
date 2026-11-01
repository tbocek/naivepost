//! The Settings dialog's wiring: server rows, value boxes and buttons connected to the store and the tests.

use super::settings_servers;
use crate::server_entries::{self, Section};
use crate::ui::settings::*;
use adw::prelude::*;
use gtk4 as gtk;
use std::rc::Rc;

pub(crate) fn wire_servers(dialog: &Rc<Dialog>) {
    let fill = {
        let dialog = Rc::downgrade(dialog);
        move |section: Section, entry: &server_entries::Entry| {
            let Some(dialog) = dialog.upgrade() else {
                return;
            };
            for conf_key in section.conf_keys() {
                let Some(box_key) = server_entries::box_of(conf_key) else {
                    continue;
                };
                let value = entry.get(*conf_key).map(String::as_str).unwrap_or("");
                if let Some((_, input)) = dialog
                    .entries
                    .borrow()
                    .iter()
                    .find(|(key, _)| key == box_key)
                {
                    input.set_text(value);
                }
            }
            if let Some(store) = dialog.store.borrow_mut().as_mut() {
                store.adopt(|conf| server_entries::apply(section, entry, conf));
            }
            // The same text set again emits no change, and the voices folder has no box: arm the beat here.
            dialog.touched();
        }
    };
    let values_now = {
        let dialog = Rc::downgrade(dialog);
        move |section: Section| {
            dialog
                .upgrade()
                .map(|dialog| server_entries::values_from(section, &dialog.typed_conf()))
                .unwrap_or_default()
        }
    };
    let note = {
        let dialog = Rc::downgrade(dialog);
        move |line: &str| {
            if let Some(dialog) = dialog.upgrade() {
                dialog.note(line);
            }
        }
    };
    let conf = dialog.store_base();
    settings_servers::wire(
        settings_servers::Lent {
            fill: Rc::new(fill),
            values_now: Rc::new(values_now),
            note: Rc::new(note),
        },
        &conf,
    );
}

pub(crate) fn register_value_boxes(dialog: &Rc<Dialog>) {
    fn walk(node: &gtk::Widget, out: &mut Vec<(String, gtk::Entry)>) {
        if let (Some(key), Some(entry)) = (
            node.widget_name()
                .strip_prefix("entry-")
                .map(str::to_string),
            node.downcast_ref::<gtk::Entry>(),
        ) {
            out.push((key, entry.clone()));
        }
        let mut child = node.first_child();
        while let Some(c) = child {
            child = c.next_sibling();
            walk(&c, out);
        }
    }
    let mut found = Vec::new();
    walk(dialog.window.upcast_ref(), &mut found);
    let mut entries = dialog.entries.borrow_mut();
    for (key, entry) in found {
        if !entries.iter().any(|(known, _)| *known == key) {
            entries.push((key, entry));
        }
    }
}

pub(crate) fn wire_use_target(dialog: &Rc<Dialog>) {
    for key in ["llm", "decision"] {
        if let Some(entry) = dialog
            .entries
            .borrow()
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, e)| e.clone())
        {
            let focus = gtk::EventControllerFocus::new();
            let key = key.to_string();
            focus.connect_enter(move |_| {
                USE_TARGET.with(|target| *target.borrow_mut() = key.clone());
            });
            entry.add_controller(focus);
        }
    }
}

/// Every value box reports its keystrokes to the store (§5: written 600 ms after the last one).
pub(crate) fn wire_entries_to_store(dialog: &Rc<Dialog>) {
    for (key, entry) in dialog.entries.borrow().iter() {
        let owned = Rc::clone(dialog);
        let key = key.clone();
        entry.connect_changed(move |entry| {
            // §5: typing into a box changes the server entry in use as well.
            settings_servers::edited(&key, &entry.text());
            owned.touched();
        });
    }
}

/// Each `test-<key>-button` gets the row whose entry shares its key.
pub(crate) fn wire_buttons(dialog: &Rc<Dialog>) {
    for key in test_keys() {
        let Some(button) = crate::ui::window::find_widget_by_name(
            dialog.window.upcast_ref(),
            &format!("test-{key}-button"),
        )
        .and_then(|w| w.downcast::<gtk::Button>().ok()) else {
            continue;
        };
        let Some(entry) = crate::ui::window::find_widget_by_name(
            dialog.window.upcast_ref(),
            &format!("entry-{key}"),
        )
        .and_then(|w| w.downcast::<gtk::Entry>().ok()) else {
            continue;
        };
        let Some(badge) = crate::ui::window::find_widget_by_name(
            dialog.window.upcast_ref(),
            &format!("badge-{key}"),
        )
        .and_then(|w| w.downcast::<gtk::Label>().ok()) else {
            continue;
        };
        dialog.entries.borrow_mut().push((key.to_string(), entry));
        dialog.badges.borrow_mut().push((key.to_string(), badge));
        dialog
            .buttons
            .borrow_mut()
            .push((key.to_string(), button.clone()));

        let owned = Rc::clone(dialog);
        let key = key.to_string();
        button.connect_clicked(move |_| owned.ask(&key));
    }

    if let Some(all) =
        crate::ui::window::find_widget_by_name(dialog.window.upcast_ref(), "test-all-button")
            .and_then(|w| w.downcast::<gtk::Button>().ok())
    {
        let owned = Rc::clone(dialog);
        all.connect_clicked(move |_| owned.ask_all());
    }

    // Fetch models and Use: the two buttons that share the Writing block. They are looked up by name
    // like every other control, so neither can be drawn without being wired.
    if let Some(fetch) =
        crate::ui::window::find_widget_by_name(dialog.window.upcast_ref(), "fetch-models-button")
            .and_then(|w| w.downcast::<gtk::Button>().ok())
    {
        let owned = Rc::clone(dialog);
        fetch.connect_clicked(move |_| owned.fetch_models());
    }
    if let Some(use_choice_button) =
        crate::ui::window::find_widget_by_name(dialog.window.upcast_ref(), "use-model-button")
            .and_then(|w| w.downcast::<gtk::Button>().ok())
    {
        let owned = Rc::clone(dialog);
        use_choice_button.connect_clicked(move |_| owned.use_model());
    }
    // The two badges drawn outside `add(...)` join the same key -> badge map, so `report` paints
    // them exactly like a per-row verdict instead of dropping straight into the log.
    for key in [FETCH_KEY, USE_KEY] {
        if let Some(badge) = crate::ui::window::find_widget_by_name(
            dialog.window.upcast_ref(),
            &format!("badge-{key}"),
        )
        .and_then(|w| w.downcast::<gtk::Label>().ok())
        {
            dialog.badges.borrow_mut().push((key.to_string(), badge));
        }
    }
}
