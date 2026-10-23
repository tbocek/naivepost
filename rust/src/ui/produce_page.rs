//! §08-produce#1-screen — the Produce page's surface: §1's fifteen numbered widgets, laid out as
//! spec/inventory/produce.md §A says (horizontal paned: left = images row + instruction + negative
//! prompt + thumbnail; right = title + description above the encoder settings in a scroller) and wired
//! thin to the rules.
//!
//! No rule lives here. Every option, default, tooltip and sentence comes from [`crate::produce_screen`]
//! (the rows) or [`crate::produce_runs`] (what a press costs); a handler reads one widget, calls one
//! rules function and prints what came back on the status line (spec/00-principles.md §5).
//!
//! WHY a state holder instead of reading the project: `Project`'s publish and produce settings sit
//! behind `window.rs`'s private session reads, so the page renders a state the flows publish into it
//! through [`set_state`] — the same shape `narrate_page` uses.

use adw::prelude::*;
use gtk4 as gtk;

use crate::produce_screen as screen;
use crate::project::{Codec, Container, Subtitles};
use crate::ui::produce_page_layout as layout;
use crate::ui::produce_page_rows::{session_tree, words_half};

/// What the page shows. Published by the flows, read by every handler.
#[derive(Debug, Clone, Default)]
pub struct ProduceState {
    /// The images row: first = the base the image model edits, the rest references.
    pub frames: Vec<String>,
    /// A picture chosen as the thumbnail rather than drawn (`Publish::own`).
    pub own: bool,
    /// Words printed over the thumbnail, each with its own box.
    pub texts: Vec<crate::project::TextMark>,
    pub title: String,
    pub thumb_title: String,
    /// The first draw already printed the title, so a retitle does not repaint the picture.
    pub title_seeded: bool,
    pub instruction: String,
    pub negative: String,
    pub description: String,
    // ---- the encoder row, spelled by the row names the page draws ------------------
    pub container: String,
    pub codec: String,
    pub preset: String,
    pub resolution: String,
    pub frame_rate: String,
    pub audio: String,
    pub subtitles: String,
    pub translate: Vec<String>,
    pub game_volume: f64,
    pub crf: f64,
    pub vfr: bool,
    pub mono: bool,
    pub blurred_edges: bool,
    // ---- what the Inputs row reads ----------------------------------------------
    pub clips: usize,
    pub seconds: f64,
    pub narration_off: bool,
    pub to_speak: usize,
    pub publish_written: bool,
    /// The session's own language, which is never offered for translation.
    pub session_language: String,
    // ---- what the Inputs row's tooltip reads, published by the flows ---------------
    /// The cut's own sentence ("N clip(s) from mm:ss to mm:ss"), empty before a cut exists.
    pub cut_line: String,
    /// The first line the narration reads aloud, empty with nothing to speak.
    pub first_spoken: String,
    /// Aspect ratios present in the kept clips, spelled as §A spells them; more than one means the
    /// render letterboxes.
    pub mixed_sizes: Vec<String>,
    /// The voice the take would use, empty until F4.6's picker answers.
    pub voice: String,
    // ---- the Outputs row ---------------------------------------------------------
    pub out_files: usize,
    pub out_bytes: u64,
    /// A render is running: ⤓ Save video and ↻ Transcode both refuse while it is.
    pub rendering: bool,
    /// The base picture's own shape, unknown until something reads the file (`None`).
    pub base_shape: Option<(u32, u32)>,
    /// The finished video's shape. `(0, 0)` means NO SHAPE IS KNOWN YET — the page invents no default,
    /// because a guessed shape would draw a crop box over a frame that may not need one at all.
    pub video_shape: (u32, u32),
}

thread_local! {
    static PRODUCE_STATE: std::cell::RefCell<ProduceState> = const { std::cell::RefCell::new(ProduceState {
        frames: Vec::new(), own: false, texts: Vec::new(), title: String::new(),
        thumb_title: String::new(), title_seeded: false, instruction: String::new(),
        negative: String::new(), description: String::new(),
        container: String::new(), codec: String::new(), preset: String::new(),
        resolution: String::new(), frame_rate: String::new(), audio: String::new(),
        subtitles: String::new(), translate: Vec::new(), game_volume: 0.22, crf: 24.0,
        vfr: false, mono: false, blurred_edges: true,
        clips: 0, seconds: 0.0, narration_off: false, to_speak: 0, publish_written: false,
        session_language: String::new(),
        cut_line: String::new(), first_spoken: String::new(), mixed_sizes: Vec::new(),
        voice: String::new(),
        out_files: 0, out_bytes: 0, rendering: false,
        base_shape: None, video_shape: (0, 0),
    }) };
}

