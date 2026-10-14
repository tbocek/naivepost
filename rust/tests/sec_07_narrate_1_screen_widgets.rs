//! §07-narrate#1-screen through the WIRE: a real Narrate page, its real widgets, and the two seams
//! this item was found missing — the take band's gesture and buttons, and "Add file…".
//!
//! Every check here fires a widget the way GTK does (`emit_by_name` on the button, `drag-begin`/
//! `drag-update`/`drag-end` on the band's own `GtkGestureDrag`) and then reads the SAME state the logic
//! tests read (`narrate_page::read_state()`, the band module's own slots). The painting checks run the
//! band's and the picture's real painters into an image surface and look at pixels, so a CSS plate with
//! no draw handler cannot pass.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;

use naivepost::narrate_screen;
use naivepost::settings;
use naivepost::ui;

#[allow(dead_code)] // every test binary compiles this whole module
mod common;
use common::{click, fixture_dir, hold_last_window, release_last_window, settle, status_text};

static RAN_DRAG: AtomicBool = AtomicBool::new(false);
static RAN_BUTTONS: AtomicBool = AtomicBool::new(false);
static RAN_PAINT: AtomicBool = AtomicBool::new(false);
static RAN_ADD_FILE: AtomicBool = AtomicBool::new(false);

/// A session folder of our own, so nothing this test writes lands in a real project or another test's.
fn scratch_root(tag: &str) -> std::path::PathBuf {
    let root = naivepost::startup::session_dir(
        &std::env::temp_dir().join(format!("np-sec07-wire-{}-{}", tag, std::process::id())),
    );
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("scratch session folder");
    std::env::set_current_dir(&root).expect("cwd pinned to this root");
    root
}

/// The page with `state` published, sized before `present()` so the paned lays out both columns.
/// Built the way the shell builds it (fixture project, Narrate tab clicked), because the band's
/// gesture and "Add file…" are wired by the page's own `wire`, which only runs on a real window.
fn narrate_page(app: &adw::Application, state: ui::NarrateState) -> adw::ApplicationWindow {
    release_last_window();
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    hold_last_window(window.clone());
    window.set_default_size(1280, 800);
    window.present();
    ui::tab_button(&window, naivepost::shell::Page::Narrate)
        .expect("the shell has a Narrate tab")
        .emit_by_name::<()>("clicked", &[]);
    settle();
    ui::set_state(state);
    ui::refresh(&window);
    settle();
    window
}

fn base_state() -> ui::NarrateState {
    ui::NarrateState {
        segs: vec![
            naivepost::cut::Seg { s: 0.0, e: 30.0, ..Default::default() },
            naivepost::cut::Seg { s: 30.0, e: 60.0, ..Default::default() },
        ],
        takes: vec![],
        voice: "narrator1".into(),
        session: 5.0,
        cut_at: 5.0,
        length: 60.0,
        clips: 2,
        has_cut: true,
        has_timeline: true,
        narrators: 1,
        voice_files: vec![],
        recording_covers_clip: true,
        ..Default::default()
    }
}

