//! §07-narrate#1-screen — the Narrate page's surface: the 22 numbered widgets, laid out as
//! spec/inventory/narrate.md §A.1 says (horizontal paned: left = tick + preview + transport + voice
//! picker + take band + sample + pitch; right = the lines column) and wired thin to the rules.
//!
//! No rule lives here. Every number, label and sentence comes from [`crate::narrate_screen`],
//! [`crate::narrate_details`] or [`crate::narrate_off`]; a handler reads one widget, calls one rules
//! function and prints what came back on the status line (spec/00-principles.md §5).
//!
//! WHY a state holder instead of reading the project: `Project`'s narration and cut sit behind
//! `window.rs`'s private `session_reads()`, so the page renders a state the flow rounds publish into
//! it through [`set_state`] — the same shape the effects lane uses (holders built empty, filled by a
//! refresh seam rather than by poking globals from the callback).

use adw::prelude::*;
use gtk4 as gtk;

use crate::bodies::ServerPath;
use crate::cut::Seg;
use crate::narrate_details;
use crate::narrate_off;
use crate::narrate_preview;
use crate::narrate_screen::{self, Audition, Fit};
use crate::narration::Entry;

/// What the page shows. Published by the flows, read by every handler.
#[derive(Debug, Clone, Default)]
pub struct NarrateState {
    pub entries: Vec<Entry>,
    pub segs: Vec<Seg>,
    /// The hand-picked takes of the narrator slot's recording, in seconds.
    pub takes: Vec<(f64, f64)>,
    /// The chosen voice's id (`narrate_screen::CAPTIONS`, `narrator1`, or a voices-folder name).
    pub voice: String,
    pub pitch: f64,    /// P.policy.ttsLanguage: the project's language, published by the flow that reads the project.
    /// Empty means the project states none, and `narrate_tts::language` then answers its fallback — it is
    /// never hard-coded here, because a model told to speak English reads Polish spelling as English.
    pub language: String,
    /// S2's two answers, published with the rest of the state so the page asks for no server probe of its
    /// own; both false on a default state, which is what "nothing checked yet" looks like downstream.
    pub audio_healthy: bool,
    pub audio_models: Vec<crate::services::AudioModel>,
    /// The TTS model id to ask for (`services::tts_model`), empty meaning the compiled-in default.
    pub tts_model: String,
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
    /// A synthesis is running: it outranks the other refusals on the row's ▶.
    pub busy: bool,
    pub narration_off: bool,
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
            busy: false,
            narration_off: false,
            covered_spans: Vec::new(),
            language: String::new(),
            audio_healthy: false,
            audio_models: Vec::new(),
            tts_model: String::new(),
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

fn mutate<T>(f: impl FnOnce(&mut NarrateState) -> T) -> T {
    NARRATE_STATE.with(|held| f(&mut held.borrow_mut()))
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
    let css = ".narrate-plate { background-color: #1c1c1d; border: 1px solid #000000; }\n";
    let provider = gtk::CssProvider::new();
    let _ = provider.load_from_data(css);
    gtk::StyleContext::add_provider_for_display(
        &gtk::gdk::Display::default().expect("a display to style"),
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    PAGE_CSS_LOADED.with(|cell| cell.set(true));
}

/// Build the whole page. Called from `page_box` once per window for the Narrate tab.
pub fn build() -> gtk::Widget {
    install_page_css_once();
    let split = gtk::Paned::new(gtk::Orientation::Horizontal);
    // §A.1: 560 px to the left pair, the window's extra width going to the lines.
    split.set_position(560);
    split.set_wide_handle(true);

    let left = gtk::Box::new(gtk::Orientation::Vertical, 8);
    // Fills the paned's 560 px rather than hugging its children: with `false` here nothing in the column
    // asked for width, so GTK gave the column its natural (~140 px) size and every control below was cut
    // off at the fold.
    left.set_hexpand(true);

    // **1** the Narration tick.
    let tick = gtk::CheckButton::with_label(narrate_off::TICK_LABEL);
    tick.set_widget_name("narration-tick");
    tick.set_tooltip_text(Some(narrate_off::TICK_TIP));
    left.append(&tick);

    // **2** the preview: one fixed 16:9 frame, the same pinned-height trick the Cut page's
    // `preview-panel` uses so GTK cannot collapse it against an expanding sibling.
    let preview = gtk::Box::new(gtk::Orientation::Vertical, 0);
    preview.set_widget_name("narrate-preview");
    preview.set_size_request(240, 135);
    // NOT `Start`: the frame is a fixed-size box inside a column that is itself only as wide as its widest
    // child, so `Start` left it at its minimum width and the whole left column collapsed to ~140 px while
    // the picture became an invisible sliver. `Fill` makes the column take the paned's 560 px and the
    // frame fill them at its own 16:9 shape.
    preview.set_halign(gtk::Align::Fill);
    preview.set_tooltip_text(Some("The finished frame with the narration mixed over the cut's sound"));
    let picture = gtk::DrawingArea::new();
    picture.set_widget_name("narrate-picture");
    // NOT `vexpand`, and it needs a size of its own as well: the box's 135 px is a MINIMUM, not a cap, so
    // an expanding child soaked up the paned's whole height and the "16:9 frame" rendered ~1170 px tall.
    // Dropping the expansion alone was not enough either — with no natural height and no request, GTK gave
    // the area 2 px inside the 135 px box. The explicit request makes the plate the frame it claims to be.
    picture.set_vexpand(false);
    picture.set_size_request(240, 135);
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
    preview.append(&picture);
    left.append(&preview);

    // **3** back 3 s · ▶ · forward 3 s, then **4** ＋ a line at this second.
    let transport = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    transport.set_widget_name("narrate-transport");
    transport.set_halign(gtk::Align::Start);
    for (name, label, tip) in [
        (
            "narrate-back-3s",
            "\u{219a} 3 s",
            "back three seconds \u{2014} the reach the row's \u{25b6} auditions from",
        ),
        ("narrate-play", "\u{25b6}", "play or pause the preview"),
        ("narrate-forward-3s", "3 s \u{21a3}", "forward three seconds"),
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
    left.append(&transport);

    // **5** the slider over the cut's own clock + the two-face clock.
    let scrub = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    scrub.set_widget_name("narrate-scrub-row");
    let slider = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 1.0, 0.1);
    slider.set_draw_value(false);
    slider.set_widget_name("narrate-slider");
    slider.set_hexpand(true);
    slider.set_tooltip_text(Some(
        "The cut's own clock \u{2014} what the edit removed is not on this bar",
    ));
    scrub.append(&slider);
    let clock = gtk::Label::new(Some(&narrate_screen::clock_line(0.0, 0.0, 0.0)));
    clock.set_widget_name("narrate-clock");
    clock.set_xalign(0.0);
    scrub.append(&clock);
    left.append(&scrub);

    // **6** the shared volume.
    let volume = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 1.0, 0.01);
    volume.set_draw_value(false);
    volume.set_value(0.8);
    volume.set_widget_name("narrate-volume");
    volume.set_tooltip_text(Some("Preview volume, shared with the Cut page"));
    left.append(&volume);

    // **7** the voice picker: unlabelled, its rows are the module's list in the module's order.
    let picker = gtk::DropDown::from_strings(&[
        "No audio \u{2014} captions only",
        "Narrator 1 \u{2014} cut from the recording",
    ]);
    picker.set_widget_name("voice-picker");
    picker.set_tooltip_text(Some(&narrate_screen::sample_tip()));
    // The row shown is the chosen voice's, spelled by `voice_options` rather than hardcoded: the spec's
    // shot has the picker carrying `Narrator 1 — 2026-09-16 18-43-01.mkv` as its own face. A number
    // outside the list leaves the dropdown on its first row, so this cannot crash on an empty state.
    if let Some(row) = narrate_screen::voice_options(1, &[])
        .iter()
        .position(|option| option.id == read_state().voice)
    {
        picker.set_selected(row as u32);
    }
    left.append(&picker);

    // **8** the take band's ＋ − ▶ and **9** Add file….
    let take_row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    take_row.set_widget_name("take-row");
    take_row.set_halign(gtk::Align::Start);
    let take_add = gtk::Button::from_icon_name(narrate_screen::ADD_LINE_ICON);
    take_add.set_widget_name("take-add");
    take_add.set_tooltip_text(Some(narrate_details::take_add_hint()));
    let take_remove = gtk::Button::with_label("\u{ff0d}");
    take_remove.set_widget_name("take-remove");
    take_remove.set_tooltip_text(Some(narrate_details::take_remove_hint()));
    let take_play = gtk::Button::with_label("\u{25b6}");
    take_play.set_widget_name("take-play");
    take_play.set_tooltip_text(Some("Play the takes from the red bar"));
    take_row.append(&take_add);
    take_row.append(&take_remove);
    take_row.append(&take_play);
    let add_file = gtk::Button::with_label("Add file\u{2026}");
    add_file.set_widget_name("voice-add-file");
    add_file.set_tooltip_text(Some(&narrate_screen::add_file_tip()));
    take_row.append(&add_file);
    left.append(&take_row);

    // **10** the take band itself.
    let band = gtk::DrawingArea::new();
    band.set_widget_name("take-band");
    band.set_size_request(240, narrate_screen::BAND_LANE_PX as i32);
    band.set_tooltip_text(Some(narrate_screen::band_status(&[])));
    // Same reason as the preview's plate above: no draw handler, so the lane needs its own painted ground
    // or the take marks have nothing to be drawn over.
    band.add_css_class("narrate-plate");
    left.append(&band);

    // **11** the sample sentence, **12** its ▶ ⏹ ⟳, **13** the pitch slider.
    let sample = gtk::Entry::new();
    sample.set_widget_name("sample-sentence");
    sample.set_text(narrate_screen::DEFAULT_SAMPLE);
    sample.set_tooltip_text(Some("Spoken in the selected voice"));
    left.append(&sample);

    let sample_row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    sample_row.set_widget_name("sample-row");
    sample_row.set_halign(gtk::Align::Start);
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
    left.append(&sample_row);

    let pitch = gtk::Scale::with_range(
        gtk::Orientation::Horizontal,
        narrate_screen::PITCH_MIN_SEMITONES,
        narrate_screen::PITCH_MAX_SEMITONES,
        narrate_screen::PITCH_STEP,
    );
    pitch.set_round_digits(1);
    pitch.set_widget_name("pitch-slider");
    pitch.set_tooltip_text(Some(&narrate_screen::pitch_tip()));
    left.append(&pitch);

    // **14–21** the lines column: one row per entry, sorted by clip then offset.
    let scroller = gtk::ScrolledWindow::new();
    // BOTH axes `Automatic`, not `External`: `External` tells GTK that some other code owns the vertical
    // adjustment and feeds the scrolled window through it. Nothing here does — no scrollbar widget, no
    // `vadjustment` wiring — so the list was never laid out as a scrollable area and only its first row
    // appeared however many entries were published. `Automatic` lets the window build and manage its own.
    scroller.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);
    scroller.set_hexpand(true);
    scroller.set_vexpand(true);
    // Bounded content height: an unbounded ScrolledWindow reports its child's FULL natural height as its
    // own minimum, so four rows made the page demand ~2682 px and GTK sized the window to that (a 1280x800
    // request rendered 2669x3025). These three calls are the GTK4 way of saying "this area scrolls; it does
    // not report its content height upward".
    scroller.set_min_content_height(240);
    scroller.set_max_content_height(560);
    scroller.set_propagate_natural_height(false);
    // A cap on the CONTENT height is not a cap on the widget: with `vexpand` inside a column whose own
    // minimum is already huge, GTK gave this window its content's full natural height anyway (measured:
    // rows 38 px x4 yet the list allocated 2682 px). The explicit request pins it; the content bounds
    // above then decide when the scrollbar appears.
    scroller.set_size_request(-1, 560);
    let lines = gtk::ListBox::new();
    lines.set_widget_name("narrate-lines");
    scroller.set_child(Some(&lines));

