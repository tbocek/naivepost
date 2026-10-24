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

use crate::common;
use common::{
    click, fixture_dir, hold_last_window, release_last_window, settle, status_text, wait_until,
};

static RAN_DRAG: AtomicBool = AtomicBool::new(false);
static RAN_BUTTONS: AtomicBool = AtomicBool::new(false);
static RAN_PAINT: AtomicBool = AtomicBool::new(false);
static RAN_ADD_FILE: AtomicBool = AtomicBool::new(false);
static RAN_POINTER: AtomicBool = AtomicBool::new(false);
static RAN_SCROLLBAR: AtomicBool = AtomicBool::new(false);
static RAN_FRAME_STEP: AtomicBool = AtomicBool::new(false);
static RAN_BAND_SHOWN: AtomicBool = AtomicBool::new(false);
static RAN_GLYPHS: AtomicBool = AtomicBool::new(false);
static RAN_NUMBERED_WIDGETS: AtomicBool = AtomicBool::new(false);
static RAN_INPUTS_READOUT: AtomicBool = AtomicBool::new(false);
static RAN_PITCH_ROW: AtomicBool = AtomicBool::new(false);

/// A session folder of our own, so nothing this test writes lands in a real project or another test's.
fn scratch_root(tag: &str) -> std::path::PathBuf {
    let root = naivepost::startup::session_dir(&std::env::temp_dir().join(format!(
        "np-sec07-wire-{}-{}",
        tag,
        std::process::id()
    )));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("scratch session folder");
    std::env::set_current_dir(&root).expect("cwd pinned to this root");
    root
}

/// Stage a session that makes `state`'s Inputs counts true, then repaint the bottom bar through the same
/// door an Open uses (`window::paint_readouts_for`). The shell reads FOLDERS, not the page's state, so this
/// is the only honest way to show §1's stale mark on that row: four equal clips with one unwritten and two
/// lines sitting past them, or the timeline file absent, or no cut at all.
fn stage_session_and_repaint(window: &adw::ApplicationWindow, state: &ui::NarrateState) {
    let root = std::env::current_dir().expect("a cwd to hold the session");
    let tree = naivepost::layout::Tree::new(naivepost::startup::session_dir(&root))
        .expect("the session folder beside the cwd");
    std::fs::create_dir_all(tree.dir()).expect("the session folder");
    // A project file is what makes the shell count anything at all: without it there is no tree to read.
    std::fs::write(
        root.join(naivepost::project::PROJECT_FILE),
        "{\"sources\": []}\n",
    )
    .expect("a project file");
    let each = if state.clips > 0 {
        state.length / state.clips as f64
    } else {
        0.0
    };
    let segs: Vec<naivepost::cut::Seg> = (0..state.clips)
        .map(|index| naivepost::cut::Seg {
            s: f64::from(index as u32) * each,
            e: f64::from(index as u32 + 1) * each,
            cam: 0,
            ..Default::default()
        })
        .collect();
    if state.has_cut {
        std::fs::create_dir_all(tree.dir().join("cut")).expect("the cut folder exists");
        std::fs::write(
            tree.cut_json(),
            serde_json::to_vec_pretty(&naivepost::cut::Cut {
                segs: segs.clone(),
                ..Default::default()
            })
            .expect("a cut"),
        )
        .expect("the cut is written");
    } else {
        let _ = std::fs::remove_file(tree.cut_json());
    }
    // Written clips first, then the off-cut lines: a line whose span no clip covers is exactly what
    // "N line(s) off the cut" counts, and a clip left without one is what "unwritten" counts.
    let mut entries: Vec<naivepost::narration::Entry> = segs
        .iter()
        .take(state.clips.saturating_sub(state.unwritten))
        .map(|seg| naivepost::narration::Entry {
            s: seg.s,
            e: seg.e,
            at: 0.5,
            text: "staged".into(),
            emotion: String::new(),
            pos: String::new(),
            roll: 0,
        })
        .collect();
    for index in 0..state.off_cut {
        let past = state.length + f64::from(index as u32 + 1) * 10.0;
        entries.push(naivepost::narration::Entry {
            s: past,
            e: past + 5.0,
            at: 0.5,
            text: "off the cut".into(),
            emotion: String::new(),
            pos: String::new(),
            roll: 0,
        });
    }
    std::fs::create_dir_all(tree.dir().join("narrate")).expect("the narrate folder exists");
    std::fs::write(
        tree.narration_json(),
        serde_json::to_vec_pretty(&naivepost::narration::Narration {
            entries,
            silent: Vec::new(),
        })
        .expect("a narration"),
    )
    .expect("the narration is written");
    // §1's "no timeline" names a missing `session.tsv`, which the tally reads from the session's own
    // `prepare/transcript/` folder — the same path `Tree::session_tsv()` answers with.
    let timeline = tree.dir().join("prepare").join("transcript");
    if state.has_timeline {
        std::fs::create_dir_all(&timeline).expect("the transcript folder exists");
        std::fs::write(timeline.join("session.tsv"), "0.0\tstaged timeline\n").expect("a timeline");
    } else {
        let _ = std::fs::remove_file(timeline.join("session.tsv"));
    }
    naivepost::ui::window::paint_readouts_for(window, &project_of(&tree), tree.dir());
}

