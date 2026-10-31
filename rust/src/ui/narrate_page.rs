//! §07-narrate#1-screen: the Narrate page's surface, the 22 numbered widgets laid out per spec/inventory/narrate.md
//! §A.1 (horizontal paned: left = tick, preview, transport, voice picker, take band, sample, pitch; right = lines column).
//! No rule lives here: numbers, labels and sentences come from [`crate::narrate_screen`] or [`crate::narrate_details`]
//! (spec/00-principles.md §5). The page renders a state the flows publish through [`set_state`], as the effects lane does.

use adw::prelude::*;
use gtk4 as gtk;

use crate::cut::Seg;
use crate::narrate_details;
use crate::narrate_edit;
use crate::narrate_preview_leg::{self, PreviewPage};
use crate::narrate_screen::{self, Audition, Fit};
use crate::narration::Entry;
use crate::ui::narrate_page_speak;
use crate::ui::narrate_picker;
use crate::ui::narrate_volume;
use crate::voice_ref;

#[derive(Debug, Clone, Default)]
pub struct NarrateState {
    pub entries: Vec<Entry>,
    pub segs: Vec<Seg>,
    pub takes: Vec<(f64, f64)>,
    pub voice: String,
    pub pitch: f64,
    /// P.policy.ttsLanguage; empty means none stated and `narrate_tts::language` answers its fallback. Never hard-coded: a model told to speak English reads Polish spelling as English.
    pub language: String,
    /// Session second under the red line. Server health is asked on every speak (`speak_leg`), not carried here, as a published answer goes stale.
    pub session: f64,
    pub cut_at: f64,
    pub length: f64,
    pub clips: usize,
    pub unwritten: usize,
    pub off_cut: usize,
    pub has_cut: bool,
    pub has_timeline: bool,
    pub narrators: usize,
    pub voice_files: Vec<String>,
    pub recording_covers_clip: bool,
    pub playing: bool,
    /// §5: has this preview been cued and not ⏹-ed since. `started` (not `playing`) makes the run bar serve the preview rather than the step.
    pub started: bool,
    pub busy: bool,
    /// Session spans a recording covers (F4.5 S1 checks before cueing). Empty means nothing was published, read as full coverage.
    pub covered_spans: Vec<(f64, f64)>,
}

thread_local! {
    static NARRATE_STATE: std::cell::RefCell<NarrateState> =
        const { std::cell::RefCell::new(NarrateState {
            entries: Vec::new(),
            segs: Vec::new(),
            takes: Vec::new(),
            voice: String::new(),
            pitch: 0.0,
            session: 0.0,
            cut_at: 0.0,
            length: 0.0,
            clips: 0,
            unwritten: 0,
            off_cut: 0,
            has_cut: false,
            has_timeline: false,
            narrators: 0,
            voice_files: Vec::new(),
            recording_covers_clip: true,
            playing: false,
            started: false,
            busy: false,
            covered_spans: Vec::new(),
            language: String::new(),
        }) };
}

pub fn set_state(next: NarrateState) {
    NARRATE_STATE.with(|held| *held.borrow_mut() = next);
}

pub fn read_state() -> NarrateState {
    NARRATE_STATE.with(|held| held.borrow().clone())
}

pub(crate) fn mutate<T>(f: impl FnOnce(&mut NarrateState) -> T) -> T {
    NARRATE_STATE.with(|held| f(&mut held.borrow_mut()))
}

/// Re-publish the line rows from a re-read narration (F0.11 S2), keeping the other fields; the page draws its rows from this state.
pub fn publish_lines(entries: Vec<Entry>) {
    mutate(|s| s.entries = entries);
}

