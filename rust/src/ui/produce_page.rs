//! §08-produce#1-screen: the Produce page's surface, §1's fifteen numbered widgets laid out per
//! spec/inventory/produce.md §A. No rule lives here: options, tooltips and sentences come from
//! [`crate::produce_screen`] and [`crate::produce_runs`] (spec/00-principles.md §5).
//! The page renders a state the flows publish through [`set_state`], like `narrate_page`.

use adw::prelude::*;
use gtk4 as gtk;

use crate::produce_flow::codec_name;
pub(crate) use crate::produce_flow::container_name;
use crate::produce_screen as screen;
use crate::ui::produce_page_layout as layout;
pub use crate::ui::produce_page_presses::*;
pub use crate::ui::produce_page_run::*;
pub use crate::ui::produce_page_settings::*;
pub use crate::ui::produce_page_wire::*;

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
    pub(crate) static SETTINGS_GUARD: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

pub fn set_state(next: ProduceState) {
    PRODUCE_STATE.with(|held| *held.borrow_mut() = next);
}

pub fn read_state() -> ProduceState {
    PRODUCE_STATE.with(|held| held.borrow().clone())
}

pub(crate) fn mutate<T>(f: impl FnOnce(&mut ProduceState) -> T) -> T {
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

pub(crate) fn find(node: &gtk::Widget, name: &str) -> Option<gtk::Box> {
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
    pub(crate) static ROOT: std::cell::RefCell<Option<gtk::Widget>> = const { std::cell::RefCell::new(None) };
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

/// The page's words printed onto `thumbnail.png` again from the plain picture after every box edit; on a worker.
pub(crate) fn reprint_picture() {
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

pub(crate) fn read_state_default() -> ProduceState {
    ProduceState {
        game_volume: 0.22,
        crf: 24.0,
        blurred_edges: true,
        ..ProduceState::default()
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