thread_local! {
    /// `refresh` writes widget values programmatically, and `set_selected` / `set_value` /
    /// `set_active` each EMIT their own change signal. Without this guard the page would answer its own
    /// repaint as if it were a user edit — the same trap `window.rs` threads `guard` through for the
    /// tab switch. Handlers check it first; `refresh` holds it across every programmatic write.
    static SETTINGS_GUARD: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Publish what the page should show.
pub fn set_state(next: ProduceState) {
    PRODUCE_STATE.with(|held| *held.borrow_mut() = next);
}

/// Read the page's current state back out.
pub fn read_state() -> ProduceState {
    PRODUCE_STATE.with(|held| held.borrow().clone())
}

fn mutate<T>(f: impl FnOnce(&mut ProduceState) -> T) -> T {
    PRODUCE_STATE.with(|held| f(&mut held.borrow_mut()))
}

// --- the surface -------------------------------------------------------------------------------------

thread_local! {
    static PAGE_CSS_LOADED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// The page's own ground colour, installed once per process the way `narrate_page` does it: a bare
/// `DrawingArea` paints nothing until something gives it a plate, and there is no `connect_draw`
/// handler anywhere in `src/`.
fn install_page_css_once() {
    if PAGE_CSS_LOADED.with(|cell| cell.get()) {
        return;
    }
    let css = ".produce-plate { background-color: #1c1c1d; border: 1px solid #000000; }\n\
               .produce-crop-box { border: 2px solid #d0a215; background-color: alpha(#d0a215, 0.15); }\n\
               .produce-word-box { border: 2px dashed #8a5fd0; }\n";
    let provider = gtk::CssProvider::new();
    provider.load_from_string(css);
    gtk::style_context_add_provider_for_display(
        &gtk::gdk::Display::default().expect("a display to style"),
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    PAGE_CSS_LOADED.with(|cell| cell.set(true));
}

/// A heading with its tooltip — every heading on this page carries §A's sentence for the group.
fn heading(text: &str, tip: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.add_css_class("heading");
    label.set_xalign(0.0);
    label.set_tooltip_text(Some(tip));
    label
}

/// A multi-line box of `lines` rows, framed, with §A's tooltip. Spec says 4 / 2 / 8 lines, so these are
/// `TextView`s rather than single-line `Entry`s.
fn text_box(name: &str, lines: i32, tip: &str) -> gtk::TextView {
    let view = gtk::TextView::new();
    view.set_widget_name(name);
    view.set_wrap_mode(gtk::WrapMode::WordChar);
    view.set_top_margin(4);
    view.set_bottom_margin(4);
    // `lines` visible rows: one line-height of request per row, which is what makes a "4-line box" be
    // four rows tall rather than whatever the column hands out.
    view.set_size_request(-1, 20 * lines);
    view.set_tooltip_text(Some(tip));
    view
}

/// **1** Add image… · the images row · **4** Edit instruction · **5** Negative prompt ·
/// **6** Thumbnail ⤓ ↻ · **7** the thumbnail picture.
fn build_left_column() -> gtk::Box {
    let left = gtk::Box::new(gtk::Orientation::Vertical, 8);
    // Named for the same reason as `produce-right`: §1 lists this column's parts in order and a geometry
    // test has to walk exactly this box.
    left.set_widget_name("produce-left");
    left.set_hexpand(true);

    // **1** the Images heading and ＋ Add image…, on one row as §A draws them.
    let images_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    images_row.append(&heading("Images", screen::IMAGES_HEADING_TIP));
    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    images_row.append(&spacer);
    let add = gtk::Button::with_label("Add image\u{2026}");
    add.set_widget_name("add-image-button");
    add.set_tooltip_text(Some(screen::EMPTY_IMAGES_HINT));
    images_row.append(&add);
    left.append(&images_row);

    // The slots themselves, rebuilt whole by `draw_slots`; the empty state lives in here too, because
    // §A prints it where the slots go.
    let slots = gtk::Box::new(gtk::Orientation::Vertical, 4);
    slots.set_widget_name("publish-images");
    left.append(&slots);

    // **4** Edit instruction (4 lines) and **5** Negative prompt (2 lines).
    left.append(&heading("Edit instruction", ""));
    left.append(&text_box(
        "edit-instruction",
        screen::INSTRUCTION_LINES as i32,
        screen::INSTRUCTION_TIP,
    ));
    left.append(&heading("Negative prompt", ""));
    left.append(&text_box(
        "negative-prompt",
        screen::NEGATIVE_LINES as i32,
        screen::NEGATIVE_TIP,
    ));

    // **6** the Thumbnail heading with ⤓ export and ↻ redraw beside it.
    let thumb_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    thumb_row.append(&heading("Thumbnail", ""));
    let thumb_spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    thumb_spacer.set_hexpand(true);
    thumb_row.append(&thumb_spacer);
    let export = gtk::Button::with_label("\u{2913}");
    export.set_widget_name("thumbnail-export");
    export.set_tooltip_text(Some(screen::EXPORT_TIP));
    thumb_row.append(&export);
    let redraw = gtk::Button::with_label("\u{21bb}");
    redraw.set_widget_name("thumbnail-redraw");
    redraw.set_tooltip_text(Some(screen::REDRAW_TIP));
    thumb_row.append(&redraw);
    left.append(&thumb_row);

    // **7** the picture, and the text-overlay editor that lives over it. A fixed-size plate so the
    // column cannot collapse it against an expanding sibling (the narrate page's frame lesson).
    // The plate goes inside an OVERLAY of exactly its own size: §A draws a word mark ON the thumbnail,
    // and a bare `DrawingArea` holds no children, so the dashed boxes had nowhere to live but the column
    // below the picture. The overlay is the plate's only parent, so the left column's child ORDER — which
    // §1 pins — keeps reading as the thumbnail itself.
    // §A: a word mark is visible ON the picture, not only as its chip below. The holder that can float
    // children over a `DrawingArea` (which holds none) is the overlay above. The NAME stays on the plate:
    // every §1 lookup and check measures the plate itself ("the picture wears the `.produce-plate` class"),
    // so a wrapper wearing it would answer an `Overlay` where a plate is expected. The wrapper is named for
    // itself instead, which no §1 list reads — §1's ORDER entry is this holder either way.
    let picture_holder = gtk::Overlay::new();
    picture_holder.set_widget_name("thumbnail-plate");
    picture_holder.set_vexpand(false);
    let picture = gtk::DrawingArea::new();
    picture.set_vexpand(false);
    picture.set_widget_name("thumbnail-picture");
    picture.set_widget_name("thumbnail-picture");
    picture.set_halign(gtk::Align::Fill);
    picture.set_size_request(320, 180);
    picture.add_css_class("produce-plate");
    picture.set_tooltip_text(Some(
        "Drag a box over the picture to put words on it \u{2014} they print to fill the box. Drag a border \
to resize, the middle to move, and press its \u{270e} to reword or remove it.",
    ));
    picture_holder.set_child(Some(&picture));
    // §A: a word mark is visible ON the picture, not only as its chip below. The holder that can float
    // children over a `DrawingArea` (which holds none) is the overlay above; §1 pins the left column's
    // child ORDER, so the column appends THAT holder rather than a wrapper around the plate.
    let marks = gtk::Box::new(gtk::Orientation::Vertical, 0);
    marks.set_widget_name("thumbnail-word-boxes");
    // The overlay slot the dashed boxes are appended to; empty until a mark exists, so it draws nothing.
    picture_holder.add_overlay(&marks);
    // Not measured: the holder must not push the plate's size around, it only floats over it.
    picture_holder.set_measure_overlay(&marks, false);
    left.append(&picture_holder);
    // The ✎ chips live in a named holder so `refresh` can rebuild them with the marks they stand for.
    let chips = gtk::Box::new(gtk::Orientation::Vertical, 2);
    chips.set_widget_name("thumbnail-word-chips");
    left.append(&chips);
    // §1's Produce page must fit §1's minimum window height with nothing scrolling as a whole
    // (`tests/suite/window_shrinks.rs` measures zero travel), so the column ends with a spacer that soaks up
    // whatever height is left rather than letting the last box stretch past the band.
    let filler = gtk::Box::new(gtk::Orientation::Vertical, 0);
    filler.set_vexpand(true);
    left.append(&filler);
    left
}

/// **8** Title + ↻ suggest · **9** YouTube description · **10** Transcode ⤓ ↻ ·
/// **11**–**14** the encoder settings scroller (its rows are built by
/// [`crate::ui::produce_page_layout`] with the pairing rule §A asks for) · **15** the Inputs readout.
fn build_right_column() -> gtk::Box {
    let right = gtk::Box::new(gtk::Orientation::Vertical, 8);
    // Named so a test can measure the column §1 describes ("Right column (at least 360 px wide)") rather
    // than guess at it from a grandchild's bounds.
    right.set_widget_name("produce-right");
    // §A: the written column has a minimum of 360 px, so the paned cannot starve it.
    right.set_size_request(360, -1);
    right.set_hexpand(true);

    // **8** Title with ↻ suggest beside it.
    let title_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    title_row.append(&heading("Title", ""));
    let title_spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    title_spacer.set_hexpand(true);
    title_row.append(&title_spacer);
    let suggest = gtk::Button::with_label("\u{21bb}");
    suggest.set_widget_name("title-suggest");
    suggest.set_tooltip_text(Some(screen::SUGGEST_TIP));
    title_row.append(&suggest);
    right.append(&title_row);
    let title = gtk::Entry::new();
    title.set_widget_name("title-entry");
    title.set_placeholder_text(Some(screen::TITLE_PLACEHOLDER));
    title.set_tooltip_text(Some(screen::TITLE_TIP));
    title.set_hexpand(true);
    right.append(&title);

    // **9** the YouTube description, eight lines.
    right.append(&heading("YouTube description", ""));
    right.append(&text_box(
        "description-box",
        screen::DESCRIPTION_LINES as i32,
        "What goes under the video on YouTube",
    ));

    // §1's separator between what a person WRITES and how the video is ENCODED: "the YouTube description
    // heading and box, a separator, then the encoder settings". Named because the spec lists it as one of
    // the page's widgets, so a test can point at it.
    let written_split = gtk::Separator::new(gtk::Orientation::Horizontal);
    written_split.set_widget_name("produce-written-split");
    right.append(&written_split);

    // **10** Transcode with ⤓ save and ↻ encode again.
    let transcode_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    transcode_row.append(&heading("Transcode", ""));
    let transcode_spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    transcode_spacer.set_hexpand(true);
    transcode_row.append(&transcode_spacer);
    let save = gtk::Button::with_label("\u{2913}");
    save.set_widget_name("transcode-save");
    save.set_tooltip_text(Some(screen::SAVE_VIDEO_TIP));
    transcode_row.append(&save);
    let again = gtk::Button::with_label("\u{21bb}");
    again.set_widget_name("transcode-again");
    again.set_tooltip_text(Some(screen::TRANSCODE_TIP));
    transcode_row.append(&again);
    right.append(&transcode_row);

    // **11**–**14** the thirteen settings rows, in a scroller as §A puts them.
    let scroller = gtk::ScrolledWindow::new();
    // `Automatic` on BOTH axes: the grid lays out several label+control pairs to a line (§A), so its natural
    // width is wider than a narrow window's right column, and with an `External` horizontal policy it simply
    // ran off the edge instead of scrolling. Automatic gives it a bar when the column is squeezed and full
    // width whenever the column has room — which is what makes the pairing measurable at all.
    scroller.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);
    // No minimum content WIDTH on purpose: a floor wider than the column is exactly what overflowed it. The
    // grid lays itself out at whatever width the column gives it, which is what makes the pairing measurable
    // and keeps items 11–14 inside the column instead of cut at its edge.
    // And room to be TALL: the page hosts this scroller inside `page-scroll-Produce`, whose own scroller
    // shrinks anything that expands (see `window_page_box`'s comment on why pages scroll), so with only
    // `vexpand` the grid got one row's height and six of its lines were invisible. §1 asks for the encoder
    // settings "in a scroller", so the grid's own height is what scrolls, not what the shell leaves over.
    scroller.set_min_content_height(GRID_MIN_H_PX);
    scroller.set_vexpand(true);
    let grid = gtk::Grid::new();
    grid.set_widget_name("encoder-settings");
    grid.set_row_spacing(6);
    grid.set_column_spacing(8);
    // Built un-wired here: `wire` attaches every control after `set_content`, because a handler put on
    // a widget that is not yet in the realized tree never fires (the same reason the Cut page attaches its
    // gestures after `set_content`).
    layout::build_settings(&grid, 0);
    scroller.set_child(Some(&grid));
    right.append(&scroller);

    // **15** the Inputs readout, and the Outputs count with its folder button.
    let readouts = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    readouts.set_halign(gtk::Align::End);
    let inputs = gtk::Label::new(Some(""));
    inputs.set_widget_name("produce-inputs");
    inputs.set_xalign(1.0);
    readouts.append(&inputs);
    let outputs = gtk::Label::new(Some(""));
    outputs.set_widget_name("produce-outputs");
    outputs.set_xalign(1.0);
    readouts.append(&outputs);
    let folder = gtk::Button::from_icon_name("folder-symbolic");
    folder.set_widget_name("produce-folder-button");
    folder.set_tooltip_text(Some(screen::FOLDER_TIP));
    readouts.append(&folder);
    right.append(&readouts);
    right
}

/// How tall the encoder scroller has to be to show the lines §A groups: five rows of controls plus their
/// spacing. Shorter and the last pairs hide behind the scroller's own bar instead of being drawn.
const GRID_MIN_H_PX: i32 = 240;

/// The height the page asks its column for, so the encoder scroller's [`GRID_MIN_H_PX`] is a FLOOR rather
/// than a wish. `page_box` hosts every page inside `page-scroll-<page>`, whose vertical policy is `Never`
/// (§1 wants nothing scrolling as a whole at §1's minimum size), so a child that only expands gets whatever
/// height is left over — and headless, where nothing maps the window before a snapshot takes its pixels,
/// what was left over was a sliver: the grid drew one line and the rest fell outside the shot. Asking for
/// the page's own content height makes GTK honour the floor in both cases; on a real window the ask is
/// below what the band gives, so it changes nothing there.
const PAGE_MIN_H_PX: i32 = 640;

/// §1's Produce split, opened at HALF: half of the 1240 px window §1 opens at, less the page's own 24 px
/// margins on both sides ((1240 − 48) / 2). The same arithmetic Prepare uses, stated here rather than
/// imported because it is THIS page's §1 clause, not Prepare's — and a shared constant would let one page's
/// window move the other's handle. A starting position only: dragging the handle replaces it.
const PRODUCE_HALF_PX: i32 = 596;

/// The page: horizontal paned, images/thumbnail left, written column and settings right (§A).
pub fn build() -> gtk::Widget {
    install_page_css_once();
    let split = gtk::Paned::new(gtk::Orientation::Horizontal);
    split.set_widget_name("produce-split");
    split.set_wide_handle(true);
    // §1: "split left and right by a draggable divider opened at HALF", not at 560 px. `shrink_*_child(true)`
    // on BOTH halves is what keeps the window shrinkable — a Paned that refuses to shrink forces each
    // column's own minimum width on the window, and `tests/suite/window_shrinks.rs` pins the shell's 1240 px
    // floor (the same reason `prepare-split` does this).
    split.set_shrink_start_child(true);
    split.set_shrink_end_child(true);
    let left = build_left_column();
    let right = build_right_column();
    // Both halves must TAKE the width the paned hands them, or the handle sits wherever their natural widths
    // leave it and "at half" is only true by accident.
    left.set_hexpand(true);
    right.set_hexpand(true);
    // No height floor here either: the shell's divider splits its leftover by share between expanding halves,
    // and a page that asks for 640 px of itself charges that to the page half of `main-divider`, which is what
    // squeezed the bottom band out of §1's window. The columns' own scrollers handle a short band.
    left.set_size_request(-1, PAGE_MIN_H_PX);
    right.set_size_request(-1, PAGE_MIN_H_PX);
    split.set_start_child(Some(&left));
    split.set_end_child(Some(&right));
    ROOT.with(|held| *held.borrow_mut() = Some(split.clone().upcast()));
    draw_into(&read_state());
    // Set after the children exist and after `draw_into`: the slot row is what gives the left column its
    // natural width, and a position set before that is measured against an empty column.
    split.set_position(PRODUCE_HALF_PX);
    split.upcast()
}

// --- the page's own lookups and output channels ----------------------------------------------------

pub fn widget_in_root(name: &str) -> Option<gtk::Box> {
    // The page is built once and held here so `draw_slots` can find its own box before the window has
    // a content root to walk from (the same reason the narrate page looks up from `window.content()`).
    ROOT.with(|held| held.borrow().clone())
        .and_then(|root| find(&root, name))
}

fn find(node: &gtk::Widget, name: &str) -> Option<gtk::Box> {
    if node.widget_name() == name {
        return node.clone().downcast::<gtk::Box>().ok();
    }
    if let Some(child) = node.first_child() {
        let mut cursor = Some(child);
        while let Some(current) = cursor {
            if let Some(found) = find(&current, name) {
                return Some(found);
            }
            cursor = current.next_sibling();
        }
    }
    None
}

thread_local! {
    /// The page's own root, remembered at `build()` so its seams can reach their widgets before any
    /// window exists. ONE per process: `build()` runs once and every later window embeds that same
    /// widget tree, which is why a second window cannot also claim it (see `wire`).
    static ROOT: std::cell::RefCell<Option<gtk::Widget>> = const { std::cell::RefCell::new(None) };
}

/// The window this page lives in, once `wire` has run. Slot buttons are attached through it.
pub fn held_window() -> Option<adw::ApplicationWindow> {
    ROOT.with(|held| held.borrow().clone())
        .and_then(|w| w.downcast::<adw::ApplicationWindow>().ok())
}

/// Publish a window as the page's live one WITHOUT rebuilding anything — a test's seam for the same slot `wire`
/// writes. `draw_into` finds its Images box through `held_window()`, so a check that drives its own window's
/// slots has to be the window held; without this, an earlier window left in the slot gets the repaint and the
/// rows the check reads back never change. Nothing here constructs widgets, so it cannot recurse into `build`.
/// Named `_for_test` because only a test asks: `wire` is the program's own door for this slot.
pub fn hold_window_for_test(window: &adw::ApplicationWindow) {
    ROOT.with(|held| *held.borrow_mut() = Some(window.clone().upcast()));
}

/// The page's own root, as `build()` published it — the tree every name lookup reaches, and therefore the one a
/// check must have repainted. `draw_into` prefers the held window over this root, so in a binary that builds a
/// fresh window per check the repaint lands on whichever window was built last unless the root is asked for by
/// name; `page_image_box_for_test` is how a test asks.
/// The page's own root, as `build()` published it (or the window `wire` put there) — the tree every name
/// lookup reaches. `page_image_box_for_test` is a box INSIDE this one.
pub fn page_root() -> Option<gtk::Widget> {
    ROOT.with(|held| held.borrow().clone())
}

pub fn page_image_box_for_test() -> Option<gtk::Widget> {
    page_root().and_then(|root| crate::ui::window::find_widget_by_name(&root, "publish-images"))
}

/// Find one widget by name from the window this page sits in.
pub fn widget_in(window: &adw::ApplicationWindow, name: &str) -> Option<gtk::Widget> {
    let content = window.content()?;
    fn walk(node: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
        if node.widget_name() == name {
            return Some(node.clone());
        }
        if let Some(child) = node.first_child() {
            let mut cursor = Some(child);
            while let Some(current) = cursor {
                if let Some(found) = walk(&current, name) {
                    return Some(found);
                }
                cursor = current.next_sibling();
            }
        }
        None
    }
    walk(&content, name)
}

fn label(window: &adw::ApplicationWindow, name: &str) -> Option<gtk::Label> {
    widget_in(window, name)?.downcast().ok()
}

/// Redraw the settings rows whole. A row's presence is part of what is being drawn — §F4.8 takes the
/// Game-audio row away with the narration tick — so `refresh` cannot only update values inside whatever
/// `build()` happened to draw first: the grid is emptied and refilled from `settings_rows_shown`, which
/// is the same door `build_settings` walks. Rebuilding also re-attaches nothing twice, because every
/// child is removed from the grid before any is added.
fn rebuild_settings(window: &adw::ApplicationWindow, _s: &ProduceState) {
    let Some(grid) =
        widget_in(window, "encoder-settings").and_then(|w| w.downcast::<gtk::Grid>().ok())
    else {
        return;
    };
    while let Some(child) = grid.first_child() {
        grid.remove(&child);
    }
    layout::build_settings(&grid, 0);
    // The fresh controls carry no handlers until this runs: rebuilding the grid would otherwise leave
    // every row decorative.
    wire_settings(window);
}

/// Say it on the status line and in the log, the page's two output channels.
fn say(window: &adw::ApplicationWindow, said: &str) -> String {
    if let Some(status) = crate::ui::window::find_status(window.upcast_ref()) {
        status.set_text(said);
    }
    crate::ui::window::log_line(said);
    said.to_string()
}

/// Repaint everything the held state owns: slots, the settings' values, the readouts.
pub fn refresh(window: &adw::ApplicationWindow) {
    let mut s = read_state();
    // §A's Game-audio row and §15's "no narration" clause are the same fact about the project, so they are
    // decided BEFORE anything is drawn: `build_settings` reads the held state, and a page that rebuilt its
    // rows from last minute's flag drew the slider beside a readout saying there is no narration. `None` (no
    // session to read) leaves whatever a flow published in place rather than claiming the folder is empty.
    if let Some(counts) = session_readouts(s.narration_off) {
        s.to_speak = counts.0;
        s.publish_written = screen::publish_written(s.publish_written, counts.1);
    }
    draw_into(&s);
    // `draw_into` rebuilt the slot rows whole, so their fresh `DrawingArea`s have no painter until this
    // runs again — and the thumbnail must repaint too, since its words or crop may just have changed.
    if let Some(win) = held_window() {
        crate::ui::produce_paint::install(&win);
    }
    // The ✎ chips belong to the marks, so they are redrawn by the same door that redraws the slots.
    crate::ui::produce_words_press::draw_word_chips(window, &s);
    // §A: a row's presence is itself drawn, so the grid is rebuilt whole rather than updated in place.
    // The new controls are seeded from the held state as they are created (`build_settings`), which is
    // why no guarded set-value pass is needed here: nothing emits a change signal during construction,
    // so a repaint cannot be mistaken for an edit.
    rebuild_settings(window, &s);
    // The four typed boxes are the exception: these `set_text` calls DO emit `changed`, and a keystroke
    // wire would otherwise answer our own repaint as an edit (and, for the title, re-enter `refresh`).
    SETTINGS_GUARD.with(|guard| guard.set(true));
    if let Some(entry) =
        widget_in(window, "title-entry").and_then(|w| w.downcast::<gtk::Entry>().ok())
    {
        entry.set_text(&s.title);
    }
    for (name, text) in [
        ("edit-instruction", &s.instruction),
        ("negative-prompt", &s.negative),
        ("description-box", &s.description),
    ] {
        if let Some(view) = widget_in(window, name).and_then(|w| w.downcast::<gtk::TextView>().ok())
        {
            view.buffer().set_text(text);
        }
    }
    SETTINGS_GUARD.with(|guard| guard.set(false));
    if let Some(inputs) = label(window, "produce-inputs") {
        inputs.set_text(&screen::inputs_line(
            s.clips,
            s.seconds,
            s.narration_off,
            s.to_speak,
            s.publish_written,
        ));
        // The tooltip answers "what exactly is behind that count": the clip range, the first line to
        // // be spoken, the voice in use, and whether text leaves the machine. Only what THIS session has
        // is stated; a thing the flows have not published yet contributes no row rather than a guess.
        let first_line = if s.first_spoken.is_empty() {
            None
        } else {
            Some(s.first_spoken.as_str())
        };
        let voice = if s.voice.is_empty() {
            None
        } else {
            Some(s.voice.as_str())
        };
        inputs.set_tooltip_text(Some(&screen::inputs_tooltip(
            &s.cut_line,
            first_line,
            &s.mixed_sizes,
            voice,
            s.publish_written,
        )));
    }
    if let Some(outputs) = label(window, "produce-outputs") {
        outputs.set_text(&screen::outputs_summary(s.out_files, s.out_bytes));
    }
}

/// §15's two session counts: how many narration lines still need to be spoken, and whether `publish.json`
/// exists. `None` when this working directory is no session at all — a page opened over a folder without a
/// `session.naivepost` has nothing to count, and keeps what a flow published rather than inventing zeros.
///
/// "Still needs to be spoken" is the prototype's own question (`updateInputs` counts `entries` whose
/// `ttsWav` is missing, and says "no narration" when there are no lines at all), asked here through the
/// stamp's seam: `wav_of` is where a take lives, so the count and the stamp can never disagree about which
/// lines are owed. Narration off is answered by the caller's own flag, not read here: the page holds it
/// (§A hides Game audio behind it) and `lines_to_speak` already returns nothing for it. The flag arrives
/// through [`note_narration_off`] rather than being read off disk here, so that the rows and the readout
/// cannot be built from two different answers.
fn session_readouts(narration_off: bool) -> Option<(usize, bool)> {
    let dir = crate::startup::session_dir(&std::env::current_dir().unwrap_or_default());
    let tree = crate::layout::Tree::new(&dir).ok()?;
    let lines = crate::narration::load(&tree)
        .map(|n| n.entries)
        .unwrap_or_default();
    let owed = crate::narrate_off::lines_to_speak(narration_off, &lines)
        .iter()
        .filter(|line| {
            crate::produce_stamp::wav_of(&tree, line, None, 0).is_none_or(|wav| !wav.exists())
        })
        .count();
    // §A's "no upload text" is the file's existence, and `publish::is_written` is that question already
    // asked (a present-but-unparseable record is still one the next ▶ overwrites).
    Some((owed, crate::publish::is_written(&tree)))
}

/// Hand the page the narration switch of the project this window is about (§F4.8, §08 §A).
///
/// The page does NOT read `naivepost.json` for this: `build_settings` hides the Game-audio row from the
/// held state and `inputs_line` spells its "no narration" clause from the same field, so a page that
/// learned the fact twice — once from a file probe, once from whatever a flow published — drew the slider
/// beside a readout saying there is no narration. One door, called by whoever takes a project on or moves
/// the tick, publishes it once.
pub fn note_narration_off(off: bool) {
    mutate(|state| state.narration_off = off);
}

// --- the seams: one widget press, one rules call, one printed sentence -------------------------------

/// **1** ＋ Add image… — never replaces anything, and a ninth picture is refused rather than dropping
/// one (§1: which frame was chosen first is not something an add should decide).
pub fn press_add_image(window: &adw::ApplicationWindow, path: &str) -> String {
    let added = mutate(|s| screen::add_image(&mut s.frames, path));
    if !added {
        return say(
            window,
            &format!(
                "{} images is the most the image model takes \u{2014} remove one first",
                screen::MAX_IMAGES
            ),
        );
    }
    refresh(window);
    say(window, &format!("image added: {}", short(path)))
}

/// Move one slot's picture to the front, where the image model edits it.
pub fn press_make_base(window: &adw::ApplicationWindow, index: usize) -> String {
    let moved = mutate(|s| {
        if index == 0 || index >= s.frames.len() {
            return false;
        }
        let path = s.frames.remove(index);
        s.frames.insert(0, path);
        true
    });
    if !moved {
        return say(window, "that slot is already the base");
    }
    refresh(window);
    say(
        window,
        "base changed \u{2014} the image model edits the first picture now",
    )
}

/// Swap one slot's picture for another file.
pub fn press_change_image(window: &adw::ApplicationWindow, index: usize, path: &str) -> String {
    let changed = mutate(|s| screen::change_image(&mut s.frames, index, path));
    if !changed {
        return say(window, "there is no such slot to change");
    }
    refresh(window);
    say(window, &format!("image changed: {}", short(path)))
}

/// Remove one slot. Removing the base makes the next picture the base, which is why the row is redrawn.
pub fn press_remove_image(window: &adw::ApplicationWindow, index: usize) -> String {
    let removed = mutate(|s| {
        if index >= s.frames.len() {
            return None;
        }
        Some(s.frames.remove(index))
    });
    let Some(path) = removed else {
        return say(window, "there is no such slot to remove");
    };
    refresh(window);
    if read_state().frames.is_empty() {
        return say(window, screen::EMPTY_IMAGES);
    }
    say(window, &format!("image removed: {}", short(&path)))
}

/// **3** Set Thumbnail: put this picture on the thumbnail as it is — a copy, cropped to the video's
/// shape, with no model call and no GPU (§A). Forwarder: the record-and-print lives in `produce_press_wiring`.
pub fn press_set_thumbnail(window: &adw::ApplicationWindow, index: usize) -> String {
    crate::ui::produce_press_wiring::set_thumbnail(window, index)
}

/// **6** ⤓ export: the JPEG an uploader takes, under its 2 MB limit. The weight it reports is measured off
/// the file the ladder wrote, so the widget hands over nothing but the press itself.
pub fn press_export_thumbnail(window: &adw::ApplicationWindow) -> String {
    crate::ui::produce_press_wiring::export_thumbnail(window)
}

/// **6** ↻ redraw: one sd.cpp call, nothing rewritten when the words are left alone.
pub fn press_redraw_thumbnail(window: &adw::ApplicationWindow, only_thumbnail: bool) -> String {
    crate::ui::produce_press_wiring::redraw_thumbnail(window, only_thumbnail)
}

/// **8** ↻ suggest: the only thing on this page that asks a model to rewrite text, and it rewrites all
/// three written things at once so they cannot disagree about what the video is. No draw: the picture stays.
pub fn press_suggest(window: &adw::ApplicationWindow) -> String {
    crate::ui::produce_press_wiring::reword(window)
}

/// **10** ↻ Transcode: an encode with no model call. Refused while a render runs.
pub fn press_transcode_again(window: &adw::ApplicationWindow) -> String {
    crate::ui::produce_press_wiring::transcode_again(window)
}

/// **10** Save video as the widget fires it — the project-named default for the destination, read by
/// `produce_press_wiring::save_video_from_button`.
pub fn press_save_video_from_button(window: &adw::ApplicationWindow) -> String {
    crate::ui::produce_press_wiring::save_video_from_button(window)
}

/// Store one row's value and answer with the sentence this press produced.
fn store_setting(row: &str, value: &str) -> String {
    mutate(|s| match row {
        "Container" => {
            let container = parse_container(value);
            let codec = parse_codec(&s.codec);
            let subtitles = parse_subtitles(&s.subtitles);
            let (forced_codec, forced_subs) =
                screen::apply_container_rules(container, codec, subtitles);
            s.container = container_name(container).to_string();
            s.codec = codec_name(forced_codec).to_string();
            s.subtitles = subtitles_name(forced_subs).to_string();
            if forced_codec != codec || forced_subs != subtitles {
                format!(
                    "container \u{2192}{} — it forces {} and no subtitle track",
                    s.container,
                    codec_name(forced_codec)
                )
            } else {
                format!("container \u{2192}{}", s.container)
            }
        }
        "Codec" => {
            s.codec = codec_name(parse_codec(value)).to_string();
            format!("codec {}", s.codec)
        }
        "Subtitles" => {
            s.subtitles = subtitles_name(parse_subtitles(value)).to_string();
            format!("subtitles {}", s.subtitles)
        }
        "Game audio" => {
            s.game_volume = value.parse().unwrap_or(s.game_volume);
            format!("game audio {:.2}", s.game_volume)
        }
        "Quality (CRF)" => {
            s.crf = value.parse().unwrap_or(s.crf);
            format!("quality crf {:.0}", s.crf)
        }
        other => {
            set_plain_row(s, other, value);
            format!("{other} {value}")
        }
    })
}

/// One setting changed on the row. Container carries §A's forcing rule: webm forces vp9 and turns
/// "track in file" into "none in the video", and what got forced is said rather than silently applied.
pub fn set_setting(window: &adw::ApplicationWindow, row: &str, value: &str) -> String {
    let said = store_setting(row, value);
    // The path the save dialog was OPENED on is this row's own offer, so a chooser that is never answered still
    // leaves the press naming the file the row asks for (`produce_press_wiring::asked_path` reads it back).
    if row == "Container" {
        crate::ui::produce_press_wiring::answer_path(None);
    }
    refresh(window);
    say(window, &said)
}

/// One of the three TICK rows changed (§A: Frame timing, Channels, Frame edges). A separate seam rather
/// than another arm of `set_setting`: a tick has no value string, and teaching `set_setting` to parse
/// "on"/"off" would give one function two kinds of input.
pub fn set_toggle(window: &adw::ApplicationWindow, row: &str, on: bool) -> String {
    mutate(|s| match row {
        "Frame timing" => s.vfr = on,
        "Channels" => s.mono = on,
        "Frame edges" => s.blurred_edges = on,
        _ => (),
    });
    refresh(window);
    say(window, &format!("{row} {}", if on { "on" } else { "off" }))
}

fn set_plain_row(s: &mut ProduceState, row: &str, value: &str) {
    match row {
        "Preset" => s.preset = value.to_string(),
        "Resolution" => s.resolution = value.to_string(),
        "Frame rate" => s.frame_rate = value.to_string(),
        "Audio" => s.audio = value.to_string(),
        _ => {}
    }
}

/// ✎ on a word box: reword it, or empty removes it (§1: an empty text is not a mark).
pub fn press_words_reword(window: &adw::ApplicationWindow, index: usize, text: &str) -> String {
    let removed = text.trim().is_empty();
    let done = mutate(|s| {
        if removed {
            return screen::remove_words(&mut s.texts, index);
        }
        match s.texts.get_mut(index) {
            Some(mark) => {
                screen::set_words(mark, text);
                true
            }
            None => false,
        }
    });
    if !done {
        return say(window, "no such word box on the thumbnail");
    }
    refresh(window);
    say(
        window,
        if removed {
            "words removed"
        } else {
            "words reworded"
        },
    )
}

/// ✎ remove on a word box. The others keep their order.
pub fn press_words_remove(window: &adw::ApplicationWindow, index: usize) -> String {
    let done = mutate(|s| screen::remove_words(&mut s.texts, index));
    if !done {
        return say(window, "no such word box on the thumbnail");
    }
    refresh(window);
    say(window, "words removed")
}

/// A box dragged onto the picture prints words into it. The gesture lives in the widget layer; it calls
/// THIS seam, which is the same `place_words` rule the logic tests drive, so a created box and an
/// edited one cannot drift.
pub fn press_words_place(
    window: &adw::ApplicationWindow,
    cx: f64,
    cy: f64,
    wf: f64,
    hf: f64,
    text: &str,
) -> String {
    mutate(|s| {
        let mut mark = crate::project::TextMark::default();
        screen::place_words(&mut mark, cx, cy, wf, hf, text);
        s.texts.push(mark);
    });
    refresh(window);
    say(window, &format!("words placed: {}", text.trim()))
}

// --- spelling helpers ------------------------------------------------------------------------------

fn short(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}

fn parse_container(value: &str) -> Container {
    match value {
        "mkv" => Container::Mkv,
        "webm" => Container::Webm,
        _ => Container::Mp4,
    }
}

fn parse_codec(value: &str) -> Codec {
    match value {
        "h265" => Codec::H265,
        "vp9" => Codec::Vp9,
        _ => Codec::H264,
    }
}

fn parse_subtitles(value: &str) -> Subtitles {
    match value {
        "burned in" => Subtitles::BurnedIn,
        "track in file" => Subtitles::TrackInFile,
        _ => Subtitles::None,
    }
}

pub(crate) fn container_name(c: Container) -> &'static str {
    match c {
        Container::Mp4 => "mp4",
        Container::Mkv => "mkv",
        Container::Webm => "webm",
    }
}

fn codec_name(c: Codec) -> &'static str {
    match c {
        Codec::H264 => "h264",
        Codec::H265 => "h265",
        Codec::Vp9 => "vp9",
    }
}

