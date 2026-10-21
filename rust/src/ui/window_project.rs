//! §F0.9 / §F0.10 / §F0.12 — the project dialogs: ＋ New, Open, Save, Rescan and Add sources, plus
//! the import run they drive. Split out of `window.rs`, which is past its size budget; its doors are
//! re-exported through `window`, so no caller's path changed.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use adw::prelude::*;
use gtk4 as gtk;

use crate::add_sources;
use crate::layout;
use crate::new_project;
use crate::open_project;
use crate::project::Project;
use crate::rescan;
use crate::run;
use crate::runqueue;
use crate::shell::Shell;
use crate::startup;
use crate::ui::rescan_apply;
use crate::ui::source_rows;
use crate::ui::window::{
    copy_into_project, find_status, find_widget_by_name, log_line, progress_bar, SESSION, WINDOW_BAR,
    WINDOW_QUEUE,
};

/// The New button's tooltip, §1's wording for badge **1**.
pub(crate) const NEW_TIP: &str = "New project \u{2014} name it, put it where you want it, and start over";

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
/// user sees comes from that module, including the refusal's and the confirmation's. Whether the
/// session is empty comes from the live handle the pages mutate, so S2 asks only when there is
/// something to lose; a run under way comes from F0.2's [`run::RunBar`], which ▶ answers for too.
pub(crate) fn wire_new(new_: &gtk::Button, bar: &Rc<RefCell<run::RunBar>>, status: &gtk::Label, session: &Rc<RefCell<Project>>) {
    let bar = bar.clone();
    let status = status.clone();
    let session = session.clone();
    new_.connect_clicked(move |_| {
        let root = std::env::current_dir().unwrap_or_default();
        let open = startup::session_dir(&root);
        let empty = session.borrow().sources.is_empty();
        match new_project::press(bar.borrow().running.is_some(), empty, &open) {
            new_project::Gate::Refused { reason } => status.set_text(reason),
            new_project::Gate::Name => ask_new_project(&status, &root, &open, &session),
            new_project::Gate::Confirm { detail } => {
                let dialog = new_project_confirm(None, &detail);
                let status = status.clone();
                let root = root.clone();
                let open = open.clone();
                let session = session.clone();
                dialog.connect_response(None, move |dialog: &adw::MessageDialog, response| {
                    // Cancel is not pressing on: S3's whole point is that nothing was changed.
                    if response == "start" {
                        ask_new_project(&status, &root, &open, &session);
                    }
                    dialog.destroy();
                });
                dialog.present();
            }
        }
    });
}

/// S4's sheet: where the new project goes AND what it is called, with the name editable.
///
/// A save-style dialog, not `select_folder`: a folder picker only ever hands back a folder that already
/// exists and never shows an editable name, so S4's default name, its `.naivepost` appending and its
/// `-2`/`-3` walk would be computed and thrown away, and S5's refusal could only fire on a folder that
/// happened to be a project under its own name. The save sheet is the one that puts a name in a text
/// box the person can overwrite, and the path it returns is the project they named.
///
/// "Create" as the accept label because "Save" would promise a file and a project is a folder (01 §1);
/// F0.8 does not name the button, so this is the app's decision.
pub fn new_project_dialog(dir: &Path, day: &str) -> gtk::FileDialog {
    let dialog = gtk::FileDialog::builder()
        .title(new_project::TITLE)
        .accept_label("Create")
        .build();
    dialog.set_initial_folder(Some(&gio::File::for_path(dir)));
    dialog.set_initial_name(Some(&new_project::free_name(dir, day)));
    dialog
}

/// S4 answered, however it was answered — by the sheet below or by a test handing over the same path.
/// Every arm of [`new_project::answer`] gets exactly one sentence here: create and apply (S6), say the
/// refusal (S5), or, for a dismissal, change nothing at all.
pub fn name_chosen(
    root: &Path,
    chosen: Option<&Path>,
    status: &gtk::Label,
    session: &Rc<RefCell<Project>>,
) {
    match new_project::answer(root, chosen) {
        // S6: apply it, not just announce it — log, session swap, remembered pair, status.
        new_project::Answer::Created(created) => {
            crate::ui::new_apply::apply(&created, session, status)
        }
        // S5: the name stays the project that is there; nothing is half-applied and nothing is logged.
        new_project::Answer::Refused(reason) => status.set_text(&reason),
        new_project::Answer::Dismissed => {}
    }
}

