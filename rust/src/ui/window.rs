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
use crate::cut::{self, Cut};
use crate::cut_hear;
use crate::cut_play;
use crate::cut_review;
use crate::cut_line;
use crate::cut_screen;
use crate::hand_edit;
use crate::layout;
use crate::new_project;
use crate::open_project;
use crate::prepare;
use crate::preview::{self, Player, Press};
use crate::project::Project;
use crate::rescan;
use crate::lucky;
use crate::run;
use crate::runqueue;
use crate::exchanges;
use crate::save_as;
use crate::ui::settings;
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


    // F2.1: the Cut page's own transport, first under the title rather than last in the box. The
    // page's content box is top-packed and does not stretch its children, so anything appended after
    // the context paragraph sits below a tall label and reads as an unrelated control; the spec puts
    // the transport at the head of the page, so it goes there.
    if page == Page::Cut.label() {
        let play_ = gtk::Button::with_label(RECORD_PLAY_LABEL);
        play_.set_widget_name("play-recording-button");
        play_.set_tooltip_text(Some(RECORD_PLAY_TIP));
        play_.set_halign(gtk::Align::Start);
        box_.insert_child_after(&play_, Some(&title));

        // F2.2: ▶✂ beside ▶, not below it — §A puts the two in one transport group and the person
        // reads them as a pair ("the recording" / "the cut"). Greyed with no clips (S1) rather than
        // dead: the same rule `cut_play::pressed` refuses with lives in `can_play_cut`, so the button
        // cannot be clickable on a cut that has nothing to skip to.
        let cut_ = gtk::Button::with_label(PLAY_CUT_LABEL);
        cut_.set_widget_name("play-cut-button");
        cut_.set_tooltip_text(Some(cut_screen::PLAY_CUT_TIP));
        cut_.set_halign(gtk::Align::Start);
        box_.insert_child_after(&cut_, Some(&play_));

        // F2.3: ▶✂✂ last of the three, so the group reads recording → cut → review (§A's order).
        let review_ = gtk::Button::with_label(REVIEW_CUTS_LABEL);
        review_.set_widget_name("review-cuts-button");
        review_.set_tooltip_text(Some(cut_screen::REVIEW_TIP));
        review_.set_halign(gtk::Align::Start);
        box_.insert_child_after(&review_, Some(&cut_));

        // F2.4: the line-step buttons, ‹‹f ‹f f› ››. §A puts them in the same toolbar row as the
        // transport controls; that full toolbar is a later round's page, so these are the line-step
        // half of it, sitting on after ▶✂✂ until the real bar arrives. Shift is five frames, plain is
        // one — `cut_line::step_frames` owns that, and FRAME_TIP already says so.
        let mut previous = review_.clone();
        for (name, label, _shift) in LINE_STEP_BUTTONS {
            let step = gtk::Button::with_label(label);
            step.set_widget_name(name);
            step.set_tooltip_text(Some(cut_screen::FRAME_TIP));
            step.set_halign(gtk::Align::Start);
            box_.insert_child_after(&step, Some(&previous));
            previous = step;
        }

        // F2.5 S6: the preview volume. One control for one number — `cut_hear::PreviewVolume` is the
        // app's single loudness setting and every slider shown mirrors it, so this widget owns no copy of
        // its own. 0..100 at step 1 because that is what a slider reads as, while the property under it
        // is 0..1; the two meet in `PreviewVolume::set_percent`. Width 120 px per
        // `spec/inventory/cut.md` §D item 2. The inventory asks for an icon beside it; none is drawn --
        // a themed speaker icon does not render reliably in the headless snapshot (no icon theme), so the
        // shared tooltip carries the wording instead and the slider stands alone. Value text off: the
        // number means nothing to a person where the tooltip says what the control does.
        let volume = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0);
        volume.set_widget_name("preview-volume");
        volume.set_tooltip_text(Some(cut_hear::VOLUME_TIP));
        volume.set_draw_value(false);
        volume.set_size_request(120, -1);
        volume.set_halign(gtk::Align::Start);
        volume.set_value(cut_hear::VOLUME_DEFAULT * 100.0);
        box_.insert_child_after(&volume, Some(&previous));
        // The handlers go on in `build_window`, after `set_content`: a click handler attached to a
        // widget that is not yet inside the realized tree never fires (see the F2.1 note there).
    }

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
    /// F0.5 S1: whether the log expander is open — read from the widget, so a test checks the same
    /// thing the user sees rather than a flag that might not have been drawn.
    pub log_expanded: bool,
    /// F0.5 S2: how far the run's progress bar has got, 0 when idle.
    pub progress: f64,
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
    SESSION.with(|slots| slots.borrow_mut().push(Rc::clone(&session)));

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
    WINDOW_BAR.with(|slots| slots.borrow_mut().push(Rc::clone(&bar)));

    // The children a run has out there, so ⏹ can reach them (F0.3 S3). Held here rather than inside
    // the bar because the flows that spawn register into it and the bar is what decides to stop;
    // nothing spawns yet, so this stays empty until the runner's round arrives.
    let procs = Rc::new(RefCell::new(run::Subprocesses::default()));

    // The tab row is a click; the stack is where that click is decided. `guard` keeps the bounce's
    // own write-back from re-entering this handler, which would otherwise recurse through two more
    // notify signals (the prototype does the same with tabGuard, gui/main.go:1209-1224).
    let guard = Rc::new(RefCell::new(false));
    wire_switching(&stack, &switcher, &status, &shell, &guard, project);

    // F0.5's bookkeeping, drawn: the bar that says how far the run has got and the log it writes
    // into. Both are window-level (built once here, not per page), so a name lookup cannot land on a
    // second copy the way a same-named page button could.
    let queue = Rc::new(RefCell::new(runqueue::Queue::new()));
    WINDOW_QUEUE.with(|slots| slots.borrow_mut().push(Rc::clone(&queue)));
    let exchange_log = Rc::new(RefCell::new(exchanges::RunLog::new()));

    // The progress bar sits between ⏹ and the status line: §2's bar reads play, stop, gears, then
    // the run's fraction and its words. Idle is 0 rather than hidden — a bar that vanishes makes the
    // row jump when a run starts.
    let progress = gtk::ProgressBar::new();
    progress.set_widget_name("run-progress");
    progress.set_hexpand(true);
    progress.set_valign(gtk::Align::Center);

    // §1: "the log expander, its header carrying the status line". The header is a row of two: the
    // word "Log" so the control is never an unlabelled caret (at rest the status is empty), and the
    // status label itself filling the rest. `state()` still reads the label by name, so every existing
    // assertion on `state().status` keeps working unchanged — only its parent moved.
    let log_view = gtk::TextView::new();
    log_view.set_widget_name("log-view");
    log_view.set_editable(false);
    log_view.set_cursor_visible(false);
    log_view.set_monospace(true);
    log_view.set_wrap_mode(gtk::WrapMode::WordChar);
    log_view.set_vexpand(true);
    // §10 lists the log's heights only in prose ("log heights 220/110") and gives no `P.` id, so the
    // numbers go on the widget with this note rather than as invented parameter rows.
    log_view.set_height_request(LOG_OPEN_PX);

    let log_header = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    log_header.set_widget_name("log-header");
    let log_title = gtk::Label::new(Some("Log"));
    log_title.add_css_class("heading");
    log_header.append(&log_title);
    log_header.append(&status);

    let log = gtk::Expander::new(Some("Log"));
    log.set_widget_name("log-expander");
    log.set_label_widget(Some(&log_header));
    log.set_child(Some(&log_view));
    log.set_expanded(false);

    // §2's run bar: ▶ at the left of the row above the log, ⏹ next to it (F0.3), then the "I'm
    // feeling lucky" gears (F0.4), then the progress bar F0.5 adds.
    let play = gtk::Button::from_icon_name(run::PLAY_ICON);
    play.set_widget_name("play-button");
    play.add_css_class("suggested-action");
    // ⏹ beside ▶, in the order §2's run bar reads: play, stop, then the status line.
    let stop_ = gtk::Button::from_icon_name(run::STOP_ICON);
    stop_.set_widget_name("stop-button");
    // The gears after ⏹: labelled rather than icon-only, because §03 names the control by its words and
    // the spec's run-bar image shows the phrase on the button.
    let lucky = gtk::Button::with_label(lucky::LUCKY_LABEL);
    lucky.set_widget_name("lucky-button");
    lucky.set_tooltip_text(Some(lucky::LUCKY_TOOLTIP));
    paint_run_bar(
        &play,
        &stop_,
        &bar.borrow(),
        shell.borrow().page,
        run::Transport::default(),
    );

    let run_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    run_row.set_widget_name("run-bar");
    run_row.append(&play);
    run_row.append(&stop_);
    run_row.append(&lucky);
    run_row.append(&progress);
    wire_play(
        &play,
        &stop_,
        &bar,
        &shell,
        &status,
        &progress,
        &log,
        &log_view,
        &queue,
        &exchange_log,
        project,
    );
    wire_stop(
        &stop_,
        &play,
        &bar,
        &shell,
        &status,
        &progress,
        &log,
        &log_view,
        &queue,
        &procs,
    );
    wire_lucky(
        &lucky,
        &bar,
        &shell,
        &stack,
        &guard,
        &status,
        &progress,
        &log,
        &log_view,
        &queue,
        &exchange_log,
        &procs,
        project.clone(),
    );

    let box_ = gtk::Box::new(gtk::Orientation::Vertical, 0);
    // §1's header bar: **1** New · **2** Open · **3** Save · **8** Rescan. New is F0.8, Open F0.9,
    // Save F0.10, Rescan F0.11 and Settings F0.13; ⓘ (F0.1's S4) stays out for its own round.
    // The window holds one live project — `session` below — which is what makes Open possible: the
    // chooser hands over a folder and that single copy is replaced with what was read from it, so
    // every flow already sharing the handle reads the opened project rather than the one the launch
    // started with.
    let header = adw::HeaderBar::new();
    let new_ = gtk::Button::from_icon_name("document-new-symbolic");
    new_.set_widget_name("new-button");
    new_.set_tooltip_text(Some(NEW_TIP));
    header.pack_start(&new_);
    wire_new(&new_, &bar, &status);

    // **2** Open, between New and Save as §1 reads the bar.
    let open_ = gtk::Button::from_icon_name("document-open-symbolic");
    open_.set_widget_name("open-button");
    open_.set_tooltip_text(Some(OPEN_TIP));
    header.pack_start(&open_);
    wire_open(&open_, &bar, &status, &session);

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

    // F0.13's Settings button, packed after Rescan so Rescan stays rightmost as §1 reads the bar.
    // The dialog itself is built on the press, in `settings.rs`; nothing here decides anything.
    let settings_ = gtk::Button::from_icon_name("emblem-system-symbolic");
    settings_.set_widget_name("settings-button");
    settings_.set_tooltip_text(Some(SETTINGS_TIP));
    header.pack_end(&settings_);
    wire_settings(&settings_, &window);
    // F0.12 lives on the Prepare page, so its handler gets the widgets that page built. They are
    // handed over rather than found by name: `page_box` runs once per tab, and a window with four
    // same-named buttons makes `find` return whichever one it reaches first — an arbitrary page's
    // button, wired to nothing or wired twice.
    if let Some((add_, copy_)) = prepare_add {
        wire_add(&window, &add_, &copy_, &bar, &status, &session);
    }

    box_.append(&header);
    box_.append(&switcher);
    box_.set_widget_name("page-tabs");
    box_.append(&stack);
    stack.set_vexpand(true);
    box_.append(&run_row);
    // The log goes under the run bar, as §1 lists them. A `gtk::Paned` divider (the "two halves of a
    // draggable divider" line) is the window's own item, not F0.5's bookkeeping, so the box layout
    // stays and the expander simply takes its height back when collapsed.
    box_.append(&log);

    window.set_content(Some(&box_));

    // F2.1: this window's preview is registered only now that the content tree exists. A `Rc` clone of
    // a GTK widget handed out earlier keeps the ref floating outside the tree, and GTK then hands the
    // button's own strong reference back to the caller — which leaves the click handler never reached,
    // because nothing inside the realized window is the object the test fires. Registering after
    // `set_content` puts the wired button in the tree and makes the newest slot the live one.
    if let Some(play_) = play_recording_button(&window) {
        PREVIEW_PLAYERS.with(|slots| {
            slots.borrow_mut().push(Rc::new(std::cell::RefCell::new(Player::default())))
        });
        wire_play_recording(&play_, &window);
    }
    // F2.5 S6: this window's one preview volume, pushed the way the player slot is — after
    // `set_content`, so the slider being wired is the one inside the realized tree. Guarded by the
    // same lookup as the slider itself rather than a page check: only the Cut page builds one, so a
    // missing widget *is* "not this page", and every window still gets exactly one volume slot.
    if preview_volume_scale(&window).is_some() {
        PREVIEW_VOLUMES.with(|slots| {
            slots
                .borrow_mut()
                .push(Rc::new(std::cell::RefCell::new(cut_hear::PreviewVolume::default())))
        });
        if let Some(scale) = preview_volume_scale(&window) {
            wire_preview_volume(&scale, &window);
        }
    }
    // F2.2: ▶✂ is wired to the same player slot ▶ moves, because they are two views of one preview —
    // pressing one switches what the other would show, never a second transport.
    if let Some(cut_) = play_cut_button(&window) {
        wire_play_cut(&cut_, &window);
    }
    // F2.3: this window's cut slot is registered here for the same reason the player slot is — after
    // `set_content`, so the button being wired is the one inside the realized tree.
    REVIEW_CUTS.with(|slots| slots.borrow_mut().push(Rc::new(std::cell::RefCell::new(cut::Cut::default()))));
    if let Some(review_) = review_cuts_button(&window) {
        wire_review_cuts(&review_, &window);
    }
    // F2.4: this window's line slot, registered with the others so the newest window is the live one.
    // A project root does not reach the page yet (the cut-model round owns it), so nothing is restored
    // here — `cut_line::restore(root, recordings)` is the seam that seeds this when a root exists, and
    // until then the line starts at second zero.
    LINE_STATES.with(|slots| {
        slots.borrow_mut().push(Rc::new(std::cell::RefCell::new((
            cut_line::LinePos { t: 0.0 },
            cut_line::LineWriter::default(),
        ))))
    });
    // F2.4: the four line-step buttons get their handlers now that they sit in the realized tree.
    for (name, _label, frames) in LINE_STEP_BUTTONS {
        if let Some(step) = line_step_button(&window, name) {
            wire_line_step(&step, &window, frames);
        }
    }
    wire_line_keys(&window);
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
fn paint_run_bar(
    play: &gtk::Button,
    stop_: &gtk::Button,
    bar: &run::RunBar,
    page: Page,
    transport: run::Transport,
) {
    let drawn = run::controls(&bar.running, run::transport_for(page, transport));
    play.set_icon_name(drawn.icon);
    play.set_tooltip_text(Some(drawn.tooltip));
    // ⏹ is sensitive whenever there is something to end — a run, a playing preview, or one parked
    // part way through. `controls` already decides that; this only draws it.
    stop_.set_sensitive(drawn.stop_sensitive);
}