    // **22** the two readouts under the lines.
    let readouts = gtk::Box::new(gtk::Orientation::Vertical, 2);
    readouts.set_widget_name("narrate-readouts");
    let inputs = gtk::Label::new(Some(&narrate_screen::inputs_readout(0, 0.0, 0, 0, false, false)));
    inputs.set_widget_name("narrate-inputs");
    inputs.set_xalign(0.0);
    let outputs = gtk::Label::new(Some(narrate_screen::outputs_readout()));
    outputs.set_widget_name("narrate-outputs");
    outputs.set_xalign(0.0);
    readouts.append(&inputs);
    readouts.append(&outputs);

    let right = gtk::Box::new(gtk::Orientation::Vertical, 6);
    right.set_hexpand(true);
    right.append(&scroller);
    right.append(&readouts);

    split.set_start_child(Some(&left));
    split.set_end_child(Some(&right));
    upcast_widget(split)
}

fn upcast_widget(w: gtk::Paned) -> gtk::Widget {
    w.upcast()
}

/// Repaint the labels, the picker's rows and the line rows from the published state.
pub fn refresh(window: &adw::ApplicationWindow) {
    install_page_css_once();
    let s = read_state();
    if let Some(clock) = label(window, "narrate-clock") {
        clock.set_text(&narrate_screen::clock_line(s.session, s.cut_at, s.length));
    }
    if let Some(inputs) = label(window, "narrate-inputs") {
        inputs.set_text(&narrate_screen::inputs_readout(
            s.clips,
            s.length,
            s.unwritten,
            s.off_cut,
            s.has_timeline,
            s.has_cut,
        ));
    }
    if let Some(outputs) = label(window, "narrate-outputs") {
        outputs.set_text(narrate_screen::outputs_readout());
    }
    if let Some(band) = widget_in(window, "take-band") {
        band.set_tooltip_text(Some(narrate_screen::band_status(&s.takes)));
    }
    // The picker follows the published voice for the same reason the clock follows the published second:
    // a flow that changes the voice without this leaves the dropdown showing row 0 while the state says
    // otherwise, and the two disagree on screen.
    if let Some(picker) = widget_in(window, "voice-picker")
        .and_then(|w| w.downcast::<gtk::DropDown>().ok())
    {
        let files: Vec<&str> = s.voice_files.iter().map(String::as_str).collect();
        if let Some(row) = narrate_screen::voice_options(s.narrators, &files)
            .iter()
            .position(|option| option.id == s.voice)
        {
            picker.set_selected(row as u32);
        }
    }
    let before = read_state().entries.len();
    draw_rows(window, &s);
    // Always re-wired: the rows are rebuilt whole on every refresh, so their handlers go with the old
    // ones. A guard on the count would leave a same-length refresh with dead buttons.
    wire_row_buttons(window);
    apply_greys(window, s.narration_off);
}

