//! The window: renders a [`Project`]'s state and nothing else.
//!
//! No rules live here (spec/00-principles.md §5) — a page is a label of the
//! state it was handed, so every value on screen traces back to the model.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use adw::prelude::*;
use gtk4 as gtk;

use crate::add_sources;
use crate::bench;
use crate::layout;
use crate::new_project;
use crate::prepare;
use crate::project::Project;
use crate::rescan;
use crate::run;
use crate::save_as;
use crate::sources::{self, Control};
use crate::startup;
use crate::shell::{self, Move, Outcome, Page, Shell};
use crate::PAGES;

pub const APP_ID: &str = "ch.bocek.naivepost";

/// One page of the window: its name and the state it shows.
/// Prepare's two widgets are handed back beside the page so `build_window` can wire *that* button —
/// see [`build_window`] for why they are not found by name. On every other page both are `None`.
fn page_box(
    page: &str,
    project: &Project,
    session: &Rc<RefCell<Project>>,
    status: &gtk::Label,
) -> (gtk::Widget, Option<gtk::Button>, Option<gtk::CheckButton>) {
    let view = adw::ToolbarView::new();

    let box_ = gtk::Box::new(gtk::Orientation::Vertical, 12);
    box_.set_margin_start(24);
    box_.set_margin_end(24);
    box_.set_margin_top(24);
    box_.set_margin_bottom(24);

    let title = gtk::Label::new(Some(page));
    title.add_css_class("title-1");
    title.set_halign(gtk::Align::Start);
    box_.append(&title);

    // §1's badge **9** is the visible tab's own page, so the way in belongs to Prepare and to no
    // other page. Only its two widgets are here: the source rows with their camera and microphone
    // icons, and the Freq / Original / Language / Style controls under them, are F1.x's — this item
    // is how files get added, not what each row says about itself.
    let mut prepare: Option<(gtk::Button, gtk::CheckButton)> = None;
    if page == "Prepare" {
        let add_ = gtk::Button::with_label(ADD_LABEL);
        add_.set_widget_name("add-sources-button");
        let copy_ = gtk::CheckButton::with_label(COPY_LABEL);
        copy_.set_widget_name("copy-into-project");
        // Tick on: the default is to bring the files inside the project, so the folder stays one
        // thing to move (F0.12 S2).
        copy_.set_active(!project.reference_sources);

        let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        row.set_widget_name("add-sources-row");
        row.append(&add_);
        row.append(&copy_);
        box_.append(&row);

        // §4's list itself. The rows are drawn from the project and every control on them forwards to
        // `sources.rs` — this page holds no rule of its own (spec/00-principles.md §5), so what a test
        // can read back is which rows exist, which controls they show, and what a press put on the
        // status line.
        let list = sources_list(&session, &status);
        let frame = gtk::Frame::new(None);
        frame.set_child(Some(&list));
        box_.append(&frame);

        // §1's badge **8**: Freq and Language under the list. Both are choices from a list rather
        // than free text — an interval off the stops would be sent to the model as a claim about
        // frames that were never picked.
        box_.append(&frame_controls(&session));

        // §1's badges **2**-**5**: the prompt bench, one heading row over one text box. The rows it
        // offers and every sentence on it come from `bench`; the box only forwards what is typed.
        let (heading, editor) = bench_box(&session, &bench_paths());
        box_.append(&heading);
        box_.append(&editor);

        prepare = Some((add_, copy_));
    }

    let context = gtk::Label::new(Some(&project.context));
    context.set_wrap(true);
    context.set_xalign(0.0);
    context.add_css_class("body");
    box_.append(&context);

    view.set_content(Some(&box_));
    // The two widgets are handed back only for Prepare; every other page has none. `build_window`
    // wires that button directly rather than finding it by name, because a window whose four pages
    // each hold an "add-sources-button" makes `find_widget_by_name` return whichever one it reaches
    // first — and wiring an arbitrary page's copy leaves the visible one doing nothing.
    let (add_, copy_) = prepare.map_or((None, None), |(add_, copy_)| (Some(add_), Some(copy_)));
    (view.upcast(), add_, copy_)
}

/// §4's list of rows, one per source file, drawn from the project and wired to [`crate::sources`].
///
/// The rows are named so a test can find them (`source-row-<index>`), and each control carries its own
/// widget name — the same reason Add's button is named: so a test presses it the way a user does,
/// rather than calling the rule behind it.
fn sources_list(session: &Rc<RefCell<Project>>, status: &gtk::Label) -> gtk::Widget {
    let list = gtk::ListBox::new();
    list.set_widget_name("sources-list");
    list.set_selection_mode(gtk::SelectionMode::None);
    list.set_tooltip_text(Some(sources::LIST_TIP));
    // One hairline between rows, which is how §4's band reads; the frame around it is the page's.
    list.add_css_class("boxed-list");

    for index in 0..session.borrow().sources.len() {
        list.append(&source_row(session, status, index));
    }
    list.upcast()
}

/// §1's badge **8**: the two frame controls under the list. Freq is a dropdown of the stops
/// ([`prepare::FREQ_STOPS`]) and Language an entry with the warning as its tooltip; both write
/// through to the session, so the page holds no rule of its own.
fn frame_controls(session: &Rc<RefCell<Project>>) -> gtk::Widget {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row.set_widget_name("frame-controls");

    row.append(&label("Freq"));
    let freq = gtk::DropDown::from_strings(
        &prepare::FREQ_STOPS
            .iter()
            .map(|secs| prepare::freq_label(*secs))
            .collect::<Vec<_>>()
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
    );
    freq.set_widget_name("freq-control");
    freq.set_selected(
        prepare::FREQ_STOPS
            .iter()
            .position(|stop| *stop == session.borrow().interval)
            .map(|index| index as u32)
            .unwrap_or(gtk::INVALID_LIST_POSITION),
    );
    let held = session.clone();
    freq.connect_selected_notify(move |picker| {
        // `INVALID_LIST_POSITION` is the empty selection GTK gives before a choice exists; anything
        // off the stops is `set_freq`'s refusal to make, not this handler's decision.
        let index = picker.selected();
        if index == gtk::INVALID_LIST_POSITION {
            return;
        }
        let secs = prepare::FREQ_STOPS[usize::try_from(index).unwrap_or(0)];
        prepare::set_freq(&mut held.borrow_mut(), secs);
    });
    row.append(&freq);

    row.append(&label("Language"));
    let language = gtk::Entry::new();
    language.set_widget_name("language-entry");
    language.set_tooltip_text(Some(prepare::LANGUAGE_TIP));
    // The code as it will be used, so an empty box shows the "en" it means rather than a blank.
    language.set_text(&prepare::language_label(&session.borrow().language));
    let held = session.clone();
    language.connect_changed(move |entry| {
        held.borrow_mut().language = entry.text().to_string();
    });
    row.append(&language);

    row.upcast()
}

