//! §08-produce#1-screen: the Produce page's surface, §1's fifteen numbered widgets laid out per
//! spec/inventory/produce.md §A. No rule lives here: options, tooltips and sentences come from
//! [`crate::produce_screen`] and [`crate::produce_runs`] (spec/00-principles.md §5).
//! The page renders a state the flows publish through [`set_state`], like `narrate_page`.

use adw::prelude::*;
use gtk4 as gtk;

use crate::narrate_preview_media::source_name as short;
use crate::produce_flow::codec_name;
pub(crate) use crate::produce_flow::container_name;
use crate::produce_screen as screen;
use crate::project::{Codec, Container, Subtitles};
use crate::ui::narrate_page::say;
use crate::ui::produce_page_layout as layout;
use crate::ui::produce_page_rows::{session_tree, words_half};

#[derive(Debug, Clone, Default)]
pub struct ProduceState {
    pub frames: Vec<String>,
    pub own: bool,
    pub texts: Vec<crate::project::TextMark>,
    pub title: String,
    pub thumb_title: String,
    pub title_seeded: bool,
    pub title_box: Option<crate::project::TitleBox>,
    pub instruction: String,
    pub negative: String,
    pub description: String,
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
    pub clips: usize,
    pub seconds: f64,
    pub to_speak: usize,
    pub publish_written: bool,
    pub session_language: String,
    /// The cut's own sentence ("N clip(s) from mm:ss to mm:ss"), empty before a cut exists.
    pub cut_line: String,
    pub first_spoken: String,
    /// Separate recordings mixed into each clip's own sound, by base name (the Inputs tooltip).
    pub mixed_recordings: Vec<String>,
    pub voice: String,
    pub out_files: usize,
    pub out_bytes: u64,
    pub base_shape: Option<(u32, u32)>,
    /// The finished video's shape; `(0, 0)` means none known yet: no guessed shape, which would draw a needless crop box.
    pub video_shape: (u32, u32),
}

thread_local! {
    static PRODUCE_STATE: std::cell::RefCell<ProduceState> = const { std::cell::RefCell::new(ProduceState {
        frames: Vec::new(), own: false, texts: Vec::new(), title: String::new(),
        thumb_title: String::new(), title_seeded: false, title_box: None, instruction: String::new(),
        negative: String::new(), description: String::new(),
        container: String::new(), codec: String::new(), preset: String::new(),
        resolution: String::new(), frame_rate: String::new(), audio: String::new(),
        subtitles: String::new(), translate: Vec::new(), game_volume: 0.22, crf: 24.0,
        vfr: false, mono: false, blurred_edges: true,
        clips: 0, seconds: 0.0, to_speak: 0, publish_written: false,
        session_language: String::new(),
        cut_line: String::new(), first_spoken: String::new(), mixed_recordings: Vec::new(),
        voice: String::new(),
        out_files: 0, out_bytes: 0,
        base_shape: None, video_shape: (0, 0),
    }) };
}