fn subtitles_name(s: Subtitles) -> &'static str {
    match s {
        Subtitles::BurnedIn => "burned in",
        Subtitles::TrackInFile => "track in file",
        Subtitles::None => "none in the video",
    }
}

/// The page's row as the settings struct the transcode log reads. Built here rather than read off the
/// project because the page holds the row's truth until a run saves it.
/// Mark the thumbnail as the user's own picture, with the current title as its printed line. The state half
/// of `produce_press_wiring::set_thumbnail`, kept here because the fields are this page's.
pub fn set_own_thumbnail() {
    mutate(|st| {
        st.own = true;
        st.thumb_title = st.title.clone();
    });
}

/// The other half of §F5.7's ↻ over the thumbnail: the picture goes back to the model, so it stops being the
/// user's own. `produce_press_wiring::legs` pairs this with the write that persists it.
pub fn clear_own_thumbnail() {
    mutate(|st| st.own = false);
}

/// The row's container word (`"mp4"`/`"mkv"`/`"webm"`), for a press that names the file it copies.
pub fn container_word() -> String {
    container_name(parse_container(&read_state().container)).to_string()
}

/// The row's container as the enum `save_default` takes.
pub fn container_of_row() -> Container {
    parse_container(&read_state().container)
}

/// The render's own output file, which is what a copy reads and weighs. Named here so the wiring module
/// asks the page rather than guessing the container spelling twice.
pub fn rendered_video() -> std::path::PathBuf {
    let dir = crate::startup::session_dir(&std::env::current_dir().unwrap_or_default());
    match crate::layout::Tree::new(dir) {
        Ok(tree) => tree.final_video(container_word().as_str()),
        Err(_) => std::path::PathBuf::from("produce/final.mp4"),
    }
}

