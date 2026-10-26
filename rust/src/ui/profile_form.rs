//! §03-shell F0.14 S3, S4, S7: the profile on Prepare's bench. The User Context row's heading carries "Profile",
//! its dropdown ("(none)" and every profile on this machine) and View; a profile replaces the bench's text box
//! with its form, one row per field in the template's order, each with the control its kind names. Every change
//! stores the value in the project's `profile.values` and rebuilds `context` from the template, so everything
//! that reads the User Context reads the filled-in text. The template's rules are [`crate::profile`]'s.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

use adw::prelude::*;
use gtk4 as gtk;

use crate::profile::{self, Field, Kind, Template};
use crate::project::{ProfileUse, Project};

/// A text area is this share of the window's height: a third.
pub const TEXTAREA_SHARE: i32 = 3;
/// Its floor, for a window that has not been laid out yet.
const TEXTAREA_MIN_PX: i32 = 80;

/// The dropdown's first row: no profile, the free-text User Context.
pub const NONE: &str = "(none)";

/// One bench's profile controls.
struct Form {
    session: Rc<RefCell<Project>>,
    row: gtk::Box,
    picker: gtk::DropDown,
    scroller: gtk::ScrolledWindow,
    editor: gtk::TextView,
    form: gtk::Box,
    /// The form's own viewport: what the scroller holds while a profile is chosen. Made here and kept, because a
    /// `ScrolledWindow` handed a plain box wraps it in a viewport of ITS own, so `child()` never answers the box
    /// and a swap back put the box into a second parent while the first still held it (GTK crashed).
    form_view: gtk::Viewport,
    /// The names behind the dropdown's rows after "(none)".
    names: RefCell<Vec<String>>,
    /// Up while the page sets the dropdown itself, so that is not read as a choice.
    painting: Cell<bool>,
    /// The bench shows the User Context row.
    on_context: Cell<bool>,
}

thread_local! {
    static FORMS: RefCell<Vec<Rc<Form>>> = const { RefCell::new(Vec::new()) };
}

fn paths() -> Option<crate::settings::Paths> {
    crate::settings::from_environment()
}

/// The template a profile name stands for on this machine, parsed. `Err` says why there is none.
fn template_of(name: &str) -> Result<Template, String> {
    let paths = paths().ok_or("nowhere to keep profiles on this machine")?;
    let source = profile::load(&paths, name)
        .map_err(|_| format!("the profile {name} is not on this machine"))?;
    profile::parse(&source).map_err(|errors| {
        format!(
            "the profile {name} has an error: {} \u{2014} fix it in Profiles",
            errors[0]
        )
    })
}