thread_local! {
    /// `set_selected` / `set_value` / `set_active` emit their own change signals; handlers check this first so the page does not answer its own repaint as an edit.
    static SETTINGS_GUARD: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

pub fn set_state(next: ProduceState) {
    PRODUCE_STATE.with(|held| *held.borrow_mut() = next);
}

pub fn read_state() -> ProduceState {
    PRODUCE_STATE.with(|held| held.borrow().clone())
}

fn mutate<T>(f: impl FnOnce(&mut ProduceState) -> T) -> T {
    PRODUCE_STATE.with(|held| f(&mut held.borrow_mut()))
}

thread_local! {
    static PAGE_CSS_LOADED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Installs the page's ground colour once per process (a bare `DrawingArea` paints nothing), as `narrate_page`.
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

fn heading(text: &str, tip: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.add_css_class("heading");
    label.set_xalign(0.0);
    label.set_tooltip_text(Some(tip));
    label
}

/// A framed multi-line box (`TextView`, since §A asks for 4 / 2 / 8 lines) with §A's tooltip.
fn text_box(name: &str, lines: i32, tip: &str) -> gtk::TextView {
    let view = gtk::TextView::new();
    crate::ui::margins::text_box(&view);
    view.set_widget_name(name);
    view.set_wrap_mode(gtk::WrapMode::WordChar);
    view.set_size_request(-1, 20 * lines);
    view.set_tooltip_text(Some(tip));
    view
}

fn build_left_column() -> gtk::Box {
    let left = gtk::Box::new(gtk::Orientation::Vertical, 8);
    // Named so a geometry test can walk exactly this box (§1 lists the column's parts in order).
    left.set_widget_name("produce-left");
    left.set_hexpand(true);

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

    let slots = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    slots.set_homogeneous(true);
    slots.set_widget_name("publish-images");
    left.append(&slots);

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

    // **7** the picture in an Overlay of exactly its own size, so word marks can float ON it (a `DrawingArea` holds no
    // children). The NAME stays on the plate: §1 lookups measure the plate itself (`.produce-plate`).
    let picture_holder = gtk::Overlay::new();
    picture_holder.set_widget_name("thumbnail-plate");
    picture_holder.set_vexpand(true);
    let picture = gtk::DrawingArea::new();
    picture.set_vexpand(true);
    picture.set_widget_name("thumbnail-picture");
    picture.set_halign(gtk::Align::Fill);
    picture.set_hexpand(true);
    // Height only: a WIDTH here made the left column wider than its half (see `produce_slots::SLOT_PICTURE_MIN_PX`).
    picture.set_size_request(-1, PLATE_MIN_H_PX);
    picture.add_css_class("produce-plate");
    picture.set_tooltip_text(Some(
        "Drag a box over the picture to put words on it \u{2014} they print to fill the box. Drag a border \
to resize, the middle to move, and press its \u{270e} to reword or remove it.",
    ));
    picture_holder.set_child(Some(&picture));
    crate::ui::produce_paint::follow_plate(&picture);
    // A `Fixed`, since each box sits at its own x/y: in a stacking box a second mark was placed under the first.
    let marks = gtk::Fixed::new();
    marks.set_widget_name("thumbnail-word-boxes");
    picture_holder.add_overlay(&marks);
    picture_holder.set_measure_overlay(&marks, false);
    // Not a target: otherwise every press on the thumbnail landed here and the plate's drag-a-box gesture never fired.
    marks.set_can_target(false);
    left.append(&picture_holder);
    let chips = gtk::Box::new(gtk::Orientation::Vertical, 2);
    chips.set_widget_name("thumbnail-word-chips");
    left.append(&chips);
    left
}

fn build_right_column() -> gtk::Box {
    let right = gtk::Box::new(gtk::Orientation::Vertical, 8);
    // Named so a test can measure §1's "Right column (at least 360 px wide)".
    right.set_widget_name("produce-right");
    right.set_size_request(360, -1);
    right.set_hexpand(true);

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

    // **9** takes the column's spare height (§1: the box runs down to the separator).
    right.append(&heading("YouTube description", ""));
    let description = text_box(
        "description-box",
        screen::DESCRIPTION_LINES as i32,
        "What goes under the video on YouTube",
    );
    // A bare text view asks for all its text's height, so a long description raised the page's minimum and the divider could not move up.
    let description_scroller = gtk::ScrolledWindow::new();
    description_scroller.set_widget_name("description-scroller");
    description_scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    description_scroller.set_min_content_height(20 * screen::DESCRIPTION_LINES as i32);
    description_scroller.set_child(Some(&description));
    description_scroller.set_vexpand(true);
    right.append(&description_scroller);

    let written_split = gtk::Separator::new(gtk::Orientation::Horizontal);
    written_split.set_widget_name("produce-written-split");
    // 12 px each side like every line on every page: the column's 8 px spacing and these 4.
    written_split.set_margin_top(crate::ui::margins::DIVIDER_GAP_PX - 8);
    written_split.set_margin_bottom(crate::ui::margins::DIVIDER_GAP_PX - 8);
    right.append(&written_split);

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

    // Natural height, not expanding: the description above takes the slack (§1's picture); expanding drew an empty band under the last line.
    let scroller = gtk::ScrolledWindow::new();
    scroller.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);
    scroller.set_propagate_natural_height(true);
    let settings = gtk::Box::new(gtk::Orientation::Vertical, 6);
    settings.set_widget_name("encoder-settings");
    // Wired after `set_content`: a handler on a widget outside the realized tree never fires.
    layout::build_settings(&settings);
    scroller.set_child(Some(&settings));
    right.append(&scroller);
    // §1's **15** is the window's bottom bar (painted by `Shell::inputs` / `Shell::outputs`); a copy here disagreed with it.
    right
}

/// §A "min 120". A height, never a width: a width here is a minimum the left column cannot shrink below and clipped its headings.
const PLATE_MIN_H_PX: i32 = 180;

/// Floor on the page's height: `page-scroll-<page>` never scrolls vertically, and headless, before the window is
/// mapped, a merely-expanding child got a sliver (the grid drew one line).
const PAGE_MIN_H_PX: i32 = 640;

/// §1's split at HALF: (1240 − 48) / 2, as Prepare, but not shared so one page's window cannot move the other's handle.
const PRODUCE_HALF_PX: i32 = 596;

pub fn build() -> gtk::Widget {
    install_page_css_once();
    let split = gtk::Paned::new(gtk::Orientation::Horizontal);
    split.set_widget_name("produce-split");
    split.set_wide_handle(false);
    // §1: opened at HALF. `shrink_*_child(true)` on BOTH halves keeps the window shrinkable to §1's 1240 px floor
    // (`tests/suite/window_shrinks.rs`).
    split.set_shrink_start_child(true);
    split.set_shrink_end_child(true);
    let left = build_left_column();
    let right = build_right_column();
    left.set_hexpand(true);
    right.set_hexpand(true);
    // No height floor on the whole page: 640 px charged to the page half of `main-divider` squeezed the bottom band out of §1's window.
    left.set_size_request(-1, PAGE_MIN_H_PX);
    right.set_size_request(-1, PAGE_MIN_H_PX);
    split.set_start_child(Some(&left));
    split.set_end_child(Some(&right));
    ROOT.with(|held| *held.borrow_mut() = Some(split.clone().upcast()));
    draw_into(&read_state());
    // After `draw_into`: the slot row gives the left column its natural width.
    split.set_position(PRODUCE_HALF_PX);
    split.upcast()
}

pub fn widget_in_root(name: &str) -> Option<gtk::Box> {
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
    /// The page's root, remembered at `build()`; ONE per process, so a second window cannot also claim it (see `wire`).
    static ROOT: std::cell::RefCell<Option<gtk::Widget>> = const { std::cell::RefCell::new(None) };
}

pub fn held_window() -> Option<adw::ApplicationWindow> {
    ROOT.with(|held| held.borrow().clone())
        .and_then(|w| w.downcast::<adw::ApplicationWindow>().ok())
}

/// Test seam: publish a window as the page's live one without rebuilding. `draw_into` finds the Images box through
/// `held_window()`, so a check driving its own window must be the one held.
pub fn hold_window_for_test(window: &adw::ApplicationWindow) {
    ROOT.with(|held| *held.borrow_mut() = Some(window.clone().upcast()));
}

/// The page's root as `build()` published it (or the window `wire` put there); `page_image_box_for_test` is a box inside it.
pub fn page_root() -> Option<gtk::Widget> {
    ROOT.with(|held| held.borrow().clone())
}

pub fn page_image_box_for_test() -> Option<gtk::Widget> {
    page_root().and_then(|root| crate::ui::window::find_widget_by_name(&root, "publish-images"))
}

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

/// Redraw the settings rows whole: a row's presence is drawn too (§F4.8 removes the Game-audio row with the narration tick).
fn rebuild_settings(window: &adw::ApplicationWindow, _s: &ProduceState) {
    let Some(settings) =
        widget_in(window, "encoder-settings").and_then(|w| w.downcast::<gtk::Box>().ok())
    else {
        return;
    };
    while let Some(child) = settings.first_child() {
        settings.remove(&child);
    }
    layout::build_settings(&settings);
    wire_settings(window);
    let set_by = encoder_profile();
    settings.set_sensitive(true);
    settings.set_tooltip_text(
        set_by
            .map(|name| {
                format!(
                    "Started from the profile {name} \u{2014} a change here is this project's own"
                )
            })
            .as_deref(),
    );
}

pub fn refresh(window: &adw::ApplicationWindow) {
    let mut s = read_state();
    // §15's counts are read BEFORE drawing; `None` keeps what a flow published rather than claiming an empty folder.
    if let Some(counts) = session_readouts() {
        s.to_speak = counts.owed;
        s.publish_written = screen::publish_written(s.publish_written, counts.publish_written);
    }
    draw_into(&s);
    // The rebuilt slot rows have no painter until this runs again; the thumbnail may have changed too.
    if let Some(win) = held_window() {
        crate::ui::produce_paint::install(&win);
    }
    crate::ui::produce_words_press::draw_word_chips(window, &s);
    // §A: a row's presence is drawn, so the grid is rebuilt whole, seeded from the held state; construction emits no change signal.
    rebuild_settings(window, &s);
    // These `set_text` calls DO emit `changed`: guard so a keystroke wire does not answer our own repaint (and re-enter `refresh` for the title).
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
}

/// §15's two session counts: narration lines still to be spoken, and whether `publish.json` exists. The page and
/// `Shell::inputs` both ask here. Owed is asked through the stamp's seam (`wav_of`); no lines with words is "no narration" (§07 §1).
pub fn session_counts(tree: &crate::layout::Tree) -> SessionCounts {
    let lines = crate::narration::load(tree)
        .map(|n| n.entries)
        .unwrap_or_default();
    let voice = Some(read_state().voice).filter(|voice| !voice.is_empty());
    let owed = crate::narration::lines_to_speak(&lines)
        .iter()
        .filter(|line| {
            crate::produce_stamp::wav_of(tree, line, voice.as_deref(), line.roll)
                .is_none_or(|wav| !wav.exists())
        })
        .count();
    SessionCounts {
        owed,
        narrated: crate::narration::has_lines(&lines),
        publish_written: crate::publish::is_written(tree),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionCounts {
    pub owed: usize,
    pub narrated: bool,
    pub publish_written: bool,
}

/// [`session_counts`] for the session beside the launch; `None` when this working directory is no session, which keeps what a flow published.
fn session_readouts() -> Option<SessionCounts> {
    let dir = crate::startup::open_dir();
    let tree = crate::layout::Tree::new(&dir).ok()?;
    Some(session_counts(&tree))
}

/// **1** ＋ Add image…: a ninth picture is refused rather than dropping one (§1).
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

pub fn press_change_image(window: &adw::ApplicationWindow, index: usize, path: &str) -> String {
    let changed = mutate(|s| screen::change_image(&mut s.frames, index, path));
    if !changed {
        return say(window, "there is no such slot to change");
    }
    refresh(window);
    say(window, &format!("image changed: {}", short(path)))
}

/// Remove one slot; removing the base makes the next picture the base.
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

fn store_setting(row: &str, value: &str) -> String {
    mutate(|s| store_setting_on(s, row, value))
}

/// One setting on a page state, shared by a hand on the page and a profile's `## produce` rows.
fn store_setting_on(s: &mut ProduceState, row: &str, value: &str) -> String {
    match row {
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
    }
}

/// One setting changed on the row. Container carries §A's forcing rule (webm forces vp9); what got forced is said.
pub fn set_setting(window: &adw::ApplicationWindow, row: &str, value: &str) -> String {
    let said = store_setting(row, value);
    keep_settings();
    // The save dialog opens on this row's own offer, so an unanswered chooser still leaves the press naming it (`produce_press_wiring::asked_path`).
    if row == "Container" {
        crate::ui::produce_press_wiring::answer_path(None);
    }
    refresh(window);
    say(window, &said)
}

/// A scale moved: stored without [`refresh`], which would rebuild the grid and end the drag after its first step.
fn set_scale(window: &adw::ApplicationWindow, row: &str, value: &str) -> String {
    let said = store_setting(row, value);
    keep_settings();
    say(window, &said)
}

/// One of the three TICK rows changed (§A); separate from `set_setting`, as a tick has no value string.
pub fn set_toggle(window: &adw::ApplicationWindow, row: &str, on: bool) -> String {
    mutate(|s| match row {
        "Frame timing" => s.vfr = on,
        "Channels" => s.mono = on,
        "Frame edges" => s.blurred_edges = on,
        _ => (),
    });
    keep_settings();
    refresh(window);
    say(window, &format!("{row} {}", if on { "on" } else { "off" }))
}

/// F0.14 S9: a profile's `## produce` rows on the encoder settings, each as a hand would set it (ticks take on/off,
/// `Translate` takes `none` or comma-separated languages). Unlisted rows keep the project's value.
pub fn apply_rows(
    produce: &crate::project::Produce,
    rows: &[(String, String)],
) -> crate::project::Produce {
    let mut s = read_state_default();
    set_encoder_fields(&mut s, produce);
    for (row, value) in rows {
        let on = value.trim() == "on";
        match row.as_str() {
            "Frame timing" => s.vfr = on,
            "Channels" => s.mono = on,
            "Frame edges" => s.blurred_edges = on,
            "Translate" => {
                s.translate.clear();
                for language in value
                    .split(',')
                    .map(str::trim)
                    .filter(|l| !l.is_empty() && *l != "none")
                {
                    crate::ui::produce_languages::apply(&mut s, language, true);
                }
            }
            _ => {
                store_setting_on(&mut s, row, value.trim());
            }
        }
    }
    produce_settings(&s)
}

/// The narration's game audio level, set on the Narrate page (§07 §1) and kept with the encoder settings.
pub fn set_game_volume(value: f64) {
    mutate(|s| s.game_volume = value.clamp(0.0, 1.0));
    keep_settings();
}

pub(crate) fn encoder_state(produce: &crate::project::Produce) -> ProduceState {
    let mut s = read_state_default();
    set_encoder_fields(&mut s, produce);
    s
}

fn encoder_profile() -> Option<String> {
    let used = crate::ui::window::SESSION
        .with(|slots| slots.borrow().last().cloned())
        .and_then(|session| session.borrow().profile.clone())?;
    let paths = crate::settings::from_environment()?;
    let template = crate::profile::parse(&crate::profile::load(&paths, &used.name).ok()?).ok()?;
    template
        .produce
        .iter()
        .any(|(row, _)| row != "Game audio")
        .then_some(used.name)
}

pub fn show_encoder(produce: &crate::project::Produce) {
    mutate(|s| set_encoder_fields(s, produce));
    if let Some(window) = held_window() {
        refresh(&window);
    }
}

fn set_encoder_fields(s: &mut ProduceState, p: &crate::project::Produce) {
    s.container = container_name(p.container).to_string();
    s.codec = codec_name(p.codec).to_string();
    s.preset = crate::produce_render::preset_name(p.preset).to_string();
    s.resolution = match p.resolution {
        crate::project::Resolution::P720 => "720p",
        crate::project::Resolution::P1080 => "1080p",
        crate::project::Resolution::Original => "original",
    }
    .to_string();
    s.frame_rate = crate::produce_render::frame_rate_name(p.frame_rate).to_string();
    s.audio = p.audio_kbps.to_string();
    s.subtitles = subtitles_name(p.subtitles).to_string();
    s.translate = p.translate.clone();
    s.game_volume = p.game_volume;
    s.crf = f64::from(p.crf);
    s.vfr = p.vfr;
    s.mono = p.mono;
    s.blurred_edges = p.blurred_edges;
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
    keep_page_publish();
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

pub fn press_words_remove(window: &adw::ApplicationWindow, index: usize) -> String {
    let done = mutate(|s| screen::remove_words(&mut s.texts, index));
    if !done {
        return say(window, "no such word box on the thumbnail");
    }
    keep_page_publish();
    refresh(window);
    say(window, "words removed")
}

/// A box dragged onto the picture prints words into it, through the `place_words` rule the logic tests drive.
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
    keep_page_publish();
    refresh(window);
    say(window, &format!("words placed: {}", text.trim()))
}

pub fn press_words_move(
    window: &adw::ApplicationWindow,
    hold: crate::ui::produce_words_press::Hold,
    at: crate::project::TitleBox,
) -> String {
    use crate::ui::produce_words_press::Hold;
    let done = mutate(|s| match hold {
        Hold::Title => {
            s.title_box = Some(at);
            true
        }
        Hold::Mark(index) => match s.texts.get_mut(index) {
            Some(mark) => {
                (mark.cx, mark.cy, mark.wf, mark.hf) = (at.cx, at.cy, at.wf, at.hf);
                true
            }
            None => false,
        },
    });
    if !done {
        return say(window, "no such word box on the thumbnail");
    }
    keep_page_publish();
    refresh(window);
    say(
        window,
        match hold {
            Hold::Title => "title moved",
            Hold::Mark(_) => "words moved",
        },
    )
}

/// The title band's ✎: reword, or take off when emptied. Counts as seeding either way, so a line taken off stays off.
pub fn press_title_words(window: &adw::ApplicationWindow, text: &str) -> String {
    let words = text.trim().to_string();
    let removed = words.is_empty();
    mutate(|s| {
        s.thumb_title = words;
        s.title_seeded = true;
    });
    keep_page_publish();
    refresh(window);
    say(
        window,
        if removed {
            "title taken off the picture"
        } else {
            "title on the picture reworded"
        },
    )
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

/// The encoder rows as the project keeps them (`naivepost.json`'s `produce`, §01 §3), written on every change.
/// A failed write is said on the log; the row keeps the choice.
fn keep_settings() {
    if let Err(why) = keep_produce(&produce_settings(&read_state())) {
        crate::ui::window::log_line(&format!("!!! the encoder settings were not kept: {why}"));
    }
}

/// The thumbnail's words kept in the window's session so autosave and close write them. Kept in the project's
/// `publish` record, not `publish.json`, which a RUN wrote and whose existence says the upload text was written.
pub(crate) fn keep_page_publish() {
    keep_record();
    reprint_picture();
}

/// The page onto the project's record, which wins when the project opens. `own` is the page's outright, as the merge only adds it.
fn keep_record() {
    let Some(session) =
        crate::ui::window::PLAY_SESSION.with(|slots| slots.borrow().last().cloned())
    else {
        return;
    };
    let page = crate::ui::produce_snapshot::publish_of_state(&read_state());
    let own = page.own;
    let held = session.borrow().publish.clone().unwrap_or_default();
    let mut merged = crate::ui::produce_snapshot::merge_publish(Some(page), &held);
    merged.own = own;
    session.borrow_mut().publish = Some(merged);
}

/// The page's words printed onto `thumbnail.png` again from the plain picture after every box edit; on a worker.
fn reprint_picture() {
    let s = read_state();
    let tree = crate::ui::produce_page_rows::session_tree();
    let plain = tree.thumbnail_plain_png();
    if !plain.is_file() {
        return;
    }
    let title = if s.title_seeded {
        s.thumb_title.clone()
    } else {
        String::new()
    };
    let band = s.title_box.unwrap_or_default();
    let texts = s.texts.clone();
    crate::ui::background::run(
        "thumbnail words",
        move || {
            crate::produce_upload_print::print_words(
                &plain,
                &tree.thumbnail_png(),
                &texts,
                &title,
                &band,
            )
        },
        |printed| {
            if let Err(why) = printed {
                crate::ui::window::log_line(&format!(
                    "    publish: printing the words failed: {why}"
                ));
            }
        },
    );
}

/// The encoder settings kept in both places: the window's session (written whole by autosave and close) and the file.
/// The file alone let the next autosave put back the settings the window opened with.
pub(crate) fn keep_produce(produce: &crate::project::Produce) -> Result<(), String> {
    if let Some(session) =
        crate::ui::window::PLAY_SESSION.with(|slots| slots.borrow().last().cloned())
    {
        session.borrow_mut().produce = produce.clone();
    }
    let dir = crate::startup::open_dir();
    let mut project = crate::project::load(&dir).map_err(|why| why.to_string())?;
    project.produce = produce.clone();
    crate::project::save(&project, &dir).map_err(|why| why.to_string())
}

/// The page as the opened project holds it (§08-produce#1-screen): encoder rows, publish half (`publish.json`, else the
/// project's copy), the cut, the narration switch, language and voice.
pub fn load_page(tree: &crate::layout::Tree) -> ProduceState {
    let project = crate::project::load(tree.dir()).unwrap_or_default();
    let cut = crate::cut::load(tree).unwrap_or_default();
    let p = &project.produce;
    // The project's own record carries the printed title, its seeding and the crop; `publish.json` fills what it left empty.
    let mut publish = project.publish.clone().unwrap_or_default();
    if crate::publish::is_written(tree) {
        let written = crate::publish::load(tree).unwrap_or_default();
        let fill = |held: &mut String, from: &str| {
            if held.is_empty() {
                *held = from.to_string();
            }
        };
        fill(&mut publish.title, &written.title);
        fill(&mut publish.thumb_title, &written.thumb_title);
        fill(&mut publish.prompt, &written.prompt);
        fill(&mut publish.negative, &written.negative);
        fill(&mut publish.description, &written.description);
        if publish.frames.is_empty() {
            publish.frames = written.frames;
        }
        if publish.texts.is_empty() {
            publish.texts = written.texts;
        }
        if project.publish.is_none() {
            publish.own = written.own;
        }
        publish.title_seeded = publish.title_seeded || written.title_seeded;
        if publish.title_box.is_none() {
            publish.title_box = written.title_box;
        }
    }
    let clips: Vec<&crate::cut::Seg> = cut.segs.iter().filter(|seg| seg.e > seg.s).collect();
    let narration = crate::narration::load(tree).unwrap_or_default();
    ProduceState {
        frames: publish.frames.clone(),
        own: publish.own,
        texts: publish.texts.clone(),
        title: publish.title.clone(),
        thumb_title: publish.thumb_title.clone(),
        title_seeded: publish.title_seeded,
        title_box: publish.title_box,
        instruction: publish.prompt.clone(),
        negative: publish.negative.clone(),
        description: publish.description.clone(),
        container: container_name(p.container).to_string(),
        codec: codec_name(p.codec).to_string(),
        preset: crate::produce_render::preset_name(p.preset).to_string(),
        resolution: match p.resolution {
            crate::project::Resolution::P720 => "720p",
            crate::project::Resolution::P1080 => "1080p",
            crate::project::Resolution::Original => "original",
        }
        .to_string(),
        frame_rate: crate::produce_render::frame_rate_name(p.frame_rate).to_string(),
        audio: p.audio_kbps.to_string(),
        subtitles: subtitles_name(p.subtitles).to_string(),
        translate: p.translate.clone(),
        game_volume: p.game_volume,
        crf: f64::from(p.crf),
        vfr: p.vfr,
        mono: p.mono,
        blurred_edges: p.blurred_edges,
        clips: clips.len(),
        seconds: crate::cut_screen::cut_seconds(&cut),
        session_language: language_name(&project.policy.language.value),
        cut_line: match (clips.first(), clips.last()) {
            (Some(first), Some(last)) => format!(
                "{} clip(s) from {} to {}",
                clips.len(),
                crate::tools::mm_ss(first.s),
                crate::tools::mm_ss(last.e)
            ),
            _ => String::new(),
        },
        first_spoken: narration
            .entries
            .iter()
            .find(|entry| !entry.text.is_empty() && entry.pos.is_empty())
            .map(|entry| entry.text.clone())
            .unwrap_or_default(),
        voice: crate::ui::narrate_seed::project_voice(tree),
        mixed_recordings: project
            .sources
            .iter()
            .filter(|source| !source.footage)
            .filter_map(|source| {
                std::path::Path::new(&source.path)
                    .file_stem()
                    .map(|stem| stem.to_string_lossy().to_string())
            })
            .collect(),
        // The base's shape off the file's header and the video's from the cut's aspect; no aspect means no shape and no box.
        base_shape: publish.frames.first().and_then(|stored| {
            let path = crate::layout::resolve(tree.dir(), tree.dir(), stored);
            gtk::gdk_pixbuf::Pixbuf::file_info(&path)
                .map(|(_, wide, high)| (wide.max(0) as u32, high.max(0) as u32))
        }),
        video_shape: if cut.aspect.trim().is_empty() {
            (0, 0)
        } else {
            crate::produce_upload::frame_size_from_aspect(&cut.aspect)
        },
        ..read_state_default()
    }
}

fn read_state_default() -> ProduceState {
    ProduceState {
        game_volume: 0.22,
        crf: 24.0,
        blurred_edges: true,
        ..ProduceState::default()
    }
}

fn language_name(code: &str) -> String {
    match code {
        "en" => "English",
        "de" => "German",
        "fr" => "French",
        other => other,
    }
    .to_string()
}

fn subtitles_name(s: Subtitles) -> &'static str {
    match s {
        Subtitles::BurnedIn => "burned in",
        Subtitles::TrackInFile => "track in file",
        Subtitles::None => "none in the video",
    }
}

/// Mark the standing picture as the user's own thumbnail. The title is printed on it unless a line is there already.
pub fn set_own_thumbnail() {
    mutate(|st| {
        st.own = true;
        if !st.title_seeded {
            st.thumb_title = st.title.clone();
            st.title_seeded = true;
        }
    });
    keep_record();
}

/// The other half of §F5.7's ↻: the picture goes back to the model, so it stops being the user's own.
pub fn clear_own_thumbnail() {
    mutate(|st| st.own = false);
}

pub fn container_word() -> String {
    container_name(parse_container(&read_state().container)).to_string()
}

pub fn container_of_row() -> Container {
    parse_container(&read_state().container)
}

pub fn rendered_video() -> std::path::PathBuf {
    let dir = crate::startup::open_dir();
    match crate::layout::Tree::new(dir) {
        Ok(tree) => tree.final_video(container_word().as_str()),
        Err(_) => std::path::PathBuf::from("produce/final.mp4"),
    }
}

/// The page's row as the settings struct the transcode log reads; the page holds the row's truth until a run saves it.
pub fn row_settings() -> crate::project::Produce {
    produce_settings(&read_state())
}

fn produce_settings(s: &ProduceState) -> crate::project::Produce {
    let defaults = crate::project::Produce::default();
    crate::project::Produce {
        container: parse_container(&s.container),
        codec: parse_codec(&s.codec),
        resolution: match s.resolution.as_str() {
            "720p" => crate::project::Resolution::P720,
            "original" => crate::project::Resolution::Original,
            _ => crate::project::Resolution::P1080,
        },
        // Every row reaches the render: a shown choice the encode ignores would lie about the file.
        preset: [
            crate::project::Preset::Ultrafast,
            crate::project::Preset::Veryfast,
            crate::project::Preset::Fast,
            crate::project::Preset::Medium,
            crate::project::Preset::Slow,
            crate::project::Preset::VerySlow,
        ]
        .into_iter()
        .find(|preset| crate::produce_render::preset_name(*preset) == s.preset)
        .unwrap_or(defaults.preset),
        frame_rate: [
            crate::project::FrameRate::Source,
            crate::project::FrameRate::F60,
            crate::project::FrameRate::F30,
            crate::project::FrameRate::F24,
        ]
        .into_iter()
        .find(|rate| crate::produce_render::frame_rate_name(*rate) == s.frame_rate)
        .unwrap_or(defaults.frame_rate),
        audio_kbps: s.audio.parse().unwrap_or(defaults.audio_kbps),
        subtitles: parse_subtitles(&s.subtitles),
        game_volume: s.game_volume,
        crf: s.crf as u32,
        vfr: s.vfr,
        mono: s.mono,
        blurred_edges: s.blurred_edges,
        // The ticked languages travel with the settings (§F5.4 S7/S9 write one sidecar per language, in this order).
        translate: s.translate.clone(),
        ..Default::default()
    }
}

/// Store one typed box's whole text under the widget's own name. Verbatim: §1 gives no trimming rule for the four
/// boxes, and their spacing belongs to whoever wrote them.
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
            "title-entry" => {
                s.title = text.to_string();
            }
            _ => return false,
        }
        true
    });
    if stored {
        keep_record();
        "".to_string()
    } else {
        "no such text box on this page".to_string()
    }
}

