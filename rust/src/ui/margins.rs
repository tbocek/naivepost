//! 03-shell §1's margins: "nothing on a page or in the bottom bar touches the window's edge or a divider".
//! A page keeps its distance from the window's edges in `window_page_box` (24 px, more than the 12 the spec
//! asks); the bottom bar keeps [`BAR_EDGE_PX`] from them in `window_divider`; a column divider keeps
//! [`DIVIDER_GAP_PX`] on each side through one stylesheet, because pages build their paneds whenever they
//! build them (the Cut layout inserts its own later) and a walk at build time would miss them; and every text
//! box is made through [`text_box`], which gives it [`TEXT_INSET_PX`] between its frame and its text (GTK
//! does not apply CSS padding to a text view's text).

use gtk4 as gtk;

/// The gap on each side of a column divider (§1: "at least 12 px … from each side of a column divider").
pub const DIVIDER_GAP_PX: i32 = 12;

/// Between a text box's frame and its text (§1: "keeps 8 px between its frame and its text").
pub const TEXT_INSET_PX: i32 = 8;

/// The bottom bar's distance from the window's left, right and bottom edges (§1: "12 px").
pub const BAR_EDGE_PX: i32 = 12;

/// Above the bottom bar's run row, under the divider (§1: "8 px under the divider above it").
pub const BAR_TOP_PX: i32 = 8;

/// Between the run row and the log header under it, and between the run row's controls (§1: "8 px").
pub const BAR_GAP_PX: i32 = 8;

/// The bottom bar's margins above and below it. GTK lays margins OUTSIDE a widget's size request, so the
/// band's request is its height less these, while the divider's handle goes at the height with them.
pub const BAR_MARGINS_PX: i32 = BAR_TOP_PX + BAR_EDGE_PX;

/// What the bottom bar's own spacing adds to its two rows: the gap above, between and below them.
pub const BAR_SPACING_PX: i32 = BAR_MARGINS_PX + BAR_GAP_PX;

/// The stylesheet: the children of every column divider kept off it.
pub fn css() -> String {
    format!(
        "paned.horizontal > :first-child {{ margin-right: {DIVIDER_GAP_PX}px; }}\n\
         paned.horizontal > :last-child {{ margin-left: {DIVIDER_GAP_PX}px; }}\n"
    )
}

/// A text box's text kept [`TEXT_INSET_PX`] off its frame on every side, never less than it already had.
pub fn text_box(view: &gtk::TextView) {
    use gtk::prelude::TextViewExt;
    view.set_left_margin(view.left_margin().max(TEXT_INSET_PX));
    view.set_right_margin(view.right_margin().max(TEXT_INSET_PX));
    view.set_top_margin(view.top_margin().max(TEXT_INSET_PX));
    view.set_bottom_margin(view.bottom_margin().max(TEXT_INSET_PX));
}

/// Put [`css`] on the display, once per process.
pub fn install_once() {
    static LOADED: std::sync::Once = std::sync::Once::new();
    LOADED.call_once(|| {
        let Some(display) = gtk::gdk::Display::default() else {
            return;
        };
        let provider = gtk::CssProvider::new();
        provider.load_from_string(&css());
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    });
}
