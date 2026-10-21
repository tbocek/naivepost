//! §07-narrate#1-screen, number **10**: the take band's INK and its refusal.
//!
//! Two things were wrong on this lane and neither was visible from a test: a take's duration label was inked
//! near-black on a dark wash, so §1's "labelled with its duration alone" drew letters nobody could read; and
//! when the envelope could not be decoded the lane printed one public sentence ("reading the recording…")
//! while the reason went nowhere at all, against §00's "failure is specific and local". Both are pinned here
//! as pixels and as a log line — the plan says where a take goes, the surface decides whether it can be read.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;

use naivepost::narrate_screen;
use naivepost::ui;

use crate::common;
use common::{fixture_dir, hold_last_window, release_last_window, settle};

static RAN_TAKE: AtomicBool = AtomicBool::new(false);
static RAN_WAVE: AtomicBool = AtomicBool::new(false);
static RAN_LOG: AtomicBool = AtomicBool::new(false);

/// A session folder of our own, so nothing written here lands in a real project or another test's.
fn scratch_root(tag: &str) -> std::path::PathBuf {
    let root = naivepost::startup::session_dir(
        &std::env::temp_dir().join(format!("np-ink-{}-{}", tag, std::process::id())),
    );
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("scratch session folder");
    std::env::set_current_dir(&root).expect("cwd pinned to this root");
    root
}

/// The page with `state` published, sized before `present()` so the paned lays out both columns. Built the
/// way the shell builds it, because the band's seconds come off the session folder the window pins.
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

/// A mono 16-bit PCM wav of `samples` silent samples at `rate`: what `voice_ref::wav_header` measures the
/// band's seconds from, and what `wave_build` folds, without ffmpeg.
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
    bytes.resize(44 + data_bytes as usize, 0);
    // Not all-zero: a flat fold reads as an empty lane, so every other bucket carries a tone burst that
    // `wave_build::peak_byte` lands well above zero. Deterministic, so no pixel comparison is flaky.
    let period = (rate / 200).max(1) as usize;
    for at in 0..samples as usize {
        if at % 200 < 100 {
            let phase = (at % period) as f64 / period as f64 * std::f64::consts::TAU;
            let sample = (i16::MAX as f64 * 0.5 * phase.sin()) as i16;
            let offset = 44 + at * 2;
            bytes[offset..offset + 2].copy_from_slice(&sample.to_le_bytes());
        }
    }
    std::fs::create_dir_all(path.parent().expect("a parent folder")).expect("folder for the wav");
    std::fs::write(path, bytes).expect("the wav is written");
}

/// One pixel of the flat RGB buffer `painted_pixels_for_test` hands back.
fn rgb(px: &[u8], width: i32, x: i32, y: i32) -> (u16, u16, u16) {
    let stride = width as usize * 3;
    let at = y as usize * stride + x as usize * 3;
    (px[at] as u16, px[at + 1] as u16, px[at + 2] as u16)
}

/// Is any pixel of these rows blue enough to be the envelope's own ink (89,153,229)?
fn any_blue(px: &[u8], width: i32, height: i32, from_y: i32) -> bool {
    (from_y..height).any(|y| {
        (0..width).any(|x| {
            let (r, _, b) = rgb(px, width, x, y);
            b > r + 60
        })
    })
}