fn draw_rows(window: &adw::ApplicationWindow, s: &NarrateState) {
    let Some(list) = widget_in(window, "narrate-lines")
        .and_then(|w| w.downcast::<gtk::ListBox>().ok())
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
            .then(
                a.at.partial_cmp(&b.at)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
    });
    for (index, entry) in ordered.clone() {
        let row = gtk::ListBoxRow::new();
        row.set_widget_name(&format!("narrate-line-{index}"));
        let box_ = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let session_second = entry.s + entry.at;
        let time = gtk::Label::new(Some(&narrate_screen::time_field(session_second)));
        time.set_widget_name(&format!("line-time-{index}"));
        box_.append(&time);
        let status = gtk::Label::new(Some(&narrate_screen::status_line(narrate_screen::status(
            entry, false,
        ))));
        status.set_widget_name(&format!("line-status-{index}"));
        box_.append(&status);
        let warning = gtk::Label::new(Some(
            &narrate_screen::fit_warning(fit_for(entry, &ordered)).unwrap_or_default(),
        ));
        warning.set_widget_name(&format!("line-warning-{index}"));
        if narrate_screen::row_is_red(fit_for(entry, &ordered)) {
            warning.add_css_class("error");
        }
        box_.append(&warning);
        for (suffix, label, tip) in [
            ("speak", "\u{25b6}", "speak this line"),
            ("reroll", "\u{27f3}", "a new take, same words"),
            ("remove", "\u{1f5d1}", "remove this line"),
        ] {
            let button = gtk::Button::with_label(label);
            button.set_widget_name(&format!("line-{suffix}-{index}"));
            button.set_tooltip_text(Some(tip));
            box_.append(&button);
        }
        let below = gtk::Button::from_icon_name(narrate_screen::ADD_LINE_ICON);
        below.set_widget_name(&format!("line-add-below-{index}"));
        below.set_tooltip_text(Some("Add a line below this one"));
        box_.append(&below);
        let text = gtk::Entry::new();
        text.set_widget_name(&format!("line-text-{index}"));
        text.set_text(&narrate_screen::write_box(entry));
        text.set_tooltip_text(Some(&narrate_screen::row_tooltip()));
        text.set_hexpand(true);
        box_.append(&text);
        row.set_child(Some(&box_));
        list.append(&row);
    }
}

