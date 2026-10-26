//! §03-shell.md §5: the Settings dialog's named server entries, the widget half. Writing, Audio and Drawing each
//! get a dropdown of entries with ＋ New and 🗑 Delete under their heading; [`crate::server_entries`] holds the
//! rules and the file. Picking an entry fills the section's boxes and makes its values the ones `llm.conf` holds;
//! typing into a box writes through to the entry in use as well. Split out of `settings.rs`, which is past its
//! size budget; the dialog hands over the three things it owns (filling its boxes, the boxes' current values,
//! its log) as closures.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk4 as gtk;

use crate::server_entries::{self, Entry, Section, Servers};

/// The section's widgets, built under its heading and wired once the dialog exists.
#[derive(Clone)]
struct Controls {
    dropdown: gtk::DropDown,
    new: gtk::MenuButton,
    name: gtk::Entry,
    create: gtk::Button,
    refused: gtk::Label,
    delete: gtk::Button,
}

/// What the dialog lends: filling a section's boxes from an entry, the section's values as the boxes stand, and
/// one line to its log.
pub(crate) struct Lent {
    pub fill: Rc<dyn Fn(Section, &Entry)>,
    pub values_now: Rc<dyn Fn(Section) -> Entry>,
    pub note: Rc<dyn Fn(&str)>,
}

/// The open dialog's entries: the file's contents, whether they changed since the last write, and the controls.
struct State {
    paths: Option<crate::settings::Paths>,
    servers: Servers,
    dirty: bool,
    controls: Vec<(Section, Controls)>,
    /// Set while a pick fills the boxes, so their change signals do not count as typing into the entry.
    filling: bool,
    /// Set while the dropdowns are being repainted, so `set_selected` is not taken for a pick.
    painting: bool,
    /// The "a run is open" sentence, said once per dialog.
    said_run_open: bool,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
    /// The newest 🗑 question, so a test answers the very dialog a person would.
    static LAST_CONFIRM: RefCell<Option<adw::MessageDialog>> = const { RefCell::new(None) };
    /// The controls built for the dialog being built, taken by [`wire`].
    static BUILT: RefCell<Vec<(Section, Controls)>> = const { RefCell::new(Vec::new()) };
}

/// The section a heading belongs to, if it has entries.
pub(crate) fn section_of(heading_key: &str) -> Option<Section> {
    Section::ALL
        .into_iter()
        .find(|section| section.key() == heading_key)
}

/// The row under a section's heading: the entries dropdown, ＋ New (a popover asking for the name) and 🗑.
pub(crate) fn row(section: Section) -> gtk::Widget {
    let key = section.key();
    let line = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    line.set_widget_name(&format!("servers-{key}-row"));
    let dropdown = gtk::DropDown::from_strings(&[]);
    dropdown.set_widget_name(&format!("servers-{key}"));
    dropdown.set_tooltip_text(Some(
        "The server entry in use for this section: picking one fills the boxes and makes them the settings",
    ));
    line.append(&dropdown);

    let new = gtk::MenuButton::new();
    new.set_icon_name("list-add-symbolic");
    new.set_widget_name(&format!("servers-{key}-new"));
    new.set_tooltip_text(Some("New entry from what the boxes say now"));
    let column = gtk::Box::new(gtk::Orientation::Vertical, 6);
    column.set_margin_top(6);
    column.set_margin_bottom(6);
    column.set_margin_start(6);
    column.set_margin_end(6);
    let name = gtk::Entry::new();
    name.set_widget_name(&format!("servers-{key}-name"));
    name.set_placeholder_text(Some("a name, e.g. the second machine"));
    let create = gtk::Button::with_label("Create");
    create.set_widget_name(&format!("servers-{key}-create"));
    let refused = gtk::Label::new(None);
    refused.set_widget_name(&format!("servers-{key}-refused"));
    refused.add_css_class("error");
    refused.set_wrap(true);
    column.append(&name);
    column.append(&create);
    column.append(&refused);
    let popover = gtk::Popover::new();
    popover.set_child(Some(&column));
    new.set_popover(Some(&popover));
    line.append(&new);

    let delete = gtk::Button::from_icon_name("user-trash-symbolic");
    delete.set_widget_name(&format!("servers-{key}-delete"));
    delete.set_tooltip_text(Some("Delete this entry (asks first; the last one stays)"));
    line.append(&delete);

    BUILT.with(|built| {
        built.borrow_mut().push((
            section,
            Controls {
                dropdown,
                new,
                name,
                create,
                refused,
                delete,
            },
        ))
    });
    line.upcast()
}