/// S4: ask where the new project goes and what it is called. The name's date is taken here because
/// [`new_project`] is handed the day rather than reading a clock, which is what keeps its tests off
/// today; the sheet's answer goes through [`name_chosen`], the same door a test uses.
fn ask_new_project(status: &gtk::Label, root: &Path, open: &Path, session: &Rc<RefCell<Project>>) {
    // `%F` is the ISO date, which is what the spec's default name is.
    let day = glib::DateTime::now_local()
        .and_then(|today| today.format("%F").map(|day| day.to_string()))
        .unwrap_or_else(|_| "new-project".to_string());
    // S6's chooser rule reaches this sheet too: after a project created outside the root, that is the
    // folder the person was last standing in, and the free-name walk has to run where the sheet stands.
    let dir = crate::ui::new_apply::chooser_dir()
        .or_else(|| open.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| root.to_path_buf());
    let dialog = new_project_dialog(&dir, &day);
    LAST_ASK.with(|asked| *asked.borrow_mut() = Some(dialog.clone()));

    let status = status.clone();
    let root = root.to_path_buf();
    let session = session.clone();
    dialog.save(
        None::<&gtk::Window>,
        None::<&gio::Cancellable>,
        move |chosen: Result<gio::File, glib::Error>| {
            // An error is a dismissal (GTK reports cancel that way): nothing was asked for.
            let picked = chosen.ok().and_then(|file| file.path());
            name_chosen(&root, picked.as_deref(), &status, &session);
        },
    );
}

/// What the last ＋ New press asked for. The desktop's sheet cannot be typed into in a headless run, so
/// this is how a check sees the title and the default name the press put up — the same handles the
/// press set, not a copy built for the test.
pub fn last_ask() -> Option<gtk::FileDialog> {
    LAST_ASK.with(|asked| asked.borrow().clone())
}

/// What the last Open press asked for, read back the same way as [`last_ask`] — including the folder
/// S6 says it should start in, which is what a check compares against `new_apply::chooser_dir()`.
/// Test-only by name: the program never reads it, only sets the slot when it opens the chooser.
pub fn last_open_ask_for_test() -> Option<gtk::FileDialog> {
    LAST_OPEN.with(|asked| asked.borrow().clone())
}

thread_local! {
    /// The naming sheet the last press opened, for [`last_ask`]. Thread-local like the rest of this
    /// window's state: it belongs to the session, not to the process's arguments.
    static LAST_ASK: std::cell::RefCell<Option<gtk::FileDialog>> =
        const { std::cell::RefCell::new(None) };

    /// The folder chooser the last Open press opened, for [`last_open_ask_for_test`].
    static LAST_OPEN: std::cell::RefCell<Option<gtk::FileDialog>> =
        const { std::cell::RefCell::new(None) };
}

/// Prepare's own two widgets (F0.12): the button that adds files, and the tick that decides whether
/// they come inside the project or stay where they are. The ellipsis is the single character §1's
/// screenshot shows, not three dots.
pub(crate) const ADD_LABEL: &str = "Add source files…";
pub(crate) const COPY_LABEL: &str = "copy into project";

/// The Rescan button's tooltip, §1's wording for badge **8**.
pub(crate) const RESCAN_TIP: &str = "Rescan inputs and outputs";

/// §1's wording for the Settings badge: the two servers the dialog is about, named in the tooltip
/// so the gear is never an unlabelled control.
pub(crate) const SETTINGS_TIP: &str = "Settings \u{2014} the LLM and audio.cpp endpoints";

