//! §07-narrate#1-screen — the Narrate page's surface: the 22 numbered widgets, laid out as
//! spec/inventory/narrate.md §A.1 says (horizontal paned: left = tick + preview + transport + voice
//! picker + take band + sample + pitch; right = the lines column) and wired thin to the rules.
//!
//! No rule lives here. Every number, label and sentence comes from [`crate::narrate_screen`] or
//! [`crate::narrate_details`]; a handler reads one widget, calls one rules function and prints what came
//! back on the status line (spec/00-principles.md §5).
//!
//! WHY a state holder instead of reading the project: `Project`'s narration and cut sit behind
//! `window.rs`'s private `session_reads()`, so the page renders a state the flow rounds publish into
//! it through [`set_state`] — the same shape the effects lane uses (holders built empty, filled by a
//! refresh seam rather than by poking globals from the callback).

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

/// What the page shows. Published by the flows, read by every handler.
#[derive(Debug, Clone, Default)]
pub struct NarrateState {
    pub entries: Vec<Entry>,
    pub segs: Vec<Seg>,
    /// The hand-picked takes of the narrator slot's recording, in seconds.
    pub takes: Vec<(f64, f64)>,
    /// The chosen voice's id (`narrate_screen::CAPTIONS`, `narrator1`, or a voices-folder name).
    pub voice: String,
    pub pitch: f64,
    /// P.policy.ttsLanguage: the project's language, published by the flow that reads the project.
    /// Empty means the project states none, and `narrate_tts::language` then answers its fallback — it is
    /// never hard-coded here, because a model told to speak English reads Polish spelling as English.
    pub language: String,
    /// S2 is asked of the server on every speak ([`crate::speak_leg::health`]), not carried in state:
    /// speaking is not a repaint, so an answer published at the last paint could be minutes stale by
    /// the click. Nothing here holds health, the model list or the TTS id — `speak_leg` reads all
    /// three off the Settings file per call (§02-services#1).
    /// Session second under the red line.
    pub session: f64,
    /// Where that second falls in the finished video, and the video's length.
    pub cut_at: f64,
    pub length: f64,
    pub clips: usize,
    pub unwritten: usize,
    pub off_cut: usize,
    pub has_cut: bool,
    pub has_timeline: bool,
    /// How many narrator slots Prepare has a recording for, and the voices folder's .wav names.
    pub narrators: usize,
    pub voice_files: Vec<String>,
    /// Whether the clip under the row has a recording covering it at all (§1's speaks-alone cases).
    pub recording_covers_clip: bool,
    pub playing: bool,
    /// §5 (`the run bar is the preview's transport only once the preview started`): has this preview
    /// been cued at all and not ⏹-ed since. `playing` says whether it is going now; `started` is what
    /// makes the bar serve the preview rather than the step, so a page nobody has pressed ▶ on keeps ▶
    /// as "write and speak".
    pub started: bool,
    /// A synthesis is running: it outranks the other refusals on the row's ▶.
    pub busy: bool,
    /// The session spans a recording actually covers, published by the flow that read Prepare. F4.5 S1 asks
    /// these before cueing: a start landing off every span would report itself as playing over a black frame.
    /// Empty means "nothing was published", which `preview_covered` reads as full coverage so a page driven
    /// without that fact still previews rather than refusing everything.
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

/// Publish what the page should show.
pub fn set_state(next: NarrateState) {
    NARRATE_STATE.with(|held| *held.borrow_mut() = next);
}

/// Read the page's current state back out (tests drive a seam and check the state moved with it).
pub fn read_state() -> NarrateState {
    NARRATE_STATE.with(|held| held.borrow().clone())
}

pub(crate) fn mutate<T>(f: impl FnOnce(&mut NarrateState) -> T) -> T {
    NARRATE_STATE.with(|held| f(&mut held.borrow_mut()))
}

/// Re-publish the line rows from a narration that was just re-read, keeping every other field of the
/// page's state. The flows call this after they re-read `narrate/narration.json` (F0.11 S2): the page
/// draws its rows from THIS state, so replacing the window's held narration alone would leave lines
/// whose recording is gone sitting on screen.
pub fn publish_lines(entries: Vec<Entry>) {
    mutate(|s| s.entries = entries);
}

// --- the surface -------------------------------------------------------------------------------------

thread_local! {
    static PAGE_CSS_LOADED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// The page's own ground colour, installed once per process the way `window.rs::install_lane_css_once`
/// installs the lane's palette. Both the preview frame and the take band are `DrawingArea`s whose paint
/// belongs to a draw handler this build does not have yet, so without a background they render as the
/// window's own grey and read as empty space rather than as a picture and a waveform lane.
fn install_page_css_once() {
    if PAGE_CSS_LOADED.with(|cell| cell.get()) {
        return;
    }
    // A line's text box is framed (§1 badge 20, `spec/img/07-narrate.png`): unframed, a white box on the white
    // lines column cannot be seen, and a fresh line looked like a row with nowhere to type.
    let css = ".narrate-plate { background-color: #1c1c1d; border: 1px solid #000000; }\n\
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

/// The preview plate: the left column's 560 px at 16:9, so 560 * 9 / 16 = 315. One pair of numbers for the frame
/// and for where §A.1's second divider opens (`narrate-left-split`), so changing the frame moves the handle.
pub(crate) const PREVIEW_FRAME_W_PX: i32 = 560;
pub(crate) const PREVIEW_FRAME_H_PX: i32 = 315;

/// One row of buttons in a page column, measured on this page's own transport and take rows.
pub(crate) const TRANSPORT_ROW_PX: i32 = 34;

/// The Narration tick's row, which sits inside the upper half and so belongs in the divider's opening.
pub(crate) const TICK_ROW_PX: i32 = 27;

/// The gap both halves of the left column put between their rows.
pub(crate) const COLUMN_GAP_PX: i32 = 8;

/// Build the whole page. Called from `page_box` once per window for the Narrate tab.
pub fn build() -> gtk::Widget {
    install_page_css_once();
    let split = gtk::Paned::new(gtk::Orientation::Horizontal);
    // §A.1: 560 px to the left pair, the window's extra width going to the lines.
    split.set_position(560);
    // NOT `true`: §1 opens the left column at 560 px and sends every extra window pixel to the RIGHT ("extra
    // window width goes to the right"), so the lines column is what grows. With `true` the paned handed the growth
    // to the left pair and the rows were starved of width.
    split.set_resize_start_child(false);
    split.set_shrink_start_child(false);
    split.set_wide_handle(true);

    // §1's "Left column, split top and bottom by a second draggable divider (opened with the upper part 420 px
    // tall)": ABOVE it, top to bottom, the Narration tick, the preview and the transport row; BELOW it, the voice
    // picker and everything after it. The two halves carry `left`'s own width flags — this fills the paned's 560 px
    // rather than hugging its children (`false` here gave the column its natural ~140 px and cut every control off
    // at the fold), and §A.1's 560 px is stated as a REQUEST as well as a paned position because the take band
    // sizes its lane off the width its scroller actually got (`narrate_band::note_lane_size`) and a position is
    // only a suggestion next to a child's own minimum.
    let upper = gtk::Box::new(gtk::Orientation::Vertical, 8);
    upper.set_widget_name("narrate-upper");
    let lower = gtk::Box::new(gtk::Orientation::Vertical, 8);
    lower.set_widget_name("narrate-lower");
    for half in [&upper, &lower] {
        half.set_hexpand(true);
        half.set_size_request(560, -1);
        half.set_halign(gtk::Align::Fill);
    }
    let vsplit = gtk::Paned::new(gtk::Orientation::Vertical);
    vsplit.set_widget_name("narrate-left-split");
    // §A.1's opening, as ONE formula over the rows it has to clear rather than a second magic number: the tick
    // (`TICK_ROW_PX`) + a gap + the frame (`PREVIEW_FRAME_H_PX`, the column at 16:9) + a gap + the transport row
    // (`TRANSPORT_ROW_PX`) = 392, which is where `spec/img/07-narrate.png` draws the handle — just under the
    // transport, with the voice picker the first row below it. A headless window is never dragged, so this is also
    // where the shot's handle sits.
    vsplit.set_position(PREVIEW_FRAME_H_PX + TRANSPORT_ROW_PX + TICK_ROW_PX + 2 * COLUMN_GAP_PX);
    vsplit.set_wide_handle(true);
    vsplit.set_shrink_start_child(true);
    // NOT `true`: with `resize_start_child` GTK hands every extra pixel of window height to the UPPER half, whose
    // content is fixed-size (tick + the 16:9 frame + the transport row) — so the leftover showed up as an empty
    // band between the transport row and the voice picker, where `spec/img/07-narrate.png` has the picker directly
    // under the transport. The lower half takes the slack instead; the opening above still fixes where the handle
    // starts, and a drag moves it either way because both halves can shrink.
    vsplit.set_resize_start_child(false);
    vsplit.set_start_child(Some(&upper));
    vsplit.set_end_child(Some(&lower));

    // **1** the game audio level, at the top right of the column: how loud the recording plays under a spoken
    // line, the narration's own number (it used to sit among Produce's encoder rows). There is no Narration
    // tick: the page is always live, and a video is narrated when its narration has lines (§1; first rewrite:
    // a tick here switched the narration off).
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
    // §1 (**22**): the row's bracketed halves ("⚠ N clip(s) unwritten, N line(s) off the cut", "· no
    // timeline") are arithmetic over the session files — a clip with no entry and not deliberately silent
    // waits to be written, a line on video the cut dropped is off it (§07 §B) — and a flow publishes the rows,
    // the cut and the voice but never that arithmetic. So the page derives it at the same moment it learns
    // what the project holds, through the same door (`narrate_seed::seed_from_disk` says what may overwrite
    // what a flow published), and a caller that published counts of its own keeps them.

    // **2** the preview: one fixed 16:9 frame, the same pinned-height trick the Cut page's
    // `preview-panel` uses so GTK cannot collapse it against an expanding sibling.
    let preview = gtk::Box::new(gtk::Orientation::Vertical, 0);
    preview.set_widget_name("narrate-preview");
    // The plate is the column's width at 16:9 (560 → 315), which is what §A.1's picture draws and what the second
    // divider opens past. NOT `vexpand`: the box's height here is its SIZE, and an expanding frame soaked up the
    // upper half's whole allocation, grew to ~400 px (measured) and pushed the transport row under the fold.
    preview.set_size_request(PREVIEW_FRAME_W_PX, PREVIEW_FRAME_H_PX);
    // NOT `Start`: the frame is a fixed-size box inside a column that is itself only as wide as its widest
    // child, so `Start` left it at its minimum width and the whole left column collapsed to ~140 px while
    // the picture became an invisible sliver. `Fill` makes the column take the paned's 560 px and the
    // frame fill them at its own 16:9 shape.
    preview.set_halign(gtk::Align::Fill);
    preview.set_tooltip_text(Some(
        "The finished frame with the narration mixed over the cut's sound",
    ));
    let picture = gtk::DrawingArea::new();
    picture.set_widget_name("narrate-picture");
    // NOT `vexpand`, and it needs a size of its own as well: an expanding child soaked up the paned's whole height
    // and the "16:9 frame" rendered ~1170 px tall; with no natural height and no request, GTK gave the area 2 px
    // inside its box. The explicit request makes the plate the frame it claims to be.
    picture.set_vexpand(false);
    picture.set_size_request(PREVIEW_FRAME_W_PX, PREVIEW_FRAME_H_PX);
    // The plate the frame is cut out of: a bare `DrawingArea` paints nothing until a
    // `connect_draw` handler exists (there is none anywhere in `src/`), so headless and live alike the
    // frame was invisible. CSS gives it the dark 16:9 field with a stroke, which is what §2's image shows.
    picture.add_css_class("narrate-plate");
    // F4.5 S1 (`click the picture or ▶`): the picture is its own toggle. A bare `DrawingArea` takes a
    // gesture controller fine (it has no button handler of its own), and this page's picture is NOT the Cut
    // page's `preview-panel`, whose single-button-1 rule exists because that widget already carries a drag.
    // One click gesture per widget still holds here: there is no second gesture on `narrate-picture` to race.
    let picture_click = gtk::GestureClick::new();
    picture_click.connect_released(move |_click, _n, _x, _y| {
        press_preview_picture(&crate::ui::window::main_window());
    });
    picture.add_controller(picture_click);
    // The frame itself (§1 badge 2, "finished frame"): a `Picture` under the painter, showing the extracted frame
    // at the line or, while the preview plays, the built-in player's own frames (`narrate_picture`). The painter
    // above it draws only what the cut lays over the footage (masks, stills, titles).
    let video = gtk::Picture::new();
    video.set_widget_name("narrate-video");
    video.set_content_fit(gtk::ContentFit::Contain);
    video.set_can_shrink(true);
    video.set_size_request(PREVIEW_FRAME_W_PX, PREVIEW_FRAME_H_PX);
    video.add_css_class("narrate-plate");
    let stack = gtk::Overlay::new();
    stack.set_child(Some(&video));
    stack.add_overlay(&picture);
    picture.remove_css_class("narrate-plate");
    preview.append(&stack);
    upper.append(&preview);

    // **3** back 3 s · ▶ · forward 3 s, then **4** ＋ a line at this second.
    let transport = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    transport.set_widget_name("narrate-transport");
    transport.set_halign(gtk::Align::Fill);
    // NOT `Start`: the row has to fill the column so the slider after the buttons gets the slack. §1's Preview
    // bullet puts the slider, the clock and the volume on THIS row (`spec/img/07-narrate.png` draws badges 3-6 on
    // one line under the frame), and pinned to the start the row squeezed into a few pixels.
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

    // **5** the slider over the cut's own clock + the two-face clock, and **6** the shared volume — all on the
    // transport row, in §1's left-to-right order, which is what `spec/img/07-narrate.png` draws.
    let slider = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 1.0, 0.1);
    slider.set_draw_value(false);
    slider.set_widget_name("narrate-slider");
    slider.set_hexpand(true);
    // §A.4: the bar is "the cut, end to end", so its own scale is the cut's length and a drag reads as a
    // second of the finished video rather than a fraction nobody named. `refresh` keeps it in step from here on.
    slider.set_tooltip_text(Some(
        "The cut's own clock \u{2014} what the edit removed is not on this bar",
    ));
    transport.append(&slider);
    let clock = gtk::Label::new(Some(&narrate_screen::clock_line(0.0, 0.0, 0.0)));
    clock.set_widget_name("narrate-clock");
    clock.set_xalign(0.0);
    transport.append(&clock);

    // (**6**) the shared preview volume: its own scale, wearing the number this window's slot holds
    // (`ensure_slot` has already opened that slot by the time the page is built).
    // (§6) the shared preview volume: its own scale, wearing the number this window's slot holds.
    // `narrate_volume::ensure_slot` pushed that slot in `build_window`, before any page was built.
    let volume = narrate_volume::build();
    transport.append(&volume);
    upper.append(&transport);

    // **7** who speaks, **8** the take band's buttons and **9** Add file…, on ONE row: §1's "First row, left to
    // right: unlabelled dropdown …; the take band's buttons; "Add file…"". The picker itself (its rows, its shown
    // row and what a pick does) lives in `narrate_picker`; this box only says where it sits.
    let voice_row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    voice_row.set_widget_name("voice-row");
    let picker = narrate_picker::build();
    // The dropdown takes the row's spare width, as §1's picture draws it; the four buttons after it keep their
    // own natural width.
    picker.set_hexpand(true);
    voice_row.append(&picker);
    let take_add = gtk::Button::from_icon_name(narrate_screen::ADD_LINE_ICON);
    take_add.set_widget_name("take-add");
    take_add.set_tooltip_text(Some(narrate_details::take_add_hint()));
    // §1's Prototype note: the − is an icon, not U+FF0D — that sign is CJK-only and draws as a missing-glyph
    // box without one, like the ＋ beside it. The tooltip keeps the prototype's wording (§6 spells its
    // sentences with those glyphs), so the icon name lives here rather than in that string.
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

    // **10** the take band itself: the lane inside its own horizontal scroller, painted by
    // `narrate_band` (envelope, ruler, takes, red ▶-start bar, live selection, playhead).
    let (band_scroller, band) = crate::ui::narrate_band::build_band();
    band.set_tooltip_text(Some(narrate_screen::band_status(&[])));
    lower.append(&band_scroller);

    // **11** the sample sentence, **12** its ▶ ⏹ ⟳ and **13** the pitch slider, on ONE row: §1's "Third row,
    // left to right: Sample: an entry …, ▶ speaks it, ⏹, ⟳ new take; pitch −6..+6 semitones".
    let sample_row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    sample_row.set_widget_name("sample-row");
    // No `halign = Start` on this row: it has to fill the column so every part keeps its place. Pinned to the
    // start with an entry that only asked for its natural width, the row squeezed into a few pixels and the
    // pitch part fell to the line below.
    let sample = gtk::Entry::new();
    sample.set_widget_name("sample-sentence");
    sample.set_text(narrate_screen::DEFAULT_SAMPLE);
    sample.set_tooltip_text(Some("Spoken in the selected voice"));
    // The entry takes the width of its row, so the buttons and the pitch control after it keep their own.
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
    // **13** pitch: the labelled row (`pitch:` · slider · the shift's number), built by
    // [`crate::ui::narrate_pitch`] on the sample line, as spec/img/07-narrate.png draws badges 11–13 together.
    // Seeded from the state above, so control and state agree on `narrate/pitch.txt`'s shift from the first paint.
    sample_row.append(&crate::ui::narrate_pitch::build_row(read_state().pitch));
    lower.append(&sample_row);

    // **14–21** the lines column: one row per entry, sorted by clip then offset.
    let scroller = gtk::ScrolledWindow::new();
    // BOTH axes `Automatic`, not `External`: `External` tells GTK that some other code owns the vertical
    // adjustment and feeds the scrolled window through it. Nothing here does — no scrollbar widget, no
    // `vadjustment` wiring — so the list was never laid out as a scrollable area and only its first row
    // appeared however many entries were published. `Automatic` lets the window build and manage its own.
    // HORIZONTAL `Never`, not `Automatic`: a wrapped monospace `TextView` reports a huge NATURAL width (the
    // longest line unwrapped), and `Automatic` lets a scrolled window grow to its child's natural width — so the
    // list took that width, the row's four header buttons landed past the window's right edge as a clipped sliver,
    // and the words ran off the panel ("…a 51 percent"). `Never` says the content is exactly the viewport's width,
    // which is what makes §1's "UNDER the header line, THE ROW'S FULL WIDTH" mean the column's width, and keeps
    // the buttons at "the header line's right end" inside it. The vertical axis stays `Automatic`: the rows scroll.
    scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroller.set_hexpand(true);
    scroller.set_vexpand(true);
    // Bounded content height: an unbounded ScrolledWindow reports its child's FULL natural height as its
    // own minimum, so four rows made the page demand ~2682 px and GTK sized the window to that (a 1280x800
    // request rendered 2669x3025). These three calls are the GTK4 way of saying "this area scrolls; it does
    // not report its content height upward".
    scroller.set_min_content_height(240);
    scroller.set_max_content_height(560);
    scroller.set_propagate_natural_height(false);
    // §1's growth sentence: "text boxes, lists and the log take the extra height and get their own
    // scrollbar when their content is longer". A `max_content_height` alone stops at 560 px however tall
    // the window gets, so the row list stayed a small plate under a wall of empty page; vexpand lets it fill
    // whatever room the pane gives while the bounds above still decide when its scrollbar appears.
    scroller.set_vexpand(true);
    scroller.set_vexpand_set(true);
    // No height floor on the column any more, and the reason is the shell's divider: a `gtk::Paned` splits its
    // leftover between children that expand by SHARE, so every page's own floor was charged to the page half of
    // `main-divider` — measured, Prepare sat at 342 px while the bottom box took 501 px of an 844 px divider and
    // painted its plate over the bottom of the page. §1 wants the band to be exactly its two rows and the page to
    // "take all the rest of the height", which needs the pages to ask for nothing they can scroll instead.
    scroller.set_size_request(-1, 560);
    let lines = gtk::ListBox::new();
    lines.set_widget_name("narrate-lines");
    scroller.set_child(Some(&lines));

    // §1 badge **22**'s Inputs/Outputs readout is NOT drawn here: §A.8 puts it on the window's BOTTOM BAR
    // ("Bottom bar"), which `window_bench::paint_readouts` paints from `Shell::inputs` / `Shell::outputs`
    // for every tab, so a copy in this column was a duplicate that could drift from the real one.
    let right = gtk::Box::new(gtk::Orientation::Vertical, 6);
    right.set_hexpand(true);
    right.append(&scroller);

    split.set_start_child(Some(&vsplit));
    split.set_end_child(Some(&right));
    upcast_widget(split)
}

fn upcast_widget(w: gtk::Paned) -> gtk::Widget {
    w.upcast()
}

thread_local! {
    /// True while the PAGE, not the user, is moving the scrub thumb. GTK fires `value-changed` for a
    /// programmatic `set_value` exactly as it does for a drag, and the bar's handler seeks — so without this the
    /// sync every repaint would turn into a phantom seek of the second it just painted.
    static SYNCING_SCRUB: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Paint the scrub bar to the clock without asking it to seek: the flag is restored on every path, including
/// one the sync itself panics on, so a failed repaint cannot leave the bar deaf to real drags.
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

/// Whether this `value-changed` came from the page painting the bar rather than from a hand on the thumb.
fn scrub_is_being_painted() -> bool {
    SYNCING_SCRUB.with(std::cell::Cell::get)
}

/// §1's transport "▶/⏸": the play button wears ⏸ while the preview plays, sized for both faces so the swap
/// never moves it. Painted by `refresh` and by every transport change (`publish_transport`).
pub(crate) fn paint_play_face(window: &adw::ApplicationWindow, playing: bool) {
    if let Some(play) = crate::ui::line_step_button(window, "narrate-play") {
        crate::ui::cut_record_play::set_steady_label(
            &play,
            if playing { "\u{23f8}" } else { "\u{25b6}" },
            &["\u{25b6}", "\u{23f8}"],
        );
    }
}

/// Everything that shows where the line is: the clock, the slider, the painted frame and the take band. The
/// preview's tick and a row's cue both move the line without anything else on the page changing, so they repaint
/// these and leave the rows alone (a rebuild under a click would take the focus out of the box clicked into).
fn paint_head(window: &adw::ApplicationWindow) {
    let s = read_state();
    if let Some(clock) = label(window, "narrate-clock") {
        clock.set_text(&clock_text(&s));
    }
    sync_scrub(window, &s);
    // The painted frame walks with the line: the preview's crop, zoom and still are read at `session`, so the
    // 100 ms tick is what makes the picture move.
    crate::ui::narrate_picture::repaint(window);
    crate::ui::narrate_band::repaint(window);
}

/// §1 badge 5's clock, "session · cut/length". The middle field is where `session` falls in the finished video,
/// read off the page's clips at every paint. Kept as a second number beside `session`, it was walked only by the
/// doors that remembered it (the tick, the 3 s buttons) and not by a row's cue, the slider or the wheel, so the
/// clock read `00:12 · 00:00/00:20` with the line at 12 s on a cut kept whole. In a removed stretch the finished
/// video's second is the kept length before the line; with no clips published the stored number stands.
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

/// Repaint the labels, the picker's rows and the line rows from the published state.
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
    // The band's own painted picture follows the same state: takes, voice, and where ▶ got to.
    crate::ui::narrate_band::repaint(window);
    // And so does its very existence: §A.7 shows the band only for a narrator slot with a recording.
    crate::ui::narrate_band::show_only_with_recording(window);
    // So does the preview frame: the second it shows is `session`, which any seek just changed.
    crate::ui::narrate_picture::repaint(window);
    // The scrub bar carries the same second (§A.4: the bar IS the cut's clock), so a seek by button, wheel or
    // program moves the thumb instead of leaving it where the last drag put it.
    sync_scrub(window, &s);
    // The volume slider shows the same one number every other preview hears (§A.4), so a run or the Cut
    // page moving it has to show up here too — mirrored only when it differs, since writing a scale back
    // would read as a phantom drag.
    narrate_volume::mirror(window);
    // Why here: a flow that changes the voice without this leaves the dropdown on another row than the
    // page speaks.
    narrate_picker::follow(window, s.narrators, &s.voice_files, &s.voice);
    draw_rows(window, &s);
    // Always re-wired: the rows are rebuilt whole on every refresh, so their handlers go with the old
    // ones. A guard on the count would leave a same-length refresh with dead buttons.
    wire_row_buttons(window);
}

thread_local! {
    /// Set while [`draw_rows`] rebuilds the list. GTK selects a row on its own when the selected one is removed
    /// under the keyboard focus, and F4.5 S5's jump belongs to a person's selection only: the first rewrite let
    /// that self-selection through, so every refresh after a click on a line (a frame step, a seek) threw the
    /// preview back to the FIRST line's lead-in.
    static REBUILDING_ROWS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn draw_rows(window: &adw::ApplicationWindow, s: &NarrateState) {
    let list = widget_in(window, "narrate-lines").and_then(|w| w.downcast::<gtk::ListBox>().ok());
    // The person's selection and whether the keyboard was in the list, so the rebuilt rows carry both on.
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
    // GTK settles the focus of a removed row later in the same turn of the loop; the guard stays up until then.
    glib::idle_add_local_once(|| REBUILDING_ROWS.with(|cell| cell.set(false)));
}

fn draw_rows_now(window: &adw::ApplicationWindow, s: &NarrateState) {
    let Some(list) =
        widget_in(window, "narrate-lines").and_then(|w| w.downcast::<gtk::ListBox>().ok())
    else {
        return;
    };
    // Rebuilt whole: the sort order changes when a line moves, and keeping rows in place would make the
    // index each button carries disagree with the entry it stands for.
    while let Some(row) = list.row_at_index(0) {
        list.remove(&row);
    }
    if s.entries.is_empty() {
        // The empty page still has to say something where its rows go; an empty list box reads as a broken
        // panel rather than as "nothing written yet". No buttons are wired into this row -- there is no
        // entry for them to point at -- so `wire_row_buttons`' count loop never reaches it.
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
        // §1's row shape lives in `narrate_row` (header line with the four buttons at its right end, the
        // text box full width under it, the warning last); this loop only supplies the numbers the page owns:
        // the session second the time field shows and the fit the render's ladder answered for this line.
        let row = crate::ui::narrate_row::build(
            index,
            entry,
            entry.s + entry.at,
            fit_for(entry, &ordered, &s.voice),
        );
        list.append(&row);
    }
}

/// The row's fit, asked of the render's own ladder (F4.3) rather than of a per-line estimate: every line of
/// THIS clip goes through `narrate_screen::mirror_fit` together, so what the row warns about is what the render
/// will do — including speeding the narration up, which a single line measured against its clip can never show.
///
/// Speech length comes from §F4.3's page-only rule: characters over the take's MEASURED rate once its wav is on
/// disk ([`speech_for`]), over [`narrate_screen::SPEECH_CHARS_PER_SECOND`] until there is anything to measure.
///
/// Two kinds of row never wear a fit warning because neither is spoken: an empty line and a caption (`pos` set —
/// "the viewer reads it; never spoken"). Those answer `Fit::Fits` here rather than being filtered inside the
/// mirror, since the skip is about what the row means, not about the ladder.
fn fit_for(entry: &Entry, all: &[(usize, &Entry)], voice: &str) -> Fit {
    if entry.text.is_empty() || !entry.pos.is_empty() {
        return Fit::Fits;
    }
    let tree = crate::ui::window::narrate_session_tree();
    // Every line standing on this clip's bounds, in play order, as the ladder wants it: (index, at, speech).
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

/// How long one line runs on the page: §F4.3's measured rate once its take's wav is on disk, §C.2's
/// 15 chars/s until there is anything to measure, and the 8..28 clamp over either.
fn speech_for(tree: Option<&crate::layout::Tree>, entry: &Entry, voice: &str) -> f64 {
    let chars = entry.text.chars().count();
    let measured = tree.and_then(|tree| {
        crate::narrate_tts::take_seconds(tree, &crate::narration::key_for(entry, Some(voice)))
    });
    // A take's own seconds stand as they measured; only a wild measurement (past 8 or 28 chars/s) is moved,
    // by the clamped rate back into seconds. No wav means no measurement, so §C.2's estimate answers.
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

/// F4.5: the page as the preview leg sees it — the same seam shape `window.rs` offers
/// [`crate::cut_play_leg::CutPreviewPage`]. Every method is a one-line read of published state; no
/// rule lives here, so the leg's tick and the page cannot drift about what the cut, the record or the
/// loudness numbers are.
pub struct PageDriver(adw::ApplicationWindow);

impl PageDriver {
    /// The same driver, for the transport doors in `narrate_page_transport`, which need the page's two
    /// loudness numbers but must not reach into this file's private field.
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
            // The clock's cut half follows the line (§1 badge 5, "session · cut/length").
            if let Some(cut_at) = crate::cut_play::cut_time(&s.segs, at) {
                s.cut_at = cut_at;
            }
        });
        // The clock and the slider walk with the line while the preview plays, not only on a press.
        paint_head(&self.0);
    }

    fn tree(&self) -> Option<crate::layout::Tree> {
        crate::ui::window::narrate_session_tree()
    }

    fn cut(&self) -> crate::cut::Cut {
        // The session's own cut file wins: the page's `segs` mirror only what a flow last published,
        // and a lap of the cut must hear the effects that are actually saved.
        self.tree()
            .map(|tree| crate::cut::load(&tree).unwrap_or_default())
            .unwrap_or_else(|| narrate_preview_leg::cut_of(read_state().segs, Vec::new()))
    }

    fn entries(&self) -> Vec<Entry> {
        read_state().entries
    }

    fn game_volume(&self) -> f64 {
        // P.policy.gameVolume off the project; with nothing to read, the project's OWN default rather
        // than 0.0, which would duck the bed into silence on a session that never stated a level.
        self.tree()
            .map(|tree| narrate_preview_leg::game_volume_for(&tree))
            .unwrap_or_else(|| crate::project::Produce::default().game_volume)
    }

    fn volume(&self) -> f64 {
        // The shared preview volume slider (§A.4, F2.5 S6), read from its ONE holder rather than off
        // this page's widget: two copies would be two answers, and an audition that sounded different
        // from the preview it belongs to is exactly what §C.4 forbids.
        crate::ui::preview_volume(&self.0)
    }

    fn say(&self, said: &str) {
        say(&self.0, said);
    }

    /// §5: the tick ended the preview itself (the picture walked past the last clip), with no ⏹ press to
    /// clear the page's flags. Left alone, `started` would keep the bar answering for a dead preview.
    fn stopped(&self) {
        mutate(|state| {
            state.playing = false;
            state.started = false;
        });
        super::narrate_page_transport::publish_transport();
    }
}

/// Add one name to the page's voice list, sorted, without repeating it. Public because "Add file…"
/// lands a new voice this way and nothing else is allowed to reach into the page's state.
pub fn offer_voice(name: &str) {
    mutate(|state| {
        if !state.voice_files.iter().any(|held| held == name) {
            state.voice_files.push(name.to_string());
            state.voice_files.sort();
        }
    });
}

/// Say it on the status line and in the log, the page's two output channels.
pub(crate) fn say(window: &adw::ApplicationWindow, said: &str) -> String {
    if let Some(status) = crate::ui::window::find_status(window.upcast_ref()) {
        status.set_text(said);
    }
    crate::ui::window::log_line(said);
    said.to_string()
}

// --- the seams: one widget press, one rules call, one printed sentence -------------------------------
// The transport's own seams (back / forward / the seek they end in) live in `narrate_page_seek`, because this
// file is at its size budget and a seek is a rule of its own, not part of the drawing.
pub use crate::ui::narrate_page_seek::{
    note_scrub_drag, pending_scrub_for_test, press_back, press_forward, seek_preview,
};

/// The preview transport lives in `narrate_page_transport` (this file is at its size budget and ▶/⏸/⏹ are
/// rules of their own, not part of the drawing); these are the names the shell and the tests already call.
pub use crate::ui::narrate_page_transport::{
    narrate_preview_playing, press_narrate_play, press_preview_picture, press_preview_stop,
};

// The take band and the sample plate (＋ / － / ▶ on a take, ▶ / ⏹ / ⟳ on the sample sentence) live in
// `narrate_page_take` (this file is past its size budget); these are the names the shell and the tests
// already call.
pub use crate::ui::narrate_page_take::{
    press_sample_play, press_sample_reroll, press_sample_stop, press_take_add, press_take_play,
    press_take_remove,
};

/// **4** ＋ a line at this second. Ok pushes the entry where the rule put it.
pub fn press_narrate_add_line(window: &adw::ApplicationWindow) -> String {
    // §F4.7: "preview paused first, picture and voice" — ⏹'s own door stops both players, so a new
    // line is never laid down while the old audio is still running under it.
    if read_state().playing {
        press_preview_stop(window);
    }
    let s = read_state();
    match narrate_screen::add_at_playhead(s.session, &s.segs, &s.entries) {
        Ok(at) => {
            // `add_at_playhead` answers with the clip's start, which is what the marker question and
            // the status sentence are both about.
            // §F4.7's free add lands where the playhead stands, so the entry's clip-relative `at` is the
            // gap between the two: `s`/`e` stay the clip's bounds (§C.1), `at` carries the placement.
            let said = if narrate_edit::silent_marker_on(at, &s.entries).is_some() {
                // §F4.7: when the clip's only entry is the empty "deliberately silent" marker, THAT
                // entry moves to the playhead and becomes the line — no second row. The spec gives no
                // sentence of its own here, so the page says what happened plainly rather than reusing
                // "a line starts at", which would hide that nothing was added.
                mutate(|state| {
                    if let Some(index) = narrate_edit::silent_marker_on(at, &state.entries) {
                        state.entries[index].at = (s.session - at).max(0.0);
                    }
                });
                format!(
                    "the silent marker became this line at {}",
                    narrate_screen::time_field(s.session)
                )
            } else {
                mutate(|state| {
                    let clip = state
                        .segs
                        .iter()
                        .find(|seg| seg.s <= at && at < seg.e)
                        .cloned()
                        .unwrap_or_default();
                    state.entries.push(Entry {
                        s: clip.s,
                        e: clip.e,
                        at: (at - clip.s).max(0.0),
                        ..Default::default()
                    });
                });
                format!("a line starts at {}", narrate_screen::time_field(at))
            };
            refresh(window);
            // §F4.7: the record autosaves, and a new row lives in that same file — it must not wait for
            // somebody to type in a box before it reaches disk.
            crate::ui::narrate_text::note_structure_change(window);
            say(window, &said)
        }
        Err(refused) => {
            // §F4.7's node J: a press within 1 s of a line does not argue with the person, it takes the
            // playhead to that line and repeats the sentence naming it. Every other refusal leaves the
            // picture where it was.
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

/// Does a recording cover second `at` of a line's clip (§1's "a clip no recording covers speaks alone")? Read off the
/// filmed spans the page loaded; none published reads as covered, as the preview's own start check does.
pub(crate) fn line_covered(s: &NarrateState, at: f64) -> bool {
    s.recording_covers_clip
        && (s.covered_spans.is_empty()
            || s.covered_spans
                .iter()
                .any(|(from, to)| *from <= at && at < *to))
}

/// **16** ▶ speak this line: whichever of the six answers the row's own state gives.
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
        entry.at,
    );
    let (case, said) = if line_covered(&s, entry.s + entry.at) {
        (case, said)
    } else {
        narrate_screen::audition_without_recording(case, index + 1)
    };
    let line = match said {
        Some(said) => said,
        None => match case {
            // The line IS to be spoken: hand it to F4.4 rather than only saying so. The audition above owns
            // what the refusals sound like; this leg owns whether a wav actually lands.
            Audition::Speak { from } => return speak_this_line(window, &s, entry, from),
            _ => narrate_screen::spoken_alone(index + 1),
        },
    };
    say(window, &line)
}

/// F4.4 for one row, over the real wire: the page's own thin wire into
/// [`crate::ui::narrate_page_speak::speak_and_audition`], which files the take and plays it. The two
/// volumes are read here because they belong to this page's sliders — see the door's own doc comment.
fn speak_this_line(
    window: &adw::ApplicationWindow,
    s: &NarrateState,
    entry: &Entry,
    // The second the audition starts from — `narrate_screen::audition`'s lead-in, which decides whether
    // the three seconds ahead are this line's own or still belong to the line above it.
    from: f64,
) -> String {
    let driver = PageDriver(window.clone());
    let (game_volume, preview_volume) = (driver.game_volume(), driver.volume());
    narrate_page_speak::speak_and_audition(window, s, entry, from, game_volume, preview_volume)
}

/// **17** ↻ re-roll: refused when there is nothing to draw again of.
pub fn press_line_reroll(window: &adw::ApplicationWindow, index: usize) -> String {
    let s = read_state();
    // §6 (`Re-roll / sample refusals`): with the captions voice nothing has a take, so ↻ has nothing to
    // draw again. This comes FIRST, before any roll bump or dial: re-rolling a caption would otherwise put
    // a request on the speech server for a voice that does not exist.
    if crate::narrate_run::voice_is_captions_only(&s.voice) {
        return say(window, narrate_details::re_roll_captions_refusal());
    }
    let has_line = s
        .entries
        .get(index)
        .map(|entry| !entry.text.is_empty())
        .unwrap_or(false);
    let Some(refused) = narrate_screen::re_roll_refusal(index + 1, has_line) else {
        // §F4.7: roll + 1 first, so the take that is spoken next is keyed on the new roll and the old
        // wav stays untouched (it is what an undo reads back). Then the same real speak leg a row's ▶
        // uses — nothing scripted here.
        mutate(|state| {
            if let Some(entry) = state.entries.get_mut(index) {
                entry.roll = narrate_edit::reroll(entry.roll);
            }
        });
        let after = read_state();
        let Some(entry) = after.entries.get(index) else {
            return say(window, narrate_details::nothing_picked());
        };
        refresh(window);
        // §6: the accepted answer names the row and says why it is speaking — the person is about to hear a
        // different take from the one they had.
        say(window, &narrate_details::new_take_status(index));
        // A re-roll speaks the NEW take in place (§C.3), so the audition starts at the line's own offset
        // rather than 3 s ahead: those seconds belong to the take the person just heard.
        return speak_this_line(window, &after, entry, entry.at);
    };
    say(window, &refused)
}

/// **13** pitch: clamped to the slider's ends and reported with the voice it will clone from.
pub fn press_pitch(window: &adw::ApplicationWindow, delta: f64) -> String {
    let pitch = mutate(|s| {
        s.pitch = narrate_screen::clamp_pitch(s.pitch + delta);
        s.pitch
    });
    // S4: a pitch change invalidates only the shifted copy; the base stays and re-shifts. The value goes to
    // `narrate/pitch.txt` (§07-narrate#3-data); a write failure goes to the log, since this press speaks of
    // the voice.
    if let Some(why) = crate::ui::narrate_seed::note_pitch_for_session(pitch) {
        crate::ui::window::log_line(&format!("!!! could not save the pitch -- {why}"));
    }
    let voice = read_state().voice;
    // Badge 13: this is the other path that moves the shift, so the number beside the slider follows it too.
    crate::ui::narrate_pitch::show_value(window, pitch);
    say(window, &narrate_details::voice_line(&voice, pitch))
}

/// F4.6 S1: the voice picker's own seam — one choice, resolved against what Prepare tagged and what
/// the voices folder holds, then persisted and reported. A refusal is printed verbatim and changes
/// nothing at all: a half-switched voice would leave the page showing one voice while the next line is
/// cloned from another.
pub fn press_voice(window: &adw::ApplicationWindow, id: &str) -> String {
    let s = read_state();
    // The folder named in the refusal is the one the app actually reads (§7: `AUDIOCPP_VOICES`, else
    // the legacy models root, else Flatpak/dev default), not a string this page invented.
    let dir = voices_dir_label();
    match voice_ref::pick_voice(id, s.narrators, &s.voice_files, &dir) {
        voice_ref::VoiceChoice::Refused(why) => say(window, &why),
        voice_ref::VoiceChoice::Silent(said) => {
            // Captions speak nothing, so there is no reference to cut or keep: drop it the way a voice
            // change drops it, and remember the choice.
            let _ = crate::ui::narrate_seed::note_voice_for_session(id);
            mutate(|state| state.voice = id.to_string());
            // A new voice means a different recording, so the band's hand (its selection, zoom and red
            // bar) is about another take and must not carry over.
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

/// The voices folder as it should be spelled in S1's "no longer in DIR" sentence. Read per press so a
/// Settings change shows up in the next refusal rather than naming where the last one looked.
pub(crate) fn voices_dir_label() -> String {
    crate::ui::narrate_voices::voices_folder()
        .display()
        .to_string()
}

/// Attach every handler. Called from `build_window` after `set_content`, like every other control on
/// every page: a click handler attached before the widget is inside the realized tree never fires.
pub fn wire(window: &adw::ApplicationWindow) {
    let win = window.clone();
    // The page is the opened project's: loaded here, once the window knows its own folder (during the build
    // it does not yet, and the lookup answered the previous window's or the launch folder's project), and
    // before the tick is wired so setting its state is not read as a person's press.
    if let Some(tree) = crate::ui::window::narrate_session_tree() {
        set_state(crate::ui::narrate_seed::load_page(
            &tree,
            crate::ui::window::line_position(window).t,
        ));
    }
    // The pitch slider stands at the loaded shift, set before its handler is connected so this is no drag.
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
    // (**6**) the shared preview volume: a drag goes to the one property and nowhere else (F2.5 S6).
    narrate_volume::wire(&win);
    if let Some(button) = crate::ui::line_step_button(&win, "take-add") {
        let w = win.clone();
        // ＋ acts on whatever the hand just dragged on the band; with nothing selected the rule says so.
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
        // §1: the shift applies 400 ms after the last move, so a drag across the slider costs one
        // re-clone rather than one per pixel.
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
                        // §07-narrate#3-data: once per drag rather than per pixel, written by the same
                        // call the keyboard step makes.
                        if let Some(why) = crate::ui::narrate_seed::note_pitch_for_session(pitch) {
                            crate::ui::window::log_line(&format!(
                                "!!! could not save the pitch -- {why}"
                            ));
                        }
                        let voice = read_state().voice;
                        // Badge 13: the number beside the slider follows the drag, once per drag like the write.
                        crate::ui::narrate_pitch::show_value(&w, value);
                        say(&w, &narrate_details::voice_line(&voice, value));
                        glib::ControlFlow::Break
                    }
                },
            );
        });
    }
    // The take band's painter, gesture and wheel live in `narrate_band`; installed here, after the
    // window exists, so the one drag and the one scroll controller land on the real widget.
    crate::ui::narrate_band::attach(&win);
    // The preview picture paints through the Cut page's own painter (`narrate_picture`), so the frame
    // here is the finished frame: crop, zoom, stops and titles at the second the page stands on.
    crate::ui::narrate_picture::install_draw(&win);
    crate::ui::narrate_frame_wheel::install(&win);
    refresh(&win);
    // Rows are drawn by `refresh`, so their handlers go on after it: before this they do not exist yet.
    wire_row_buttons(&win);

    // F4.5 S4: the scrub slider seeks, and a seek lands on the cut — the snapped second is what the
    // player was asked for, not merely what the bar shows. Same seam the transport buttons use, so a drag
    // cues the picture whether or not ▶ has ever run.
    if let Some(slider) =
        widget_in(&win, "narrate-slider").and_then(|w| w.downcast::<gtk::Scale>().ok())
    {
        let w = win.clone();
        slider.connect_value_changed(move |scale| {
            // A repaint moved the thumb; nobody dragged it. Seeking here would move the clock to its own
            // snapped second before the user asked for anything.
            if scrub_is_being_painted() {
                return;
            }
            let target = scale.value();
            let forward = target >= read_state().session;
            // The bar's own drag is the one seek that knows which clips the page is showing; the session's
            // cut.json is the fallback for the buttons, which carry no clip list of their own.
            let segs = read_state().segs;
            note_scrub_drag(&w, target, Some(&segs), forward);
        });
    }

    // F4.5 S5 (§A.5): selecting a row is the only place the preview jumps by itself, and it jumps to
    // the line's lead-in — three seconds ahead unless those seconds belong to the line before.
    if let Some(list) =
        widget_in(&win, "narrate-lines").and_then(|w| w.downcast::<gtk::ListBox>().ok())
    {
        let w = win.clone();
        list.connect_row_selected(move |list, row| {
            let Some(row) = row else { return };
            // A selection the rebuild made by itself is no person's choice: the preview stays where it is.
            if REBUILDING_ROWS.with(|cell| cell.get()) {
                return;
            }
            // The empty-state placeholder carries no entry, so it must not move the picture.
            if row.widget_name() == "narrate-lines-empty" {
                return;
            }
            // `row_at_index` is the only index lookup gtk4-rs 0.11 gives a ListBox, so the position is
            // found by walking the rows until this one turns up rather than by asking the row.
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
                // §1: the clock, the slider and the picture follow the line whether it moves by play, by a
                // seek or by a row; this door moved the number alone, so a paused page kept its old clock.
                paint_head(&w);
            }
        });
    }