/// The page's row as the settings struct the transcode log reads. Public for `produce_press_wiring`;
/// built here rather than read off the project because the page holds the row's truth until a run saves it.
pub fn row_settings() -> crate::project::Produce {
    produce_settings(&read_state())
}

/// The page's row as the settings struct the transcode log reads. Public for `produce_press_wiring`;
fn produce_settings(s: &ProduceState) -> crate::project::Produce {
    crate::project::Produce {
        container: parse_container(&s.container),
        codec: parse_codec(&s.codec),
        resolution: match s.resolution.as_str() {
            "720p" => crate::project::Resolution::P720,
            "original" => crate::project::Resolution::Original,
            _ => crate::project::Resolution::P1080,
        },
        game_volume: s.game_volume,
        crf: s.crf as u32,
        vfr: s.vfr,
        mono: s.mono,
        blurred_edges: s.blurred_edges,
        // The ticked languages travel with the settings, or the render never learns that a translation
        // was asked for (§F5.4 S7/S9 write one sidecar per language, in this order).
        translate: s.translate.clone(),
        ..Default::default()
    }
}

/// Store one typed box's whole text, under the widget's own name. Verbatim: §1 gives no trimming, case
/// or length rule for any of the four, and an instruction or a description is prose whose spacing,
/// leading blank line and trailing space belong to whoever wrote them — so the machine keeps the bytes.
/// Store one typed box's whole text, under the widget's own name. The page's seam for the four typed
/// boxes: `produce_text`'s wires call it, and a test can check the storing rule without a display.
/// Verbatim: §1 gives no trimming, case or length rule for any of the four, and an instruction or a
/// description is prose whose spacing, leading blank line and trailing space belong to whoever wrote them
/// — so the machine keeps the bytes rather than inventing a tidy-up of its own.
pub fn set_text_field(field: &str, text: &str) -> String {
    let stored = mutate(|s| {
        match field {
            "edit-instruction" => {
                s.instruction = text.to_string();
            }
            "negative-prompt" => {
                s.negative = text.to_string();
            }
            "description-box" => {
                s.description = text.to_string();
            }
            // The title is stored like the rest; whether it repaints the picture is `title_seeded`'s call.
            "title-entry" => {
                s.title = text.to_string();
            }
            _ => return false,
        }
        true
    });
    if stored {
        "".to_string()
    } else {
        "no such text box on this page".to_string()
    }
}