/// Attach every handler, from `build_window` after `set_content`. `build()` runs once per window and each publishes
/// ITS OWN root, so calling this again for a second window is safe.
pub fn wire(window: &adw::ApplicationWindow) {
    // The page is the opened project's, loaded whole: the state is one per process, so a later project kept the first one's.
    if let Ok(tree) = crate::layout::Tree::new(crate::startup::open_dir()) {
        set_state(load_page(&tree));
    }
    // A profile edited while this project was closed still sets its encoder rows, which are greyed here as the profile's.
    crate::ui::profile_form::follow_profile_produce();
    // Hold the WINDOW: the slot row looks up its per-slot buttons by name from the window root.
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
                    crate::ui::produce_choosers::open_image_chooser(&w, None);
                }
                // The five presses that call a model, sd.cpp or ffmpeg walk on a worker (§03-shell §1): the button greys until the sentence lands.
                "title-suggest" => {
                    crate::ui::produce_press_wiring::reword_from_button(&w);
                }
                "thumbnail-export" => {
                    crate::ui::produce_choosers::open_export_dialog(&w);
                }
                "thumbnail-redraw" => {
                    crate::ui::produce_press_wiring::redraw_from_button(&w);
                }
                "transcode-save" => {
                    crate::ui::produce_choosers::open_save_video_dialog(&w);
                }
                _ => {
                    crate::ui::produce_press_wiring::transcode_from_button(&w);
                }
            });
        }
    }
    // Wired before the slot buttons: `refresh` below rebuilds the grid whole.
    wire_settings(&win);
    let text_win = win.clone();
    crate::ui::produce_text::wire_text(
        &win,
        std::rc::Rc::new(|| SETTINGS_GUARD.with(|guard| guard.get())),
        std::rc::Rc::new(move |field, text| {
            set_text_field(field, text);
            refresh(&text_win);
        }),
    );
    crate::ui::produce_words_press::install_words_gesture(&win);
    // `draw_slots` rebuilds the slot widgets whole, so `refresh` installs again for the new ones.
    crate::ui::produce_paint::install(&win);
    refresh(&win);
}

