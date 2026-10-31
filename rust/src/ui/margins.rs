//! 03-shell §1's margins: "nothing on a page or in the bottom bar touches the window's edge or a divider".

use gtk4 as gtk;

/// The gap on each side of a divider, either way (§1: "12 px from each side of a divider").
pub const DIVIDER_GAP_PX: i32 = 12;

pub const PAGE_EDGE_PX: i32 = 12;

/// Between a text box's frame and its text (§1: "keeps 8 px between its frame and its text").
pub const TEXT_INSET_PX: i32 = 8;

/// The bottom bar's distance from the window's left, right and bottom edges (§1: "12 px").
pub const BAR_EDGE_PX: i32 = 12;

/// Above the bottom bar's run row, under the divider (§1: "12 px" from a divider, like everything else).
pub const BAR_TOP_PX: i32 = 12;

/// Between the run row and the log header under it, and between the run row's controls (§1: "8 px").
pub const BAR_GAP_PX: i32 = 8;

/// The bottom bar's margins above and below it. GTK lays margins OUTSIDE a widget's size request, so the
/// band's request is its height less these, while the divider's handle goes at the height with them.
pub const BAR_MARGINS_PX: i32 = BAR_TOP_PX + BAR_EDGE_PX;

/// What the bottom bar's own spacing adds to its two rows: the gap above, between and below them.
pub const BAR_SPACING_PX: i32 = BAR_MARGINS_PX + BAR_GAP_PX;

/// The stylesheet: the children of every divider, either way, kept off it. The shell's own divider is left out:
/// its two halves keep [`PAGE_EDGE_PX`] and [`BAR_TOP_PX`] themselves, the same 12 px.
pub fn css() -> String {
    format!(
        "paned.horizontal > :first-child {{ margin-right: {DIVIDER_GAP_PX}px; }}\n\
         paned.horizontal > :last-child {{ margin-left: {DIVIDER_GAP_PX}px; }}\n\
         paned.vertical > :first-child {{ margin-bottom: {DIVIDER_GAP_PX}px; }}\n\
         paned.vertical > :last-child {{ margin-top: {DIVIDER_GAP_PX}px; }}\n\
         #main-divider > :first-child {{ margin-bottom: 0; }}\n\
         #main-divider > :last-child {{ margin-top: 0; }}\n\
         button.text-button {{ padding-top: 0; padding-bottom: 0; min-height: 34px; }}\n\
         #play-button, #stop-button, #play-recording-button, #play-cut-button, #review-cuts-button, #narrate-play, \
         #take-play, #sample-play, #sample-stop, .media-button {{ font-family: \"Noto Sans Symbols 2\"; }}\n\
         .video-plate, .video-plate:hover, .video-plate:active, .video-plate:focus {{ background: #000000; \
         border: none; border-radius: 0; box-shadow: none; outline: none; padding: 0; min-width: 0; min-height: 0; }}\n"
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