/// The project file beside the session folder — the readouts' lock argument reads it, so it has to exist.
fn project_of(tree: &naivepost::layout::Tree) -> naivepost::project::Project {
    tree.dir()
        .parent()
        .and_then(|parent| naivepost::project::load(parent).ok())
        .unwrap_or_default()
}

/// The page with `state` published, sized before `present()` so the paned lays out both columns.
/// Built the way the shell builds it (fixture project, Narrate tab clicked), because the band's
/// gesture and "Add file…" are wired by the page's own `wire`, which only runs on a real window.
///
/// The bottom bar is NOT repainted here: §A.8 puts that row on the bar and the shell paints it from the
/// SESSION, so a check on it stages files and repaints through [`stage_session_and_repaint`] instead.
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
            naivepost::cut::Seg {
                s: 0.0,
                e: 30.0,
                ..Default::default()
            },
            naivepost::cut::Seg {
                s: 30.0,
                e: 60.0,
                ..Default::default()
            },
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
        assert_eq!(
            takes.len(),
            1,
            "\u{ff0b} committed exactly one take: {takes:?}"
        );
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
        assert_eq!(
            said,
            narrate_screen::take_too_short(too_short.1 - too_short.0)
        );
        assert!(
            ui::read_state().takes.is_empty(),
            "a refused take wrote nothing"
        );
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
        assert!(
            text.iter().any(|row| *row),
            "the top strip carries ink (the ruler)"
        );
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
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
            ])
            .arg("sine=frequency=440:duration=1")
            .args(["-ac", "2", "-c:a", "pcm_s16le"])
            .arg(&source)
            .status()
            .expect("ffmpeg makes the source wav");
        assert!(made.success(), "the test's own source wav was made");
        let folder = ui::narrate_voices::voices_folder();
        let before = naivepost::voices::list_wavs(&folder);
        let said =
            ui::narrate_voices::add_voice_file(&window, &source).expect("the copy succeeded");
        let after = naivepost::voices::list_wavs(&folder);
        assert_eq!(
            after.len(),
            before.len() + 1,
            "one more wav in the folder: {after:?}"
        );
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
            ui::read_state().voice,
            landed,
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

    // The pointer / scrollbar / frame-step / visibility checks run here rather than in their own `#[test]`
    // because GTK binds to the first thread that initialises it: a second application on a second test
    // thread dies at once. `second_round` is its own function so the two groups stay readable.
    second_round(app);
    third_round(app);
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
    bytes.len() > 24
        && &bytes[..4] == b"RIFF"
        && &bytes[8..12] == b"WAVE"
        && u16::from_le_bytes([bytes[22], bytes[23]]) == 1
}

/// The strings a dropdown offers, in order.
fn strings_of(picker: &gtk::DropDown) -> Vec<String> {
    let Some(model) = picker.model() else {
        return Vec::new();
    };
    let Ok(list) = model.downcast::<gtk::StringList>() else {
        return Vec::new();
    };
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
    app.run_with_args::<&str>(&[]);
    assert!(
        RAN_DRAG.load(Ordering::SeqCst),
        "the band gesture never ran"
    );
    assert!(
        RAN_BUTTONS.load(Ordering::SeqCst),
        "the take buttons never ran"
    );
    assert!(RAN_PAINT.load(Ordering::SeqCst), "the band paint never ran");
    assert!(
        RAN_ADD_FILE.load(Ordering::SeqCst),
        "Add file\u{2026} never ran"
    );
    // The second round runs inside THIS activate (see `round`): GTK and libadwaita bind to the first
    // thread that initialises them, and cargo runs the tests of a binary on different threads, so a
    // second `app.run()` in another `#[test]` dies with "libadwaita may only be used from the main
    // thread". One application, one main loop, every check as a stage of the same activate.
    assert!(
        RAN_POINTER.load(Ordering::SeqCst),
        "the pointer check never ran"
    );
    assert!(
        RAN_SCROLLBAR.load(Ordering::SeqCst),
        "the scrollbar check never ran"
    );
    assert!(
        RAN_FRAME_STEP.load(Ordering::SeqCst),
        "the frame-step check never ran"
    );
    assert!(
        RAN_BAND_SHOWN.load(Ordering::SeqCst),
        "the band-visibility check never ran"
    );
    assert!(
        RAN_GLYPHS.load(Ordering::SeqCst),
        "the take row's glyph check never ran"
    );
    assert!(
        RAN_NUMBERED_WIDGETS.load(Ordering::SeqCst),
        "the numbered-widget sweep never ran"
    );
    assert!(
        RAN_INPUTS_READOUT.load(Ordering::SeqCst),
        "the Inputs readout check never ran"
    );
    assert!(
        RAN_PITCH_ROW.load(Ordering::SeqCst),
        "the labelled pitch row check never ran"
    );
}

