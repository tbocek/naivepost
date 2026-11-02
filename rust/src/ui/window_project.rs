//! §F0.9 / §F0.10 / §F0.12 — the project dialogs: ＋ New, Open, Save, Rescan and Add sources, plus the
//! import run they drive. Doors are re-exported through `window`.

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
    copy_into_project, find_status, find_widget_by_name, log_line, progress_bar,
};
use crate::ui::window_page_state::{WindowPage, PAGE as WINDOW_PAGE};

pub(crate) const NEW_TIP: &str =
    "New project \u{2014} name it, put it where you want it, and start over";

/// S3's confirmation, drawn from [`new_project`]'s strings: "Start new…" is destructive, and Cancel keeps the
/// focus so a blind Enter does nothing.
pub fn new_project_confirm(
    parent: Option<&adw::ApplicationWindow>,
    detail: &str,
) -> adw::MessageDialog {
    let dialog = adw::MessageDialog::new(parent, Some(new_project::QUESTION), Some(detail));
    // The × answers exactly as Cancel does; `close-response` is set through the object because this binding
    // only has the trait method on Window.
    dialog.set_property("close-response", "cancel");
    dialog.add_response("cancel", "Cancel");
    dialog.add_response("start", new_project::BUTTON);
    dialog.set_response_appearance("start", adw::ResponseAppearance::Destructive);
    dialog.set_default_response(Some("cancel"));
    dialog
}

