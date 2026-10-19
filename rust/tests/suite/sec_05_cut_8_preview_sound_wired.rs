//! §05-cut#8 bullet 2, the WIRE — `each change of the footage's own mute logs one line naming the reason`,
//! `… stream volume … is reset to full once at build`, and `a failing pipeline says so (… status "<page>
//! would not play — see log")`.
//!
//! tests/suite/cut_hear_mix.rs pins the WORDING of these sentences. What it could not pin, because nothing called
//! them, is that the program ever says them. This file drives the same doors the page drives — the tick's mix
//! seam, a real window build, a cue against a player that is not there — and asserts the line reaches the log
//! and the status row, which is the difference between a rule and a constant.
//!
//! One application, one `connect_activate`, one `#[test]`: these checks read the newest-window player slot,
//! so they run together and report through flags (the shape tests/suite/cut_hear_mix_widgets.rs uses).

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use naivepost::cut::Seg;
use naivepost::preview;
use naivepost::shell::Page;
use naivepost::ui;
use crate::common;
use common::{fixture_dir, settle};

static RAN_MUTE: AtomicBool = AtomicBool::new(false);
static RAN_RESET: AtomicBool = AtomicBool::new(false);
static RAN_FAILURE: AtomicBool = AtomicBool::new(false);

/// A window on the Cut page: the page that owns `preview-volume`, whose build therefore performs the
/// stream-volume reset this file checks, and whose mix seam is the door the tick drives.
fn cut_window(app: &adw::Application) -> adw::ApplicationWindow {
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();
    ui::tab_button(&window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_clicked();
    settle();
    window
}

/// Lines the shell's log holds right now.
fn log() -> Vec<String> {
    ui::window_logs()
}

/// The mute half: two ticks under a scene that does not hear the footage print ONE line, and the tick that
/// changes the cause prints the other one.
fn check_the_mute_change_logs_one_line_naming_its_reason(app: &adw::Application) {
    let window = cut_window(app);
    // A footage scene with the footage's own sound off: `cut_hear::footage_sound_heard` says `!mute`, so
    // this is the page's own answer for "the scene under the line does not hear it", not a scripted one.
    let silent = vec![Seg { s: 0.0, e: 60.0, mute: true, ..Default::default() }];
    let heard = vec![Seg { s: 0.0, e: 60.0, ..Default::default() }];
    ui::window::clear_logs_for_test();

    // Tick 1: the line moves under the silent scene. That is a CHANGE — the player started unmuted — so
    // exactly one line, and it names the scene as the cause.
    let first = ui::apply_mix(&window, &silent, &[], &[], 1.0);
    assert!(first.footage_muted, "the mix says the footage is silent here");
    let after_first: Vec<String> = log()
        .into_iter()
        .filter(|line| line.starts_with(">>> preview: the footage's own sound"))
        .collect();
    assert_eq!(
        after_first.len(),
        1,
        "one change, one line, got {after_first:?}"
    );
    assert_eq!(
        after_first[0],
        ">>> preview: the footage's own sound is muted -- the scene under the line does not hear it",
        "the line names WHICH cause silenced it"
    );

    // Ticks 2..6 under the SAME answer: nothing changed, so the log must not grow. This is the half that
    // makes it "one line per change" rather than one per 100 ms tick.
    for tick in 2..=6 {
        ui::apply_mix(&window, &silent, &[], &[], tick as f64);
    }
    let after_steady: Vec<String> = log()
        .into_iter()
        .filter(|line| line.starts_with(">>> preview: the footage's own sound"))
        .collect();
    assert_eq!(
        after_steady.len(),
        1,
        "five ticks under the same mute printed nothing more, got {after_steady:?}"
    );

    // The card over the picture is the OTHER reason, and it outranks the scene's own hearing because it is
    // the cause a person can see on the band: an OVERWRITING card (s == e) is heard but holds the footage,
    // so the scene under it says "heard" and only the card mutes — which is exactly the reason §8 makes it
    // name first.
    let card = vec![
        Seg { s: 0.0, e: 60.0, ..Default::default() },
        Seg { s: 2.0, e: 2.0, dur: 4.0, ins: "f2-12-sting.mp4".into(), ..Default::default() },
    ];
    assert!(
        naivepost::ui::preview_sound::card_holds_for_test(&window, &card, 2.0),
        "the card over that second holds the footage's own sound"
    );
    assert!(
        !naivepost::ui::preview_sound::card_holds_for_test(&window, &heard, 3.0),
        "no card, so a silence there can only be the scene's own"
    );
    ui::apply_mix(&window, &heard, &[], &[], 1.0);
    assert!(
        !ui::preview_player(&window).footage_muted,
        "a plain scene hears the footage, so the next mute has to name a cause"
    );
    // The card line must be counted on its own: the log already holds the scene line from further up.
    let said_before_card: usize = log()
        .iter()
        .filter(|line| line.contains("a card or a stop"))
        .count();
    assert_eq!(said_before_card, 0, "nothing has named the card yet");
    let heard_before_card: usize = log()
        .iter()
        .filter(|line| line.contains("is heard again"))
        .count();
    assert_eq!(
        heard_before_card, 1,
        "the step back to a hearing scene was itself the un-mute, and said so once"
    );
    ui::apply_mix(&window, &card, &[], &[], 2.0);
    assert!(
        ui::preview_player(&window).footage_muted,
        "the card's mute reached the player the rest of the app reads"
    );
    let by_card: Vec<String> = log()
        .into_iter()
        .filter(|line| line.contains("a card or a stop"))
        .collect();
    assert_eq!(
        by_card,
        vec![">>> preview: the footage's own sound is muted -- a card or a stop stands over the picture".to_string()],
        "the card names itself as the reason, and it outranks the scene's own hearing"
    );
    // Back to a plain heard scene with no card: that un-mute is announced too, and once only — the same
    // rule that kept five steady ticks silent, read from the other side.
    ui::apply_mix(&window, &heard, &[], &[], 3.0);
    let heard_again: Vec<String> = log()
        .into_iter()
        .filter(|line| line.contains("is heard again"))
        .collect();
    assert_eq!(
        heard_again.len(),
        heard_before_card + 1,
        "one more un-mute, one line, got {heard_again:?}"
    );
    assert_eq!(
        heard_again[0],
        ">>> preview: the footage's own sound is heard again",
        "and in the spec's own words"
    );
    drop(window);
    RAN_MUTE.store(true, Ordering::SeqCst);
}

/// The build half: opening a window put the stream volume to full on the element this app owns.
fn check_the_build_reset_the_stream_volume(app: &adw::Application) {
    let window = cut_window(app);
    assert!(
        preview::volume_is_app_owned(),
        "§8's premise: gain and mute go to an app-owned element"
    );
    assert_eq!(
        naivepost::ui::preview_sound::stream_gain_for_test(&window),
        Some(preview::STREAM_VOLUME),
        "the reset landed on the app's own gain slot, at full"
    );
    // And it is ONCE: a second window's build does not re-write it (§8: `reset to full once at build`, not
    // per play — a preview that set it every ▶ would leave last session's loudness behind for the desktop).
    let second = cut_window(app);
    assert_eq!(
        naivepost::ui::preview_sound::stream_gain_for_test(&second),
        Some(preview::STREAM_VOLUME),
        "a later window still reads full"
    );
    // The player's slot is the app's element: nothing here reaches a sound-server stream volume.
    assert!(
        preview::reset_stream_volume(true).is_none(),
        "the rule itself says the reset has already been made"
    );
    drop(second);
    drop(window);
    RAN_RESET.store(true, Ordering::SeqCst);
}

/// The failing-pipeline half: a cue against a player program that is not there logs the reason and the status
/// row says where to find it. `NAIVEPOST_PLAYER` is the seam the program itself resolves through
/// (`play_back::player_program`), so this is the real leg pointed at a missing binary, not a stand-in.
fn check_a_failing_pipeline_says_so(app: &adw::Application) {
    std::env::set_var("NAIVEPOST_PLAYER", "/nonexistent/naivepost-fake-player");
    assert_eq!(
        naivepost::play_back::player_program(),
        "/nonexistent/naivepost-fake-player",
        "the leg resolves the program through the same door this test points at"
    );
    let window = cut_window(app);
    ui::window::clear_logs_for_test();
    let cued = naivepost::ui::cut_cut_play::cue_running(&window, 1.0);
    assert!(!cued, "a player that is not there cannot be cued");
    let lines = log();
    assert!(
        lines.iter().any(|line| line.starts_with("!!! Cut: playback failed")),
        "the log holds the reason, got {lines:?}"
    );
    let status = ui::find_status(window.upcast_ref()).expect("the window has a status line");
    assert_eq!(
        status.text().as_str(),
        preview::playback_status("Cut"),
        "the status row points at the log with the spec's own sentence"
    );
    std::env::remove_var("NAIVEPOST_PLAYER");
    drop(window);
    RAN_FAILURE.store(true, Ordering::SeqCst);
}

#[test]
fn sec_05_cut_8_details_confirmed_against_the_code_verification_pass_s2_the_preview_says_its_own_sound() {
    common::pin_private_cwd();
    let app = adw::Application::builder()
        .application_id(ui::APP_ID)
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    let inner = app.clone();
    app.connect_activate(move |_app| {
        check_the_mute_change_logs_one_line_naming_its_reason(&inner);
        check_the_build_reset_the_stream_volume(&inner);
        check_a_failing_pipeline_says_so(&inner);
        inner.quit();
    });
    app.run_with_args::<String>(&[]);
    assert!(RAN_MUTE.load(Ordering::SeqCst), "the mute-change check ran");
    assert!(RAN_RESET.load(Ordering::SeqCst), "the stream-volume reset check ran");
    assert!(RAN_FAILURE.load(Ordering::SeqCst), "the failing-pipeline check ran");
}

/// The mix-lane half: the sentence a dropped lane says, pinned against the door the log now carries. The
/// failure itself (a spawn of a program that is not there) is what
/// tests/suite/f2_5_lane_pipelines.rs already drives through `lane_hear::apply_for_test`; what was missing, and
/// what codehalter named, was that the program ever used §8's sentence for it — so this pins that the door
/// `lane_hear::apply_with` calls is the spec's own, naming the recording.
#[test]
fn sec_05_cut_8_details_confirmed_against_the_code_verification_pass_s2_a_lane_that_drops_is_named() {
    common::pin_private_cwd();
    let base = "2026-09-16 17-26-20";
    let said = preview::mix_lane_failure(base, "no such file");
    assert_eq!(
        said,
        format!("!!! preview: {base} will not play \u{2014} no such file"),
        "the lane's own sentence names the recording that dropped out"
    );
    // And it is a different failure from a page that will not play at all: §0's local, specific failure.
    assert_ne!(said, preview::playback_status("Cut"));
    assert!(said.starts_with("!!! preview:"), "the log's own prefix");
}

/// The `Seg` import is what the card segments above are built from; this keeps the card-reason door honest
/// while it is at it.
const _: fn(&adw::ApplicationWindow, &[Seg], f64) -> bool =
    |window, segs, t| naivepost::ui::preview_sound::card_holds_for_test(window, segs, t);