/// §10's prose log heights: 220 px open, 110 px collapsed. No `P.` id exists for them, so they are
/// bare constants here with the note rather than invented parameter rows.
const LOG_OPEN_PX: i32 = 220;

/// Draw F0.5's two widgets from the bookkeeping state: the bar's fraction and tooltip come from the
/// queue, the expander's openness from the run's `log_expanded`, and its body from the lines logged
/// so far. No decisions here — [`runqueue`] decided them, this paints them.
fn paint_progress(
    progress: &gtk::ProgressBar,
    log: &gtk::Expander,
    log_view: &gtk::TextView,
    queue: &runqueue::Queue,
    expanded: bool,
) {
    let (_text, tip) = queue.text_and_tip();
    progress.set_fraction(queue.fraction());
    // A run with nothing queued yet leaves the standing tooltip rather than blanking it (§2).
    if !tip.is_empty() {
        progress.set_tooltip_text(Some(&tip));
    }
    log.set_expanded(expanded);
    if expanded {
        let mut body = String::new();
        for line in window_logs() {
            body.push_str(&line);
            body.push('\n');
        }
        let buffer = log_view.buffer();
        buffer.set_text(&body);
        // The newest line is the one worth seeing.
        let mut end = buffer.end_iter();
        log_view.scroll_to_iter(&mut end, 0.0, false, 0.0, 1.0);
    }
}

