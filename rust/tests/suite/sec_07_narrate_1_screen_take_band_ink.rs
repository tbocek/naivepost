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
    // The lane's own geometry, from the module that owns it: every block below paints a 500×lane image and
    // reads pixels below the ruler strip.
    let lane_h = narrate_screen::BAND_LANE_PX as i32;
    let ruler = narrate_screen::BAND_RULER_PX as i32;

    // --- a wide take is shaded, and its seconds can actually be READ -------------------
    {
        scratch_root("ink-take");
        let mut state = base_state();
        // A LONG take (20 s of the 45 s reference) so the view needs no zooming: at the band's fit view
        // `clamp_view` pins the offset to 0 and px/s is fixed by the fit, so in a 500 px image this take is
        // ~222 px wide — far more than the ~24 px its word needs. Wheeling a zoom to get there was tried and
        // cannot work from a test: `paint` maps with the live lane's px/s, which a test cannot know.
        state.takes = vec![(5.0, 25.0)];
        ui::narrate_band::clear_hand();
        let window = narrate_page(app, state);
        // The band has seconds only over a recording it can measure (§A.7), and its px/s comes from that
        // length, so stand a real 45 s reference wav in the session folder first (§F4.6 S3's output; the
        // bench seeds one the same way for `just snapshot 07-narrate`). No wave is staged, so the lane also
        // prints its "reading the recording…" sentence — grey (191,199,204), which fails the r >= 200 filter
        // below and so cannot be mistaken for glyph ink.
        let tree = naivepost::ui::window::narrate_session_tree().expect("a session folder");
        write_wav(
            &naivepost::narrate_data::base_reference(&tree),
            16_000,
            45 * 16_000,
        );
        ui::refresh(&window);
        settle();
        let width = 500i32;
        // Every geometry number comes from the band's own plan for THIS image: `painted_plan_for_test` paints
        // a scratch surface at exactly this width, at offset 0 like any standalone surface, and returns the
        // `Plan` `paint` built — so the box, the word and the cue column are the ones the ink used. Deriving
        // them by hand is what made this check sample pixels holding no part of the take.
        let plan = ui::narrate_band::painted_plan_for_test(width as f64, lane_h as f64);
        let (left, w, drawn_label) = plan
            .takes
            .first()
            .map(|held| (held.0, held.1, held.2.clone()))
            .expect("the one take this state holds is in the paint plan");
        assert!(
            w >= 100.0,
            "at fit this take is wide enough for its word — if this fails, the span is too short, not the \
             ink: {w} px in a {width} px image"
        );
        assert_eq!(
            naivepost::narrate_band::label_in((5.0, 25.0)),
            "20.0s",
            "§1: labelled with its DURATION ALONE"
        );
        assert_eq!(drawn_label, "20.0s", "and the plan drew exactly that word");
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
        // The label strip: §1's seconds have to survive the dark wash, and the bug this pins was near-black
        // letters, which leave it with no bright pixel at all. `paint` draws the word at `left + 3` on one
        // baseline, so the box to read is that and nothing wider — sampling from the take's own left edge
        // measured bare wash. Nothing else in those rows passes the strict near-white filter: the red ▶-start
        // bar is (219,51,46) with r leading, and the waiting sentence is grey at 191.
        let columns = (((left + 3.0) as i32).max(0))..(((left + 63.0).min((pw - 1) as f64)) as i32);
        let readable = (ruler + 3..ruler + 16)
            .flat_map(|y| columns.clone().map(move |x| (x, y)))
            .filter(|&(x, y)| {
                let (r, g, b) = rgb(&px, pw, x, y);
                r >= 200 && g >= 200 && b >= 200 && b.abs_diff(r) <= 12
            })
            .count();
        assert!(
            readable >= 3,
            "the take's seconds are inked LIGHT ENOUGH TO READ on the wash: {readable} bright pixels in the \
             label strip of a {pw}×{ph} lane (take at {left}..{} px)",
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