// --- S7/S8 + §A.7 visibility: the pointer, the scrollbar, the frame step ---------

/// The controller of `widget` carrying `name`, found the way `cut_view_wheel.rs` finds one: walk the
/// widget's own controller list rather than trusting that exactly one exists.
fn controller(widget: &gtk::Widget, name: &str) -> gtk::EventController {
    let held = widget.observe_controllers();
    (0..held.n_items())
        .filter_map(|index| held.item(index))
        .find(|item| {
            item.downcast_ref::<gtk::EventController>()
                .is_some_and(|c| c.name().is_some_and(|n| n == name))
        })
        .and_then(|item| item.downcast::<gtk::EventController>().ok())
        .unwrap_or_else(|| {
            panic!(
                "{} carries no controller named {name}",
                widget.widget_name()
            )
        })
}

/// Is this `(x, y)` of an RGB buffer still the plate's dark ground? Same threshold as `is_ground`.
fn rgb_is_ground(rgb: &[u8], width: i32, x: i32, y: i32) -> bool {
    let at = ((y as usize) * width as usize + x as usize) * 3;
    rgb[at] < 40 && rgb[at + 1] < 40 && rgb[at + 2] < 40
}

/// Blend cairo's premultiplied fill over the ground by hand, so the check names the ink it expects
/// instead of accepting any brighter pixel: `paint` lays the pointer bar with `POINTER_COLOUR` alpha.
fn blended(ground: (u8, u8, u8), want: (f64, f64, f64, f64)) -> (i32, i32, i32) {
    let a = want.3;
    (
        (ground.0 as f64 * (1.0 - a) + want.0 * 255.0 * a).round() as i32,
        (ground.1 as f64 * (1.0 - a) + want.1 * 255.0 * a).round() as i32,
        (ground.2 as f64 * (1.0 - a) + want.2 * 255.0 * a).round() as i32,
    )
}

