//! The window: renders a [`Project`]'s state and nothing else.
//!
//! No rules live here (spec/00-principles.md §5) — a page is a label of the
//! state it was handed, so every value on screen traces back to the model.

use adw::prelude::*;
use gtk4 as gtk;

use crate::project::Project;
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

/// Build the window showing `project`. `page` selects the visible tab.
pub fn build_window(app: &impl IsA<gtk::Application>, project: &Project, page: &str) -> adw::ApplicationWindow {
    let window = adw::ApplicationWindow::new(app);
    window.set_default_size(1400, 900);

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

    let box_ = gtk::Box::new(gtk::Orientation::Vertical, 0);
    box_.append(&switcher);
    box_.set_widget_name("page-tabs");
    box_.append(&stack);
    stack.set_vexpand(true);

    window.set_content(Some(&box_));
    window
}