/// Hang the profile controls on one bench: the row goes into `heading` after `after`, the form takes the
/// scroller's place of `editor` while a profile is chosen. Called once per bench by `window_bench::bench_box`.
pub fn attach(
    heading: &gtk::Box,
    after: &impl IsA<gtk::Widget>,
    scroller: &gtk::ScrolledWindow,
    editor: &gtk::TextView,
    session: &Rc<RefCell<Project>>,
) {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    row.set_widget_name("profile-row");
    let label = gtk::Label::new(Some("Profile"));
    label.add_css_class("dim-label");
    let picker = gtk::DropDown::from_strings(&[NONE]);
    picker.set_widget_name("profile-picker");
    picker.set_tooltip_text(Some(
        "A User Context template from Profiles: its fields are filled in here and the context is the template \
filled in. (none) is the free-text context.",
    ));
    let view = gtk::Button::with_label("View");
    view.set_widget_name("profile-view");
    view.set_tooltip_text(Some(
        "The User Context as it is sent, with the fields filled in",
    ));
    row.append(&label);
    row.append(&picker);
    row.append(&view);
    heading.insert_child_after(&row, Some(after));
    let form_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
    form_box.set_widget_name("profile-form");
    form_box.set_margin_top(crate::ui::margins::TEXT_INSET_PX);
    form_box.set_margin_start(crate::ui::margins::TEXT_INSET_PX);
    form_box.set_margin_end(crate::ui::margins::TEXT_INSET_PX);

    let form_view = gtk::Viewport::new(None::<&gtk::Adjustment>, None::<&gtk::Adjustment>);
    form_view.set_child(Some(&form_box));
    let form = Rc::new(Form {
        session: session.clone(),
        row,
        picker: picker.clone(),
        scroller: scroller.clone(),
        editor: editor.clone(),
        form: form_box,
        form_view,
        names: RefCell::new(Vec::new()),
        painting: Cell::new(false),
        on_context: Cell::new(true),
    });
    FORMS.with(|forms| forms.borrow_mut().push(form.clone()));
    paint(&form);

    let chosen = Rc::downgrade(&form);
    picker.connect_selected_notify(move |picker| {
        let Some(form) = chosen.upgrade() else {
            return;
        };
        if form.painting.get() {
            return;
        }
        let index = picker.selected() as usize;
        let name = if index == 0 {
            None
        } else {
            form.names.borrow().get(index - 1).cloned()
        };
        // Compared with the project's own choice, not trusted as a hand: replacing the dropdown's model moves
        // its row by itself, and that emission lands after `paint` lowered its flag, so a repaint would read it
        // as a choice and repaint again, without end.
        let current = form
            .session
            .borrow()
            .profile
            .as_ref()
            .map(|used| used.name.clone());
        if name == current {
            return;
        }
        // After the signal, not inside it: `choose` repaints, and replacing a dropdown's model inside that
        // dropdown's own `selected` emission left GTK reading the old model when the handler returned (a crash).
        let later = Rc::downgrade(&form);
        glib::idle_add_local_once(move || {
            if let Some(form) = later.upgrade() {
                choose(&form, name.as_deref());
            }
        });
    });
    let viewed = Rc::downgrade(&form);
    view.connect_clicked(move |button| {
        let Some(form) = viewed.upgrade() else {
            return;
        };
        let held = form.session.borrow();
        let name = held
            .profile
            .as_ref()
            .map(|used| used.name.clone())
            .unwrap_or_else(|| NONE.to_string());
        let parent = button
            .root()
            .and_then(|root| root.downcast::<gtk::Window>().ok());
        crate::ui::profiles_dialog::open_view(parent.as_ref(), &name, &held.context);
    });
}

/// The bench switched rows: the profile controls belong to the User Context row only.
pub fn follow_row(editor: &gtk::TextView, on_context: bool) {
    FORMS.with(|forms| {
        for form in forms.borrow().iter().filter(|form| form.editor == *editor) {
            form.on_context.set(on_context);
            show(form);
        }
    });
}

/// Re-read every bench's profiles (a profile was saved or deleted in the Profiles window).
pub fn refresh_all() {
    let forms: Vec<Rc<Form>> = FORMS.with(|forms| forms.borrow().clone());
    for form in forms {
        paint(&form);
    }
}

/// S4: a profile chosen (or "(none)"): the project names it, keeps the values of fields both share, and its
/// context is the template filled in. "(none)" keeps the last filled-in text as the free-text context.
fn choose(form: &Rc<Form>, name: Option<&str>) {
    // Kept and shown after the project's borrow ends: the Produce page's refresh reads the project too.
    let mut to_keep: Option<(crate::project::Produce, String, Vec<(String, String)>)> = None;
    {
        let mut held = form.session.borrow_mut();
        match name {
            None => held.profile = None,
            Some(name) => {
                let kept = held
                    .profile
                    .as_ref()
                    .map(|used| used.values.clone())
                    .unwrap_or_default();
                let values = match template_of(name) {
                    Ok(template) => {
                        let values = profile::values_for(&template, &kept);
                        held.context = profile::fill(&template, &values);
                        // S9: the profile's `## produce` rows set the project's encoder settings, through the
                        // Produce page's own rules; rows it leaves out keep the project's value.
                        if !template.produce.is_empty() {
                            held.produce = crate::ui::produce_page::apply_rows(
                                &held.produce,
                                &template.produce,
                            );
                            to_keep = Some((
                                held.produce.clone(),
                                name.to_string(),
                                template.produce.clone(),
                            ));
                        }
                        values
                    }
                    Err(_) => kept,
                };
                held.profile = Some(ProfileUse {
                    name: name.to_string(),
                    values,
                });
            }
        }
    }
    if let Some((produce, name, rows)) = to_keep {
        keep_produce(&produce, &name, &rows);
    }
    derive_policy();
    paint(form);
}

