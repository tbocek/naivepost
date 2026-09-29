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
    let css = ".produce-plate { background-color: #1c1c1d; border: 1px solid #000000; }\n";
    let provider = gtk::CssProvider::new();
    let _ = provider.load_from_data(css);
    gtk::StyleContext::add_provider_for_display(
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
    let picture = gtk::DrawingArea::new();
    picture.set_widget_name("thumbnail-picture");
    picture.set_vexpand(false);
    picture.set_halign(gtk::Align::Fill);
    picture.set_size_request(320, 180);
    picture.add_css_class("produce-plate");
    picture.set_tooltip_text(Some(
        "Drag a box over the picture to put words on it \u{2014} they print to fill the box. Drag a border \
to resize, the middle to move, and press its \u{270e} to reword or remove it.",
    ));
    left.append(&picture);
    // The ✎ chips live in a named holder so `refresh` can rebuild them with the marks they stand for.
    let chips = gtk::Box::new(gtk::Orientation::Vertical, 2);
    chips.set_widget_name("thumbnail-word-chips");
    left.append(&chips);
    left
}

/// The settings grid: one labelled control per row, drawn from `settings_rows_shown` so the
/// Game-audio row is ABSENT when narration is off (§F4.8), each widget named by `row_widget`.
/// Build the settings rows with the values the page currently holds already IN them. Doing it here, at
/// creation, is what lets `refresh` rebuild the grid whole without a repaint ever landing on a control
/// that shows a default instead of the state — and it needs no guard, because nothing is wired yet.
fn build_settings(grid: &gtk::Grid, mut row: i32) -> i32 {
    let s = read_state();
    for name in screen::settings_rows_shown(s.narration_off) {
        let name: &str = name.as_ref();
        // The label is what §A prints left of the control; every branch attaches it at column 0 and its
        // control at column 1, so hiding or showing a row moves both halves together.
        match name {
            "Game audio" => {
                grid.attach(&setting_label("Game audio"), 0, row, 1, 1);
                let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 1.0, 0.02);
                scale.set_widget_name(&row_widget("Game audio"));
                scale.set_draw_value(false);
                scale.set_hexpand(true);
                scale.set_value(s.game_volume);
                scale.set_tooltip_text(Some(screen::tooltip("Game audio")));
                grid.attach(&scale, 1, row, 2, 1);
            }
            "Quality (CRF)" => {
                grid.attach(&setting_label("Quality (CRF)"), 0, row, 1, 1);
                let scale = gtk::Scale::with_range(
                    gtk::Orientation::Horizontal,
                    screen::CRF_MIN as f64,
                    screen::CRF_MAX as f64,
                    1.0,
                );
                scale.set_widget_name(&row_widget("Quality (CRF)"));
                scale.set_hexpand(true);
                scale.set_value(s.crf as f64);
                scale.set_tooltip_text(Some(screen::tooltip("Quality (CRF)")));
                grid.attach(&scale, 1, row, 2, 1);
            }
            "Frame timing" | "Channels" | "Frame edges" => {
                // Three tick rows: the tick carries §A's own word, not the row's name.
                let (tick_label, widget) = match name {
                    "Frame timing" => (screen::VFR_TICK, "tick-peak-rate-vfr"),
                    "Channels" => (screen::MONO_TICK, "tick-mono"),
                    _ => (screen::EDGES_TICK, "tick-blurred"),
                };
                let tick = gtk::CheckButton::with_label(tick_label);
                tick.set_widget_name(widget);
                tick.set_active(match name {
                    "Frame timing" => s.vfr,
                    "Channels" => s.mono,
                    _ => s.blurred_edges,
                });
                tick.set_tooltip_text(Some(screen::tooltip(name)));
                grid.attach(&tick, 1, row, 2, 1);
            }
            "Translate" => {
                grid.attach(&setting_label("Translate"), 0, row, 1, 1);
                // One tick per offered language, built from `translate_options` so the session's own
                // language never appears as a row at all.
                let ticks = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                // The row is ONE `setting-*` control holding its per-language ticks, so counting the
                // drawn `setting-*` names counts Translate as a row like every other one.
                ticks.set_widget_name(&row_widget("Translate"));
                for language in screen::translate_options(&s.session_language) {
                    let tick = gtk::CheckButton::with_label(language);
                    tick.set_widget_name(&format!("translate-tick-{language}"));
                    tick.set_active(s.translate.iter().any(|t| t == language));
                    tick.set_tooltip_text(Some(screen::tooltip("Translate")));
                    ticks.append(&tick);
                }
                grid.attach(&ticks, 1, row, 2, 1);
            }
            other => {
                grid.attach(&setting_label(&other), 0, row, 1, 1);
                let choices = screen::options(name).unwrap_or(&[]);
                let picker = gtk::DropDown::from_strings(choices);
                picker.set_widget_name(&row_widget(name));
                if let Some(at) = screen::options(name)
                    .and_then(|list| list.iter().position(|v| *v == held_choice(&s, name)))
                {
                    picker.set_selected(at as u32);
                }
                picker.set_tooltip_text(Some(screen::tooltip(name)));
                grid.attach(&picker, 1, row, 2, 1);
            }
        }
        row += 1;
    }
    row
}