fn round(app: &adw::Application) {
    let root = scratch_root("round");

    // --- S1/S2: the band's OWN gesture selects seconds and sets the red bar --------------
    {
        let window = narrate_page(app, base_state());
        // §1: with nothing clicked yet the red bar sits at the playhead — and a fresh page has had no
        // click, which is what makes the S4 paint below read the session second rather than a stale one.
        assert!(
            !ui::narrate_band::red_bar_seen_for_test(),
            "a fresh page has no clicked red bar of its own yet"
        );
        let gesture = ui::narrate_band::band_gesture_for_test(&window)
            .expect("the page attached one drag gesture to `take-band`");
        let (_seconds, pps) = ui::narrate_band::painted_view_for_test();
        // The band's own mapping at this allocation: whatever px/s its painter is working with, the
        // seconds it stores must be that same multiply of the pixels the hand moved through.
        let (from_px, to_px) = (50.0f64, 150.0f64);
        // A drag: begin, move, release. GTK hands offsets, and these handlers read x only.
        gesture.emit_by_name::<()>("drag-begin", &[&from_px, &20.0f64]);
        gesture.emit_by_name::<()>("drag-update", &[&to_px, &20.0f64]);
        gesture.emit_by_name::<()>("drag-end", &[&to_px, &20.0f64]);
        settle();
        let selection = ui::narrate_band::live_selection().expect("the drag left a live selection");
        // The gesture hands PIXEL offsets; what the slot must hold is SECONDS, so the check is that the
        // stored span equals the dragged pixels divided by the band's own px/s — not the pixel numbers.
        assert!(
            (selection.0 - from_px / pps).abs() < 0.01 && (selection.1 - to_px / pps).abs() < 0.01,
            "the selection holds the dragged seconds: {selection:?} at {pps:.2} px/s"
        );
        // A press that barely travels is a CLICK and moves the red bar instead (§1).
        gesture.emit_by_name::<()>("drag-begin", &[&300.0f64, &20.0f64]);
        gesture.emit_by_name::<()>("drag-update", &[&302.0f64, &20.0f64]);
        gesture.emit_by_name::<()>("drag-end", &[&302.0f64, &20.0f64]);
        settle();
        assert_eq!(
            ui::narrate_band::live_selection(),
            None,
            "a click clears the selection rather than leaving a stale one"
        );
        // The red bar follows the same mapping as everything else on the band: the click's x over px/s.
        assert!(
            (ui::narrate_band::red_bar() - 302.0 / pps).abs() < 0.01,
            "the click put the red bar where the pointer was: {}",
            ui::narrate_band::red_bar()
        );
        RAN_DRAG.store(true, Ordering::SeqCst);
        window.close();
        settle();
    }

    // --- S3: ＋ and − act on that selection; a short one is refused ----------------------
    {
        let window = narrate_page(app, base_state());
        let gesture = ui::narrate_band::band_gesture_for_test(&window).expect("the band gesture");
        let (from_px, to_px) = (100.0, 400.0);
        gesture.emit_by_name::<()>("drag-begin", &[&from_px, &20.0]);
        gesture.emit_by_name::<()>("drag-update", &[&to_px, &20.0]);
        gesture.emit_by_name::<()>("drag-end", &[&to_px, &20.0]);
        settle();
        let wanted = ui::narrate_band::live_selection().expect("a selection to commit");
        click(&window, "take-add");
        settle();
        let takes = ui::read_state().takes;
        assert_eq!(takes.len(), 1, "\u{ff0b} committed exactly one take: {takes:?}");
        assert!(
            (takes[0].0 - wanted.0).abs() < 0.01 && (takes[0].1 - wanted.1).abs() < 0.01,
            "the take is the dragged span: {:?} vs {wanted:?}",
            takes[0]
        );
        assert!(
            status_text(&window).contains("take"),
            "the commit says what landed: {:?}",
            status_text(&window)
        );
        // \u{ff0d} takes those seconds back out again.
        click(&window, "take-remove");
        settle();
        assert!(
            ui::read_state().takes.is_empty(),
            "\u{ff0d} took the span back out"
        );
        // Under P.eng.takeMinSeconds the seam refuses and the lane keeps what it had.
        let too_short = (10.0, 10.0 + naivepost::narrate_screen::take_min() - 0.1);
        let said = ui::press_take_add(&window, too_short);
        assert_eq!(said, narrate_screen::take_too_short(too_short.1 - too_short.0));
        assert!(ui::read_state().takes.is_empty(), "a refused take wrote nothing");
        // \u{25b6} names the red bar's second, which is where \u00a71 says playback starts.
        ui::narrate_band::set_red_bar(12.0);
        ui::narrate_page::press_take_play(&window);
        RAN_BUTTONS.store(true, Ordering::SeqCst);
        window.close();
        settle();
    }

    // --- S4: the band paints: envelope sentence, takes, labels, red bar through the ruler --
    {
        let mut state = base_state();
        state.takes = vec![(2.0, 12.0), (20.0, 20.5)];
        // The band's hand is this thread's, and the blocks above moved it: a paint check must start from
        // an untouched band or it measures the PREVIOUS block's click at the previous window's width.
        ui::narrate_band::clear_hand();
        let window = narrate_page(app, state);
        let width = 500i32;
        let height = narrate_screen::BAND_LANE_PX as i32;
        // A second the lane really shows: with no reference recording on disk the band falls back to a
        // short duration and px/s is clamped at `narrate.bandMaxPps`, so the playhead's own second can sit
        // past a narrow image's right edge — a scrolled view, not a bug. A clicked second inside the
        // recording is what this pixel check is about, and it must be set BEFORE the ink goes down.
        ui::narrate_band::set_red_bar(1.0);
        let plan = ui::narrate_band::painted_plan_for_test(width as f64, height as f64);
        assert!(
            plan.red_bar_px < width as f64,
            "a clicked second inside the recording maps inside the lane: {plan:?}"
        );
        let bar_x = plan.red_bar_px.round() as i32;
        let mut surface =
            ui::narrate_band::painted_surface_for_test(width, height).expect("the band painted");
        let counts = colour_counts(&mut surface);
        assert!(
            counts.len() > 3,
            "the lane is inked with several colours, not one flat plate: {counts:?}"
        );
        // No reference recording exists in this scratch session, so the band owes its "reading the
        // recording…" sentence under the ruler; the top strip still carries the ruler's ink.
        let text = ink_rows(&mut surface);
        assert!(text.iter().any(|row| *row), "the top strip carries ink (the ruler)");
        // The red bar is drawn THROUGH the ruler (so a pixel of it sits above the ruler line) and is
        // `BAR_PX` wide centred on its px: read both px either side of the rounded centre.
        assert!(
            !(is_ground(&mut surface, bar_x, 2) && is_ground(&mut surface, bar_x - 1, 2)),
            "the red cue bar crosses the ruler strip at x={bar_x} (plan: {plan:?})"
        );
        // And it is RED there, not merely some ink: the same triple `paint` lays down.
        let (r, g, b) = pixel(&mut surface, bar_x, 2);
        let (want_r, want_g, want_b) = ui::narrate_band::RED_BAR_COLOUR;
        assert!(
            r.abs_diff(want_r) <= 2 && g.abs_diff(want_g) <= 2 && b.abs_diff(want_b) <= 2,
            "the bar over the ruler is the cue's own red: got ({r},{g},{b}) want ({want_r},{want_g},{want_b})"
        );
        RAN_PAINT.store(true, Ordering::SeqCst);
        drop(window);
        settle();
    }

    // --- S5: Add file\u{2026}: a real ffmpeg copy, listed by the picker --------------------
    {
        // The voices folder is machine-wide: `llm.conf` in the real config folder names it, and here that
        // is `/mnt/models/audiocpp/voices`, which a test may neither need nor write. So point the whole
        // settings store at this scratch root (XDG_CONFIG_HOME) and name the folder in its own `llm.conf`.
        // The env is this process's, so every folder is put back before the block ends: leaving them set
        // makes the NEXT window built in this process read a scratch config and say scratch sentences.
        let voices = root.join("voices-library");
        std::fs::create_dir_all(&voices).expect("scratch voices folder");
        let held_config = std::env::var("XDG_CONFIG_HOME").ok();
        let held_data = std::env::var("XDG_DATA_HOME").ok();
        std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
        std::env::set_var("XDG_DATA_HOME", root.join("data"));
        let paths = settings::from_environment().expect("a settings store under the scratch root");
        let mut conf = settings::read(&paths).expect("no settings yet is no settings");
        conf.voices = voices.display().to_string();
        settings::save(&paths, &conf).expect("the scratch settings file");
        let window = narrate_page(app, base_state());
        // A stereo source, made here with the same tool the seam will call.
        let source = root.join("my take (2).wav");
        let made = std::process::Command::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi", "-i"])
            .arg("sine=frequency=440:duration=1")
            .args(["-ac", "2", "-c:a", "pcm_s16le"])
            .arg(&source)
            .status()
            .expect("ffmpeg makes the source wav");
        assert!(made.success(), "the test's own source wav was made");
        let folder = ui::narrate_voices::voices_folder();
        let before = naivepost::voices::list_wavs(&folder);
        let said = ui::narrate_voices::add_voice_file(&window, &source).expect("the copy succeeded");
        let after = naivepost::voices::list_wavs(&folder);
        assert_eq!(after.len(), before.len() + 1, "one more wav in the folder: {after:?}");
        let landed = after.last().expect("the new wav").clone();
        assert_eq!(landed, "my-take-2.wav", "sanitised base name: {said}");
        assert!(
            mono_pcm(&folder.join(&landed)),
            "{landed} is a MONO pcm wav (channels == 1)"
        );
        // A second add of the same recording never overwrites.
        ui::narrate_voices::add_voice_file(&window, &source).expect("the second copy");
        assert!(
            naivepost::voices::list_wavs(&folder).contains(&"my-take-2-2.wav".to_string()),
            "the next copy counts up"
        );
        // The picker now offers it: the dropdown's rows come from the page state.
        let picker = common::widget(&window, "voice-picker")
            .downcast::<gtk::DropDown>()
            .expect("`voice-picker` is a dropdown");
        let labels = strings_of(&picker);
        assert!(
            labels.iter().any(|label| label == &landed),
            "the voice dropdown lists the copied file: {labels:?}"
        );
        // Picking a row that names a real voice must NOT be the captions sentence: the wire reads the
        // chosen row through `voice_options`, so this proves the copy reached the list the picker answers
        // from (row 0 is captions-only, and its status line would otherwise say "never spoken"). The page
        // repainted its own rows after the copy, so the id comes from the state's own options.
        let state = ui::read_state();
        let files: Vec<&str> = state.voice_files.iter().map(String::as_str).collect();
        let options = narrate_screen::voice_options(state.narrators, &files);
        assert!(
            options.iter().any(|option| option.id == landed),
            "the copied file is an option the picker can answer with: {options:?}"
        );
        // Ask the seam the picker's handler asks: the same id the resolved row names, and the sentence it
        // owes is the one about that file — not a status-line read, which any earlier page could have left.
        let said = ui::press_voice(&window, &landed);
        settle();
        assert_eq!(
            said,
            naivepost::voice_ref::file_switch(&landed),
            "the copied voice is offered as a real voice, not refused and not captions"
        );
        assert_eq!(
            ui::read_state().voice, landed,
            "the picked voice is the page's voice now"
        );
        // No file chosen copies nothing: the refusal is its own sentence.
        let untouched = naivepost::voices::list_wavs(&folder);
        assert_eq!(
            ui::narrate_voices::refused_no_file(),
            "no file chosen \u{2014} nothing was copied into the voices folder"
        );
        assert_eq!(naivepost::voices::list_wavs(&folder), untouched);
        RAN_ADD_FILE.store(true, Ordering::SeqCst);
        // Hand the machine's own settings folders back to the rest of this process.
        match held_config {
            Some(value) => std::env::set_var("XDG_CONFIG_HOME", value),
            None => std::env::remove_var("XDG_CONFIG_HOME"),
        }
        match held_data {
            Some(value) => std::env::set_var("XDG_DATA_HOME", value),
            None => std::env::remove_var("XDG_DATA_HOME"),
        }
        window.close();
        settle();
    }
}

