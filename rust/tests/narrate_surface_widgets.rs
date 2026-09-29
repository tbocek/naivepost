//! §07-narrate#1-screen — the WIRE: a click on the Narrate page reaches the rules in
//! `naivepost::narrate_screen` / `narrate_details`, and what they answer lands on the status line and in
//! the log instead of staying inside a callback.
//!
//! `rust/tests/narrate_screen_widgets.rs` proves the rules; this file proves the page can be reached at
//! all — before this round `page_box` drew only a title for the Narrate tab, so every rule was tested and
//! no click arrived at any of them.
//!
//! One application, one `connect_activate`, exactly one `#[test]`: these checks mutate process-shared
//! state (this window's widgets and the page's published state), so they run once and report through the
//! flags asserted at the end — the shape tests/clamp_effects_widget.rs uses.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut::Seg;
use naivepost::narrate_details;
use naivepost::narrate_off;
use naivepost::narrate_screen::{self, CAPTIONS};
use naivepost::narration::Entry;
use naivepost::shell::Page;
use naivepost::ui;

use naivepost::narrate_tts;

/// The session folder this window's own press resolves its sample into, so the assertion is the leg's
/// real answer read from the real path rather than a copy of its wording typed in twice.
fn session_tree() -> naivepost::layout::Tree {
    let dir = naivepost::startup::session_dir(&std::env::current_dir().unwrap());
    naivepost::layout::Tree::new(&dir).expect("the session folder ends in .naivepost")
}

#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::{fixture_dir, hold_last_window, release_last_window, settle, status_text, widget_in};

static RAN_TICK: AtomicBool = AtomicBool::new(false);
static RAN_TRANSPORT: AtomicBool = AtomicBool::new(false);
static RAN_ADD_LINE: AtomicBool = AtomicBool::new(false);
static RAN_SPEAK: AtomicBool = AtomicBool::new(false);
static RAN_ROW_BUTTONS: AtomicBool = AtomicBool::new(false);
static RAN_TAKE_BAND: AtomicBool = AtomicBool::new(false);
static RAN_SAMPLE: AtomicBool = AtomicBool::new(false);
static RAN_PITCH: AtomicBool = AtomicBool::new(false);
static RAN_READOUTS: AtomicBool = AtomicBool::new(false);
static RAN_VOICE_PICKER: AtomicBool = AtomicBool::new(false);
static RAN_BACK_CLAMP: AtomicBool = AtomicBool::new(false);
static RAN_LAST_ROW: AtomicBool = AtomicBool::new(false);
static RAN_PITCH_DEBOUNCE: AtomicBool = AtomicBool::new(false);
static RAN_FIT_ROW: AtomicBool = AtomicBool::new(false);
static RAN_OFF_WIRE: AtomicBool = AtomicBool::new(false);

/// Let the main context run what the widget emissions queued.

fn button(window: &adw::ApplicationWindow, name: &str) -> gtk::Button {
    widget_in(window, name)
        .and_then(|w| w.downcast::<gtk::Button>().ok())
        .unwrap_or_else(|| panic!("the Narrate page carries {name}"))
}

fn label_text(window: &adw::ApplicationWindow, name: &str) -> String {
    widget_in(window, name)
        .and_then(|w| w.downcast::<gtk::Label>().ok())
        .map(|l| l.text().to_string())
        .unwrap_or_else(|| panic!("the Narrate page carries a label named {name}"))
}

/// Three kept clips of ten seconds each, so every second the page writes down is checkable by hand.
fn three_clips() -> Vec<Seg> {
    vec![
        Seg { s: 0.0, e: 10.0, cam: 0, ..Default::default() },
        Seg { s: 10.0, e: 20.0, cam: 0, ..Default::default() },
        Seg { s: 20.0, e: 30.0, cam: 0, ..Default::default() },
    ]
}

/// One written line on clip 1, one caption on clip 2, one blank row on clip 3: the three states the row's
/// ▶ has to tell apart, plus the two the other buttons refuse about.
fn three_entries() -> Vec<Entry> {
    vec![
        Entry {
            s: 0.0,
            e: 10.0,
            at: 2.0,
            text: "a spoken line".into(),
            emotion: "calm".into(),
            pos: String::new(),
            roll: 0,
        },
        Entry {
            s: 10.0,
            e: 20.0,
            at: 1.0,
            text: "a caption".into(),
            emotion: String::new(),
            pos: "bottom".into(),
            roll: 0,
        },
        Entry {
            s: 20.0,
            e: 30.0,
            at: 1.0,
            text: String::new(),
            emotion: String::new(),
            pos: String::new(),
            roll: 0,
        },
    ]
}