/// The row's fit, asked of the render's own ladder (F4.3) rather than of a per-line estimate: every line of
/// THIS clip goes through `narrate_screen::mirror_fit` together, so what the row warns about is what the render
/// will do — including speeding the narration up, which a single line measured against its clip can never show.
///
/// Speech length comes from §F4.3's page-only rule: characters over [`narrate_screen::SPEECH_CHARS_PER_SECOND`]
/// (the page holds no measurement to clamp yet; when a take exists its wav length replaces this).
///
/// Two kinds of row never wear a fit warning because neither is spoken: an empty line and a caption (`pos` set —
/// "the viewer reads it; never spoken"). Those answer `Fit::Fits` here rather than being filtered inside the
/// mirror, since the skip is about what the row means, not about the ladder.
fn fit_for(entry: &Entry, all: &[(usize, &Entry)]) -> Fit {
    if entry.text.is_empty() || !entry.pos.is_empty() {
        return Fit::Fits;
    }
    // Every line standing on this clip's bounds, in play order, as the ladder wants it: (index, at, speech).
    let lines: Vec<(usize, f64, f64)> = all
        .iter()
        .filter(|(_, other)| other.s == entry.s && other.e == entry.e)
        .map(|(index, other)| {
            let spoken = if other.text.is_empty() || !other.pos.is_empty() {
                0.0
            } else {
                narrate_screen::page_speech_seconds(
                    other.text.chars().count(),
                    narrate_screen::SPEECH_CHARS_PER_SECOND,
                )
            };
            (*index, other.at, spoken)
        })
        .collect();
    narrate_screen::mirror_fit((entry.e - entry.s).max(0.0), &lines)
}

