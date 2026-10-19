//! F2.1 — Play the recording (▶).
//!
//! What one press of ▶ on the Cut page means: switching back from a cut preview, pausing, or starting
//! at the red line and letting every second play, cuts and all. The rules live in [`naivepost::preview`],
//! so this checks them without a window, a player or a recording (spec/00-principles.md §5).

use naivepost::preview::{self as preview, Player, Press};
use naivepost::run::Transport;

#[test]
fn f2_1_s1_pressing_play_while_the_cut_is_previewed_switches_back_to_the_recording() {
    // S1: "the preview is the cut?" → "switch to the recording · 'preview is the recording again — …'".
    let runs = [(0.0, 60.0)];

    let mut player = Player { cut_only: true, playhead: Some(30.0), ..Default::default() };
    assert_eq!(
        preview::press_recording(&mut player, &runs),
        Press::SwitchedToRecording { kept_playing: false }
    );
    assert!(!player.cut_only, "the preview is the recording now");
    assert!(!player.transport.playing, "nothing was running, and nothing started");
    assert_eq!(
        preview::RECORDING_STATUS,
        "preview is the recording again \u{2014} everything plays, cuts and all"
    );

    // A review previews the cut too — it skips the removed stretches exactly as ▶✂ does.
    let mut reviewing = Player { reviewing: true, playhead: Some(30.0), ..Default::default() };
    assert_eq!(
        preview::press_recording(&mut reviewing, &runs),
        Press::SwitchedToRecording { kept_playing: false }
    );
    assert!(!reviewing.reviewing);

    // "already playing: carry on" — ▶ does not pause the preview it has just switched the view of.
    let mut running = Player {
        cut_only: true,
        reviewing: false,
        transport: Transport { playing: true, started: true },
        playhead: Some(30.0),
        volume: None,
        footage_muted: false,
    };
    assert_eq!(
        preview::press_recording(&mut running, &runs),
        Press::SwitchedToRecording { kept_playing: true }
    );
    assert!(running.transport.playing, "the switch carries on playing");
    assert!(!running.cut_only);

    // Switching costs the red line nothing: it is where the person left it either way.
    assert_eq!(running.playhead, Some(30.0));
}

#[test]
fn f2_1_s2_the_button_toggles_and_plays_from_the_red_line() {
    // S2: "Else toggle: playing → pause; else play from the red line."
    let runs = [(0.0, 60.0)];
    let mut player = Player { playhead: Some(12.5), ..Default::default() };

    assert_eq!(preview::press_recording(&mut player, &runs), Press::Playing { from: 12.5 });
    assert!(player.transport.playing && player.transport.started);

    // Pause: `playing` clears, `started` stays — the preview is the run bar's business until ⏹ (run.rs).
    assert_eq!(preview::press_recording(&mut player, &runs), Press::Paused);
    assert!(!player.transport.playing);
    assert!(player.transport.started);
    assert!(player.transport.cued(), "a paused preview still has something to resume");

    // And the next press resumes from the same line rather than from the top.
    assert_eq!(preview::press_recording(&mut player, &runs), Press::Playing { from: 12.5 });

    // No red line yet: the first thing filmed is where the recording starts.
    let mut fresh = Player::default();
    assert_eq!(
        preview::press_recording(&mut fresh, &[(5.0, 60.0)]),
        Press::Playing { from: 5.0 }
    );
    assert_eq!(fresh.playhead, Some(5.0));

    // Nothing filmed is nothing to play: the press refuses and changes nothing.
    let mut empty = Player::default();
    assert_eq!(preview::press_recording(&mut empty, &[]), Press::NoFootage);
    assert!(!empty.transport.playing);
    assert_eq!(empty.playhead, None, "no line was invented to play from");
}

#[test]
fn f2_1_s3_every_second_plays_cuts_and_all() {
    // S3: "While playing every second plays, cuts and all" — a line inside a stretch the cut removed
    // still walks straight through it. The cut here keeps one second of the hour.
    let removed_at = 40.0;
    assert_eq!(preview::play_advance(removed_at, 0.1, false), 40.1);

    // Ten ticks of a tenth of a second walk the session at walking pace: no skipping, no rounding away
    // a frame's worth per tick.
    let mut line = removed_at;
    for _ in 0..10 {
        line = preview::play_advance(line, 0.1, false);
    }
    assert!((line - 41.0).abs() < 1e-9, "ten tenths landed on {line}");

    // The ✂ argument changes nothing this round: F2.2 owns the skipping and its rule is not invented here.
    assert_eq!(preview::play_advance(removed_at, 0.1, true), preview::play_advance(removed_at, 0.1, false));
}