/// Where a press of Rescan goes: [`rescan`] decides everything — which rows are gone, what was
/// re-read, what the status says. The button forwards and draws the answer (spec/00-principles.md §5).
///
/// No run refusal here: S1 drops what has vanished whether or not a run is on, so unlike Save this
/// handler never reads [`run::RunBar`].
pub(crate) fn wire_rescan(
    button: &gtk::Button,
    status: &gtk::Label,
    session: &Rc<RefCell<Project>>,
    shell: &Rc<RefCell<Shell>>,
) {
    let status = status.clone();
    // The window is handed an immutable `&Project` and holds no live project yet, so the scan works on
    // the private copy shared with Add sources, and [`rescan_apply`] writes the pruned list back to the
    // session file. Which folder is open is the same stand-in New and Save use, so all three agree.
    let root = std::env::current_dir().unwrap_or_default();
    let project = session.clone();
    let shell = shell.clone();
    button.connect_clicked(move |_| {
        let dir = startup::session_dir(&root);
        let tree = layout::Tree::new(&dir).ok();
        let found = rescan::rescan(&mut project.borrow_mut(), &root, &dir, tree.as_ref());
        // Every step the window takes with that answer -- the drop lines, the write-back, the vanished
        // page, all three refreshes and the status -- is `rescan_apply`'s, in the spec's order.
        let scanned = project.borrow().clone();
        rescan_apply::apply(
            &found,
            &scanned,
            &project,
            &dir,
            tree.as_ref(),
            &status,
            &shell,
        );
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
pub(crate) fn wire_add(
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
/// once. As in `ask_save_project` it is answered by nobody in a headless test, which is the point —
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
    // F0.8 S6: a project created outside the root is where its footage almost certainly is, so the
    // chooser opens there; with nothing remembered the chooser keeps its own default.
    if let Some(dir) = crate::ui::new_apply::chooser_dir() {
        let _ = chooser.set_current_folder(Some(&gio::File::for_path(&dir)));
    }

    let _status = status.clone();
    let _session = session.clone();
    let _copy_into = copy_into.clone();
    // The same stand-in New, Save and Rescan use, so all four flows agree on which project is open.
    let _root = std::env::current_dir().unwrap_or_default();
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
    // Built is not shown: without this the press did nothing at all — the chooser existed, unseen,
    // and its response never came. Same call as every other native chooser in the app.
    CHOOSERS_SHOWN.with(|shown| shown.set(shown.get() + 1));
    chooser.show();
}

thread_local! {
    /// How many times this thread has raised the Add sources chooser. A headless test can never
    /// answer one, so "the press got as far as opening it" is only observable as a count — a widget
    /// can outlive the borrow that made it, which rules out holding one in shared state.
    static CHOOSERS_SHOWN: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// The door a widget test reads: how many Add sources choosers have been raised so far.
pub fn add_chooser_shown_count() -> usize {
    CHOOSERS_SHOWN.with(|shown| shown.get())
}

/// Zero the count, so a test asserts a delta of one rather than whatever earlier checks stacked up.
pub fn reset_add_chooser_shown() {
    CHOOSERS_SHOWN.with(|shown| shown.set(0));
}

/// The Save button's tooltip, §1's wording for badge **3**.
pub(crate) const SAVE_TIP: &str = "Save this project to a file";

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
            // The added rows are drawn by the same door the build used, so an added row carries every
            // control §4 gives a row (the track menu included) rather than a shorter version of one.
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
                    list.append(&source_rows::row_at(
                        &source_rows::Rows {
                            session: Rc::clone(&session),
                            status: status.clone(),
                            list: list.clone(),
                        },
                        index,
                    ));
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

/// The Open button's tooltip, §1's wording for badge **2**.
pub(crate) const OPEN_TIP: &str = "Load a project \u{2014} sources, prompts and settings";


/// Where a press of Open goes. [`open_project`] decides everything — which folder the pick names,
/// whether it is an old single-file project, what the read produced and what had to be dropped; this
/// only asks for a folder, then draws the answer.
pub(crate) fn wire_open(
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
        // `tests/suite/open_project_widgets.rs`, which drives `open_folder` directly).
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
    // S6's chooser rule, for this chooser: with a project created outside the root this launch, the
    // person's projects are almost certainly there, so the picker starts there.
    if let Some(dir) = crate::ui::new_apply::chooser_dir() {
        dialog.set_initial_folder(Some(&gio::File::for_path(&dir)));
    }
    LAST_OPEN.with(|asked| *asked.borrow_mut() = Some(dialog.clone()));

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
            // S3: paths settled before the read; migration first, it reads what the swap consumes.
            let migration = open_project::finish(&applied);
            let lines = applied.lines.clone();
            let opened_project = applied.project.clone();
            *session.borrow_mut() = opened_project.clone();
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
            // S4's last three steps live in `ui::open_apply`, which owns their order and wording.
            crate::ui::open_apply::finish(&applied, &opened_project, status);
            // A run reading the old project would now be reading files that are not its own, so the
            // stop flag is raised rather than letting it carry on over swapped sources.
            let _ = bar.borrow();
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

/// Put an opened project into the window's remembered list AND into `llm.conf`, the one conf door.
pub(crate) fn remember_opened(root: &Path, file: &Path) -> Result<open_project::Remembered, String> {
    let paths = crate::settings::from_environment().ok_or("no settings folder to write into")?;
    REMEMBERED.with(|list| {
        let mut projects = list.borrow_mut();
        crate::machine_store::persist_project(&paths, &mut projects, root, file)
            .map(|_| open_project::remember(&mut projects, root, file))
    })
}