/// A plain label beside a control, dimmed the way a field name is.
fn label(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.add_css_class("dim-label");
    label
}

/// §1's badges **2**-**5**: the bench's heading row and its one text box. Everything the row says —
/// title, mark, Reset's liveness — is asked of [`bench`]; the widgets only forward.
fn bench_box(
    session: &Rc<RefCell<Project>>,
    paths: &Option<crate::settings::Paths>,
) -> (gtk::Box, gtk::TextView) {
    let row = Rc::new(RefCell::new(bench::Bench::new()));
    // Each handler below holds its own handle on the same answer to "where does this machine keep a
    // prompt", which is why it travels as an `Rc` rather than as a borrow of the caller's.
    let paths = Rc::new(paths.clone());

    let heading = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    heading.set_widget_name("bench-heading");

    let title = gtk::Label::new(None);
    title.set_widget_name("prepare-title");
    title.add_css_class("title-4");
    title.set_halign(gtk::Align::Start);
    heading.append(&title);

    let mark = gtk::Label::new(Some(bench::EDITED_MARK));
    mark.set_widget_name("bench-mark");
    mark.set_tooltip_text(Some(bench::EDITED_TIP));
    mark.add_css_class("dim-label");
    heading.append(&mark);

    // **3** the row picker: the thirteen rows in pipeline order, chosen from a menu. A DropDown is
    // GTK's own menu of strings, which is what §1 asks for and what carries the selected index.
    let titles: Vec<String> = bench::picker_rows()
        .iter()
        .map(|(_, title)| title.to_string())
        .collect();
    let picker = gtk::DropDown::from_strings(
        &titles
            .iter()
            .map(String::as_str)
            .collect::<Vec<&str>>(),
    );
    picker.set_widget_name("bench-picker");
    picker.set_halign(gtk::Align::Start);

    // **4** Reset, live only while this machine holds an edit.
    let reset = gtk::Button::with_label("Reset");
    reset.set_widget_name("bench-reset");
    reset.set_tooltip_text(Some("Restore the shipped wording"));
    heading.append(&picker);
    heading.append(&reset);

    // **5** the prompt or the User Context.
    let editor = gtk::TextView::new();
    editor.set_widget_name("bench-text");
    editor.set_monospace(true);
    editor.set_wrap_mode(gtk::WrapMode::WordChar);
    editor.set_vexpand(true);
    editor.set_top_margin(6);
    editor.set_bottom_margin(6);
    let scroller = gtk::ScrolledWindow::new();
    scroller.set_child(Some(&editor));
    scroller.set_vexpand(true);

    // The box's contents are set when the row changes, never from its own `changed` handler: typing
    // stores text and storing must not retype the buffer out from under the cursor.
    let guard = Rc::new(RefCell::new(false));
    paint_bench(&row, &title, &mark, &reset, &editor, &guard, session, &paths);

    let session_for_picker = session.clone();
    let paths_for_picker = paths.clone();
    let row_for_picker = row.clone();
    let guard_for_picker = guard.clone();
    let title_for_picker = title.clone();
    let mark_for_picker = mark.clone();
    let reset_for_picker = reset.clone();
    let editor_for_picker = editor.clone();
    picker.connect_selected_notify(move |picker| {
        let index = picker.selected();
        if index == gtk::INVALID_LIST_POSITION {
            return;
        }
        row_for_picker.borrow_mut().select(usize::try_from(index).unwrap_or(0));
        paint_bench(
            &row_for_picker,
            &title_for_picker,
            &mark_for_picker,
            &reset_for_picker,
            &editor_for_picker,
            &guard_for_picker,
            &session_for_picker,
            &paths_for_picker,
        );
    });

    // Reset (**4**) puts the shipped wording back and forgets this machine's file; the buffer then
    // shows what is stored, which is why it is repainted rather than trusted to the click.
    let session_for_reset = session.clone();
    let paths_for_reset = paths.clone();
    let title_for_reset = title.clone();
    let mark_for_reset = mark.clone();
    let editor_for_reset = editor.clone();
    let reset_for_reset = reset.clone();
    let row_for_reset = row.clone();
    let guard_for_reset = guard.clone();
    reset.connect_clicked(move |_| {
        let current = row_for_reset.borrow().row().clone();
        let Some(paths) = paths_for_reset.as_ref() else { return };
        // User Context has no shipped wording to restore, and `bench::reset` says so with `None`.
        if let Ok(Some(_)) = bench::reset(paths, &current) {
            paint_bench(
                &row_for_reset,
                &title_for_reset,
                &mark_for_reset,
                &reset_for_reset,
                &editor_for_reset,
                &guard_for_reset,
                &session_for_reset,
                &paths_for_reset,
            );
        }
    });

    let buffer = editor.buffer();
    let session_for_text = session.clone();
    let paths_for_text = paths.clone();
    let mark_for_text = mark;
    let reset_for_text = reset;
    let row_for_text = row;
    buffer.connect_changed(move |buffer| {
        if *guard.borrow() {
            return;
        }
        let (start, end) = (buffer.bounds().0, buffer.bounds().1);
        let text = buffer.text(&start, &end, false).to_string();
        // Every keystroke is stored (§1): there is no Save button on this bench. The session takes
        // the text whatever this machine's settings can do with it — User Context lives there, and a
        // prompt that cannot be written must still not be thrown away.
        let current = row_for_text.borrow().row().clone();
        let mut held = session_for_text.borrow_mut();
        match paths_for_text.as_ref() {
            Some(paths) => {
                let _ = bench::store(paths, &mut held, &current, &text);
            }
            // With no config folder a prompt has nowhere to go; the User Context still lands on the
            // session, which is where it belongs.
            None if bench::is_context(&current) => {
                held.context = text.clone();
            }
            None => {}
        }
        let edited = match paths_for_text.as_ref() {
            Some(paths) => bench::edited(paths, &current),
            None => false,
        };
        mark_for_text.set_visible(bench::shows_mark(&current, edited));
        reset_for_text.set_visible(bench::shows_reset(&current, edited));
        reset_for_text.set_sensitive(bench::shows_reset(&current, edited));
    });

    // The heading and the box are returned together; `page_box` stacks them in §1's order. The
    // scroller is dropped with its child still inside: GTK keeps what it was added to.
    drop(scroller);
    (heading, editor)
}

/// Where this machine keeps an edited prompt (§1's ✎). `None` — no config home at all — leaves the
/// bench showing rows with no mark rather than pretending to hold edits it cannot write.
fn bench_paths() -> Option<crate::settings::Paths> {
    crate::settings::from_environment()
}