/// Attach every settings control to `set_setting` / `set_toggle` (§1 items 11–14): bail when the guard says the
/// emission is our own repaint, read the widget, hand it to the seam that owns the rule.
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
            set_scale(&w, &row, &scale.value().to_string());
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
    let w = window.clone();
    crate::ui::produce_languages::wire_ticks(
        window,
        std::rc::Rc::new(|| SETTINGS_GUARD.with(|guard| guard.get())),
        std::rc::Rc::new(move |language, on| {
            mutate(|s| crate::ui::produce_languages::apply(s, language, on));
            keep_settings();
            refresh(&w);
            let said = crate::ui::produce_languages::status_line(language, on);
            say(&w, &said);
        }),
    );
}

/// The row's widget name, the page's ONE spelling: "Quality (CRF)" → `setting-quality--crf`.
pub fn row_widget(row: &str) -> String {
    format!(
        "setting-{}",
        row.to_lowercase()
            .replace(['(', ')', ' '], "-")
            .trim_matches('-')
    )
}

/// Draw the Images row into its holder and wire the buttons it created; called from `build` and `refresh`.
pub(crate) fn draw_into(s: &ProduceState) {
    // `build()` runs once per WINDOW, so ROOT can point at another window's holder; emptying the wrong box makes the live
    // one accumulate rows (`gtk_box_append: assertion 'gtk_widget_get_parent (child) == NULL'`). Prefer the refreshed window's holder.
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
            crate::ui::produce_press_wiring::set_thumbnail_from_button(w, i);
        }),
        change: std::rc::Rc::new(|w, i| {
            crate::ui::produce_choosers::open_image_chooser(w, Some(i));
        }),
        remove: std::rc::Rc::new(|w, i| {
            press_remove_image(w, i);
        }),
    };
    crate::ui::produce_slots::draw(&list, s, held_window().as_ref(), &doors);
}

