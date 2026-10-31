use adw::prelude::*;
use gtk4 as gtk;

use crate::cut::{self, Cut};
use crate::layout;
use crate::narration::{self, Narration};
use crate::shell::{Page, Shell};
use crate::startup;

/// Does this holder already own a child with exactly this widget name? Scoped to the holder on purpose —
/// see [`wire_fold_all`] for what a tree-wide search costs.
pub(crate) fn holder_children_named(holder: &gtk::Box, name: &str) -> bool {
    holder
        .observe_children()
        .iter::<glib::Object>()
        .flatten()
        .filter_map(|child| child.downcast::<gtk::Widget>().ok())
        .any(|child| child.widget_name() == name)
}

/// The shell as the window starts: on the page named, or on Prepare for `""` and for a name that is
/// not one of the four (a page nobody asked for is not a reason to fail to open).
pub(crate) fn start_shell(page: &str) -> Shell {
    match Page::all()
        .into_iter()
        .find(|candidate| candidate.label() == page)
    {
        Some(page) => Shell {
            page,
            help_page: page,
            ..Shell::default()
        },
        None => Shell::default(),
    }
}

/// The folder this window is working on, with its cut and narration read from it.
pub(crate) fn session_reads() -> (Option<layout::Tree>, Cut, Narration) {
    let dir = startup::open_dir();
    let tree = if dir.join(crate::project::PROJECT_FILE).is_file() {
        layout::Tree::new(&dir).ok()
    } else {
        None
    };
    let cut = tree
        .as_ref()
        .and_then(|tree| cut::load(tree).ok())
        .unwrap_or_default();
    let narration = tree
        .as_ref()
        .and_then(|tree| narration::load(tree).ok())
        .unwrap_or_default();
    (tree, cut, narration)
}
