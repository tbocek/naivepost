//! §1's bench, the tab buttons and the readouts they repaint — the shell's half of those rows. Split out
//! of `window.rs`, which is past its size budget; its doors are re-exported through `window`.
//!
//! What is here draws: every number and sentence comes from [`crate::bench`], [`crate::layout`] or
//! [`crate::shell`], so a label cannot disagree with the state it names.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use adw::prelude::*;
use gtk4 as gtk;

use crate::bench;
use crate::cut::{self, Cut};
use crate::cut_screen;
use crate::cut_select;
use crate::layout;
use crate::narration::{self, Narration};
use crate::prepare;
use crate::project::Project;
use crate::run;
use crate::runqueue;
use crate::shell::{Page, Shell};
use crate::ui::window as ui;

/// §1's readout as the row draws it: the label is what `shell` computed, so this file adds no prefix of its own
/// to it — §1 badge **15**'s word "Outputs:" is a separate label in front of the folder button (`window.rs`).
const INPUTS_PREFIX: &str = "Inputs: ";
/// §1's badge **15** is a folder button beside the count; its tooltip names what the button opens.
const OUTPUTS_FOLDER_TIP: &str = "Open this project's output folder";

/// A plain label beside a control, dimmed the way a field name is.
pub fn label(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.add_css_class("dim-label");
    label
}

// §1's badges **2**-**5**: the bench's heading row and its one text box. Everything the row says —
// title, mark, Reset's liveness — is asked of [`bench`]; the widgets only forward.
thread_local! {
    /// The row the prompt bench opens on, when a caller says which one. `ui::bench_box` reads this once,
    /// before its first paint; nothing in the app ever sets it, so the ordinary state is `None` =
    /// row 0, the User Context. It exists because `--snapshot 04-prompt-picker` has to show the
    /// picker OPEN on a prompt row (that is what `spec/img/04-prompt-picker.png` pictures) and the
    /// only other way would be to build a second bench for the snapshot — a copy of the page that
    /// could drift from the real one.
    static BENCH_OPEN_ROW: std::cell::RefCell<Option<usize>> =
        const { std::cell::RefCell::new(None) };
}

/// Ask the next bench built in this thread to open on `index`. Snapshot-only seam; see
/// [`BENCH_OPEN_ROW`].
pub fn set_bench_open_row(index: usize) {
    BENCH_OPEN_ROW.with(|slot| *slot.borrow_mut() = Some(index));
}

/// The row `ui::bench_box` should start on: the requested one if it names a real row, else 0. A row
/// index off the end falls back rather than panicking, because a stale snapshot name must still draw
/// something rather than kill the run.
fn bench_open_row(count: usize) -> usize {
    BENCH_OPEN_ROW.with(|slot| {
        // Read ONCE and cleared: a second window built in the same process (a test round, or a second
        // shell) must open on its own default rather than inherit the ask that was made for another.
        let asked = slot.borrow_mut().take();
        asked.filter(|index| *index < count).unwrap_or(0)
    })
}

