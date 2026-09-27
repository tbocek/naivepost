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

use crate::cut::Seg;
use crate::narrate_details;
use crate::narrate_off;
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
    pub pitch: f64,
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

/// Build the whole page. Called from `page_box` once per window for the Narrate tab.
pub fn build() -> gtk::Widget {
    let split = gtk::Paned::new(gtk::Orientation::Horizontal);
    // §A.1: 560 px to the left pair, the window's extra width going to the lines.
    split.set_position(560);
    split.set_wide_handle(true);

    let left = gtk::Box::new(gtk::Orientation::Vertical, 8);
    left.set_hexpand(false);

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
    preview.set_halign(gtk::Align::Start);
    preview.set_tooltip_text(Some("The finished frame with the narration mixed over the cut's sound"));
    let picture = gtk::DrawingArea::new();
    picture.set_widget_name("narrate-picture");
    picture.set_vexpand(true);
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
    scroller.set_policy(gtk::PolicyType::External, gtk::PolicyType::Automatic);
    scroller.set_hexpand(true);
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
    let mut ordered: Vec<(usize, &Entry)> = s.entries.iter().enumerate().collect();
    ordered.sort_by(|(_, a), (_, b)| {
        a.s.partial_cmp(&b.s)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(
                a.at.partial_cmp(&b.at)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
    });
    for (index, entry) in ordered {
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
            &narrate_screen::fit_warning(fit_for(entry)).unwrap_or_default(),
        ));
        warning.set_widget_name(&format!("line-warning-{index}"));
        if narrate_screen::row_is_red(fit_for(entry)) {
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

/// The row's fit, asked of the only estimate available before a take exists: §C.2's chars-per-second.
fn fit_for(entry: &Entry) -> Fit {
    let room = (entry.e - (entry.s + entry.at)).max(0.0);
    let speech = entry.text.chars().count() as f64 / narrate_screen::SPEECH_CHARS_PER_SECOND;
    if entry.text.is_empty() || !entry.pos.is_empty() {
        return Fit::Fits;
    }
    if speech > room {
        Fit::Overruns {
            past: speech - room,
            sped_up: false,
        }
    } else if speech + narrate_screen::SPEECH_TAIL_SECONDS > room {
        Fit::Tight {
            speech,
            before: room,
            next_line: false,
        }
    } else {
        Fit::Fits
    }
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

/// The preview's play/pause. §1 gives it no sentence, so the clock at the new position is reported.
pub fn press_narrate_play(window: &adw::ApplicationWindow) -> String {
    mutate(|s| s.playing = !s.playing);
    let s = read_state();
    say(window, &narrate_screen::clock_line(s.session, s.cut_at, s.length))
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
            Audition::Speak { .. } => narrate_screen::speaking_line(),
            _ => narrate_screen::spoken_alone(index + 1),
        },
    };
    say(window, &line)
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