/// Where a press of ＋ New goes: ask [`new_project::press`], then draw what it says. Whether the session is
/// empty comes from the live handle the pages mutate; a run under way comes from F0.2's [`run::RunBar`].
pub(crate) fn wire_new(
    new_: &gtk::Button,
    bar: &Rc<RefCell<run::RunBar>>,
    status: &gtk::Label,
    session: &Rc<RefCell<Project>>,
) {
    let bar = bar.clone();
    let status = status.clone();
    let session = session.clone();
    new_.connect_clicked(move |_| {
        let root = std::env::current_dir().unwrap_or_default();
        let open = startup::open_dir();
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
                    // Cancel changes nothing.
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
/// A save-style dialog, not `select_folder`: a folder picker never shows an editable name, so S4's default
/// name, its `.naivepost` appending and its `-2`/`-3` walk would be thrown away. "Create" is the accept label
/// because "Save" would promise a file and a project is a folder (01 §1).
pub fn new_project_dialog(dir: &Path, day: &str) -> gtk::FileDialog {
    let dialog = gtk::FileDialog::builder()
        .title(new_project::TITLE)
        .accept_label("Create")
        .build();
    dialog.set_initial_folder(Some(&gio::File::for_path(dir)));
    dialog.set_initial_name(Some(&new_project::free_name(dir, day)));
    dialog
}

/// S4 answered, by the sheet or by a test handing over the same path. Each arm of [`new_project::answer`]
/// gets one sentence: create and apply (S6), say the refusal (S5), or change nothing on a dismissal.
pub fn name_chosen(
    root: &Path,
    chosen: Option<&Path>,
    status: &gtk::Label,
    session: &Rc<RefCell<Project>>,
) {
    match new_project::answer(root, chosen) {
        // S6: apply it: log, session swap, remembered pair, status.
        new_project::Answer::Created(created) => {
            crate::ui::new_apply::apply(&created, session, status)
        }
        // S5: the name stays the project that is there; nothing is half-applied or logged.
        new_project::Answer::Refused(reason) => status.set_text(&reason),
        new_project::Answer::Dismissed => {}
    }
}

/// S4: ask where the new project goes and what it is called. The day is taken here because [`new_project`]
/// is handed it rather than reading a clock; the answer goes through [`name_chosen`], the door a test uses.
fn ask_new_project(status: &gtk::Label, root: &Path, open: &Path, session: &Rc<RefCell<Project>>) {
    let day = glib::DateTime::now_local()
        .and_then(|today| today.format("%F").map(|day| day.to_string()))
        .unwrap_or_else(|_| "new-project".to_string());
    // S6's chooser rule: the free-name walk has to run where the person was last standing.
    let dir = crate::ui::new_apply::chooser_dir()
        .or_else(|| open.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| root.to_path_buf());
    let dialog = new_project_dialog(&dir, &day);
    WINDOW_PAGE.with(
        |WindowPage {
             last_ask: asked, ..
         }| *asked.borrow_mut() = Some(dialog.clone()),
    );

    let status = status.clone();
    let root = root.to_path_buf();
    let session = session.clone();
    dialog.save(
        None::<&gtk::Window>,
        None::<&gio::Cancellable>,
        move |chosen: Result<gio::File, glib::Error>| {
            // An error is a dismissal (GTK reports cancel that way).
            let picked = chosen.ok().and_then(|file| file.path());
            name_chosen(&root, picked.as_deref(), &status, &session);
        },
    );
}

/// What the last ＋ New press asked for: a headless run cannot type into the sheet, so a check reads the
/// title and default name here.
pub fn last_ask() -> Option<gtk::FileDialog> {
    WINDOW_PAGE.with(
        |WindowPage {
             last_ask: asked, ..
         }| asked.borrow().clone(),
    )
}

/// What the last Open press asked for, including the folder it should start in (compare `new_apply::chooser_dir()`).
/// Test-only by name.
pub fn last_open_ask_for_test() -> Option<gtk::FileDialog> {
    WINDOW_PAGE.with(
        |WindowPage {
             last_open: asked, ..
         }| asked.borrow().clone(),
    )
}

/// Prepare's two widgets (F0.12): the button that adds files, and the tick that decides whether they come
/// inside the project. The ellipsis is the single character §1's screenshot shows.
pub(crate) const ADD_LABEL: &str = "Add source files…";
pub(crate) const COPY_LABEL: &str = "copy into project";

pub(crate) const RESCAN_TIP: &str = "Rescan inputs and outputs";

/// §1's wording for the Settings badge: names the two servers so the gear is never unlabelled.
pub(crate) const SETTINGS_TIP: &str = "Settings \u{2014} the LLM and audio.cpp endpoints";

/// Where a press of Rescan goes: [`rescan`] decides everything. S1 drops what has vanished whether or not a
/// run is on, so this never reads [`run::RunBar`].
pub(crate) fn wire_rescan(
    button: &gtk::Button,
    status: &gtk::Label,
    session: &Rc<RefCell<Project>>,
    shell: &Rc<RefCell<Shell>>,
) {
    let status = status.clone();
    // The window holds no live project yet, so the scan works on the private copy shared with Add sources and
    // [`rescan_apply`] writes the pruned list back. The folder is the same stand-in New and Save use.
    let root = std::env::current_dir().unwrap_or_default();
    let project = session.clone();
    let shell = shell.clone();
    button.connect_clicked(move |_| {
        let dir = startup::open_dir();
        let tree = layout::Tree::new(&dir).ok();
        let found = rescan::rescan(&mut project.borrow_mut(), &root, &dir, tree.as_ref());
        // Every step with that answer is `rescan_apply`'s, in the spec's order.
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

/// Where a press of "Add source files…" goes: [`add_sources`] decides whether it may happen and what is
/// copied or only referenced.
///
/// A copy is a run of its own (F0.12 S0): it reports through the bookkeeping ▶ uses ([`add_files`] feeds
/// `runqueue::Queue`) rather than a second bar. The session is the window's private copy, as in [`wire_rescan`].
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
        // S0 first: no chooser opens while something else runs.
        if let Err(reason) = add_sources::press(bar.borrow().running.is_some()) {
            status.set_text(reason);
            return;
        }
        ask_add_sources(&window, &status, &session, &copy_into);
    });
}

/// S1's chooser: titled for adding, filtered to audio and video, accepting a whole card at once. A headless
/// test answers nobody; it asserts the press got this far.
///
/// GTK 4.10 deprecated the file chooser, but `gtk::FileDialog` has no multiple-selection filter that answers a
/// *native* chooser, so this stays on the old calls and takes their warnings here.
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
    let chooser = gtk::FileChooserNative::builder()
        .title(add_sources::CHOOSER_TITLE)
        .modal(true)
        .action(gtk::FileChooserAction::Open)
        .build();
    // `add_choice` is not the three-label call it looks like (it adds a combo), so the buttons are set here.
    chooser.set_accept_label(Some("Add"));
    chooser.set_cancel_label(Some("Cancel"));
    chooser.set_select_multiple(true);
    let filter = gtk::FileFilter::new();
    filter.set_name(Some(add_sources::FILTER_NAME));
    for ext in add_sources::MEDIA_EXT {
        filter.add_pattern(&format!("*{ext}"));
    }
    chooser.add_filter(&filter);
    // F0.8 S6: a project created outside the root is where its footage almost certainly is.
    if let Some(dir) = crate::ui::new_apply::chooser_dir() {
        let _ = chooser.set_current_folder(Some(&gio::File::for_path(&dir)));
    }

    let _status = status.clone();
    let _session = session.clone();
    let _copy_into = copy_into.clone();
    // The same stand-in New, Save and Rescan use.
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
    // Built is not shown: without this the chooser existed unseen and its response never came.
    WINDOW_PAGE.with(
        |WindowPage {
             choosers_shown: shown,
             ..
         }| shown.set(shown.get() + 1),
    );
    crate::ui::chooser_hold::show(&chooser);
}

/// How many Add sources choosers have been raised so far.
pub fn add_chooser_shown_count() -> usize {
    WINDOW_PAGE.with(
        |WindowPage {
             choosers_shown: shown,
             ..
         }| shown.get(),
    )
}

/// Zero the count, so a test asserts a delta of one.
pub fn reset_add_chooser_shown() {
    WINDOW_PAGE.with(
        |WindowPage {
             choosers_shown: shown,
             ..
         }| shown.set(0),
    );
}

pub(crate) const SAVE_TIP: &str = "Save this project to a file";

/// The seam a widget test drives: the body the chooser's callback calls, reached through the window. It
/// finds the widgets this window built and forwards to [`open_folder`].
pub fn open_from(window: &adw::ApplicationWindow, picked: &Path) -> bool {
    let (Some(bar), Some(status), Some(session)) = (
        WINDOW_PAGE.with(
            |WindowPage {
                 window_bar: slots, ..
             }| slots.borrow().last().cloned(),
        ),
        find_status(window.upcast_ref()),
        WINDOW_PAGE.with(|WindowPage { session: slots, .. }| slots.borrow().last().cloned()),
    ) else {
        panic!("the window has no run bar, status line or session");
    };
    open_folder(&bar, &status, &session, picked)
}

/// The seam a widget test drives for F0.12: the body the "Add source files…" response calls. [`add_sources`]
/// decides; this is where the copy's bytes reach the bar.
///
/// A copy is a run of its own (F0.12 S0): one track, each file one task, each file's byte fraction what `prog`
/// reports, so the bar moves with the bytes copied (`add_sources::Progress`).
///
/// The copy runs on a worker (§03-shell §1); each file's row is added and drawn here as it lands. ⏹ ends the
/// copy between two files; what landed stays.
/// Returns whether the copy started. Refused while another run is on (S0), leaving the session untouched.
pub fn add_files(window: &adw::ApplicationWindow, files: &[std::path::PathBuf]) -> bool {
    let (Some(bar), Some(status), Some(session), Some(queue), Some(progress)) = (
        WINDOW_PAGE.with(
            |WindowPage {
                 window_bar: slots, ..
             }| slots.borrow().last().cloned(),
        ),
        find_status(window.upcast_ref()),
        WINDOW_PAGE.with(|WindowPage { session: slots, .. }| slots.borrow().last().cloned()),
        WINDOW_PAGE.with(
            |WindowPage {
                 window_queue: slots,
                 ..
             }| slots.borrow().last().cloned(),
        ),
        progress_bar(window),
    ) else {
        panic!("the window has no run bar, status line, session, queue or progress bar");
    };
    let copy = copy_into_project(window).is_some_and(|tick| tick.is_active());
    let root = std::env::current_dir().unwrap_or_default();
    let dir = startup::open_dir();
    // S0 again at the seam: pressed while a run is on, it says the sentence and stops before any byte moves.
    // Nothing here clears a ▶ run's flag; ⏹ owns that.
    if let Err(reason) = add_sources::press(bar.borrow().running.is_some()) {
        status.set_text(reason);
        return false;
    }
    // The folder a copy lands in must exist before the first `.part` is renamed into it (F0.12 S2).
    if copy && std::fs::create_dir_all(dir.join("sources")).is_err() {
        status.set_text(add_sources::NO_SOURCES_DIR);
        log_line(add_sources::NO_SOURCES_DIR);
        return false;
    }
    // The import marks itself as *the* run so a ▶ press during a copy is refused; only this flow clears it.
    bar.borrow_mut().running = Some(run::Run {
        step: run::Step::Prepare,
        paused: false,
        log_expanded: true,
        sources: run::snapshot_sources(&session.borrow()),
    });
    // One job on the first track; each file is one task, so the tooltip counts the pick, not survivors.
    queue.borrow_mut().job(TRACK_IMPORT, IMPORT_JOB, 0, 0);
    queue.borrow_mut().push(TRACK_IMPORT, files.len(), "file");
    let counted = Rc::new(std::cell::Cell::new((0usize, 0usize)));
    let on_landed = {
        let window = window.clone();
        let session = session.clone();
        let status = status.clone();
        let queue = queue.clone();
        let progress = progress.clone();
        let counted = counted.clone();
        let root = root.clone();
        let dir = dir.clone();
        move |landed: Landed| {
            queue.borrow_mut().take(TRACK_IMPORT);
            queue.borrow_mut().prog(
                TRACK_IMPORT,
                landed.fraction,
                &format!("copying {}", landed.name),
            );
            paint_import(&progress, &queue);
            // `add_sources`' own rule: the copy finds this file already on disk at its size and only stats it.
            let added = add_sources::add(
                &mut session.borrow_mut(),
                &root,
                &dir,
                std::slice::from_ref(&landed.file),
                copy,
            );
            match added {
                Ok(answer) => {
                    let (went_in, already) = counted.get();
                    counted.set((
                        went_in + answer.added,
                        already + answer.asked - answer.added,
                    ));
                    draw_new_rows(&window, &session, &status);
                }
                Err(err) => log_line(&err),
            }
        }
    };
    let finish = {
        let bar = bar.clone();
        let status = status.clone();
        let queue = queue.clone();
        let counted = counted.clone();
        move |copied: Result<bool, String>| {
            queue.borrow_mut().done(TRACK_IMPORT, 1.0);
            // Set the status by hand: `end_run` clears it, and this flow needs it for S5's sentence.
            bar.borrow_mut().running = None;
            bar.borrow_mut().stop_flag = false;
            crate::cancel_leg::clear();
            match copied {
                Ok(stopped) => {
                    let (went_in, already) = counted.get();
                    status.set_text(&add_sources::status(went_in, already));
                    if stopped {
                        log_line(">>> add sources: stopped -- the files copied so far are in");
                    }
                }
                Err(err) => {
                    status.set_text(add_sources::NO_SOURCES_DIR);
                    log_line(&err);
                }
            }
        }
    };
    let files = files.to_vec();
    super::background::run_with_notes(
        "add sources",
        move |note| copy_files(&files, &dir, copy, note),
        on_landed,
        finish,
    );
    true
}

/// One file of the pick: the file, its name for the bar, and the byte fraction of the pick landed so far.
struct Landed {
    file: std::path::PathBuf,
    name: String,
    fraction: f64,
}

/// The copy half of a press, on the worker. `Ok(true)` when ⏹ ended it between two files.
fn copy_files(
    files: &[std::path::PathBuf],
    dir: &Path,
    copy: bool,
    note: &dyn Fn(Landed),
) -> Result<bool, String> {
    // The total is taken before the first byte moves, so the fraction never chases a growing sum.
    let total: u64 = files
        .iter()
        .map(|file| file.metadata().map(|meta| meta.len()).unwrap_or(0))
        .sum();
    let mut done_bytes = 0u64;
    for file in files {
        if crate::cancel_leg::cancelled() {
            return Ok(true);
        }
        let size = file.metadata().map(|meta| meta.len()).unwrap_or(0);
        if copy {
            // `add_sources`' own copy rule (`.part` then rename, same size skipped); not reimplemented here.
            add_sources::copy_one(file, dir).map_err(|err| err.to_string())?;
        }
        done_bytes += size;
        // The bar means bytes on disk, not task count: three tiny files beside one long recording are not mostly copied.
        let fraction = if total == 0 {
            1.0
        } else {
            done_bytes as f64 / total as f64
        };
        note(Landed {
            file: file.clone(),
            name: file
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_default(),
            fraction,
        });
    }
    Ok(false)
}

/// Draw every row the list does not show yet, by the door the build used, so an added row carries every
/// control §4 gives a row.
fn draw_new_rows(
    window: &adw::ApplicationWindow,
    session: &Rc<std::cell::RefCell<Project>>,
    status: &gtk::Label,
) {
    let total_rows = session.borrow().sources.len();
    let Some(list) = find_widget_by_name(window.upcast_ref(), "sources-list")
        .and_then(|widget| widget.downcast::<gtk::ListBox>().ok())
    else {
        return;
    };
    // Rows are counted with the same widget-tree walk the rest of this module uses.
    let drawn = list
        .observe_children()
        .iter::<glib::Object>()
        .filter_map(|child| child.ok())
        .filter(|child| child.downcast_ref::<gtk::ListBoxRow>().is_some())
        .count();
    for index in drawn..total_rows {
        list.append(&source_rows::row_at(
            &source_rows::Rows {
                session: Rc::clone(session),
                status: status.clone(),
                list: list.clone(),
            },
            index,
        ));
    }
}

/// Draw the import on F0.5's bar; fraction and tooltip come from the queue as in [`paint_progress`]. The
/// expander's body is left alone, since an import writes no run lines.
fn paint_import(progress: &gtk::ProgressBar, queue: &std::cell::RefCell<runqueue::Queue>) {
    let (_text, tip) = queue.borrow().text_and_tip();
    progress.set_fraction(queue.borrow().fraction());
    if !tip.is_empty() {
        progress.set_tooltip_text(Some(&tip));
    }
}

const IMPORT_JOB: &str = "import";
/// The copy gets one track: the whole pick is a single run of its own (hence S0 refuses while one is on).
const TRACK_IMPORT: usize = 0;

pub(crate) const OPEN_TIP: &str = "Load a project \u{2014} sources, prompts and settings";

/// Where a press of Open goes. [`open_project`] decides everything; this asks for a folder, then draws.
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
        // A headless test cannot answer the chooser; rules live in `open_project` (see `tests/suite/open_project_widgets.rs`).
        ask_open_project(&bar, &status, &session);
    });
}

