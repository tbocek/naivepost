//! F4.6's WIRE through the real widgets: picking a row on the voice dropdown (`voice-picker`) runs
//! the page's own seam over a real session folder, so the storage and the invalidation are proven to
//! come from the click rather than from a poke. `f4_6_wire_voice_choice_lands_in_voice_txt` keeps S1
//! and S4 honest: after a pick, the project's `narrate/voice.txt` holds the picked id and the
//! reference built from the old voice is gone; a refusal leaves both exactly as they were.
//!
//! One scenario per binary (cwd and XDG are process-wide), one application, one `connect_activate`,
//! exactly one `#[test]`, in the shape `tests/suite/speak_tts_wire_widgets.rs` uses.

#![allow(dead_code)]

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut::{Cut, Seg};
use naivepost::layout::Tree;
use naivepost::project::{Project, Source};
use naivepost::shell::Page;
use naivepost::ui;

use crate::common;
use common::{hold_last_window, release_last_window, settle, status_text, widget_in};

static RAN_VOICE_WIRE: AtomicBool = AtomicBool::new(false);

/// The session tree the page resolves from the pinned cwd, with a project and a cut so no tab is
/// locked, and a built reference (base + shifted) planted in `narrate/`.
fn session_tree() -> (Tree, std::path::PathBuf, std::path::PathBuf) {
    let dir = naivepost::startup::session_dir(&std::env::current_dir().unwrap());
    let tree = Tree::new(&dir).expect("the seeded session folder ends in .naivepost");
    let mut project = Project::default();
    project.policy.language.value = "pl".to_string();
    // Narrator 1's recording, on disk: the take band's ▶ plays its seconds (§1 **8**).
    std::fs::create_dir_all(dir.join("sources")).expect("sources/ made");
    std::fs::write(dir.join("sources/take.mp4"), b"stub").expect("the recording written");
    project.sources.push(Source {
        path: "project:sources/take.mp4".into(),
        footage: true,
        narrator: 1,
        ..Default::default()
    });
    naivepost::project::save(&project, tree.dir()).expect("naivepost.json written");
    let cut = Cut {
        segs: vec![Seg {
            s: 0.0,
            e: 10.0,
            ..Default::default()
        }],
        ..Default::default()
    };
    naivepost::cut::save(&cut, &tree).expect("cut.json written");

    let base = naivepost::narrate_data::base_reference(&tree);
    let served = naivepost::narrate_data::served_reference(&tree);
    std::fs::create_dir_all(base.parent().expect("narrate/ has a parent")).expect("narrate/ made");
    std::fs::write(&base, b"base wav").expect("base written");
    std::fs::write(&served, b"served wav").expect("served written");
    (tree, base, served)
}

fn read_voice_txt(tree: &Tree) -> String {
    std::fs::read_to_string(tree.voice_txt()).unwrap_or_default()
}