/// S9: the encoder settings a profile set, kept in the project file, shown on the Produce page, and said.
fn keep_produce(produce: &crate::project::Produce, name: &str, rows: &[(String, String)]) {
    let dir = crate::startup::open_dir();
    if let Ok(mut on_disk) = crate::project::load(&dir) {
        on_disk.produce = produce.clone();
        if let Err(why) = crate::project::save(&on_disk, &dir) {
            crate::ui::window::log_line(&format!(
                "!!! the profile's encoder settings were not kept: {why}"
            ));
        }
    }
    crate::ui::produce_page::show_encoder(produce);
    let said: Vec<String> = rows
        .iter()
        .map(|(row, value)| format!("{row} {value}"))
        .collect();
    crate::ui::window::log_line(&format!(
        ">>> profile {name}: encoder settings {}",
        said.join(", ")
    ));
}

/// F0.7's debounced derivation follows a context change made here, as it follows a typed one.
fn derive_policy() {
    if let Some(window) = crate::ui::window::POLICY_WATCH_WINDOW.with(|w| w.borrow().clone()) {
        crate::ui::window::schedule_policy_derive(&window);
    }
}

/// The dropdown, the form and which of form or text box the scroller holds, from the project.
fn paint(form: &Rc<Form>) {
    let names = paths().as_ref().map(profile::list).unwrap_or_default();
    let used = form.session.borrow().profile.clone();
    let mut rows: Vec<String> = vec![NONE.to_string()];
    let mut behind = names.clone();
    // S7: a profile this machine does not have still shows, by name, so the project's choice is visible.
    if let Some(used) = used.as_ref().filter(|used| !names.contains(&used.name)) {
        behind.push(used.name.clone());
    }
    rows.extend(behind.iter().map(|name| {
        if names.contains(name) {
            name.clone()
        } else {
            format!("{name} (not on this machine)")
        }
    }));
    *form.names.borrow_mut() = behind.clone();
    form.painting.set(true);
    let strings = gtk::StringList::new(&rows.iter().map(String::as_str).collect::<Vec<_>>());
    form.picker.set_model(Some(&strings));
    let at = used
        .as_ref()
        .and_then(|used| behind.iter().position(|name| *name == used.name))
        .map_or(0, |at| at + 1);
    form.picker.set_selected(at as u32);
    form.painting.set(false);
    build_fields(form);
    show(form);
}

/// Which of the form and the text box the scroller shows, and whether the text box may be typed in.
fn show(form: &Rc<Form>) {
    form.row.set_visible(form.on_context.get());
    let used = form.session.borrow().profile.clone();
    let template = used.as_ref().map(|used| template_of(&used.name));
    let wants_form = form.on_context.get() && matches!(template, Some(Ok(_)));
    // S7: a profile that cannot be read leaves the stored context, read-only, under its name.
    let locked = form.on_context.get() && matches!(template, Some(Err(_)));
    form.editor.set_editable(!locked);
    form.editor.set_cursor_visible(!locked);
    let child = form.scroller.child();
    if wants_form {
        if child.as_ref() != Some(form.form_view.upcast_ref()) {
            form.scroller.set_child(Some(&form.form_view));
        }
    } else if child.as_ref() != Some(form.editor.upcast_ref()) {
        form.scroller.set_child(Some(&form.editor));
    }
}