// The four checks below are named after this item and run as a stage of `round`'s own activate (see the
// note there): GTK binds to the first thread that initialises it, so they cannot live in a second
// `#[test]` with its own application. Their assertions are the ones added to the test above.
fn second_round(app: &adw::Application) {
    // §A.7's visibility check runs FIRST on purpose: the reference wav it writes into the session
    // folder outlives its window, so every later block measures a REAL recording length (60 s here),
    // which is what makes the zoom-past-fit case reachable at all.
    {
        let mut quiet = base_state();
        quiet.voice = narrate_screen::CAPTIONS.to_string();
        let window = narrate_page(app, quiet);
        let scroller = common::widget(&window, "take-band-scroller");
        assert!(
            !scroller.is_visible(),
            "captions-only shows no take band (§A.7)"
        );
        let tree = naivepost::layout::Tree::new(naivepost::startup::session_dir(
            &std::env::current_dir().unwrap(),
        ))
        .expect("the scratch session folder");
        let reference = naivepost::narrate_data::base_reference(&tree);
        std::fs::create_dir_all(reference.parent().expect("narrate folder")).expect("folder");
        // 60 s of 8 kHz mono: long enough that zooming past fit outgrows the strip.
        write_wav(&reference, 8_000, 60 * 8_000);
        assert!(
            naivepost::voice_ref::wav_header(&reference).is_some(),
            "the test's own reference is a wav the band can measure"
        );
        assert!(
            ui::narrate_band::recording_seconds().is_some(),
            "the band reads its length off the file, not off a guess"
        );
        let voiced = narrate_page(app, base_state());
        assert!(
            common::widget(&voiced, "take-band-scroller").is_visible(),
            "a voice with a recording gets its band back"
        );
        RAN_BAND_SHOWN.store(true, Ordering::SeqCst);
        voiced.close();
        settle();
    }

    // --- the faint red bar under the pointer (§1) -----------------------------------
    {
        ui::narrate_band::clear_hand();
        let window = narrate_page(app, base_state());
        let lane = common::widget(&window, "take-band");
        let motion = controller(&lane, "motion-take-band");
        let width = 400i32;
        let height = narrate_screen::BAND_LANE_PX as i32;
        let row = (narrate_screen::BAND_RULER_PX as i32 + 4).min(height - 1);
        let bare = ui::narrate_band::painted_pixels_for_test(width, height).expect("bare paint");
        // Whatever column the hand hovers must start as plain ground: take the first ground column
        // below the ruler instead of assuming the envelope or a take left a particular one clear.
        let column = (2..width)
            .find(|&x| rgb_is_ground(&bare.0, bare.1, x, row))
            .expect("the lane draws some ground to hover over");
        motion.emit_by_name::<()>("motion", &[&(column as f64), &(row as f64)]);
        settle();
        assert!(
            ui::narrate_band::pointer_px() >= 0.0,
            "the motion controller stored its x"
        );
        let hovered =
            ui::narrate_band::painted_pixels_for_test(width, height).expect("hover paint");
        let at = ((row as usize) * width as usize + column as usize) * 3;
        let got = (hovered.0[at], hovered.0[at + 1], hovered.0[at + 2]);
        let want = blended((28, 28, 29), ui::narrate_band::POINTER_COLOUR);
        assert!(
            got.0.abs_diff(want.0 as u8) <= 3
                && got.1.abs_diff(want.1 as u8) <= 3
                && got.2.abs_diff(want.2 as u8) <= 3,
            "the pointer draws its own faint red at x={column}: got {got:?} want {want:?}"
        );
        motion.emit_by_name::<()>("leave", &[]);
        settle();
        assert!(
            ui::narrate_band::pointer_px() < 0.0,
            "a hand off the band leaves no pointer behind"
        );
        let left = ui::narrate_band::painted_pixels_for_test(width, height).expect("leave paint");
        assert!(
            rgb_is_ground(&left.0, left.1, column, row),
            "the bar is gone once the pointer leaves: {:?}",
            (left.0[at], left.0[at + 1], left.0[at + 2])
        );
        RAN_POINTER.store(true, Ordering::SeqCst);
        drop(window);
        settle();
    }

    // --- the band's own scrollbar appears past fit (§A.7) ---------------------------
    {
        // Its own tree handle: the session folder is the process cwd, which is what §A.7 wrote into.
        let tree = naivepost::layout::Tree::new(naivepost::startup::session_dir(
            &std::env::current_dir().unwrap(),
        ))
        .expect("the scratch session folder");
        ui::narrate_band::clear_hand();
        let window = narrate_page(app, base_state());
        let lane = common::widget(&window, "take-band");
        let wheel = controller(&lane, "wheel-take-band");
        let viewport = ui::narrate_band::view_width_for_test();
        // The band's own seconds come from the file it is drawn across, so read them back off that
        // file (rate from the header, sample count from the bytes after its `data` chunk) instead of
        // trusting a slot only a real paint would have filled.
        let seconds = {
            let path = naivepost::narrate_data::base_reference(&tree);
            let (_, rate) = naivepost::voice_ref::wav_header(&path).expect("the 60 s reference");
            let bytes = std::fs::read(&path).expect("the reference wav");
            let data_at = (12..bytes.len().saturating_sub(4))
                .find(|&i| &bytes[i..i + 4] == b"data")
                .expect("a data chunk");
            let declared = u32::from_le_bytes([
                bytes[data_at + 4],
                bytes[data_at + 5],
                bytes[data_at + 6],
                bytes[data_at + 7],
            ]) as usize;
            let samples = declared.min(bytes.len() - (data_at + 8)) / 2;
            samples as f64 / rate as f64
        };
        let before = lane.width_request();
        assert!(
            seconds > 30.0,
            "the band is drawn across the 60 s reference"
        );
        // One notch past fit and no further: px/s is `fit * zoom` capped at
        // `narrate.bandMaxPps`, so a first notch always grows the content (60 s over ~560 px fits at
        // under 10 px/s), while every later one only chases the cap — and GTK then allocates the lane
        // wider than its parent, which makes the remembered viewport drift from this reading.
        let _ = wheel.emit_by_name::<bool>("scroll", &[&0.0f64, &-1.0f64]);
        settle();
        let asked = lane.width_request();
        // The request is exactly what `fit_lane_to_view` handed the widget after that notch, computed
        // through the same door the program uses.
        let want =
            naivepost::ui::narrate_band::lane_width(ui::narrate_band::zoom(), seconds, viewport);
        assert_eq!(
            asked, want as i32,
            "the lane asks for exactly its content width"
        );
        assert!(
            want > viewport && asked > before,
            "60 s past fit outgrows the {viewport}px strip: {before} -> {asked}, content {want}"
        );
        RAN_SCROLLBAR.store(true, Ordering::SeqCst);
        drop(window);
        settle();
    }

    // --- paused wheel steps one frame on the picture (§1) ---------------------------
    {
        let mut state = base_state();
        state.playing = false;
        state.session = 10.0;
        let window = narrate_page(app, state);
        let picture = common::widget(&window, "narrate-picture");
        let wheel = controller(&picture, "wheel-narrate-picture");
        let frame = naivepost::narrate_frame::frame_seconds(naivepost::narrate_frame::DEFAULT_FPS);
        let started = ui::read_state().session;
        let _ = wheel.emit_by_name::<bool>("scroll", &[&0.0f64, &1.0f64]);
        settle();
        let after = ui::read_state().session;
        assert!(
            (after - (started + frame)).abs() < 1e-9,
            "one notch while paused is one frame: {started} -> {after}, frame {frame}"
        );
        // Playing: the players own the clock, so the same notch must not move the picture.
        ui::set_state({
            let mut running = ui::read_state();
            running.playing = true;
            running
        });
        let _ = wheel.emit_by_name::<bool>("scroll", &[&0.0f64, &1.0f64]);
        settle();
        assert_eq!(
            ui::read_state().session,
            after,
            "a notch on a moving picture changes nothing"
        );
        RAN_FRAME_STEP.store(true, Ordering::SeqCst);
        // Close, not drop: `narrate_page` keeps a strong hold on the window it built, so this is the
        // only thing that lets `app.run()` in the test above return.
        window.close();
        settle();
    }
}