/// §1 `Off greys lines, preview and voice picker`: exactly the three names the rules module lists.
fn apply_greys(window: &adw::ApplicationWindow, off: bool) {
    let targets = [
        ("lines", "narrate-lines"),
        ("preview", "narrate-preview"),
        ("voice", "voice-picker"),
    ];
    for (which, name) in targets {
        assert!(
            narrate_screen::off_greys().contains(&which),
            "{which} greying off is not one of off_greys()'s three"
        );
        if let Some(widget) = widget_in(window, name) {
            widget.set_sensitive(!off);
        }
    }
    // The controls that only exist to serve a narration go with them.
    for name in [
        "narrate-add-line",
        "take-band",
        "take-add",
        "take-remove",
        "take-play",
        "sample-sentence",
        "sample-play",
        "sample-stop",
        "sample-reroll",
        "pitch-slider",
    ] {
        if let Some(widget) = widget_in(window, name) {
            widget.set_sensitive(!off);
        }
    }
}

fn widget_in(window: &adw::ApplicationWindow, name: &str) -> Option<gtk::Widget> {
    let content = window.content()?;
    crate::ui::window::find_widget_by_name(&content, name)
}

fn label(window: &adw::ApplicationWindow, name: &str) -> Option<gtk::Label> {
    widget_in(window, name)?.downcast().ok()
}

/// Say it on the status line and in the log, the page's two output channels.
fn say(window: &adw::ApplicationWindow, said: &str) -> String {
    if let Some(status) = crate::ui::window::find_status(window.upcast_ref()) {
        status.set_text(said);
    }
    crate::ui::window::log_line(said);
    said.to_string()
}

// --- the seams: one widget press, one rules call, one printed sentence -------------------------------

/// ‹‹ back three seconds; the clock is what the button reports, §1 giving it no sentence of its own.
pub fn press_back(window: &adw::ApplicationWindow) -> String {
    mutate(|s| {
        s.session = (s.session - narrate_screen::BACK_SECONDS).max(0.0);
        s.cut_at = (s.cut_at - narrate_screen::BACK_SECONDS).max(0.0);
    });
    let s = read_state();
    refresh(window);
    say(window, &narrate_screen::clock_line(s.session, s.cut_at, s.length))
}

pub fn press_forward(window: &adw::ApplicationWindow) -> String {
    mutate(|s| {
        s.session += narrate_screen::BACK_SECONDS;
        s.cut_at += narrate_screen::BACK_SECONDS;
    });
    let s = read_state();
    refresh(window);
    say(window, &narrate_screen::clock_line(s.session, s.cut_at, s.length))
}

/// The session spans the preview may cue against. An unpublished state (empty) is read as full coverage of
/// the cut: the refusal exists for a recording that does not reach the cue, not for a flow that never said.
fn preview_covered(s: &NarrateState) -> Vec<(f64, f64)> {
    if s.covered_spans.is_empty() {
        return s.segs.iter().map(|seg| (seg.s, seg.e)).collect();
    }
    s.covered_spans.clone()
}

/// F4.5 S1: one click on the picture, or the transport's ▶ — the same door, so the two cannot drift.
/// Asks `narrate_preview::press_picture` with what this page knows: the kept clips, the line under the
/// playhead (if any) at its own start second, and the covered spans. Stores the answer in `playing` and says
/// the refusal verbatim, or names the second playback starts from.
pub fn press_preview_picture(window: &adw::ApplicationWindow) -> String {
    let s = read_state();
    let line_start = narrate_preview::line_at(s.session, &s.entries)
        .map(|index| s.entries[index].s + s.entries[index].at);
    let pressed = narrate_preview::press_picture(s.playing, &s.segs, line_start, &preview_covered(&s));
    let next_playing = pressed.playing();
    mutate(|state| state.playing = next_playing);
    match pressed {
        narrate_preview::Pressed::Refused(why) => say(window, &why),
        narrate_preview::Pressed::Playing { from } => say(
            window,
            &format!(
                "playing the cut from {} \u{2014} \u{23f9} stops both players",
                narrate_screen::time_field(from)
            ),
        ),
        narrate_preview::Pressed::Paused => {
            say(window, "paused \u{2014} \u{25b6} plays on from here")
        }
    }
}