/// Where settings are not: no folder at all, so every read finds nothing and every shipped wording
/// shows through. Used when this machine has no config home to write prompts into.
const NO_SETTINGS: crate::settings::Paths = crate::settings::Paths {
    config_dir: std::path::PathBuf::new(),
    data_dir: std::path::PathBuf::new(),
};

/// Draw one row of the bench: its title, whether this machine holds it, and its text.
fn paint_bench(
    row: &Rc<RefCell<bench::Bench>>,
    title: &gtk::Label,
    mark: &gtk::Label,
    reset: &gtk::Button,
    editor: &gtk::TextView,
    guard: &Rc<RefCell<bool>>,
    session: &Rc<RefCell<Project>>,
    paths: &Option<crate::settings::Paths>,
) {
    let held = row.borrow();
    title.set_text(&bench::title(held.row()));
    paint_bench_marks(mark, reset, paths.as_ref(), held.row());

    // The text is set here and only here, with the store handler parked: writing into a buffer
    // fires `changed`, and a `changed` that stores would repaint the buffer again.
    *guard.borrow_mut() = true;
    let buffer = editor.buffer();
    // With no config folder there is nothing this machine could hold, so the box shows the shipped
    // wording — which is what `bench::text` answers for a row nobody has touched.
    buffer.set_text(&bench::text(paths.as_ref().unwrap_or(&NO_SETTINGS), &session.borrow(), held.row()));
    *guard.borrow_mut() = false;
}

/// The mark and Reset, both live only while this machine holds the row's wording (§1).
fn paint_bench_marks(
    mark: &gtk::Label,
    reset: &gtk::Button,
    paths: Option<&crate::settings::Paths>,
    row: &bench::Row,
) {
    let edited = paths.is_some_and(|paths| bench::edited(paths, row));
    mark.set_visible(bench::shows_mark(row, edited));
    reset.set_visible(bench::shows_reset(row, edited));
    reset.set_sensitive(bench::shows_reset(row, edited));
}

/// A symbol control: an icon, flat so a row of them reads as one band rather than a row of boxes, and
/// carrying §4's key to the four symbols at the end of its tooltip.
fn icon_toggle(icon: &str, name: &str, tip: &str) -> gtk::ToggleButton {
    let button = gtk::ToggleButton::builder().icon_name(icon).build();
    button.set_widget_name(name);
    button.add_css_class("flat");
    button.set_tooltip_text(Some(&format!("{tip}{}", sources::ROW_KEY)));
    button
}

/// The same for a control that is pressed rather than ticked. `icon` empty leaves the face to be set by
/// the caller, which is how the narrator button shows the slot it holds.
fn icon_button(icon: &str, name: &str, tip: &str) -> gtk::Button {
    let button = if icon.is_empty() {
        gtk::Button::new()
    } else {
        gtk::Button::from_icon_name(icon)
    };
    button.set_widget_name(name);
    button.add_css_class("flat");
    button.set_tooltip_text(Some(&format!("{tip}{}", sources::ROW_KEY)));
    button
}

/// One row: the file's name, and the controls §4 gives it. A control that does not apply to this file
/// is absent rather than insensitive — an audio row has no footage tick to grey out.
fn source_row(session: &Rc<RefCell<Project>>, status: &gtk::Label, index: usize) -> gtk::ListBoxRow {
    let source = session.borrow().sources[index].clone();
    let name = sources::row_name(&source.path);

    let row = gtk::ListBoxRow::new();
    row.set_widget_name(&format!("source-row-{index}"));
    row.set_tooltip_text(Some(&source.path));

    let box_ = gtk::Box::new(gtk::Orientation::Horizontal, 12);

    if sources::row_controls(&source, source.tracks.len().max(1)).contains(&Control::Footage) {
        let footage = icon_toggle("camera-video-symbolic", &format!("footage-{index}"), sources::FOOTAGE_TIP);
        footage.set_active(source.footage);
        footage.set_active(source.footage);
        footage.set_tooltip_text(Some(sources::FOOTAGE_TIP));
        wire_footage(&footage, Rc::clone(session), status, index, name.clone());
        box_.append(&footage);
    }

    let narrator = icon_button("", &format!("narrator-{index}"), &sources::NARRATOR_TIP);
    // The face is the slot, which is the one thing about this control a glance has to catch; slot 1 is
    // the narration's voice, so it is drawn as suggested (§4's "slot 1 highlighted").
    narrator.set_label(&source.narrator.to_string());
    if source.narrator == 1 {
        narrator.add_css_class("suggested-action");
    }
    wire_narrator(&narrator, Rc::clone(session), status, index, name.clone());
    box_.append(&narrator);

    let label = gtk::Label::new(Some(&name));
    label.set_widget_name(&format!("source-name-{index}"));
    label.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    label.set_hexpand(true);
    label.set_xalign(0.0);
    box_.append(&label);

    let remove = icon_button("user-trash-symbolic", &format!("remove-{index}"), sources::REMOVE_TIP);
    wire_remove(&remove, Rc::clone(session), status, index);
    box_.append(&remove);

    row.set_child(Some(&box_));
    row
}

/// The 🎥 toggle. Each of the three handlers below takes the handles it needs by value, which is what
/// keeps this to one `Rc` clone per control instead of a shadowed local moved into the first closure
/// and missed by the next — the rule itself is [`sources::toggle_footage`], and the page only repeats
/// what it did.
fn wire_footage(
    footage: &gtk::ToggleButton,
    session: Rc<RefCell<Project>>,
    status: &gtk::Label,
    index: usize,
    name: String,
) {
    let status = status.clone();
    footage.connect_clicked(move |button| {
        if sources::toggle_footage(&mut session.borrow_mut(), index, button.is_active()) {
            status.set_text(&format!(
                "{name}: {}",
                if button.is_active() { "footage" } else { "not footage" }
            ));
        }
    });
}

/// The 🎤 narrator button. `face` is the button's own handle: `connect_clicked` takes the closure by
/// moving it, so the button cannot also be borrowed into it.
fn wire_narrator(
    narrator: &gtk::Button,
    session: Rc<RefCell<Project>>,
    status: &gtk::Label,
    index: usize,
    name: String,
) {
    let status = status.clone();
    let face = narrator.clone();
    narrator.connect_clicked(move |_| {
        let slot = sources::cycle_narrator(&mut session.borrow_mut(), index);
        // The button's face is the slot it just took, so the row repaints itself.
        face.set_label(&slot.to_string());
        face.set_tooltip_text(Some(&format!("{}{}", sources::slot_tip(slot), sources::ROW_KEY)));
        face.remove_css_class("suggested-action");
        if slot == 1 {
            face.add_css_class("suggested-action");
        }
        status.set_text(&format!("{name}: narrator {slot}"));
    });
}

