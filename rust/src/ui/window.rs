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
use crate::layout;
use crate::new_project;
use crate::project::Project;
use crate::rescan;
use crate::run;
use crate::save_as;
use crate::startup;
use crate::shell::{self, Move, Outcome, Page, Shell};
use crate::PAGES;

pub const APP_ID: &str = "ch.bocek.naivepost";

/// One page of the window: its name and the state it shows.
/// Prepare's two widgets are handed back beside the page so `build_window` can wire *that* button —
/// see [`build_window`] for why they are not found by name. On every other page both are `None`.
fn page_box(page: &str, project: &Project) -> (gtk::Widget, Option<gtk::Button>, Option<gtk::CheckButton>) {
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

    let stack = adw::ViewStack::new();
    let mut prepare_add: Option<(gtk::Button, gtk::CheckButton)> = None;
    for name in PAGES {
        let (child, add_, copy_) = page_box(name, project);
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

    // The window is handed an immutable `&Project` and holds no live project yet, so the flows that
    // change the session — Rescan (F0.11) and Add sources (F0.12) — work on one private copy shared
    // between them. F0.9's live project state replaces this; until then it is what keeps the session
    // each flow leaves behind for the next.
    let session = Rc::new(RefCell::new(project.clone()));

    // Where ▶ is decided: the run bar's own state, beside the shell's. Nothing about which of
    // pause / transport / start applies is worked out here — run.rs does that (F0.2).
    let bar = Rc::new(RefCell::new(run::RunBar::default()));

    // The status line: the shell's sentence, right-aligned in the bottom row (§1's "status line").
    let status = gtk::Label::new(Some(""));
    status.set_widget_name("status-line");
    status.set_xalign(1.0);
    status.set_hexpand(true);
    status.set_ellipsize(gtk::pango::EllipsizeMode::End);
    status.add_css_class("dim-label");
    status.set_margin_end(8);

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
