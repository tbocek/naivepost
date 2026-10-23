//! §07-narrate#1-screen, numbers **8** and **10**, through the WIRE: the band's own drag gesture selects
//! seconds, the real `take-add` button commits them, and the lane then PAINTS the take it was given —
//! shaded and labelled, which is the ink rule `sec_07_narrate_1_screen_take_band_ink.rs` pins from the
//! other side. The same press has two refusals (§6 keeps each in its own sentence: too short, and "nothing
//! was dragged at all"), and both are fired from the same button rather than called directly.
//!
//! Harness copied from `sec_07_narrate_1_screen_widgets.rs` (that file is the reference): one
//! `adw::Application`, blocks as stages of one `connect_activate`, each block its own scratch session
//! folder, every window closed before the next block. No sleeps, no server.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;

use naivepost::narrate_data::{self, Take};
use naivepost::narrate_screen;
use naivepost::ui;

use crate::common;
use common::{fixture_dir, hold_last_window, release_last_window, settle, status_text, wait_until};

static RAN_WIRE: AtomicBool = AtomicBool::new(false);
static RAN_REFUSALS: AtomicBool = AtomicBool::new(false);

/// A session folder of our own, so nothing this test writes lands in a real project or another test's.
fn scratch_root(tag: &str) -> std::path::PathBuf {
    let root = naivepost::startup::session_dir(&std::env::temp_dir().join(format!(
        "np-071-wire-{}-{}",
        tag,
        std::process::id()
    )));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("scratch session folder");
    std::env::set_current_dir(&root).expect("cwd pinned to this root");
    root
}

/// The project the shot's session holds: one narrator-tagged recording, because `takes.json` is keyed by
/// that file's stem (§3-data) and without a tagged source `take_recording_key` answers None and the press
/// files nothing at all.
fn stand_project() {
    let tree = ui::window::narrate_session_tree().expect("the scratch cwd is a session folder");
    std::fs::create_dir_all(tree.dir()).expect("session folder");
    naivepost::project::save(
        &naivepost::project::Project {
            sources: vec![naivepost::project::Source {
                path: "/media/lecture.mkv".into(),
                narrator: 1,
                footage: true,
                ..Default::default()
            }],
            ..Default::default()
        },
        tree.dir(),
    )
    .expect("naivepost.json with a narrator");
}

/// The page with `state` published, sized before `present()` so the paned lays out both columns. Built the
/// way the shell builds it, because the band's gesture and ＋ are wired by the page's own `wire`.
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

/// A mono 16-bit PCM wav of `samples` silent samples at `rate`: what `voice_ref::wav_header` measures the
/// band's seconds from, without ffmpeg.
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
    if let Some(folder) = path.parent() {
        // `narrate/` exists only once something has written a file into it, and this is usually the first.
        std::fs::create_dir_all(folder).expect("the folder the wav goes in");
    }
    std::fs::write(path, bytes).expect("write the reference wav");
}

fn rgb(px: &[u8], width: i32, x: i32, y: i32) -> (i32, i32, i32) {
    let at = ((y * width + x) * 3) as usize;
    (px[at] as i32, px[at + 1] as i32, px[at + 2] as i32)
}

/// Drag on the band the way GTK fires its own gesture, and hand back the seconds it stored.
fn drag_on_band(window: &adw::ApplicationWindow, from_px: f64, to_px: f64) -> Option<(f64, f64)> {
    let gesture =
        ui::narrate_band::band_gesture_for_test(window).expect("the page wired the band's gesture");
    gesture.emit_by_name::<()>("drag-begin", &[&from_px, &20.0f64]);
    gesture.emit_by_name::<()>("drag-update", &[&to_px, &20.0f64]);
    gesture.emit_by_name::<()>("drag-end", &[&to_px, &20.0f64]);
    settle();
    ui::narrate_band::live_selection()
}