/// The 🗑 button.
fn wire_remove(
    remove: &gtk::Button,
    session: Rc<RefCell<Project>>,
    status: &gtk::Label,
    index: usize,
) {
    let status = status.clone();
    remove.connect_clicked(move |_| {
        // The path is read before the list changes, so the sentence can name what left. Dropping a row
        // re-indexes every row below it, and these widgets are drawn once — redrawing the list belongs
        // to F0.9's live project state, so this page reports the press without pretending to repaint.
        let Some(path) = session.borrow().sources.get(index).map(|source| source.path.clone()) else {
            return;
        };
        if sources::remove(&mut session.borrow_mut(), index).is_some() {
            let left = session.borrow().sources.len();
            status.set_text(&sources::removal_status(&path, left));
        }
    });
}

/// The tab row, drawn from [`Shell`].
///
/// A locked page's tab is greyed and carries the lock reason as its tooltip; an unlocked one carries
/// its own description (§03-shell.md §1: "Locked tab: greyed, not disabled; tooltip = the reason").
/// Nothing here decides which of those two applies — [`shell::lock`] does.
///
/// `adw::ViewSwitcher` builds its buttons itself and offers no handle to them (its API is
/// `stack`/`policy` alone), so they are found by walking the switcher's descendants for the label
/// each button draws — the page word, which is unique to one tab. Reaching the button rather than
/// the label matters: a tooltip on the label would vanish with the label when the bar narrows to
/// icons, and greying only the text would leave the button looking live.
fn paint_tabs(switcher: &adw::ViewSwitcher, shell: &Shell, project: &Project) {
    let buttons = tab_buttons(switcher.upcast_ref());
    for page in Page::all() {
        let reason = shell::lock(page, project);
        // The page shown is never greyed, even where it is locked: §1's "prerequisites vanish while
        // open" case leaves a locked page on screen for the moment before it is sent back.
        let dimmed = reason.is_some() && shell.page != page;
        let tooltip = match reason {
            Some(reason) if dimmed => reason,
            _ => page.tip(),
        };
        let matching = buttons
            .iter()
            .filter(|button| has_label(button, page.label()))
            .max_by_key(|button| depth(button));
        if let Some(button) = matching {
            button.set_tooltip_text(Some(tooltip));
            if dimmed {
                button.add_css_class("dim-label");
            } else {
                button.remove_css_class("dim-label");
            }
        }
    }
}

/// Every clickable inside `root`, deepest last.
fn tab_buttons(root: &gtk::Widget) -> Vec<gtk::Widget> {
    let mut out = Vec::new();
    for child in root.observe_children().iter::<glib::Object>() {
        let Ok(child) = child else { continue };
        let Ok(widget) = child.downcast::<gtk::Widget>() else { continue };
        if widget.downcast_ref::<gtk::Button>().is_some() {
            out.push(widget.clone());
        }
        out.extend(tab_buttons(&widget));
    }
    out
}

fn has_label(widget: &gtk::Widget, text: &str) -> bool {
    descendants(widget)
        .iter()
        .filter_map(|child| child.downcast_ref::<gtk::Label>())
        .any(|label| label.text() == text)
}

/// How deep a widget sits under its ancestor: the tab button holding the label is the deepest one,
/// while its rows and the switcher itself hold it too.
fn depth(widget: &gtk::Widget) -> usize {
    descendants(widget).len()
}

fn descendants(root: &gtk::Widget) -> Vec<gtk::Widget> {
    let mut out = Vec::new();
    for child in root.observe_children().iter::<glib::Object>() {
        let Ok(child) = child else { continue };
        if let Ok(widget) = child.downcast::<gtk::Widget>() {
            out.push(widget.clone());
            out.extend(descendants(&widget));
        }
    }
    out
}

/// The window's switching state and what it decided on arrival. `help_page` is read from the shell
/// by S4's own test; here only what is on screen.
#[derive(Debug, PartialEq)]
pub struct UiState {
    pub page: Page,
    pub status: String,
}