thread_local! {
    static PAGE_CSS_LOADED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// The page's ground colour, installed once per process: the preview frame and take band are `DrawingArea`s that would render as window grey.
fn install_page_css_once() {
    if PAGE_CSS_LOADED.with(|cell| cell.get()) {
        return;
    }
    // A line's text box is framed (§1 badge 20, `spec/img/07-narrate.png`): unframed, a white box on the white lines column cannot be seen.
    let css = ".narrate-plate { background-color: #1c1c1d; }\n\
               #narrate-lines textview.line-text { border: 1px solid alpha(currentColor, 0.25); \
               border-radius: 6px; padding: 4px; }\n";
    let provider = gtk::CssProvider::new();
    provider.load_from_string(css);
    gtk::style_context_add_provider_for_display(
        &gtk::gdk::Display::default().expect("a display to style"),
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    PAGE_CSS_LOADED.with(|cell| cell.set(true));
}

/// The preview plate: 560 px at 16:9, so 560 * 9 / 16 = 315; also where §A.1's second divider opens (`narrate-left-split`).
pub(crate) const PREVIEW_FRAME_W_PX: i32 = 560;
pub(crate) const PREVIEW_FRAME_H_PX: i32 = 315;

const COLUMN_MIN_W_PX: i32 = crate::ui::cut_zoom_hand::PREVIEW_PANEL_W as i32;
const COLUMN_MIN_H_PX: i32 = crate::ui::cut_zoom_hand::PREVIEW_PANEL_H as i32;

pub(crate) const TRANSPORT_ROW_PX: i32 = 34;

pub(crate) const TICK_ROW_PX: i32 = 27;

pub(crate) const COLUMN_GAP_PX: i32 = 8;

pub fn build() -> gtk::Widget {
    install_page_css_once();
    let split = gtk::Paned::new(gtk::Orientation::Horizontal);
    split.set_position(PREVIEW_FRAME_W_PX);
    // NOT `true`: §1 sends extra window width to the RIGHT, so the lines column grows; otherwise the rows were starved.
    split.set_resize_start_child(false);
    split.set_shrink_start_child(false);
    split.set_wide_handle(false);

    // §1: ABOVE the second divider (opened 420 px tall) the tick, preview and transport; BELOW it the voice picker onward.
    // The halves carry `left`'s width flags (`false` gave a ~140 px column), and 560 px is also a REQUEST because the take
    // band sizes its lane off the width its scroller got (`narrate_band::note_lane_size`).
    let upper = gtk::Box::new(gtk::Orientation::Vertical, 8);
    upper.set_widget_name("narrate-upper");
    let lower = gtk::Box::new(gtk::Orientation::Vertical, 8);
    lower.set_widget_name("narrate-lower");
    for half in [&upper, &lower] {
        half.set_hexpand(true);
        half.set_size_request(COLUMN_MIN_W_PX, -1);
        half.set_halign(gtk::Align::Fill);
    }
    let vsplit = gtk::Paned::new(gtk::Orientation::Vertical);
    vsplit.set_widget_name("narrate-left-split");
    // §A.1's opening as one formula: tick (`TICK_ROW_PX`) + gap + frame (`PREVIEW_FRAME_H_PX`) + gap + transport
    // (`TRANSPORT_ROW_PX`) + gap = 400, just under the transport as `spec/img/07-narrate.png` draws the handle.
    vsplit.set_position(
        PREVIEW_FRAME_H_PX
            + TRANSPORT_ROW_PX
            + TICK_ROW_PX
            + 2 * COLUMN_GAP_PX
            + crate::ui::margins::DIVIDER_GAP_PX,
    );
    vsplit.set_wide_handle(false);
    // Neither half goes under its own minimum: shrinkable, the upper half was cut off from the top.
    vsplit.set_shrink_start_child(false);
    vsplit.set_shrink_end_child(false);
    // NOT `true`: extra height would go to the fixed-size UPPER half and show as a band between transport and voice picker.
    vsplit.set_resize_start_child(false);
    vsplit.set_start_child(Some(&upper));
    vsplit.set_end_child(Some(&lower));

    let top_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    top_row.append(&spacer);
    let game_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    game_row.set_widget_name("narrate-game-row");
    let game_label = gtk::Label::new(Some("Game audio:"));
    game_label.add_css_class("dim-label");
    let game = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 1.0, 0.02);
    game.set_widget_name("narrate-game-audio");
    game.set_size_request(140, -1);
    game.set_tooltip_text(Some(crate::produce_screen::tooltip("Game audio")));
    game_row.append(&game_label);
    game_row.append(&game);
    top_row.append(&game_row);
    upper.append(&top_row);
    // §1 (**22**): the bracketed halves are arithmetic over the session files that a flow never publishes, so the page derives
    // them when it learns what the project holds, via `narrate_seed::seed_from_disk`; counts a caller published are kept.

    let preview = gtk::Box::new(gtk::Orientation::Vertical, 0);
    preview.set_widget_name("narrate-preview");
    // The plate is the column's width at 16:9 (560 → 315). NOT `vexpand`: an expanding frame grew to ~400 px and pushed the
    // transport under the fold. `COLUMN_MIN_*` is only its floor; pinned at 560 by 315 it held the divider off everything to its left.
    preview.set_size_request(COLUMN_MIN_W_PX, COLUMN_MIN_H_PX);
    preview.set_vexpand(true);
    // NOT `Start`: the column would shrink to its widest child (~140 px) and the picture become a sliver.
    preview.set_halign(gtk::Align::Fill);
    preview.set_tooltip_text(Some(
        "The finished frame with the narration mixed over the cut's sound",
    ));
    let picture = gtk::DrawingArea::new();
    picture.set_widget_name("narrate-picture");
    // NOT `vexpand`, but with an explicit request: an expanding child rendered ~1170 px tall, and with none GTK gave the area 2 px.
    picture.set_vexpand(false);
    picture.set_size_request(COLUMN_MIN_W_PX, COLUMN_MIN_H_PX);
    // A bare `DrawingArea` paints nothing; CSS gives it the dark 16:9 field with a stroke (§2's image).
    picture.add_css_class("narrate-plate");
    // F4.5 S1 (`click the picture or ▶`): the picture is its own toggle; unlike the Cut page's `preview-panel` it carries no drag, so no second gesture races.
    let picture_click = gtk::GestureClick::new();
    picture_click.connect_released(move |_click, _n, _x, _y| {
        press_preview_picture(&crate::ui::window::main_window());
    });
    picture.add_controller(picture_click);
    // §1 badge 2: a `Picture` under the painter shows the extracted frame or the built-in player's frames (`narrate_picture`); the painter draws what the cut lays over the footage.
    let video = gtk::Picture::new();
    video.set_widget_name("narrate-video");
    video.set_content_fit(gtk::ContentFit::Contain);
    video.set_can_shrink(true);
    video.set_size_request(COLUMN_MIN_W_PX, COLUMN_MIN_H_PX);
    video.add_css_class("video-plate");
    let stack = gtk::Overlay::new();
    stack.set_vexpand(true);
    stack.set_child(Some(&video));
    stack.add_overlay(&picture);
    picture.remove_css_class("narrate-plate");
    preview.append(&stack);
    upper.append(&preview);

    let transport = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    transport.set_widget_name("narrate-transport");
    transport.set_halign(gtk::Align::Fill);
    // NOT `Start`: the row must fill the column so the slider gets the slack (§1 puts slider, clock and volume on this row).
    for (name, label, tip) in [
        (
            "narrate-back-3s",
            "\u{219a} 3 s",
            "back three seconds \u{2014} the reach the row's \u{25b6} auditions from",
        ),
        ("narrate-play", "\u{25b6}", "play or pause the preview"),
        (
            "narrate-forward-3s",
            "3 s \u{21a3}",
            "forward three seconds",
        ),
    ] {
        let button = gtk::Button::with_label(label);
        button.set_widget_name(name);
        button.set_tooltip_text(Some(tip));
        transport.append(&button);
    }
    let add_line = gtk::Button::from_icon_name(narrate_screen::ADD_LINE_ICON);
    add_line.set_widget_name("narrate-add-line");
    add_line.set_tooltip_text(Some("Add a line at this second"));
    transport.append(&add_line);

    let slider = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 1.0, 0.1);
    slider.set_draw_value(false);
    slider.set_widget_name("narrate-slider");
    slider.set_hexpand(true);
    // §A.4: the bar is "the cut, end to end", so its scale is the cut's length; `refresh` keeps it in step.
    slider.set_tooltip_text(Some(
        "The cut's own clock \u{2014} what the edit removed is not on this bar",
    ));
    transport.append(&slider);
    let clock = gtk::Label::new(Some(&narrate_screen::clock_line(0.0, 0.0, 0.0)));
    clock.set_widget_name("narrate-clock");
    clock.set_xalign(0.0);
    transport.append(&clock);

    // (**6**) the shared preview volume; `narrate_volume::ensure_slot` pushed the slot in `build_window`, before any page was built.
    let volume = narrate_volume::build();
    transport.append(&volume);
    upper.append(&transport);

    // **7** who speaks, **8** the take band's buttons and **9** Add file… on ONE row (§1); the picker is `narrate_picker`'s.
    let voice_row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    voice_row.set_widget_name("voice-row");
    let picker = narrate_picker::build();
    picker.set_hexpand(true);
    voice_row.append(&picker);
    let take_add = gtk::Button::from_icon_name(narrate_screen::ADD_LINE_ICON);
    take_add.set_widget_name("take-add");
    take_add.set_tooltip_text(Some(narrate_details::take_add_hint()));
    let take_remove = gtk::Button::from_icon_name(narrate_screen::TAKE_REMOVE_ICON);
    take_remove.set_widget_name("take-remove");
    take_remove.set_tooltip_text(Some(narrate_details::take_remove_hint()));
    let take_play = gtk::Button::with_label("\u{25b6}");
    take_play.set_widget_name("take-play");
    take_play.set_tooltip_text(Some("Play the takes from the red bar"));
    voice_row.append(&take_add);
    voice_row.append(&take_remove);
    voice_row.append(&take_play);
    let add_file = gtk::Button::with_label("Add file\u{2026}");
    add_file.set_widget_name("voice-add-file");
    add_file.set_tooltip_text(Some(&narrate_screen::add_file_tip()));
    voice_row.append(&add_file);
    lower.append(&voice_row);

    let (band_scroller, band) = crate::ui::narrate_band::build_band();
    band.set_tooltip_text(Some(narrate_screen::band_status(&[])));
    lower.append(&band_scroller);

    let sample_row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    sample_row.set_widget_name("sample-row");
    // No `halign = Start` on this row: it must fill the column, or it squeezed into a few pixels and the pitch part fell to the next line.
    let sample = gtk::Entry::new();
    sample.set_widget_name("sample-sentence");
    sample.set_text(narrate_screen::DEFAULT_SAMPLE);
    sample.set_tooltip_text(Some("Spoken in the selected voice"));
    sample.set_hexpand(true);
    sample_row.append(&sample);
    for (name, label, tip) in [
        ("sample-play", "\u{25b6}", "speak the sample in this voice"),
        ("sample-stop", "\u{23f9}", "stop the sample"),
        ("sample-reroll", "\u{27f3}", "a new take of the sample"),
    ] {
        let button = gtk::Button::with_label(label);
        button.set_widget_name(name);
        button.set_tooltip_text(Some(tip));
        sample_row.append(&button);
    }
    // **13** pitch (`narrate_pitch`), seeded from the state so control and `narrate/pitch.txt` agree from the first paint.
    sample_row.append(&crate::ui::narrate_pitch::build_row(read_state().pitch));
    lower.append(&sample_row);

    let scroller = gtk::ScrolledWindow::new();
    // Vertical `Automatic`, not `External`: nothing owns the vertical adjustment, and only the first row appeared.
    // Horizontal `Never`: a wrapped monospace `TextView` reports a huge NATURAL width, which pushed the row's buttons past the
    // window's edge; `Never` makes the column's width §1's "THE ROW'S FULL WIDTH".
    scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroller.set_hexpand(true);
    scroller.set_vexpand(true);
    // Bounded content height: an unbounded ScrolledWindow reports its child's FULL height as its minimum (a 1280x800 request rendered 2669x3025).
    scroller.set_min_content_height(240);
    scroller.set_max_content_height(560);
    scroller.set_propagate_natural_height(false);
    // §1: lists take the extra height; `vexpand` lets the list fill the pane, the bounds above decide when its scrollbar appears.
    scroller.set_vexpand(true);
    scroller.set_vexpand_set(true);
    // A floor on the column is charged to the page half of `main-divider` (a `gtk::Paned` splits leftover by SHARE between expanding children):
    // §1 wants pages to "take all the rest of the height", so pages ask for nothing they can scroll instead.
    scroller.set_size_request(-1, 560);
    let lines = gtk::ListBox::new();
    lines.set_widget_name("narrate-lines");
    scroller.set_child(Some(&lines));

    // §1 badge **22**'s Inputs/Outputs readout is the window's BOTTOM BAR (§A.8, `window_bench::paint_readouts`); a copy here could drift.
    let right = gtk::Box::new(gtk::Orientation::Vertical, 6);
    right.set_hexpand(true);
    right.append(&scroller);

    let left = gtk::ScrolledWindow::new();
    left.set_widget_name("narrate-left-scroller");
    left.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Never);
    left.set_min_content_width(COLUMN_MIN_W_PX);
    left.set_child(Some(&vsplit));
    split.set_start_child(Some(&left));
    split.set_end_child(Some(&right));
    upcast_widget(split)
}

