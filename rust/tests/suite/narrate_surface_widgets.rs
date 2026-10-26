//! §07-narrate#1-screen — the WIRE: a click on the Narrate page reaches the rules in
//! `naivepost::narrate_screen` / `narrate_details`, and what they answer lands on the status line and in
//! the log instead of staying inside a callback.
//!
//! `rust/tests/suite/narrate_screen_widgets.rs` proves the rules; this file proves the page can be reached at
//! all — before this round `page_box` drew only a title for the Narrate tab, so every rule was tested and
//! no click arrived at any of them.
//!
//! One application, one `connect_activate`, exactly one `#[test]`: these checks mutate process-shared
//! state (this window's widgets and the page's published state), so they run once and report through the
//! flags asserted at the end — the shape tests/suite/clamp_effects_widget.rs uses.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut::Seg;
use naivepost::narrate_details;
use naivepost::narrate_screen::{self, CAPTIONS};
use naivepost::narration::Entry;
use naivepost::shell::Page;
use naivepost::ui;

use naivepost::narrate_tts;

/// The session folder as the press sees it: `narrate_session_tree` resolves `startup::session_dir` against
/// the process's cwd, which this test pins to its own temp root — so the readout is the same folder the
/// press wrote into, not a path built from a remembered name.
fn session_tree() -> naivepost::layout::Tree {
    let dir = naivepost::startup::session_dir(&std::env::current_dir().unwrap());
    naivepost::layout::Tree::new(&dir).expect("the session folder ends in .naivepost")
}

/// A take that exists on disk is played by `narrate_sample::play`, which spawns the player program
/// `narrate_preview_leg::voice_program` resolves — Settings' `VOICE_PLAYER`, then `NAIVEPOST_VOICE_PLAYER`,
/// then `gst-play-1.0`. This container has no sound hardware and that name may resolve to nothing, so the
/// press is pointed at `true`: a child that exits at once. The env var is read per spawn, so nothing leaks
/// into a settings file and the program still takes its real path through `play`.
fn point_the_player_nothing() {
    std::env::set_var("NAIVEPOST_VOICE_PLAYER", "/usr/bin/true");
}

/// One audio.cpp on a free loopback port that can do every step of the flowchart: `/health` answers 200,
/// `/v1/models` lists one cloneable model (`checks::tts_endpoint_verdict` wants `task: "clon"` or the
/// `index_tts2` family), `/v1/ui/upload` answers a server path, and `/v1/audio/speech` answers a 200 big
/// enough to be a wav (`narrate_tts::MIN_WAV_BYTES`). The address IS the seam (`speak_leg`'s own note):
/// no program code knows the difference between this and the real server.
///
/// It is held for the whole run because the port must not be recycled: released, another test binary can
/// take it and this press then dials an ANSWERER when the scenario meant a refusal, or vice versa — the
/// shape this check flaked in.
struct FakeAudio {
    url: String,
}