/// Build the window showing `project`. `page` selects the visible tab.
pub fn build_window(app: &impl IsA<gtk::Application>, project: &Project, page: &str) -> adw::ApplicationWindow {
    let window = adw::ApplicationWindow::new(app);
    window.set_default_size(1400, 900);

    // Where the window is and what it has to say: every rule behind a switch lives in shell.rs.
    let shell = Rc::new(RefCell::new(start_shell(page)));

    // The window is handed an immutable `&Project` and holds no live project yet, so the flows that
    // change the session — Rescan (F0.11) and Add sources (F0.12) — work on one private copy shared
    // between them. F0.9's live project state replaces this; until then it is what keeps the session
    // each flow leaves behind for the next. Made before the pages so Prepare's rows can hold a handle
    // to it: a row that moved the session has to move the one copy every other flow reads.
    let session = Rc::new(RefCell::new(project.clone()));

    // The status line: the shell's sentence, right-aligned in the bottom row (§1's "status line").
    // Also before the pages, for the same reason — a row's press reports itself there.
    let status = gtk::Label::new(Some(""));
    status.set_widget_name("status-line");
    status.set_xalign(1.0);
    status.set_hexpand(true);
    status.set_ellipsize(gtk::pango::EllipsizeMode::End);
    status.add_css_class("dim-label");
    status.set_margin_end(8);

    let stack = adw::ViewStack::new();
    let mut prepare_add: Option<(gtk::Button, gtk::CheckButton)> = None;
    for name in PAGES {
        let (child, add_, copy_) = page_box(name, project, &session, &status);
        stack.add_titled(&child, Some(name), name);
        if let (Some(add_), Some(copy_)) = (add_, copy_) {
            prepare_add = Some((add_, copy_));
        }
    }
    if !page.is_empty() {
        stack.set_visible_child_name(page);
    }

    // The tab strip is the ViewStack's own title widget: a TabBar needs a TabView.
    let switcher = adw::ViewSwitcher::builder()
        .stack(&stack)
        .policy(adw::ViewSwitcherPolicy::Wide)
        .build();
    paint_tabs(&switcher, &shell.borrow(), project);

    // Where ▶ is decided: the run bar's own state, beside the shell's. Nothing about which of
    // pause / transport / start applies is worked out here — run.rs does that (F0.2).
    let bar = Rc::new(RefCell::new(run::RunBar::default()));

    // The tab row is a click; the stack is where that click is decided. `guard` keeps the bounce's
    // own write-back from re-entering this handler, which would otherwise recurse through two more
    // notify signals (the prototype does the same with tabGuard, gui/main.go:1209-1224).
    let guard = Rc::new(RefCell::new(false));
    wire_switching(&stack, &switcher, &status, &shell, &guard, project);

    // §2's run bar: ▶ at the left of the row above the log. Only ▶ belongs to F0.2 — the "I'm
    // feeling lucky" gears are F0.4, ⏹ is F0.3, and the progress bar with the log expander is F0.5,
    // so none of those widgets go in here yet.
    let play = gtk::Button::from_icon_name(run::PLAY_ICON);
    play.set_widget_name("play-button");
    play.add_css_class("suggested-action");
    paint_run_bar(&play, &bar.borrow(), shell.borrow().page, run::Transport::default());

    let run_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    run_row.set_widget_name("run-bar");
    run_row.append(&play);
    run_row.append(&status);
    wire_play(&play, &bar, &shell, &status, project);

    let box_ = gtk::Box::new(gtk::Orientation::Vertical, 0);
    // §1's header bar: **1** New · **3** Save · **8** Rescan. Only those three are here — New is
    // F0.8, Save F0.10, Rescan F0.11 — while Open (**2**) is F0.9, ⓘ F0.1's S4 and Settings F0.13 stay out
    // Rescan F0.11. The window also holds no live project state (`build_window` is handed a
    // `&Project`), so both flows work off the working copy beside the root until F0.9 says which
    // project is open, and their `>>>`/`!!!` log lines have no expander to go to until F0.5's;
    // [`window_logs`] holds them meanwhile, and the status line carries what §1 shows there.
    let header = adw::HeaderBar::new();
    let new_ = gtk::Button::from_icon_name("document-new-symbolic");
    new_.set_widget_name("new-button");
    new_.set_tooltip_text(Some(NEW_TIP));
    header.pack_start(&new_);
    wire_new(&new_, &bar, &status);

    let save_ = gtk::Button::from_icon_name("document-save-symbolic");
    save_.set_widget_name("save-button");
    save_.set_tooltip_text(Some(SAVE_TIP));
    header.pack_start(&save_);
    wire_save(&save_, &bar, &status, project);

    // **8** sits at the RIGHT end of the bar, next to where Settings goes — `pack_end`, not
    // `pack_start`, so it does not join New and Save.
    let rescan_ = gtk::Button::from_icon_name("view-refresh-symbolic");
    rescan_.set_widget_name("rescan-button");
    rescan_.set_tooltip_text(Some(RESCAN_TIP));
    header.pack_end(&rescan_);
    wire_rescan(&rescan_, &status, &session);

    // F0.12 lives on the Prepare page, so its handler gets the widgets that page built. They are
    // handed over rather than found by name: `page_box` runs once per tab, and a window with four
    // same-named buttons makes `find` return whichever one it reaches first — an arbitrary page's
    // button, wired to nothing or wired twice.
    if let Some((add_, copy_)) = prepare_add {
        wire_add(&add_, &copy_, &bar, &status, &session);
    }

    box_.append(&header);
    box_.append(&switcher);
    box_.set_widget_name("page-tabs");
    box_.append(&stack);
    stack.set_vexpand(true);
    box_.append(&run_row);

    window.set_content(Some(&box_));
    window
}

/// The shell as the window starts: on the page named, or on Prepare for `""` and for a name that is
/// not one of the four (a page nobody asked for is not a reason to fail to open).
fn start_shell(page: &str) -> Shell {
    match Page::all().into_iter().find(|candidate| candidate.label() == page) {
        Some(page) => Shell { page, help_page: page, ..Shell::default() },
        None => Shell::default(),
    }
}

/// Where a tab click is decided: ask the shell, redraw from what it says, and put the strip back if
/// it refused. Split out of [`build_window`] so the handler has one job and can be driven directly.
fn wire_switching(
    stack: &adw::ViewStack,
    switcher: &adw::ViewSwitcher,
    status: &gtk::Label,
    shell: &Rc<RefCell<Shell>>,
    guard: &Rc<RefCell<bool>>,
    project: &Project,
) {
    let stack = stack.clone();
    let switcher = switcher.clone();
    let status = status.clone();
    let shell = shell.clone();
    let guard = guard.clone();
    let project = project.clone();
    stack.connect_visible_child_name_notify(move |stack| {
        if *guard.borrow() {
            return;
        }
        let shown = stack.visible_child_name().unwrap_or_default().to_string();
        let Some(target) = Page::all().into_iter().find(|page| page.label() == shown) else {
            return;
        };
        // S1-S5, all of it in shell.rs; the tree is None and there are no clips or lines because the
        // pages that read them have their own rounds — the switch itself must not depend on them.
        let outcome =
            shell.borrow_mut().switch(target, Move::Click, &project, None, &[], &mut Vec::new());
        paint_tabs(&switcher, &shell.borrow(), &project);
        status.set_text(&shell.borrow().status);

        if let Outcome::Bounced { .. } = outcome {
            // S2: the page did not move.
            *guard.borrow_mut() = true;
            stack.set_visible_child_name(shell.borrow().page.label());
            *guard.borrow_mut() = false;
        }
    });
}

/// The run bar as it should be drawn right now: the icon and tooltip [`run::controls`] says, and
/// nothing else. It decides nothing — ▶ is one button that becomes ⏸ rather than a second button
/// beside it, and which of the two applies is F0.2's precedence, settled in run.rs.
/// The run bar's icon and tooltip are the whole of what F0.2 puts on screen. `transport` is the
/// visible page's preview, which only Cut and Narrate have — [`run::transport_for`] answers `None`
/// for the other two, and that is why their ▶ runs the step while a preview plays.
fn paint_run_bar(play: &gtk::Button, bar: &run::RunBar, page: Page, transport: run::Transport) {
    let drawn = run::controls(&bar.running, run::transport_for(page, transport));
    play.set_icon_name(drawn.icon);
    play.set_tooltip_text(Some(drawn.tooltip));
}

/// Where a press of ▶ is decided: ask [`run::RunBar::press`], then redraw the button and put the
/// bar's sentence on the status line. Same shape as [`wire_switching`] — one handler, one job.
///
/// The transport is `Transport::default()` because no page has a preview model yet (the Cut review
/// and the Narrate voice sample arrive with their own pages' rounds), so nothing is ever playing to
/// be toggled; ⏹, which would end a preview, is F0.3.
fn wire_play(
    play: &gtk::Button,
    bar: &Rc<RefCell<run::RunBar>>,
    shell: &Rc<RefCell<Shell>>,
    status: &gtk::Label,
    project: &Project,
) {
    let bar = bar.clone();
    let shell = shell.clone();
    let status = status.clone();
    let project = project.clone();
    play.connect_clicked(move |play| {
        let pressed = bar.borrow_mut().press(shell.borrow().page, run::Transport::default(), &project);
        // A press that started a step has the page's own work to do (F1.1/F2.14/F4.1/F5.1), and that
        // is those flows' round; the bar only records the run it opened.
        let _ = pressed;
        paint_run_bar(
            play,
            &bar.borrow(),
            shell.borrow().page,
            run::Transport::default(),
        );
        status.set_text(&bar.borrow().status);
    });
}