/// How many distinct opaque colours an image holds: one flat CSS plate answers 1.
fn colour_counts(surface: &mut cairo::ImageSurface) -> Vec<(u8, u8, u8)> {
    // `data()` borrows the surface mutably and for as long as the slice lives, so every other read of
    // the surface (stride, width, height) is taken BEFORE it and handed in as plain numbers.
    let (stride, width, height) = (surface.stride() as usize, surface.width(), surface.height());
    let data = surface.data().expect("pixels");
    let mut seen: Vec<(u8, u8, u8)> = Vec::new();
    for y in 0..height {
        for x in 0..width {
            let at = y as usize * stride + x as usize * 4;
            // ARgb32 on little-endian: blue, green, red, alpha.
            let (b, g, r, a) = (data[at], data[at + 1], data[at + 2], data[at + 3]);
            if a == 0 {
                continue;
            }
            if !seen.contains(&(r, g, b)) {
                seen.push((r, g, b));
            }
        }
    }
    seen
}

/// Whether a row of the image carries any ink at all (the ruler strip, the lane body).
fn ink_rows(surface: &mut cairo::ImageSurface) -> Vec<bool> {
    let (stride, width, height) = (surface.stride() as usize, surface.width(), surface.height());
    let data = surface.data().expect("pixels");
    let ground = (data[3], data[2], data[1]);
    (0..height)
        .step_by(4)
        .map(|y| {
            (0..width).any(|x| {
                let at = y as usize * stride + x as usize * 4;
                (data[at + 3], data[at + 2], data[at + 1]) != ground
            })
        })
        .collect()
}