/// The F5.1 run, driven from the page; every decision and string is `produce_flow`'s (spec/00-principles.md §5).
/// The halves arrive as closures because headless there is no sd.cpp or ffmpeg. The gate and snapshot are read here
/// on the window thread; the halves walk on a worker and the run's ending closes the bar, so ⏹ reaches it (§03-shell §1).
/// Answers the sentence when the press ended here, empty while it goes on.
pub fn press_produce_run(window: &adw::ApplicationWindow) -> String {
    let s = read_state();
    let project = crate::ui::window::live_project();
    let tree = crate::layout::Tree::new(crate::startup::open_dir()).ok();
    // S3: the Cut page's own segments win, so an unsaved tweak still renders (`window_preview::newest_review_cut`).
    let cut = match tree.as_ref() {
        Some(tree) => crate::produce_flow::what_is_the_cut(
            Some(crate::ui::window_preview::newest_review_cut()),
            tree,
        ),
        None => crate::ui::window_preview::newest_review_cut(),
    };
    if cut.segs.is_empty() {
        crate::ui::run_close::close_window_run();
        return say(window, crate::cut::NO_CUT_YET);
    }
    let settings = produce_settings(&s);
    let Some(tree) = tree.as_ref() else {
        return finish_produce(
            window,
            &s,
            &project,
            cut,
            settings,
            crate::produce_flow::Gate::Encode,
        );
    };
    // S2: up to date is the stamp's question, asked over the narration too (§F5.3): `gate_stamp` reads
    // `narrate/narration.json` itself, and measures the shape with the helper the snapshot uses.
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

/// The S2 answer acted on, then S4–S6. A `ConfirmOverwrite` gate opens the dialog and the encode runs from its
/// response handler on Overwrite (`encode_and_finish`).
fn finish_produce(
    window: &adw::ApplicationWindow,
    s: &ProduceState,
    project: &crate::project::Project,
    cut: crate::cut::Cut,
    settings: crate::project::Produce,
    gate: crate::produce_flow::Gate,
) -> String {
    if gate == crate::produce_flow::Gate::ConfirmOverwrite {
        let video = crate::layout::Tree::new(crate::startup::open_dir())
            .map(|t| t.final_video(container_name(parse_container(&s.container))))
            .unwrap_or_default();
        let (win, st, prj, ct, set) = (
            window.clone(),
            s.clone(),
            project.clone(),
            cut.clone(),
            settings.clone(),
        );
        let asked_on = window.clone();
        return crate::ui::produce_confirm::confirm_then(
            window,
            &video,
            move || {
                encode_and_finish(
                    &win,
                    &st,
                    &prj,
                    &ct,
                    &set,
                    crate::produce_flow::Gate::Encode,
                );
            },
            move |asked| {
                crate::ui::run_close::close_window_run();
                if let Some(status) = crate::ui::window::find_status(asked_on.upcast_ref()) {
                    status.set_text(asked);
                }
            },
        );
    }
    encode_and_finish(window, s, project, &cut, &settings, gate)
}

/// F5.6's answer onto the page once the run lands: what the words half wrote to `publish.json` replaces what the page
/// showed, and the plates repaint. A part the run left empty keeps the page's.
pub(crate) fn take_written_publish(window: &adw::ApplicationWindow) {
    let tree = session_tree();
    if !crate::publish::is_written(&tree) {
        return;
    }
    let Ok(written) = crate::publish::load(&tree) else {
        return;
    };
    mutate(|s| {
        let take = |held: &mut String, from: &str| {
            if !from.trim().is_empty() {
                *held = from.to_string();
            }
        };
        take(&mut s.title, &written.title);
        take(&mut s.thumb_title, &written.thumb_title);
        take(&mut s.instruction, &written.prompt);
        take(&mut s.negative, &written.negative);
        take(&mut s.description, &written.description);
        if !written.frames.is_empty() {
            s.frames = written.frames.clone();
            s.own = written.own;
        }
        // The seeding travels with the printed title, or the plate stays untitled while `thumbnail.png` carries it.
        s.title_seeded = s.title_seeded || written.title_seeded;
        if written.title_box.is_some() {
            s.title_box = written.title_box;
        }
        if !written.texts.is_empty() {
            s.texts = written.texts.clone();
        }
    });
    keep_record();
    refresh(window);
}

fn snapshot_run(
    s: &ProduceState,
    project: &crate::project::Project,
    cut: &crate::cut::Cut,
    settings: &crate::project::Produce,
) -> (
    crate::ui::produce_snapshot::Snapshot,
    crate::produce_flow::Run,
) {
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
        lines: snap.lines.clone(),
        // Cloned: F5.5 S5 runs after the render and needs the container and codec.
        settings: settings.clone(),
        sources: crate::run::snapshot_sources(project),
        // The crop, title box and image row have no page row, so merge `publish.json`'s record or the thumbnail redraws without its crop.
        publish: Some(crate::ui::produce_snapshot::publish_of(&snap)),
        aspect: String::new(),
        clips: s.clips,
        publish_written: s.publish_written,
        voice: s.voice.clone(),
    };
    let run = crate::produce_flow::Run {
        aspect: run.cut.aspect.clone(),
        ..run
    };
    (snap, run)
}

