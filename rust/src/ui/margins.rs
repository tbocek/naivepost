//! 03-shell §1's margins: "nothing on a page or in the bottom bar touches the window's edge or a divider".
//! A page keeps [`PAGE_EDGE_PX`] from the window's edges in `window_page_box`; the bottom bar keeps [`BAR_EDGE_PX`] from them in `window_divider`; a column divider keeps
//! [`DIVIDER_GAP_PX`] on each side through one stylesheet, because pages build their paneds whenever they
//! build them (the Cut layout inserts its own later) and a walk at build time would miss them; and every text
//! box is made through [`text_box`], which gives it [`TEXT_INSET_PX`] between its frame and its text (GTK
//! does not apply CSS padding to a text view's text).

use gtk4 as gtk;

/// The gap on each side of a divider, either way (§1: "12 px from each side of a divider").
pub const DIVIDER_GAP_PX: i32 = 12;

/// §1: what a page keeps from the window's edges and from the header and the divider around it, on all four sides,
/// so its content lines up with the bottom bar's [`BAR_EDGE_PX`]. The first rewrite kept 24 at the sides and the
/// foot and 12 at the top, which read as a wide empty border and left the page out of line with the bar.
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
/// The text-button rule keeps every button one height: a glyph from a fallback font (▶✂, ‹‹f) has a taller line,
/// which over the theme's padding made Cut's text buttons 37 px and stretched the whole linked row, against 34 px
/// everywhere else. The height stands and the label centres in it, whatever font the glyph came from.
/// The media rule draws every button's ▶ ⏸ ⏹ and ✂ from one font, in every flow. Left to the fallback they came
/// from three fonts (▶ Adwaita Sans, ⏸ ⏹ Noto Sans Symbols 2, ✂ Noto Sans CJK), of different sizes and centred
/// differently, so a button's icon jumped as it turned from ▶ to ⏸. Named per button because this font also holds
/// digits and a wide space, which would change "3 s" or "Narrator 1". With the font missing, the fallback draws them.
/// The video-plate rule is what stands behind the video preview, on Cut and Narrate alike: black where the picture's
/// shape leaves room, and nothing around it. Cut's picture sits in a button, whose theme grey and padding drew a
/// grey frame round the slide, against Narrate's dark one with a stroke.
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