/// The New button's tooltip, §1's wording for badge **1**.
const NEW_TIP: &str = "New project \u{2014} name it, put it where you want it, and start over";

/// S3's confirmation, drawn from [`new_project`]'s strings and deciding nothing: the question, the
/// two paragraphs, and the two answers. "Start new…" is destructive because what it does cannot be
/// undone, and Cancel keeps the focus so a blind Enter does nothing (the prototype's `confirm`).
pub fn new_project_confirm(
    parent: Option<&adw::ApplicationWindow>,
    detail: &str,
) -> adw::MessageDialog {
    let dialog = adw::MessageDialog::new(parent, Some(new_project::QUESTION), Some(detail));
    // The × in the header bar answers exactly as Cancel does. `close-button` is adw::Window's own
    // property (the response its × produces is `close-response`, which names Cancel), so both are
    // set through the object rather than through a trait method this binding only has on Window.
    dialog.set_property("close-response", "cancel");
    dialog.add_response("cancel", "Cancel");
    dialog.add_response("start", new_project::BUTTON);
    dialog.set_response_appearance("start", adw::ResponseAppearance::Destructive);
    dialog.set_default_response(Some("cancel"));
    dialog
}

/// Where a press of ＋ New goes: ask [`new_project::press`], then draw what it says. Every string the
/// user sees comes from that module, including the refusal's and the confirmation's.
///
/// The session is never reported empty here because the window has no source list to ask — its
/// project is the one it was built with, and the pages that own sources arrive with their own rounds.
/// A run under way is read from F0.2's [`run::RunBar`], which is the same state ▶ answers for.
fn wire_new(new_: &gtk::Button, bar: &Rc<RefCell<run::RunBar>>, status: &gtk::Label) {
    let bar = bar.clone();
    let status = status.clone();
    new_.connect_clicked(move |_| {
        let root = std::env::current_dir().unwrap_or_default();
        let open = startup::session_dir(&root);
        match new_project::press(bar.borrow().running.is_some(), false, &open) {
            new_project::Gate::Refused { reason } => status.set_text(reason),
            new_project::Gate::Name => ask_new_project(&status, &root, &open),
            new_project::Gate::Confirm { detail } => {
                let dialog = new_project_confirm(None, &detail);
                let status = status.clone();
                let root = root.clone();
                let open = open.clone();
                dialog.connect_response(None, move |dialog: &adw::MessageDialog, response| {
                    // Cancel is not pressing on: S3's whole point is that nothing was changed.
                    if response == "start" {
                        ask_new_project(&status, &root, &open);
                    }
                    dialog.destroy();
                });
                dialog.present();
            }
        }
    });
}

/// S4: ask where the new project goes and what it is called. The name's date is taken here because
/// [`new_project`] is handed the day rather than reading a clock, which is what keeps its tests off
/// today; the folder chooser is the desktop's own sheet, so the folder it hands back *is* the new
/// project, and S5's refusal or S6's sentence goes on the status line.
fn ask_new_project(status: &gtk::Label, root: &Path, open: &Path) {
    // `%F` is the ISO date, which is what the spec's default name is.
    let day = glib::DateTime::now_local()
        .and_then(|today| today.format("%F").map(|day| day.to_string()))
        .unwrap_or_else(|_| "new-project".to_string());
    let dir = open.parent().map(Path::to_path_buf).unwrap_or_else(|| root.to_path_buf());
    let dialog = gtk::FileDialog::builder()
        .title(new_project::TITLE)
        .accept_label("New project")
        .build();
    dialog.set_initial_folder(Some(&gio::File::for_path(&dir)));
    dialog.set_initial_name(Some(&new_project::free_name(&dir, &day)));

    let status = status.clone();
    let root = root.to_path_buf();
    dialog.select_folder(
        None::<&gtk::Window>,
        None::<&gio::Cancellable>,
        move |chosen: Result<gio::File, glib::Error>| {
            let Ok(file) = chosen else {
                return; // dismissed: nothing was asked for, so nothing changed
            };
            // The folder the chooser hands back *is* the project (01 §1), and its own name is what
            // S5 checked and S6 says in the status line.
            let path = file.path().unwrap_or_default();
            let named = path.file_name().unwrap_or_default().to_string_lossy().to_string();
            match new_project::create(&root, &path, &named) {
                Ok(created) => status.set_text(&created.status),
                Err(reason) => status.set_text(&reason),
            }
        },
    );
}

/// Prepare's own two widgets (F0.12): the button that adds files, and the tick that decides whether
/// they come inside the project or stay where they are. The ellipsis is the single character §1's
/// screenshot shows, not three dots.
const ADD_LABEL: &str = "Add source files…";
const COPY_LABEL: &str = "copy into project";

/// The Rescan button's tooltip, §1's wording for badge **8**.
const RESCAN_TIP: &str = "Rescan inputs and outputs";

/// Where a press of Rescan goes: [`rescan`] decides everything — which rows are gone, what was
/// re-read, what the status says. The button forwards and draws the answer (spec/00-principles.md §5).
///
/// No run refusal here: S1 drops what has vanished whether or not a run is on, so unlike Save this
/// handler never reads [`run::RunBar`].
fn wire_rescan(button: &gtk::Button, status: &gtk::Label, session: &Rc<RefCell<Project>>) {
    let status = status.clone();
    // The window is handed an immutable `&Project` and holds no live project yet, so the scan works
    // on the private copy shared with Add sources. F0.9's live project state replaces this; until
    // then it is what keeps the pruned list for whatever flow reads the session next. Which folder is
    // open is the same stand-in New and Save use, so all three flows agree.
    let root = std::env::current_dir().unwrap_or_default();
    let project = session.clone();
    button.connect_clicked(move |_| {
        let dir = startup::session_dir(&root);
        let tree = layout::Tree::new(&dir).ok();
        let found = rescan::rescan(&mut project.borrow_mut(), &root, &dir, tree.as_ref());
        for line in &found.logs {
            log_line(line);
        }
        // Only the status and the log: the pages render the `&Project` handed to `build_window` at
        // startup, so repainting them belongs with F0.9's live project state — as does F0.5's log
        // expander, which is why these lines are not yet on screen.
        status.set_text(found.status);
    });
}