pub(crate) fn bench_box(
    session: &Rc<RefCell<Project>>,
    paths: &Option<crate::settings::Paths>,
) -> (gtk::Box, gtk::ScrolledWindow) {
    let mut bench_state = bench::Bench::new();
    // The snapshot seam above; `select` ignores an index off the end, so this is a no-op unless a
    // caller asked for a specific row.
    // §03-shell §5b: the prompts are configured in profiles, so the bench is the User Context alone; a
    // snapshot's ask for another row is spent and ignored.
    let _ = bench_open_row(bench::ROWS.len());
    bench_state.select(0);
    let row = Rc::new(RefCell::new(bench_state));
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
    // §6: the mark's tooltip says where the kept wording lives and what Reset would do to it; the
    // visibility rule is `bench_paint::paint_marks`'s.
    mark.set_tooltip_text(Some(bench::KEPT_TIP));
    mark.add_css_class("dim-label");
    heading.append(&mark);

    // **3** the row picker: the thirteen rows in pipeline order, chosen from a menu. A DropDown is
    // GTK's own menu of strings, which is what §1 asks for and what carries the selected index.
    let titles: Vec<String> = bench::picker_rows()
        .iter()
        .map(|(_, title)| title.to_string())
        .collect();
    let picker =
        gtk::DropDown::from_strings(&titles.iter().map(String::as_str).collect::<Vec<&str>>());
    picker.set_widget_name("bench-picker");
    picker.set_halign(gtk::Align::Start);
    // The dropdown's own face must agree with the row the bench opened on, or the shot would read
    // "Describe prompt" beside a picker still showing "User Context". Set before `paint_bench` so
    // no notify fires against a half-built heading.
    picker.set_selected(u32::try_from(row.borrow().selected).unwrap_or(0));
    // The prompt rows are gone from the page (a profile's per-step sections replace them): no picker to show.
    picker.set_visible(false);

    // **4** Reset, live only while this machine holds an edit.
    let reset = gtk::Button::with_label("Reset");
    reset.set_widget_name("bench-reset");
    // §6's other tooltip: idle wording until something is edited; `bench_paint::paint_marks` swaps in
    // the live one.
    reset.set_tooltip_text(Some(bench::UNCHANGED_TIP));
    // §1 puts the picker and Reset at the heading row's RIGHT END, after the title and its mark: the
    // spacer takes the free width so they sit at the column's edge rather than beside the title.
    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    heading.append(&spacer);
    heading.append(&picker);
    heading.append(&reset);

    // **5** the prompt or the User Context.
    let editor = gtk::TextView::new();
    crate::ui::margins::text_box(&editor);
    editor.set_widget_name("bench-text");
    editor.set_monospace(true);
    editor.set_wrap_mode(gtk::WrapMode::WordChar);
    editor.set_vexpand(true);
    let scroller = gtk::ScrolledWindow::new();
    scroller.set_child(Some(&editor));
    scroller.set_vexpand(true);
    // The prompt's own height band. Left uncapped the text view reported its FULL document height as the
    // page's minimum — the shipped prompts run to hundreds of lines, and that alone pinned the shell at
    // ~970 px tall so the window could not be shrunk (`tests/suite/window_shrinks.rs`). Inside this scroller
    // the editor scrolls between these bounds instead of stretching the page.
    scroller.set_min_content_height(BENCH_MIN_H);
    // NO `max_content_height`: a ceiling here capped the box at 420 px however much room the column had, so
    // in §1's 900 px window the bench stopped ~460 px down while the page band ran on below it — the shot
    // showed the right column ending short above an empty area. A `ScrolledWindow` without a ceiling still
    // reports only its MINIMUM as its own minimum height, so the text view's full document height cannot leak
    // into the shell's floor (the failure this cap was written for); vexpand fills the column and the text
    // scrolls inside it, which is §4's "one text box filling the rest of the column (it scrolls)".

    // The box's contents are set when the row changes, never from its own `changed` handler: typing
    // stores text and storing must not retype the buffer out from under the cursor.
    let guard = Rc::new(std::cell::Cell::new(false));
    // F0.14: the profile row in the heading (User Context row only) and its form in the scroller's place.
    crate::ui::profile_form::attach(&heading, &mark, &scroller, &editor, session);
    paint_bench(
        &row, &title, &mark, &picker, &reset, &editor, &guard, session, &paths,
    );

    let session_for_picker = session.clone();
    let paths_for_picker = paths.clone();
    let row_for_picker = row.clone();
    let guard_for_picker = guard.clone();
    let title_for_picker = title.clone();
    let mark_for_picker = mark.clone();
    let picker_for_picker = picker.clone();
    let reset_for_picker = reset.clone();
    let editor_for_picker = editor.clone();
    picker.connect_selected_notify(move |picker| {
        let index = usize::try_from(picker.selected()).unwrap_or(0);
        if picker.selected() == gtk::INVALID_LIST_POSITION {
            return;
        }
        row_for_picker.borrow_mut().select(index);
        paint_bench(
            &row_for_picker,
            &title_for_picker,
            &mark_for_picker,
            &picker_for_picker,
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
    let picker_for_reset = picker.clone();
    let editor_for_reset = editor.clone();
    let reset_for_reset = reset.clone();
    let row_for_reset = row.clone();
    let guard_for_reset = guard.clone();
    reset.connect_clicked(move |_| {
        let current = *row_for_reset.borrow().row();
        let Some(paths) = paths_for_reset.as_ref() else {
            return;
        };
        // User Context has no shipped wording to restore, and `bench::reset` says so with `None`.
        let reset_now = match bench::reset(paths, &current) {
            Ok(reset_now) => reset_now,
            Err(why) => {
                super::window_logs::say_failure(&format!(
                    "!!! could not reset the prompt -- {why}"
                ));
                None
            }
        };
        if reset_now.is_some() {
            paint_bench(
                &row_for_reset,
                &title_for_reset,
                &mark_for_reset,
                &picker_for_reset,
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
    let failing = std::rc::Rc::new(std::cell::Cell::new(false));
    buffer.connect_changed(move |buffer| {
        if guard.get() {
            return;
        }
        let (start, end) = (buffer.bounds().0, buffer.bounds().1);
        let text = buffer.text(&start, &end, false).to_string();
        // Every keystroke is stored (§1): there is no Save button on this bench. The session takes
        // the text whatever this machine's settings can do with it — User Context lives there, and a
        // prompt that cannot be written must still not be thrown away.
        let current = *row_for_text.borrow().row();
        let mut held = session_for_text.borrow_mut();
        match paths_for_text.as_ref() {
            Some(paths) => {
                // Every keystroke stores, so a failing write is said once per run of failures, not per key.
                match bench::store(paths, &mut held, &current, &text) {
                    Ok(()) => failing.set(false),
                    Err(why) => {
                        if !failing.replace(true) {
                            super::window_logs::say_failure(&format!(
                                "!!! could not save the prompt -- {why}"
                            ));
                        }
                    }
                }
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
        // F0.7's first door: editing the User Context re-derives the policy — but debounced, so a
        // paragraph typed costs one request rather than one per keystroke. Only the context row
        // triggers it; every other bench row is prompt wording, which no policy depends on.
        if bench::is_context(&current) {
            if let Some(window) = ui::POLICY_WATCH_WINDOW.with(|w| w.borrow().clone()) {
                ui::schedule_policy_derive(&window);
            }
        }
    });

    // The heading and the box are returned together; `page_box` stacks them in §1's order. What comes back as
    // the "box" is the SCROLLER, not the text view: dropping this scroller with its child still inside (as this
    // did) left the bare `gtk::TextView` in Prepare's column, and a text view reports its whole document as its
    // minimum height -- measured 2536 px for the shipped prompt -- which climbed through `prepare-split`,
    // Prepare's page box and `AdwViewStack` into `main-divider`'s start-half minimum, so every tab was laid out
    // ~2731 px tall inside an 890 px window and each page's content was painted above the top of the window.
    // The view keeps its own name inside the scroller, so every flow that finds `bench-text` still does.
    (heading, scroller)
}

/// Where this machine keeps an edited prompt (§1's ✎). `None` — no config home at all — leaves the
/// bench showing rows with no mark rather than pretending to hold edits it cannot write.
pub(crate) fn bench_paths() -> Option<crate::settings::Paths> {
    crate::settings::from_environment()
}

/// Where settings are not: no folder at all, so every read finds nothing and every shipped wording
/// shows through. Used when this machine has no config home to write prompts into.
const NO_SETTINGS: crate::settings::Paths = crate::settings::Paths {
    config_dir: std::path::PathBuf::new(),
    data_dir: std::path::PathBuf::new(),
};

/// Draw one row of the bench: its title, whether this machine holds it, and its text.
#[allow(clippy::too_many_arguments)] // one bench row is title, machine line, path, tick and buttons
fn paint_bench(
    row: &Rc<RefCell<bench::Bench>>,
    title: &gtk::Label,
    mark: &gtk::Label,
    picker: &gtk::DropDown,
    reset: &gtk::Button,
    editor: &gtk::TextView,
    guard: &Rc<std::cell::Cell<bool>>,
    session: &Rc<RefCell<Project>>,
    paths: &Option<crate::settings::Paths>,
) {
    let held = row.borrow();
    title.set_text(&bench::title(held.row()));
    // §6 badge 5: the picker says what the row it names does, and follows the ui::selection.
    picker.set_tooltip_text(Some(&bench::tooltip(held.row())));
    crate::ui::bench_paint::paint_marks(mark, reset, paths.as_ref(), held.row());

    // The text is set here and only here, with the store handler parked: writing into a buffer
    // fires `changed`, and a `changed` that stores would repaint the buffer again.
    guard.set(true);
    let buffer = editor.buffer();
    // With no config folder there is nothing this machine could hold, so the box shows the shipped
    // wording — which is what `bench::text` answers for a row nobody has touched.
    buffer.set_text(&bench::text(
        paths.as_ref().unwrap_or(&NO_SETTINGS),
        &session.borrow(),
        held.row(),
    ));
    guard.set(false);
    crate::ui::profile_form::follow_row(editor, bench::is_context(held.row()));
}

/// The mark and Reset, both live only while this machine holds the row's wording (§1).
/// The tab row, drawn from [`Shell`].
///
/// A locked page's tab is greyed and carries the lock reason as its tooltip; an unlocked one carries
/// its own description (§03-shell.md §1: "Locked tab: greyed, not disabled; tooltip = the reason").
/// Nothing here decides which of those two applies — [`crate::shell::lock`] does.
///
/// `adw::ViewSwitcher` builds its buttons itself and offers no handle to them (its API is
/// `stack`/`policy` alone), so they are found by walking the switcher's descendants for the label
/// each button draws — the page word, which is unique to one tab. Reaching the button rather than
/// the label matters: a tooltip on the label would vanish with the label when the bar narrows to
/// icons, and greying only the text would leave the button looking live.
/// Grey or brighten each tab for the project now open, on the newest window's own shell.
pub(crate) fn paint_tabs_for(window: &adw::ApplicationWindow, project: &Project) {
    let (Some(switcher), Some(shell)) = (
        ui::find_switcher(window.upcast_ref()),
        ui::WINDOW_SHELLS.with(|shells| shells.borrow().last().cloned()),
    ) else {
        return;
    };
    paint_tabs(&switcher, &shell.borrow(), project);
}

/// Repaint the idle-form readings that answer from the CUT — `Cut:`, `Cut at 1×:`, `Source:` and
/// `Segments:` — from the cut this window holds NOW.
///
/// The rows are built once by `cut_form_column` from `idle_readouts` for an EMPTY cut and nothing has
/// rewritten them since, so after any verb that changes the timeline — a suggest landing, a fold, a
/// paste — the page still read `Segments: 0` / `Cut: 00:00` beside landed scenes. Each value comes from
/// the same row list [`idle_readouts`] emits, so no second copy of a reading exists to drift. Everything
/// else on that form keeps its own owner: Playhead belongs to the preview clock, Selection to
/// [`refresh_selection_readout`] and Aspect ratio to `refresh_aspect_readout`.
pub(crate) fn refresh_cut_readouts(window: &adw::ApplicationWindow) {
    let cut_ = ui::newest_review_cut();
    // The footage total: what the `Source:` row answers with — the session's merged filmed runs, the same
    // spans the strip above paints, so the number matches the picture.
    let source: f64 = crate::timeline::filmed_runs(&crate::ui::cut_folds::page_recordings())
        .iter()
        .map(|(start, end)| end - start)
        .sum();
    let rows = cut_screen::idle_readouts(
        &cut_.aspect,
        crate::ui::preview_player(window).playhead.unwrap_or(0.0),
        cut_select::marks(ui::selection(window).as_ref()),
        &cut_,
        source,
    );
    for row in rows.iter() {
        // Only the four readings that answer from the timeline. The other three each have their own owner
        // already: Aspect ratio by `refresh_aspect_readout`,
        // Selection by [`refresh_selection_readout`], and Playhead by the preview's own clock tick.
        if !matches!(
            row.label,
            "Cut" | "Cut at 1\u{00d7}" | "Segments" | "Source"
        ) {
            continue;
        }
        let name = cut_screen::readout_widget(row.label);
        if let Some(label) = ui::find_widget_by_name(window.upcast_ref(), &name) {
            if let Ok(label) = label.downcast::<gtk::Label>() {
                label.set_text(&row.value);
            }
        }
    }
}

/// Redraw the visible tab's `Inputs:`/`Outputs:` row from the folder just switched TO, whose tree, cut
/// and narration are read here — [`session_reads`] still points at the working copy beside the root. Same
/// door a tab switch uses, so an Open, a Rescan and a staged test session all show through it.
pub fn paint_readouts_for(window: &adw::ApplicationWindow, project: &Project, dir: &Path) {
    let (Some(shell), Some(inputs), Some(folder), Some(outputs)) = (
        ui::WINDOW_SHELLS.with(|shells| shells.borrow().last().cloned()),
        ui::inputs_readout(window),
        ui::outputs_folder_button(window),
        ui::outputs_readout(window),
    ) else {
        return;
    };
    // Same door a tab switch uses, so the opened project shows through `Shell::inputs`/`outputs`.
    let tree = layout::Tree::new(dir).ok();
    let cut = tree
        .as_ref()
        .and_then(|tree| cut::load(tree).ok())
        .unwrap_or_default();
    let narr = tree
        .as_ref()
        .and_then(|tree| narration::load(tree).ok())
        .unwrap_or_default();
    paint_readouts(
        &shell.borrow(),
        project,
        tree.as_ref(),
        &cut,
        &narr,
        inputs.upcast_ref(),
        &folder,
        outputs.upcast_ref(),
    );
}

pub(crate) fn paint_tabs(switcher: &adw::ViewSwitcher, shell: &Shell, project: &Project) {
    let buttons = tab_buttons(switcher.upcast_ref());
    for page in Page::all() {
        // §2's "when it opens" column. `tab_states::opens` IS [`crate::shell::lock`] — asking it here is
        // what keeps the table and the tab row one rule instead of two that could drift.
        let reason = crate::tab_states::opens(page, project);
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
pub(crate) fn tab_buttons(root: &gtk::Widget) -> Vec<gtk::Widget> {
    let mut out = Vec::new();
    for child in root.observe_children().iter::<glib::Object>() {
        let Ok(child) = child else { continue };
        let Ok(widget) = child.downcast::<gtk::Widget>() else {
            continue;
        };
        if widget.downcast_ref::<gtk::Button>().is_some() {
            out.push(widget.clone());
        }
        out.extend(tab_buttons(&widget));
    }
    out
}

pub(crate) fn has_label(widget: &gtk::Widget, text: &str) -> bool {
    descendants(widget)
        .iter()
        .filter_map(|child| child.downcast_ref::<gtk::Label>())
        .any(|label| label.text() == text)
}

/// How deep a widget sits under its ancestor: the tab button holding the label is the deepest one,
/// while its rows and the switcher itself hold it too.
pub(crate) fn depth(widget: &gtk::Widget) -> usize {
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

/// S4's readout row, drawn from the shell. The two texts come from `Shell::inputs` and
/// `Shell::outputs` alone — this function only prefixes and sets labels, so a sentence on that row
/// always traces back to a rule in `shell.rs`. The folder button is insensitive while there is no
/// project folder to open, which is the visible proof the row knows what it is counting.
#[allow(clippy::too_many_arguments)]
// the readout row paints inputs, outputs, their folder button and
// the log expander from one shell read
/// The four readings [`paint_readouts`] takes off a worker.
struct Readings {
    inputs: String,
    inputs_tip: Option<String>,
    outputs: String,
    outputs_tip: Option<String>,
}

thread_local! {
    /// How many readout repaints were asked; a landing paints only when it answers the newest.
    static READOUT_ASKS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    /// Readouts read on the window thread itself: none ever should; a test reads it.
    static READ_HERE: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// How many readouts were read on this (the window's) thread.
pub fn readouts_read_here_for_test() -> usize {
    READ_HERE.with(std::cell::Cell::get)
}

pub(crate) fn paint_readouts(
    shell: &Shell,
    project: &Project,
    tree: Option<&layout::Tree>,
    cut: &Cut,
    narration: &Narration,
    inputs_label: &gtk::Label,
    outputs_folder: &gtk::Button,
    outputs_label: &gtk::Label,
) {
    // The four readings count files in the page's folders, which can be thousands of frames: they are read
    // on a worker and painted when they land (§03-shell §1, the window thread never waits). The last
    // readout stays up meanwhile, and only the newest ask is painted, so quick tab switches cannot land
    // out of order.
    let asked = READOUT_ASKS.with(|asks| {
        asks.set(asks.get() + 1);
        asks.get()
    });
    let held = (
        shell.clone(),
        project.clone(),
        tree.cloned(),
        cut.clone(),
        narration.clone(),
    );
    let owed = crate::ui::settle::owe("readouts", outputs_label);
    let inputs_label = inputs_label.clone();
    let outputs_label = outputs_label.clone();
    super::background::run(
        "readouts",
        move || {
            if !super::background::on_worker() {
                READ_HERE.with(|count| count.set(count.get() + 1));
            }
            let (shell, project, tree, cut, narration) = held;
            let tree = tree.as_ref();
            Readings {
                inputs: shell.inputs(tree, &project, &cut, &narration),
                inputs_tip: shell.inputs_tip(tree, &project),
                outputs: shell.outputs(tree),
                outputs_tip: shell.outputs_tip(tree),
            }
        },
        move |read: Readings| {
            if READOUT_ASKS.with(|asks| asks.get()) == asked {
                inputs_label.set_text(&format!("{INPUTS_PREFIX}{}", read.inputs));
                // §1 badge **6**: the Inputs row's tooltip is per-file. Prepare has one; every other page's
                // row clears it rather than keeping a stale per-source list from the tab just left.
                match read.inputs_tip {
                    Some(tip) => {
                        inputs_label.set_tooltip_text(Some(&tip));
                        inputs_label.set_sensitive(true);
                    }
                    None => inputs_label.set_tooltip_text(None::<&str>),
                }
                outputs_label.set_text(&read.outputs);
                // §6's second half: "newest <ago>" on hover, cleared for any page that has no age to date.
                match read.outputs_tip {
                    Some(tip) => outputs_label.set_tooltip_text(Some(&tip)),
                    None => outputs_label.set_tooltip_text(None::<&str>),
                }
            }
            drop(owed);
        },
    );
    outputs_folder.set_sensitive(tree.is_some());
    // §1 badge **4**: the header's project name/path is redrawn at the same tick as the readouts
    // ("show the page and its Inputs/Outputs readouts", F0.1 S4), at whatever stage its width allows.
    ui::paint_project_title();
    // §1: "Outputs: folder button and count" — the button points at the page's own output folder,
    // which is the directory `Shell::outputs` just counted.
    if let Some(tree) = tree {
        // Prepare's tooltip goes further than "open this folder": §1 asks that it name the three
        // subfolders the count covers, so the number says what is inside it.
        let page_tip = match shell.page {
            crate::shell::Page::Prepare => format!(" ({})", prepare::OUTPUTS_TIP),
            // §08-produce §1: "folder button tooltip: produce/ — the finished video, the per-clip encodes,
            // the thumbnail and the upload text".
            crate::shell::Page::Produce => format!(" ({})", crate::produce_screen::FOLDER_TIP),
            _ => String::new(),
        };
        outputs_folder.set_tooltip_text(Some(&format!(
            "{OUTPUTS_FOLDER_TIP}: {}{page_tip}",
            shell.output_dir(tree).display()
        )));
    }
    // S4's ⓘ sync: the header button describes the page `help_page` names, which a switch sets and
    // a bounce leaves alone. Repainting it here is what makes "sync ⓘ" happen on the same tick as the
    // readouts rather than in a second pass over the window.
    if let Some(info) = ui::INFO_BUTTON.with(|slots| slots.borrow().last().cloned()) {
        info.set_tooltip_text(Some(&shell.info_tip()));
    }
}

/// The run bar as it should be drawn right now: the icon and tooltip [`run::controls`] says, and
/// nothing else. It decides nothing — ▶ is one button that becomes ⏸ rather than a second button
/// beside it, and which of the two applies is F0.2's precedence, settled in run.rs.
/// The run bar's icon and tooltip are the whole of what F0.2 puts on screen. `transport` is the
/// visible page's preview, which only Cut and Narrate have — [`run::transport_for`] answers `None`
/// for the other two, and that is why their ▶ runs the step while a preview plays.
pub fn paint_run_bar(
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
    // F0.15: ↺ sits after ⏹, and a run's files are not to be taken from under it.
    if let Some(reset_) = stop_.next_sibling() {
        reset_.set_sensitive(bar.running.is_none());
    }
}

/// Repaint the run bar's ▶ and ⏹ from `window`'s bar and its preview, for a change that came from outside the
/// run bar: a Cut transport press starts or stops a preview, and ⏹ ("Stop the run or the playback", F0.3) must
/// be live while one plays. The first rewrite painted ⏹ only from the run bar's own presses, so a preview
/// started from ▶ / ▶✂ / ▶✂✂ left it grey and the playback could not be stopped from the bar.
pub fn repaint_run_bar(window: &adw::ApplicationWindow) {
    let (Some(play), Some(stop_)) = (
        crate::ui::window::play_button(window),
        crate::ui::window::stop_button(window),
    ) else {
        return;
    };
    let Some(bar) = crate::ui::window::RUN_BARS.with(|bars| bars.borrow().last().cloned()) else {
        return;
    };
    // A press painting while the bar is being changed elsewhere skips this paint; that change paints.
    let Ok(bar) = bar.try_borrow() else {
        return;
    };
    paint_run_bar(
        &play,
        &stop_,
        &bar,
        crate::ui::window::state(window).page,
        crate::ui::window::preview_player(window).transport,
    );
}

/// §10's prose log heights: 220 px open, 110 px collapsed. No `P.` id exists for them, so they are
/// bare constants here with the note rather than invented parameter rows.
pub(crate) const LOG_OPEN_PX: i32 = 220;
/// The least an open log's text keeps. Less room than this and the log collapses (`window_divider`): the run bar
/// and the "Log" header stay either way.
pub(crate) const LOG_MIN_BODY_PX: i32 = 60;
/// The prompt bench's floor: enough lines to read a paragraph of prompt without scrolling. The ceiling
/// is the scroller's own (`editor_scroller`), so nothing here caps it twice.
pub(crate) const BENCH_MIN_H: i32 = 160;

/// Draw F0.5's two widgets from the bookkeeping state: the bar's fraction and tooltip come from the
/// queue, the expander's openness from the run's `log_expanded`, and its body from the lines logged
/// so far. No decisions here — [`runqueue`] decided them, this paints them.
pub fn paint_progress(
    progress: &gtk::ProgressBar,
    log: &gtk::Expander,
    log_view: &gtk::TextView,
    queue: &runqueue::Queue,
    line: crate::ui::shell_beats::StatusLine<'_>,
    expanded: bool,
) {
    let (_text, tip) = queue.text_and_tip();
    progress.set_fraction(queue.fraction());
    // A run with nothing queued yet leaves the standing tooltip rather than blanking it (§2).
    if !tip.is_empty() {
        progress.set_tooltip_text(Some(&tip));
    }
    // §2: the bar draws no text -- the tracks' joined line goes to the status line.
    crate::ui::shell_beats::status_from_queue(line.status, queue, line.owned);
    log.set_expanded(expanded);
    if expanded {
        // The newest line is the one worth seeing while the person follows it (`log_paint::follow_end`).
        super::log_paint::repaint_lines(log_view, &ui::window_logs());
    }
}
