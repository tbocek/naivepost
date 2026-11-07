//! F5.2 — the bar while the clips encode. First rewrite: it moved by clip count only, reported before each clip, so
//! "encoding clip 1 of 64" took the pulse away and painted 0 % for the whole first clip, and every later clip added a
//! 64th: the bar looked gone. Now each clip's share is its time on screen, filled by the stats ffmpeg prints as it goes.

use naivepost::run_stages;
use naivepost::subprocess::{run_listed_timed, stats_time};

#[test]
fn f5_2_the_seconds_written_are_read_off_ffmpegs_stats_line() {
    let line = "frame=  301 fps=120 q=-1.0 size=    1024KiB time=00:01:02.50 bitrate=1342.2kbits/s speed=4.1x";
    assert_eq!(stats_time(line), Some(62.5));
    assert_eq!(
        stats_time("frame=0 time=N/A bitrate=N/A"),
        None,
        "nothing written yet"
    );
    assert_eq!(
        stats_time("time=-00:00:00.02"),
        Some(0.0),
        "a start before the first frame is nought"
    );
    assert_eq!(stats_time("Input #0, matroska,webm, from 'a.mkv':"), None);
}

#[test]
fn f5_2_a_clip_fills_its_own_share_of_the_bar_as_it_encodes() {
    // The second of three clips: 10 s of 40 on screen, after 20 s of clips already done.
    run_stages::encoding("encoding clip 2 of 3", 0.5, 0.25, 10.0);
    run_stages::encoded_to(4.0);
    let (status, fraction) = run_stages::take_progress().expect("a report");
    assert_eq!(status, "encoding clip 2 of 3");
    assert!((fraction.unwrap() - 0.6).abs() < 1e-9, "{fraction:?}");
    run_stages::encoded_to(99.0);
    assert!(
        (run_stages::take_progress().unwrap().1.unwrap() - 0.75).abs() < 1e-9,
        "never past its own share"
    );
    // After the encodes, the join's and the mux's stats move nothing.
    run_stages::encoding_done();
    run_stages::encoded_to(5.0);
    assert_eq!(run_stages::take_progress(), None);
}

#[test]
fn f5_2_a_running_ffmpeg_reports_its_time_as_it_goes() {
    if std::process::Command::new("ffmpeg")
        .arg("-version")
        .output()
        .is_err()
    {
        eprintln!("no ffmpeg here: not checked");
        return;
    }
    let dir = crate::common::temp_dir_created("encode-progress");
    let out = dir.join("c.mkv");
    let mut times = Vec::new();
    let args = [
        "-y",
        "-f",
        "lavfi",
        "-i",
        "testsrc2=s=640x360:r=30:d=6",
        "-c:v",
        "libx264",
        "-preset",
        "veryslow",
        out.to_str().unwrap(),
    ];
    run_listed_timed("ffmpeg", &args, |_| {}, |t| times.push(t)).expect("ffmpeg ran");
    assert!(!times.is_empty(), "the stats line was read while it ran");
    assert!(
        times.windows(2).all(|w| w[1] >= w[0]),
        "and only ever forward: {times:?}"
    );
    assert!(
        (times.last().unwrap() - 6.0).abs() < 0.2,
        "ending at the clip's length: {times:?}"
    );
}