/// Where a press of "Add source files…" goes: [`add_sources`] decides whether it may happen at all,
/// which kinds the chooser offers, what is copied and what is only referenced, and what the status
/// says. The button forwards (spec/00-principles.md §5).
///
/// The byte progress (`add_sources::Progress::fraction`) belongs to F0.5's progress bar, which is not
/// built yet, so it is recorded by the module and not drawn; and the session that grows here is the
/// window's private copy for the same reason [`wire_rescan`] spells out — the pages render the
/// `&Project` handed to `build_window`, and a live project state is F0.9's.
fn wire_add(
    button: &gtk::Button,
    copy_into: &gtk::CheckButton,
    bar: &Rc<RefCell<run::RunBar>>,
    status: &gtk::Label,
    session: &Rc<RefCell<Project>>,
) {
    let bar = bar.clone();
    let status = status.clone();
    let session = session.clone();
    // The tick is read when the chooser is answered, not when it opened.
    let copy_into = copy_into.clone();
    button.connect_clicked(move |_| {
        // S0 first: a copy is a run of its own, so no chooser opens while something else runs.
        if let Err(reason) = add_sources::press(bar.borrow().running.is_some()) {
            status.set_text(reason);
            return;
        }
        ask_add_sources(&status, &session, &copy_into);
    });
}

/// S1's chooser: titled for adding, filtered to audio and video, and willing to take a whole card at
/// once. As in [`ask_save_project`] it is answered by nobody in a headless test, which is the point —
/// what a test asserts is that the press got this far.
///
/// GTK 4.10 replaced the file chooser with `gtk::FileDialog` and deprecated everything below it; the
/// replacement has no multiple-selection filter API that answers a *native* chooser, so this stays on
/// the old calls and takes their warnings here rather than silencing them crate-wide.
#[expect(
    deprecated,
    reason = "the 4.10 replacement cannot offer a filtered multi-select native chooser"
)]
fn ask_add_sources(status: &gtk::Label, session: &Rc<RefCell<Project>>, copy_into: &gtk::CheckButton) {
    let chooser = gtk::FileChooserNative::builder()
        .title(add_sources::CHOOSER_TITLE)
        .modal(true)
        .action(gtk::FileChooserAction::Open)
        .build();
    // The accept button says what it does. `add_choice` is not the three-label call it looks like —
    // that one adds an extra combo — so the buttons are set where they belong.
    chooser.set_accept_label(Some("Add"));
    chooser.set_cancel_label(Some("Cancel"));
    chooser.set_select_multiple(true);
    let filter = gtk::FileFilter::new();
    filter.set_name(Some(add_sources::FILTER_NAME));
    for ext in add_sources::MEDIA_EXT {
        filter.add_pattern(&format!("*{ext}"));
    }
    chooser.add_filter(&filter);

    let status = status.clone();
    let session = session.clone();
    let copy_into = copy_into.clone();
    // The same stand-in New, Save and Rescan use, so all four flows agree on which project is open.
    let root = std::env::current_dir().unwrap_or_default();
    chooser.connect_response(move |chooser, response| {
        if response != gtk::ResponseType::Accept {
            return;
        }
        let files: Vec<std::path::PathBuf> = chooser
            .files()
            .iter::<gio::File>()
            .flatten()
            .filter_map(|file| file.path())
            .collect();
        let copy = copy_into.is_active();
        let dir = startup::session_dir(&root);
        match add_sources::add(&mut session.borrow_mut(), &root, &dir, &files, copy) {
            Ok(added) => status.set_text(&added.status),
            // S0's abandonment: the log keeps the reason, the status line the sentence.
            Err(err) => {
                status.set_text(add_sources::NO_SOURCES_DIR);
                log_line(&err);
            }
        }
    });
}

/// The Save button's tooltip, §1's wording for badge **3**.
const SAVE_TIP: &str = "Save this project to a file";

/// Where a press of Save goes: [`save_as::press`] decides whether it may happen at all, and the
/// chooser only names the project. Every sentence the user reads comes from [`save_as`].
///
/// As in [`wire_new`], the open project is the working copy beside the root until F0.9 keeps the
/// window's own project path; a run under way is F0.2's [`run::RunBar`] state.
fn wire_save(save_: &gtk::Button, bar: &Rc<RefCell<run::RunBar>>, status: &gtk::Label, project: &Project) {
    let bar = bar.clone();
    let status = status.clone();
    let project = project.clone();
    save_.connect_clicked(move |_| {
        if let Err(reason) = save_as::press(bar.borrow().running.is_some()) {
            status.set_text(reason);
            log_line(reason);
            return;
        }
        ask_save_project(&status, &project, &startup::session_dir(&std::env::current_dir().unwrap_or_default()));
    });
}

/// S2: the dialog that names the project. `select_folder` rather than `save_folder` because a
/// project is a folder (01 §1) and a file chooser would insist on a file; the name typed in the
/// sheet is what S3 renames to, so it starts as the open folder's own name.
fn ask_save_project(status: &gtk::Label, project: &Project, open: &Path) {
    let dialog = gtk::FileDialog::builder()
        .title(save_as::TITLE)
        .accept_label("Save")
        .build();
    if let Some(dir) = open.parent() {
        dialog.set_initial_folder(Some(&gio::File::for_path(dir)));
    }
    dialog.set_initial_name(open.file_name().and_then(|name| name.to_str()));

    let status = status.clone();
    let open = open.to_path_buf();
    let project = project.clone();
    dialog.select_folder(
        None::<&gtk::Window>,
        None::<&gio::Cancellable>,
        move |chosen: Result<gio::File, glib::Error>| {
            let Ok(file) = chosen else {
                return; // dismissed: nothing was asked for, so nothing changed
            };
            let named = file
                .path()
                .unwrap_or_default()
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            match save_as::save_as(&project, &open, &named) {
                Ok(saved) => {
                    for line in &saved.logs {
                        log_line(line);
                    }
                    status.set_text(&saved.status);
                }
                // The `!!!` sentence is a log line as much as the status line's: it says where the
                // files still are, which is what F0.5's log will be for.
                Err(reason) => {
                    status.set_text(&reason);
                    log_line(&reason);
                }
            }
        },
    );
}

