//! §03-shell §5b / F0.14 S1, S2, S5: the Profiles window. Edit mode (the header's Profiles button): the profile
//! dropdown (New first, always there), Load…, Save, Save as…, 🗑 and ⓘ (the template syntax), the template in a
//! monospace box, and one line saying what the template declares or its first error. View mode (Prepare's View): the User Context as sent,
//! read-only, with Edit template. The template's rules are [`crate::profile`]'s; this is only its window.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk4 as gtk;

use crate::profile;

thread_local! {
    /// The Profiles window while it is open: one at a time, and a test reaches it here.
    static OPEN: RefCell<Option<gtk::Window>> = const { RefCell::new(None) };
}

/// The window, while it is open.
pub fn open_window() -> Option<gtk::Window> {
    OPEN.with(|open| open.borrow().clone())
}

fn paths() -> Option<crate::settings::Paths> {
    crate::settings::from_environment()
}

fn named<W: IsA<gtk::Widget>>(widget: &W, name: &str) -> W {
    widget.set_widget_name(name);
    widget.clone()
}

/// A window of its own over the main one, replacing a Profiles window already open.
fn window(parent: Option<&gtk::Window>, title: &str) -> gtk::Window {
    if let Some(old) = open_window() {
        old.close();
    }
    let window = gtk::Window::builder()
        .title(title)
        .default_width(760)
        .default_height(760)
        .build();
    window.set_widget_name("profiles-window");
    if let Some(parent) = parent {
        window.set_transient_for(Some(parent));
    }
    window.connect_close_request(|_| {
        OPEN.with(|open| open.borrow_mut().take());
        glib::Propagation::Proceed
    });
    let esc = gtk::EventControllerKey::new();
    let closing = window.clone();
    esc.connect_key_pressed(move |_, key, _, _| {
        if key == gtk::gdk::Key::Escape {
            closing.close();
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    });
    window.add_controller(esc);
    OPEN.with(|open| *open.borrow_mut() = Some(window.clone()));
    window
}

/// The dropdown's first row: a starter template, always there, never saved over or deleted, only kept with
/// Save as….
pub const NEW: &str = "New";

/// The template box's floor, in px: on a small window the Produce form below it took the whole height and the
/// template (its fields, User Context and `## step` sections) was squeezed out of sight.
pub const TEMPLATE_MIN_PX: i32 = 240;

/// The edit window's state: which profile the box holds (`None` = New) and the widgets that follow it.
struct Editor {
    current: RefCell<Option<String>>,
    picker: gtk::DropDown,
    editor: gtk::TextView,
    summary: gtk::Label,
    save: gtk::Button,
    trash: gtk::Button,
    /// The Produce form: the Produce page's own rows, holding the profile's `## produce` section.
    produce: gtk::Box,
    /// The Narrate form: the Narrate page's game audio level, kept in the same section.
    narrate: gtk::Box,
    /// Up while the window sets the dropdown itself.
    painting: std::cell::Cell<bool>,
}

thread_local! {
    static EDITOR: RefCell<Option<Rc<Editor>>> = const { RefCell::new(None) };
}

/// The whole template as it is saved: the box's text, and the Produce form as its `## produce` section.
fn composed(state: &Editor) -> String {
    let mut rows = crate::ui::produce_page_layout::rows_from(state.narrate.upcast_ref());
    rows.extend(crate::ui::produce_page_layout::rows_from(
        state.produce.upcast_ref(),
    ));
    profile::join_produce(&text_of(&state.editor), &rows)
}

/// Put a template in the window: its text in the box, its `## produce` rows in the form (a row it leaves out
/// shows the default).
fn show_template(state: &Editor, source: &str) {
    let (text, rows) = profile::split_produce(source);
    state.editor.buffer().set_text(&text);
    for form in [&state.produce, &state.narrate] {
        while let Some(child) = form.first_child() {
            form.remove(&child);
        }
    }
    let produce = crate::ui::produce_page::apply_rows(&crate::project::Produce::default(), &rows);
    let seeded = crate::ui::produce_page::encoder_state(&produce);
    // The Produce page's rows, and the Narrate page's game audio: each where its page has it.
    crate::ui::produce_page_layout::build_settings_flow(
        &state.produce,
        &seeded,
        &crate::produce_screen::settings_rows_shown(),
    );
    crate::ui::produce_page_layout::build_settings_flow(&state.narrate, &seeded, &["Game audio"]);
}

fn text_of(editor: &gtk::TextView) -> String {
    let buffer = editor.buffer();
    let (start, end) = buffer.bounds();
    buffer.text(&start, &end, false).to_string()
}

/// The summary line: what the template declares, or its first error in red.
fn say_summary(summary: &gtk::Label, text: &str) {
    match profile::parse(text) {
        Ok(template) => {
            summary.remove_css_class("error");
            summary.set_text(&profile::summary(&template));
        }
        Err(errors) => {
            summary.add_css_class("error");
            summary.set_text(&errors[0]);
        }
    }
}

/// Put `name` (or New) in the box: the dropdown, the text, and Save and 🗑 live only on a saved profile.
fn select(state: &Editor, name: Option<&str>) {
    *state.current.borrow_mut() = name.map(str::to_string);
    let names = paths().as_ref().map(profile::list).unwrap_or_default();
    let mut rows = vec![NEW.to_string()];
    rows.extend(names.iter().cloned());
    state.painting.set(true);
    let strings = gtk::StringList::new(&rows.iter().map(String::as_str).collect::<Vec<_>>());
    state.picker.set_model(Some(&strings));
    let at = name
        .and_then(|name| names.iter().position(|n| n == name))
        .map_or(0, |at| at + 1);
    state.picker.set_selected(at as u32);
    state.painting.set(false);
    let text = match name {
        Some(name) => paths()
            .and_then(|paths| profile::load(&paths, name).ok())
            .unwrap_or_default(),
        None => profile::starter(),
    };
    show_template(state, &text);
    paint_buttons(state);
}

fn paint_buttons(state: &Editor) {
    let saved = state.current.borrow().is_some();
    state.save.set_sensitive(saved);
    state.trash.set_sensitive(saved);
}

/// S2's Save: the sentence the window shows, and every Prepare form re-read on success.
fn save_named(name: &str, text: &str) -> String {
    let Some(paths) = paths() else {
        return "nowhere to keep profiles on this machine".to_string();
    };
    match profile::save(&paths, name, text) {
        Ok(file) => {
            crate::ui::profile_form::refresh_all();
            format!("profile {name} saved to {}", file.display())
        }
        Err(why) => why,
    }
}

/// Is `path` a profile file, in this machine's profiles folder? Its name, when it is.
fn profile_named(path: &std::path::Path) -> Option<String> {
    let folder = profile::dir(&paths()?);
    let in_folder = path.parent().is_some_and(|parent| {
        parent == folder
            || parent.canonicalize().ok() == folder.canonicalize().ok() && folder.exists()
    });
    let stem = path.file_stem()?.to_string_lossy().into_owned();
    (in_folder && path.extension().is_some_and(|ext| ext == "txt")).then_some(stem)
}

/// S2's Save as…, for the path the chooser answered: in the profiles folder it is a profile, named by the file
/// (refused with the template's error); anywhere else the box is written there as it stands, to share it.
pub fn save_as_to(path: &std::path::Path) -> String {
    let Some(state) = EDITOR.with(|editor| editor.borrow().clone()) else {
        return String::new();
    };
    let text = composed(&state);
    let said = match profile_named(path) {
        Some(name) => {
            let said = save_named(&name, &text);
            if said.starts_with("profile ") {
                select(&state, Some(&name));
            }
            said
        }
        None => match std::fs::write(path, &text) {
            Ok(()) => format!("written to {}", path.display()),
            Err(why) => format!("{}: {why}", path.display()),
        },
    };
    state.summary.set_text(&said);
    said
}

/// S2's Load…, for the path the chooser answered: a profile in the profiles folder is selected; any other file
/// is read into the box as New, kept only once Save as… names it.
pub fn load_from(path: &std::path::Path) -> String {
    let Some(state) = EDITOR.with(|editor| editor.borrow().clone()) else {
        return String::new();
    };
    if let Some(name) = profile_named(path) {
        select(&state, Some(&name));
        return format!("profile {name}");
    }
    let said = match std::fs::read_to_string(path) {
        Ok(text) => {
            select(&state, None);
            show_template(&state, &text);
            format!(
                "loaded from {} \u{2014} Save as\u{2026} keeps it as a profile",
                path.display()
            )
        }
        Err(why) => format!("{}: {why}", path.display()),
    };
    state.summary.set_text(&said);
    said
}

/// A file chooser over the profiles folder, answering `then` with the path picked.
#[allow(deprecated)]
fn choose_file(
    parent: &gtk::Window,
    title: &str,
    saving: Option<&str>,
    then: impl Fn(&std::path::Path) + 'static,
) {
    let chooser = gtk::FileChooserNative::builder()
        .title(title)
        .action(if saving.is_some() {
            gtk::FileChooserAction::Save
        } else {
            gtk::FileChooserAction::Open
        })
        .transient_for(parent)
        .modal(true)
        .build();
    if let Some(folder) = paths().map(|paths| profile::dir(&paths)) {
        let _ = std::fs::create_dir_all(&folder);
        let _ = chooser.set_current_folder(Some(&gtk::gio::File::for_path(&folder)));
    }
    if let Some(name) = saving {
        chooser.set_current_name(name);
    }
    // The answer is one door, shared by the chooser's response and a test answering it (a desktop chooser
    // cannot be clicked in a headless run): what a test drives is what a person's pick runs.
    let then: Rc<dyn Fn(&std::path::Path)> = Rc::new(then);
    LAST_CHOOSER.with(|last| *last.borrow_mut() = Some((title.to_string(), Rc::clone(&then))));
    CHOOSERS_RAISED.with(|raised| raised.set(raised.get() + 1));
    chooser.connect_response(move |chooser, response| {
        let picked = (response == gtk::ResponseType::Accept)
            .then(|| chooser.file().and_then(|file| file.path()))
            .flatten();
        answer(&then, picked.as_deref());
        chooser.destroy();
    });
    chooser.show();
}

/// A chooser's answer: a picked path goes to what the press asked for; no pick (Cancel, ×) changes nothing.
fn answer(then: &Rc<dyn Fn(&std::path::Path)>, picked: Option<&std::path::Path>) {
    if let Some(path) = picked {
        then(path);
    }
}

thread_local! {
    /// The last chooser this window raised, its title and its answer, for [`answer_chooser_for_test`].
    static LAST_CHOOSER: RefCell<Option<(String, Rc<dyn Fn(&std::path::Path)>)>> = const { RefCell::new(None) };
    /// How many choosers a press raised.
    static CHOOSERS_RAISED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Answer the last chooser raised as a person would (a pick, or `None` for Cancel), through the same door its
/// response runs; returns the chooser's title. How many were raised, see [`choosers_raised_for_test`].
pub fn answer_chooser_for_test(picked: Option<&std::path::Path>) -> Option<String> {
    let (title, then) = LAST_CHOOSER.with(|last| last.borrow().clone())?;
    answer(&then, picked);
    Some(title)
}

/// How many choosers the Profiles window has raised.
pub fn choosers_raised_for_test() -> usize {
    CHOOSERS_RAISED.with(std::cell::Cell::get)
}

/// S1: edit mode, on `name` when given, else on the open project's profile, else New.
pub fn open_edit(parent: Option<&gtk::Window>, name: Option<&str>) -> gtk::Window {
    let window = window(parent, "Profiles");
    let column = gtk::Box::new(gtk::Orientation::Vertical, 6);
    column.set_margin_top(12);
    column.set_margin_bottom(12);
    column.set_margin_start(12);
    column.set_margin_end(12);
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let picker = named(&gtk::DropDown::from_strings(&[NEW]), "profiles-list");
    picker.set_hexpand(true);
    let load = named(&gtk::Button::with_label("Load\u{2026}"), "profile-load");
    load.set_tooltip_text(Some(
        "Open a profile, or any text file (it comes in as New until Save as\u{2026} names it)",
    ));
    let save = named(&gtk::Button::with_label("Save"), "profile-save");
    save.set_tooltip_text(Some(
        "Save this profile (New is kept only with Save as\u{2026})",
    ));
    let save_as = named(
        &gtk::Button::with_label("Save as\u{2026}"),
        "profile-save-as",
    );
    save_as.set_tooltip_text(Some(
        "Save under a name in the profiles folder, or anywhere else to share the template",
    ));
    let trash = named(
        &gtk::Button::from_icon_name("user-trash-symbolic"),
        "profile-delete",
    );
    trash.set_tooltip_text(Some("Delete this profile"));
    let help = gtk::MenuButton::new();
    help.set_widget_name("profile-syntax");
    help.set_icon_name("help-about-symbolic");
    help.set_tooltip_text(Some("How a template is written"));
    let syntax = gtk::Label::new(Some(profile::SYNTAX));
    syntax.set_widget_name("profile-syntax-text");
    syntax.set_xalign(0.0);
    syntax.set_selectable(true);
    syntax.add_css_class("monospace");
    syntax.set_margin_top(8);
    syntax.set_margin_bottom(8);
    syntax.set_margin_start(8);
    syntax.set_margin_end(8);
    let popover = gtk::Popover::new();
    popover.set_child(Some(&syntax));
    help.set_popover(Some(&popover));
    for widget in [
        picker.upcast_ref::<gtk::Widget>(),
        load.upcast_ref(),
        save.upcast_ref(),
        save_as.upcast_ref(),
        trash.upcast_ref(),
        help.upcast_ref(),
    ] {
        row.append(widget);
    }
    let editor = named(&gtk::TextView::new(), "profile-template");
    editor.set_monospace(true);
    editor.set_wrap_mode(gtk::WrapMode::WordChar);
    let scroller = gtk::ScrolledWindow::new();
    scroller.set_child(Some(&editor));
    scroller.set_vexpand(true);
    // The template keeps room to be read however small the window: below this the window's content scrolls.
    scroller.set_min_content_height(TEMPLATE_MIN_PX);
    let summary = named(&gtk::Label::new(None), "profile-summary");
    summary.set_xalign(0.0);
    summary.set_wrap(true);
    // The Produce form: the same controls as the Produce page (F0.14 S9), not the section as text.
    let produce_heading = gtk::Label::new(Some("Produce settings for projects using this profile"));
    produce_heading.add_css_class("heading");
    produce_heading.set_xalign(0.0);
    let produce = gtk::Box::new(gtk::Orientation::Vertical, 6);
    produce.set_widget_name("profile-produce");
    column.append(&row);
    column.append(&scroller);
    column.append(&summary);
    let narrate_heading = gtk::Label::new(Some("Narrate settings for projects using this profile"));
    narrate_heading.add_css_class("heading");
    narrate_heading.set_xalign(0.0);
    let narrate = gtk::Box::new(gtk::Orientation::Vertical, 6);
    narrate.set_widget_name("profile-narrate");
    column.append(&narrate_heading);
    column.append(&narrate);
    column.append(&produce_heading);
    column.append(&produce);
    // Shorter than the template's floor plus the form, the window scrolls rather than squeezing the template.
    let page = gtk::ScrolledWindow::new();
    page.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    page.set_child(Some(&column));
    window.set_child(Some(&page));

    let state = Rc::new(Editor {
        current: RefCell::new(None),
        picker: picker.clone(),
        editor: editor.clone(),
        summary: summary.clone(),
        save: save.clone(),
        trash: trash.clone(),
        produce: produce.clone(),
        narrate: narrate.clone(),
        painting: std::cell::Cell::new(false),
    });
    EDITOR.with(|slot| *slot.borrow_mut() = Some(state.clone()));
    {
        let summary = summary.clone();
        editor.buffer().connect_changed(move |buffer| {
            let (start, end) = buffer.bounds();
            say_summary(&summary, &buffer.text(&start, &end, false));
        });
    }
    // Asked for no profile (the header's button), the window opens on the one the open project uses: that is the
    // profile a person most likely came to change. New only when the project uses none this machine has.
    let used = crate::ui::window::SESSION
        .with(|slots| slots.borrow().last().cloned())
        .and_then(|session| {
            session
                .borrow()
                .profile
                .as_ref()
                .map(|used| used.name.clone())
        });
    let wanted = name.map(str::to_string).or(used);
    let known = wanted.filter(|name| {
        paths()
            .as_ref()
            .map(profile::list)
            .unwrap_or_default()
            .iter()
            .any(|n| n == name)
    });
    select(&state, known.as_deref());
    say_summary(&summary, &text_of(&editor));

    {
        let state = Rc::downgrade(&state);
        picker.connect_selected_notify(move |picker| {
            let Some(state) = state.upgrade() else {
                return;
            };
            if state.painting.get() {
                return;
            }
            let name = (picker.selected() > 0)
                .then(|| picker.selected_item())
                .flatten()
                .and_then(|item| item.downcast::<gtk::StringObject>().ok())
                .map(|item| item.string().to_string());
            // After the signal: `select` replaces this dropdown's model (see `profile_form`).
            glib::idle_add_local_once(move || select(&state, name.as_deref()));
        });
    }
    {
        let state = Rc::downgrade(&state);
        save.connect_clicked(move |_| {
            let Some(state) = state.upgrade() else {
                return;
            };
            let Some(name) = state.current.borrow().clone() else {
                return;
            };
            let said = save_named(&name, &composed(&state));
            state.summary.set_text(&said);
        });
    }
    {
        let (window_, current) = (window.clone(), Rc::downgrade(&state));
        save_as.connect_clicked(move |_| {
            let suggested = current
                .upgrade()
                .and_then(|state| state.current.borrow().clone())
                .unwrap_or_else(|| "untitled".to_string());
            choose_file(
                &window_,
                "Save the profile as",
                Some(&format!("{suggested}.txt")),
                |path| {
                    save_as_to(path);
                },
            );
        });
    }
    {
        let window_ = window.clone();
        load.connect_clicked(move |_| {
            choose_file(&window_, "Load a profile", None, |path| {
                load_from(path);
            });
        });
    }
    {
        let (window_, state) = (window.clone(), Rc::downgrade(&state));
        trash.connect_clicked(move |_| {
            let Some(state) = state.upgrade() else {
                return;
            };
            let Some(name) = state.current.borrow().clone() else {
                return;
            };
            let dialog = adw::MessageDialog::new(
                Some(&window_),
                Some(&format!("Delete the profile {name}?")),
                Some("Projects using it keep their User Context as it is."),
            );
            dialog.add_responses(&[("cancel", "Cancel"), ("delete", "Delete")]);
            dialog.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
            dialog.connect_response(None, move |dialog, response| {
                dialog.close();
                if response == "delete" {
                    delete_current(&state);
                }
            });
            dialog.present();
        });
    }
    window.present();
    window
}

/// 🗑 answered Delete: the file goes, the box goes back to New, and Prepare's forms re-read the list.
fn delete_current(state: &Editor) {
    let Some(name) = state.current.borrow().clone() else {
        return;
    };
    let said = match paths().map(|paths| profile::delete(&paths, &name)) {
        Some(Ok(())) => format!("profile {name} deleted"),
        Some(Err(why)) => why,
        None => "nowhere to keep profiles on this machine".to_string(),
    };
    select(state, None);
    state.summary.set_text(&said);
    crate::ui::profile_form::refresh_all();
}

/// The open edit window's profile (`None` = New) and whether Save and 🗑 are live, for a test.
pub fn edit_state_for_test() -> Option<(Option<String>, bool, bool)> {
    EDITOR.with(|editor| {
        editor.borrow().as_ref().map(|state| {
            (
                state.current.borrow().clone(),
                state.save.is_sensitive(),
                state.trash.is_sensitive(),
            )
        })
    })
}

/// 🗑's confirmed answer, for a test (the confirmation dialog is the hand's).
pub fn delete_for_test() {
    if let Some(state) = EDITOR.with(|editor| editor.borrow().clone()) {
        delete_current(&state);
    }
}

/// S5: view mode, the User Context as sent for `name`, with Edit template.
pub fn open_view(parent: Option<&gtk::Window>, name: &str, context: &str) -> gtk::Window {
    let window = window(parent, &format!("{name} \u{2014} the User Context as sent"));
    let column = gtk::Box::new(gtk::Orientation::Vertical, 6);
    column.set_margin_top(12);
    column.set_margin_bottom(12);
    column.set_margin_start(12);
    column.set_margin_end(12);
    let view = named(&gtk::TextView::new(), "profile-view-text");
    view.set_editable(false);
    view.set_cursor_visible(false);
    view.set_wrap_mode(gtk::WrapMode::WordChar);
    view.buffer().set_text(context);
    let scroller = gtk::ScrolledWindow::new();
    scroller.set_child(Some(&view));
    scroller.set_vexpand(true);
    let edit = named(
        &gtk::Button::with_label("Edit template"),
        "profile-edit-template",
    );
    edit.set_halign(gtk::Align::End);
    column.append(&scroller);
    column.append(&edit);
    window.set_child(Some(&column));
    let (name, parent) = (name.to_string(), parent.cloned());
    edit.connect_clicked(move |_| {
        open_edit(parent.as_ref(), Some(&name));
    });
    window.present();
    window
}