/// The preview's play/pause. Routes through the picture's door: inventory §A.3/§A.4 are one toggle wearing
/// two faces, and two implementations would disagree about what "play" means after an edit.
pub fn press_narrate_play(window: &adw::ApplicationWindow) -> String {
    press_preview_picture(window)
}

/// Whether the preview is running right now. Exported so the run bar can ask before reading a ⏹ press as the
/// end of a run: while a preview runs, ⏹ belongs to the preview (F4.5 S6).
pub fn narrate_preview_playing() -> bool {
    read_state().playing
}

/// F4.5 S6: ⏹ over the preview. Stops the picture and the voice riding along with it and hands ▶ back to
/// the step; the sentence comes from `narrate_preview::hand_play_back`, so the bar cannot claim ▶ back while
/// a player is still going.
pub fn press_preview_stop(window: &adw::ApplicationWindow) -> String {
    let stopped = narrate_preview::stop();
    mutate(|state| state.playing = false);
    let said = narrate_preview::hand_play_back(stopped)
        .unwrap_or("the preview did not stop \u{2014} see log")
        .to_string();
    say(window, &said)
}

/// **4** ＋ a line at this second. Ok pushes the entry where the rule put it.
pub fn press_narrate_add_line(window: &adw::ApplicationWindow) -> String {
    let s = read_state();
    match narrate_screen::add_at_playhead(s.session, &s.segs, &s.entries) {
        Ok(at) => {
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
                    at: 0.0,
                    ..Default::default()
                });
            });
            refresh(window);
            say(
                window,
                &format!("a line starts at {}", narrate_screen::time_field(at)),
            )
        }
        Err(refused) => say(window, &refused),
    }
}

/// S3's answer when no upload was scripted: the reference is on disk but nothing carried it to the server.
const NO_UPLOAD: &str = "no voice reference was uploaded for this line \u{2014} the speech request has no \
                     server path to name";

/// S4/S5's answer when no speech reply was scripted: the request went out and nothing answered it.
const NO_SPEECH_REPLY: &str = "the audio.cpp server did not answer POST /v1/audio/speech";