// --- §1's numbered widgets, the take row's glyph, and the Inputs readout -----------

/// The page's own widget lookups, so a missing control fails with its §1 number rather than a bare panic.
fn found(window: &adw::ApplicationWindow, name: &str) -> gtk::Widget {
    naivepost::ui::find_source_widget(window, name)
        .unwrap_or_else(|| panic!("the Narrate page drew no `{name}`"))
}

fn tip_of(widget: &gtk::Widget) -> String {
    widget.tooltip_text().unwrap_or_default().to_string()
}

fn label_of(button: &gtk::Widget) -> String {
    button
        .downcast_ref::<gtk::Button>()
        .and_then(|b| b.label().map(String::from))
        .unwrap_or_default()
}

fn text_of(widget: &gtk::Widget) -> String {
    if let Some(view) = widget.downcast_ref::<gtk::TextView>() {
        return naivepost::ui::narrate_row::box_text(view);
    }
    if let Some(entry) = widget.downcast_ref::<gtk::Entry>() {
        return entry.text().to_string();
    }
    widget
        .downcast_ref::<gtk::Label>()
        .map(|l| l.text().to_string())
        .unwrap_or_default()
}

/// **s9** — the take row's − is an icon, not a CJK-only sign, and its press still removes the dragged span.
///
/// §1's Prototype note rejects U+FF0B because only CJK fonts carry it; U+FF0D was the same hazard on this
/// row. The check is on the BUILT button (what the user sees), and the removal goes through the real click
/// so the icon swap cannot have broken the wire.
fn s9_glyph_round(app: &adw::Application) {
    let window = narrate_page(app, base_state());
    let gesture = ui::narrate_band::band_gesture_for_test(&window).expect("the band gesture");
    // Make a take first, by dragging and pressing the real ＋, so there is something to take away.
    ui::narrate_band::clear_hand();
    gesture.emit_by_name::<()>("drag-begin", &[&100.0f64, &20.0f64]);
    gesture.emit_by_name::<()>("drag-update", &[&400.0f64, &20.0f64]);
    gesture.emit_by_name::<()>("drag-end", &[&400.0f64, &20.0f64]);
    settle();
    let wanted = ui::narrate_band::live_selection().expect("a dragged span");
    click(&window, "take-add");
    settle();
    assert_eq!(ui::read_state().takes.len(), 1, "\u{ff0b} made one take");

    let remove = found(&window, "take-remove");
    assert_eq!(
        label_of(&remove),
        "",
        "the \u{2212} button draws an icon, not a full-width CJK-only label"
    );
    assert!(
        !tip_of(&remove).is_empty(),
        "the \u{2212} keeps its tooltip even without a text label"
    );
    // And the icon name is one this theme actually carries (bare `list-remove` resolves to nothing here).
    assert_eq!(
        naivepost::narrate_screen::TAKE_REMOVE_ICON,
        "list-remove-symbolic"
    );
    click(&window, "take-remove");
    settle();
    assert!(
        ui::read_state().takes.is_empty(),
        "the icon button's press still removed the dragged span {wanted:?}"
    );
    RAN_GLYPHS.store(true, Ordering::SeqCst);
    eprintln!("SEC07-1 s9 (take row glyph + real minus press) done");
    window.close();
    settle();
}

