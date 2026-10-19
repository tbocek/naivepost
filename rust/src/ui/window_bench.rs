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
use crate::layout;
use crate::cut_select;
use crate::prepare;
use crate::run;
use crate::runqueue;
use crate::narration::{self, Narration};
use crate::project::Project;
use crate::shell::{Page, Shell};
use crate::ui::window as ui;

/// §1's two readouts as the row draws them: each label is the word plus what `shell` computed, so
/// the prefix is the only text this file adds to those two rows.
const INPUTS_PREFIX: &str = "Inputs: ";
const OUTPUTS_PREFIX: &str = "Outputs: ";
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
        slot.borrow()
            .filter(|index| *index < count)
            .unwrap_or(0)
    })
}

pub(crate) fn bench_box(
    session: &Rc<RefCell<Project>>,
    paths: &Option<crate::settings::Paths>,
) -> (gtk::Box, gtk::TextView) {
    let mut bench_state = bench::Bench::new();
    // The snapshot seam above; `select` ignores an index off the end, so this is a no-op unless a
    // caller asked for a specific row.
    bench_state.select(bench_open_row(bench::ROWS.len()));
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
    let picker = gtk::DropDown::from_strings(
        &titles
            .iter()
            .map(String::as_str)
            .collect::<Vec<&str>>(),
    );
    picker.set_widget_name("bench-picker");
    picker.set_halign(gtk::Align::Start);
    // The dropdown's own face must agree with the row the bench opened on, or the shot would read
    // "Describe prompt" beside a picker still showing "User Context". Set before `paint_bench` so
    // no notify fires against a half-built heading.
    picker.set_selected(u32::try_from(row.borrow().selected).unwrap_or(0));

    // **4** Reset, live only while this machine holds an edit.
    let reset = gtk::Button::with_label("Reset");
    reset.set_widget_name("bench-reset");
    // §6's other tooltip: idle wording until something is edited; `bench_paint::paint_marks` swaps in
    // the live one.
    reset.set_tooltip_text(Some(bench::UNCHANGED_TIP));
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
    // The prompt's own height band. Left uncapped the text view reported its FULL document height as the
    // page's minimum — the shipped prompts run to hundreds of lines, and that alone pinned the shell at
    // ~970 px tall so the window could not be shrunk (`tests/suite/window_shrinks.rs`). Inside this scroller
    // the editor scrolls between these bounds instead of stretching the page.
    scroller.set_min_content_height(BENCH_MIN_H);
    scroller.set_max_content_height(BENCH_MAX_H);

    // The box's contents are set when the row changes, never from its own `changed` handler: typing
    // stores text and storing must not retype the buffer out from under the cursor.
    let guard = Rc::new(std::cell::Cell::new(false));
    paint_bench(
        &row,
        &title,
        &mark,
        &picker,
        &reset,
        &editor,
        &guard,
        session,
        &paths,
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
        let index = picker.selected();
        if index == gtk::INVALID_LIST_POSITION {
            return;
        }
        row_for_picker.borrow_mut().select(usize::try_from(index).unwrap_or(0));
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
        let Some(paths) = paths_for_reset.as_ref() else { return };
        // User Context has no shipped wording to restore, and `bench::reset` says so with `None`.
        if let Ok(Some(_)) = bench::reset(paths, &current) {
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
        // F0.7's first door: editing the User Context re-derives the policy — but debounced, so a
        // paragraph typed costs one request rather than one per keystroke. Only the context row
        // triggers it; every other bench row is prompt wording, which no policy depends on.
        if bench::is_context(&current) {
            if let Some(window) = ui::POLICY_WATCH_WINDOW.with(|w| w.borrow().clone()) {
                ui::schedule_policy_derive(&window);
            }
        }
    });

    // The heading and the box are returned together; `page_box` stacks them in §1's order. The
    // scroller is dropped with its child still inside: GTK keeps what it was added to.
    drop(scroller);
    (heading, editor)
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
    buffer.set_text(&bench::text(paths.as_ref().unwrap_or(&NO_SETTINGS), &session.borrow(), held.row()));
    guard.set(false);
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
/// [`refresh_selection_readout`], Aspect ratio to `refresh_aspect_readout`, and Thumbnails is set by the
/// 🖼−/🖼+ ladder, which writes the number it drives.
pub(crate) fn refresh_cut_readouts(window: &adw::ApplicationWindow) {
    let cut_ = ui::newest_review_cut();
    // The footage total: what the `Source:` row answers with — the session's merged filmed runs, the same
    // spans the strip above paints, so the number matches the picture.
    let source: f64 = crate::timeline::filmed_runs(&crate::ui::cut_folds::page_recordings())
        .iter()
        .map(|(start, end)| end - start)
        .sum();
    let rows = cut_screen::idle_readouts(
        crate::cut_screen::THUMB_AT_OPEN,
        &cut_.aspect,
        crate::ui::preview_player(window).playhead.unwrap_or(0.0),
        cut_select::marks(ui::selection(window).as_ref()),
        &cut_,
        source,
    );
    for row in rows.iter() {
        // Only the four readings that answer from the timeline. The other four each have their own owner
        // already: Thumbnails is written by the 🖼−/🖼+ ladder, Aspect ratio by `refresh_aspect_readout`,
        // Selection by [`refresh_selection_readout`], and Playhead by the preview's own clock tick.
        if !matches!(row.label, "Cut" | "Cut at 1\u{00d7}" | "Segments" | "Source") {
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
/// and narration are read here — [`session_reads`] still points at the working copy beside the root.
pub(crate) fn paint_readouts_for(window: &adw::ApplicationWindow, project: &Project, dir: &Path) {
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
    let cut = tree.as_ref().and_then(|tree| cut::load(tree).ok()).unwrap_or_default();
    let narr = tree.as_ref().and_then(|tree| narration::load(tree).ok()).unwrap_or_default();
    paint_readouts(&shell.borrow(), project, tree.as_ref(), &cut, &narr, inputs.upcast_ref(), &folder, outputs.upcast_ref());
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
        let Ok(widget) = child.downcast::<gtk::Widget>() else { continue };
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
#[allow(clippy::too_many_arguments)] // the readout row paints inputs, outputs, their folder button and
// the log expander from one shell read
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
    inputs_label.set_text(&format!("{INPUTS_PREFIX}{}", shell.inputs(tree, project, cut, narration)));
    // §1 badge **6**: the Inputs row's tooltip is per-file. Prepare has one; every other page's
    // row clears it rather than keeping a stale per-source list from the tab just left.
    match shell.inputs_tip(tree, project) {
        Some(tip) => {
            inputs_label.set_tooltip_text(Some(&tip));
            inputs_label.set_sensitive(true);
        }
        None => inputs_label.set_tooltip_text(None::<&str>),
    }
    outputs_label.set_text(&format!("{OUTPUTS_PREFIX}{}", shell.outputs(tree)));
    // §6's second half: "newest <ago>" on hover, cleared for any page that has no age to date.
    match shell.outputs_tip(tree) {
        Some(tip) => outputs_label.set_tooltip_text(Some(&tip)),
        None => outputs_label.set_tooltip_text(None::<&str>),
    }
    outputs_folder.set_sensitive(tree.is_some());
    // §1 badge **4**: the header's project name/path is redrawn at the same tick as the readouts
    // ("show the page and its Inputs/Outputs readouts", F0.1 S4), at whatever stage its width allows.
    ui::paint_project_title();
    // §1: "Outputs: folder button and count" — the button points at the page's own output folder,
    // which is the directory `Shell::outputs` just counted.
    if let Some(tree) = tree {
        // Prepare's tooltip goes further than "open this folder": §1 asks that it name the three
        // subfolders the count covers, so the number says what is inside it.
        let page_tip = if shell.page == crate::shell::Page::Prepare {
            format!(" ({})", prepare::OUTPUTS_TIP)
        } else {
            String::new()
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
    lucky: &gtk::Button,
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
    // §2: the gears are insensitive while a run is busy; ⏸ and ⏹ are what act on that run.
    lucky.set_sensitive(drawn.lucky_sensitive);
    // §2: the gears turn while ANY run computes, a single ▶ step included, so the bar says work is going
    // on even when the progress bar has nothing to count. Owning the class here (not only in `lucky_run`)
    // is what makes that true: `lucky_run` also toggles it around a chain, and both agree -- ON exactly
    // while a run stands -- so the two never fight.
    if drawn.gears_turning {
        lucky.add_css_class(crate::ui::lucky_run::RUNNING_CLASS);
    } else {
        lucky.remove_css_class(crate::ui::lucky_run::RUNNING_CLASS);
    }
}

/// §10's prose log heights: 220 px open, 110 px collapsed. No `P.` id exists for them, so they are
/// bare constants here with the note rather than invented parameter rows.
pub(crate) const LOG_OPEN_PX: i32 = 220;
/// The prompt bench's height band: enough lines to read a paragraph of prompt without scrolling, and
/// capped so a long shipped prompt scrolls inside the bench instead of pushing the page taller (see
/// `bench_box`).
pub(crate) const BENCH_MIN_H: i32 = 160;
pub(crate) const BENCH_MAX_H: i32 = 420;

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
        let buffer = log_view.buffer();
        buffer.set_text("");
        super::log_link::paint_lines(&buffer, &ui::window_logs());
        // The newest line is the one worth seeing.
        let mut end = buffer.end_iter();
        log_view.scroll_to_iter(&mut end, 0.0, false, 0.0, 1.0);
    }
}