thread_local! {
    /// A scripted TTS reply for one press, shaped `(status, bytes)` — the same trick `window.rs`'s
    /// `NARRATE_SCRIPT` plays for the F4.2 narration call: the real row button's handler reads THIS, so a
    /// test drives the widget and never opens a socket. Empty means no server dialled, which is a refusal
    /// rather than a silent pass.
    static SPEECH_SCRIPT: std::cell::RefCell<Option<(u16, Vec<u8>)>> =
        const { std::cell::RefCell::new(None) };
    /// How many times the reference went up. Held apart from the reply so a test can assert S3 ran for every
    /// line without reading the take back off disk.
    static UPLOAD_COUNT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Script the reply this window's ▶ gets from `POST /v1/audio/speech`.
pub fn set_speech_script(reply: Option<(u16, Vec<u8>)>) {
    SPEECH_SCRIPT.with(|cell| *cell.borrow_mut() = reply);
}

/// Whether a TTS reply is loaded — exported so a test can assert the button read the script rather than
/// falling through to the no-server path.
pub fn speech_script_loaded() -> bool {
    SPEECH_SCRIPT.with(|cell| cell.borrow().is_some())
}

/// How many uploads have happened since the last reset (S3 re-uploads per line; see
/// [`crate::bodies::VOICE_REF_REUPLOADED_EVERY_LINE`]).
pub fn upload_count() -> usize {
    UPLOAD_COUNT.get()
}

/// Forget the scripted reply and the upload tally.
pub fn clear_speech_script() {
    SPEECH_SCRIPT.with(|cell| *cell.borrow_mut() = None);
    UPLOAD_COUNT.set(0);
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
    let (case, said) = if s.recording_covers_clip {
        (case, said)
    } else {
        narrate_screen::audition_without_recording(case, index + 1)
    };
    let line = match said {
        Some(said) => said,
        None => match case {
            // The line IS to be spoken: hand it to F4.4 rather than only saying so. The audition above owns
            // what the refusals sound like; this leg owns whether a wav actually lands.
            Audition::Speak { .. } => return speak_this_line(window, &s, entry),
            _ => narrate_screen::spoken_alone(index + 1),
        },
    };
    say(window, &line)
}

/// F4.4 for one row: the take's key and seed come from the record itself, the language and the server's
/// answers from the published state, and the two network legs from the script thread-locals above (empty =
/// no server dialled). Every step's own sentence reaches the status line verbatim: the module names the
/// model and the reason, and the page adds nothing to that diagnosis.
fn speak_this_line(
    window: &adw::ApplicationWindow,
    s: &NarrateState,
    entry: &Entry,
) -> String {
    let Some(tree) = crate::ui::window::narrate_session_tree() else {
        return say(window, "no project folder is open \u{2014} nowhere to keep the take");
    };
    let key = crate::narration::tts_key(entry, Some(&s.voice), None);
    let seed = crate::narration::tts_seed(&key);
    let model = if s.tts_model.is_empty() {
        crate::services::TTS_MODEL.to_string()
    } else {
        s.tts_model.clone()
    };
    let outcome = crate::narrate_tts::speak_line(
        &tree,
        &entry.text,
        &entry.emotion,
        seed,
        &key,
        &s.language,
        s.audio_healthy,
        &s.audio_models,
        &model,
        |_tree| {
            UPLOAD_COUNT.with(|count| count.set(count.get() + 1));
            Ok(ServerPath::from_upload("/tmp/naivepost-voice-ref.wav")
                .expect("a literal absolute path is a server path"))
        },
        |_body| match SPEECH_SCRIPT.with(|cell| cell.borrow().clone()) {
            Some(reply) => Ok(reply),
            None => Err(NO_SPEECH_REPLY.to_string()),
        },
    );
    say(window, &crate::narrate_tts::outcome_said(&outcome))
}

/// **17** ↻ re-roll: refused when there is nothing to draw again of.
pub fn press_line_reroll(window: &adw::ApplicationWindow, index: usize) -> String {
    let s = read_state();
    let has_line = s
        .entries
        .get(index)
        .map(|entry| !entry.text.is_empty())
        .unwrap_or(false);
    let said = match narrate_screen::re_roll_refusal(index + 1, has_line) {
        Some(refused) => refused,
        None => narrate_details::new_take_status(index),
    };
    say(window, &said)
}

/// **18** ＋ a line below this one.
pub fn press_line_add_below(window: &adw::ApplicationWindow, index: usize) -> String {
    let s = read_state();
    let Some(entry) = s.entries.get(index) else {
        return say(window, narrate_details::nothing_picked());
    };
    let audio_end = entry.s + entry.at;
    let clip_end = entry.e;
    match narrate_screen::add_below(audio_end, clip_end) {
        Ok(at) => {
            mutate(|state| {
                if let Some(row) = state.entries.get_mut(index) {
                    row.at = at - row.s;
                }
            });
            refresh(window);
            say(
                window,
                &format!("a line added below at {}", narrate_screen::time_field(at)),
            )
        }
        Err(refused) => say(window, &refused),
    }
}

/// **19** 🗑 remove this line.
pub fn press_line_remove(window: &adw::ApplicationWindow, index: usize) -> String {
    let s = read_state();
    let Some(entry) = s.entries.get(index) else {
        return say(window, narrate_details::nothing_picked());
    };
    let clip_s = entry.s;
    let other_on_clip = s
        .entries
        .iter()
        .any(|other| other.s == clip_s && !(other.s == entry.s && other.at == entry.at));
    let said = narrate_screen::remove_line(other_on_clip, clip_s);
    mutate(|state| {
        state.entries.remove(index);
    });
    refresh(window);
    say(window, &said)
}

/// **8** ＋ makes the selection a take, refused under P.eng.takeMinSeconds.
pub fn press_take_add(window: &adw::ApplicationWindow, selection: (f64, f64)) -> String {
    if !narrate_screen::can_make_take(selection) {
        let seconds = selection.1 - selection.0;
        return say(window, &narrate_screen::take_too_short(seconds));
    }
    let takes = mutate(|s| {
        s.takes = narrate_screen::add_takes(&s.takes, selection);
        s.takes.clone()
    });
    refresh(window);
    say(
        window,
        &narrate_details::commit_status(
            "takes",
            takes.len(),
            takes.iter().map(|(a, b)| b - a).sum(),
            narrate_screen::take_min(),
        ),
    )
}

/// **8** − takes those seconds back out, splitting takes when they fall inside one.
pub fn press_take_remove(window: &adw::ApplicationWindow, selection: (f64, f64)) -> String {
    let takes = mutate(|s| {
        s.takes = narrate_screen::remove_takes(&s.takes, selection);
        s.takes.clone()
    });
    refresh(window);
    if takes.is_empty() {
        return say(window, narrate_details::no_takes_left());
    }
    say(
        window,
        &narrate_details::commit_status(
            "takes",
            takes.len(),
            takes.iter().map(|(a, b)| b - a).sum(),
            narrate_screen::take_min(),
        ),
    )
}

/// **8** ▶ plays the takes from the red bar (the raw recording when none).
pub fn press_take_play(window: &adw::ApplicationWindow) -> String {
    let s = read_state();
    let total: f64 = s.takes.iter().map(|(a, b)| b - a).sum();
    say(window, &narrate_details::playing_takes(s.takes.len(), total, s.session))
}

/// **12** ▶ speaks the sample, or says which click would clear what stops it.
pub fn press_sample_play(window: &adw::ApplicationWindow) -> String {
    let s = read_state();
    let text = widget_in(window, "sample-sentence")
        .and_then(|w| w.downcast::<gtk::Entry>().ok())
        .map(|e| e.text().to_string())
        .unwrap_or_default();
    let said = sample_ladder(&s, &text);
    say(window, said)
}

/// The four ways ▶ Sample refuses, in the order §6 lists them, then the working answer.
fn sample_ladder(s: &NarrateState, text: &str) -> &'static str {
    if s.busy {
        return narrate_details::busy_sample_status();
    }
    if s.voice == narrate_screen::CAPTIONS {
        return narrate_details::no_voice_to_sample_refusal();
    }
    if s.voice.is_empty() {
        return narrate_details::pick_a_voice_refusal();
    }
    if text.trim().is_empty() {
        return narrate_details::empty_sample_refusal();
    }
    narrate_details::sample_status()
}