/// Attach every handler. Called from `build_window` after `set_content`, like every other control on
/// every page: a handler attached before the widget is inside the realized tree never fires.
///
/// ONE PAGE, ONE WINDOW. `build()` runs once per window and each build publishes ITS OWN root, so a
/// second window gets its own widgets and its own wires — which is what makes this safe to call again.
pub fn wire(window: &adw::ApplicationWindow) {
    // Hold the WINDOW, not just the content root: the slot row needs it to attach its per-slot buttons,
    // which are looked up by name from the window root.
    ROOT.with(|held| *held.borrow_mut() = Some(window.clone().upcast()));
    let win = window.clone();
    for name in [
        "add-image-button",
        "title-suggest",
        "thumbnail-export",
        "thumbnail-redraw",
        "transcode-save",
        "transcode-again",
    ] {
        if let Some(button) = crate::ui::line_step_button(&win, name) {
            let w = win.clone();
            let owned = name.to_string();
            button.connect_clicked(move |_| match owned.as_str() {
                "add-image-button" => {
                    // The stand-in for the file chooser: a real extracted frame when the session has one, so
                    // the row gains a picture the crop and the request can open (`produce_slots::real_frame_entry`).
                    // With nothing extracted there is no chooser to answer, but §A's Add image still hands a
                    // picture back — the row takes the next name it would have shown. A slot's Change… is
                    // stricter (it swaps one picture for another, and with nothing on disk it refuses with
                    // NO_FRAME_TO_ADD); an empty row has nothing to swap, only something to add.
                    let next = crate::ui::produce_slots::real_frame_entry().unwrap_or_else(|| {
                        format!(
                            "project:produce/publish/frame-{}.jpg",
                            read_state().frames.len() + 1
                        )
                    });
                    press_add_image(&w, &next);
                }
                "title-suggest" => {
                    press_suggest(&w);
                }
                "thumbnail-export" => {
                    press_export_thumbnail(&w);
                }
                "thumbnail-redraw" => {
                    press_redraw_thumbnail(&w, true);
                }
                "transcode-save" => {
                    press_save_video_from_button(&w);
                }
                _ => {
                    press_transcode_again(&w);
                }
            });
        }
    }
    // The thirteen settings rows, wired before the slot buttons because `refresh` below rebuilds the
    // grid whole and the wires must exist by the time a person touches a control.
    wire_settings(&win);
    // §1 items 4/5/8/9: the four boxes a person TYPES into. Same hand-over as the Translate row — the
    // guard and the state mutation stay this file's, so `produce_text` touches no thread-local of ours.
    let text_win = win.clone();
    crate::ui::produce_text::wire_text(
        &win,
        std::rc::Rc::new(|| SETTINGS_GUARD.with(|guard| guard.get())),
        std::rc::Rc::new(move |field, text| {
            set_text_field(field, text);
            refresh(&text_win);
        }),
    );
    // The overlay editor's drag lives on the picture itself; the ✎ chips arrive with each refresh.
    crate::ui::produce_words_press::install_words_gesture(&win);
    // §2 and §7: a plate is a flat CSS rectangle until someone paints it. Installed here, once — but
    // `draw_slots` rebuilds the slot widgets whole, so `refresh` installs again for the new ones.
    crate::ui::produce_paint::install(&win);
    refresh(&win);
}