/// Read the entries (migrating from `llm.conf` the first time) and wire the controls built for this dialog.
pub(crate) fn wire(lent: Lent, conf: &crate::settings::Conf) {
    let paths = crate::settings::from_environment();
    let controls: Vec<(Section, Controls)> =
        BUILT.with(|built| std::mem::take(&mut *built.borrow_mut()));
    let (mut servers, said) = match &paths {
        Some(paths) => {
            let loaded = server_entries::load(paths, conf);
            (loaded.servers, loaded.said)
        }
        None => (server_entries::migrate(conf), None),
    };
    if let Some(line) = said {
        (lent.note)(&line);
    }
    let mut owed = paths
        .as_ref()
        .is_some_and(|paths| !server_entries::path(paths).exists());
    // `llm.conf` is what every flow reads, so the entry in use says what it says: a hand edit to the file since
    // the last open lands in the entry instead of being undone by the next pick.
    for section in Section::ALL {
        let now = server_entries::values_from(section, conf);
        let group = servers.group_mut(section);
        let active = group.active.clone();
        if let Some(entry) = group.entries.get_mut(&active) {
            if *entry != now {
                *entry = now;
                owed = true;
            }
        }
    }
    STATE.with(|state| {
        *state.borrow_mut() = Some(State {
            paths,
            servers,
            dirty: owed,
            controls: controls.clone(),
            filling: false,
            painting: false,
            said_run_open: false,
        })
    });
    // The first open writes the migrated entries, so the file says what the dialog shows.
    persist(&*lent.note);
    paint();
    let lent = Rc::new(lent);
    for (section, controls) in controls {
        {
            let lent = Rc::clone(&lent);
            controls.dropdown.connect_selected_notify(move |dropdown| {
                if STATE.with(|state| state.borrow().as_ref().is_some_and(|s| s.painting)) {
                    return;
                }
                let Some(name) = selected_name(dropdown) else {
                    return;
                };
                pick(section, &name, &lent);
            });
        }
        {
            let lent = Rc::clone(&lent);
            let held = controls.clone();
            let press = move || {
                let values = (lent.values_now)(section);
                let typed = held.name.text().to_string();
                let made = with_state(|state| {
                    server_entries::create(state.servers.group_mut(section), &typed, values)
                        .map(|()| state.dirty = true)
                })
                .unwrap_or_else(|| Err("the dialog is closed".to_string()));
                match made {
                    Ok(()) => {
                        held.refused.set_text("");
                        held.name.set_text("");
                        held.new.popdown();
                        (lent.note)(&format!(
                            "{} entry {} made from the boxes",
                            section.key(),
                            typed.trim()
                        ));
                        persist(&*lent.note);
                        paint();
                    }
                    Err(why) => held.refused.set_text(&why),
                }
            };
            let press = Rc::new(press);
            let on_click = Rc::clone(&press);
            controls.create.connect_clicked(move |_| on_click());
            controls.name.connect_activate(move |_| press());
        }
        {
            let lent = Rc::clone(&lent);
            let held = controls.clone();
            controls.delete.connect_clicked(move |button| {
                let Some(name) = selected_name(&held.dropdown) else {
                    return;
                };
                let parent = button.root().and_then(|root| root.downcast::<gtk::Window>().ok());
                let question = adw::MessageDialog::new(
                    parent.as_ref(),
                    Some(&format!("Delete the {} entry {name}?", section.key())),
                    Some("Its server, key and model ids are forgotten; llm.conf keeps what is in use."),
                );
                question.add_responses(&[("cancel", "Cancel"), ("delete", "Delete")]);
                question.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
                question.set_default_response(Some("cancel"));
                question.set_close_response("cancel");
                let lent = Rc::clone(&lent);
                question.connect_response(None, move |question, response| {
                    question.close();
                    if response != "delete" {
                        return;
                    }
                    let gone = with_state(|state| {
                        server_entries::delete(state.servers.group_mut(section), &name).map(|next| {
                            state.dirty = true;
                            next
                        })
                    })
                    .unwrap_or_else(|| Err("the dialog is closed".to_string()));
                    match gone {
                        Ok(next) => {
                            (lent.note)(&format!("{} entry {name} deleted", section.key()));
                            persist(&*lent.note);
                            paint();
                            pick(section, &next, &lent);
                        }
                        Err(why) => (lent.note)(&format!("!!! {why}")),
                    }
                });
                LAST_CONFIRM.with(|last| *last.borrow_mut() = Some(question.clone()));
                question.present();
            });
        }
    }
}

