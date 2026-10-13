// F3.6 S5 (`applied in the preview even while paused`, `overlapping gains multiply`) — the gain reaches SOUND.
//
// `tests/volume_form_widgets.rs` proves the form: its five fields, the refusal, Apply's status line and one ↶
// taking the record back. `tests/cut_volume_hand.rs` proves `fx_volume`'s arithmetic. Neither proves the clause
// this file is about, because until now nothing consumed the answer at all: `ui::paused_preview_gain` had no
// caller, so a parked red line inside a raised passage computed 2.0 and told nobody. What closes that gap is
// `ui::cut_volume_hear::apply_paused_gain` handing the number to the footage leg, and the only honest proof of
// a hand is the receiving process: here a REAL player subprocess (the app's own media leg, `play_back`, which is
// `gst-play-1.0` asked for loudness through playbin's `--volume` PROPERTY) whose argv this test reads back.
//
// The seam is the program's own: `play_back::player_program()` reads `NAIVEPOST_PLAYER`, exactly as
// `tests/f2_2_spare_pipeline.rs` and `tests/f2_1_recording_play_widgets.rs` point it at a fake. No for-test door
// is consulted anywhere below — every assertion is a line the fake player wrote about itself.
//
// One `#[test]`, ordered blocks: the leg's slots (one running player, one volume stamp) and `NAIVEPOST_PLAYER`
// are process-wide, which is the arrangement those two files document for the same reason.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use adw::prelude::*;

use naivepost::cut::{Cut, Fx};
use naivepost::cut_hear;
use naivepost::cut_line::LinePos;
use naivepost::fx_volume;
use naivepost::play_back;
use naivepost::ui;

#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::{cut_page_unpainted as cut_page, entry, settle, status_text};

static RAN_PAUSED: AtomicBool = AtomicBool::new(false);
static RAN_MULTIPLY: AtomicBool = AtomicBool::new(false);
static RAN_CEILING: AtomicBool = AtomicBool::new(false);
static RAN_SILENT: AtomicBool = AtomicBool::new(false);
static RAN_UNDO: AtomicBool = AtomicBool::new(false);
static RAN_WIRE: AtomicBool = AtomicBool::new(false);

/// A second inside every passage this file builds, comfortably past any fade and before any fade-out.
const AT: f64 = 10.0;

/// Bounded wait for what a real process writes: no bare sleep, and no clock dependence — the deadline is a
/// ceiling, the loop exits the moment the line appears. Same shape as `f2_2_spare_pipeline::wait_until`.
fn wait_until(what: impl Fn() -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(4);
    while Instant::now() < deadline {
        if what() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    what()
}

/// The fake player: records its argv one argument per line, then stays alive answering SIGTERM the way the other
/// two player tests' fakes do (the `kill -0 $$` in the trap is not decoration — a shell defers a trapped signal
/// until the foreground command returns, so without it a replacement would read as still there).
fn fake_player(root: &Path, log: &Path) -> PathBuf {
    let script = root.join("volume-player.sh");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" >> '{}'\ntrap 'kill -0 $$; exit 0' TERM INT\nwhile :; do sleep 0.05; done\n",
            log.display()
        ),
    )
    .expect("the player stub");
    std::fs::set_permissions(&script, PermissionsExt::from_mode(0o755)).expect("+x");
    std::env::set_var("NAIVEPOST_PLAYER", &script);
    script
}

/// The tape the footage leg is cued with, and the ONE cue this round needs.
///
/// `play_back::set_volume` asks playbin's volume property by replacing the running player over the file list it
/// holds — so the app's own shape is "a player is already running when a gain arrives", which ▶ establishes here
/// before its tick hands one. A line move while paused therefore goes: cue (a player exists), then move the line
/// (the gain door asks that player).
fn cue_footage(tape: &str, at: f64) {
    play_back::cue(&[tape.to_string()], at, &|_| {}).expect("the footage leg started");
}

/// The `--volume` values the fake player has been asked for, in order, as written (`{:.3}`, three decimals —
/// the precision `play_back::volume_args` asks playbin's property in).
fn asked_volumes(log: &Path) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(log) else {
        return Vec::new();
    };
    let lines: Vec<&str> = text.lines().collect();
    lines
        .iter()
        .enumerate()
        .filter(|(_, arg)| **arg == "--volume")
        .filter_map(|(index, _)| lines.get(index + 1))
        .map(|value| value.to_string())
        .collect()
}

/// Wait for the ask this second deserves, and fail with what WAS asked if it never arrives: "nothing reached the
/// player" and "something else did" are different bugs, and a log tail says which.
fn wait_for_volume(log: &Path, want: &str) {
    let ok = wait_until(|| asked_volumes(log).last().map(String::as_str) == Some(want));
    assert!(
        ok,
        "the player was never asked for --volume {want}; the asks it did get were {:?}",
        asked_volumes(log)
    );
}