/// Attach every settings control to `set_setting` / `set_toggle`, so §1's items 11–14 reach the rules
/// from the widgets and not only from a test. Each handler is the same three steps: bail when the guard
/// says this emission is our own repaint, read what the widget now holds, hand it to the seam that owns
/// the rule (including webm's forcing).
fn wire_settings(window: &adw::ApplicationWindow) {
    for row in [
        "Container",
        "Codec",
        "Preset",
        "Resolution",
        "Frame rate",
        "Audio",
        "Subtitles",
    ] {
        let Some(picker) =
            widget_in(window, &row_widget(row)).and_then(|w| w.downcast::<gtk::DropDown>().ok())
        else {
            continue;
        };
        let w = window.clone();
        let row = row.to_string();
        picker.connect_selected_notify(move |picker| {
            if SETTINGS_GUARD.with(|guard| guard.get()) {
                return;
            }
            let at = picker.selected() as usize;
            let Some(value) = screen::options(&row).and_then(|list| list.get(at)).copied() else {
                return;
            };
            set_setting(&w, &row, value);
        });
    }
    for row in ["Game audio", "Quality (CRF)"] {
        let Some(scale) =
            widget_in(window, &row_widget(row)).and_then(|w| w.downcast::<gtk::Scale>().ok())
        else {
            continue;
        };
        let w = window.clone();
        let row = row.to_string();
        scale.connect_value_changed(move |scale| {
            if SETTINGS_GUARD.with(|guard| guard.get()) {
                return;
            }
            set_setting(&w, &row, &scale.value().to_string());
        });
    }
    for (row, name) in [
        ("Frame timing", "tick-peak-rate-vfr"),
        ("Channels", "tick-mono"),
        ("Frame edges", "tick-blurred"),
    ] {
        let Some(tick) =
            widget_in(window, name).and_then(|w| w.downcast::<gtk::CheckButton>().ok())
        else {
            continue;
        };
        let w = window.clone();
        let row = row.to_string();
        tick.connect_toggled(move |tick| {
            if SETTINGS_GUARD.with(|guard| guard.get()) {
                return;
            }
            set_toggle(&w, &row, tick.is_active());
        });
    }
    // The Translate row's ticks live in `produce_languages` (this file is at its size budget); the
    // guard and the state mutation are handed over as shared closures so that module stays out of this
    // one's thread-locals.
    let w = window.clone();
    crate::ui::produce_languages::wire_ticks(
        window,
        std::rc::Rc::new(|| SETTINGS_GUARD.with(|guard| guard.get())),
        std::rc::Rc::new(move |language, on| {
            mutate(|s| crate::ui::produce_languages::apply(s, language, on));
            refresh(&w);
            let said = crate::ui::produce_languages::status_line(language, on);
            say(&w, &said);
        }),
    );
}