/// **s10** — every widget §1 numbers **1**-**22** is on the built page, carrying its wording. A loop, so a
/// control that was never built (or lost its tooltip when it grew an icon face) fails by its number.
fn s10_widgets_round(app: &adw::Application) {
    let mut state = base_state();
    // One written line and one caption, so the row's own parts (**14**, **16**-**21**) exist to be found.
    state.entries = vec![
        naivepost::narration::Entry {
            s: 0.0,
            e: 30.0,
            at: 1.0,
            text: "one spoken line".into(),
            ..Default::default()
        },
        naivepost::narration::Entry {
            s: 30.0,
            e: 60.0,
            at: 0.0,
            text: "a caption".into(),
            pos: "top".into(),
            ..Default::default()
        },
    ];
    state.clips = 2;
    let window = narrate_page(app, state);

    // **1**-**13**: the left column, in §1's order, each checked for the thing §1 says it shows.
    assert_eq!(
        text_of(&found(&window, "sample-sentence")),
        narrate_screen::DEFAULT_SAMPLE,
        "**11** the sample sentence carries §1's default wording verbatim"
    );
    assert!(
        tip_of(&found(&window, "voice-picker")).starts_with("Who speaks the narration"),
        "**7** the voice picker's tooltip is §1's sentence"
    );
    assert_eq!(
        tip_of(&found(&window, "voice-add-file")),
        "Copy one recording into the voices folder and use it",
        "**9** Add file\u{2026} says what it does"
    );
    for name in [
        "narration-tick",     // **1**
        "narrate-preview",    // **2**
        "narrate-picture",    // **2**'s frame
        "narrate-back-3s",    // **3**
        "narrate-play",       // **3**
        "narrate-forward-3s", // **3**
        "narrate-add-line",   // **4**
        "narrate-slider",     // **5**
        "narrate-clock",      // **5**
        "narrate-volume",     // **6**
        "voice-picker",       // **7**
        "take-add",           // **8**
        "take-remove",        // **8**
        "take-play",          // **8**
        "voice-add-file",     // **9**
        "sample-sentence",    // **11**
        "sample-play",        // **12**
        "sample-stop",        // **12**
        "sample-reroll",      // **12**
        "pitch-slider",       // **13**
        "narrate-lines",      // the rows below
        "inputs-readout", // **22** — §A.8 puts the readouts on the window's bottom bar, not in the page
    ] {
        let widget = found(&window, name);
        assert!(widget.is_visible(), "`{name}` is drawn, not hidden away");
    }
    // **10** the take band: its lane is on the page but stays hidden until the voice has a recording to
    // draw across (§A.7), so it is checked by presence, not with the visible ones above.
    assert!(found(&window, "take-band")
        .upcast_ref::<gtk::Widget>()
        .parent()
        .is_some());
    // **3** the transport's three buttons are clickable buttons, not labels.
    for name in ["narrate-back-3s", "narrate-play", "narrate-forward-3s"] {
        assert!(
            found(&window, name).downcast_ref::<gtk::Button>().is_some(),
            "`{name}` is a button the toolkit can click"
        );
    }
    // **5** the clock reads "session · cut/length": one separator, then position over length.
    let clock = text_of(&found(&window, "narrate-clock"));
    assert_eq!(
        (clock.matches('\u{b7}').count(), clock.matches('/').count()),
        (1, 1),
        "**5** the clock is \"session · cut/length\": {clock:?}"
    );
    // **13** pitch spans ±6 semitones.
    let pitch = found(&window, "pitch-slider")
        .downcast::<gtk::Scale>()
        .expect("`pitch-slider` is a scale");
    assert_eq!(
        (pitch.adjustment().lower(), pitch.adjustment().upper()),
        (-6.0, 6.0)
    );
    assert!(
        tip_of(&found(&window, "pitch-slider")).contains("reference"),
        "**13** the pitch slider explains itself"
    );
    // **13** as spec/img/07-narrate.png draws it: `pitch: ─── 0.0` — a label, the slider and the number,
    // on ONE row. A bare slider (which is what this page had) shows neither the label nor the value.
    assert_eq!(
        text_of(&found(&window, "pitch-label")),
        "pitch:",
        "**13** the control is labelled"
    );
    assert_eq!(
        text_of(&found(&window, "pitch-value")),
        narrate_screen::pitch_readout(0.0),
        "**13** the shift's number is on screen at build"
    );
    let slider_widget = found(&window, "pitch-slider");
    let label_parent = found(&window, "pitch-label").parent();
    assert_eq!(
        label_parent
            .as_ref()
            .map(|p| p.upcast_ref::<gtk::Widget>().widget_name().to_string()),
        Some("pitch-row".to_string()),
        "**13** the label sits on the pitch row"
    );
    assert!(
        slider_widget.parent().as_ref() == label_parent.as_ref(),
        "**13** label, slider and value share one row, not three places"
    );
    assert!(
        found(&window, "pitch-value").parent().as_ref() == label_parent.as_ref(),
        "**13** the readout shares that row too"
    );

    // **14**-**21**: row 0's parts. The time field, the status, the four row buttons and the text box.
    assert_eq!(
        text_of(&found(&window, "line-time-0")),
        narrate_screen::time_field(1.0),
        "**14** the time field shows the line's second on the session clock"
    );
    assert!(
        !text_of(&found(&window, "line-status-0")).is_empty(),
        "**15** the status says when the line ends"
    );
    for suffix in ["speak", "reroll", "remove"] {
        let name = format!("line-{suffix}-0");
        assert!(
            found(&window, &name)
                .downcast_ref::<gtk::Button>()
                .is_some(),
            "`{name}` is a real button (**16**, **17**, **19**)"
        );
    }
    assert!(
        found(&window, "line-add-below-0").is_visible(),
        "**18** \u{ff0b} a line below"
    );
    assert_eq!(
        text_of(&found(&window, "line-text-1")),
        "[top] a caption",
        "**20** a caption prints its placement tag in the box"
    );
    // **21** the fit warning exists as its own widget per row (empty where the line fits).
    assert!(
        found(&window, "line-warning-0").is_visible(),
        "**21** the fit warning has a place on every row"
    );
    RAN_NUMBERED_WIDGETS.store(true, Ordering::SeqCst);
    eprintln!("SEC07-1 s10 (numbered widgets 1-22) done");
    window.close();
    settle();
}