fn base_state() -> ui::NarrateState {
    ui::NarrateState {
        entries: three_entries(),
        segs: three_clips(),
        takes: vec![],
        voice: String::new(),
        pitch: 0.0,
        session: 5.0,
        cut_at: 5.0,
        length: 30.0,
        clips: 3,
        unwritten: 0,
        off_cut: 0,
        has_cut: true,
        has_timeline: true,
        narrators: 1,
        voice_files: vec!["warm.wav".into()],
        recording_covers_clip: true,
        playing: false,
        busy: false,
        narration_off: false,
        // F4.4's three inputs, published by the flow that reads the project. Left at "nothing checked":
        // block (4) drives the refusals, and block (11) sets these where it wants a server that answers.
        language: String::new(),
        // F4.5's coverage spans: unpublished, which the page reads as full coverage of the cut.
        covered_spans: Vec::new(),
    }
}

/// A window sitting on the Narrate tab with the given state published into the page.
fn narrate_page(app: &adw::Application, state: ui::NarrateState) -> adw::ApplicationWindow {
    release_last_window();
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    assert!(
        model.sources.iter().any(|source| source.footage),
        "the fixture must carry a footage row or the Narrate tab is locked (shell::lock)"
    );
    let window = ui::build_window(app, &model, "Prepare");
    hold_last_window(window.clone());
    window.present();
    ui::tab_button(&window, Page::Narrate)
        .expect("the shell has a Narrate tab")
        .emit_by_name::<()>("clicked", &[]);
    settle();
    // The surface itself: without these names the tab is a bare title and nothing below can fire.
    for name in [
        "narration-tick",
        "narrate-preview",
        "narrate-back-3s",
        "narrate-play",
        "narrate-forward-3s",
        "narrate-add-line",
        "narrate-slider",
        "narrate-clock",
        "narrate-volume",
        "voice-picker",
        "take-add",
        "take-remove",
        "take-play",
        "voice-add-file",
        "take-band",
        "sample-sentence",
        "sample-play",
        "sample-stop",
        "sample-reroll",
        "pitch-slider",
        "narrate-lines",
        "narrate-inputs",
        "narrate-outputs",
    ] {
        assert!(
            widget_in(&window, name).is_some(),
            "§1's numbered widget {name} is missing from the Narrate page"
        );
    }
    ui::set_state(state);
    ui::refresh(&window);
    settle();
    window
}

// --- the round ----------------------------------------------------------------------------------------