thread_local! {
    /// The window's log lines, standing in for F0.5's log expander: `>>> moved the output folder
    /// to …` and the `!!!` refusals have nowhere to be drawn until that row exists, and a test has
    /// to be able to see that they were said at all.
    static LOGS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

fn log_line(line: &str) {
    LOGS.with(|logs| logs.borrow_mut().push(line.to_string()));
}

/// Every line the window has logged this session, oldest first.
pub fn window_logs() -> Vec<String> {
    LOGS.with(|logs| logs.borrow().clone())
}

/// The switching state as the window currently shows it — for the tests that drive a real click.
pub fn state(window: &adw::ApplicationWindow) -> UiState {
    let (Some(switcher), Some(status)) = (find_switcher(window.upcast_ref()), find_status(window.upcast_ref()))
    else {
        panic!("the window has no tab row");
    };
    let page = switcher
        .stack()
        .and_then(|stack| stack.visible_child_name().map(|name| name.to_string()))
        .unwrap_or_default();
    UiState {
        page: Page::all()
            .into_iter()
            .find(|candidate| candidate.label() == page)
            .unwrap_or(Page::Prepare),
        status: status.text().to_string(),
    }
}

/// The tooltip the tab button for `page` carries, which is how a test reads §1's "tooltip = the
/// reason" without reaching inside the switcher itself.
pub fn tab_tooltip(window: &adw::ApplicationWindow, page: Page) -> Option<String> {
    tab_button(window, page)?.tooltip_text().map(|text| text.to_string())
}

/// Whether the tab for `page` is greyed (§1's "greyed, not disabled" — the class, never `set_sensitive`).
pub fn tab_dimmed(window: &adw::ApplicationWindow, page: Page) -> bool {
    tab_button(window, page)
        .is_some_and(|button| button.has_css_class("dim-label"))
}

/// The tab button drawing `page`'s word, so a test can click it the way a user does.
pub fn tab_button(window: &adw::ApplicationWindow, page: Page) -> Option<gtk::Button> {
    let switcher = find_switcher(window.upcast_ref())?;
    tab_buttons(switcher.upcast_ref())
        .into_iter()
        .filter(|button| has_label(button, page.label()))
        .max_by_key(|button| depth(button))?
        .downcast()
        .ok()
}

/// The switcher inside the window: it is the one thing in here with a stack of its own.
fn find_switcher(root: &gtk::Widget) -> Option<adw::ViewSwitcher> {
    if let Some(switcher) = root.downcast_ref::<adw::ViewSwitcher>() {
        return Some(switcher.clone());
    }
    for child in root.observe_children().iter::<glib::Object>() {
        let Ok(child) = child else { continue };
        let Ok(widget) = child.downcast::<gtk::Widget>() else { continue };
        if let Some(found) = find_switcher(&widget) {
            return Some(found);
        }
    }
    None
}

/// The Save button, named so a test can press it the way a user does.
pub fn save_button(window: &adw::ApplicationWindow) -> Option<gtk::Button> {
    find_widget_by_name(window.upcast_ref(), "save-button")?
        .downcast()
        .ok()
}

/// The tooltip Save carries, which is how a test reads §1's wording for badge **3**.
pub fn save_tooltip(window: &adw::ApplicationWindow) -> Option<String> {
    save_button(window)?.tooltip_text().map(|text| text.to_string())
}

/// The Rescan button, named so a test can press it the way a user does.
pub fn rescan_button(window: &adw::ApplicationWindow) -> Option<gtk::Button> {
    find_widget_by_name(window.upcast_ref(), "rescan-button")?
        .downcast()
        .ok()
}

/// One widget from Prepare's sources list, found by the name §4's rows are built with — `sources-list`,
/// `source-row-<i>`, `footage-<i>`, `narrator-<i>`, `remove-<i>`, `source-name-<i>`. One accessor for
/// the whole list rather than six, because the row count is the project's business: a test asking for
/// `remove-7` of a two-row session gets `None`, which is the honest answer.
pub fn find_source_widget(window: &adw::ApplicationWindow, name: &str) -> Option<gtk::Widget> {
    find_widget_by_name(window.upcast_ref(), name)
}

/// The tooltip Rescan carries, which is how a test reads §1's wording for badge **8**.
pub fn rescan_tooltip(window: &adw::ApplicationWindow) -> Option<String> {
    rescan_button(window)?.tooltip_text().map(|text| text.to_string())
}

/// Prepare's "Add source files…" button, named so a test can press it the way a user does.
pub fn add_sources_button(window: &adw::ApplicationWindow) -> Option<gtk::Button> {
    find_widget_by_name(window.upcast_ref(), "add-sources-button")?
        .downcast()
        .ok()
}

/// The "copy into project" tick beside it (F0.12 S2), which is what the press reads to decide
/// between a copy and a reference.
pub fn copy_into_project(window: &adw::ApplicationWindow) -> Option<gtk::CheckButton> {
    find_widget_by_name(window.upcast_ref(), "copy-into-project")?
        .downcast()
        .ok()
}

/// The ASR code the page's Language box holds (§1's badge **10**) — how a test sees that typing
/// reached [`Project::language`] rather than stopping at the widget.
pub fn bench_language(window: &adw::ApplicationWindow) -> Option<String> {
    let entry = find_widget_by_name(window.upcast_ref(), "language-entry")?
        .downcast::<gtk::Entry>()
        .ok()?;
    Some(entry.text().to_string())
}



/// The New button, named so a test can press it the way a user does.
pub fn new_button(window: &adw::ApplicationWindow) -> Option<gtk::Button> {
    find_widget_by_name(window.upcast_ref(), "new-button")?
        .downcast()
        .ok()
}

/// The tooltip ＋ New carries, which is how a test reads §1's wording for badge **1**.
pub fn new_tooltip(window: &adw::ApplicationWindow) -> Option<String> {
    new_button(window)?.tooltip_text().map(|text| text.to_string())
}

/// The ▶ button, named rather than searched for by icon: the icon changes to ⏸ while a run works,
/// and an unnamed busy button would then be findable no more.
pub fn play_button(window: &adw::ApplicationWindow) -> Option<gtk::Button> {
    find_widget_by_name(window.upcast_ref(), "play-button")?.downcast().ok()
}

/// The tooltip ▶ carries, which is how a test reads §2's wording without reaching into the button.
pub fn play_tooltip(window: &adw::ApplicationWindow) -> Option<String> {
    play_button(window)?.tooltip_text().map(|text| text.to_string())
}

/// The first widget under `root` carrying `name`, found the same walk as [`find_status`].
fn find_widget_by_name(root: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
    if root.widget_name() == name {
        return Some(root.clone());
    }
    for child in root.observe_children().iter::<glib::Object>() {
        let Ok(child) = child else { continue };
        let Ok(widget) = child.downcast::<gtk::Widget>() else { continue };
        if let Some(found) = find_widget_by_name(&widget, name) {
            return Some(found);
        }
    }
    None
}

/// The status line, named rather than searched for by text: an empty label is the ordinary state and
/// would match any label in the tree.
fn find_status(root: &gtk::Widget) -> Option<gtk::Label> {
    if root.widget_name() == "status-line" {
        return root.downcast_ref::<gtk::Label>().cloned();
    }
    for child in root.observe_children().iter::<glib::Object>() {
        let Ok(child) = child else { continue };
        let Ok(widget) = child.downcast::<gtk::Widget>() else { continue };
        if let Some(found) = find_status(&widget) {
            return Some(found);
        }
    }
    None
}