/// One row per field: the label (with `*` when required) and its control, in the template's order.
fn build_fields(form: &Rc<Form>) {
    while let Some(child) = form.form.first_child() {
        form.form.remove(&child);
    }
    let Some(used) = form.session.borrow().profile.clone() else {
        return;
    };
    let template = match template_of(&used.name) {
        Ok(template) => template,
        Err(why) => {
            let said = gtk::Label::new(Some(&why));
            said.set_widget_name("profile-form-error");
            said.add_css_class("error");
            said.set_wrap(true);
            said.set_xalign(0.0);
            form.form.append(&said);
            return;
        }
    };
    let grid = gtk::Grid::new();
    grid.set_row_spacing(6);
    grid.set_column_spacing(12);
    let values = profile::values_for(&template, &used.values);
    // A one-line field has its label beside it; a text area has its label on a line of its own and the column's
    // full width under it (beside a long label it was squeezed into the right half).
    let mut row = 0;
    for field in &template.fields {
        let label = gtk::Label::new(Some(&if field.required {
            format!("{} *", field.label)
        } else {
            field.label.clone()
        }));
        label.set_valign(gtk::Align::Center);
        if field.required {
            label.set_tooltip_text(Some("Required: Prepare's \u{25b6} waits for it"));
        }
        let value = values.get(&field.name).cloned().unwrap_or_default();
        let control = control_for(form, field, &value);
        control.set_hexpand(true);
        if field.kind == Kind::TextArea {
            label.set_xalign(0.0);
            grid.attach(&label, 0, row, 2, 1);
            grid.attach(&control, 0, row + 1, 2, 1);
            row += 2;
        } else {
            label.set_xalign(1.0);
            grid.attach(&label, 0, row, 1, 1);
            grid.attach(&control, 1, row, 1, 1);
            row += 1;
        }
    }
    form.form.append(&grid);
}

/// The control a field's kind names (§5b's table), wired to store its value on every change.
fn control_for(form: &Rc<Form>, field: &Field, value: &str) -> gtk::Widget {
    let name = format!("profile-field-{}", field.name);
    let store = {
        let form = Rc::downgrade(form);
        let field = field.clone();
        move |value: String| {
            if let Some(form) = form.upgrade() {
                set_value(&form, &field, value);
            }
        }
    };
    match field.kind {
        Kind::Text | Kind::Number => {
            let entry = gtk::Entry::new();
            entry.set_widget_name(&name);
            entry.set_text(value);
            if field.kind == Kind::Number {
                entry.set_input_purpose(gtk::InputPurpose::Number);
            }
            let checked = field.clone();
            mark(&entry, profile::accepts(&checked, value));
            entry.connect_changed(move |entry| {
                let text = entry.text().to_string();
                mark(entry, profile::accepts(&checked, &text));
                store(text);
            });
            entry.upcast()
        }
        Kind::TextArea => {
            let view = gtk::TextView::new();
            view.set_widget_name(&name);
            view.set_wrap_mode(gtk::WrapMode::WordChar);
            crate::ui::margins::text_box(&view);
            view.buffer().set_text(value);
            view.buffer().connect_changed(move |buffer| {
                let (start, end) = buffer.bounds();
                store(buffer.text(&start, &end, false).to_string());
            });
            // A third of the window tall, scrolling inside itself, following the window as it is resized: a
            // pasted script grew the box to its whole length and pushed the form past the page.
            let scroller = gtk::ScrolledWindow::new();
            scroller.set_widget_name(&format!("{name}-box"));
            scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
            scroller.set_child(Some(&view));
            scroller.add_css_class("card");
            scroller.add_tick_callback(|scroller, _| {
                let page = scroller
                    .root()
                    .map_or(0, |root| root.upcast_ref::<gtk::Widget>().height());
                let wanted = (page / TEXTAREA_SHARE).max(TEXTAREA_MIN_PX);
                if scroller.min_content_height() != wanted {
                    // Max first when shrinking and min first when growing, so the pair never crosses.
                    if wanted < scroller.min_content_height() {
                        scroller.set_min_content_height(wanted);
                        scroller.set_max_content_height(wanted);
                    } else {
                        scroller.set_max_content_height(wanted);
                        scroller.set_min_content_height(wanted);
                    }
                }
                glib::ControlFlow::Continue
            });
            scroller.upcast()
        }
        Kind::Choice => {
            let mut rows: Vec<String> = Vec::new();
            if field.required {
                rows.push("\u{2014} choose \u{2014}".to_string());
            }
            rows.extend(field.values.iter().cloned());
            let offset = usize::from(field.required);
            let picker =
                gtk::DropDown::from_strings(&rows.iter().map(String::as_str).collect::<Vec<_>>());
            picker.set_widget_name(&name);
            let at = field
                .values
                .iter()
                .position(|v| v == value)
                .map_or(0, |at| at + offset);
            picker.set_selected(at as u32);
            let values = field.values.clone();
            picker.connect_selected_notify(move |picker| {
                let index = picker.selected() as usize;
                let value = index
                    .checked_sub(offset)
                    .and_then(|i| values.get(i).cloned())
                    .unwrap_or_default();
                store(value);
            });
            picker.upcast()
        }
        Kind::ChoiceCustom => {
            let line = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            line.add_css_class("linked");
            let entry = gtk::Entry::new();
            entry.set_widget_name(&name);
            entry.set_text(value);
            entry.set_hexpand(true);
            entry.connect_changed(move |entry| store(entry.text().to_string()));
            let offered = gtk::Box::new(gtk::Orientation::Vertical, 0);
            let popover = gtk::Popover::new();
            for (at, choice) in field.values.iter().enumerate() {
                let pick = gtk::Button::with_label(choice);
                // Named, so a pick can be found and pressed by name like every other control.
                pick.set_widget_name(&format!("{name}-value-{at}"));
                pick.add_css_class("flat");
                let (entry, popover, choice) = (entry.clone(), popover.clone(), choice.clone());
                pick.connect_clicked(move |_| {
                    entry.set_text(&choice);
                    popover.popdown();
                });
                offered.append(&pick);
            }
            popover.set_child(Some(&offered));
            let menu = gtk::MenuButton::new();
            menu.set_widget_name(&format!("{name}-values"));
            menu.set_popover(Some(&popover));
            menu.set_tooltip_text(Some(
                "The values this profile offers; any other may be typed",
            ));
            line.append(&entry);
            line.append(&menu);
            line.upcast()
        }
    }
}