impl FakeAudio {
    fn start() -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a free loopback port");
        let url = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        let want_upload = url.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                use std::io::{Read, Write as _};
                let mut request = Vec::new();
                let mut byte = [0u8; 1];
                let mut content_length = 0usize;
                let mut header_end = None;
                while stream.read(&mut byte).unwrap_or(0) == 1 {
                    request.push(byte[0]);
                    if header_end.is_none() && request.ends_with(b"\r\n\r\n") {
                        let head = String::from_utf8_lossy(&request).to_lowercase();
                        content_length = head
                            .lines()
                            .find_map(|line| line.strip_prefix("content-length:"))
                            .and_then(|value| value.trim().parse::<usize>().ok())
                            .unwrap_or(0);
                        header_end = Some(request.len());
                    }
                    if let Some(end) = header_end {
                        if request.len() >= end + content_length {
                            break;
                        }
                    }
                }
                let text = String::from_utf8_lossy(&request).to_lowercase();
                // Only the request line: a body carrying the word "health" must not read as a health ask.
                let line = text.lines().next().unwrap_or("").to_string();
                let body: Vec<u8> = if line.contains("audio/speech") {
                    vec![0x52_u8; 4096]
                } else if line.contains("upload") {
                    b"/uploaded/voice_ref.wav".to_vec()
                } else if line.contains("v1/models") {
                    br#"{"data":[{"id":"index-tts2","task":"clon","family":"index_tts2"}]}"#
                        .to_vec()
                } else {
                    Vec::new()
                };
                let note = if line.contains("audio/speech") && !text.contains(&want_upload) {
                    "x-fake-note: address is not the fake's\r\n"
                } else {
                    ""
                };
                let reply = format!(
                    "HTTP/1.1 200 OK\r\n{note}content-length: {}\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(reply.as_bytes());
                let _ = stream.write_all(&body);
                let _ = stream.flush();
            }
        });
        Self { url }
    }

    /// Settings wins over the environment for audio.cpp (`services::audio_endpoint`), and `speak_leg`
    /// re-reads that file on EVERY call, so the next press dials here.
    fn point(&self) {
        let paths = naivepost::settings::from_environment().expect("the settings folder");
        let mut conf = naivepost::settings::read(&paths).expect("the settings read back");
        conf.audio_server = self.url.clone();
        naivepost::settings::save(&paths, &conf).expect("the audio address written");
    }
}

use crate::common;
use common::{
    fixture_dir, hold_last_window, narrate_button as button, release_last_window, settle,
    status_text, widget_in,
};

static RAN_NO_SWITCH: AtomicBool = AtomicBool::new(false);
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

/// Let the main context run what the widget emissions queued.

fn label_text(window: &adw::ApplicationWindow, name: &str) -> String {
    widget_in(window, name)
        .and_then(|w| w.downcast::<gtk::Label>().ok())
        .map(|l| l.text().to_string())
        .unwrap_or_else(|| panic!("the Narrate page carries a label named {name}"))
}