/// The row's widget name, the page's ONE spelling: "Quality (CRF)" → `setting-quality--crf`,
/// "Translate" → `setting-translate`. A test finds any row by its §A name without knowing whether that
/// row is a dropdown, a scale or a box of ticks.
/// The row's widget name, the page's ONE spelling: "Quality (CRF)" → `setting-quality--crf`,
/// "Translate" → `setting-translate`. A test finds any row by its §A name without knowing whether that
/// row is a dropdown, a scale or a box of ticks.
pub fn row_widget(row: &str) -> String {
    format!(
        "setting-{}",
        row.to_lowercase()
            .replace(['(', ')', ' '], "-")
            .trim_matches('-')
    )
}

/// Draw the Images row into its holder and wire whatever buttons the drawing created. Called from
/// `build` (before any window exists) and from `refresh`; the slots' rules live in `produce_slots`.
pub(crate) fn draw_into(s: &ProduceState) {
    // A box that is being drawn into again still holds what the last draw left in it: `build()` runs once
    // per WINDOW, so a second window's page has its own holder while ROOT still points at the first one's,
    // and emptying the wrong box leaves the live one to accumulate rows -- GTK's
    // `gtk_box_append: assertion 'gtk_widget_get_parent (child) == NULL'`. So the holder is found through
    // the window being refreshed when there is one, and only falls back to ROOT for the draw `build()` does
    // before any window exists.
    let list = match held_window()
        .and_then(|w| widget_in(&w, "publish-images"))
        .and_then(|w| w.downcast::<gtk::Box>().ok())
    {
        Some(list) => list,
        None => match widget_in_root("publish-images") {
            Some(list) => list,
            None => return,
        },
    };
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
    let doors = crate::ui::produce_slots::Doors {
        make_base: std::rc::Rc::new(|w, i| {
            press_make_base(w, i);
        }),
        set_thumbnail: std::rc::Rc::new(|w, i| {
            press_set_thumbnail(w, i);
        }),
        // Change… stands in for the file chooser by naming the newest frame the session holds, or a
        // synthesised one when nothing is extracted: the slot always ends up holding a different picture.
        change: std::rc::Rc::new(|w, i| {
            let stood_in_for = crate::ui::produce_slots::next_stood_in();
            press_change_image(w, i, &stood_in_for);
        }),
        remove: std::rc::Rc::new(|w, i| {
            press_remove_image(w, i);
        }),
    };
    crate::ui::produce_slots::draw(&list, s, held_window().as_ref(), &doors);
}