fn round(app: &adw::Application) {
    // --- THE WIRE: drag → real ＋ press → state, file, status, and the painted take ----------
    {
        scratch_root("take");
        stand_project();
        ui::narrate_band::clear_hand();
        let mut state = base_state();
        state.takes = vec![];
        let window = narrate_page(app, state);
        let tree = ui::window::narrate_session_tree().expect("a session folder");
        // The band has seconds only over a recording it can measure (§A.7), and its px/s — the number every
        // pixel in this block is converted with — comes from that length. A 45 s reference is what the shot
        // and the ink test both stand, so the two files agree about the mapping too. It must also SURVIVE the
        // press below: ＋ files the picks and then invalidates the reference cut (§F4.6 S4), which DELETES
        // `voice_ref_base.wav` — after that the band falls back to its 0.1 s floor and every take maps off the
        // edge of any image, so nothing is drawn or labelled.
        write_wav(&narrate_data::base_reference(&tree), 16_000, 45 * 16_000);
        ui::refresh(&window);
        settle();
        let _ = common::widget(&window, "take-band");
        let (seconds, pps) = ui::narrate_band::painted_view_for_test();
        assert!(
            seconds > 30.0,
            "the lane is drawn across the 45 s reference: {seconds} s"
        );
        // Pixels that map to seconds the band's OWN rule accepts (≥ P.eng.takeMinSeconds) and that stay
        // inside the recording. The drag is placed by SECONDS converted through the band's live px/s, and it
        // must be a LONG span (15 s): the take's own seconds decide where it lands once the recording is refit
        // to the 500 px image painted below, and a short span near the left of the file can still end up too
        // narrow there for §1's word — an ink failure that would say nothing about the ink.
        let from_px = 3.0 * pps;
        let to_px = 18.0 * pps;
        assert!(
            to_px / pps < seconds,
            "the drag must land inside the recording: {to_px} px at {pps:.2} px/s over {seconds} s"
        );
        let dragged =
            drag_on_band(&window, from_px, to_px).expect("the drag left a live selection");
        assert!(
            dragged.1 - dragged.0 >= narrate_screen::take_min(),
            "the dragged span reaches the floor: {:.2} s",
            dragged.1 - dragged.0
        );
        // The button, fired the way the toolkit does — not `press_take_add`, which is the door the
        // logic tests call. The wire reads the band's live selection itself. Then the reference goes back,
        // because §F4.6 S4 has this very press delete it and the painted take below needs seconds to map by.
        common::widget(&window, "take-add").emit_by_name::<()>("clicked", &[]);
        settle();
        write_wav(&narrate_data::base_reference(&tree), 16_000, 45 * 16_000);
        ui::refresh(&window);
        settle();
        let takes = ui::read_state().takes;
        assert_eq!(
            takes.len(),
            1,
            "the press committed exactly one take: {takes:?}"
        );
        assert!(
            (takes[0].0 - dragged.0).abs() < 0.01 && (takes[0].1 - dragged.1).abs() < 0.01,
            "the take is the dragged span: {:?} vs {dragged:?}",
            takes[0]
        );
        assert!(
            takes[0].1 - takes[0].0 >= narrate_screen::take_min(),
            "and cleaned: {:?}",
            takes[0]
        );
        wait_until("the status line to name the take", || {
            status_text(&window).contains("take")
        });
        assert!(
            status_text(&window).contains("take"),
            "the commit says what landed: {:?}",
            status_text(&window)
        );
        // §3-data: the picks are the PROJECT's, filed under the recording's stem, and what a later page
        // build re-seeds from. `load_takes` is that file's own reader.
        let key =
            ui::narrate_seed::take_recording_key(&tree).expect("the narrator recording's key");
        let stored = narrate_data::load_takes(&tree).expect("takes.json reads");
        assert_eq!(
            stored.get(&key).map(|list| list.len()),
            Some(1),
            "narrate/takes.json holds this recording's one take: {stored:?}"
        );
        let stored_span = &stored[&key][0];
        assert!(
            (stored_span.s - dragged.0).abs() < 0.01 && (stored_span.e - dragged.1).abs() < 0.01,
            "the file holds the same span the state does: {stored_span:?} vs {dragged:?}"
        );
        assert_eq!(
            stored.get(&key).map(|list| list.as_slice()),
            Some(
                &[Take {
                    s: stored_span.s,
                    e: stored_span.e
                }][..]
            ),
            "and nothing else is filed under it"
        );
        // And the page's own seed door — what a rebuild re-reads that file through — shows the same span.
        assert_eq!(
            ui::narrate_seed::stored_takes(&tree),
            vec![(stored_span.s, stored_span.e)],
            "the file and the page's seed agree: the take survives a rebuild"
        );

        // Now the lane's own picture of it: §1 says each take is SHADED and carries its duration alone when
        // it is wide. Every geometry number comes from the band's own plan for THIS image: `painted_plan_for_test`
        // paints a scratch surface at exactly this width and returns the `Plan` `paint` built, and at fit
        // `clamp_view` pins the offset to 0 — which is what a standalone surface draws with, so no zooming and
        // no panning is needed to bring a take into the frame.
        ui::narrate_band::clear_hand();
        let width = 500i32;
        let height = narrate_screen::BAND_LANE_PX as i32;
        let ruler = narrate_screen::BAND_RULER_PX as i32;
        let plan = ui::narrate_band::painted_plan_for_test(width as f64, height as f64);
        let (left, take_w, drawn_label) = plan
            .takes
            .first()
            .map(|held| (held.0, held.1, held.2.clone()))
            .expect("the take the button made is in the paint plan");
        assert!(
            take_w >= 100.0,
            "the take the press made is drawn inside this image and wide enough for its word — if this fails \
             the dragged span is too short or off the frame, not the ink: {take_w} px in a {width} px image"
        );
        // The word the plan drew is the one this take's seconds spell — read from the dragged span, never
        // hardcoded: the drag's pixel ends land on their own seconds.
        assert_eq!(
            drawn_label,
            naivepost::narrate_band::label_in(dragged),
            "§1: labelled with its DURATION ALONE ({take_w} px wide)"
        );
        let (px, pw, ph) = {
            let (px, pw, ph, _, _) =
                ui::narrate_band::painted_pixels_for_test(width, height).expect("the band painted");
            (px, pw, ph)
        };
        let shaded = (((left as i32).max(0))..((left + take_w) as i32).min(pw)).any(|x| {
            ((ruler + 1)..ph).any(|y| {
                let (r, g, b) = rgb(&px, pw, x, y);
                g > r + 30 && g > b + 20
            })
        });
        assert!(
            shaded,
            "the take the button made is SHADED across its own seconds ({left}..{} px)",
            left + take_w
        );
        // The label's ink sits at `left + 3` on one baseline, so that box is the strip to read. Nothing else
        // in those rows passes the strict near-white filter: the red ▶-start bar is (219,51,46) with r leading,
        // and the waiting sentence is grey at 191 — both rejected by the filter itself, no exclusion needed.
        let strip =
            (((left + 3.0) as i32).max(0))..(((left + 63.0).min((width - 1) as f64)) as i32);
        let glyphed = strip
            .flat_map(|x| ((ruler + 3)..(ruler + 16)).map(move |y| (x, y)))
            .filter(|&(x, y)| {
                let (r, g, b) = rgb(&px, pw, x, y);
                r >= 200 && g >= 200 && b >= 200 && b.abs_diff(r) <= 12
            })
            .count();
        assert!(
            glyphed >= 3,
            "and its seconds are inked in LIGHT glyphs over the wash: {glyphed} bright pixels in the label \
             strip of a {pw}×{ph} lane (take at {left}..{} px)",
            left + take_w
        );
        RAN_WIRE.store(true, Ordering::SeqCst);
        window.close();
        settle();
    }

    // --- THE OTHER BRANCHES of the same button: too short, and nothing dragged at all -------
    {
        scratch_root("refuse");
        stand_project();
        ui::narrate_band::clear_hand();
        let window = narrate_page(app, base_state());
        let tree = ui::window::narrate_session_tree().expect("a session folder");
        write_wav(&narrate_data::base_reference(&tree), 16_000, 45 * 16_000);
        ui::refresh(&window);
        settle();
        // Shorter than P.eng.takeMinSeconds but still a DRAG (§1's slop: 3 px of travel or less is a click).
        // The slop is counted in PIXELS while the floor is in SECONDS, so both are read off this band's own
        // fitted px/s — no zooming needed: at ~12 px/s for a 45 s reference in a 560 px column, 0.9 × the
        // 0.4 s floor is ~4.3 px of travel, past the slop and under the floor at the same time.
        let (_seconds, pps) = ui::narrate_band::painted_view_for_test();
        assert!(
            pps >= 10.0,
            "the fit px/s must be high enough that a sub-floor span still travels past the 3 px click slop \
             — got {pps:.2} px/s"
        );
        let short_seconds = narrate_screen::take_min() * 0.9;
        let short = short_seconds * pps;
        assert!(
            short > narrate_screen::CLICK_SLOP_PX + 0.5,
            "the short drag must clear the click slop: {short} px at {pps:.2} px/s"
        );
        assert!(
            short_seconds < narrate_screen::take_min(),
            "and still be under the floor: {short_seconds:.3} s at {pps:.2} px/s"
        );
        let dragged_short =
            drag_on_band(&window, 60.0, 60.0 + short).expect("even a short drag selects");
        assert!(
            dragged_short.1 - dragged_short.0 < narrate_screen::take_min(),
            "the drag is under the floor: {:.3} s at {pps:.2} px/s",
            dragged_short.1 - dragged_short.0
        );
        common::widget(&window, "take-add").emit_by_name::<()>("clicked", &[]);
        settle();
        assert!(
            ui::read_state().takes.is_empty(),
            "a too-short selection makes no take"
        );
        wait_until("the too-short sentence", || {
            status_text(&window)
                == narrate_screen::take_too_short(dragged_short.1 - dragged_short.0)
        });
        assert_eq!(
            status_text(&window),
            narrate_screen::take_too_short(dragged_short.1 - dragged_short.0),
            "the button says the floor in §6's words"
        );
        // A press with NOTHING dragged is a different sentence (§6 keeps the two apart, and it names the
        // gesture to use). A click — begin and end within 3 px — is exactly that.
        let _ = drag_on_band(&window, 200.0, 202.0);
        assert_eq!(
            ui::narrate_band::live_selection(),
            None,
            "a click clears the selection"
        );
        common::widget(&window, "take-add").emit_by_name::<()>("clicked", &[]);
        settle();
        assert!(
            ui::read_state().takes.is_empty(),
            "with no selection the press writes nothing"
        );
        wait_until("the drag-first sentence", || {
            status_text(&window).contains("drag across the wave")
        });
        assert!(
            status_text(&window).contains("drag across the wave"),
            "and it names the gesture instead: {:?}",
            status_text(&window)
        );
        assert!(
            !tree.takes_json().exists(),
            "neither refusal filed anything in narrate/takes.json"
        );
        RAN_REFUSALS.store(true, Ordering::SeqCst);
        window.close();
        settle();
    }
}

#[test]
fn sec_07_narrate_1_screen_s10_drag_then_the_real_take_add_button_makes_a_labelled_take() {
    let _ = gtk::init();
    let app = adw::Application::builder()
        .application_id(ui::APP_ID)
        .flags(gtk4::gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.connect_activate(round);
    // `run` returns when the last window closes and both blocks close their own; never `quit()` from
    // inside `activate` — that fires before the loop starts and later `run()`s return without running.
    app.run_with_args::<&str>(&[]);
    assert!(
        RAN_WIRE.load(Ordering::SeqCst),
        "the drag-and-＋ block never ran"
    );
    assert!(
        RAN_REFUSALS.load(Ordering::SeqCst),
        "the refusal block never ran"
    );
}