#[test]
fn f2_1_s4_running_into_a_fold_opens_it_and_says_so() {
    // S3 / §B: "▶ playing into a fold opens it ('unfolded m:ss — ▶ ran into it')".
    let playing = |at: f64| Player {
        transport: Transport { playing: true, started: true },
        playhead: Some(at),
        ..Default::default()
    };

    let mut folds = vec![[20.0, 30.0]];
    assert_eq!(preview::walk_fold(&playing(22.0), &mut folds), Some("unfolded 00:20 \u{2014} \u{25b6} ran into it".to_string()));
    assert!(folds.is_empty(), "the fold is gone, not merely skipped");

    // Short of the fold nothing opens.
    let mut folds = vec![[20.0, 30.0]];
    assert_eq!(preview::walk_fold(&playing(10.0), &mut folds), None);
    assert_eq!(folds, vec![[20.0, 30.0]]);

    // Paused, nothing runs into anything.
    let mut folds = vec![[20.0, 30.0]];
    let paused = Player { playhead: Some(22.0), ..Default::default() };
    assert_eq!(preview::walk_fold(&paused, &mut folds), None);
    assert_eq!(folds, vec![[20.0, 30.0]]);

    // And §B's other half: "▶✂ never enters one" — a fold is the person's own choice of what not to look
    // at, and the cut preview unfolding it would undo that under them.
    let mut folds = vec![[20.0, 30.0]];
    let cut_preview = Player { cut_only: true, playhead: Some(22.0), ..playing(22.0) };
    assert_eq!(preview::walk_fold(&cut_preview, &mut folds), None);
    assert_eq!(folds, vec![[20.0, 30.0]]);

    // A page with no line yet has nowhere to have run into anything from.
    let mut folds = vec![[20.0, 30.0]];
    assert_eq!(preview::walk_fold(&Player::default(), &mut folds), None);
    assert_eq!(folds.len(), 1);
}

#[test]
fn f2_1_s5_at_a_recordings_end_the_line_walks_on() {
    // S3: "at a recording's end the line walks on to the next one (or a camera still rolling then)".
    // §D's own order: "another camera rolling at this second, else the next recording start".
    let gap = [(0.0, 60.0), (65.0, 90.0)];
    assert_eq!(preview::walk_on(&gap, 60.0), Some(65.0), "the next recording's start");

    // Two cameras over the same minute: at the second one of them stops, the other is still rolling, so
    // the line stays where it is and the player re-cues onto that file.
    let two_cameras = [(0.0, 60.0), (0.0, 45.0)];
    assert_eq!(preview::walk_on(&two_cameras, 30.0), Some(30.0));

    // Past the last recording there is nothing to walk on to — §D's gap rule pauses, which is F2.2's.
    let one = [(0.0, 60.0)];
    assert_eq!(preview::walk_on(&one, 60.0), None);
    assert_eq!(preview::walk_on(&[], 0.0), None);

    // Inside a run the line is simply on footage: it stays.
    assert_eq!(preview::walk_on(&gap, 30.0), Some(30.0));
}

#[test]
fn f2_1_s6_the_clock_reads_session_time_ten_times_a_second() {
    // S4: "Clock shows session time; line follows the player ten times a second, smoothed live clock in
    // between." §D: '"mm:ss.d"; "--:--.-" with no playhead'.
    assert_eq!(preview::TICK_MS, 100);
    assert_eq!(preview::follows_per_second(), 10);

    assert_eq!(preview::clock(Some(75.34)), "01:15.3");
    assert_eq!(preview::clock(Some(0.0)), "00:00.0");
    assert!(preview::clock(Some(3661.0)).starts_with("61:01"), "minutes keep counting: {}", preview::clock(Some(3661.0)));
    assert_eq!(preview::clock(None), "--:--.-");

    // The live clock runs at the rate, no further than one tick, and never backwards.
    assert!((preview::live_clock(10.0, 40, 1.0) - 10.04).abs() < 1e-9);
    assert!((preview::live_clock(10.0, 900, 1.0) - 10.1).abs() < 1e-9, "capped at one tick");
    assert!(preview::live_clock(10.0, 40, -1.0) >= 10.0, "monotone");
    assert!((preview::live_clock(10.0, 40, 2.0) - 10.08).abs() < 1e-9, "at double speed");
}