/// Three kept clips of ten seconds each, so every second the page writes down is checkable by hand.
fn three_clips() -> Vec<Seg> {
    vec![
        Seg {
            s: 0.0,
            e: 10.0,
            cam: 0,
            ..Default::default()
        },
        Seg {
            s: 10.0,
            e: 20.0,
            cam: 0,
            ..Default::default()
        },
        Seg {
            s: 20.0,
            e: 30.0,
            cam: 0,
            ..Default::default()
        },
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
        started: false,
        busy: false,
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
    // The bottom bar's readouts are read on a worker (§03-shell §1): let them land before a block reads them.
    crate::common::settle_workers();
    // The surface itself: without these names the tab is a bare title and nothing below can fire.
    for name in [
        "narrate-game-audio",
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
    // --- (1) no narration switch: no tick, the game audio slider shown, the page live ----------------
    // §07 §1: the page is always live, and a video is narrated when its narration has lines (first rewrite: a
    // Narration tick here greyed the lines, the preview and the voice picker, and hid the slider).
    let window = narrate_page(app, base_state());
    assert!(
        widget_in(&window, "narration-tick").is_none(),
        "the Narrate page has no Narration tick"
    );
    let game_row = widget_in(&window, "narrate-game-row").expect("the game audio row is drawn");
    assert!(
        game_row.is_visible() && game_row.is_sensitive(),
        "the Game audio slider is shown and live"
    );
    assert!(
        widget_in(&window, "narrate-game-audio").is_some(),
        "with its slider"
    );
    for name in ["narrate-lines", "narrate-preview", "voice-picker"] {
        assert!(
            widget_in(&window, name).unwrap().is_sensitive(),
            "{name} is live: nothing greys for a narration switch"
        );
    }
    RAN_NO_SWITCH.store(true, Ordering::SeqCst);
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
    assert_eq!(
        ui::read_state().entries.len(),
        3,
        "a refused add adds nothing"
    );
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
        blank_said, "clip 3 has no line \u{2014} playing it on its own audio",
        "the wordless row says what it is"
    );
    button(&window, "line-speak-1").emit_by_name::<()>("clicked", &[]);
    settle();
    let caption_said = status_text(&window);
    assert_eq!(
        caption_said, "line 2 is a caption \u{2014} read, never spoken; playing its moment",
        "the caption row says what it is"
    );
    button(&window, "line-speak-0").emit_by_name::<()>("clicked", &[]);
    assert!(
        naivepost::ui::background::settle_for_test(std::time::Duration::from_secs(120)),
        "the press's worker answered"
    );
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
    assert!(
        naivepost::ui::background::settle_for_test(std::time::Duration::from_secs(120)),
        "the press's worker answered"
    );
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
    // A span that clears `P.eng.takeMinSeconds` is accepted, and the sentence it says is §F4.6's own:
    // the seconds just dragged, then §6's commit status judged against `P.machine.refWantSeconds` — the
    // mark a reference is worth cloning at. `take_min()` there read "(0 s is plenty)" beside 2.6 s.
    let added = ui::press_take_add(&window, (0.0, 2.6));
    assert_eq!(
        added,
        format!(
            "take added: 00:00\u{2013}00:02 \u{2014} {}",
            narrate_details::commit_status(
                "takes",
                1,
                2.6,
                narrate_details::REFERENCE_WANTED_SECONDS
            )
        ),
        "// \u{a7}F4.6's caption: 'take added: 0:00\u{2013}0:03 \u{2014} 1 take(s), 2.6 s of reference (14 s is plenty)', to the minute `mm_ss` floors to"
    );
    assert_eq!(status_text(&window), added, "the status line says it too");
    // --- the take band's ▶ and the empty case, measured while NO take is on the band ------------------
    // (the assertions below expect `takes.is_empty()`), so the take this arm added comes back off here.
    ui::press_take_remove(&window, (0.0, 2.6));
    settle();
    assert!(
        ui::read_state().takes.is_empty(),
        "\u{ff}－ took the seconds back out"
    );
    // and the − sentence carries the same 14 s mark
    ui::press_take_add(&window, (5.0, 8.0));
    settle();
    let minus = ui::press_take_remove(&window, (6.0, 7.0));
    assert!(
        minus.contains("(14 s is plenty)"),
        "// \u{a7}6's commit status is judged against refWantSeconds: {minus}"
    );
    // \u{ff} take everything back off: the \u{25b6} assertion below is the NO-takes sentence, and a band
    // left holding a take would answer with the takes-playing one instead.
    ui::press_take_remove(&window, (5.0, 8.0));
    settle();
    assert!(ui::read_state().takes.is_empty(), "the band is empty again");
    // ▶ plays the voice's RECORDING from the red bar (§1 **8**), so the voice needs one on disk: the fixture
    // tags narrator 1, and its file is planted where that row points.
    let tree =
        naivepost::ui::window::narrate_session_tree().expect("the page resolves its project");
    let project = naivepost::project::load(tree.dir()).expect("the project loads");
    let recording = naivepost::narrate_band_face::recording_path(&tree, &project, "own")
        .expect("the fixture tags narrator 1");
    std::fs::create_dir_all(recording.parent().expect("a folder")).expect("its folder made");
    std::fs::write(&recording, b"stub").expect("the recording written");
    let mut voiced = ui::read_state();
    voiced.voice = "own".into();
    voiced.narrators = 1;
    ui::set_state(voiced);
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
    if let Some(entry) =
        widget_in(&window, "sample-sentence").and_then(|w| w.downcast::<gtk::Entry>().ok())
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
    if let Some(entry) =
        widget_in(&window, "sample-sentence").and_then(|w| w.downcast::<gtk::Entry>().ok())
    {
        entry.set_text(narrate_screen::DEFAULT_SAMPLE);
    }
    // A fake audio.cpp held for the whole run, so the press below that really dials answers as the flowchart
    // says rather than meeting whatever happens to own the loopback port under a full-suite run: here the
    // reference is missing, so S1 refuses BEFORE any dial (F4.4 S1) -- the fake's job is to make that
    // ordering observable, since a live server answering /health would otherwise put two requests on the wire.
    let audio = FakeAudio::start();
    audio.point();
    point_the_player_nothing();
    if let Some(entry) =
        widget_in(&window, "sample-sentence").and_then(|w| w.downcast::<gtk::Entry>().ok())
    {
        entry.set_text(narrate_screen::DEFAULT_SAMPLE);
    }
    button(&window, "sample-play").emit_by_name::<()>("clicked", &[]);
    // The synthesis is a worker's (§03-shell §1): the press says it is synthesizing, the answer follows.
    assert_eq!(
        status_text(&window),
        narrate_details::sample_status(),
        "the press returns at once, saying the sample is being made"
    );
    assert!(
        naivepost::ui::background::settle_for_test(std::time::Duration::from_secs(120)),
        "the sample's worker answered"
    );
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
    assert_eq!(
        status_text(&window),
        narrate_details::sample_stopped_status()
    );
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
        .position(|option| option.id == "own")
        .expect("narrator 1 (`own`, also stored as `narrator1`) is one of the picker's rows");
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
    common::wait_until("the pitch slider's debounced write", || {
        ui::read_state().pitch.abs() >= 1e-9
    });
    assert!(
        (ui::read_state().pitch - narrate_screen::PITCH_MAX_SEMITONES).abs() < 1e-9,
        "a value past the top lands clamped at +6.0 after the {} ms debounce",
        narrate_screen::PITCH_APPLY_MS
    );
    RAN_PITCH_DEBOUNCE.store(true, Ordering::SeqCst);
    window.close();
    settle();

    // --- (9) the two readouts, on the window's BOTTOM BAR (§A.8), not in this page -----------------
    // §A.8 heads its Inputs/Outputs block "Bottom bar", and §3 §1 puts that row on every tab, so the
    // widgets checked here are the shell's (`inputs-readout`, `outputs-readout`), painted by
    // `window_bench::paint_readouts` from `Shell::inputs` / `Shell::outputs`. The page draws no copy: the
    // names it used to carry are gone, which is the second half of what this pins.
    let window = narrate_page(app, base_state());
    for gone in [
        "narrate-inputs",
        "narrate-outputs",
        "narrate-outputs-folder",
    ] {
        assert!(
            widget_in(&window, gone).is_none(),
            "badge **22** lives on the bottom bar only; the page still draws a `{gone}`"
        );
    }
    let tree = naivepost::layout::Tree::new(fixture_dir()).expect("the fixture is a project");
    let narrate_shell = naivepost::shell::Shell {
        page: Page::Narrate,
        ..Default::default()
    };
    assert_eq!(
        label_text(&window, "outputs-readout"),
        narrate_screen::outputs_readout(),
        "Outputs names the three things the folder owns, byte for byte"
    );
    assert_eq!(
        narrate_shell.outputs(Some(&tree)),
        narrate_screen::outputs_readout(),
        "and it comes from `Shell::outputs`, the one read the bar is painted from"
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
                    .and_then(|w| w.downcast::<gtk::TextView>().ok())
                    // The field holds what `write_box` writes (`[tag] text`), not the raw entry text.
                    .map(|view| {
                        naivepost::ui::narrate_row::box_text(&view)
                            == narrate_screen::write_box(needle)
                    })
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
    assert_eq!(
        rows.iter().collect::<std::collections::HashSet<_>>().len(),
        4,
        "{rows:?}"
    );
    let (short, slid, sped, caption) = (rows[0], rows[1], rows[2], rows[3]);

    // (c) a short line: empty label, no error class.
    let quiet = label_text(&window, &format!("line-warning-{short}"));
    assert_eq!(quiet, "", "a line that fits wears no warning (row {short})");
    let quiet_widget =
        widget_in(&window, &format!("line-warning-{short}")).expect("warning label exists");
    assert!(
        !quiet_widget.has_css_class("error"),
        "a fitting row is not marked red"
    );
    // A.5 marks the ROW red, not only its warning: the row that fits wears no `error` class either.
    let quiet_row = widget_in(&window, &format!("narrate-line-{short}")).expect("row exists");
    assert!(
        !quiet_row.has_css_class("error"),
        "a fitting row is not marked red"
    );

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
    let slid_expected = narrate_screen::fit_warning(narrate_screen::mirror_fit(10.0, &slid_lines))
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
    // A.5: it is THE ROW that is marked red, so the row itself carries the class its warning wears.
    assert!(
        widget_in(&window, &format!("narrate-line-{slid}"))
            .expect("row exists")
            .has_css_class("error"),
        "a warned row is marked red, not only its warning label"
    );

    // (b) the tempo rung: the branch the old per-line estimate could never produce.
    let sped_warning = label_text(&window, &format!("line-warning-{sped}"));
    let sped_speech =
        narrate_screen::page_speech_seconds(240, narrate_screen::SPEECH_CHARS_PER_SECOND);
    let sped_expected = narrate_screen::fit_warning(narrate_screen::mirror_fit(
        10.0,
        &[(sped, 0.3, sped_speech)],
    ))
    .expect("a run past what a slide fixes warns");
    assert_eq!(
        sped_warning, sped_expected,
        "same sentence from widget and logic"
    );
    assert!(
        sped_warning.contains("sped up"),
        "the tempo remedy must reach the row: {sped_warning}"
    );
    assert!(
        sped_warning.contains("moved earlier and sped up"),
        "{sped_warning}"
    );
    assert!(
        widget_in(&window, &format!("line-warning-{sped}"))
            .expect("warning label exists")
            .has_css_class("error"),
        "a sped-up row is marked red too"
    );

    // (d) a caption never wears a fit warning, however long it is: the viewer reads it, it is not spoken.
    let caption_warning = label_text(&window, &format!("line-warning-{caption}"));
    assert_eq!(
        caption_warning, "",
        "a caption wears no fit warning (row {caption})"
    );
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
    // `tests/suite/speak_tts_wire_widgets.rs`, where a fake audio.cpp on a loopback port answers the
    // same click and the take lands from bytes that really crossed a socket.
}

#[test]
fn sec_07_narrate_1_surface_the_narrate_page_reaches_its_rules() {
    // Pin cwd to a folder that no OTHER test binary shares (the pid alone is shared by every binary
    // of one `cargo test` run, and a sibling that saves the project under a new name moves this
    // folder out from under the tick's write -- which is exactly F0.10 S3).
    let root = std::env::temp_dir().join(format!(
        "np-narrate-{}-{}",
        std::process::id(),
        std::thread::current()
            .name()
            .unwrap_or("round")
            .replace(':', "_")
    ));
    std::fs::remove_dir_all(&root).ok();
    std::fs::create_dir_all(&root).expect("temp root created");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");

    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(narrate_round);
        app.run_with_args::<&str>(&[]);
    });

    assert!(
        RAN_NO_SWITCH.load(Ordering::SeqCst),
        "the no-narration-switch block never ran"
    );
    assert!(
        RAN_TRANSPORT.load(Ordering::SeqCst),
        "the transport block never ran"
    );
    assert!(
        RAN_ADD_LINE.load(Ordering::SeqCst),
        "the add-at-playhead block never ran"
    );
    assert!(
        RAN_SPEAK.load(Ordering::SeqCst),
        "the row's \u{25b6} block never ran"
    );
    assert!(
        RAN_ROW_BUTTONS.load(Ordering::SeqCst),
        "the row's reroll / add-below / remove block never ran"
    );
    assert!(
        RAN_TAKE_BAND.load(Ordering::SeqCst),
        "the take band block never ran"
    );
    assert!(
        RAN_SAMPLE.load(Ordering::SeqCst),
        "the sample ladder never ran"
    );
    assert!(
        RAN_PITCH.load(Ordering::SeqCst),
        "the pitch clamp never ran"
    );
    assert!(
        RAN_READOUTS.load(Ordering::SeqCst),
        "the readouts never ran"
    );
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
    assert!(
        RAN_FIT_ROW.load(Ordering::SeqCst),
        "the F4.3 fit-row block never ran"
    );
}
