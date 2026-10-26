//! §07-narrate#1-screen **8** ▶ on the take band: it plays the RECORDING's own seconds from the red bar, take by
//! take: the bar inside a take plays to that take's end, then the next take, the red bar walking with it and
//! jumping over what is not picked; ▶ wears ⏹ meanwhile (prototype `playClicked` / `takeQueue`). The first
//! rewrite played the built reference file, which does not exist before a line is spoken, so ▶ did nothing.

use std::path::Path;
use std::time::{Duration, Instant};

use adw::prelude::*;
use naivepost::ui;

use crate::common;

/// A player that notes its arguments and plays for exactly the `-t` it was given.
fn fake_player(root: &Path) -> (std::path::PathBuf, std::path::PathBuf) {
    std::fs::create_dir_all(root).unwrap();
    let log = root.join("player.log");
    let script = root.join("fake-voice-player");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\necho \"$@\" >> '{}'\nlen=2\nwhile [ $# -gt 0 ]; do [ \"$1\" = -t ] && len=$2; shift; done\nsleep $len\n",
            log.display()
        ),
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    (script, log)
}

#[test]
fn sec_07_narrate_1_screen_take_band_play_walks_the_takes_from_the_red_bar() {
    common::pin_private_cwd();
    let root = common::temp_dir("take-walk");
    let (player, log) = fake_player(&root);
    std::env::set_var("NAIVEPOST_VOICE_PLAYER", &player);
    let dir = root.join("walk.naivepost");
    std::fs::create_dir_all(dir.join("sources")).unwrap();
    std::fs::write(dir.join("sources/voice.mkv"), b"stub").unwrap();
    std::fs::write(
        dir.join("naivepost.json"),
        r#"{"sources": [{"path": "project:sources/voice.mkv", "footage": true, "narrator": 1, "sepvoice": false, "tracks": []}]}"#,
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("cut")).unwrap();
    std::fs::write(
        dir.join("cut/cut.json"),
        r#"{"segs": [{"s": 0.0, "e": 6.0}]}"#,
    )
    .unwrap();
    common::one_app_round(move |app| {
        let loaded = naivepost::project::load(&dir).expect("the project loads");
        let window = ui::window::build_window_at(app, &loaded, "Narrate", &dir);
        window.present();
        common::settle();
        let mut state = ui::narrate_page::read_state();
        state.takes = vec![(0.5, 2.0), (4.0, 5.5)];
        ui::narrate_page::set_state(state);
        ui::narrate_band::set_red_bar(0.7);
        let face = || {
            ui::line_step_button(&window, "take-play")
                .and_then(|button| button.label())
                .unwrap_or_default()
                .to_string()
        };

        let said = ui::narrate_page_take::press_take_play(&window);
        assert_eq!(
            said, "playing 2 take(s), 2.8 s from 00:00",
            "two takes from the bar, the first trimmed to it"
        );
        assert_eq!(
            face(),
            "\u{23f9}",
            "\u{25b6} wears \u{23f9} while the takes play"
        );
        let mut bars = Vec::new();
        let started = Instant::now();
        while ui::narrate_page_take::walking() && started.elapsed() < Duration::from_secs(12) {
            bars.push(ui::narrate_band::walk_head());
            common::settle();
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(
            !ui::narrate_page_take::walking(),
            "the walk ended on its own"
        );
        assert_eq!(
            ui::narrate_band::red_bar(),
            0.7,
            "the red bar stays where \u{25b6} starts"
        );
        assert!(
            ui::narrate_band::walk_head() < 0.0,
            "and the walk's playhead is gone"
        );
        assert!(
            bars.iter().all(|at| !(2.05..3.95).contains(at)),
            "the playhead jumped over the seconds between the takes: {bars:?}"
        );
        assert!(
            bars.iter().any(|at| (0.75..=2.0).contains(at)),
            "it walked through the first take: {bars:?}"
        );
        assert!(
            bars.iter().any(|at| (4.0..=5.5).contains(at)),
            "and through the second: {bars:?}"
        );
        assert_eq!(face(), "\u{25b6}", "and \u{25b6} again after the last take");
        let calls = std::fs::read_to_string(&log).unwrap_or_default();
        let lines: Vec<&str> = calls.lines().collect();
        assert_eq!(lines.len(), 2, "one play per take: {calls}");
        // The stub is spoken to as the default `gst-play-1.0` (its own flags, §07 Preview): a start and no
        // length, the band's own clock ending each take where it ends.
        assert!(
            lines[0].contains("-s 0.700") && !lines[0].contains("-t "),
            "the first from the bar, ended by the clock: {}",
            lines[0]
        );
        assert!(
            lines[1].contains("-s 4.000") && !lines[1].contains("-t "),
            "then the next take whole: {}",
            lines[1]
        );
        assert!(
            lines[0].contains("sources/voice.mkv"),
            "the recording itself is played: {}",
            lines[0]
        );
        window.close();
    });
}

#[test]
fn sec_07_narrate_1_screen_take_queue_follows_the_prototype() {
    use naivepost::narrate_screen::take_queue;
    let takes = [(0.5, 1.0), (2.0, 2.6)];
    assert_eq!(
        take_queue(&takes, 0.7, 10.0),
        (vec![(0.7, 1.0), (2.0, 2.6)], true),
        "inside a take: from the bar"
    );
    assert_eq!(
        take_queue(&takes, 1.5, 10.0),
        (vec![(2.0, 2.6)], true),
        "between takes: the next one"
    );
    assert_eq!(
        take_queue(&takes, 3.0, 10.0),
        (vec![(3.0, 10.0)], false),
        "past the last: the recording from the bar"
    );
    assert_eq!(
        take_queue(&[], 4.0, 10.0),
        (vec![(4.0, 10.0)], false),
        "no takes: the recording from the bar"
    );
    assert_eq!(
        take_queue(&[], 10.0, 10.0),
        (vec![], false),
        "at the end: nothing"
    );
}