pub(crate) fn press_run() -> Option<crate::produce_flow::Run> {
    let cut = crate::ui::window_preview::newest_review_cut();
    if cut.segs.is_empty() {
        return None;
    }
    let project = crate::project::load(session_tree().dir()).ok()?;
    Some(snapshot_run(&read_state(), &project, &cut, &row_settings()).1)
}

fn encode_and_finish(
    window: &adw::ApplicationWindow,
    s: &ProduceState,
    project: &crate::project::Project,
    cut: &crate::cut::Cut,
    settings: &crate::project::Produce,
    gate: crate::produce_flow::Gate,
) -> String {
    let language = project.policy.language.value.clone();
    let skip = gate == crate::produce_flow::Gate::Skip;
    // S3: everything the halves read is gathered here, once, on the GUI thread; a leg re-reading the window mid-render would see later moves.
    let (snap, run) = snapshot_run(s, project, cut, settings);
    if skip {
        crate::ui::window::log_line(crate::produce_flow::SKIP_LOG);
    } else {
        crate::ui::window::log_line(&crate::produce_flow::opening_line(&run, false));
    }
    let carried = crate::ui::produce_worker::Carried::take();
    let settings = settings.clone();
    let w = window.clone();
    let open = std::rc::Rc::new(std::cell::Cell::new(true));
    let still_open = std::rc::Rc::clone(&open);
    crate::ui::produce_progress::follow(window, move || still_open.get());
    crate::ui::background::run(
        "produce",
        move || {
            carried.load();
            // The run's `llm/` page is this thread's (§7 keeps one per thread).
            crate::exchange_leg::note_run_started();
            if skip {
                if let Err(why) = words_half(&run) {
                    crate::ui::window::log_line(&crate::produce_flow::words_failed(&why));
                }
                crate::ui::produce_languages::tag_page(&settings, &language);
                crate::exchange_leg::note_run_ended();
                return None;
            }
            // Both halves through `run_with`; a words failure is logged and never stops the render (F5.6, F5.2).
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
            // F5.5 S5: after both halves; S6 keeps the render's error as the verdict.
            crate::ui::produce_languages::tag_page(&settings, &language);
            crate::exchange_leg::note_run_ended();
            Some(ending)
        },
        move |ending| {
            open.set(false);
            take_written_publish(&w);
            let dir = crate::startup::open_dir();
            if let Ok(project) = crate::project::load(&dir) {
                crate::ui::window::paint_readouts_for(&w, &project, &dir);
            }
            let Some(ending) = ending else {
                crate::ui::run_close::close_window_run();
                say(&w, crate::produce_flow::SKIP_LOG);
                return;
            };
            crate::ui::window::log_line(&ending.log);
            if !ending.progress.is_empty() {
                crate::ui::window::log_line(&ending.progress);
            }
            crate::ui::run_close::close_window_run();
            say(&w, ending.status);
        },
    );
    String::new()
}