/// The F5.1 run, driven from the page. Kept here rather than in `window.rs` so the shell's play handler
/// stays a forwarder (spec/00-principles.md §5): every decision is `produce_flow`'s, every string comes
/// from it too, and this only gathers what the page holds and writes the answers back.
///
/// The two halves arrive as closures because headless there is no sd.cpp and no ffmpeg pipeline to call:
/// a soft words failure and a passing render are what a test can assert against without a server. When the
/// real F5.6 words and F5.2 render land in their own rounds they replace those two closures and nothing
/// else in this function changes.
pub fn press_produce_run(window: &adw::ApplicationWindow) -> String {
    let s = read_state();
    let project = crate::ui::window::live_project();
    let root = std::env::current_dir().unwrap_or_default();
    let tree = crate::layout::Tree::new(crate::startup::session_dir(&root)).ok();
    // S3: ONE function answers "what is the cut", and the Cut page's own segments win so a tweak that
    // was never saved still renders. `review_cut_of` is that live cut.
    let cut = match tree.as_ref() {
        Some(tree) => {
            crate::produce_flow::what_is_the_cut(Some(crate::ui::review_cut_of(window)), tree)
        }
        None => crate::ui::review_cut_of(window),
    };
    // S1: nothing to render. The shell refuses before the bar opens; this is the page's own copy of that
    // answer for the door that calls the seam directly.
    if cut.segs.is_empty() {
        return say(window, crate::cut::NO_CUT_YET);
    }
    let settings = produce_settings(&s);
    let Some(tree) = tree.as_ref() else {
        // No project folder means no stamp and no video to ask about: encode.
        return finish_produce(
            window,
            &s,
            &project,
            cut,
            settings,
            crate::produce_flow::Gate::Encode,
        );
    };
    // S2: up to date is the stamp's question, and it is asked over the narration too — §F5.3 lists the
    // lines (bounds, text, wav size and mtime) among what the hash covers, so `gate_stamp` reads
    // `narrate/narration.json` itself rather than hashing an empty list and calling that cheap. It also
    // takes the whole cut and the footage frame, because those are what the encoder is handed beside the
    // segments; the shape is measured by the same helper the snapshot below uses, so gate and run agree.
    let voice = if s.voice.is_empty() {
        None
    } else {
        Some(s.voice.as_str())
    };
    let current = crate::produce_stamp::gate_stamp(
        tree,
        &settings,
        &cut,
        &project.sources,
        voice,
        project.no_narration,
        crate::ui::produce_snapshot::first_footage_shape(
            tree,
            &crate::ui::cut_folds::page_recordings(),
        ),
    );
    let gate = crate::produce_flow::gate(
        crate::produce_stamp::skip_encode(
            crate::produce_stamp::read_stamp(tree).as_deref(),
            &current,
        ),
        tree.final_video(container_name(parse_container(&s.container)))
            .exists(),
    );
    finish_produce(window, &s, &project, cut, settings, gate)
}

/// The S2 answer acted on and S4–S6 run. Split out only so the `Tree` borrow above ends before the run.
///
/// A `ConfirmOverwrite` gate does NOT stop at the question: §S2 asks in order to be answered, so the
/// dialog opens here and the encode runs from its response handler when the answer is Overwrite (and
/// stops, with the question left said, when it is Cancel or ×). The rest of the press therefore lives in
/// `encode_and_finish`, which both the straight-through path and the answered path call.
fn finish_produce(
    window: &adw::ApplicationWindow,
    s: &ProduceState,
    project: &crate::project::Project,
    cut: crate::cut::Cut,
    settings: crate::project::Produce,
    gate: crate::produce_flow::Gate,
) -> String {
    // S2: a stale video standing here gets the overwrite question, in §S2's own two paragraphs. Asking
    // writes nothing; only the answer's Overwrite runs the encode.
    if gate == crate::produce_flow::Gate::ConfirmOverwrite {
        let root = std::env::current_dir().unwrap_or_default();
        let video = crate::layout::Tree::new(crate::startup::session_dir(&root))
            .map(|t| t.final_video(container_name(parse_container(&s.container))))
            .unwrap_or_default();
        let (win, st, prj, ct, set) = (
            window.clone(),
            s.clone(),
            project.clone(),
            cut.clone(),
            settings.clone(),
        );
        return crate::ui::produce_confirm::confirm_then(window, &video, move || {
            encode_and_finish(
                &win,
                &st,
                &prj,
                &ct,
                &set,
                crate::produce_flow::Gate::Encode,
            );
        });
    }
    encode_and_finish(window, s, project, &cut, &settings, gate)
}

/// S3's snapshot and S4–S6, reached by the straight-through paths and by an answered confirmation.
fn encode_and_finish(
    window: &adw::ApplicationWindow,
    s: &ProduceState,
    project: &crate::project::Project,
    cut: &crate::cut::Cut,
    settings: &crate::project::Produce,
    gate: crate::produce_flow::Gate,
) -> String {
    if gate == crate::produce_flow::Gate::Skip {
        // F5.5 S5 runs on the skip path too: the page is rewritten even when the encode was skipped.
        crate::ui::produce_languages::tag_page(settings);
        return say(window, crate::produce_flow::SKIP_LOG);
    }
    // S3: EVERYTHING the two halves read is gathered here, once, on the GUI thread. From this line on the
    // run asks `snap` and never the page: a render walks subprocesses for minutes, and a leg that re-read
    // the window inside the run would describe whatever had been moved in the meantime.
    let snap = crate::ui::produce_snapshot::gather(
        &session_tree(),
        cut,
        settings,
        project,
        &crate::ui::cut_folds::page_recordings(),
        crate::ui::produce_snapshot::publish_of_state(s),
    );
    let run = crate::produce_flow::Run {
        cut: cut.clone(),
        // S3: the run is built from the snapshot, so the narration the session holds travels with it. With
        // nothing in `lines` the words half has no clip to caption and §F5.2 S2's "speaking" step is vacuous.
        lines: snap.lines.clone(),
        // Cloned rather than moved: F5.5 S5 runs after the render and needs the container and codec to
        // ask whether the result is something a browser plays.
        settings: settings.clone(),
        sources: crate::run::snapshot_sources(project),
        // The page's rows merged with what `publish.json` already recorded: the crop, the title box and the
        // image row have no page row to spell them, so a run that carried the rows alone would redraw a
        // thumbnail from a record missing its own crop.
        publish: Some(crate::ui::produce_snapshot::publish_of(&snap)),
        aspect: String::new(),
        clips: s.clips,
        publish_written: s.publish_written,
        voice: s.voice.clone(),
        no_narration: project.no_narration,
    };
    let run = crate::produce_flow::Run {
        aspect: run.cut.aspect.clone(),
        ..run
    };
    // S4: the one opening line, logged before either half starts.
    crate::ui::window::log_line(&crate::produce_flow::opening_line(&run, false));
    // S5/S6: both halves through `run_with`, so the closing line and the bar's progress text come from
    // one place. The words half is F5.6's; its failure is logged with the spec's own tail and never stops
    // the render. The render half is F5.2's, run here by `produce_exec`: it plans the whole run, walks
    // the subprocesses and reports back what it made.
    let ending = crate::produce_flow::run_with(
        &run,
        |run| match words_half(run) {
            Ok(()) => (true, None),
            Err(why) => {
                crate::ui::window::log_line(&crate::produce_flow::words_failed(&why));
                (false, Some(why))
            }
        },
        |run| crate::ui::produce_snapshot::render_half(run, &snap),
    );
    // F5.5 S5: after both halves; the page's own outcome is an input to nothing (S6 keeps the render's
    // error as the run's verdict).
    crate::ui::produce_languages::tag_page(settings);
    crate::ui::window::log_line(&ending.log);
    if !ending.progress.is_empty() {
        crate::ui::window::log_line(&ending.progress);
    }
    say(window, ending.status)
}