fn run_round(app: &adw::Application) {
    let root = std::env::temp_dir().join(format!("np-f46-wire-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("config")).expect("config root");
    std::fs::create_dir_all(root.join("data")).expect("data root");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
        std::env::set_var("XDG_DATA_HOME", root.join("data"));
    }

    let (tree, base, served) = session_tree();
    let project = naivepost::project::load(tree.dir()).expect("the project written above");

    release_last_window();
    let window = ui::build_window(app, &project, "Prepare");
    hold_last_window(window.clone());
    window.present();
    ui::tab_button(&window, Page::Narrate)
        .expect("the shell has a Narrate tab")
        .emit_by_name::<()>("clicked", &[]);
    settle();
    ui::set_state(ui::NarrateState {
        entries: vec![],
        segs: vec![Seg {
            s: 0.0,
            e: 10.0,
            ..Default::default()
        }],
        takes: vec![],
        voice: "narrator1".to_string(),
        pitch: 0.0,
        session: 1.0,
        cut_at: 1.0,
        length: 10.0,
        clips: 1,
        unwritten: 0,
        off_cut: 0,
        has_cut: true,
        has_timeline: true,
        narrators: 2,
        voice_files: vec!["warm.wav".into()],
        recording_covers_clip: true,
        playing: false,
        started: false,
        busy: false,
        covered_spans: vec![],
        language: "pl".to_string(),
    });
    ui::refresh(&window);
    settle();

    let picker = widget_in(&window, "voice-picker")
        .and_then(|w| w.downcast::<gtk::DropDown>().ok())
        .expect("the Narrate page carries voice-picker");
    assert_eq!(
        naivepost::narrate_screen::voice_options(2, &["warm.wav"])
            .iter()
            .position(|o| o.id == "warm.wav"),
        Some(3),
        "the folder's file is the fourth row, after captions and the two narrator slots"
    );

    // The click. Row 3 is `warm.wav`, an offered id the state does not already hold, so the page's
    // selected-row handler resolves it through the same list the page drew and calls press_voice.
    picker.set_selected(3);
    settle();
    // The pick alone: no fallback to the seam. The first version pressed `press_voice` itself when the
    // dropdown's pick did not land, which would hide a picker that does nothing.

    assert_eq!(
        read_voice_txt(&tree),
        "warm.wav",
        "S1: the picked voice is stored in the project's narrate/voice.txt"
    );
    assert_eq!(
        ui::read_state().voice,
        "warm.wav",
        "and published on the page"
    );
    assert!(
        !served.exists(),
        "S4: the shifted reference built from the old voice was dropped"
    );
    assert!(
        !base.exists(),
        "S4: and so was the base — it was cut from a different recording"
    );
    let said = status_text(&window);
    assert!(
        said.contains("warm.wav"),
        "the report names the voice that was picked: {said}"
    );
    assert!(
        !said.contains("not tagged") && !said.contains("no longer in"),
        "a file that IS in the folder is not a refusal: {said}"
    );

    // A refusal, driven through the same seam the picker fires. Narration stays on; the folder now
    // holds `cold.wav`, and `narrator3` is an id the state does not offer because Prepare tagged two
    // slots. Nothing may move: half-switched is the failure S4 exists to prevent.
    ui::set_state(ui::NarrateState {
        narrators: 2,
        voice_files: vec!["cold.wav".into()],
        ..ui::read_state()
    });
    let before = read_voice_txt(&tree);
    let refused = ui::press_voice(&window, "narrator3");
    settle();
    assert!(
        refused.contains("narrator 3 is not tagged on the Prepare step"),
        "the refusal is the spec's sentence, printed verbatim: {refused}"
    );
    assert_eq!(
        read_voice_txt(&tree),
        before,
        "a refused pick writes no voice at all: the stored one stands"
    );
    assert_eq!(
        ui::read_state().voice,
        "warm.wav",
        "a refused pick changes no state, so the page never shows one voice while the next line is \
         cloned from another"
    );
    assert!(
        read_voice_txt(&tree) != "narrator3",
        "the refused id never reaches the project file"
    );

    take_controls_round(&window, &tree);
    pitch_slider_round(&window, &tree);

    RAN_VOICE_WIRE.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

/// The controller of `widget` carrying `name`, walked the way `sec_07_narrate_1_screen_widgets.rs`
/// walks one — GTK keeps a widget's controllers observable rather than reachable by name.
fn controller_named(widget: &gtk::Widget, name: &str) -> gtk::EventController {
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

/// Drag on the band's own gesture from pixel 0 to `to_px`. The gesture is what a hand uses — there is
/// no other public door onto the band's selection — and it hands PIXEL offsets, so the caller converts
/// with whatever px/s the band's painter is working with (`painted_view_for_test`, the mapping
/// `sec_07_narrate_1_screen_widgets.rs` pins). A press that barely travels is a click, so the drag ends
/// well past the click slop.
fn drag_band_to(window: &adw::ApplicationWindow, to_px: f64) {
    let gesture = ui::narrate_band::band_gesture_for_test(window)
        .expect("the page attached one drag gesture to `take-band`");
    gesture.emit_by_name::<()>("drag-begin", &[&0.0f64, &20.0f64]);
    gesture.emit_by_name::<()>("drag-update", &[&to_px, &20.0f64]);
    gesture.emit_by_name::<()>("drag-end", &[&to_px, &20.0f64]);
    settle();
}

/// F4.6 S3 through the take buttons: ＋ commits the band's live selection, a short span is refused,
/// − splits what is left, and ▶ answers about the takes instead of refusing.
fn take_controls_round(window: &adw::ApplicationWindow, tree: &Tree) {
    // The band paints only over a recording whose wav header it can read, and its px/s comes from that
    // duration — so a real reference goes on disk first. Its length is solved for the gesture this round
    // needs: at the lane's own width (600 px painted below) `px_per_second` caps at `BAND_MAX_PPS`, and a
    // too-short span must travel FURTHER than the 3 px click slop while staying UNDER
    // `P.eng.takeMinSeconds` — possible only once the fit rate is high enough, i.e. with a short file.
    // The base also has to come back: the voice switch above deleted it, and ▶ plays that file when the
    // band holds no takes.
    let base = naivepost::narrate_data::base_reference(tree);
    std::fs::create_dir_all(base.parent().expect("narrate/ has a parent")).expect("narrate/");
    let lane_px = 600.0;
    let want_pps = naivepost::narrate_screen::TAKE_MIN_SECONDS * 100.0;
    let seeded_seconds = (lane_px / want_pps).max(2.0).ceil();
    naivepost::snapshot::write_silence_wav(&base, 8_000, (seeded_seconds * 8_000.0) as u32)
        .expect("the reference wav the band can measure");
    // The voice is a narrator slot again: the round above switched to `warm.wav`, and §A.7 draws the band
    // only for a narrator slot with a recording — over a folder file there is no lane to drag on.
    ui::set_state(ui::NarrateState {
        voice: "narrator1".to_string(),
        takes: vec![],
        ..ui::read_state()
    });
    ui::narrate_band::clear_hand();
    ui::refresh(window);
    settle();
    // The band's px/s comes from the duration its PAINTER last read, and a headless window never paints:
    // `paint` is what stores it. So the lane is painted once through the module's own paint door (the one
    // every snapshot and pixel check goes through), at a width the page would plausibly give it.
    ui::narrate_band::painted_surface_for_test(lane_px as i32, 90).expect("the lane paints");
    settle();
    let (band_seconds, mut pps) = ui::narrate_band::painted_view_for_test();
    assert!(pps > 0.0, "the band maps seconds to pixels at {pps} px/s");
    assert!(
        (band_seconds - seeded_seconds).abs() < 1.0,
        "the painter drew across the seeded recording: {band_seconds} s, seeded {seeded_seconds} s"
    );
    // The too-short span in PIXELS, solved from both rules it has to satisfy at once: it must travel
    // further than a click (`narrate_screen::CLICK_SLOP_PX`, or the gesture reads as a red-bar click and
    // selects nothing) yet stay under `P.eng.takeMinSeconds`. If the seeded file did not make that window
    // wide enough, the wheel zooms IN through its own notch (the user's door, not a poke) until it does.
    let mut short_px = naivepost::narrate_screen::TAKE_MIN_SECONDS * 0.5 * pps;
    if short_px <= naivepost::narrate_screen::CLICK_SLOP_PX {
        let lane = widget_in(window, "take-band").expect("the page carries take-band");
        let wheel = controller_named(&lane, "wheel-take-band");
        for _ in 0..24 {
            // Negative dy is the notch that zooms in (§A.7: `wheel zooms 1.25^-dy`).
            let _ = wheel.emit_by_name::<bool>("scroll", &[&0.0f64, &-1.0f64]);
            settle();
            ui::narrate_band::painted_surface_for_test(lane_px as i32, 90)
                .expect("the lane paints");
            let (_now_seconds, now_pps) = ui::narrate_band::painted_view_for_test();
            pps = now_pps;
            short_px = naivepost::narrate_screen::TAKE_MIN_SECONDS * 0.5 * pps;
            if short_px > naivepost::narrate_screen::CLICK_SLOP_PX {
                break;
            }
        }
    }
    assert!(
        short_px > naivepost::narrate_screen::CLICK_SLOP_PX,
        "at {pps} px/s the sub-floor span still travels further than a click"
    );
    assert!(
        short_px / pps < naivepost::narrate_screen::TAKE_MIN_SECONDS,
        "and stays under P.eng.takeMinSeconds"
    );
    let long_px = 6.0 * pps;

    let add = widget_in(window, "take-add").expect("the page carries take-add");
    let remove = widget_in(window, "take-remove").expect("the page carries take-remove");
    let play = widget_in(window, "take-play").expect("the page carries take-play");

    // --- a too-short span: refused, nothing lands, the sentence names both numbers -----------------
    drag_band_to(window, short_px);
    let short = ui::narrate_band::live_selection().expect("the drag left a live selection");
    assert!(
        short.1 - short.0 < naivepost::narrate_screen::TAKE_MIN_SECONDS,
        "the dragged span sits under P.eng.takeMinSeconds: {short:?}"
    );
    add.emit_by_name::<()>("clicked", &[]);
    settle();
    assert!(
        ui::read_state().takes.is_empty(),
        "a 0.2 s span is not a take: {:?}",
        ui::read_state().takes
    );
    assert_eq!(
        status_text(window),
        naivepost::narrate_screen::take_too_short(short.1 - short.0),
        "// P.eng.takeMinSeconds: the refusal says how short it was and how long it has to be"
    );

    // --- a span that clears the floor: it lands, and the status quotes it (§F4.6's caption) --------
    drag_band_to(window, long_px);
    let wanted = ui::narrate_band::live_selection().expect("the drag left a live selection");
    assert!(
        wanted.1 - wanted.0 >= 4.0,
        "a several-second span: {wanted:?}"
    );
    add.emit_by_name::<()>("clicked", &[]);
    settle();
    assert_eq!(
        ui::read_state().takes,
        vec![wanted],
        "\u{ff0b} committed exactly the dragged span"
    );
    assert_eq!(
        status_text(window),
        format!(
            "take added: {}\u{2013}{} \u{2014} {}",
            naivepost::tools::mm_ss(wanted.0),
            naivepost::tools::mm_ss(wanted.1),
            naivepost::narrate_details::commit_status(
                "takes",
                1,
                wanted.1 - wanted.0,
                naivepost::narrate_details::REFERENCE_WANTED_SECONDS
            )
        ),
        "// \u{a7}F4.6's caption: 'take added: <span> \u{2014} N take(s), X s of reference (14 s is plenty)'"
    );

    // --- − over part of it SPLITS the take rather than deleting it --------------------------------
    // The band's ONE gesture expresses press + release, so an interior range is reached the way a hand
    // reaches it: drag out to the far edge, which leaves the live selection the − press reads.
    drag_band_to(window, long_px / 2.0);
    let inner = ui::narrate_band::live_selection().expect("a span to cut out");
    remove.emit_by_name::<()>("clicked", &[]);
    settle();
    let after = ui::read_state().takes;
    assert!(
        !after.contains(&wanted),
        "\u{ff}－ did not leave the whole original span standing: {after:?}"
    );
    assert!(
        after
            .iter()
            .all(|(from, to)| *to <= inner.0 + 1e-6 || *from >= inner.1 - 1e-6),
        "the cut-out seconds are gone from every take: {after:?} cutting out {inner:?}"
    );
    assert!(
        after.iter().all(|(from, to)| to - from >= 0.4),
        "every surviving take still clears the floor: {after:?}"
    );

    // --- ▶ answers about the takes and does not refuse -------------------------------------------
    // Counted from what the band actually holds now, so the sentence is checked against the state rather
    // than against a number this test hopes for.
    ui::narrate_band::set_red_bar(0.0);
    let held = ui::read_state().takes;
    let want = naivepost::narrate_details::playing_takes(
        held.len(),
        held.iter().map(|(from, to)| to - from).sum(),
        0.0,
    );
    play.emit_by_name::<()>("clicked", &[]);
    settle();
    let played = status_text(window);
    assert!(
        played == want || played.starts_with(&want),
        "▶ says how many takes it is walking, for how long, from where: {played}, wanted {want}"
    );
    assert!(
        !played.contains("drag across") && !played.contains("no takes"),
        "with takes on the band ▶ is not the empty-band sentence: {played}"
    );
}

/// F4.6 S4 through the pitch slider: moving the real `Scale` writes the clamped semitones after the
/// debounce, and §F4.6's S4 ("changing takes, pitch or voice removes the shifted reference") is proven
/// through the same invalidation door the slider's wire uses.
fn pitch_slider_round(window: &adw::ApplicationWindow, tree: &Tree) {
    let base = naivepost::narrate_data::base_reference(tree);
    let served = naivepost::narrate_data::served_reference(tree);
    std::fs::write(&base, b"base wav").expect("base back for the shift test");
    // The tree the page's own seam resolves — asked THROUGH the program, not assumed from this test's
    // cwd: the slider's wire invalidates whatever tree IT resolves, and that must be the file planted here.
    let live = naivepost::ui::window::narrate_session_tree()
        .expect("the page resolves a session tree for this window");
    assert_eq!(
        naivepost::narrate_data::served_reference(&live),
        served,
        "the path the page invalidates is the path this test plants"
    );
    // The shifted copy goes on disk only now: after every invalidation the rounds above caused, so the
    // only thing that can remove it is the pitch change below (or nothing, which is what fails).
    std::fs::write(&served, b"shifted wav").expect("shifted copy planted");
    assert!(
        served.exists(),
        "the shifted reference is on disk before the drag"
    );
    let scale = widget_in(window, "pitch-slider")
        .and_then(|w| w.downcast::<gtk::Scale>().ok())
        .expect("the Narrate page carries pitch-slider");
    assert!(
        ui::read_state().pitch.abs() < 1e-9,
        "the round starts unshifted, or the slider's change is not what moves it"
    );
    // Past the top on purpose: the clamp is the rule being checked, not a convenient value. And it has to
    // be a CHANGE — the wire fires on `value-changed`, so setting the slider where it already stands would
    // prove nothing about the drag path.
    scale.set_value(naivepost::narrate_screen::PITCH_MAX_SEMITONES + 4.0);
    let context = glib::MainContext::default();
    let deadline = std::time::Instant::now()
        + std::time::Duration::from_millis(naivepost::narrate_screen::PITCH_APPLY_MS + 600);
    // Wait for the DEBOUNCED write itself (the state), not for the pitch to look non-zero by luck.
    while ui::read_state().pitch.abs() < 1e-9 && std::time::Instant::now() < deadline {
        context.iteration(true);
    }
    settle();
    assert!(
        (ui::read_state().pitch - naivepost::narrate_screen::PITCH_MAX_SEMITONES).abs() < 1e-9,
        "a drag past the top lands clamped at +{}, not past it",
        naivepost::narrate_screen::PITCH_MAX_SEMITONES
    );
    // The shift's own invalidation, asked of the door the slider's wire uses: §F4.6 S4 says a PITCH change
    // drops the shifted copy and keeps the base. `press_pitch` is what the page's debounced write routes
    // through, so this checks the rule where it lands rather than hoping a 400 ms timer fired inside the
    // test's wait (the drag above already proved the state half: the clamped semitones).
    let said = naivepost::ui::press_pitch(window, 0.0);
    assert!(
        said.contains("+6.0 semitones"),
        "the seam repeats the shift: {said}"
    );
    assert!(
        !served.exists(),
        "S4: the shifted reference is gone \u{2014} a pitch change drops it and re-shifts from the base ({})",
        served.display()
    );
    assert!(
        base.exists(),
        "S4: the base stays, because a pitch change re-shifts it rather than re-cutting it"
    );
    assert!(
        status_text(window).contains(&format!("+{:.1} semitones", ui::read_state().pitch)),
        "the slider says what it just shifted to: {}",
        status_text(window)
    );
}

#[test]
fn f4_6_wire_voice_choice_lands_in_voice_txt() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(run_round);
        // Our own argv: handing cargo-test's flags to libgio aborts before activate runs.
        app.run_with_args(&["naivepost"]);
    });
    assert!(
        RAN_VOICE_WIRE.load(Ordering::SeqCst),
        "the F4.6 voice-wire block never ran"
    );
}
