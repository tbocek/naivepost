//! F4.5 S3 at the PLAYER: which files and which second the picture's command line is built from.
//!
//! The rules module can say "ducked, hushed, rate 2" all day; what counts is that those numbers reach the
//! subprocess. So these checks read the argv the media layer builds — `--volume`, `-s` and the file list — for
//! cuts whose speed effect and camera assignment make the right answer differ from the naive one (the cut's own
//! second into a flat list of every recording).

use naivepost::cut::{Cut, Fx, Seg};
use naivepost::narrate_preview_media as media;

/// A footage clip off camera `cam`, kept in the cut.
fn clip(s: f64, e: f64, cam: i32) -> Seg {
    Seg {
        s,
        e,
        dur: e - s,
        cam,
        ..Default::default()
    }
}

/// A speed effect over `t..t+dur` playing at `rate`.
fn speed(t: f64, dur: f64, rate: f64) -> Fx {
    Fx {
        kind: "speed".into(),
        t,
        dur,
        rate,
        ..Default::default()
    }
}

/// S3: a stretched second must ask the player for twice the ground inside the recording.
///
/// `gst-play-1.0` has no rate flag (measured on this container's `--help-all`: only `--volume`, `-a`, `-i` and
/// `-s`), so converting the SECOND is the only way a speed effect reaches the cue at all — otherwise "speed
/// effects apply at seeks" would be true of the page's clock and false of the picture.
#[test]
fn f4_5_s3_the_cue_asks_the_player_for_the_footage_second() {
    let plain = Cut {
        segs: vec![clip(0.0, 20.0, 0)],
        ..Default::default()
    };
    // No effect: the distance into the clip, unchanged.
    assert_eq!(media::footage_second(&plain, 0.0), 0.0);
    assert_eq!(media::footage_second(&plain, 7.5), 7.5);

    // 2× over 4..14: five cut seconds past 9.0 are ten seconds of footage.
    let fast = Cut {
        segs: vec![clip(0.0, 20.0, 0)],
        fx: vec![speed(4.0, 10.0, 2.0)],
        ..Default::default()
    };
    assert_eq!(
        media::footage_second(&fast, 9.0),
        18.0,
        "2x covers twice the ground"
    );
    // Outside the effect the clip runs on its own clock again: rate 1 there.
    assert_eq!(media::footage_second(&fast, 2.0), 2.0);
    assert_eq!(media::footage_second(&fast, 18.0), 18.0);

    // Half speed: half the footage ground per cut second.
    let slow = Cut {
        segs: vec![clip(10.0, 30.0, 0)],
        fx: vec![speed(10.0, 20.0, 0.5)],
        ..Default::default()
    };
    assert_eq!(media::footage_second(&slow, 14.0), 2.0, "0.5x covers half");

    // A clip that plays its recording from partway in adds that offset to everything.
    let trimmed = Cut {
        segs: vec![Seg {
            ss: 12.0,
            ..clip(0.0, 20.0, 0)
        }],
        fx: vec![speed(0.0, 20.0, 2.0)],
        ..Default::default()
    };
    assert_eq!(
        media::footage_second(&trimmed, 3.0),
        18.0,
        "the recording's own start rides along: 3 x 2 + 12"
    );

    // A removed second has no recording under it: the target stands rather than a guess.
    let gap = Cut {
        segs: vec![clip(0.0, 10.0, 0)],
        ..Default::default()
    };
    assert_eq!(media::footage_second(&gap, 25.0), 25.0);
}

/// S3/S1: the cue starts at the recording that actually covers the target.
///
/// `-s` seeks into the FIRST file named, so with two cameras the covering one has to lead the list or the seek
/// lands in the wrong recording — 30 s into cam0 when the second belongs to cam1.
/// S3/S1: the cue starts at the recording that actually covers the target.
///
/// `-s` seeks into the FIRST file named, so with two cameras the covering one has to lead the list or the seek
/// lands in the wrong recording — 5 s into cam0 when the second belongs to cam1.
/// S3/S1: the cue starts at the recording that actually covers the target.
///
/// `-s` seeks into the FIRST file named, so with two cameras the covering one has to lead the list or the seek
/// lands in the wrong recording — 5 s into cam0 when the second belongs to cam1.
#[test]
fn f4_5_s3_the_cue_starts_at_the_recording_that_covers_the_target() {
    let root = std::env::temp_dir().join(format!("np-f45-bedcue-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let session = root.join("session.naivepost");
    std::fs::create_dir_all(&session).expect("session folder");
    std::env::set_current_dir(&root).expect("cwd pinned to the scenario root");
    for name in ["cam0.mp4", "cam1.mp4"] {
        std::fs::write(root.join(name), b"stub footage").expect("footage on disk");
    }
    let tree = naivepost::layout::Tree::new(&session).expect("a .naivepost folder is a project");
    naivepost::project::save(
        &naivepost::project::Project {
            sources: vec![
                naivepost::project::Source {
                    path: "cam0.mp4".into(),
                    footage: true,
                    ..Default::default()
                },
                naivepost::project::Source {
                    path: "cam1.mp4".into(),
                    footage: true,
                    ..Default::default()
                },
            ],
            ..Default::default()
        },
        tree.dir(),
    )
    .expect("project written");

    let cam0 = root.join("cam0.mp4").to_string_lossy().into_owned();
    let cam1 = root.join("cam1.mp4").to_string_lossy().into_owned();
    let cut = Cut {
        segs: vec![clip(0.0, 20.0, 0), clip(20.0, 40.0, 1)],
        fx: vec![speed(20.0, 20.0, 2.0)],
        ..Default::default()
    };

    // Inside cam0's clip: cam0 leads, and the second is the plain distance.
    let first = media::cue_files(&tree, &cut, 5.0);
    assert_eq!(first.first().map(String::as_str), Some(cam0.as_str()));
    assert_eq!(media::footage_second(&cut, 5.0), 5.0);

    // Inside cam1's clip AND inside a 2× stretch: cam1 leads so `-s` means "this second of cam1", and the
    // converted second is twice the distance into the clip.
    let second = media::cue_files(&tree, &cut, 25.0);
    assert_eq!(
        second.first().map(String::as_str),
        Some(cam1.as_str()),
        "the cue names the recording covering the target first, because -s seeks into the first file"
    );
    assert_eq!(
        media::footage_second(&cut, 25.0),
        10.0,
        "five cut seconds at 2x are ten seconds of cam1"
    );

    // The argv carries both numbers the rules answered: the duck/hush as `--volume`, the footage second as `-s`.
    let args = media::bed_argv(&second, media::footage_second(&cut, 25.0), 0.5);
    assert_eq!(args[0], "--volume=0.500");
    let position = args
        .iter()
        .position(|a| a == "-s")
        .expect("the seek flag is there");
    assert_eq!(args[position + 1], "10.0");
    assert_eq!(args.last().map(String::as_str), Some(cam0.as_str()));

    // A second outside every clip keeps the cut's own order: nothing covers it, so nothing is guessed.
    let outside = media::cue_files(&tree, &cut, 50.0);
    assert_eq!(
        outside.len(),
        2,
        "both recordings stay listed for a forward skip to land in"
    );
    assert_eq!(outside[0], cam0);
}