/// A value the field does not take wears the error style.
fn mark(entry: &gtk::Entry, good: bool) {
    if good {
        entry.remove_css_class("error");
    } else {
        entry.add_css_class("error");
    }
}

/// S4: one value stored, and the context rebuilt from the template.
fn set_value(form: &Rc<Form>, field: &Field, value: String) {
    let Some(name) = form
        .session
        .borrow()
        .profile
        .as_ref()
        .map(|used| used.name.clone())
    else {
        return;
    };
    let Ok(template) = template_of(&name) else {
        return;
    };
    {
        let mut held = form.session.borrow_mut();
        let Some(used) = held.profile.as_mut() else {
            return;
        };
        used.values.insert(field.name.clone(), value);
        let values = profile::values_for(&template, &used.values);
        held.context = profile::fill(&template, &values);
    }
    derive_policy();
}

/// S6: Prepare's ▶ with a required field empty refuses with the field's label. `None` with no profile, a
/// profile this machine lacks (S7 runs with the context as it is), or nothing missing.
pub fn refusal(project: &Project) -> Option<String> {
    let used = project.profile.as_ref()?;
    let template = template_of(&used.name).ok()?;
    let values: BTreeMap<String, String> = profile::values_for(&template, &used.values);
    let field = profile::first_missing(&template, &values)?;
    Some(format!(
        "fill in {} first \u{2014} the profile {} needs it",
        field.label, used.name
    ))
}

/// The profile that sets `row` of the encoder settings for the window's project (its `## produce` section names
/// the row), if any: a page greys that control and names the profile (§03-shell F0.14 S9).
pub(crate) fn sets_row(row: &str) -> Option<String> {
    let used = crate::ui::window::SESSION
        .with(|slots| slots.borrow().last().cloned())
        .and_then(|session| session.borrow().profile.clone())?;
    let template = template_of(&used.name).ok()?;
    template
        .produce
        .iter()
        .any(|(held, _)| held == row)
        .then_some(used.name)
}