    // F4.5 S2: the 100 ms tick that follows the picture, once per window.
    narrate_preview_leg::start_tick(std::rc::Rc::new(PageDriver(win.clone())));
    // §5: this page's transport is what the run bar reads once the preview starts, and `paint_run_bar`
    // can only read a slot this window registered — the Cut page pushes its own with its ▶ (F2.1).
    crate::ui::window::register_preview_player_slot();
    // And it starts out telling the truth: the slots default to `started: true`, which on the Narrate
    // page would hand the bar to a preview this window has never cued (§5's only-once-started clause).
    crate::ui::narrate_page_transport::publish_transport();
    // The loaded project on the page: clock, picker, band, rows and greys.
    refresh(&win);
}

/// The per-row buttons, wired for however many rows the page currently holds. Called after every refresh
/// that can change the row count, each button pointing at its own index's seam.
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
        // Enter commits the typed second; a refusal writes the box back (§F4.7's time field).
        if let Some(field) =
            crate::ui::narrate_page::widget_in(window, &format!("line-time-{index}"))
                .and_then(|w| w.downcast::<gtk::Entry>().ok())
        {
            let w = window.clone();
            field.connect_activate(move |field| {
                crate::ui::narrate_lines::press_line_move(&w, index, &field.text());
            });
        }
        // §F4.7 (**Text**): every keystroke goes through `note_typing`, which parses the tag, updates the
        // entry at once, and re-arms the 400 ms autosave. `changed` (not Enter) is the signal, because a
        // person who types and walks away still gets their words on disk.
        if let Some(field) =
            crate::ui::narrate_page::widget_in(window, &format!("line-text-{index}"))
                .and_then(|w| w.downcast::<gtk::TextView>().ok())
        {
            let w = window.clone();
            // A `TextView` emits `changed` on its BUFFER, not on the view.
            field.buffer().connect_changed(move |_| {
                let text = crate::ui::narrate_row::box_text(&field);
                crate::ui::narrate_text::note_typing(&w, index, &text);
            });
        }
    }
}

/// §1's game audio slider: the project's level on the slider, greyed while narration is off (there is no line to
/// sit under) or while the project's profile sets it (changed in Profiles).
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

/// The slider's wire: a hand's move is kept with the encoder settings; the page's own repaint is not a move.
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
    /// Up while the page sets the game audio slider itself.
    static GAME_GUARD: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
