//! F2.5 — the wire: a real drag of the Cut page's `preview-volume` slider reaches `cut_hear`, moving
//! the same one-number state tests/cut_hear_mix.rs asserts from the logic side, and the page's mix seam
//! hands back exactly what the rules return.
//!
//! One application, one `connect_activate`, exactly one `#[test]`: these checks mutate process-shared
//! state (this window's preview-volume slot and preview player), so they run once and report through
//! flags the test asserts on — the shape tests/cut_line_place_step_widgets.rs uses.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut::{Cut, Fx, Lane, Seg};
use naivepost::cut_hear::{self, LaneStart};
use naivepost::shell::Page;
use naivepost::ui;

static RAN_VOLUME: AtomicBool = AtomicBool::new(false);
static RAN_MIX: AtomicBool = AtomicBool::new(false);

fn fixture_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/demo.naivepost")
}

/// A window sitting on the Cut page, built fresh per check so one check's volume cannot leak into
/// another's assertions; the newest window is the one every accessor reads.
fn cut_window(app: &adw::Application) -> adw::ApplicationWindow {
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    assert!(
        model.sources.iter().any(|source| source.footage),
        "the fixture must carry a footage row or the Cut tab is locked (shell::lock) and this would \
         test the wrong page"
    );
    let window = ui::build_window(app, &model, "Prepare");
    window.present();
    ui::tab_button(&window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_clicked();
    assert_eq!(ui::state(&window).page, Page::Cut, "the Cut page is showing");
    settle();
    window
}

/// Let GTK finish laying out and mapping what was just shown. gtk4-rs 0.11 exposes no free
/// `events_pending`, so pump the GLib main context directly until the widgets are mapped.
fn settle() {
    let context = glib::MainContext::default();
    for _ in 0..64 {
        if !context.iteration(false) {
            break;
        }
    }
}

/// F2.5 S6: the slider is drawn, and dragging it hands ONE number to every preview.
fn check_the_volume_slider_hands_one_number_to_every_preview(app: &adw::Application) {
    let window = cut_window(app);

    // The real widget, by its stable name — not a value written straight to the seam.
    let scale = ui::preview_volume_scale(&window).expect("the Cut page carries preview-volume");
    assert!(scale.is_mapped(), "the slider is mapped on the visible Cut page");
    println!(
        "LAYOUT preview-volume width={} height={} mapped={}",
        scale.width(),
        scale.height(),
        scale.is_mapped()
    );
    // Measured, not assumed: a GTK4 horizontal Scale laid out in this page comes to ~96 x 10 px even
    // though 120 was requested (the trough does not fill the request). What matters for F2.5 S6 is that
    // it is drawn and grabbable, so assert against the measured floor, not the requested number.
    assert!(
        scale.width() > 60,
        "slider wide enough to drag, got {} px",
        scale.width()
    );
    assert!(
        scale.height() >= 8,
        "slider thick enough to grab, got {} px",
        scale.height()
    );
    assert_eq!(
        scale.tooltip_text().map(|t| t.to_string()),
        Some(cut_hear::VOLUME_TIP.to_string()),
        "the tooltip is the shared sentence, verbatim, so it cannot drift from the module's"
    );
    // The label beside the trough, read off the widget tree. A tooltip only says what a control is to a
    // pointer that already found it; the word next to the slider is what makes it findable, and this
    // checks that word is in the tree AND drawn -- not merely constructed somewhere.
    let row = scale
        .parent()
        .expect("the slider sits in a labelled row, not loose in the page column");
    let row = row.downcast::<gtk::Box>().expect("the volume row is a horizontal Box");
    let label = row
        .first_child()
        .and_then(|c| c.downcast::<gtk::Label>().ok())
        .expect("the first thing in the volume row is the label");
    println!(
        "LABEL {} mapped={}",
        label.text(),
        label.is_mapped()
    );
    assert_eq!(
        label.text().as_str(),
        cut_hear::VOLUME_LABEL,
        "the word beside the slider is the module's own, so the two cannot disagree"
    );
    assert!(label.is_mapped(), "the label is actually drawn, not just built");
    println!("ROW height={}", row.height());
    assert!(
        row.height() >= 24,
        "the row holds the 24 px floor so it reads as a control, got {} px",
        row.height()
    );
    assert_eq!((scale.adjustment().upper(), scale.adjustment().step_increment()), (100.0, 1.0),
        "0..100 at step 1 — what a slider reads as, while the property under it is 0..1");

    // Untouched means full: an unset control is audible, not mute.
    assert_eq!(ui::preview_volume(&window), cut_hear::VOLUME_DEFAULT, "an untouched slider reads full");
    assert_eq!(ui::preview_player(&window).volume, Some(cut_hear::VOLUME_DEFAULT),
        "and the player starts at that same one number");

    // Drag it to 40 the way a drag does: setting the widget's value fires `value-changed` for real, and
    // the wired handler forwards to `set_preview_volume`.
    scale.set_value(40.0);
    settle();
    assert_eq!(
        ui::preview_volume(&window),
        cut_hear::mix_gain(0.4, 1.0),
        "40% lands as the gain the mix rule gives, 0.4 — not 40, not 0.004"
    );
    assert!(
        (ui::preview_volume(&window) - 0.4).abs() < 1e-9 && cut_hear::clamp_volume(0.4) == 0.4,
        "inside the clamp, unchanged"
    );
    assert_eq!(
        ui::preview_player(&window).volume,
        Some(ui::preview_volume(&window)),
        "the live player carries the identical number — one volume, every preview"
    );

    // A second move proves it is not a one-shot and that both ends of the clamp hold.
    scale.set_value(100.0);
    settle();
    assert_eq!(ui::preview_volume(&window), 1.0, "full travel is full gain");
    assert_eq!(ui::preview_player(&window).volume, Some(1.0), "and the player follows again");
    scale.set_value(-5.0);
    settle();
    assert_eq!(
        ui::preview_volume(&window),
        cut_hear::clamp_volume(-0.05),
        "below nought clamps to silence, never a phase flip"
    );
    assert_eq!(ui::preview_player(&window).volume, Some(0.0), "silence reaches the player too");

    window.close();
}

/// F2.5 S1/S2 through the page's own seam: a louder mix still refuses a lane the scene silences.
fn check_the_page_seam_refuses_a_silenced_lane_whatever_the_gain(app: &adw::Application) {
    let window = cut_window(app);
    let hushed = "2026-09-16 17-26-20";
    let mut cut = Cut::default();
    // One scene that hears nothing from that lane, with its own sound left on.
    cut.segs.push(Seg {
        s: 30.0,
        e: 70.0,
        quiet: vec![hushed.into()],
        ..Default::default()
    });
    // Placed so the session second used below (40.0) falls inside the scene AND inside the file: at 30 s
    // of the session the lane's own clock is at 0, running 45 s. A lane stamped outside the scene would
    // answer `Nothing` rather than `Silenced`, which is a different rule and not the one under test.
    let lanes = [Lane {
        name: hushed.into(),
        src: "project:audio/2026-09-16 17-26-20.wav".into(),
        at: 30.0,
        off: 0.0,
        dur: 45.0,
    }];
    let fx: [Fx; 0] = [];

    // Loud on purpose: turning the room up must not buy back a lane the scene silenced.
    ui::set_preview_volume(&window, 100.0);
    let loud = ui::apply_mix(&window, &cut.segs, &lanes, &fx, 40.0);
    assert_eq!(loud.gain, 1.0, "the slider is at full, so the gain is full");
    assert_eq!(
        loud.lane_starts.as_slice(),
        &[(hushed.to_string(), LaneStart::Silenced)][..],
        "yet the silenced lane is refused — never started, whatever the gain"
    );
    assert!(!loud.footage_muted, "the scene leaves its own picture sound on, so it is not muted");

    // Same call, same second, scene mutes its own sound instead: only that flag moves.
    cut.segs[0].quiet.clear();
    cut.segs[0].mute = true;
    let muted = ui::apply_mix(&window, &cut.segs, &lanes, &fx, 40.0);
    assert!(muted.footage_muted, "S2: the footage's own sound is muted here");
    assert!(
        matches!(muted.lane_starts[0].1, LaneStart::Seek { .. }),
        "and the lane, no longer silenced, is asked to play"
    );
    assert_eq!(
        ui::preview_player(&window).footage_muted,
        muted.footage_muted,
        "the answer lands on the player, which is where the pipeline will read it"
    );

    // A second outside every scene answers with no new say rather than inventing a silence.
    let nowhere = ui::apply_mix(&window, &cut.segs, &lanes, &fx, 5.0);
    assert!(nowhere.lane_starts.is_empty(), "no scene decides, so no lane is answered for");
    assert!(!nowhere.footage_muted, "and nothing is muted by a scene that isn't there");

    window.close();
}

fn window_round() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(|app| {
            check_the_volume_slider_hands_one_number_to_every_preview(app);
            RAN_VOLUME.store(true, Ordering::SeqCst);
            check_the_page_seam_refuses_a_silenced_lane_whatever_the_gain(app);
            RAN_MIX.store(true, Ordering::SeqCst);
            app.quit();
        });
        app.run_with_args::<String>(&[]);
    });
}

#[test]
fn f2_5_s6_the_preview_volume_control_hands_one_number_to_every_preview() {
    window_round();
    assert!(RAN_VOLUME.load(Ordering::SeqCst), "the slider check never ran");
    assert!(RAN_MIX.load(Ordering::SeqCst), "the mix-seam check never ran");
}