fn upcast_widget(w: gtk::Paned) -> gtk::Widget {
    w.upcast()
}

thread_local! {
    /// True while the PAGE moves the scrub thumb: a programmatic `set_value` fires `value-changed` like a drag, which would seek.
    static SYNCING_SCRUB: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn sync_scrub(window: &adw::ApplicationWindow, s: &NarrateState) {
    let Some(slider) =
        widget_in(window, "narrate-slider").and_then(|w| w.downcast::<gtk::Scale>().ok())
    else {
        return;
    };
    SYNCING_SCRUB.with(|held| held.set(true));
    slider.set_range(0.0, s.length.max(1.0));
    if (slider.value() - s.session).abs() > 0.01 {
        slider.set_value(s.session);
    }
    SYNCING_SCRUB.with(|held| held.set(false));
}

fn scrub_is_being_painted() -> bool {
    SYNCING_SCRUB.with(std::cell::Cell::get)
}

pub(crate) fn paint_play_face(window: &adw::ApplicationWindow, playing: bool) {
    if let Some(play) = crate::ui::line_step_button(window, "narrate-play") {
        crate::ui::cut_record_play::set_steady_label(
            &play,
            if playing { "\u{23f8}" } else { "\u{25b6}" },
            &["\u{25b6}", "\u{23f8}"],
        );
    }
}

/// Everything that shows where the line is: clock, slider, painted frame, take band. Rows are left alone, as a rebuild under a click takes the focus.
fn paint_head(window: &adw::ApplicationWindow) {
    let s = read_state();
    if let Some(clock) = label(window, "narrate-clock") {
        clock.set_text(&clock_text(&s));
    }
    sync_scrub(window, &s);
    crate::ui::narrate_band::follow_head(s.session);
    crate::ui::narrate_picture::repaint(window);
    crate::ui::narrate_band::repaint(window);
}

/// §1 badge 5's clock, "session · cut/length"; the middle field is where `session` falls in the finished video, read off the page's clips at every paint.
/// In a removed stretch it is the kept length before the line; with no clips published the stored number stands.
pub(crate) fn clock_text(s: &NarrateState) -> String {
    let cut_at = if s.segs.is_empty() {
        s.cut_at
    } else {
        crate::cut_play::cut_time(&s.segs, s.session).unwrap_or_else(|| {
            s.segs
                .iter()
                .filter(|seg| seg.e > seg.s && seg.e <= s.session)
                .map(|seg| seg.e - seg.s)
                .sum()
        })
    };
    narrate_screen::clock_line(s.session, cut_at, s.length)
}

/// The session's recordings were measured: the take band's tape follows, when this page is on screen. Another page is left alone (Produce's run must not wait behind it).
pub fn recordings_landed() {
    if let Some(window) = crate::ui::window::built_window() {
        if crate::ui::state(&window).page == crate::shell::Page::Narrate
            && widget_in(&window, "take-band").is_some()
        {
            refresh(&window);
        }
    }
}

pub fn refresh(window: &adw::ApplicationWindow) {
    install_page_css_once();
    let s = read_state();
    if let Some(clock) = label(window, "narrate-clock") {
        clock.set_text(&clock_text(&s));
    }
    paint_play_face(window, s.playing);
    paint_game_audio(window);
    if let Some(band) = widget_in(window, "take-band") {
        band.set_tooltip_text(Some(narrate_screen::band_status(&s.takes)));
    }
    crate::ui::narrate_band::repaint(window);
    crate::ui::narrate_band::show_only_with_recording(window);
    crate::ui::narrate_picture::repaint(window);
    sync_scrub(window, &s);
    crate::ui::narrate_band::follow_head(s.session);
    // The slider shows the one volume every preview hears (§A.4); mirrored only when it differs, as writing a scale back reads as a drag.
    narrate_volume::mirror(window);
    narrate_picker::follow(window, s.narrators, &s.voice_files, &s.voice);
    draw_rows(window, &s);
    // Always re-wired: rows are rebuilt whole, and a guard on the count would leave a same-length refresh with dead buttons.
    wire_row_buttons(window);
}

thread_local! {
    /// Set while [`draw_rows`] rebuilds the list: GTK self-selects a row when the selected one is removed, and F4.5 S5's jump belongs to a person's selection only.
    static REBUILDING_ROWS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn draw_rows(window: &adw::ApplicationWindow, s: &NarrateState) {
    let list = widget_in(window, "narrate-lines").and_then(|w| w.downcast::<gtk::ListBox>().ok());
    let chosen = list
        .as_ref()
        .and_then(|list| list.selected_row())
        .map(|row| row.widget_name());
    let focused = list
        .as_ref()
        .is_some_and(|list| list.focus_child().is_some() || list.has_focus());
    REBUILDING_ROWS.with(|cell| cell.set(true));
    draw_rows_now(window, s);
    if let (Some(list), Some(name)) = (list.as_ref(), chosen) {
        let mut index = 0;
        while let Some(row) = list.row_at_index(index) {
            if row.widget_name() == name {
                list.select_row(Some(&row));
                if focused {
                    row.grab_focus();
                }
                break;
            }
            index += 1;
        }
    }
    // GTK settles the focus of a removed row later in the same loop turn; the guard stays up until then.
    glib::idle_add_local_once(|| REBUILDING_ROWS.with(|cell| cell.set(false)));
}

fn draw_rows_now(window: &adw::ApplicationWindow, s: &NarrateState) {
    let Some(list) =
        widget_in(window, "narrate-lines").and_then(|w| w.downcast::<gtk::ListBox>().ok())
    else {
        return;
    };
    // Rebuilt whole: a moved line changes the sort order, and the index each button carries must match its entry.
    while let Some(row) = list.row_at_index(0) {
        list.remove(&row);
    }
    if s.entries.is_empty() {
        // The empty page says something where its rows go; no buttons are wired into this row (`wire_row_buttons` never reaches it).
        let placeholder = gtk::ListBoxRow::new();
        placeholder.set_widget_name("narrate-lines-empty");
        let note = gtk::Label::new(Some(narrate_screen::no_lines_note()));
        note.set_xalign(0.0);
        note.add_css_class("dim-label");
        placeholder.set_child(Some(&note));
        list.append(&placeholder);
        return;
    }
    let mut ordered: Vec<(usize, &Entry)> = s.entries.iter().enumerate().collect();
    ordered.sort_by(|(_, a), (_, b)| {
        a.s.partial_cmp(&b.s)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.at.partial_cmp(&b.at).unwrap_or(std::cmp::Ordering::Equal))
    });
    for (index, entry) in ordered.clone() {
        let row = crate::ui::narrate_row::build(
            index,
            entry,
            entry.s + entry.at,
            fit_for(entry, &ordered, &s.voice),
        );
        list.append(&row);
    }
}

/// The row's fit from the render's own ladder (F4.3): every line of THIS clip goes through `narrate_screen::mirror_fit`
/// together, so a warning is what the render will do (incl. speeding the narration up). Speech length is characters over the
/// take's MEASURED rate ([`speech_for`]), else [`narrate_screen::SPEECH_CHARS_PER_SECOND`]. An empty line and a caption
/// (`pos` set) are never spoken and answer `Fit::Fits`.
fn fit_for(entry: &Entry, all: &[(usize, &Entry)], voice: &str) -> Fit {
    if entry.text.is_empty() || !entry.pos.is_empty() {
        return Fit::Fits;
    }
    let tree = crate::ui::window::narrate_session_tree();
    let lines: Vec<(usize, f64, f64)> = all
        .iter()
        .filter(|(_, other)| other.s == entry.s && other.e == entry.e)
        .map(|(index, other)| {
            let spoken = if other.text.is_empty() || !other.pos.is_empty() {
                0.0
            } else {
                speech_for(tree.as_ref(), other, voice)
            };
            (*index, other.at, spoken)
        })
        .collect();
    narrate_screen::mirror_fit((entry.e - entry.s).max(0.0), &lines)
}

/// How long one line runs on the page: §F4.3's measured rate once its wav is on disk, else §C.2's 15 chars/s, clamped to 8..28.
fn speech_for(tree: Option<&crate::layout::Tree>, entry: &Entry, voice: &str) -> f64 {
    let chars = entry.text.chars().count();
    let measured = tree.and_then(|tree| {
        crate::narrate_tts::take_seconds(tree, &crate::narration::key_for(entry, Some(voice)))
    });
    // Only a wild measurement (past 8 or 28 chars/s) is moved; no wav means §C.2's estimate answers.
    match narrate_screen::measured_char_rate(chars, measured.unwrap_or(0.0)) {
        Some(rate) if measured.is_some() => (chars as f64 / rate).max(f64::EPSILON),
        _ => narrate_screen::page_speech_seconds(chars, narrate_screen::SPEECH_CHARS_PER_SECOND),
    }
}

pub(crate) fn widget_in(window: &adw::ApplicationWindow, name: &str) -> Option<gtk::Widget> {
    let content = window.content()?;
    crate::ui::window::find_widget_by_name(&content, name)
}

fn label(window: &adw::ApplicationWindow, name: &str) -> Option<gtk::Label> {
    widget_in(window, name)?.downcast().ok()
}

/// F4.5: the page as the preview leg sees it, a seam like [`crate::cut_play_leg::CutPreviewPage`]; every method reads published state.
pub struct PageDriver(adw::ApplicationWindow);

impl PageDriver {
    pub(crate) fn for_window(window: &adw::ApplicationWindow) -> Self {
        PageDriver(window.clone())
    }
}

impl narrate_preview_leg::PreviewPage for PageDriver {
    fn running(&self) -> bool {
        read_state().playing
    }

    fn head(&self) -> f64 {
        read_state().session
    }

    fn set_head(&self, at: f64) {
        mutate(|s| {
            s.session = at;
            if let Some(cut_at) = crate::cut_play::cut_time(&s.segs, at) {
                s.cut_at = cut_at;
            }
        });
        paint_head(&self.0);
    }

    fn tree(&self) -> Option<crate::layout::Tree> {
        crate::ui::window::narrate_session_tree()
    }

    fn cut(&self) -> crate::cut::Cut {
        // The session's own cut file wins: the page's `segs` mirror only what a flow last published, and a lap must hear the saved effects.
        self.tree()
            .map(|tree| crate::cut::load(&tree).unwrap_or_default())
            .unwrap_or_else(|| narrate_preview_leg::cut_of(read_state().segs, Vec::new()))
    }

    fn entries(&self) -> Vec<Entry> {
        read_state().entries
    }

    fn game_volume(&self) -> f64 {
        // With nothing to read, the project's OWN default rather than 0.0, which would duck the bed into silence.
        self.tree()
            .map(|tree| narrate_preview_leg::game_volume_for(&tree))
            .unwrap_or_else(|| crate::project::Produce::default().game_volume)
    }

    fn volume(&self) -> f64 {
        // Read from the ONE holder (§A.4, F2.5 S6): two copies would be two answers, and an audition would differ from its preview (§C.4).
        crate::ui::preview_volume(&self.0)
    }

    fn say(&self, said: &str) {
        say(&self.0, said);
    }

    /// §5: the tick ended the preview itself; without clearing `started` the bar would keep answering for a dead preview.
    fn stopped(&self) {
        mutate(|state| {
            state.playing = false;
            state.started = false;
        });
        super::narrate_page_transport::publish_transport();
    }
}

pub fn offer_voice(name: &str) {
    mutate(|state| {
        if !state.voice_files.iter().any(|held| held == name) {
            state.voice_files.push(name.to_string());
            state.voice_files.sort();
        }
    });
}

pub(crate) fn say(window: &adw::ApplicationWindow, said: &str) -> String {
    if let Some(status) = crate::ui::window::find_status(window.upcast_ref()) {
        status.set_text(said);
    }
    crate::ui::window::log_line(said);
    said.to_string()
}

pub use crate::ui::narrate_page_seek::{
    note_scrub_drag, pending_scrub_for_test, press_back, press_forward, seek_preview,
};

pub use crate::ui::narrate_page_transport::{
    narrate_preview_playing, press_narrate_play, press_preview_picture, press_preview_stop,
};

pub use crate::ui::narrate_page_take::{
    press_sample_play, press_sample_reroll, press_sample_stop, press_take_add, press_take_play,
    press_take_remove,
};

pub fn press_narrate_add_line(window: &adw::ApplicationWindow) -> String {
    // §F4.7: "preview paused first, picture and voice" — ⏹'s own door stops both players.
    if read_state().playing {
        press_preview_stop(window);
    }
    let s = read_state();
    match narrate_screen::add_at_playhead(s.session, &s.segs, &s.entries) {
        Ok(at) => {
            let clip = s
                .segs
                .iter()
                .find(|seg| seg.s <= at && at < seg.e)
                .cloned()
                .unwrap_or_default();
            let offset = (at - clip.s).max(0.0);
            let said = if let Some(index) = narrate_edit::silent_marker_on(clip.s, &s.entries) {
                mutate(|state| state.entries[index].at = offset);
                format!(
                    "the silent marker became this line at {}",
                    narrate_screen::time_field(at)
                )
            } else {
                mutate(|state| {
                    state.entries.push(Entry {
                        s: clip.s,
                        e: clip.e,
                        at: offset,
                        ..Default::default()
                    });
                    crate::narration::sort_entries(&mut state.entries);
                });
                format!("a line starts at {}", narrate_screen::time_field(at))
            };
            refresh(window);
            crate::ui::narrate_text::note_structure_change(window);
            say(window, &said)
        }
        Err(refused) => {
            // §F4.7's node J: a press within 1 s of a line takes the playhead to that line and repeats the sentence naming it.
            if let Some(start) = narrate_screen::near_line_start(s.session, &s.entries) {
                mutate(|state| {
                    state.session = start;
                    state.cut_at = start;
                });
                refresh(window);
            }
            say(window, &refused)
        }
    }
}

pub(crate) fn line_covered(s: &NarrateState, at: f64) -> bool {
    s.recording_covers_clip
        && (s.covered_spans.is_empty()
            || s.covered_spans
                .iter()
                .any(|(from, to)| *from <= at && at < *to))
}

pub fn press_line_speak(window: &adw::ApplicationWindow, index: usize) -> String {
    let s = read_state();
    let Some(entry) = s.entries.get(index) else {
        return say(window, narrate_details::nothing_picked());
    };
    let blank = entry.text.is_empty() && entry.pos.is_empty();
    let caption = !entry.pos.is_empty();
    let prev_speaks_at = s
        .entries
        .iter()
        .filter(|other| other.s == entry.s)
        .map(|other| other.s + other.at)
        .filter(|other| *other < entry.s + entry.at)
        .fold(0.0_f64, f64::max);
    let (case, said) = narrate_screen::audition(
        false,
        blank,
        caption,
        s.busy,
        index + 1,
        prev_speaks_at,
        entry.s + entry.at,
    );
    let (case, said) = if line_covered(&s, entry.s + entry.at) {
        (case, said)
    } else {
        narrate_screen::audition_without_recording(case, index + 1)
    };
    let line = match said {
        Some(said) => said,
        None => match case {
            Audition::Speak { from } => return speak_this_line(window, &s, entry, from),
            _ => narrate_screen::spoken_alone(index + 1),
        },
    };
    say(window, &line)
}

fn speak_this_line(
    window: &adw::ApplicationWindow,
    s: &NarrateState,
    entry: &Entry,
    from: f64,
) -> String {
    let driver = PageDriver(window.clone());
    let (game_volume, preview_volume) = (driver.game_volume(), driver.volume());
    narrate_page_speak::speak_and_audition(window, s, entry, from, game_volume, preview_volume)
}

pub fn press_line_reroll(window: &adw::ApplicationWindow, index: usize) -> String {
    let s = read_state();
    // §6: with the captions voice nothing has a take. FIRST, before any roll bump or dial, so no request goes to a voice that does not exist.
    if crate::narrate_run::voice_is_captions_only(&s.voice) {
        return say(window, narrate_details::re_roll_captions_refusal());
    }
    let has_line = s
        .entries
        .get(index)
        .map(|entry| !entry.text.is_empty())
        .unwrap_or(false);
    let Some(refused) = narrate_screen::re_roll_refusal(index + 1, has_line) else {
        // §F4.7: roll + 1 first, so the next take is keyed on the new roll and the old wav stays (an undo reads it back).
        mutate(|state| {
            if let Some(entry) = state.entries.get_mut(index) {
                entry.roll = narrate_edit::reroll(entry.roll);
            }
        });
        crate::ui::narrate_text::note_structure_change(window);
        let after = read_state();
        let Some(entry) = after.entries.get(index) else {
            return say(window, narrate_details::nothing_picked());
        };
        refresh(window);
        say(window, &narrate_details::new_take_status(index));
        // A re-roll speaks the NEW take in place (§C.3): start at the line's own offset, not 3 s ahead.
        return speak_this_line(window, &after, entry, entry.s + entry.at);
    };
    say(window, &refused)
}

pub fn press_pitch(window: &adw::ApplicationWindow, delta: f64) -> String {
    let pitch = mutate(|s| {
        s.pitch = narrate_screen::clamp_pitch(s.pitch + delta);
        s.pitch
    });
    // S4: a pitch change invalidates only the shifted copy. The value goes to `narrate/pitch.txt` (§07-narrate#3-data); a write failure goes to the log.
    if let Some(why) = crate::ui::narrate_seed::note_pitch_for_session(pitch) {
        crate::ui::window::log_line(&format!("!!! could not save the pitch -- {why}"));
    }
    let voice = read_state().voice;
    crate::ui::narrate_pitch::show_value(window, pitch);
    say(window, &narrate_details::voice_line(&voice, pitch))
}

/// F4.6 S1: one choice, resolved against what Prepare tagged and the voices folder, then persisted and reported.
/// A refusal changes nothing: a half-switched voice would show one voice and clone from another.
pub fn press_voice(window: &adw::ApplicationWindow, id: &str) -> String {
    let s = read_state();
    // The folder named in the refusal is the one the app reads (§7: `AUDIOCPP_VOICES`, else the legacy models root, else Flatpak/dev default).
    let dir = voices_dir_label();
    match voice_ref::pick_voice(id, s.narrators, &s.voice_files, &dir) {
        voice_ref::VoiceChoice::Refused(why) => say(window, &why),
        voice_ref::VoiceChoice::Silent(said) => {
            let _ = crate::ui::narrate_seed::note_voice_for_session(id);
            mutate(|state| state.voice = id.to_string());
            // A new voice means a different recording: the band's hand (selection, zoom, red bar) must not carry over.
            crate::ui::narrate_band::clear_hand();
            refresh(window);
            say(window, &said)
        }
        voice_ref::VoiceChoice::Chosen { id, said } => {
            if let Err(why) = crate::ui::narrate_seed::note_voice_for_session(&id) {
                crate::ui::window::log_line(&why);
                return say(window, voice_ref::INSTALL_FAILED);
            }
            mutate(|state| state.voice = id.clone());
            crate::ui::narrate_band::clear_hand();
            refresh(window);
            say(window, &said)
        }
    }
}

pub(crate) fn voices_dir_label() -> String {
    crate::ui::narrate_voices::voices_folder()
        .display()
        .to_string()
}

pub fn wire(window: &adw::ApplicationWindow) {
    let win = window.clone();
    // Loaded here, once the window knows its folder (during the build it answered the previous window's), and before the tick is wired.
    if let Some(tree) = crate::ui::window::narrate_session_tree() {
        set_state(crate::ui::narrate_seed::load_page(
            &tree,
            crate::ui::window::line_position(window).t,
        ));
    }
    // Set before its handler is connected so this is no drag.
    if let Some(pitch) =
        widget_in(&win, "pitch-slider").and_then(|w| w.downcast::<gtk::Scale>().ok())
    {
        pitch.set_value(read_state().pitch);
        crate::ui::narrate_pitch::show_value(&win, read_state().pitch);
    }
    if let Some(button) = crate::ui::line_step_button(&win, "narrate-back-3s") {
        let w = win.clone();
        button.connect_clicked(move |_| {
            press_back(&w);
        });
    }
    if let Some(button) = crate::ui::line_step_button(&win, "narrate-forward-3s") {
        let w = win.clone();
        button.connect_clicked(move |_| {
            press_forward(&w);
        });
    }
    if let Some(button) = crate::ui::line_step_button(&win, "narrate-play") {
        let w = win.clone();
        button.connect_clicked(move |_| {
            press_narrate_play(&w);
        });
    }
    if let Some(button) = crate::ui::line_step_button(&win, "narrate-add-line") {
        let w = win.clone();
        button.connect_clicked(move |_| {
            press_narrate_add_line(&w);
        });
    }
    narrate_picker::wire(&win);
    wire_game_audio(&win);
    narrate_volume::wire(&win);
    if let Some(button) = crate::ui::line_step_button(&win, "take-add") {
        let w = win.clone();
        button.connect_clicked(move |_| {
            let selection = crate::ui::narrate_band::live_selection().unwrap_or((0.0, 0.0));
            press_take_add(&w, selection);
        });
    }
    if let Some(button) = crate::ui::line_step_button(&win, "take-remove") {
        let w = win.clone();
        button.connect_clicked(move |_| {
            let selection = crate::ui::narrate_band::live_selection().unwrap_or((0.0, 0.0));
            press_take_remove(&w, selection);
        });
    }
    if let Some(button) = crate::ui::line_step_button(&win, "voice-add-file") {
        let w = win.clone();
        button.connect_clicked(move |_| {
            crate::ui::narrate_voices::press_add_file(&w);
        });
    }
    if let Some(button) = crate::ui::line_step_button(&win, "take-play") {
        let w = win.clone();
        button.connect_clicked(move |_| {
            press_take_play(&w);
        });
    }
    if let Some(button) = crate::ui::line_step_button(&win, "sample-play") {
        let w = win.clone();
        button.connect_clicked(move |_| {
            press_sample_play(&w);
        });
    }
    if let Some(button) = crate::ui::line_step_button(&win, "sample-stop") {
        let w = win.clone();
        button.connect_clicked(move |_| {
            press_sample_stop(&w);
        });
    }
    if let Some(button) = crate::ui::line_step_button(&win, "sample-reroll") {
        let w = win.clone();
        button.connect_clicked(move |_| {
            press_sample_reroll(&w);
        });
    }
    if let Some(pitch) =
        widget_in(&win, "pitch-slider").and_then(|w| w.downcast::<gtk::Scale>().ok())
    {
        let w = win.clone();
        // §1: the shift applies 400 ms after the last move: one re-clone per drag, not per pixel.
        pitch.connect_value_changed(move |scale| {
            let value = scale.value();
            glib::timeout_add_local(
                std::time::Duration::from_millis(narrate_screen::PITCH_APPLY_MS),
                {
                    let w = w.clone();
                    move || {
                        let pitch = mutate(|s| {
                            s.pitch = narrate_screen::clamp_pitch(value);
                            s.pitch
                        });
                        if let Some(why) = crate::ui::narrate_seed::note_pitch_for_session(pitch) {
                            crate::ui::window::log_line(&format!(
                                "!!! could not save the pitch -- {why}"
                            ));
                        }
                        let voice = read_state().voice;
                        crate::ui::narrate_pitch::show_value(&w, value);
                        say(&w, &narrate_details::voice_line(&voice, value));
                        glib::ControlFlow::Break
                    }
                },
            );
        });
    }
    crate::ui::narrate_band::attach(&win);
    crate::ui::narrate_picture::install_draw(&win);
    crate::ui::narrate_frame_wheel::install(&win);
    refresh(&win);
    wire_row_buttons(&win);

    // F4.5 S4: the scrub slider seeks through the transport buttons' seam, so the snapped second is what the player was asked for.
    if let Some(slider) =
        widget_in(&win, "narrate-slider").and_then(|w| w.downcast::<gtk::Scale>().ok())
    {
        let w = win.clone();
        slider.connect_value_changed(move |scale| {
            if scrub_is_being_painted() {
                return;
            }
            let target = scale.value();
            let forward = target >= read_state().session;
            // The bar's drag knows which clips the page shows; the session's cut.json is the fallback for the buttons.
            let segs = read_state().segs;
            note_scrub_drag(&w, target, Some(&segs), forward);
        });
    }

    // F4.5 S5 (§A.5): selecting a row is the only self-jump, to the line's lead-in (three seconds ahead unless they belong to the line before).
    if let Some(list) =
        widget_in(&win, "narrate-lines").and_then(|w| w.downcast::<gtk::ListBox>().ok())
    {
        let w = win.clone();
        list.connect_row_selected(move |list, row| {
            let Some(row) = row else { return };
            if REBUILDING_ROWS.with(|cell| cell.get()) {
                return;
            }
            if row.widget_name() == "narrate-lines-empty" {
                return;
            }
            // `row_at_index` is the only index lookup gtk4-rs 0.11 gives a ListBox, so walk the rows.
            let mut index = 0usize;
            loop {
                match list.row_at_index(index as i32) {
                    Some(candidate) if candidate == *row => break,
                    Some(_) => index += 1,
                    None => return,
                }
            }
            let entries = read_state().entries;
            if let Some(at) = narrate_preview_leg::lead_in_of(&entries, index) {
                mutate(|s| s.session = at);
                paint_head(&w);
            }
        });
    }

    narrate_preview_leg::start_tick(std::rc::Rc::new(PageDriver(win.clone())));
    // §5: `paint_run_bar` can only read a slot this window registered (the Cut page pushes its own, F2.1).
    crate::ui::window::register_preview_player_slot();
    // The slots default to `started: true`, which would hand the bar to a preview this window never cued (§5).
    crate::ui::narrate_page_transport::publish_transport();
    refresh(&win);
}

fn wire_row_buttons(window: &adw::ApplicationWindow) {
    let count = read_state().entries.len();
    for index in 0..count {
        if let Some(button) = crate::ui::line_step_button(window, &format!("line-speak-{index}")) {
            let w = window.clone();
            button.connect_clicked(move |_| {
                press_line_speak(&w, index);
            });
        }
        if let Some(button) = crate::ui::line_step_button(window, &format!("line-reroll-{index}")) {
            let w = window.clone();
            button.connect_clicked(move |_| {
                press_line_reroll(&w, index);
            });
        }
        if let Some(button) =
            crate::ui::line_step_button(window, &format!("line-add-below-{index}"))
        {
            let w = window.clone();
            button.connect_clicked(move |_| {
                crate::ui::narrate_lines::press_line_add_below(&w, index);
            });
        }
        if let Some(button) = crate::ui::line_step_button(window, &format!("line-remove-{index}")) {
            let w = window.clone();
            button.connect_clicked(move |_| {
                crate::ui::narrate_lines::press_line_remove(&w, index);
            });
        }
        if let Some(field) =
            crate::ui::narrate_page::widget_in(window, &format!("line-time-{index}"))
                .and_then(|w| w.downcast::<gtk::Entry>().ok())
        {
            let w = window.clone();
            field.connect_activate(move |field| {
                crate::ui::narrate_lines::press_line_move(&w, index, &field.text());
            });
        }
        // §F4.7 (**Text**): every keystroke goes through `note_typing` (parse tag, update entry, re-arm the 400 ms autosave).
        if let Some(field) =
            crate::ui::narrate_page::widget_in(window, &format!("line-text-{index}"))
                .and_then(|w| w.downcast::<gtk::TextView>().ok())
        {
            let w = window.clone();
            field.buffer().connect_changed(move |_| {
                let text = crate::ui::narrate_row::box_text(&field);
                crate::ui::narrate_text::note_typing(&w, index, &text);
            });
        }
    }
}

pub(crate) fn paint_game_audio(window: &adw::ApplicationWindow) {
    let Some(row) = widget_in(window, "narrate-game-row") else {
        return;
    };
    let Some(scale) =
        widget_in(window, "narrate-game-audio").and_then(|w| w.downcast::<gtk::Scale>().ok())
    else {
        return;
    };
    let level = crate::ui::window::narrate_session_tree()
        .map(|tree| crate::narrate_preview_leg::game_volume_for(&tree))
        .unwrap_or_else(|| crate::project::Produce::default().game_volume);
    GAME_GUARD.with(|guard| guard.set(true));
    if (scale.value() - level).abs() > 1e-9 {
        scale.set_value(level);
    }
    GAME_GUARD.with(|guard| guard.set(false));
    let set_by = crate::ui::profile_form::sets_row("Game audio");
    row.set_sensitive(set_by.is_none());
    row.set_tooltip_text(
        set_by
            .map(|name| format!("Set by the profile {name} \u{2014} change it in Profiles"))
            .as_deref(),
    );
}

fn wire_game_audio(window: &adw::ApplicationWindow) {
    let Some(scale) =
        widget_in(window, "narrate-game-audio").and_then(|w| w.downcast::<gtk::Scale>().ok())
    else {
        return;
    };
    scale.connect_value_changed(|scale| {
        if GAME_GUARD.with(|guard| guard.get()) {
            return;
        }
        let value = scale.value();
        crate::ui::produce_page::set_game_volume(value);
        if let Some(session) =
            crate::ui::window::SESSION.with(|slots| slots.borrow().last().cloned())
        {
            session.borrow_mut().produce.game_volume = value;
        }
    });
}

thread_local! {
    static GAME_GUARD: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