/// The value the page holds for one dropdown row, so `build_settings` can seed the picker.
fn held_choice<'a>(s: &'a ProduceState, row: &str) -> &'a str {
    match row {
        "Container" => s.container.as_str(),
        "Codec" => s.codec.as_str(),
        "Preset" => s.preset.as_str(),
        "Resolution" => s.resolution.as_str(),
        "Frame rate" => s.frame_rate.as_str(),
        "Audio" => s.audio.as_str(),
        "Subtitles" => s.subtitles.as_str(),
        _ => "",
    }
}

fn setting_label(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(&format!("{text}:")));
    label.set_xalign(1.0);
    label.add_css_class("dim-label");
    label
}

/// **8** Title + ↻ suggest · **9** YouTube description · **10** Transcode ⤓ ↻ ·
/// **11**–**14** the encoder settings scroller · **15** the Inputs readout.
fn build_right_column() -> gtk::Box {
    let right = gtk::Box::new(gtk::Orientation::Vertical, 8);
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
    scroller.set_policy(gtk::PolicyType::External, gtk::PolicyType::Automatic);
    scroller.set_vexpand(true);
    let grid = gtk::Grid::new();
    grid.set_widget_name("encoder-settings");
    grid.set_row_spacing(6);
    grid.set_column_spacing(8);
    // Built un-wired here: `wire` attaches every control after `set_content`, because a handler put on
    // a widget that is not yet in the realized tree never fires (the same reason the Cut page attaches its
    // gestures after `set_content`).
    build_settings(&grid, 0);
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

/// The page: horizontal paned, images/thumbnail left, written column and settings right (§A).
pub fn build() -> gtk::Widget {
    install_page_css_once();
    let split = gtk::Paned::new(gtk::Orientation::Horizontal);
    split.set_position(560);
    split.set_wide_handle(true);
    split.set_start_child(Some(&build_left_column()));
    split.set_end_child(Some(&build_right_column()));
    // Publish the root before drawing: the empty-images note is drawn by `draw_slots`, which finds its
    // holder through ROOT, and at build time `wire` has not stored it yet.
    ROOT.with(|held| *held.borrow_mut() = Some(split.clone().upcast()));
    draw_slots(&read_state());
    split.upcast()
}

// --- the page's own lookups and output channels ----------------------------------------------------

fn widget_in_root(name: &str) -> Option<gtk::Box> {
    // The page is built once and held here so `draw_slots` can find its own box before the window has
    // a content root to walk from (the same reason the narrate page looks up from `window.content()`).
    ROOT.with(|held| held.borrow().clone()).and_then(|root| find(&root, name))
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
    /// The page's own root, remembered at `build()` so its seams can reach their widgets.
    static ROOT: std::cell::RefCell<Option<gtk::Widget>> = const { std::cell::RefCell::new(None) };
}

/// The window this page lives in, once `wire` has run. Slot buttons are attached through it.
pub fn held_window() -> Option<adw::ApplicationWindow> {
    ROOT.with(|held| held.borrow().clone()).and_then(|w| w.downcast::<adw::ApplicationWindow>().ok())
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
    let Some(grid) = widget_in(window, "encoder-settings")
        .and_then(|w| w.downcast::<gtk::Grid>().ok())
    else {
        return;
    };
    while let Some(child) = grid.first_child() {
        grid.remove(&child);
    }
    build_settings(&grid, 0);
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
    let s = read_state();
    draw_slots(&s);
    // The ✎ chips belong to the marks, so they are redrawn by the same door that redraws the slots.
    draw_word_chips(window, &s);
    // §A: a row's presence is itself drawn, so the grid is rebuilt whole rather than updated in place.
    // The new controls are seeded from the held state as they are created (`build_settings`), which is
    // why no guarded set-value pass is needed here: nothing emits a change signal during construction,
    // so a repaint cannot be mistaken for an edit.
    rebuild_settings(window, &s);
    if let Some(entry) = widget_in(window, "title-entry")
        .and_then(|w| w.downcast::<gtk::Entry>().ok())
    {
        entry.set_text(&s.title);
    }
    for (name, text) in [
        ("edit-instruction", &s.instruction),
        ("negative-prompt", &s.negative),
        ("description-box", &s.description),
    ] {
        if let Some(view) = widget_in(window, name)
            .and_then(|w| w.downcast::<gtk::TextView>().ok())
        {
            view.buffer().set_text(text);
        }
    }
    if let Some(inputs) = label(window, "produce-inputs") {
        inputs.set_text(&screen::inputs_line(
            s.clips,
            s.seconds,
            s.narration_off,
            s.to_speak,
            s.publish_written,
        ));
        // The tooltip answers "what exactly is behind that count": the clip range, the first line to
        // be spoken, the voice in use, and whether text leaves the machine. Only what THIS session has
        // is stated; a thing the flows have not published yet contributes no row rather than a guess.
        let first_line = if s.first_spoken.is_empty() { None } else { Some(s.first_spoken.as_str()) };
        let voice = if s.voice.is_empty() { None } else { Some(s.voice.as_str()) };
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
    say(window, "base changed \u{2014} the image model edits the first picture now")
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
        return say(window, screen::EMPTY_IMAGES);    }
    say(window, &format!("image removed: {}", short(&path)))
}

/// **3** Set Thumbnail: put this picture on the thumbnail as it is — a copy, cropped to the video's
/// shape, with no model call and no GPU (§A). Forwarder: the record-and-print lives in `produce_press_wiring`.
pub fn press_set_thumbnail(window: &adw::ApplicationWindow, index: usize) -> String {
    crate::ui::produce_press_wiring::set_thumbnail(window, index)
}

/// **6** ⤓ export: the JPEG an uploader takes, under its 2 MB limit.
pub fn press_export_thumbnail(window: &adw::ApplicationWindow, bytes: u64) -> String {
    crate::ui::produce_press_wiring::export_thumbnail(window, bytes)
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
        if removed { "words removed" } else { "words reworded" },
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

/// One ✎ chip per word box on the picture (§1 item 7: "✎ to reword"). Rebuilt whole, because a removed
/// mark must not leave its chip behind claiming a box that no longer exists.
fn draw_word_chips(window: &adw::ApplicationWindow, s: &ProduceState) {
    let Some(holder) = widget_in_root("thumbnail-word-chips") else {
        return;
    };
    while let Some(child) = holder.first_child() {
        holder.remove(&child);
    }
    for (index, mark) in s.texts.iter().enumerate() {
        let chip = gtk::Button::with_label(&format!("\u{270e} {}", mark.text));
        chip.set_widget_name(&format!("words-chip-{index}"));
        chip.set_tooltip_text(Some("reword or remove this box"));
        chip.set_halign(gtk::Align::Start);
        let w = window.clone();
        let text = mark.text.clone();
        chip.connect_clicked(move |_| crate::ui::produce_words::open_reword(&w, index, &text));
        holder.append(&chip);
    }
}

/// The drag-a-box gesture over the picture. The box is kept as FRACTIONS of the plate, not pixels:
/// `screen::place_words` is resolution-independent, so a saved mark survives being shown at another
/// size. A press with almost no travel is a look, not a box — §A floors a box at 28 px and the plate
/// is ~320 px wide, so 0.02 is that same floor expressed as a fraction.
fn install_words_gesture(window: &adw::ApplicationWindow) {
    let Some(picture) = widget_in(window, "thumbnail-picture") else {
        return;
    };
    let gesture = gtk::GestureDrag::new();
    gesture.set_button(1);
    // Grabbed before the closure below takes what it needs: `add_controller` consumes nothing but the
    // handler must own its own copies of `picture` and `window`.
    let plate = picture.clone();
    let w = window.clone();
    // `drag-begin` carries the ABSOLUTE press position and `drag-end` only the OFFSET from it (the F2.8
    // lesson), so both corners are known only across the two signals.
    let start = std::rc::Rc::new(std::cell::Cell::new((0.0f64, 0.0f64)));
    let start_begin = start.clone();
    gesture.connect_drag_begin(move |_g, x, y| {
        start_begin.set((x, y));
    });
    gesture.connect_drag_end(move |_g, offset_x, offset_y| {
        let (start_x, start_y) = start.get();
        let wide = f64::from(plate.width()).max(1.0);
        let high = f64::from(plate.height()).max(1.0);
        let x0 = start_x.min(start_x + offset_x).max(0.0) / wide;
        let y0 = start_y.min(start_y + offset_y).max(0.0) / high;
        let wf = offset_x.abs() / wide;
        let hf = offset_y.abs() / high;
        if wf < 0.02 || hf < 0.02 {
            return;
        }
        crate::ui::produce_words::open_place(
            &w,
            x0 + wf / 2.0,
            y0 + hf / 2.0,
            wf.min(1.0),
            hf.min(1.0),
        );
    });
    picture.add_controller(gesture);
}

/// Attach every handler. Called from `build_window` after `set_content`, like every other control on
/// every page: a handler attached before the widget is inside the realized tree never fires.
pub fn wire(window: &adw::ApplicationWindow) {
    // Hold the WINDOW, not just the content root: `draw_slots` needs it to attach its per-slot buttons,
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
                    let next = format!("project:produce/publish/frame-{}.jpg", read_state().frames.len() + 1);
                    press_add_image(&w, &next);
                }
                "title-suggest" => {
                    press_suggest(&w);
                }
                "thumbnail-export" => {
                    press_export_thumbnail(&w, 1024 * 1024);
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
    // The overlay editor's drag lives on the picture itself; the ✎ chips arrive with each refresh.
    install_words_gesture(&win);
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
        let Some(picker) = widget_in(window, &row_widget(row))
            .and_then(|w| w.downcast::<gtk::DropDown>().ok())
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
        let Some(scale) = widget_in(window, &row_widget(row))
            .and_then(|w| w.downcast::<gtk::Scale>().ok())
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
        let Some(tick) = widget_in(window, name)
            .and_then(|w| w.downcast::<gtk::CheckButton>().ok())
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

/// Stand in for the file chooser Change… opens. A headless run has no chooser, so the picture name is
/// synthesised here, exactly as ＋ Add image synthesises `frame-{n}.jpg`. It counts presses rather than
/// reading the row, because the whole point of the door is that the slot's picture MOVES; deriving the
/// name from the current state would hand back what a previous press already put there.
static CHANGE_STAND_IN: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// The per-slot buttons for however many slots the page holds.
fn wire_slot_buttons(window: &adw::ApplicationWindow) {
    let count = read_state().frames.len();
    for index in 0..count {
        // The file chooser belongs to the host desktop and there is none headless, so Change… stands in
        // with a name synthesised at press time by `CHANGE_STAND_IN`, just as ＋ Add image does.
        for (prefix, door) in [
            ("image-make-base", Door::MakeBase),
            ("image-set-thumbnail", Door::SetThumbnail),
            ("image-change", Door::Change),
            ("image-remove", Door::Remove),
        ] {
            let name = format!("{prefix}-{index}");
            if let Some(button) = crate::ui::line_step_button(window, &name) {
                let w = window.clone();
                button.connect_clicked(move |_| {
                    match door {
                        Door::MakeBase => press_make_base(&w, index),
                        Door::SetThumbnail => press_set_thumbnail(&w, index),
                        // What this door proves is that the slot KEEPS ITS PLACE and only its picture
                        // changes (`screen::change_image`), not which filename arrived.
                        Door::Change => {
                            let pick = CHANGE_STAND_IN.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                            let stood_in_for = format!("project:produce/publish/chosen-{pick}.jpg");
                            press_change_image(&w, index, &stood_in_for)
                        }
                        Door::Remove => press_remove_image(&w, index),
                    };
                });
            }
        }
    }
}

#[derive(Clone, Copy)]
enum Door {
    MakeBase,
    SetThumbnail,
    Change,
    Remove,
}


/// The row's widget name: lowercased, spaces and parens turned to dashes ("Quality (CRF)" →
/// "quality-crf"), so a test can find any setting by the §A row it belongs to.
/// The control name one settings row carries, the page's ONE spelling: "Quality (CRF)" →
/// `setting-quality--crf`, "Translate" → `setting-translate`. A test finds any row by its §A name
/// without knowing whether that row is a dropdown, a scale or a box of ticks.
pub fn row_widget(row: &str) -> String {
    format!(
        "setting-{}",
        row.to_lowercase()
            .replace(['(', ')', ' '], "-")
            .trim_matches('-')
    )
}

/// `refresh`, rebuilt whole because the base changes when slot 0 is removed.
fn draw_slots(s: &ProduceState) {
    let Some(list) = widget_in_root("publish-images") else {
        return;
    };
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
    if s.frames.is_empty() {
        // §A's empty state: two sentences where the slots would be, so the page reads as empty rather
        // than broken. No buttons wired into it — there is no slot for them to point at.
        let note = gtk::Box::new(gtk::Orientation::Vertical, 2);
        note.set_widget_name("publish-images-empty");
        let first = gtk::Label::new(Some(screen::EMPTY_IMAGES));
        first.set_xalign(0.0);
        first.add_css_class("dim-label");
        note.append(&first);
        let second = gtk::Label::new(Some(screen::EMPTY_IMAGES_HINT));
        second.set_xalign(0.0);
        second.add_css_class("dim-label");
        note.append(&second);
        list.append(&note);
        return;
    }
    let count = s.frames.len();
    for (index, path) in s.frames.iter().enumerate() {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        row.set_widget_name(&format!("image-slot-{index}"));
        let name = gtk::Label::new(Some(path.rsplit('/').next().unwrap_or(path)));
        name.set_xalign(0.0);
        name.set_hexpand(true);
        name.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
        row.append(&name);
        // The buttons a slot offers come from `slot_actions`: `Make base` only on a reference,
        // `Set Thumbnail` / `Change\u{2026}` / `remove` on every one.
        for action in screen::slot_actions(index, count) {
            let (widget_name, label) = match *action {
                "Make base" => (format!("image-make-base-{index}"), "Make base"),
                "Set Thumbnail" => (format!("image-set-thumbnail-{index}"), "Set Thumbnail"),
                "Change\u{2026}" => (format!("image-change-{index}"), "Change\u{2026}"),
                _ => (format!("image-remove-{index}"), "remove"),
            };
            let button = gtk::Button::with_label(label);
            button.set_widget_name(&widget_name);
            row.append(&button);
        }
        list.append(&row);
    }
    // The buttons are found by name from the window root, so they can be wired the moment they exist:
    // `wire` runs before the first `refresh`, when there are no slots yet and nothing to attach to.
    if let Some(window) = held_window() {
        wire_slot_buttons(&window);
    }
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
    let tree = crate::layout::Tree::new(&crate::startup::session_dir(
        &std::env::current_dir().unwrap_or_default(),
    ))
    .ok();
    // S3: ONE function answers "what is the cut", and the Cut page's own segments win so a tweak that
    // was never saved still renders. `review_cut_of` is that live cut.
    let cut = match tree.as_ref() {
        Some(tree) => crate::produce_flow::what_is_the_cut(
            Some(crate::ui::review_cut_of(window)),
            tree,
        ),
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
        return finish_produce(window, &s, &project, cut, settings, crate::produce_flow::Gate::Encode);
    };
    // S2: up to date is the stamp's question, and it is asked over the narration too — §F5.3 lists the
    // lines (bounds, text, wav size and mtime) among what the hash covers, so `gate_stamp` reads
    // `narrate/narration.json` itself rather than hashing an empty list and calling that cheap.
    let voice = if s.voice.is_empty() { None } else { Some(s.voice.as_str()) };
    let current = crate::produce_stamp::gate_stamp(
        tree,
        &settings,
        &cut.segs,
        &project.sources,
        &cut.aspect,
        voice,
        project.no_narration,
    );
    let gate = crate::produce_flow::gate(
        crate::produce_stamp::skip_encode(
            crate::produce_stamp::read_stamp(tree).as_deref(),
            &current,
        ),
        tree.final_video(container_name(parse_container(&s.container))).exists(),
    );
    finish_produce(window, &s, &project, cut, settings, gate)
}

/// The S2 answer acted on and S4–S6 run. Split out only so the `Tree` borrow above ends before the run.
fn finish_produce(
    window: &adw::ApplicationWindow,
    s: &ProduceState,
    project: &crate::project::Project,
    cut: crate::cut::Cut,
    settings: crate::project::Produce,
    gate: crate::produce_flow::Gate,
) -> String {
    // S2: a stale video standing here gets the overwrite question, in §S2's own two paragraphs. Nothing
    // is written by asking; the answer's own door is what runs the encode.
    if gate == crate::produce_flow::Gate::ConfirmOverwrite {
        let video = crate::layout::Tree::new(&crate::startup::session_dir(
            &std::env::current_dir().unwrap_or_default(),
        ))
        .map(|t| t.final_video(container_name(parse_container(&s.container))))
        .unwrap_or_default();
        let base = video
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "final".to_string());
        let (bytes, mtime) = crate::produce_stamp::disk_facts(&video);
        // The title rides the status line and the two-paragraph body goes to the log: the dialog this
        // answer opens belongs to the confirmation round, and the page's own door prints both halves.
        let asked = crate::produce_flow::overwrite_title(&base);
        let body = crate::produce_flow::overwrite_detail(
            &video.to_string_lossy(),
            &format!("{bytes} bytes"),
            &format!("modified {mtime}"),
        );
        crate::ui::window::log_line(&body);
        return say(window, &asked);
    }
    if gate == crate::produce_flow::Gate::Skip {
        // F5.5 S5 runs on the skip path too: the page is rewritten even when the encode was skipped.
        crate::ui::produce_languages::tag_page(&settings);
        return say(window, crate::produce_flow::SKIP_LOG);
    }
    let run = crate::produce_flow::Run {
        cut,
        lines: vec![],
        // Cloned rather than moved: F5.5 S5 runs after the render and needs the container and codec to
        // ask whether the result is something a browser plays.
        settings: settings.clone(),
        sources: crate::run::snapshot_sources(project),
        publish: project.publish.clone(),
        aspect: String::new(),
        clips: s.clips,
        publish_written: s.publish_written,
        voice: s.voice.clone(),
        no_narration: project.no_narration,
    };
    let run = crate::produce_flow::Run { aspect: run.cut.aspect.clone(), ..run };
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
        |run| {
            let tree = session_tree();
            // Cloned twice: the two closures that need the source list each own a copy, because a `move`
            // closure takes what it captures and both of these run inside one call.
            let sources_for_stamp = project.sources.clone();
            let sources_for_clips = project.clone();
            // S1 (§F5.4): the clip's own speech, read off the session word list F1.13 saved once for
            // exactly this purpose ("the list then carries ... over the recogniser's bare words", and it is
            // shared by retakes, joins, `final.txt` and subtitles). The narrator's own recordings are
            // excluded — §F5.4 says so outright — because their words are what the narration replaced; the
            // `written` field is the fixed transcript's spelling, not the recogniser's.
            let session_words: Vec<crate::word_list::Word> =
                crate::word_list::load(&tree).ok().flatten().unwrap_or_default();
            let narrator_sources: Vec<String> = project
                .sources
                .iter()
                .filter(|src| src.narrator != 0)
                .map(|src| src.path.clone())
                .collect();
            let render = crate::produce_exec::run_render(
                run,
                &tree,
                &crate::produce_exec::Materials {
                    exists: &|file| std::path::Path::new(file).exists(),
                    fx_for: &|_| Vec::new(),
                    lanes: &run.cut.lanes,
                    src_shape: (1920, 1080),
                    wav_of: &|entry| crate::produce_stamp::wav_of(&tree, entry, Some(&run.voice), 0),
                    cues: crate::produce_exec::Cues {
                        all: Vec::new(),
                        languages: run.settings.translate.clone(),
                    },
                    words: &move |clip| crate::produce_clip_cues::words_for(
                        clip,
                        &session_words,
                        &narrator_sources,
                    ),
                    // S3 (§F5.4): the scripted reply when a test loaded one, else a real refusal that
                    // ships every line as the original with the merge's warning.
                    translate: &crate::produce_translate::scripted_ask(),
                    sources: &move || sources_for_stamp.clone(),
                    // S5's picture input: an insert reads its own asset, footage and a `copy:` stretch read
                    // the first footage recording. Resolved here, where the project is held, by the same
                    // rule `Seg::insert_asset` uses.
                    source_file: &move |clip| {
                        if clip.source.is_empty() || clip.source.starts_with("copy:") {
                            return sources_for_clips
                                .sources
                                .iter()
                                .find(|src| src.footage)
                                .map(|src| src.path.clone());
                        }
                        let root = std::env::current_dir().unwrap_or_default();
                        // `Seg::insert_asset` is the one path rule of §1; the clip carries the same spelling.
                        let mut probe = crate::cut::Seg::default();
                        probe.ins = clip.source.clone();
                        Some(
                            probe
                                .insert_asset(&root, &root)
                                .map(|p| p.to_string_lossy().to_string())
                                .unwrap_or_else(|| clip.source.clone()),
                        )
                    },
                },
                |line| crate::ui::window::log_line(line),
                speak_a_line,
                |at, which| render_asked(at, which),
                crate::ui::produce_languages::spawn,
            );
            // A render that stopped or failed says so in the log with the spec's own line, on top of the
            // ending's word on the status line: `run_with` turns the report into "production stopped" or
            // "production failed — see log", and the reason belongs where a person can read it.
            if let Some(why) = render_failure_note(&render) {
                crate::ui::window::log_line(&crate::produce_flow::failed_log(&why));
            }
            render
        },
    );
    // F5.5 S5: after both halves; the page's own outcome is an input to nothing (S6 keeps the render's
    // error as the run's verdict).
    crate::ui::produce_languages::tag_page(&settings);
    crate::ui::window::log_line(&ending.log);
    if !ending.progress.is_empty() {
        crate::ui::window::log_line(&ending.progress);
    }
    say(window, ending.status)
}

/// The session's folder, resolved the way the page resolves everything else on this path.
fn session_tree() -> crate::layout::Tree {
    crate::layout::Tree::new(&crate::startup::session_dir(
        &std::env::current_dir().unwrap_or_default(),
    ))
    .unwrap_or_else(|_| crate::layout::Tree::new(std::path::Path::new("session.naivepost")).unwrap())
}

/// S2's speak: no audio.cpp here, so every line reports why. A synthesis that fails is logged by the
/// render's own rule (`synthesis_failure_log`) and the encoder carries on — the same shape as the words
/// half's failure above, and for the same reason: one silent sentence is not a reason to lose a video.
fn speak_a_line(line: &crate::produce_exec::ToSpeak) -> Result<(), String> {
    Err(format!(
        "no speech server here: {} was not synthesized",
        line.text.chars().take(24).collect::<String>()
    ))
}

/// S10's checkpoint question. This page runs the whole render inside one press and the shell's cancel
/// flag is F0.x's, so the answer is always "go on": the seam is here because `produce_exec` is where a
/// stop is *placed* between the subprocesses, and wiring the real flag replaces one closure, not the walk.
fn render_asked(_at: crate::produce_exec::At, _which: &str) -> bool {
    true
}

/// The render's own failure, if it had one, for §B's `produce FAILED: …` line. A stop is not a failure
/// and gets no such line, which is why the report's `stopped` outranks `ok` here.
fn render_failure_note(render: &crate::produce_flow::Rendered) -> Option<String> {
    if render.ok || render.stopped {
        return None;
    }
    Some(render.size.clone())
}

/// F5.6's half of the run, forwarded to `produce_languages::upload_half`, which builds the brief, asks
/// the model and drives the picture. It reports its own failure; `finish_produce` logs it with the spec's
/// tail and the render carries on.
fn words_half(run: &crate::produce_flow::Run) -> Result<(), String> {
    crate::ui::produce_languages::upload_half(run)
}
