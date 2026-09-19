//! The window: renders a [`Project`]'s state and nothing else.
//!
//! No rules live here (spec/00-principles.md §5) — a page is a label of the
//! state it was handed, so every value on screen traces back to the model.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk4 as gtk;

use crate::project::Project;
use crate::shell::{self, Move, Outcome, Page, Shell};
use crate::PAGES;

pub const APP_ID: &str = "ch.bocek.naivepost";

/// One page of the window: its name and the state it shows.
fn page_box(page: &str, project: &Project) -> gtk::Widget {
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

    let context = gtk::Label::new(Some(&project.context));
    context.set_wrap(true);
    context.set_xalign(0.0);
    context.add_css_class("body");
    box_.append(&context);

    view.set_content(Some(&box_));
    view.upcast()
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
    for name in PAGES {
        let child = page_box(name, project);
        stack.add_titled(&child, Some(name), name);
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

    let box_ = gtk::Box::new(gtk::Orientation::Vertical, 0);
    box_.append(&switcher);
    box_.set_widget_name("page-tabs");
    box_.append(&stack);
    stack.set_vexpand(true);
    box_.append(&status);

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