/// **s11** — the Inputs readout prints §1's sentences, both of them. §A.8 puts that row on the window's
/// BOTTOM BAR ("Bottom bar") and §3 §1 draws it on every tab, so the label checked is the shell's
/// `inputs-readout`, repainted over a session staged to make each sentence true: a cut of four clips with one
/// left unwritten and two lines off it; the same cut with every clip written and no timeline file; no cut at
/// all. The wording is asserted byte for byte, because §1 spells it.
fn s11_inputs_round(app: &adw::Application) {
    let mut stale = base_state();
    stale.clips = 4;
    stale.length = 195.0;
    stale.unwritten = 1;
    stale.off_cut = 2;
    stale.has_cut = true;
    stale.has_timeline = true;
    let window = narrate_page(app, stale.clone());
    stage_session_and_repaint(&window, &stale);
    assert_eq!(
        text_of(&found(&window, "inputs-readout")),
        "Inputs: 4 clip(s) \u{b7} 03:15 \u{b7} \u{26a0} 1 clip(s) unwritten, 2 line(s) off the cut",
        "**22** the row spells the two counts \u{26a0} marks"
    );
    window.close();
    settle();

    let mut quiet = base_state();
    quiet.clips = 4;
    quiet.length = 195.0;
    quiet.unwritten = 0;
    quiet.off_cut = 0;
    quiet.has_timeline = false;
    let window = narrate_page(app, quiet.clone());
    stage_session_and_repaint(&window, &quiet);
    assert_eq!(
        text_of(&found(&window, "inputs-readout")),
        "Inputs: 4 clip(s) \u{b7} 03:15 \u{b7} no timeline",
        "**22** a missing timeline is named on the row"
    );
    window.close();
    settle();

    let mut none = base_state();
    none.has_cut = false;
    none.clips = 0;
    none.length = 0.0;
    let window = narrate_page(app, none.clone());
    stage_session_and_repaint(&window, &none);
    assert_eq!(
        text_of(&found(&window, "inputs-readout")),
        "Inputs: no cut yet \u{2014} build one on the Cut step",
        "**22** with no cut the row says where to make one"
    );
    RAN_INPUTS_READOUT.store(true, Ordering::SeqCst);
    eprintln!("SEC07-1 s11 (Inputs readout) done");
    window.close();
    settle();
}