/// S1: the folder chooser, titled for opening. Folder-select because a project *is* a folder (01 §1); picking
/// `naivepost.json` inside one still gets the folder ([`startup::project_folder`]).
fn ask_open_project(
    bar: &Rc<RefCell<run::RunBar>>,
    status: &gtk::Label,
    session: &Rc<RefCell<Project>>,
) {
    let dialog = gtk::FileDialog::builder()
        .title(open_project::TITLE)
        .accept_label("Open")
        .build();
    // S6's chooser rule: the picker starts where a project created outside the root lives.
    if let Some(dir) = crate::ui::new_apply::chooser_dir() {
        dialog.set_initial_folder(Some(&gio::File::for_path(&dir)));
    }
    WINDOW_PAGE.with(
        |WindowPage {
             last_open: asked, ..
         }| *asked.borrow_mut() = Some(dialog.clone()),
    );

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

/// F0.9's refusal during a run, in the shape New's and Save's take.
pub const OPEN_STOP_FIRST: &str =
    "stop the run first \u{2014} opening another project would leave it writing into this one";

/// S1 → S4 for one picked path, with no chooser in the way: the chooser callback's body and a widget test's seam.
///
/// S4's "refresh every page" is done by building the window again on the opened folder (`build_window_at`):
/// repainting some of it left the old project's sources and context under the new name. The old window is
/// closed FIRST, so its close hook flushes into its own folder, never the one being opened.
pub fn open_folder(
    bar: &Rc<RefCell<run::RunBar>>,
    status: &gtk::Label,
    _session: &Rc<RefCell<Project>>,
    picked: &Path,
) -> bool {
    if bar.borrow().running.is_some() {
        status.set_text(OPEN_STOP_FIRST);
        return false;
    }
    match open_project::open(picked, |path| path.is_file(), |path| path.exists()) {
        Err(failure) => {
            log_line(&failure.log);
            status.set_text(failure.status);
            false
        }
        Ok(applied) => {
            // S3: paths settled before the read; migration first, it reads what the swap consumes.
            let migration = open_project::finish(&applied);
            let opened_project = applied.project.clone();
            let window = reopen_window_on(&opened_project, &applied.root);
            for line in &applied.lines {
                log_line(line);
            }
            // S4: old folder names move, and each move is logged.
            for line in migration.lines.iter().chain(&migration.failures) {
                log_line(line);
            }
            // S4's last steps (remember, status) live in `ui::open_apply`, which owns their order and wording.
            let status = find_status(window.upcast_ref()).unwrap_or_else(|| status.clone());
            crate::ui::open_apply::finish(&applied, &opened_project, &status);
            true
        }
    }
}

/// Close the window and build it again on `dir`, same size, keeping the application alive meanwhile.
pub(crate) fn reopen_window_on(project: &Project, dir: &Path) -> adw::ApplicationWindow {
    let old = crate::ui::window::main_window();
    let app = old
        .application()
        .expect("a shell window belongs to the application that built it");
    let _alive = app.hold();
    let (width, height, maximized) = (old.width(), old.height(), old.is_maximized());
    old.close();
    let window = crate::ui::window::build_window_at(&app, project, "", dir);
    if width > 0 && height > 0 {
        window.set_default_size(width, height);
    }
    if maximized {
        window.maximize();
    }
    window.present();
    window
}

/// Put an opened project into the window's remembered list AND into `llm.conf`, the one conf door.
pub(crate) fn remember_opened(
    root: &Path,
    file: &Path,
) -> Result<open_project::Remembered, String> {
    let paths = crate::settings::from_environment().ok_or("no settings folder to write into")?;
    WINDOW_PAGE.with(
        |WindowPage {
             remembered: list, ..
         }| {
            let mut projects = list.borrow_mut();
            crate::machine_store::persist_project(&paths, &mut projects, root, file)
                .map(|_| open_project::remember(&mut projects, root, file))
        },
    )
}