pub fn press_sample_stop(window: &adw::ApplicationWindow) -> String {
    mutate(|s| s.busy = false);
    say(window, narrate_details::sample_stopped_status())
}

pub fn press_sample_reroll(window: &adw::ApplicationWindow) -> String {
    let s = read_state();
    let text = widget_in(window, "sample-sentence")
        .and_then(|w| w.downcast::<gtk::Entry>().ok())
        .map(|e| e.text().to_string())
        .unwrap_or_default();
    let said = sample_ladder(&s, &text);
    say(window, said)
}

/// **13** pitch: clamped to the slider's ends, reported with the voice it will clone from.
pub fn press_pitch(window: &adw::ApplicationWindow, delta: f64) -> String {
    let pitch = mutate(|s| {
        s.pitch = narrate_screen::clamp_pitch(s.pitch + delta);
        s.pitch
    });
    let voice = read_state().voice;
    say(window, &narrate_details::voice_line(&voice, pitch))
}

/// **1** the Narration tick. Off greys the three things `off_greys()` names and says which they were.
pub fn set_narration_off(window: &adw::ApplicationWindow, off: bool) -> String {
    mutate(|s| s.narration_off = off);
    apply_greys(window, off);
    let said = if off {
        format!(
            "narration off \u{2014} {} greyed",
            narrate_screen::off_greys().join(", ")
        )
    } else {
        "narration on".to_string()
    };
    say(window, &said)
}

/// Attach every handler. Called from `build_window` after `set_content`, like every other control on
/// every page: a click handler attached before the widget is inside the realized tree never fires.
pub fn wire(window: &adw::ApplicationWindow) {
    let win = window.clone();
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
    if let Some(tick) = widget_in(&win, "narration-tick")
        .and_then(|w| w.downcast::<gtk::CheckButton>().ok())
    {
        let w = win.clone();
        tick.connect_toggled(move |t| {
            // The tick is checked when there IS a narration, so unchecked means off.
            set_narration_off(&w, !t.is_active());
        });
    }
    if let Some(pitch) = widget_in(&win, "pitch-slider")
        .and_then(|w| w.downcast::<gtk::Scale>().ok())
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
                        mutate(|s| s.pitch = narrate_screen::clamp_pitch(value));
                        let voice = read_state().voice;
                        say(&w, &narrate_details::voice_line(&voice, value));
                        glib::ControlFlow::Break
                    }
                },
            );
        });
    }
    refresh(&win);
    // Rows are drawn by `refresh`, so their handlers go on after it: before this they do not exist yet.
    wire_row_buttons(&win);
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
        if let Some(button) = crate::ui::line_step_button(window, &format!("line-add-below-{index}")) {
            let w = window.clone();
            button.connect_clicked(move |_| {
                press_line_add_below(&w, index);
            });
        }
        if let Some(button) = crate::ui::line_step_button(window, &format!("line-remove-{index}")) {
            let w = window.clone();
            button.connect_clicked(move |_| {
                press_line_remove(&w, index);
            });
        }
    }
}