/// **s13** — the pitch control as badge 13 draws it, driven the way GTK drives it.
///
/// Two halves: at build the row is `pitch:` · slider · `0.0` on one line (spec/img/07-narrate.png), and both
/// program paths that move the shift — a drag's `value-changed`, and the keyboard step `press_pitch` — leave
/// the number beside the slider reading the state the slider wrote. The readout is asserted AFTER waiting on
/// the page's own 400 ms apply timer (`common::wait_until`, wall-clock deadline), so it cannot be the test's
/// own number echoed back: the state check goes through `read_state()`, the door the logic tests read.
fn s13_pitch_round(app: &adw::Application) {
    let window = narrate_page(app, base_state());
    assert_eq!(text_of(&found(&window, "pitch-label")), "pitch:");
    assert_eq!(
        text_of(&found(&window, "pitch-value")),
        "0.0",
        "one decimal, as the picture shows it"
    );
    // The drag path: setting a GtkScale's value emits `value-changed`, exactly as a pointer drag does.
    let scale = found(&window, "pitch-slider")
        .downcast::<gtk::Scale>()
        .expect("`pitch-slider` is a scale");
    scale.set_value(-2.5);
    wait_until(
        "the pitch readout to show -2.5 after the slider moved",
        || text_of(&found(&window, "pitch-value")) == "-2.5",
    );
    assert_eq!(
        ui::narrate_page::read_state().pitch,
        -2.5,
        "the face follows the state the slider wrote"
    );
    // The drag's own apply timer is what moved the state (§1: `applied 400 ms after the last move`), so a
    // later stage of this application must not start while it is still pending — otherwise its write lands on
    // the previous stage's value. Waiting on the condition, not on a guessed pause.
    wait_until("the slider's apply timer to settle at -2.5", || {
        ui::narrate_page::read_state().pitch == -2.5
    });
    // The other path: the keyboard step, which moves the same state by a delta.
    let said = ui::narrate_page::press_pitch(&window, 1.0);
    assert_eq!(
        ui::narrate_page::read_state().pitch,
        -1.5,
        "the step moved the shift: {said}"
    );
    wait_until(
        "the pitch readout to follow the keyboard step to -1.5",
        || text_of(&found(&window, "pitch-value")) == "-1.5",
    );
    // And past the top of the slider the face reads the end it acts as, not a number nothing can do.
    ui::narrate_page::press_pitch(&window, 9.0);
    wait_until("the pitch readout to clamp at the slider's top", || {
        text_of(&found(&window, "pitch-value")) == "6.0"
    });
    RAN_PITCH_ROW.store(true, Ordering::SeqCst);
    eprintln!("SEC07-1 s13 (labelled pitch row) done");
    // Back to no shift, so a later stage of this application sees the default face.
    ui::narrate_page::press_pitch(&window, -6.0);
    window.close();
    settle();
}

/// The three checks above run as stages of the same application (see the note on `round`).
fn third_round(app: &adw::Application) {
    s9_glyph_round(app);
    s10_widgets_round(app);
    s11_inputs_round(app);
    s13_pitch_round(app);
}

/// A tiny valid mono 16-bit PCM wav: enough header for `voice_ref::wav_header` and enough bytes for
/// the band's own seconds arithmetic, without ffmpeg or a server.
fn write_wav(path: &std::path::Path, rate: u32, samples: u32) {
    let data_bytes = samples * 2;
    let mut bytes: Vec<u8> = Vec::new();
    let push = |bytes: &mut Vec<u8>, value: u32| bytes.extend_from_slice(&value.to_le_bytes());
    let push16 = |bytes: &mut Vec<u8>, value: u16| bytes.extend_from_slice(&value.to_le_bytes());
    bytes.extend_from_slice(b"RIFF");
    push(&mut bytes, 36 + data_bytes);
    bytes.extend_from_slice(b"WAVEfmt ");
    push(&mut bytes, 16);
    push16(&mut bytes, 1);
    push16(&mut bytes, 1);
    push(&mut bytes, rate);
    push(&mut bytes, rate * 2);
    push16(&mut bytes, 2);
    push16(&mut bytes, 16);
    bytes.extend_from_slice(b"data");
    push(&mut bytes, data_bytes);
    bytes.resize((44 + data_bytes) as usize, 0);
    std::fs::write(path, bytes).expect("write the reference wav");
}