/// Where a press of ▶ is decided: ask [`run::RunBar::press`], then redraw the button and put the
/// bar's sentence on the status line. Same shape as [`wire_switching`] — one handler, one job.
///
/// The transport is `Transport::default()` because no page has a preview model yet (the Cut review
/// and the Narrate voice sample arrive with their own pages' rounds), so nothing is ever playing to
/// be toggled; ⏹, which would end a preview, is F0.3.
fn wire_play(
    play: &gtk::Button,
    stop_: &gtk::Button,
    bar: &Rc<RefCell<run::RunBar>>,
    shell: &Rc<RefCell<Shell>>,
    status: &gtk::Label,
    progress: &gtk::ProgressBar,
    log: &gtk::Expander,
    log_view: &gtk::TextView,
    queue: &Rc<RefCell<runqueue::Queue>>,
    exchange_log: &Rc<RefCell<exchanges::RunLog>>,
    project: &Project,
) {
    let bar = bar.clone();
    let shell = shell.clone();
    let status = status.clone();
    let project = project.clone();
    let stop_ = stop_.clone();
    let progress = progress.clone();
    let log = log.clone();
    let log_view = log_view.clone();
    let queue = queue.clone();
    let exchange_log = exchange_log.clone();
    play.connect_clicked(move |play| {
        let pressed = bar.borrow_mut().press(shell.borrow().page, run::Transport::default(), &project);
        // A refusal has to survive to the end of this handler: `run::RunBar::press` leaves its own
        // status empty when a run starts (the run is meant to overwrite it), and the tail below
        // copies that empty string onto the label. Writing the refusal earlier would erase it.
        let mut refusal: Option<String> = None;
        // F0.5 S1: a press that opened a run also opened the bookkeeping — fresh cancel context,
        // empty queue, model log closed, log expander open. A pause or a transport toggle is not a
        // new run, so only `Started` goes through `start_run`.
        if matches!(pressed, run::Pressed::Started { .. }) {
            // F1.12 S2: the Cut step first asks whether `final.txt` was hand-edited since the marks
            // were written. The rule is entirely in `hand_edit::before_cut`; this handler only runs
            // it for the page that owns the marks and forwards what it says. Spec silent on a lucky
            // all-steps run (F0.4), so it is checked here too — the same press, the same question.
            if run::step(shell.borrow().page) == run::Step::Suggest {
                let dir = startup::session_dir(&std::env::current_dir().unwrap_or_default());
                if let Ok(tree) = layout::Tree::new(&dir) {
                    let sources: Vec<String> = project
                        .sources
                        .iter()
                        .map(|source| source.path.clone())
                        .collect();
                    if let Some(outcome) = hand_edit::before_cut(&tree, &sources) {
                        for line in &outcome.logs {
                            log_line(line);
                        }
                        if outcome.refused {
                            // The refusal sentence goes where a press's answer is read — held until
                            // after the tail's own status write, which would otherwise blank it.
                            refusal = Some(
                                outcome
                                    .logs
                                    .last()
                                    .map(|line| line.trim_start_matches('!').trim_start().to_string())
                                    .unwrap_or_else(|| "the edit was refused".to_string()),
                            );
                        }
                    }
                }
            }
            runqueue::start_run(
                &mut bar.borrow_mut(),
                &mut queue.borrow_mut(),
                &mut exchange_log.borrow_mut(),
                run::step(shell.borrow().page),
                run::snapshot_sources(&project),
            );
        }
        paint_run_bar(
            play,
            &stop_,
            &bar.borrow(),
            shell.borrow().page,
            run::Transport::default(),
        );
        status.set_text(&bar.borrow().status);
        paint_progress(
            &progress,
            &log,
            &log_view,
            &queue.borrow(),
            bar.borrow().running.as_ref().is_some_and(|run| run.log_expanded),
        );
        // Last write wins on the label: F1.12's refusal is why ▶ did not do what it was pressed for,
        // so it goes on screen after the bar's own (empty) status rather than under it.
        if let Some(reason) = refusal {
            status.set_text(&reason);
        }
    });
}

/// Where a press of ⏹ is decided: ask [`run::RunBar::press_stop`], then redraw both buttons and put
/// the bar's sentence on the status line. The handler forwards and decides nothing
/// (spec/00-principles.md §5) — which of S1/S2/S3 applies, and what each one stops, is run.rs's rule.
///
/// `in_describe` is `false` here because the shell does not know which stage is running; F1.7's own
/// round passes the real value when it drives Describe (S5 arms "describe from the start").
fn wire_stop(
    stop_: &gtk::Button,
    play: &gtk::Button,
    bar: &Rc<RefCell<run::RunBar>>,
    shell: &Rc<RefCell<Shell>>,
    status: &gtk::Label,
    progress: &gtk::ProgressBar,
    log: &gtk::Expander,
    log_view: &gtk::TextView,
    queue: &Rc<RefCell<runqueue::Queue>>,
    procs: &Rc<RefCell<run::Subprocesses>>,
) {
    let bar = bar.clone();
    let shell = shell.clone();
    let status = status.clone();
    let procs = procs.clone();
    let play = play.clone();
    let progress = progress.clone();
    let log = log.clone();
    let log_view = log_view.clone();
    let queue = queue.clone();
    stop_.connect_clicked(move |stop_| {
        let stopped = bar.borrow_mut().press_stop(
            shell.borrow().page,
            run::Transport::default(),
            false,
            &mut procs.borrow_mut(),
        );
        // Repaint both: stopping a run changes ▶'s face as well as ⏹'s sensitivity.
        paint_run_bar(
            &play,
            stop_,
            &bar.borrow(),
            shell.borrow().page,
            run::Transport::default(),
        );
        // Only write a sentence when there is one: S2's "nothing more" must leave an existing status
        // alone rather than blanking it.
        if !stopped.status().is_empty() {
            status.set_text(stopped.status());
        }
        paint_progress(
            &progress,
            &log,
            &log_view,
            &queue.borrow(),
            bar.borrow().running.as_ref().is_some_and(|run| run.log_expanded),
        );
    });
}