/// The gain the slider holds by default: full, i.e. `cut_hear::VOLUME_DEFAULT`, so a passage untouched by any
/// volume record is asked for at exactly that number.
fn slider_gain(window: &adw::ApplicationWindow) -> f64 {
    ui::preview_volume(window)
}

/// How the player must be asked for a gain: three decimals, the app's own rounding.
fn spoken(gain: f64) -> String {
    format!("{:.3}", gain)
}

/// A volume record over `AT`, built the way `fx_volume::apply` builds one (so the numbers are the record's, not
/// a fixture's), from a percent rather than a factor.
fn volume(percent: f64, dur: f64) -> Fx {
    fx_volume::apply(&fx_volume::Form {
        t: AT - 1.0,
        percent,
        dur,
        trans: 0.0,
        tout: 0.0,
        curve: String::new(),
        lane: String::new(),
    })
    .expect("a volume record over the parked second")
}

fn round(app: &adw::Application) {
    let root = naivepost::startup::session_dir(
        &std::env::temp_dir().join(format!("np-f36-sound-{}", std::process::id())),
    );
    std::fs::create_dir_all(&root).expect("root");
    std::env::set_current_dir(&root).expect("cwd pinned to this root");
    let log = root.join("player-argv.log");
    let tape = root.join("tape.mp4");
    std::fs::write(&tape, b"a recording the player is pointed at").expect("a footage file");
    fake_player(&root, &log);

    // --- S5: a PAUSED line inside a raised passage is heard at that loudness --------------------------
    // The window is on the Cut page with a plain cut; the footage leg is started through the program's own cue
    // door, and the transport is left NOT playing, which is the state §S5 names.
    let window = cut_page(app, &Cut::default());
    assert!(
        !ui::preview_player(&window).transport.playing,
        "this check is about the PAUSED case, so the transport must be still"
    );
    let tape_text = tape.display().to_string();
    cue_footage(&tape_text, AT);
    play_back::forget_volume_for_test();
    let flat = spoken(slider_gain(&window));
    // Baseline: with nothing over the second the ask is the slider's own level, and moving the line costs an ask
    // even there — otherwise the later assertions could pass on a stale stamp.
    ui::set_line_position(&window, LinePos { t: AT });
    settle();
    wait_for_volume(&log, &flat);
    let before = asked_volumes(&log).len();

    let mut cut_ = ui::review_cut_of(&window);
    cut_.fx.push(volume(200.0, 4.0));
    ui::seed_review_cut(&window, &cut_);
    // The line moves INSIDE the raised passage while paused: this is the press §S5 is about.
    ui::set_line_position(&window, LinePos { t: AT });
    settle();
    wait_for_volume(&log, &spoken(fx_volume::DEFAULT_GAIN));
    assert!(
        asked_volumes(&log).len() > before,
        "and it cost an ask: a gain that changed nothing on the wire never reached sound"
    );
    RAN_PAUSED.store(true, Ordering::SeqCst);

    // --- S5: overlapping gains MULTIPLY ---------------------------------------------------------------
    // A second record over the same seconds, deliberately starting earlier so the line's move alone cannot be
    // what changes the answer: 200 % × 150 % is 3.0, not 2.5 and not whichever was placed last.
    let mut both = ui::review_cut_of(&window);
    both.fx.push({
        let mut second = volume(150.0, 8.0);
        second.t = AT - 4.0;
        second
    });
    ui::seed_review_cut(&window, &both);
    ui::set_line_position(&window, LinePos { t: AT });
    settle();
    let product = fx_volume::DEFAULT_GAIN * fx_volume::gain_of_percent(150.0);
    wait_for_volume(&log, &spoken(product));
    // And the stamp: asking again for the same second with the same answer changes nothing on the wire, which is
    // what keeps a 100 ms tick from re-cueing ten times a second.
    let settled = asked_volumes(&log).len();
    ui::set_line_position(&window, LinePos { t: AT });
    settle();
    assert_eq!(
        asked_volumes(&log).len(),
        settled,
        "a repeat of the same answer must not reach the player again"
    );
    RAN_MULTIPLY.store(true, Ordering::SeqCst);

    // --- S5: the ceiling is playbin's, not the multiplication's --------------------------------------
    // Two big lifts would multiply past what the volume property takes; the ask stops at `cut_hear::MAX_GAIN`.
    let mut huge = ui::review_cut_of(&window);
    huge.fx.push(volume(1000.0, 6.0));
    ui::seed_review_cut(&window, &huge);
    ui::set_line_position(&window, LinePos { t: AT });
    settle();
    wait_for_volume(&log, &spoken(cut_hear::MAX_GAIN));
    RAN_CEILING.store(true, Ordering::SeqCst);

    // --- S5: silence is an ask too ---------------------------------------------------------------------
    // A passage recorded too loudly is turned down to nothing; §A.6 counts 0 % as silence, and silence that
    // never reaches the player is just a quieter bug.
    let mut quiet = ui::review_cut_of(&window);
    quiet.fx = vec![volume(0.0, 6.0)];
    ui::seed_review_cut(&window, &quiet);
    ui::set_line_position(&window, LinePos { t: AT });
    settle();
    wait_for_volume(&log, "0.000");
    RAN_SILENT.store(true, Ordering::SeqCst);

    // --- S5: ↶ puts the recorded loudness back ---------------------------------------------------------
    // Undo took the record away, so the second is the slider's own again. Asked through the same move-the-line
    // door, because that is the only way a person finds out.
    let mut plain = ui::review_cut_of(&window);
    plain.fx.clear();
    ui::seed_review_cut(&window, &plain);
    ui::set_line_position(&window, LinePos { t: AT });
    settle();
    wait_for_volume(&log, &flat);
    RAN_UNDO.store(true, Ordering::SeqCst);

    // --- the wire: Apply's percent is the number the player gets --------------------------------------
    // Mark a band, open the form the way the dropdown row does, type a percent into the field, click Apply by
    // name — and read the result off the player rather than off a slot. This is the half no widget test can see:
    // `volume_form_widgets.rs` already pins the status line and the record, so nothing is repeated here except
    // the sentence that says WHY the gain is safe to try, checked once to tie the two halves together.
    {
        let wired = cut_page(app, &Cut::default());
        let band = ui::draw_selection(
            &wired,
            naivepost::cut_select::Surface::SelectionBand,
            None,
            AT - 1.0,
            AT + 3.0,
        );
        assert!(band.is_some(), "the band was drawn");
        settle();
        ui::line_step_button(&wired, "effect-item-volume")
            .expect("the cut toolbar offers Volume")
            .emit_by_name::<()>("clicked", &[]);
        settle();
        let typed = 300.0;
        entry(&wired, "volume-field-percent").set_text(&format!("{typed:.0}"));
        ui::line_step_button(&wired, "volume-apply-button")
            .expect("the form drew its Apply button")
            .emit_by_name::<()>("clicked", &[]);
        settle();
        assert!(
            status_text(&wired).contains(fx_volume::PICTURE_UNTOUCHED),
            "Apply's own sentence ties the record to this file's claim: {}",
            status_text(&wired)
        );
        // The record exists (Apply's job) and the line lands inside it (this file's job): the percent typed into
        // the field is the number the player is asked for, multiplied by the slider it was applied under.
        let placed_cut = ui::review_cut_of(&wired);
        let placed = placed_cut
            .fx
            .iter()
            .find(|fx| fx.kind == "volume")
            .expect("Apply put a volume record on the cut");
        ui::set_line_position(&wired, LinePos { t: AT });
        settle();
        let expected = cut_hear::mix_gain(slider_gain(&wired), placed.gain);
        wait_for_volume(&log, &spoken(expected));
        assert!(
            (placed.gain - fx_volume::gain_of_percent(typed)).abs() < 1e-9,
            "the record carries the percent that was typed, not a rounded guess"
        );
        RAN_WIRE.store(true, Ordering::SeqCst);
        play_back::halt();
        wired.close();
    }

    play_back::halt();
    window.close();
}

#[test]
fn f3_6_s5_the_gain_reaches_sound() {
    // Pin cwd to our own temp root BEFORE anything builds: the press resolves the project through
    // `startup::session_dir(current_dir())`, and leaving it at `rust/` writes a stray session folder into the repo.
    let root = naivepost::startup::session_dir(
        &std::env::temp_dir().join(format!("np-f36-sound-{}", std::process::id())),
    );
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("root");
    std::env::set_current_dir(&root).expect("cwd pinned to this root");

    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(round);
        app.run();
    });

    let missing = [
        (RAN_PAUSED.load(Ordering::SeqCst), "paused"),
        (RAN_MULTIPLY.load(Ordering::SeqCst), "multiply"),
        (RAN_CEILING.load(Ordering::SeqCst), "ceiling"),
        (RAN_SILENT.load(Ordering::SeqCst), "silent"),
        (RAN_UNDO.load(Ordering::SeqCst), "undo"),
        (RAN_WIRE.load(Ordering::SeqCst), "wire"),
    ]
    .iter()
    .filter(|(ran, _)| !*ran)
    .map(|(_, name)| *name)
    .collect::<Vec<_>>();
    assert!(missing.is_empty(), "blocks that never ran: {missing:?}");
}