fn narrate_round(app: &adw::Application) {
    // --- (1) the tick greys exactly the three things off_greys() names ---------------------------
    let window = narrate_page(app, base_state());
    assert_eq!(
        narrate_screen::off_greys().len(),
        3,
        "§1 says off greys lines, preview and voice picker -- three things"
    );
    let tick = widget_in(&window, "narration-tick")
        .and_then(|w| w.downcast::<gtk::CheckButton>().ok())
        .expect("the Narration tick is a check button");
    // The page starts with the narration ON (narrate_off::tick_checked), so the three surfaces are live
    // before the first toggle.
    for name in ["narrate-lines", "narrate-preview", "voice-picker"] {
        assert!(
            widget_in(&window, name).unwrap().is_sensitive(),
            "{name} is live while the narration is on"
        );
    }
    // Drive the real door the tick's handler calls, rather than toggling the box: `set_state` above resets
    // the published state, so the greying has to be pushed through the seam that owns it.
    let off_said = ui::set_narration_off(&window, true);
    settle();
    for name in ["narrate-lines", "narrate-preview", "voice-picker"] {
        assert!(
            !widget_in(&window, name).unwrap().is_sensitive(),
            "{name} goes insensitive when the narration is off"
        );
    }
    assert_eq!(
        status_text(&window),
        off_said,
        "the status line names which three went grey"
    );
    ui::set_narration_off(&window, false);
    settle();
    for name in ["narrate-lines", "narrate-preview", "voice-picker"] {
        assert!(
            widget_in(&window, name).unwrap().is_sensitive(),
            "{name} comes back with the narration"
        );
    }
    RAN_TICK.store(true, Ordering::SeqCst);
    window.close();
    settle();

    // --- (2) the transport steps BACK_SECONDS and the clock follows -----------------------------
    let mut state = base_state();
    state.session = 10.0;
    state.cut_at = 10.0;
    let window = narrate_page(app, state);
    button(&window, "narrate-back-3s").emit_by_name::<()>("clicked", &[]);
    button(&window, "narrate-back-3s").emit_by_name::<()>("clicked", &[]);
    settle();
    let after_two_back = ui::read_state().session;
    assert!(
        (after_two_back - (10.0 - 2.0 * narrate_screen::BACK_SECONDS)).abs() < 1e-9,
        "two presses moved by twice BACK_SECONDS (3.0): got {after_two_back}"
    );
    button(&window, "narrate-forward-3s").emit_by_name::<()>("clicked", &[]);
    settle();
    let now = ui::read_state().session;
    assert!(
        (now - 7.0).abs() < 1e-9,
        "back twice and forward once from 10.0 lands on 7.0: got {now}"
    );
    assert_eq!(
        label_text(&window, "narrate-clock"),
        narrate_screen::clock_line(now, ui::read_state().cut_at, 30.0),
        "the clock reads session \u{b7} cut/length for the new position"
    );
    RAN_TRANSPORT.store(true, Ordering::SeqCst);
    window.close();
    settle();

    // --- (3) ＋ at the playhead: refused over a gap, added inside a clip -------------------------
    let mut state = base_state();
    // Between the clips: 10.0 is the border, and `clip_at` wants s <= at < e, so aim past the third clip.
    state.session = 45.0;
    let window = narrate_page(app, state);
    button(&window, "narrate-add-line").emit_by_name::<()>("clicked", &[]);
    settle();
    assert_eq!(
        status_text(&window),
        "the playhead is between clips \u{2014} the cut has nothing to narrate here",
        "the refusal is add_at_playhead's own sentence, printed by the real click"
    );
    assert_eq!(ui::read_state().entries.len(), 3, "a refused add adds nothing");
    let mut state = base_state();
    state.session = 5.0;
    ui::set_state(state.clone());
    ui::refresh(&window);
    settle();
    button(&window, "narrate-add-line").emit_by_name::<()>("clicked", &[]);
    settle();
    assert_eq!(
        ui::read_state().entries.len(),
        4,
        "a press inside a clip put a new row on the list"
    );
    RAN_ADD_LINE.store(true, Ordering::SeqCst);
    window.close();
    settle();

    // --- (4) the row's ▶ says its own thing for each of the three kinds --------------------------
    let window = narrate_page(app, base_state());
    // Rows are sorted by clip then offset, so index 0 = spoken, 1 = caption, 2 = blank.
    button(&window, "line-speak-2").emit_by_name::<()>("clicked", &[]);
    settle();
    let blank_said = status_text(&window);
    assert_eq!(
        blank_said,
        "clip 3 has no line \u{2014} playing it on its own audio",
        "the wordless row says what it is"
    );
    button(&window, "line-speak-1").emit_by_name::<()>("clicked", &[]);
    settle();
    let caption_said = status_text(&window);
    assert_eq!(
        caption_said,
        "line 2 is a caption \u{2014} read, never spoken; playing its moment",
        "the caption row says what it is"
    );
    button(&window, "line-speak-0").emit_by_name::<()>("clicked", &[]);
    settle();
    let spoken_said = status_text(&window);
    // F4.4 reached through the row's ▶: this state has neither a reference on disk nor a checked server, so
    // the flow refuses at S1 and says so on the status line — where the old bare "synthesizing…" used to
    // promise work it never did.
    assert!(
        spoken_said.contains("voice_ref.wav") || spoken_said.contains("/health"),
        "a speak with nothing in place names what is missing, not a promise: {spoken_said}"
    );
    assert_ne!(blank_said, caption_said, "each answer is its own words");
    assert_ne!(caption_said, spoken_said, "each answer is its own words");
    RAN_SPEAK.store(true, Ordering::SeqCst);
    window.close();
    settle();

    // --- (5) ↻ / ＋ below / 🗑 on rows ---------------------------------------------------------
    let window = narrate_page(app, base_state());
    button(&window, "line-reroll-2").emit_by_name::<()>("clicked", &[]);
    settle();
    assert_eq!(
        status_text(&window),
        "clip 3 has no line to re-roll",
        "↻ refuses where there is nothing to draw again of"
    );
    button(&window, "line-reroll-0").emit_by_name::<()>("clicked", &[]);
    settle();
    // F4.7's ↻ is roll + 1 and then the SAME real speak leg a row's ▶ uses. This state has neither a
    // reference on disk nor a checked server, so the flow refuses at F4.4 S1 and says what is missing —
    // where the old bare "new take, speaking it" used to promise work it never did.
    let re_rolled = status_text(&window);
    assert!(
        re_rolled.contains("voice_ref.wav") || re_rolled.contains("/health"),
        "a re-roll with nothing in place names what is missing rather than promising a take: {re_rolled}"
    );
    // ＋ below where the clip ends first: pull the written row to its clip's last seconds.
    let mut squeezed = base_state();
    squeezed.entries[0].at = 8.6;
    let window2 = narrate_page(app, squeezed);
    button(&window2, "line-add-below-0").emit_by_name::<()>("clicked", &[]);
    settle();
    assert_eq!(
        status_text(&window2),
        "no room after this line \u{2014} the clip ends first",
        "＋ below refuses when the clip ends before the new line could be heard"
    );
    button(&window, "line-remove-2").emit_by_name::<()>("clicked", &[]);
    settle();
    assert_eq!(
        status_text(&window),
        narrate_screen::remove_line(false, 20.0),
        "🗑 on the clip's only line says the longer sentence"
    );
    assert_eq!(ui::read_state().entries.len(), 2, "the row left the list");
    RAN_ROW_BUTTONS.store(true, Ordering::SeqCst);
    window.close();
    window2.close();
    settle();

    // --- (6) the take band: P.eng.takeMinSeconds refuses, and ▶ with no takes says so -----------
    let window = narrate_page(app, base_state());
    button(&window, "take-add").emit_by_name::<()>("clicked", &[]);
    settle();
    // The button carries no selection of its own; the seam is what the drag-end calls, so drive it with
    // the too-short span the same way the band's gesture would.
    let refused = ui::press_take_add(&window, (3.0, 3.2));
    assert_eq!(
        refused,
        narrate_screen::take_too_short(0.2),
        "// P.eng.takeMinSeconds 0.4: a 0.2 s span is refused with the module's own sentence"
    );
    assert!(ui::read_state().takes.is_empty(), "no take was made");
    button(&window, "take-play").emit_by_name::<()>("clicked", &[]);
    settle();
    assert_eq!(
        status_text(&window),
        narrate_details::playing_takes(0, 0.0, ui::read_state().session),
        "▶ with no takes says the recording plays with nothing picked after it"
    );
    RAN_TAKE_BAND.store(true, Ordering::SeqCst);
    window.close();
    settle();

    // --- (7) the sample ladder: four refusals, each its own sentence ----------------------------
    let window = narrate_page(app, base_state());
    button(&window, "sample-play").emit_by_name::<()>("clicked", &[]);
    settle();
    assert_eq!(
        status_text(&window),
        narrate_details::pick_a_voice_refusal(),
        "nothing chosen in the picker: pick a voice first"
    );
    let mut captions = base_state();
    captions.voice = CAPTIONS.to_string();
    ui::set_state(captions);
    ui::refresh(&window);
    button(&window, "sample-play").emit_by_name::<()>("clicked", &[]);
    settle();
    assert_eq!(
        status_text(&window),
        narrate_details::no_voice_to_sample_refusal(),
        "the captions voice has no reference and never will"
    );
    let mut voiced = base_state();
    voiced.voice = "narrator1".into();
    ui::set_state(voiced);
    ui::refresh(&window);
    if let Some(entry) = widget_in(&window, "sample-sentence")
        .and_then(|w| w.downcast::<gtk::Entry>().ok())
    {
        entry.set_text("");
    }
    button(&window, "sample-play").emit_by_name::<()>("clicked", &[]);
    settle();
    assert_eq!(
        status_text(&window),
        narrate_details::empty_sample_refusal(),
        "an empty box has nothing to speak"
    );
    if let Some(entry) = widget_in(&window, "sample-sentence")
        .and_then(|w| w.downcast::<gtk::Entry>().ok())
    {
        entry.set_text(narrate_screen::DEFAULT_SAMPLE);
    }
    button(&window, "sample-play").emit_by_name::<()>("clicked", &[]);
    settle();
    assert_eq!(
        status_text(&window),
        narrate_tts::reference_problem(&session_tree()).unwrap(),
        "the ladder passed and the press reached the speak leg, which answered with the one thing the \
         demo session lacks: a voice reference to clone (F4.6). The synthesising sentence is not the \
         final word here, because this press really dials."
    );
    button(&window, "sample-stop").emit_by_name::<()>("clicked", &[]);
    settle();
    assert_eq!(status_text(&window), narrate_details::sample_stopped_status());
    RAN_SAMPLE.store(true, Ordering::SeqCst);
    window.close();
    settle();

    // --- (8) pitch clamps at the slider's end and says who speaks at what -----------------------
    let mut voiced = base_state();
    voiced.voice = "narrator1".into();
    let window = narrate_page(app, voiced);
    let said = ui::press_pitch(&window, -20.0);
    assert!(
        (ui::read_state().pitch - narrate_screen::PITCH_MIN_SEMITONES).abs() < 1e-9,
        "the pitch clamps at -6.0 semitones, never below the speaker"
    );
    assert_eq!(
        said,
        narrate_details::voice_line("narrator1", narrate_screen::PITCH_MIN_SEMITONES),
        "the status says who speaks at what shift"
    );
    assert_eq!(status_text(&window), said, "the real channel carried it");
    RAN_PITCH.store(true, Ordering::SeqCst);
    window.close();
    settle();

    // --- (10) the voice picker shows the published voice's row, not row 0 ----------------------
    // The dropdown is unlabelled and carries the chosen voice as its own face, so "which row is showing"
    // IS the answer to who speaks. `base_state` has no voice set; publishing `narrator1` and refreshing
    // must move the shown row to that option's index rather than leaving it on `captions`.
    let mut voiced = base_state();
    voiced.voice = "narrator1".into();
    let window = narrate_page(app, voiced.clone());
    ui::refresh(&window);
    settle();
    let picker = widget_in(&window, "voice-picker")
        .and_then(|w| w.downcast::<gtk::DropDown>().ok())
        .expect("the Narrate page carries the voice picker");
    let files: Vec<&str> = voiced.voice_files.iter().map(String::as_str).collect();
    let wanted = narrate_screen::voice_options(voiced.narrators, &files)
        .iter()
        .position(|option| option.id == "narrator1")
        .expect("narrator1 is one of the picker's rows");
    assert_eq!(
        picker.selected() as usize,
        wanted,
        "the row shown is the seeded voice's index ({wanted}), not row 0"
    );
    assert_ne!(
        picker.selected(),
        0,
        "row 0 is `captions`; a published narrator left on row 0 means the picker never followed the state"
    );
    RAN_VOICE_PICKER.store(true, Ordering::SeqCst);
    window.close();
    settle();

    // --- (11) back three seconds clamps at zero and forward walks back off it -------------------
    let mut near_start = base_state();
    near_start.session = 1.0;
    near_start.cut_at = 1.0;
    let window = narrate_page(app, near_start);
    button(&window, "narrate-back-3s").emit_by_name::<()>("clicked", &[]);
    settle();
    assert_eq!(
        ui::read_state().session,
        0.0,
        "back past the head stops at 0 rather than going negative"
    );
    button(&window, "narrate-forward-3s").emit_by_name::<()>("clicked", &[]);
    settle();
    assert!(
        (ui::read_state().session - narrate_screen::BACK_SECONDS).abs() < 1e-9,
        "forward from the clamp moves by exactly BACK_SECONDS"
    );
    RAN_BACK_CLAMP.store(true, Ordering::SeqCst);
    window.close();
    settle();

    // --- (12) the LAST seeded row's ▶ and 🗑 fire by name -------------------------------------
    // Rows are rebuilt whole and sorted, so the highest index is the last entry's buttons; firing them
    // proves the wiring tracks the rebuilt list rather than the first build's indices.
    let window = narrate_page(app, base_state());
    button(&window, "line-speak-2").emit_by_name::<()>("clicked", &[]);
    settle();
    assert_eq!(
        status_text(&window),
        ui::press_line_speak(&window, 2),
        "the row's ▶ printed what the seam answers for that index"
    );
    let before = ui::read_state().entries.len();
    button(&window, "line-remove-2").emit_by_name::<()>("clicked", &[]);
    settle();
    assert_eq!(
        ui::read_state().entries.len(),
        before - 1,
        "🗑 on the last row took it out of the list"
    );
    RAN_LAST_ROW.store(true, Ordering::SeqCst);
    window.close();
    settle();

    // --- (13) the pitch slider's debounce writes the clamped value through the widget ---------
    // Block (8) drives `press_pitch` directly; this one moves the real `Scale`, which is the path a drag
    // takes, and waits out PITCH_APPLY_MS so the deferred write is the thing being checked.
    let mut voiced = base_state();
    voiced.voice = "narrator1".into();
    let window = narrate_page(app, voiced);
    let scale = widget_in(&window, "pitch-slider")
        .and_then(|w| w.downcast::<gtk::Scale>().ok())
        .expect("the Narrate page carries the pitch slider");
    scale.set_value(narrate_screen::PITCH_MAX_SEMITONES + 4.0);
    let context = glib::MainContext::default();
    let deadline = std::time::Instant::now()
        + std::time::Duration::from_millis(narrate_screen::PITCH_APPLY_MS + 400);
    while ui::read_state().pitch.abs() < 1e-9 && std::time::Instant::now() < deadline {
        context.iteration(true);
    }
    assert!(
        (ui::read_state().pitch - narrate_screen::PITCH_MAX_SEMITONES).abs() < 1e-9,
        "a value past the top lands clamped at +6.0 after the {} ms debounce",
        narrate_screen::PITCH_APPLY_MS
    );
    RAN_PITCH_DEBOUNCE.store(true, Ordering::SeqCst);
    window.close();
    settle();

    // --- (9) the two readouts -------------------------------------------------------------------
    let window = narrate_page(app, base_state());
    assert_eq!(
        label_text(&window, "narrate-inputs"),
        narrate_screen::inputs_readout(3, 30.0, 0, 0, true, true),
        "the Inputs readout is the module's line for this cut"
    );
    assert_eq!(
        label_text(&window, "narrate-outputs"),
        narrate_screen::outputs_readout(),
        "Outputs names the three things the folder owns, byte for byte"
    );
    let mut no_cut = base_state();
    no_cut.has_cut = false;
    no_cut.clips = 0;
    no_cut.length = 0.0;
    ui::set_state(no_cut);
    ui::refresh(&window);
    assert_eq!(
        label_text(&window, "narrate-inputs"),
        "no cut yet \u{2014} build one on the Cut step",
        "with no cut the page says where to make one"
    );
    RAN_READOUTS.store(true, Ordering::SeqCst);
    window.close();
    settle();

    // --- (10) F4.3: the row's warning read off the WIDGET, not off the fit logic -----------------
    // Four entries on three 10 s clips, chosen so each rung of the render's ladder shows up in a different
    // row. Speech length here is §F4.3's page rule (chars / SPEECH_CHARS_PER_SECOND), so the character
    // counts below ARE the seconds: 38 chars ~ 2.53 s, 240 chars = 16.0 s.
    //   index 0 -> clip 1 (0..10): placed at 1.0, 2.0 s of speech  -> Fits, no warning, unclassed
    //   index 1 -> clip 2 (10..20): placed at 14.0, 2.53 s         -> past room + extend, slid only
    //   index 2 -> clip 3 (20..30): placed at 0.3, 16.0 s          -> past what a slide fixes, sped up
    //   index 3 -> clip 1 again: a CAPTION (pos set)               -> never warns, spoken or not
    let mut fit_state = base_state();
    fit_state.entries = vec![
        Entry {
            s: 0.0,
            e: 10.0,
            at: 1.0,
            text: "a short line that fits".into(), // 20 chars ~ 1.33 s
            emotion: "calm".into(),
            pos: String::new(),
            roll: 0,
        },
        Entry {
            s: 10.0,
            e: 20.0,
            at: 14.0,
            text: "x".repeat(38), // ~2.53 s, placed late on a 10 s clip
            emotion: "calm".into(),
            pos: String::new(),
            roll: 0,
        },
        Entry {
            s: 20.0,
            e: 30.0,
            at: 0.3,
            text: "y".repeat(240), // 16.0 s on a 10 s clip: even MAX_TEMPO cannot carry it
            emotion: "calm".into(),
            pos: String::new(),
            roll: 0,
        },
        Entry {
            s: 0.0,
            e: 10.0,
            at: 5.0,
            text: "z".repeat(240), // long as the tempo case, but the viewer reads it: never spoken
            emotion: String::new(),
            pos: "lower third".into(),
            roll: 0,
        },
    ];
    let window = narrate_page(app, fit_state.clone());

    // The rows are drawn in the page's own order (sorted by start, then `at`), which is NOT the order above,
    // so each row is found by matching its own text box. That match must be EXACT: every field on this page
    // carries a name, and an index whose field is missing would otherwise read as "no match" rather than as a
    // row that failed to draw.
    let row_of = |needle: &Entry| -> usize {
        let matches: Vec<usize> = (0..fit_state.entries.len())
            .filter(|index| {
                widget_in(&window, &format!("line-text-{index}"))
                    .and_then(|w| w.downcast::<gtk::Entry>().ok())
                    // The field holds what `write_box` writes (`[tag] text`), not the raw entry text.
                    .map(|field| field.text() == narrate_screen::write_box(needle))
                    .unwrap_or(false)
            })
            .collect();
        assert_eq!(
            matches.len(),
            1,
            "exactly one row should carry {needle:?}, got {matches:?} — the fixture needs distinct texts"
        );
        matches[0]
    };
    let rows: Vec<usize> = (0..fit_state.entries.len())
        .map(|n| row_of(&fit_state.entries[n]))
        .collect();
    // Each of the four entries landed on its own row, and no two of them share one.
    assert_eq!(rows.len(), 4);
    assert_eq!(rows.iter().collect::<std::collections::HashSet<_>>().len(), 4, "{rows:?}");
    let (short, slid, sped, caption) = (rows[0], rows[1], rows[2], rows[3]);

    // (c) a short line: empty label, no error class.
    let quiet = label_text(&window, &format!("line-warning-{short}"));
    assert_eq!(quiet, "", "a line that fits wears no warning (row {short})");
    let quiet_widget = widget_in(&window, &format!("line-warning-{short}")).expect("warning label exists");
    assert!(!quiet_widget.has_css_class("error"), "a fitting row is not marked red");

    // (a) the slide rung: 'moved earlier' and NOT 'sped up', and red because row_is_red drove it. The
    // expected sentence is rebuilt from this clip's OWN set of lines through the same page rule the wire
    // uses, so the widget and the logic are pinned to one sentence rather than to two estimates.
    let speech_of = |entry: &Entry| -> f64 {
        if entry.text.is_empty() || !entry.pos.is_empty() {
            0.0
        } else {
            narrate_screen::page_speech_seconds(
                entry.text.chars().count(),
                narrate_screen::SPEECH_CHARS_PER_SECOND,
            )
        }
    };
    let slid_lines: Vec<(usize, f64, f64)> = fit_state
        .entries
        .iter()
        .enumerate()
        .filter(|(_, e)| e.s == 10.0 && e.e == 20.0)
        .map(|(index, e)| (index, e.at, speech_of(e)))
        .collect();
    let slid_warning = label_text(&window, &format!("line-warning-{slid}"));
    let slid_expected =
        narrate_screen::fit_warning(narrate_screen::mirror_fit(10.0, &slid_lines))
            .expect("a run past room + extend warns");
    assert_eq!(
        slid_warning, slid_expected,
        "the widget shows the module's own sentence, byte for byte"
    );
    assert!(slid_warning.contains("moved earlier"), "{slid_warning}");
    assert!(
        !slid_warning.contains("sped up"),
        "a slide-only fix must not claim tempo: {slid_warning}"
    );
    assert!(
        widget_in(&window, &format!("line-warning-{slid}"))
            .expect("warning label exists")
            .has_css_class("error"),
        "row_is_red drives the error class on a slid row"
    );

    // (b) the tempo rung: the branch the old per-line estimate could never produce.
    let sped_warning = label_text(&window, &format!("line-warning-{sped}"));
    let sped_speech = narrate_screen::page_speech_seconds(240, narrate_screen::SPEECH_CHARS_PER_SECOND);
    let sped_expected = narrate_screen::fit_warning(narrate_screen::mirror_fit(
        10.0,
        &[(sped, 0.3, sped_speech)],
    ))
    .expect("a run past what a slide fixes warns");
    assert_eq!(sped_warning, sped_expected, "same sentence from widget and logic");
    assert!(
        sped_warning.contains("sped up"),
        "the tempo remedy must reach the row: {sped_warning}"
    );
    assert!(sped_warning.contains("moved earlier and sped up"), "{sped_warning}");
    assert!(
        widget_in(&window, &format!("line-warning-{sped}"))
            .expect("warning label exists")
            .has_css_class("error"),
        "a sped-up row is marked red too"
    );

    // (d) a caption never wears a fit warning, however long it is: the viewer reads it, it is not spoken.
    let caption_warning = label_text(&window, &format!("line-warning-{caption}"));
    assert_eq!(caption_warning, "", "a caption wears no fit warning (row {caption})");
    assert!(
        !widget_in(&window, &format!("line-warning-{caption}"))
            .expect("warning label exists")
            .has_css_class("error"),
        "a caption row is never marked red by the fit rule"
    );
    RAN_FIT_ROW.store(true, Ordering::SeqCst);
    window.close();
    settle();

    // --- (11) F4.4's widget wire is proven elsewhere, over a real socket -------------------------
    // The row button `line-speak-0` now goes to `speak_leg`, which dials the address the Settings
    // name: scripting the reply here would mean no request ever left, so this whole block moved to
    // `tests/speak_tts_wire_widgets.rs`, where a fake audio.cpp on a loopback port answers the
    // same click and the take lands from bytes that really crossed a socket.


    // --- (12) F4.8 through the REAL tick widget: the toggle greys, persists, and leaves the record --
    // Everything above block (1) drove `ui::set_narration_off` directly. This one drives the widget:
    // a `toggled` emission on `narration-tick`, which is the signal `wire()` listens for, so the wire
    // itself is what is under test here rather than the seam behind it.
    let session_root = naivepost::startup::session_dir(&std::env::current_dir().unwrap());
    let tree = naivepost::layout::Tree::new(&session_root).expect("the session folder is a project");
    let mut off_state = base_state();
    off_state.entries = vec![naivepost::narration::Entry {
        s: 0.0,
        e: 10.0,
        at: 2.0,
        text: "a line written before the tick went off".into(),
        ..Default::default()
    }];
    let window = narrate_page(app, off_state.clone());

    // Put a record where the page's project has one, so "left exactly as it is" is a byte comparison
    // against something that exists rather than against an absent file.
    std::fs::create_dir_all(tree.narration_json().parent().unwrap()).expect("narrate/ made");
    naivepost::narration::save(
        &naivepost::narration::Narration {
            entries: off_state.entries.clone(),
            silent: Vec::new(),
        },
        &tree,
    )
    .expect("record written");
    let record_before = std::fs::read(tree.narration_json()).expect("record readable");

    let tick = widget_in(&window, "narration-tick")
        .and_then(|w| w.downcast::<gtk::CheckButton>().ok())
        .expect("the Narration tick is a check button");
    // (a) resting state comes from the project's flag, not from a literal: the fixture is narrated.
    assert_eq!(
        tick.is_active(),
        narrate_off::tick_checked(false),
        "a narrated project opens with the tick CHECKED"
    );

    // (b) fire the real signal. `emit_by_name("toggled")` runs the handler without moving the box, so
    // the widget would end up disagreeing with the state; `set_active` moves the box AND emits the same
    // signal gtk's own click emits, which is why this is the emission a user press is equivalent to.
    tick.set_active(false);
    settle();
    assert!(!tick.is_active(), "the box moved off");
    assert!(
        ui::read_state().narration_off,
        "the handler carried the box's state into the page"
    );

    // (c) the greys match `narrate_off::greyed(true)` field by field, and the tick stayed pressable.
    let greyed = narrate_off::greyed(true);
    assert!(greyed.lines && greyed.preview && greyed.voice, "all three go grey");
    assert!(!greyed.tick, "the tick never greys -- it is the way back");
    for name in ["narrate-lines", "narrate-preview", "voice-picker"] {
        assert!(
            !widget_in(&window, name).unwrap().is_sensitive(),
            "{name} is insensitive with narration off, as greyed says"
        );
    }
    assert!(tick.is_sensitive(), "the tick stays live while everything else greys");

    // (d) the status line the wire printed is the seam's own sentence, unchanged by going through GTK.
    assert_eq!(
        status_text(&window),
        format!(
            "narration off \u{2014} {} greyed",
            narrate_screen::off_greys().join(", ")
        ),
        "the widget's status equals the seam's wording"
    );

    // (e) F4.8 node 2: the record was not opened. Bytes identical, and the module says so too.
    assert!(!narrate_off::touches_the_record(), "the tick must never open the record");
    assert_eq!(
        std::fs::read(tree.narration_json()).expect("still readable"),
        record_before,
        "narration.json left exactly as it was by the toggle"
    );
    // ...while the flag itself DID land in the project file, which is what makes the tick survive.
    let saved = std::fs::read_to_string(session_root.join("naivepost.json"))
        .or_else(|_| std::fs::read_to_string(tree.dir().join("naivepost.json")))
        .expect("the project file is readable");
    assert!(
        saved.contains("\"no_narration\": true"),
        "the tick wrote its flag: {saved}"
    );

    // (f) the chain's skip sentence is the owning module's, pinned against lucky's const.
    assert_eq!(
        narrate_off::skips(naivepost::run::Step::Narrate, true),
        Some(naivepost::lucky::NARRATE_SKIPPED.to_string()),
        "the skip line the lucky run logs is narrate_off's, not a copy"
    );

    // (g) S5: back on through the same widget, the surfaces return and the record still matches.
    tick.set_active(true);
    settle();
    assert!(tick.is_active(), "the box came back on");
    assert!(!ui::read_state().narration_off, "and the page came with it");
    for name in ["narrate-lines", "narrate-preview", "voice-picker"] {
        assert!(
            widget_in(&window, name).unwrap().is_sensitive(),
            "{name} is live again with the narration"
        );
    }
    assert_eq!(status_text(&window), "narration on", "and says so plainly");
    assert_eq!(
        std::fs::read(tree.narration_json()).expect("still readable"),
        record_before,
        "everything written is still there after the round trip"
    );
    assert_eq!(
        naivepost::narration::load(&tree).unwrap().entries,
        off_state.entries,
        "the lines the tick passed over are the lines that were there"
    );

    RAN_OFF_WIRE.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

#[test]
fn sec_07_narrate_1_surface_the_narrate_page_reaches_its_rules() {
    // Pin cwd to our own temp root BEFORE anything builds: the press resolves the project through
    // `startup::session_dir(current_dir())`, and leaving it at rust/ writes a stray
    // `rust/session.naivepost/` into the repo.
    let root = std::env::temp_dir().join(format!("np-narrate-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temp root created");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");

    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(narrate_round);
        app.run();
    });

    assert!(RAN_TICK.load(Ordering::SeqCst), "the tick block never ran");
    assert!(RAN_TRANSPORT.load(Ordering::SeqCst), "the transport block never ran");
    assert!(RAN_ADD_LINE.load(Ordering::SeqCst), "the add-at-playhead block never ran");
    assert!(RAN_SPEAK.load(Ordering::SeqCst), "the row's \u{25b6} block never ran");
    assert!(
        RAN_ROW_BUTTONS.load(Ordering::SeqCst),
        "the row's reroll / add-below / remove block never ran"
    );
    assert!(RAN_TAKE_BAND.load(Ordering::SeqCst), "the take band block never ran");
    assert!(RAN_SAMPLE.load(Ordering::SeqCst), "the sample ladder never ran");
    assert!(RAN_PITCH.load(Ordering::SeqCst), "the pitch clamp never ran");
    assert!(RAN_READOUTS.load(Ordering::SeqCst), "the readouts never ran");
    assert!(
        RAN_VOICE_PICKER.load(Ordering::SeqCst),
        "the voice picker block never ran"
    );
    assert!(
        RAN_BACK_CLAMP.load(Ordering::SeqCst),
        "the back-clamp block never ran"
    );
    assert!(
        RAN_LAST_ROW.load(Ordering::SeqCst),
        "the last row's speak/remove block never ran"
    );
    assert!(
        RAN_PITCH_DEBOUNCE.load(Ordering::SeqCst),
        "the pitch slider's debounce path never ran"
    );
    assert!(RAN_FIT_ROW.load(Ordering::SeqCst), "the F4.3 fit-row block never ran");
    assert!(
        RAN_OFF_WIRE.load(Ordering::SeqCst),
        "the F4.8 real-tick block never ran"
    );
}