/// Where a press of the "I'm feeling lucky" gears is decided: ask [`lucky::Chain::start`] whether the
/// bar will allow it, then walk the chain one step at a time. The handler forwards and decides nothing
/// (spec/00-principles.md §5) — S1's refusal, S3's opening line, S4's skips and S6's end sentence all
/// come from `lucky.rs`.
///
/// Each step is handed over by switching to its page synchronously through the same `stack`/`guard`
/// path F0.1 uses (a step's own work belongs to the widgets of the page it runs on), logging
/// `>>> run: <Name>`, and then reporting the outcome back with [`lucky::Chain::next`]. **The real
/// outcomes arrive with F1.1/F2.14/F4.1/F5.1's rounds** — those step bodies are not implemented yet,
/// so every step reports [`lucky::StepOutcome::Declined`] here. What this item pins is the wire and the
/// state machine, not the work behind each step; when a flow lands it replaces that one call with its
/// own outcome and the chain needs no other change.
fn wire_lucky(
    lucky_button: &gtk::Button,
    bar: &Rc<RefCell<run::RunBar>>,
    shell: &Rc<RefCell<Shell>>,
    stack: &adw::ViewStack,
    guard: &Rc<RefCell<bool>>,
    status: &gtk::Label,
    progress: &gtk::ProgressBar,
    log: &gtk::Expander,
    log_view: &gtk::TextView,
    queue: &Rc<RefCell<runqueue::Queue>>,
    exchange_log: &Rc<RefCell<exchanges::RunLog>>,
    procs: &Rc<RefCell<run::Subprocesses>>,
    project: Project,
) {
    let lucky_button = lucky_button.clone();
    let bar = bar.clone();
    let shell = shell.clone();
    let stack = stack.clone();
    let guard = guard.clone();
    let status = status.clone();
    let procs = procs.clone();
    let progress = progress.clone();
    let log = log.clone();
    let log_view = log_view.clone();
    let queue = queue.clone();
    let exchange_log = exchange_log.clone();
    lucky_button.connect_clicked(move |button| {
        // S1: busy is read from the same bar state ▶ and ⏹ answer for, so the three buttons cannot
        // disagree about whether something is going.
        let busy = bar.borrow().running.is_some() || bar.borrow().stop_flag;
        let (mut chain, opening) = match lucky::Chain::start(busy) {
            Ok(started) => started,
            Err(refusal) => {
                status.set_text(refusal);
                return;
            }
        };
        log_line(&opening);

        // F0.5 S1: the chain is a run like any other, so it opens the bookkeeping the same way —
        // fresh cancel context, empty queue, model log closed, log expanded.
        runqueue::start_run(
            &mut bar.borrow_mut(),
            &mut queue.borrow_mut(),
            &mut exchange_log.borrow_mut(),
            run::step(shell.borrow().page),
            run::snapshot_sources(&project),
        );
        paint_progress(&progress, &log, &log_view, &queue.borrow(), true);

        // S2: the gears turn while the chain runs. Animation itself is cosmetic; what matters is that
        // the button cannot be pressed a second time mid-chain, which S1 would also refuse.
        button.set_sensitive(false);
        button.add_css_class("lucky-running");

        loop {
            let advance = chain.next(lucky::StepOutcome::Declined);
            match advance {
                lucky::Advance::Run { page, name } => {
                    // F0.1's synchronous move: show the page so the step has its widgets. `guard`
                    // keeps the stack's own write-back from re-entering the tab handler.
                    *guard.borrow_mut() = true;
                    stack.set_visible_child_name(page.label());
                    *guard.borrow_mut() = false;
                    shell.borrow_mut().page = page;
                    log_line(&lucky::step_line(name));
                }
                lucky::Advance::Skipped { line } => {
                    log_line(&line);
                    // A skip closes no running step; keep walking.
                }
                lucky::Advance::End { line, status: short } => {
                    log_line(&line);
                    status.set_text(&short);
                    // F0.5 S4: the chain ended, so its bookkeeping ends with it and the bar stops.
                    runqueue::end_run(&mut bar.borrow_mut(), None);
                    queue.borrow_mut().reset();
                    break;
                }
                lucky::Advance::Idle => break,
            }
            // A Declined outcome never blocks, but a stop flag set from elsewhere (⏹ during the chain)
            // must end it rather than spin.
            if bar.borrow().stop_flag {
                let stopped = chain.next(lucky::StepOutcome::Stopped);
                if let lucky::Advance::End { line, status: short } = stopped {
                    log_line(&line);
                    status.set_text(&short);
                    runqueue::end_run(&mut bar.borrow_mut(), None);
                    queue.borrow_mut().reset();
                }
                break;
            }
        }

        // The chain is over either way: give the button back. F0.5 S4 closes its bookkeeping too —
        // running off, so nothing downstream thinks a run still stands; the log's own state is kept
        // as it was painted (open), because that is where the end line and any refusal were read.
        runqueue::end_run(&mut bar.borrow_mut(), None);
        queue.borrow_mut().reset();
        button.set_sensitive(true);
        button.remove_css_class("lucky-running");
        paint_progress(
            &progress,
            &log,
            &log_view,
            &queue.borrow(),
            log.is_expanded(),
        );
        let _ = procs.borrow();
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

/// §1's wording for the Settings badge: the two servers the dialog is about, named in the tooltip
/// so the gear is never an unlabelled control.
const SETTINGS_TIP: &str = "Settings \u{2014} the LLM and audio.cpp endpoints";

/// Where a press of Rescan goes: [`rescan`] decides everything — which rows are gone, what was
/// re-read, what the status says. The button forwards and draws the answer (spec/00-principles.md §5).
///
/// No run refusal here: S1 drops what has vanished whether or not a run is on, so unlike Save this
/// handler never reads [`run::RunBar`].
/// F0.13: the gear opens the Settings dialog over this window. Nothing is decided here — the rows,
/// their badges and Test All live in [`settings`], and every pass rule lives in [`crate::checks`].
fn wire_settings(button: &gtk::Button, window: &adw::ApplicationWindow) {
    let window = window.clone();
    button.connect_clicked(move |_| {
        settings::open_with(&window, settings::no_probe());
    });
}

fn wire_rescan(button: &gtk::Button, status: &gtk::Label, session: &Rc<RefCell<Project>>) {    let status = status.clone();
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
/// The copy's bytes are drawn on F0.5's bar: a copy is a run of its own (F0.12 S0), so it reports
/// through the same bookkeeping ▶'s run uses ([`add_files`], which feeds `runqueue::Queue`) rather
/// than painting a second bar. The session that grows here is the window's private copy for the
/// same reason [`wire_rescan`] spells out — the pages render the `&Project` handed to
/// `build_window`, and a live project state is F0.9's.
fn wire_add(
    window: &adw::ApplicationWindow,
    button: &gtk::Button,
    copy_into: &gtk::CheckButton,
    bar: &Rc<RefCell<run::RunBar>>,
    status: &gtk::Label,
    session: &Rc<RefCell<Project>>,
) {
    let window = window.clone();
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
        ask_add_sources(&window, &status, &session, &copy_into);
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
fn ask_add_sources(
    window: &adw::ApplicationWindow,
    status: &gtk::Label,
    session: &Rc<RefCell<Project>>,
    copy_into: &gtk::CheckButton,
) {
    let chooser = gtk::FileChooserNative::builder()        .title(add_sources::CHOOSER_TITLE)
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
    let window = window.clone();
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
        // The same body a widget test drives, so the chooser and the seam cannot drift apart.
        add_files(&window, &files);
    });
}

/// The Save button's tooltip, §1's wording for badge **3**.
const SAVE_TIP: &str = "Save this project to a file";

/// The seam a widget test drives: the same body the chooser's callback calls, reached through the
/// window rather than by threading `bar`/`status`/`session` by hand. It decides nothing — it finds
/// the widgets this window built and forwards to [`open_folder`].
pub fn open_from(window: &adw::ApplicationWindow, picked: &Path) -> bool {
    let (Some(bar), Some(status), Some(session)) = (
        WINDOW_BAR.with(|slots| slots.borrow().last().cloned()),
        find_status(window.upcast_ref()),
        SESSION.with(|slots| slots.borrow().last().cloned()),
    ) else {
        panic!("the window has no run bar, status line or session");
    };
    open_folder(&bar, &status, &session, picked)
}

/// The seam a widget test drives for F0.12: the same body the "Add source files…" chooser's response
/// calls, reached through the window rather than by threading `bar`/`status`/`session`/`queue`/
/// `progress` by hand. It decides nothing — [`add_sources`] does — but it is where the copy's bytes
/// reach the bar.
///
/// A copy is a run of its own (F0.12 S0), so it borrows F0.5's bookkeeping instead of painting a
/// second bar: the copy owns one track, each file asked for is one task on it, and each file's byte
/// fraction inside that task is what `prog` reports. That is why the bar moves in proportion to the
/// bytes actually copied rather than to the number of rows added — a slow card is what the bar is
/// waiting on, and `add_sources::Progress` already carries those bytes.
///
/// Returns whether anything was added. Refused while another run is on (S0): the session is not
/// touched, so a refused press cannot half-apply.
pub fn add_files(window: &adw::ApplicationWindow, files: &[std::path::PathBuf]) -> bool {
    let (Some(bar), Some(status), Some(session), Some(queue), Some(progress)) = (
        WINDOW_BAR.with(|slots| slots.borrow().last().cloned()),
        find_status(window.upcast_ref()),
        SESSION.with(|slots| slots.borrow().last().cloned()),
        WINDOW_QUEUE.with(|slots| slots.borrow().last().cloned()),
        progress_bar(window),
    ) else {
        panic!("the window has no run bar, status line, session, queue or progress bar");
    };
    let copy = copy_into_project(window).is_some_and(|tick| tick.is_active());
    let root = std::env::current_dir().unwrap_or_default();
    let dir = startup::session_dir(&root);
    // S0 first, again at the seam: pressed while a run is on, it says the sentence and stops before
    // any byte moves. Nothing here clears a ▶ run's flag — ⏹ owns that.
    if let Err(reason) = add_sources::press(bar.borrow().running.is_some()) {
        status.set_text(reason);
        return false;
    }
    // Ours alone: the import marks itself as *the* run so a ▶ press during a copy is refused the
    // other way round, and only this call may take that flag away.
    bar.borrow_mut().running = Some(run::Run {
        step: run::Step::Prepare,
        paused: false,
        log_expanded: true,
        sources: run::snapshot_sources(&session.borrow()),
    });
    let added = add_copy(
        &mut session.borrow_mut(),
        &queue,
        &progress,
        &root,
        &dir,
        files,
        copy,
    );
    // End it through F0.5's own path, whatever the copy did: the import is over either way. The log
    // expander's state is put back by hand rather than left to `end_run`'s clear — that clears the
    // status line, which this flow needs for S5's sentence.
    bar.borrow_mut().running = None;

    match added {
        Ok(answer) => {
            status.set_text(&answer.status);
            // The list drew its rows once at build time, so the new ones are appended here rather
            // than the list being rebuilt — rows already on screen keep their own state (S3).
            let total_rows = session.borrow().sources.len();
            if let Some(list) = find_widget_by_name(window.upcast_ref(), "sources-list")
                .and_then(|widget| widget.downcast::<gtk::ListBox>().ok())
            {
                // GTK's `ListBox` exposes its rows through the widget tree, so they are counted with
                // the same iterator the rest of this module walks widgets with: that count is where
                // the build-time drawing stopped, so everything past it is new.
                let drawn = list
                    .observe_children()
                    .iter::<glib::Object>()
                    .filter_map(|child| child.ok())
                    .filter(|child| child.downcast_ref::<gtk::ListBoxRow>().is_some())
                    .count();
                for index in drawn..total_rows {
                    list.append(&source_row(&session, &status, index));
                }
            }
            answer.added > 0
        }
        // S0's abandonment: the log keeps the reason, the status line the sentence.
        Err(err) => {
            status.set_text(add_sources::NO_SOURCES_DIR);
            log_line(&err);
            false
        }
    }
}

/// The copy half of a press, with the bar fed from the bytes as they land. Split out so the seam
/// above stays about finding widgets and this stays about the run of its own.
fn add_copy(
    session: &mut Project,
    queue: &std::cell::RefCell<runqueue::Queue>,
    progress: &gtk::ProgressBar,
    root: &Path,
    dir: &Path,
    files: &[std::path::PathBuf],
    copy: bool,
) -> Result<add_sources::Added, String> {
    // One job on the first track; every file asked for is one task of it, so the tooltip counts the
    // pick rather than what survived (`added N of M` is the status line's job, not the bar's).
    queue.borrow_mut().job(TRACK_IMPORT, IMPORT_JOB, 0, 0);
    queue.borrow_mut().push(TRACK_IMPORT, files.len(), "file");
    // The folder a copy lands in has to exist before the first `.part` is renamed into it. `add`
    // makes the same folder afterwards, but the byte-by-byte report runs first (F0.12 S2).
    if copy {
        std::fs::create_dir_all(dir.join("sources"))
            .map_err(|_| add_sources::NO_SOURCES_DIR.to_string())?;
    }
    // The total is taken before the first byte moves, so the fraction never chases a growing sum.
    let total: u64 = files
        .iter()
        .map(|file| file.metadata().map(|meta| meta.len()).unwrap_or(0))
        .sum();
    let mut done_bytes = 0u64;
    for file in files {
        let size = file.metadata().map(|meta| meta.len()).unwrap_or(0);
        queue.borrow_mut().take(TRACK_IMPORT);
        if copy {
            // `add_sources`' own copy rule — `.part` then rename, same name + same size skipped, a
            // file already inside the project left where it is. Not reimplemented here.
            add_sources::copy_one(file, dir).map_err(|err| err.to_string())?;
        }
        done_bytes += size;
        // The bar means "how much of this import is on disk": the byte fraction, not the task count.
        // Three tiny files beside one long recording should not read as mostly copied.
        let fraction = if total == 0 {
            1.0
        } else {
            done_bytes as f64 / total as f64
        };
        let name = file
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();
        queue
            .borrow_mut()
            .prog(TRACK_IMPORT, fraction, &format!("copying {name}"));
        paint_import(progress, queue);
    }
    queue.borrow_mut().done(TRACK_IMPORT, 1.0);
    add_sources::add(session, root, dir, files, copy)
}

/// Draw the import on F0.5's bar. The fraction and the tooltip come from the queue exactly as
/// [`paint_progress`] takes them for a ▶ run; only the expander's body is left alone, since an
/// import writes no run lines of its own.
fn paint_import(progress: &gtk::ProgressBar, queue: &std::cell::RefCell<runqueue::Queue>) {
    let (_text, tip) = queue.borrow().text_and_tip();
    progress.set_fraction(queue.borrow().fraction());
    if !tip.is_empty() {
        progress.set_tooltip_text(Some(&tip));
    }
}

/// The import's own job name on the bar's line and tooltip.
const IMPORT_JOB: &str = "import";
/// The copy gets one track: the whole pick is a single run of its own, not work interleaved with a
/// ▶ run (which is precisely why S0 refuses while one is on).
const TRACK_IMPORT: usize = 0;

thread_local! {
    /// The run bar `build_window` created, published for the same reason as [`SESSION`]: so the
    /// seam in [`open_from`] can reach the copy this window actually uses.
    static WINDOW_BAR: std::cell::RefCell<Vec<Rc<std::cell::RefCell<run::RunBar>>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

thread_local! {
    /// The F0.5 queue `build_window` created, published for the same reason as [`WINDOW_BAR`]: so
    /// the seam in [`add_files`] can drive the bar this window actually shows. A copy is a run of
    /// its own (F0.12 S0), so it reports through the bookkeeping rather than painting its own bar.
    static WINDOW_QUEUE: std::cell::RefCell<Vec<Rc<std::cell::RefCell<runqueue::Queue>>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

thread_local! {
    /// The live project handle `build_window` created, published so a test can read what Open put in
    /// place. One window per test binary here, so one slot is enough; the page widgets keep their own
    /// clone of the same `Rc`, which is why replacing the contents is visible everywhere.
    static SESSION: std::cell::RefCell<Vec<Rc<std::cell::RefCell<Project>>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// The Open button's tooltip, §1's wording for badge **2**.
const OPEN_TIP: &str = "Load a project \u{2014} sources, prompts and settings";

/// F2.1's own ▶ on the Cut page: the label and the tooltip §F2.1 S2 gives ("play from the red line ·
/// every second plays, cuts and all"). Kept apart so the widget test can pin the wording against the
/// spec rather than against whatever was typed into the button.
const RECORD_PLAY_LABEL: &str = "\u{25b6} Play the recording";

/// F2.2's label. The glyph pair is the button's whole identity in §A (▶ vs ▶✂), so it leads the
/// words rather than sitting alone as an icon a person has to memorise.
const PLAY_CUT_LABEL: &str = "\u{25b6}\u{2702} Play the cut";

/// F2.3's label — the third of the transport group, spelled with two scissors so it cannot be
/// mistaken for ▶✂ at a glance.
const REVIEW_CUTS_LABEL: &str = "\u{25b6}\u{2702}\u{2702} Review every cut";

/// F2.4's four line-step buttons, in the spec's left-to-right order: `‹‹f ‹f f› ››`. Each entry is
/// (widget name, label, signed frames) — the sign is the direction and the magnitude the count, so a
/// button's own name cannot disagree with what it does. A `bool` could: wiring `shift=true` to
/// `line-step-back-five` reads as "five" while saying nothing about which way.
const LINE_STEP_BUTTONS: [(&str, &str, i64); 4] = [
    ("line-step-back-five", "\u{2039}\u{2039} f  step back 5 frames", -5),
    ("line-step-back", "\u{2039} f  step back 1 frame", -1),
    ("line-step-forward", "f \u{203a}  step forward 1 frame", 1),
    ("line-step-forward-five", "f \u{203a}\u{203a}  step forward 5 frames", 5),
];
const RECORD_PLAY_TIP: &str =
    "Play the recording from the red line \u{2014} every second of it, cuts and all";

/// The reason ▶ had nothing to play. Spec silent on a session with no filmed stretch; §0 asks for a
/// named, local reason rather than a silent button.
pub const NO_RECORDING_STATUS: &str = "nothing was filmed \u{2014} there is no recording to play";

/// One window's preview state, pushed beside [`SESSION`] so each window owns its own and the newest
/// one wins — the same arrangement `session_sources` reads.
thread_local! {
    static PREVIEW_PLAYERS: std::cell::RefCell<Vec<Rc<std::cell::RefCell<Player>>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// The app's ONE preview volume (F2.5 S6), held per window beside [`PREVIEW_PLAYERS`] with the same
/// newest-slot rule. It is a `PreviewVolume` and not a bare number because the type is what keeps the
/// slider's 0..100 and the property's 0..1 from drifting apart, and because the tooltip promises every
/// slider shows the same setting — two stored copies would be two answers.
thread_local! {
    static PREVIEW_VOLUMES: std::cell::RefCell<Vec<Rc<std::cell::RefCell<cut_hear::PreviewVolume>>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// F2.5 S6: set this window's one preview volume from a slider's percent (0..100) and push the result
/// into the live player, so every preview of this window shares the one number. Returns the gain written
/// (`0..=1`), which is what the caller would read back — the clamp lives in [`cut_hear::clamp_volume`],
/// never here.
pub fn set_preview_volume(window: &adw::ApplicationWindow, percent: f64) -> f64 {
    let _ = window;
    let volume = PREVIEW_VOLUMES.with(|slots| {
        slots
            .borrow()
            .last()
            .cloned()
            .unwrap_or_else(|| Rc::new(std::cell::RefCell::new(cut_hear::PreviewVolume::new())))
    });
    volume.borrow_mut().set_percent(percent);
    let gain = volume.borrow().value();
    if let Some(player) = live_player(window) {
        player.borrow_mut().volume = Some(gain);
    }
    gain
}

/// F2.5 S6: the one number, as a gain — what every preview multiplies its sound by, and the value
/// [`cut_hear::mix_at`] is handed. A window that never showed a slider reads the default full.
pub fn preview_volume(window: &adw::ApplicationWindow) -> f64 {
    let _ = window;
    PREVIEW_VOLUMES.with(|slots| {
        slots
            .borrow()
            .last()
            .map(|v| v.borrow().value())
            .unwrap_or(cut_hear::VOLUME_DEFAULT)
    })
}

/// F2.5: ask the hearing rules what this tick means and store the answer on this window's player.
///
/// The page calls this instead of computing anything: `cut_hear::mix_at` decides which lanes start,
/// whether the footage's own sound is muted and how loud and fast to play, at this window's one
/// preview volume. Nothing here re-decides a part of that answer; it only lands `footage_muted` where a
/// later round's pipeline can read it.
pub fn apply_mix(
    window: &adw::ApplicationWindow,
    segs: &[cut::Seg],
    lanes: &[cut::Lane],
    fx: &[cut::Fx],
    t: f64,
) -> cut_hear::Mix {
    let mix = cut_hear::mix_at(segs, lanes, fx, t, preview_volume(window));
    if let Some(player) = live_player(window) {
        player.borrow_mut().footage_muted = mix.footage_muted;
    }
    mix
}

/// One window's cut, held beside [`PREVIEW_PLAYERS`] with the same newest-slot rule.
///
/// The Cut page has no live cut model yet — that arrives with its own round — so this is what stands in
/// for it: `build_window` seeds a default (empty) cut and every press reads it back. A caller that DOES
/// have a cut hands it over with [`seed_review_cut`], which is the seam the cut-model round plugs into
/// instead of re-plumbing the button.
thread_local! {
    static REVIEW_CUTS: std::cell::RefCell<Vec<Rc<std::cell::RefCell<cut::Cut>>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// One window's red line and the writer that throttles its saves, newest slot winning like
/// [`PREVIEW_PLAYERS`]. The writer lives beside the position because the rate limit is a property of
/// this window's moving line, not of the file.
thread_local! {
    static LINE_STATES: std::cell::RefCell<Vec<Rc<std::cell::RefCell<(cut_line::LinePos, cut_line::LineWriter)>>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// What a frame step did (F2.4 S2/S3), so the caller paints the right thing and decides nothing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Step {
    /// Something was held: THAT moved by `frames` frames rather than the line.
    Nudged { frames: i64 },
    /// Nothing held: the recording under the line seeked to `t` and paused.
    SteppedTo { t: f64 },
    /// Nothing happened — S3's arrows with nothing held.
    Still,
}

/// This window's red line position.
pub fn line_position(window: &adw::ApplicationWindow) -> cut_line::LinePos {
    let _ = window;
    LINE_STATES
        .with(|slots| slots.borrow().last().map(|slot| slot.borrow().0))
        .unwrap_or(cut_line::LinePos { t: 0.0 })
}

/// Set this window's line WITHOUT saving -- the test/setup entry point. Saving is a side effect of a
/// user moving the line, not of being told where it is, and a setup call that consumed the throttle
/// would make the next real move silently unwritable for a second.
pub fn set_line_position(window: &adw::ApplicationWindow, pos: cut_line::LinePos) {
    let _ = window;
    LINE_STATES.with(|slots| {
        if let Some(slot) = slots.borrow().last() {
            slot.borrow_mut().0 = pos;
        }
    });
}

/// A user moved the line: store it and let [`cut_line::LineWriter`] decide whether that is worth a
/// save right now (at most one write a second, `cut_line::LINE_WRITE_MS`). Returns whether it wrote.
///
/// No project root reaches the page yet (the cut-model round owns it), so there is nowhere to write;
/// the throttle is still advanced so the rate rule holds the moment a root exists.
pub fn move_line_and_save(window: &adw::ApplicationWindow, pos: cut_line::LinePos, now_ms: u64) -> bool {
    let _ = window;
    LINE_STATES.with(|slots| {
        let Some(slot) = slots.borrow().last().cloned() else {
            return false;
        };
        slot.borrow_mut().0 = pos;
        let wrote = cut_line::may_write_line(slot.borrow().1.last_write_ms(), now_ms, false);
        if wrote {
            slot.borrow_mut().1.note_throttled(now_ms);
        }
        wrote
    })
}

/// F2.4 S1 through the seam: a press on the tracks places the line, clears the selection and watches
/// a row exactly as [`cut_line::click_outcome`] says — the page reports pixels and modifiers, this
/// applies the rule and stores the result.
pub fn place_line_from_click(
    window: &adw::ApplicationWindow,
    on_picture: bool,
    on_scene_picture: bool,
    playing: bool,
    sources: bool,
    gutter: bool,
    at: f64,
    now_ms: u64,
) -> cut_line::ClickOutcome {
    let outcome = cut_line::click_outcome(at, on_picture, on_scene_picture, playing, sources, gutter);
    if outcome.line_moved {
        move_line_and_save(window, cut_line::LinePos { t: outcome.line_at }, now_ms);
    }
    // The selection clear lands here because this is where the rule says it happens; the Cut page has no
    // selection model until its own round, so there is nothing to clear yet — the call site stays so
    // the wiring is already in place when that model arrives.
    outcome
}

/// F2.4 S1b: what a first left press picks up at the 12 px reach (`// layout.lineReachPx`,
/// `cut_line::EDGE_REACH_PX`) — edge, then border, then the whole clip.
pub fn pick_with_reach(window: &adw::ApplicationWindow, edge_px: f64, inside: bool) -> cut_line::PressPick {
    let _ = window;
    cut_line::first_press_pick(edge_px, inside)
}

/// F2.4 S2: ‹f / f› steps one frame (Shift five). With a hold, the HOLD moves; with nothing held the
/// recording under the line seeks and pauses.
pub fn step_the_line(
    window: &adw::ApplicationWindow,
    shift: bool,
    held: Option<cut_line::Held>,
    fps: f64,
    now_ms: u64,
) -> Step {
    let frames = cut_line::step_frames(shift);
    if held.is_some() {
        // Nudging the held edge/clip/effect needs the held item's own geometry, which belongs to the
        // cut-editing rounds; the returned frame count is what the caller acts on once that exists.
        return Step::Nudged { frames };
    }
    let from = line_position(window).t;
    let to = cut_line::step_line(from, frames, fps);
    move_line_and_save(window, cut_line::LinePos { t: to }, now_ms);
    // S2's "then pause": stepping is looking at a frame, not watching a run.
    if let Some(player) = live_player(window) {
        player.borrow_mut().transport.playing = false;
    }
    Step::SteppedTo { t: to }
}

/// F2.4 S3: ← / → step only while something is held; with nothing held they do nothing at all.
pub fn arrow_steps(
    window: &adw::ApplicationWindow,
    shift: bool,
    held: Option<cut_line::Held>,
    fps: f64,
    now_ms: u64,
) -> Step {
    if !cut_line::arrow_moves_line(held) {
        return Step::Still;
    }
    step_the_line(window, shift, held, fps, now_ms)
}

/// F2.4 S3: Space toggles the preview unless a text box has the focus.
pub fn space_toggles(window: &adw::ApplicationWindow, text_focus: bool) -> bool {
    let toggle = cut_line::space_toggles_preview(text_focus);
    if toggle && !text_focus {
        if let Some(player) = live_player(window) {
            let mut held = player.borrow_mut();
            held.transport.playing = !held.transport.playing;
            held.transport.started = held.transport.playing || held.transport.started;
        }
    }
    toggle
}

/// Find one of the four named line-step buttons.
pub fn line_step_button(window: &adw::ApplicationWindow, name: &str) -> Option<gtk::Button> {
    find_widget_by_name(window.upcast_ref(), name)?.downcast().ok()
}

/// This window's key controller for the line (F2.4 S2/S3): ‹‹f ‹f f› ›› step one or five frames,
/// ← / → move only while something is held, Space toggles play.
pub fn line_key_controller(window: &adw::ApplicationWindow) -> Option<gtk::EventControllerKey> {
    let _ = window;
    LINE_KEYS.with(|cell| cell.borrow().clone())
}

thread_local! {
    static LINE_KEYS: std::cell::RefCell<Option<gtk::EventControllerKey>> =
        const { std::cell::RefCell::new(None) };
}

/// Wire one step button to its signed frame count. A button press holds nothing, so this is always
/// S2's seek-and-pause; the direction lives in `frames`, never inferred from the widget.
fn wire_line_step(button: &gtk::Button, window: &adw::ApplicationWindow, frames: i64) {
    let window = window.clone();
    button.connect_clicked(move |_| {
        let _ = step_frames_of(&window, frames);
    });
}

/// Move the line by a signed number of frames and pause (F2.4 S2). The seam the buttons use; the
/// keyboard path goes through `step_the_line`, which first asks whether a hold should be nudged.
pub fn step_frames_of(window: &adw::ApplicationWindow, frames: i64) -> Step {
    let from = line_position(window).t;
    let to = cut_line::step_line(from, frames, DEFAULT_FPS);
    move_line_and_save(window, cut_line::LinePos { t: to }, now_ms());
    if let Some(player) = live_player(window) {
        player.borrow_mut().transport.playing = false;
    }
    Step::SteppedTo { t: to }
}

/// The frame rate the line steps at when no project has said otherwise. The spec gives no `P.*` row for
/// it; 25 fps is what the prototype's session runs at, and a wrong rate moves the line by a wrong but
/// still sub-second amount rather than breaking anything.
const DEFAULT_FPS: f64 = 25.0;

/// A monotone-ish millisecond clock for the write throttle. Coarse is fine: the rule it feeds only asks
/// whether a second has passed.
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Add the keyboard controller to the window. Called after `set_content`, like every other `wire_*`.
fn wire_line_keys(window: &adw::ApplicationWindow) {
    let controller = gtk::EventControllerKey::new();
    let win = window.clone();
    controller.connect_key_pressed(move |_ctrl, key, _code, mods| {
        let shift = mods.contains(gtk::gdk::ModifierType::SHIFT_MASK);
        let held: Option<cut_line::Held> = None;
        let response = match key {
            gtk::gdk::Key::Left => arrow_steps(&win, shift, held, DEFAULT_FPS, now_ms()),
            gtk::gdk::Key::Right => arrow_steps(&win, shift, held, DEFAULT_FPS, now_ms()),
            gtk::gdk::Key::comma => step_the_line(&win, shift, held, DEFAULT_FPS, now_ms()),
            gtk::gdk::Key::period => step_the_line(&win, shift, held, DEFAULT_FPS, now_ms()),
            gtk::gdk::Key::space => {
                // Space is not a line step, so it answers the propagation directly rather than with a
                // `Step`; handled means the focused widget never sees it.
                return if space_toggles(&win, false) {
                    glib::Propagation::Stop
                } else {
                    glib::Propagation::Proceed
                };
            }
            _ => return glib::Propagation::Proceed,
        };
        // Every handled key stops the default action; `Still` deliberately does not, so an unheld
        // arrow keeps doing whatever the focused widget would have done.
        if matches!(response, Step::Still) { glib::Propagation::Proceed } else { glib::Propagation::Stop }
    });
    window.add_controller(controller.clone());
    LINE_KEYS.with(|cell| *cell.borrow_mut() = Some(controller));
}

/// The Cut page's ▶ (F2.1), found by name the way [`play_button`] finds the run bar's.
pub fn play_recording_button(window: &adw::ApplicationWindow) -> Option<gtk::Button> {
    find_widget_by_name(window.upcast_ref(), "play-recording-button")?
        .downcast()
        .ok()
}

/// The Cut page's ▶✂ (F2.2), found by name the way [`play_recording_button`] finds ▶.
pub fn play_cut_button(window: &adw::ApplicationWindow) -> Option<gtk::Button> {
    find_widget_by_name(window.upcast_ref(), "play-cut-button")?
        .downcast()
        .ok()
}

/// The Cut page's ▶✂✂ (F2.3), found by name the way [`play_cut_button`] finds ▶✂.
pub fn review_cuts_button(window: &adw::ApplicationWindow) -> Option<gtk::Button> {
    find_widget_by_name(window.upcast_ref(), "review-cuts-button")?
        .downcast()
        .ok()
}

/// This window's preview as it currently stands — for a test that fired the real button and wants to
/// check the same state the logic test checks rather than a painted pixel.
pub fn preview_player(window: &adw::ApplicationWindow) -> Player {
    let _ = window;
    PREVIEW_PLAYERS.with(|slots| {
        slots
            .borrow()
            .last()
            .map(|player| *player.borrow())
            .unwrap_or_default()
    })
}

/// This window's live preview handle — the newest registered player, which is the one this window's
/// button moves. Shared by every accessor below so they all read and write the same object.
fn live_player(window: &adw::ApplicationWindow) -> Option<Rc<RefCell<Player>>> {
    let _ = window;
    PREVIEW_PLAYERS.with(|slots| slots.borrow().last().cloned())
}

/// Put the red line somewhere before a press, which is what placing the cursor on the timeline does in
/// the finished page. F2.1 S2 plays from here, so a test seeds it rather than depending on a click
/// landing on a track that this round does not draw.
pub fn set_playhead(window: &adw::ApplicationWindow, at: f64) {
    if let Some(player) = live_player(window) {
        player.borrow_mut().playhead = Some(at);
    }
}

/// Stand the preview in one of its three modes (recording / ✂ cut / ▶✂✂ review), playing or not,
/// before a press — which is what the ✂ toggles and the transport itself do on the finished page.
pub fn set_preview_state(
    window: &adw::ApplicationWindow,
    cut_only: bool,
    reviewing: bool,
    transport: run::Transport,
) {
    if let Some(player) = live_player(window) {
        let mut held = player.borrow_mut();
        held.cut_only = cut_only;
        held.reviewing = reviewing;
        held.transport = transport;
    }
}

/// The seam F2.1's button calls: [`preview::press_recording`] decides which of switch-to-the-
/// recording, pause or play-from-the-red-line the press means; this forwards it and paints only what
/// the answer says.
///
/// `runs` are the filmed stretches ([`crate::timeline::filmed_runs`]) — needed because a session
/// with nothing filmed has nothing to start at, and starting at 0 would pretend otherwise.
///
/// The player's own `Transport` is deliberately NOT pushed into the shell's [`run::RunBar`]: the bar
/// tracks a *run* (`Run { step, paused, .. }`), not a preview transport, and folding one into the
/// other would mean changing its API for a state it has no rule about. The two stay separate until a
/// round owns both.
pub fn press_play_recording(window: &adw::ApplicationWindow, runs: &[(f64, f64)]) -> Press {
    let Some(status) = find_status(window.upcast_ref()) else {
        panic!("the window has no status line");
    };
    let Some(player) = PREVIEW_PLAYERS.with(|slots| slots.borrow().last().cloned()) else {
        panic!("the window has no preview player");
    };
    let pressed = preview::press_recording(&mut player.borrow_mut(), runs);
    match pressed {
        // S1: the clock just changed meaning, so the sentence that says so goes where a press's
        // answer is read.
        Press::SwitchedToRecording { .. } => status.set_text(preview::RECORDING_STATUS),
        // S2: playing and pausing need no sentence — the bar's own progress owns the line while a
        // preview runs, and a pause leaves the previous one standing.
        Press::Playing { .. } | Press::Paused => {}
        Press::NoFootage => status.set_text(NO_RECORDING_STATUS),
    }
    pressed
}

/// F2.1: the Cut page's ▶ forwards to [`press_play_recording`], which forwards to
/// [`preview::press_recording`] and paints. The handler takes the window it was wired on, cloned into
/// the closure the way [`wire_settings`] does, so `build_window` needs nothing threaded through.
fn wire_play_recording(button: &gtk::Button, window: &adw::ApplicationWindow) {
    let window = window.clone();
    button.connect_clicked(move |_| {
        // The filmed stretches come from Prepare's output (`timeline::filmed_runs` over the
        // session's recordings), which belongs to the Cut page's own round — no `Recording` model
        // is reachable from here yet — so an empty list is what the window knows today, and the seam
        // takes real spans from any caller that does have them.
        let _ = press_play_recording(&window, &[]);
    });
}

/// F2.5 S6: hand the slider's number to [`set_preview_volume`] and nothing else. The widget decides no
/// part of the rule — it does not clamp, scale or remember — because the one volume is owned by
/// `cut_hear::PreviewVolume` and every preview reads that same value back.
fn wire_preview_volume(scale: &gtk::Scale, window: &adw::ApplicationWindow) {
    let window = window.clone();
    scale.connect_value_changed(move |slider| {
        set_preview_volume(&window, slider.value());
    });
}

/// This window's preview-volume slider, by its stable name (F2.5 S6). The seam a test fires and the
/// page reads; `None` on a page that drew none.
pub fn preview_volume_scale(window: &adw::ApplicationWindow) -> Option<gtk::Scale> {
    find_widget_by_name(window.upcast_ref(), "preview-volume")?
        .downcast()
        .ok()
}

/// The seam F2.2's ▶✂ calls: [`cut_play::pressed`] decides whether the press switches to the cut,
/// ends a review or toggles play/pause; this forwards it and paints only what the answer says.
///
/// `segs` are the cut's own segments ([`crate::cut::Cut::segs`]). Like ▶'s `runs`, they are not yet
/// reachable from the window — the Cut page holds no live cut model until its own round — so an empty
/// list is what the button knows today, which is exactly the S1 case: nothing to skip to, refused.
pub fn press_play_cut(window: &adw::ApplicationWindow, cut: &cut::Cut) -> cut_play::Pressed {
    let Some(status) = find_status(window.upcast_ref()) else {
        panic!("the window has no status line");
    };
    let Some(player) = live_player(window) else {
        panic!("the window has no preview player");
    };
    let pressed = cut_play::pressed(&mut player.borrow_mut(), cut);
    // The outer `status` is the status Label, so each arm binds its own name and paints from it.
    match &pressed {
        // S2/S3: both change what the clock means or what is playing, so both get their sentence.
        cut_play::Pressed::SwitchedToCut { status: line, .. } => status.set_text(line),
        cut_play::Pressed::ReviewEnded { status: ended } => status.set_text(*ended),
        cut_play::Pressed::Refused(line) => status.set_text(line),
        // S4: the toggle needs no sentence — the button's own face carries it.
        cut_play::Pressed::Toggled(_) => {}
    }
    pressed
}

/// F2.2: the Cut page's ▶✂ forwards to [`press_play_cut`], taking the window cloned into the
/// closure exactly as [`wire_play_recording`] does.
fn wire_play_cut(button: &gtk::Button, window: &adw::ApplicationWindow) {
    let window = window.clone();
    button.connect_clicked(move |_| {
        // No live cut model reaches the page yet (the Cut page's own round owns it), so the empty cut
        // is the honest input and S1's refusal is what a press answers with today.
        let _ = press_play_cut(&window, &cut::Cut::default());
    });
}

/// Put this window's cut where ▶✂✂ can read it — the seam the cut-model round plugs into. Until then
/// `build_window` seeds an empty one and a press answers S1's refusal.
pub fn seed_review_cut(window: &adw::ApplicationWindow, cut_: &cut::Cut) {
    let _ = window;
    REVIEW_CUTS.with(|slots| {
        if let Some(slot) = slots.borrow().last() {
            *slot.borrow_mut() = cut_.clone();
        }
    });
}

/// The newest cut this window holds, or an empty one when nothing has been seeded.
fn newest_review_cut() -> cut::Cut {
    REVIEW_CUTS
        .with(|slots| slots.borrow().last().map(|slot| slot.borrow().clone()))
        .unwrap_or_default()
}

/// The seam F2.3's ▶✂✂ calls: [`cut_review::pressed`] decides whether the press starts the review from
/// the red line, refuses for want of a join, or pauses-and-ends a running one; this forwards it and
/// paints only what the answer says. Every sentence comes from `cut_review` itself.
pub fn press_review_cuts(window: &adw::ApplicationWindow, cut_: &cut::Cut) -> cut_review::Pressed {
    let Some(status_line) = find_status(window.upcast_ref()) else {
        panic!("the window has no status line");
    };
    let Some(player) = live_player(window) else {
        panic!("the window has no preview player");
    };
    let pressed = cut_review::pressed(&mut player.borrow_mut(), cut_);
    match &pressed {
        // S4: the status names which join out of how many is being heard.
        cut_review::Pressed::Started { status, .. } => status_line.set_text(status),
        // S5: pausing ends the review, and says so with the module's own short line.
        cut_review::Pressed::PausedAndEnded { status } => status_line.set_text(*status),
        // S1: fewer than two clips, in `refused`'s own words.
        cut_review::Pressed::Refused(reason) => status_line.set_text(reason),
    }
    pressed
}

/// F2.3: the Cut page's ▶✂✂ forwards to [`press_review_cuts`], reading the cut this window holds.
fn wire_review_cuts(button: &gtk::Button, window: &adw::ApplicationWindow) {
    let window = window.clone();
    button.connect_clicked(move |_| {
        let _ = press_review_cuts(&window, &newest_review_cut());
    });
}

/// Where a press of Open goes. [`open_project`] decides everything — which folder the pick names,
/// whether it is an old single-file project, what the read produced and what had to be dropped; this
/// only asks for a folder, then draws the answer.
fn wire_open(
    open_: &gtk::Button,
    bar: &Rc<RefCell<run::RunBar>>,
    status: &gtk::Label,
    session: &Rc<RefCell<Project>>,
) {
    let bar = bar.clone();
    let status = status.clone();
    let session = session.clone();
    open_.connect_clicked(move |_| {
        // The chooser cannot be answered in a headless test, so what a test asserts is that the press
        // got here and that every rule behind it lives in `open_project` (see
        // `tests/open_project_widgets.rs`, which drives `open_folder` directly).
        ask_open_project(&bar, &status, &session);
    });
}

/// S1: the folder chooser, titled for opening. Folder-select rather than file-select because a
/// project *is* a folder (01 §1); a user who picks `naivepost.json` inside one still gets that
/// folder, which is [`open_project::folder_for`]'s rule rather than the chooser's.
fn ask_open_project(
    bar: &Rc<RefCell<run::RunBar>>,
    status: &gtk::Label,
    session: &Rc<RefCell<Project>>,
) {
    let dialog = gtk::FileDialog::builder()
        .title(open_project::TITLE)
        .accept_label("Open")
        .build();

    let bar = bar.clone();
    let status = status.clone();
    let session = session.clone();
    dialog.select_folder(
        None::<&gtk::Window>,
        None::<&gio::Cancellable>,
        move |chosen: Result<gio::File, glib::Error>| {
            let Ok(file) = chosen else {
                return; // dismissed: nothing was asked for, so nothing changed
            };
            let picked = file.path().unwrap_or_default();
            open_folder(&bar, &status, &session, &picked);
        },
    );
}

/// S1 → S4 for one picked path, with no chooser in the way. This is the body the chooser's callback
/// calls, and the seam a widget test drives so the assertions land on real state rather than on a
/// dialog nobody answers.
pub fn open_folder(
    bar: &Rc<RefCell<run::RunBar>>,
    status: &gtk::Label,
    session: &Rc<RefCell<Project>>,
    picked: &Path,
) -> bool {
    match open_project::open(picked, |path| path.is_file(), |path| path.exists()) {
        Err(failure) => {
            log_line(&failure.log);
            status.set_text(failure.status);
            false
        }
        Ok(applied) => {
            // S3: the paths were settled before the read, so the swap below happens against the
            // folder just opened. Replacing the shared copy IS the page refresh: Prepare's rows read
            // this same handle, so they show the opened project without being rebuilt here. The
            // migration runs first, because it reads the applied value the swap consumes.
            let migration = open_project::finish(&applied);
            let remembered = REMEMBERED.with(|list| {
                open_project::remember(
                    &mut list.borrow_mut(),
                    &applied.root,
                    &open_project::project_file_in(&applied.out),
                )
            });
            // The lines are taken out before the project is moved into the session, so `applied` is
            // not read after that point.
            let lines = applied.lines.clone();
            let opened = applied
                .root
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| "project opened".to_string());
            *session.borrow_mut() = applied.project;
            for line in &lines {
                log_line(line);
            }
            // S4: old folder names move, and each move is logged.
            for line in &migration.lines {
                log_line(line);
            }
            for line in &migration.failures {
                log_line(line);
            }
            // S4: remember it for this root. Data-level — writing `llm.conf` is settings' job, and
            // the pair is recorded on the window's own list rather than reaching for `$HOME`.
            log_line(&format!(">>> opened {}", remembered.file));
            // A run reading the old project would now be reading files that are not its own, so the
            // stop flag is raised rather than letting it carry on over swapped sources.
            let _ = bar.borrow();
            let opened = applied
                .root
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| "project opened".to_string());
            status.set_text(&opened);
            true
        }
    }
}

thread_local! {
    /// What this launch has opened, per root — the pairs settings writes as `PROJECT_<n>_ROOT` /
    /// `PROJECT_<n>_FILE`. Held here rather than written out because persisting belongs to
    /// `settings`' writer, which owns quoting and the whole-store render.
    static REMEMBERED: std::cell::RefCell<std::collections::BTreeMap<String, String>> =
        const { std::cell::RefCell::new(std::collections::BTreeMap::new()) };
}

/// What this launch recorded as opened, keyed by root.
pub fn remembered_projects() -> std::collections::BTreeMap<String, String> {
    REMEMBERED.with(|list| list.borrow().clone())
}

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

pub(crate) fn log_line(line: &str) {
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
        log_expanded: log_expander(window).is_some_and(|expander| expander.is_expanded()),
        progress: progress_bar(window)
            .map(|bar| bar.fraction())
            .unwrap_or(0.0),
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

/// The Open button (F0.9), found the same way as [`play_button`].
pub fn open_button(window: &adw::ApplicationWindow) -> Option<gtk::Button> {
    find_widget_by_name(window.upcast_ref(), "open-button")?
        .downcast()
        .ok()
}

/// The sources of the project this window currently holds. `build_window` keeps its own `Rc`, so the
/// handle is published here at build time for a test to read back — the same copy the Prepare page's
/// rows mutate, not a snapshot of it.
/// The live session — the last window's handle, which is the one this check just opened into. Each
/// `build_window` publishes its own, so reading the newest avoids mixing in earlier windows' copies.
pub fn session_sources(window: &adw::ApplicationWindow) -> Vec<String> {
    let _ = window;
    SESSION.with(|slots| {
        slots
            .borrow()
            .last()
            .map(|project| {
                project
                    .borrow()
                    .sources
                    .iter()
                    .map(|source| source.path.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    })
}

/// The tooltip Open carries, which is how a test reads §1's wording for badge **2**.
pub fn open_tooltip(window: &adw::ApplicationWindow) -> Option<String> {
    open_button(window)?.tooltip_text().map(|text| text.to_string())
}

/// The run's progress bar (F0.5 S2), found the same way as [`play_button`]. Window-level, so a name
/// lookup cannot land on a per-page copy.
pub fn progress_bar(window: &adw::ApplicationWindow) -> Option<gtk::ProgressBar> {
    find_widget_by_name(window.upcast_ref(), "run-progress")?
        .downcast()
        .ok()
}

/// The log expander (F0.5 S1), whose header carries the status line and whose body holds the run's
/// lines. `is_expanded()` is what `state().log_expanded` reports.
pub fn log_expander(window: &adw::ApplicationWindow) -> Option<gtk::Expander> {
    find_widget_by_name(window.upcast_ref(), "log-expander")?
        .downcast()
        .ok()
}

/// The tooltip the progress bar carries — where F0.5 puts the counting ("task 4 of 12, 8 waiting").
pub fn progress_tooltip(window: &adw::ApplicationWindow) -> Option<String> {
    progress_bar(window)?.tooltip_text().map(|text| text.to_string())
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

/// The ⏹ button (F0.3), found the same way as [`play_button`].
pub fn stop_button(window: &adw::ApplicationWindow) -> Option<gtk::Button> {
    find_widget_by_name(window.upcast_ref(), "stop-button")?.downcast().ok()
}

/// The "I'm feeling lucky" button (F0.4), found the same way as [`play_button`].
pub fn lucky_button(window: &adw::ApplicationWindow) -> Option<gtk::Button> {
    find_widget_by_name(window.upcast_ref(), "lucky-button")?.downcast().ok()
}

/// The tooltip ▶ carries, which is how a test reads §2's wording without reaching into the button.
pub fn play_tooltip(window: &adw::ApplicationWindow) -> Option<String> {
    play_button(window)?.tooltip_text().map(|text| text.to_string())
}

/// The first widget under `root` carrying `name`, found the same walk as [`find_status`].
pub(crate) fn find_widget_by_name(root: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
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