#[test]
fn sec_07_narrate_1_screen_take_band_ink_the_lane_inks_its_takes_and_its_wave() {
    let _ = gtk::init();
    let app = adw::Application::builder()
        .application_id(ui::APP_ID)
        .flags(gtk4::gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.connect_activate(round);
    // One application, one main loop: GTK and libadwaita bind to the first thread that initialises them,
    // so every check is a stage of this single activate rather than a second `run()` on another thread.
    app.run_with_args::<&str>(&[]);
    assert!(RAN_TAKE.load(Ordering::SeqCst), "the take-ink check never ran");
    assert!(RAN_WAVE.load(Ordering::SeqCst), "the wave check never ran");
    assert!(RAN_LOG.load(Ordering::SeqCst), "the refusal-log check never ran");
}

fn round(app: &adw::Application) {
    let lane_h = narrate_screen::BAND_LANE_PX as i32;
    let ruler = narrate_screen::BAND_RULER_PX as i32;

    // --- a wide take is shaded, and its seconds can actually be READ -------------------
    {
        scratch_root("ink-take");
        let mut state = base_state();
        state.takes = vec![(1.0, 6.0)];
        ui::narrate_band::clear_hand();
        let window = narrate_page(app, state);
        // A 5 s take only earns its label once the lane is wide enough to show it, and the lane spans the
        // file it is drawn across — so this block first stands a real reference wav in its own session
        // folder (§F4.6 S3's output; the bench seeds one the same way for `just snapshot 07-narrate`), then
        // widens the view with the band's wheel controller, the door §1's "wheel zooms" describes.
        let tree = naivepost::ui::window::narrate_session_tree().expect("a session folder");
        write_wav(
            &naivepost::narrate_data::base_reference(&tree),
            16_000,
            45 * 16_000,
        );
        ui::refresh(&window);
        settle();
        let lane_widget = common::widget(&window, "take-band");
        let wheel = {
            // Walk the controller list rather than iterating it as a typed model: the list holds every
            // controller kind, and `iter::<T>` asserts its item type on the first one it meets.
            let held = lane_widget.observe_controllers();
            (0..held.n_items())
                .filter_map(|index| held.item(index))
                .find(|item| {
                    item.downcast_ref::<gtk::EventController>()
                        .is_some_and(|c| c.name().is_some_and(|n| n == "wheel-take-band"))
                })
                .and_then(|item| item.downcast::<gtk::EventController>().ok())
                .expect("the band carries its wheel controller")
        };
        for _ in 0..12 {
            // The band's own px/s, read back off the view it actually holds: `px_per_second` caps at
            // `narrate.bandMaxPps`, so a take can stop growing while the raw zoom keeps climbing, and a test
            // that watched the zoom would keep wheeling past the cap without the label ever getting wider.
            let (probe_seconds, probe_pps) = ui::narrate_band::painted_view_for_test();
            let probe_width = ui::narrate_band::view_width_for_test().max(500.0);
            let pps_probe = naivepost::narrate_band::px_per_second(
                ui::narrate_band::zoom(),
                probe_seconds.max(1.0),
                probe_width,
            );
            let (_, probe_w) = naivepost::narrate_band::take_box((1.0, 6.0), 0.0, pps_probe, 4000.0);
            if probe_w > 340.0 {
                break;
            }
            // `scroll` answers gboolean (GTK asks whether the event was eaten), so emit it as a bool.
            let _ = wheel.emit_by_name::<bool>("scroll", &[&0.0f64, &-50.0f64]);
            settle();
        }
        let (seconds, _) = ui::narrate_band::painted_view_for_test();
        assert!(seconds > 30.0, "the band is drawn across the 45 s reference: {seconds} s");
        let width = 500i32;
        // The take's box as THIS image draws it: `paint` fits the recording across whatever width it is
        // given, so px/s is the fit for this width and not the one the live lane happens to hold.
        let pps = naivepost::narrate_band::px_per_second(ui::narrate_band::zoom(), seconds, width as f64);
        // The box `paint` drew, asked of the rule with the numbers that paint used: offset 0 at this width.
        let (left, w) = naivepost::narrate_band::take_box((1.0, 6.0), 0.0, pps, width as f64);
        let label = naivepost::narrate_band::label_in((1.0, 6.0));
        assert_eq!(label, "5.0s", "§1: labelled with its DURATION ALONE");
        assert!(w > 34.0, "a take wider than 34 px earns its label: {w} px");
        // Move the cue bar clear of the label before painting: `paint` draws it THROUGH the lane, and this
        // state's playhead (5 s) falls inside the take, so it would land on the seconds §1 asks to read.
        // Through the band's own door — the same one a click on the lane fires.
        ui::narrate_band::set_red_bar(seconds - 1.0);
        let (px, pw, ph) = {
            let (px, pw, ph, _, _) =
                ui::narrate_band::painted_pixels_for_test(width, lane_h).expect("the lane painted whole");
            (px, pw, ph)
        };
        // The wash: green over the near-black ground, so g leads both of the others. Sampled across the
        // whole lane height, not one row: where the wave reaches its full height inside a take, blue fills
        // that column and the translucent green over it stays green-led but no longer passes a one-row test.
        let shaded = (((left as i32).max(0))..((left + w) as i32).min(pw)).any(|x| {
            ((ruler + 1)..lane_h).any(|y| {
                let (r, g, b) = rgb(&px, pw, x, y);
                g > r + 30 && g > b + 20
            })
        });
        assert!(shaded, "the take is SHADED across its own seconds ({left}..{} px)", left + w);
        // The label strip: §1's seconds have to survive the dark wash. Light ink is (240,242,247); the bug
        // was near-black letters, which leave this strip with no bright pixel at all. What else paints light
        // in those rows must be excluded, or a bare brightness count stands in for the label: the red ▶-start
        // bar ((219,51,46), bright in one channel only) and the blue envelope columns (89,153,229 — its b
        // leads r by ~140). Only near-white glyph ink survives both filters.
        let bright = (((left + 3.0) as i32).max(0))..(((left + 40.0).min((width - 1) as f64)) as i32);
        let bar = naivepost::narrate_band::second_to_px(5.0, 0.0, pps) as i32;
        let readable = (ruler + 3..ruler + 16)
            .flat_map(|y| bright.clone().map(move |x| (x, y)))
            .filter(|&(x, _)| (x - bar).abs() > 2)
            .filter(|&(x, y)| {
                let (r, g, b) = rgb(&px, pw, x, y);
                r >= 200 && g >= 200 && b >= 200 && b.abs_diff(r) <= 12
            })
            .count();
        assert!(
            readable >= 3,
            "the take's seconds are inked LIGHT ENOUGH TO READ on the wash: {readable} bright pixels \
             in the label strip of a {pw}×{ph} lane (take at {left}..{} px)",
            left + w
        );
        RAN_TAKE.store(true, Ordering::SeqCst);
        window.close();
        settle();
    }

    // --- the wave is drawn once its envelope is decoded ---------------------------------
    {
        scratch_root("ink-wave");
        let window = narrate_page(app, base_state());
        // Stage the two files the live path would build with the CONFIGURED ffmpeg (which may name a path
        // that is not on this machine): the 16 kHz decode marker short-circuits `decode_audio::ensure`, and
        // `wave_build::ensure` does the real fold into `cache/waves/<lane>.wave`. No ffmpeg runs here.
        let tree = naivepost::ui::window::narrate_session_tree().expect("a session folder");
        let reference = naivepost::narrate_data::base_reference(&tree);
        // 16 kHz is the decode's own rate, so `wave_build::SAMPLES_PER_BUCKET` divides it exactly.
        write_wav(&reference, 16_000, 45 * 16_000);
        let lane = format!("voice:{}", naivepost::narrate_data::take_base(&reference));
        let decoded = tree.voice16k_wav(&lane);
        write_wav(&decoded, 16_000, 45 * 16_000);
        naivepost::wave_build::ensure(&tree, &lane, &reference, &mut |_| {})
            .expect("the wave cache is folded from the staged decode");
        ui::refresh(&window);
        settle();
        let peaks = match ui::narrate_band::envelope_report_for_test(&ui::read_state()) {
            Ok((hz, peaks)) => {
                assert!(hz > 0.0, "the peaks arrive with their rate: {hz} per second");
                peaks
            }
            Err(why) => panic!("a staged envelope decodes instead of refusing: {why}"),
        };
        assert!(!peaks.is_empty(), "the cache holds peaks to draw");
        let (px, pw, ph, _, _) =
            ui::narrate_band::painted_pixels_for_test(500, lane_h).expect("the band painted");
        assert!(
            any_blue(&px, pw, ph, ruler + 1),
            "the decoded envelope is drawn as BLUE ink below the ruler, not as the waiting sentence"
        );
        // And the sentence is gone: its grey (191,199,204) belongs to a lane that has NO wave.
        let grey_rows = (ruler + 2..ruler + 14).any(|y| {
            (0..pw).any(|x| {
                let (r, g, b) = rgb(&px, pw, x, y);
                r.abs_diff(191) <= 6 && g.abs_diff(199) <= 6 && b.abs_diff(204) <= 6
            })
        });
        assert!(!grey_rows, "with a wave drawn, the lane prints no \"reading the recording…\" sentence");
        RAN_WAVE.store(true, Ordering::SeqCst);
        window.close();
        settle();
    }

    // --- an undecoded lane says WHY somewhere a person can read -------------------------
    {
        scratch_root("ink-bare");
        let window = narrate_page(app, base_state());
        assert!(
            ui::narrate_band::envelope_report_for_test(&ui::read_state()).is_err(),
            "nothing is staged, so the walk refuses rather than answering with an empty wave"
        );
        // Painting is what turns that refusal into a log line: §1 gives the lane one public sentence, so the
        // specifics have to go to the log or they exist nowhere (§00 failure-is-specific).
        let (px, pw, ph, _, _) =
            ui::narrate_band::painted_pixels_for_test(500, lane_h).expect("the band painted");
        assert!(
            ui::window::window_logs()
                .iter()
                .any(|line| line.starts_with("take band: ")),
            "the refusal reaches the log with its reason: {:?}",
            ui::window::window_logs()
        );
        assert!(
            !any_blue(&px, pw, ph, ruler + 1),
            "an undecoded lane draws no wave — there is nothing to draw yet"
        );
        RAN_LOG.store(true, Ordering::SeqCst);
        window.close();
        settle();
    }
}
