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
use naivepost::narrate_screen::{self, CAPTIONS};
use naivepost::narration::Entry;
use naivepost::shell::Page;
use naivepost::ui;

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

fn fixture_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/demo.naivepost")
}

thread_local! {
    /// Strong handle on the window last built, so the next check drops it first: a closed GTK window is not
    /// destroyed and its names stay parented, which would send a lookup to the wrong tree.
    static LAST_WINDOW: std::cell::RefCell<Option<adw::ApplicationWindow>> =
        const { std::cell::RefCell::new(None) };
}

fn release_last_window() {
    LAST_WINDOW.with(|cell| {
        if let Some(old) = cell.borrow_mut().take() {
            old.close();
        }
    });
    settle();
}

/// Let the main context run what the widget emissions queued.
fn settle() {
    let context = glib::MainContext::default();
    for _ in 0..64 {
        if !context.iteration(false) {
            break;
        }
    }
}

fn status_text(window: &adw::ApplicationWindow) -> String {
    ui::find_status(window.upcast_ref())
        .expect("the shell has a status line")
        .text()
        .to_string()
}

fn widget_in(window: &adw::ApplicationWindow, name: &str) -> Option<gtk::Widget> {
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
    walk(window.upcast_ref(), name)
}

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
    LAST_WINDOW.with(|cell| *cell.borrow_mut() = Some(window.clone()));
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
    assert_eq!(
        spoken_said,
        narrate_screen::speaking_line(),
        "a writable line reports that it is being synthesized"
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
    assert_eq!(
        status_text(&window),
        narrate_details::new_take_status(0),
        "↻ on a written line says a new take is coming"
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
        narrate_details::sample_status(),
        "with a voice and words the sample is being synthesized"
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
}