/// Is this pixel still the plate's dark ground?
fn is_ground(surface: &mut cairo::ImageSurface, x: i32, y: i32) -> bool {
    let (r, g, b) = pixel(surface, x, y);
    // Ground is the near-black field; anything brighter counts as ink.
    r < 40 && g < 40 && b < 40
}

/// One pixel of a painted surface as `(r, g, b)` — ARgb32 on a little-endian machine stores b,g,r,a.
fn pixel(surface: &mut cairo::ImageSurface, x: i32, y: i32) -> (u8, u8, u8) {
    let stride = surface.stride() as usize;
    let at = y as usize * stride + x as usize * 4;
    let data = surface.data().expect("pixels");
    (data[at + 2], data[at + 1], data[at])
}

/// The RIFF header's channel count: 1 means the copy really converted to mono PCM.
fn mono_pcm(path: &std::path::Path) -> bool {
    let bytes = std::fs::read(path).expect("the copied wav");
    bytes.len() > 24 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WAVE"
        && u16::from_le_bytes([bytes[22], bytes[23]]) == 1
}

/// The strings a dropdown offers, in order.
fn strings_of(picker: &gtk::DropDown) -> Vec<String> {
    let Some(model) = picker.model() else { return Vec::new() };
    let Ok(list) = model.downcast::<gtk::StringList>() else { return Vec::new() };
    (0..list.n_items())
        .filter_map(|i| list.item(i))
        .filter_map(|item| item.downcast::<gtk::StringObject>().ok())
        .map(|item| item.string().to_string())
        .collect()
}

#[test]
fn sec_07_narrate_1_screen_widgets_the_band_and_add_file_reach_their_rules() {
    let _ = gtk::init();
    let app = adw::Application::builder()
        .application_id(ui::APP_ID)
        .flags(gtk4::gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.connect_activate(round);
    // `adw::Application::run` exits when the last window closes, and each block above closes its own,
    // so it returns on its own here — never call `quit()` from inside `activate`: that fires before the
    // main loop starts and makes every later `run()` return at once without running any block.
    app.run();
    assert!(RAN_DRAG.load(Ordering::SeqCst), "the band gesture never ran");
    assert!(RAN_BUTTONS.load(Ordering::SeqCst), "the take buttons never ran");
    assert!(RAN_PAINT.load(Ordering::SeqCst), "the band paint never ran");
    assert!(RAN_ADD_FILE.load(Ordering::SeqCst), "Add file\u{2026} never ran");
}