/// Make `name` the section's entry: its values into the boxes (and so into `llm.conf` on the dialog's beat).
fn pick(section: Section, name: &str, lent: &Lent) {
    let entry = with_state(|state| {
        let group = state.servers.group_mut(section);
        let entry = group.entries.get(name).cloned()?;
        if group.active != name {
            group.active = name.to_string();
            state.dirty = true;
        }
        state.filling = true;
        Some(entry)
    })
    .flatten();
    let Some(entry) = entry else {
        return;
    };
    (lent.fill)(section, &entry);
    with_state(|state| state.filling = false);
    persist(&*lent.note);
    // No repaint here: a pick comes from the dropdown's own selection signal, and swapping its model inside
    // that signal brings GTK down. The names did not change; the dropdown already shows the pick.
    // A run already waiting on the old server keeps waiting on it: flows read the settings per request.
    let open = crate::ui::run_close::run_open();
    let say = with_state(|state| {
        let first = open && !state.said_run_open;
        if first {
            state.said_run_open = true;
        }
        first
    })
    .unwrap_or(false);
    if say {
        (lent.note)(RUN_OPEN_LINE);
    }
}

/// What a pick says once while a run is open.
pub const RUN_OPEN_LINE: &str =
    "the new server is used from the next request; \u{23f9} and \u{25b6} to restart the run now";

/// A box was typed into: the entry in use takes the new value too (not while a pick is filling the boxes).
pub(crate) fn edited(box_key: &str, value: &str) {
    let (Some(section), Some(conf_key)) = (
        Section::of_box(box_key),
        server_entries::conf_key_of_box(box_key),
    ) else {
        return;
    };
    with_state(|state| {
        if state.filling {
            return;
        }
        let group = state.servers.group_mut(section);
        let active = group.active.clone();
        if let Some(entry) = group.entries.get_mut(&active) {
            if entry.get(conf_key).map(String::as_str) != Some(value) {
                entry.insert(conf_key.to_string(), value.to_string());
                state.dirty = true;
            }
        }
    });
}

/// Write `servers.json` if anything changed; a failed write is said, never dropped.
pub(crate) fn persist(note: &dyn Fn(&str)) {
    let owed = with_state(|state| {
        if !state.dirty {
            return None;
        }
        state.dirty = false;
        state
            .paths
            .clone()
            .map(|paths| (paths, state.servers.clone()))
    })
    .flatten();
    if let Some((paths, servers)) = owed {
        if let Err(why) = server_entries::save(&paths, &servers) {
            note(&format!("!!! could not save the server entries -- {why}"));
            with_state(|state| state.dirty = true);
        }
    }
}

/// The dialog closed: land what is owed and forget the controls.
pub(crate) fn close(note: &dyn Fn(&str)) {
    persist(note);
    STATE.with(|state| *state.borrow_mut() = None);
}

/// Repaint every dropdown from the entries: names in order, the active one chosen, 🗑 greyed at one entry.
fn paint() {
    let rows: Vec<(Controls, Vec<String>, String)> = with_state(|state| {
        state.painting = true;
        state
            .controls
            .iter()
            .map(|(section, controls)| {
                let group = state.servers.group(*section);
                (
                    controls.clone(),
                    group.entries.keys().cloned().collect(),
                    group.active.clone(),
                )
            })
            .collect()
    })
    .unwrap_or_default();
    for (controls, names, active) in rows {
        let strings: Vec<&str> = names.iter().map(String::as_str).collect();
        controls
            .dropdown
            .set_model(Some(&gtk::StringList::new(&strings)));
        if let Some(at) = names.iter().position(|name| *name == active) {
            controls.dropdown.set_selected(at as u32);
        }
        controls.delete.set_sensitive(names.len() > 1);
    }
    with_state(|state| state.painting = false);
}

fn selected_name(dropdown: &gtk::DropDown) -> Option<String> {
    dropdown
        .selected_item()
        .and_then(|item| item.downcast::<gtk::StringObject>().ok())
        .map(|item| item.string().to_string())
}

fn with_state<T>(f: impl FnOnce(&mut State) -> T) -> Option<T> {
    STATE.with(|state| state.borrow_mut().as_mut().map(f))
}

/// The entries as the open dialog holds them, for a test.
pub fn servers_for_test() -> Option<Servers> {
    STATE.with(|state| state.borrow().as_ref().map(|state| state.servers.clone()))
}

/// The newest 🗑 question, for a test to answer through its own response.
pub fn confirm_for_test() -> Option<adw::MessageDialog> {
    LAST_CONFIRM.with(|last| last.borrow().clone())
}
