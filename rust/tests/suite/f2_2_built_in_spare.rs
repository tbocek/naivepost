//! F2.2 S5 through the built-in player: a join is a SWAP of two prerolled pipelines, not a reload. First rewrite:
//! the built-in player had no spare, so a jump onto the next recording set the one pipeline to NULL, loaded the file,
//! prerolled it (at its first frame, which the sink showed), then seeked — and the preview flashed the next
//! recording's first picture at every cut that crossed files. Needs GStreamer with `playbin` and `gtk4paintablesink`.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use adw::prelude::*;
use naivepost::ui;

use crate::common;

/// Two stamped recordings, 24 s each at 25 fps, the second starting a second after the first ends on the session clock (24.003 s long: a recording that overlapped by those 3 ms would be coloured onto a second row).
const A: &str = "2026-10-18 19-21-52";
const B: &str = "2026-10-18 19-22-17";

fn clip(dir: &Path, name: &str, colour: &str) {
    let made = Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            &format!("color=c={colour}:size=160x90:rate=25:duration=24"),
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=24",
            "-shortest",
        ])
        .arg(dir.join("sources").join(format!("{name}.mkv")))
        .status()
        .expect("ffmpeg runs");
    assert!(made.success(), "the test clip {name} was written");
}

fn project(root: &Path) -> PathBuf {
    let dir = root.join("spare.naivepost");
    std::fs::create_dir_all(dir.join("sources")).unwrap();
    clip(&dir, A, "red");
    clip(&dir, B, "blue");
    std::fs::write(
        dir.join("naivepost.json"),
        format!(
            r#"{{"sources": [{{"path": "project:sources/{A}.mkv", "footage": true, "narrator": 1, "sepvoice": false, "tracks": []}}, {{"path": "project:sources/{B}.mkv", "footage": true, "narrator": 0, "sepvoice": false, "tracks": []}}]}}"#
        ),
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("cut")).unwrap();
    // Four clips, three joins: 12→14 inside A, 16→22 inside A, and 24→25 from A's last second onto B.
    std::fs::write(
        dir.join("cut/cut.json"),
        r#"{"segs": [{"s": 10.0, "e": 12.0}, {"s": 14.0, "e": 16.0}, {"s": 22.0, "e": 24.0}, {"s": 25.0, "e": 28.0}]}"#,
    )
    .unwrap();
    std::fs::write(dir.join("cut/line.json"), r#"{"t": 11.0}"#).unwrap();
    dir
}

fn wait_for(mut done: impl FnMut() -> bool, limit: Duration) -> bool {
    let started = Instant::now();
    while started.elapsed() < limit {
        if done() {
            return true;
        }
        common::settle();
        std::thread::sleep(Duration::from_millis(20));
    }
    done()
}

fn line() -> f64 {
    ui::line_position().t
}

#[test]
fn f2_2_s5_the_built_in_player_swaps_a_prerolled_pipeline_in_at_every_join() {
    common::pin_private_cwd();
    std::env::remove_var("NAIVEPOST_PLAYER");
    let root = common::temp_dir("built-in-spare");
    let dir = project(&root);
    common::one_app_round(move |app| {
        let loaded = naivepost::project::load(&dir).expect("the project loads");
        let window = ui::window::build_window_at(app, &loaded, "Cut", &dir);
        window.present();
        common::settle();
        assert!(
            naivepost::play_built_in::in_use(),
            "the built-in player plays here"
        );
        assert_eq!(line(), 11.0, "the line was restored at 11 s");
        let before = naivepost::play_built_in::swapped_for_test();

        let play_cut = ui::play_cut_button(&window).expect("\u{25b6}\u{2702} is on the page");
        play_cut.emit_clicked();
        let mut in_gap = Vec::new();
        let (mut at_seam, mut past_seam) = (None, None);
        let crossed = wait_for(
            || {
                let at = line();
                if (at > 12.3 && at < 13.7) || (at > 16.3 && at < 21.7) || (at > 24.3 && at < 24.7)
                {
                    in_gap.push(at);
                }
                if at >= 23.7 && at_seam.is_none() {
                    at_seam = Some(Instant::now());
                }
                if at >= 25.3 && past_seam.is_none() {
                    past_seam = Some(Instant::now());
                }
                at > 26.0
            },
            Duration::from_secs(25),
        );
        if !crossed {
            for entry in ui::window::logs_for_test() {
                if entry.contains("player") || entry.contains("preview") || entry.contains("spare")
                {
                    eprintln!("LOG {entry}");
                }
            }
            eprintln!(
                "STATE playing={} position={:?} file={:?} swapped={} transport={}",
                naivepost::play_built_in::playing(),
                naivepost::play_built_in::position(),
                naivepost::play_built_in::file(),
                naivepost::play_built_in::swapped_for_test(),
                ui::preview_player().transport.playing
            );
        }
        assert!(
            crossed,
            "\u{25b6}\u{2702} played on past the join onto the second recording: line {}",
            line()
        );
        assert!(
            in_gap.is_empty(),
            "the line never stood in a dropped stretch: {in_gap:?}"
        );
        let swapped = naivepost::play_built_in::swapped_for_test() - before;
        assert!(
            swapped >= 3,
            "every join was crossed by swapping the prerolled spare in, not by reloading: {swapped} swap(s) over three joins"
        );
        // 23.7 s to 25.3 s is 0.6 s of the CUT; a reload across files cost a third of a second more than that.
        let seam = past_seam.expect("past the seam") - at_seam.expect("at the seam");
        assert!(
            seam < Duration::from_millis(900),
            "the join onto the second recording took {seam:?}: the pipeline was reloaded, not swapped"
        );
        let player = ui::cut_record_play::video_second().expect("the player says where it is");
        assert!(
            (player - line()).abs() < 0.35,
            "the player stands where the line does after the swap: player {player}, line {}",
            line()
        );
        assert!(
            ui::cut_picture::video_shown(),
            "the picture is the swapped-in pipeline's"
        );
        play_cut.emit_clicked();
        common::settle();
        assert!(
            !naivepost::play_back::running(),
            "\u{25b6}\u{2702} again paused it"
        );
    });
}
