//! The Prepare bench's mark and Reset, painted from what this machine holds.
//!
//! Split out of `window.rs`, which is past its size budget. Every line here is §6's liveness rule for
//! those two widgets: both live only while an override is stored, and Reset's tooltip says which of the
//! two states the row is in.

use gtk4 as gtk;
use gtk4::prelude::*;

use crate::{bench, settings};

/// The mark and Reset, both live only while this machine holds the row's wording (§1).
pub fn paint_marks(
    mark: &gtk::Label,
    reset: &gtk::Button,
    paths: Option<&settings::Paths>,
    row: &bench::Row,
) {
    let edited = paths.is_some_and(|paths| bench::edited(paths, row));
    mark.set_visible(bench::shows_mark(row, edited));
    reset.set_visible(bench::shows_reset(row, edited));
    reset.set_sensitive(bench::shows_reset(row, edited));
    // §6's two Reset tooltips travel with the button's liveness: "Put the built-in wording back" only
    // while something is actually stored to put back, "This is the built-in wording, unchanged" the rest
    // of the time. The mark's own tooltip is fixed (§6 spells it once) and set where the mark is built.
    reset.set_tooltip_text(Some(bench::reset_tip(row, edited)));
}
