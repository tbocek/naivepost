//! Where footage, cards and lines land in the finished video.

use naivepost::cut::{keep_inserts, spliced, Fx, Seg};
use naivepost::cut_suggest::drop_unfilmed;
use naivepost::narration::Entry;
use naivepost::produce_fx_chain::for_clip;
use naivepost::produce_render::{plan_clips, produced_clocks, Clip};

fn seg(s: f64, e: f64) -> Seg {
    Seg {
        s,
        e,
        ..Default::default()
    }
}

fn card(at: f64) -> Seg {
    Seg {
        s: at,
        e: at,
        dur: 3.0,
        ins: "card.svg".into(),
        ..Default::default()
    }
}

fn spans(segs: &[Seg]) -> Vec<(f64, f64)> {
    segs.iter().map(|seg| (seg.s, seg.e)).collect()
}

#[test]
fn a_card_cuts_open_the_footage_it_lands_inside() {
    let out = spliced(vec![seg(0.0, 10.0), seg(10.0, 20.0), card(5.0)]);
    assert_eq!(
        spans(&out),
        [(0.0, 5.0), (5.0, 5.0), (5.0, 10.0), (10.0, 20.0)]
    );
    assert!(out[1].is_insert());
}

#[test]
fn a_card_on_a_border_cuts_nothing() {
    let out = spliced(vec![seg(0.0, 10.0), seg(10.0, 20.0), card(10.0)]);
    assert_eq!(spans(&out), [(0.0, 10.0), (10.0, 10.0), (10.0, 20.0)]);
}

#[test]
fn a_picture_laid_over_takes_its_seconds_from_a_suggestion() {
    let over = Seg {
        s: 10.0,
        e: 14.0,
        ins: "a.png".into(),
        ..Default::default()
    };
    let out = keep_inserts(&[seg(0.0, 30.0)], &[over]);
    assert_eq!(spans(&out), [(0.0, 10.0), (10.0, 14.0), (14.0, 30.0)]);
}

#[test]
fn a_sound_laid_over_takes_its_seconds_too() {
    let over = Seg {
        s: 10.0,
        e: 14.0,
        ins: "a.wav".into(),
        ..Default::default()
    };
    let out = keep_inserts(&[seg(0.0, 30.0)], &[over]);
    assert_eq!(spans(&out), [(0.0, 10.0), (10.0, 14.0), (14.0, 30.0)]);
}

#[test]
fn a_scene_ending_on_a_recordings_end_is_filmed() {
    let (kept, dropped) = drop_unfilmed(vec![(5.0, 100.0)], &[(0.0, 100.0)]);
    assert_eq!((kept, dropped), (vec![(5.0, 100.0)], 0));
    let (kept, dropped) = drop_unfilmed(vec![(50.0, 120.0)], &[(0.0, 100.0)]);
    assert_eq!((kept.len(), dropped), (0, 1));
}

#[test]
fn clocks_add_what_each_clip_shows() {
    let clips = [
        Clip {
            on_screen: 10.0,
            rate: 1.0,
            ..Default::default()
        },
        Clip {
            on_screen: 5.0,
            rate: 2.0,
            ..Default::default()
        },
        Clip {
            on_screen: 10.0,
            rate: 1.0,
            ..Default::default()
        },
    ];
    let starts: Vec<f64> = produced_clocks(&clips).iter().map(|c| c.0).collect();
    assert_eq!(starts, [0.0, 10.0, 15.0]);
}

#[test]
fn a_line_is_spoken_once_in_the_split_piece_holding_it_at_that_pieces_clock() {
    let line = Entry {
        s: 0.0,
        e: 30.0,
        at: 14.0,
        text: "words".into(),
        ..Default::default()
    };
    let pieces = [
        seg(0.0, 10.0),
        Seg {
            rate: 2.0,
            ..seg(10.0, 20.0)
        },
        seg(20.0, 30.0),
    ];
    let (clips, _) = plan_clips(
        &pieces,
        |_| Vec::new(),
        "16:9",
        |_| true,
        &[],
        &[line],
        (1920, 1080),
        false,
        |_| 0.0,
    );
    let held: Vec<usize> = clips.iter().map(|clip| clip.lines.len()).collect();
    assert_eq!(held, [0, 1, 0]);
    assert_eq!(clips[1].lines[0].at, 2.0, "4 s of footage into a 2× piece");
}

fn text_on(clip_s: f64, length: f64) -> String {
    let text = Fx {
        kind: "text".into(),
        t: 5.0,
        dur: 10.0,
        trans: 0.5,
        tout: 0.5,
        text: "words".into(),
        ..Default::default()
    };
    let clip = Clip {
        seg_s: clip_s,
        on_screen: length,
        rate: 1.0,
        frame: (1920, 1080),
        ..Default::default()
    };
    for_clip(&clip, &[text], (1920, 1080), std::path::Path::new("/tmp"))
        .overlays
        .iter()
        .map(|overlay| format!("{};{}", overlay.prep.join(";"), overlay.place))
        .collect::<Vec<_>>()
        .join(";")
}

#[test]
fn a_text_box_runs_across_a_border_with_one_fade_at_each_end() {
    let first = text_on(0.0, 10.0);
    assert!(first.contains("between(t\\,5\\,10)"), "{first}");
    assert!(first.contains("fade=t=in:st=5:d=0.5"), "{first}");
    assert!(
        !first.contains("fade=t=out"),
        "no fade at the border: {first}"
    );
    let second = text_on(10.0, 10.0);
    assert!(second.contains("between(t\\,0\\,5)"), "{second}");
    assert!(
        !second.contains("fade=t=in"),
        "no fade at the border: {second}"
    );
    assert!(second.contains("fade=t=out:st=4.5:d=0.5"), "{second}");
}

#[test]
fn a_text_box_stays_off_clips_it_does_not_reach() {
    assert_eq!(text_on(20.0, 10.0), "");
    assert_eq!(text_on(0.0, 3.0), "");
}

/// How bright the brightest pixel of a clip is at `at`: white words on black footage read near 255.
fn brightest(file: &std::path::Path, at: f64) -> u8 {
    let out = std::process::Command::new("ffmpeg")
        .args(["-v", "error", "-ss", &format!("{at}"), "-i"])
        .arg(file)
        .args(["-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "gray", "-"])
        .output()
        .expect("ffmpeg runs");
    assert!(!out.stdout.is_empty(), "no frame at {at}");
    out.stdout.into_iter().max().unwrap_or(0)
}

/// Real encodes, measured off their frames.
#[test]
fn the_encoded_clips_show_the_words_only_while_the_band_runs() {
    let dir = std::env::temp_dir().join(format!("fx-across-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let source = dir.join("black.mp4");
    let made = std::process::Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-f", "lavfi", "-i"])
        .args([
            "color=black:s=640x360:r=10:d=30",
            "-f",
            "lavfi",
            "-i",
            "sine=d=30",
        ])
        .args(["-pix_fmt", "yuv420p", "-shortest"])
        .arg(&source)
        .status()
        .unwrap();
    assert!(made.success());
    let text = Fx {
        kind: "text".into(),
        t: 5.0,
        dur: 10.0,
        text: "WORDS".into(),
        ..Default::default()
    };
    let settings = naivepost::project::Produce {
        blurred_edges: false,
        ..Default::default()
    };
    let encode = |seg_s: f64| {
        let clip = Clip {
            no: seg_s as usize + 1,
            seg_s,
            on_screen: 10.0,
            rate: 1.0,
            frame: (640, 360),
            src: (640, 360),
            fx: vec![text.clone()],
            scratch: dir.clone(),
            ..Default::default()
        };
        let built = for_clip(&clip, &clip.fx, clip.src, &dir);
        for (path, sheet) in &built.title_files {
            std::fs::write(path, sheet).unwrap();
        }
        let file = dir.join(format!("clip{seg_s}.mp4"));
        let path = source.display().to_string();
        let mut inputs: Vec<(&str, Option<&str>)> = vec![("video", Some(path.as_str()))];
        inputs.extend(built.inputs.iter().map(|(k, p)| (k.as_str(), p.as_deref())));
        let argv =
            naivepost::produce_render::encode_command(&clip, &settings, &inputs, None, &file);
        let done = std::process::Command::new(&argv[0])
            .args(&argv[1..])
            .output()
            .unwrap();
        assert!(
            done.status.success(),
            "{argv:?}\n{}",
            String::from_utf8_lossy(&done.stderr)
        );
        file
    };
    let (first, second, third) = (encode(0.0), encode(10.0), encode(20.0));
    assert!(brightest(&first, 4.0) < 50, "before the band");
    assert!(brightest(&first, 7.0) > 200, "the band, first clip");
    assert!(
        brightest(&second, 2.0) > 200,
        "the band goes on over the border"
    );
    assert!(brightest(&second, 7.0) < 50, "and ends at 15 s, not at 20");
    assert!(brightest(&third, 2.0) < 50, "a later clip it never reaches");
    let _ = std::fs::remove_dir_all(&dir);
}

/// One clip of `source` with `fx` on it, really encoded.
fn encoded(
    dir: &std::path::Path,
    source: &std::path::Path,
    seg_s: f64,
    rate: f64,
    fx: Vec<Fx>,
) -> std::path::PathBuf {
    let on_screen = 10.0;
    let clip = Clip {
        no: seg_s as usize + 1,
        seg_s,
        on_screen,
        rate,
        frame: (640, 360),
        src: (640, 360),
        fx,
        scratch: dir.to_path_buf(),
        ..Default::default()
    };
    let built = for_clip(&clip, &clip.fx, clip.src, dir);
    for (path, sheet) in &built.title_files {
        std::fs::write(path, sheet).unwrap();
    }
    let file = dir.join(format!("out{seg_s}-{}.mp4", clip.fx.len()));
    let path = source.display().to_string();
    let mut inputs: Vec<(&str, Option<&str>)> = vec![("video", Some(path.as_str()))];
    inputs.extend(built.inputs.iter().map(|(k, p)| (k.as_str(), p.as_deref())));
    let settings = naivepost::project::Produce {
        blurred_edges: false,
        ..Default::default()
    };
    let argv = naivepost::produce_render::encode_command(&clip, &settings, &inputs, None, &file);
    let done = std::process::Command::new(&argv[0])
        .args(&argv[1..])
        .output()
        .unwrap();
    assert!(
        done.status.success(),
        "{argv:?}\n{}",
        String::from_utf8_lossy(&done.stderr)
    );
    file
}

/// A frame of a clip as grey bytes.
fn frame(file: &std::path::Path, at: f64) -> Vec<u8> {
    let out = std::process::Command::new("ffmpeg")
        .args(["-v", "error", "-ss", &format!("{at}"), "-i"])
        .arg(file)
        .args(["-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "gray", "-"])
        .output()
        .expect("ffmpeg runs");
    assert!(!out.stdout.is_empty(), "no frame at {at}");
    out.stdout
}

fn apart(a: &[u8], b: &[u8]) -> f64 {
    let total: u64 = a
        .iter()
        .zip(b)
        .map(|(x, y)| u64::from(x.abs_diff(*y)))
        .sum();
    total as f64 / a.len() as f64
}

fn moving_footage(dir: &std::path::Path) -> std::path::PathBuf {
    let source = dir.join("moving.mp4");
    let made = std::process::Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=s=640x360:r=30:d=30",
        ])
        .args([
            "-f",
            "lavfi",
            "-i",
            "sine=d=30",
            "-pix_fmt",
            "yuv420p",
            "-shortest",
        ])
        .arg(&source)
        .status()
        .unwrap();
    assert!(made.success());
    source
}

fn workspace(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("fx-real-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn a_stop_holds_one_frame_only_for_its_seconds() {
    let dir = workspace("stop");
    let source = moving_footage(&dir);
    let stop = Fx {
        kind: "speed".into(),
        t: 3.0,
        dur: 3.0,
        rate: 0.0,
        ..Default::default()
    };
    let file = encoded(&dir, &source, 0.0, 1.0, vec![stop]);
    assert!(apart(&frame(&file, 3.5), &frame(&file, 5.5)) < 3.0, "held");
    assert!(
        apart(&frame(&file, 7.0), &frame(&file, 9.0)) > 3.0,
        "the footage runs on after"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_drawing_lands_on_the_picture() {
    let dir = workspace("svg");
    let source = moving_footage(&dir);
    let svg = dir.join("box.svg");
    std::fs::write(&svg, r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100"><rect width="100" height="100" fill="white"/></svg>"#).unwrap();
    let draw = Fx {
        kind: "svg".into(),
        t: 2.0,
        dur: 2.0,
        src: svg.display().to_string(),
        ..Default::default()
    };
    let plain = encoded(&dir, &source, 0.0, 1.0, Vec::new());
    let drawn = encoded(&dir, &source, 0.0, 1.0, vec![draw]);
    assert!(
        apart(&frame(&plain, 3.0), &frame(&drawn, 3.0)) > 3.0,
        "while it runs"
    );
    assert!(
        apart(&frame(&plain, 6.0), &frame(&drawn, 6.0)) < 3.0,
        "and not after"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_moving_zoom_encodes_and_moves() {
    let dir = workspace("zoom");
    let source = moving_footage(&dir);
    let zoom = Fx {
        kind: "zoom".into(),
        t: 2.0,
        dur: 6.0,
        trans: 1.0,
        tout: 1.0,
        cx: Some(0.5),
        cy: Some(0.5),
        hf: Some(0.5),
        ..Default::default()
    };
    let plain = encoded(&dir, &source, 0.0, 1.0, Vec::new());
    let zoomed = encoded(&dir, &source, 0.0, 1.0, vec![zoom]);
    assert!(
        apart(&frame(&plain, 0.5), &frame(&zoomed, 0.5)) < 5.0,
        "before the zoom"
    );
    assert!(
        apart(&frame(&plain, 5.0), &frame(&zoomed, 5.0)) > 5.0,
        "inside it"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Loudest sample of a clip's sound between two seconds, in dB.
fn loudest(file: &std::path::Path, from: f64, to: f64) -> f64 {
    let out = std::process::Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-ss",
            &format!("{from}"),
            "-t",
            &format!("{}", to - from),
            "-i",
        ])
        .arg(file)
        .args(["-map", "0:a", "-af", "volumedetect", "-f", "null", "-"])
        .output()
        .expect("ffmpeg runs");
    let log = String::from_utf8_lossy(&out.stderr).to_string();
    log.split("max_volume:")
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|db| db.parse().ok())
        .unwrap_or(f64::NEG_INFINITY)
}

#[test]
fn a_volume_cue_changes_the_sound_only_while_it_runs() {
    let dir = workspace("volume");
    let source = moving_footage(&dir);
    let cue = Fx {
        kind: "volume".into(),
        t: 3.0,
        dur: 3.0,
        gain: 0.1,
        ..Default::default()
    };
    let file = encoded(&dir, &source, 0.0, 1.0, vec![cue]);
    let (outside, inside) = (loudest(&file, 7.0, 9.0), loudest(&file, 3.5, 5.5));
    assert!(
        inside < outside - 15.0,
        "ducked: {inside} against {outside}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_muted_stretch_is_silent_and_one_keeping_its_own_sound_is_heard_late_in_the_session() {
    let dir = workspace("snd");
    let source = moving_footage(&dir);
    let speed = |snd: &str| Fx {
        kind: "speed".into(),
        t: 0.0,
        dur: 40.0,
        rate: 2.0,
        snd: snd.into(),
        ..Default::default()
    };
    let muted = encoded(&dir, &source, 5.0, 2.0, vec![speed("mute")]);
    assert!(loudest(&muted, 1.0, 4.0) < -60.0, "muted");
    let clip = Clip {
        no: 9,
        seg_s: 15.0,
        on_screen: 5.0,
        rate: 2.0,
        frame: (640, 360),
        src: (640, 360),
        fx: vec![speed("own")],
        read_head: Some((15.0, 25.0)),
        scratch: dir.clone(),
        ..Default::default()
    };
    let path = source.display().to_string();
    let out = dir.join("own.mp4");
    let settings = naivepost::project::Produce {
        blurred_edges: false,
        ..Default::default()
    };
    let argv = naivepost::produce_render::encode_command(
        &clip,
        &settings,
        &[("video", Some(path.as_str()))],
        None,
        &out,
    );
    let done = std::process::Command::new(&argv[0])
        .args(&argv[1..])
        .output()
        .unwrap();
    assert!(
        done.status.success(),
        "{}",
        String::from_utf8_lossy(&done.stderr)
    );
    assert!(loudest(&out, 1.0, 4.0) > -40.0, "its own sound plays");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_cue_naming_a_lane_ducks_that_lane_and_not_the_clip() {
    let dir = workspace("lane");
    let source = moving_footage(&dir);
    let quiet = dir.join("quiet.mp4");
    let made = std::process::Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=black:s=64x36:r=10:d=12",
        ])
        .args([
            "-f",
            "lavfi",
            "-i",
            "anullsrc=r=48000:cl=stereo",
            "-shortest",
        ])
        .arg(&quiet)
        .status()
        .unwrap();
    assert!(made.success());
    let encode = |gain: f64| {
        let cue = Fx {
            kind: "volume".into(),
            t: 0.0,
            dur: 10.0,
            gain,
            lane: "mic".into(),
            ..Default::default()
        };
        let clip = Clip {
            no: 1,
            on_screen: 5.0,
            rate: 1.0,
            frame: (640, 360),
            src: (640, 360),
            fx: vec![cue],
            lanes: vec!["mic".into()],
            lane_files: vec![naivepost::produce_render::LaneFile {
                name: "mic".into(),
                path: source.display().to_string(),
                seek: 0.0,
                delay: 0.0,
            }],
            scratch: dir.clone(),
            ..Default::default()
        };
        let path = quiet.display().to_string();
        let out = dir.join(format!("lane{gain}.mp4"));
        let settings = naivepost::project::Produce {
            blurred_edges: false,
            ..Default::default()
        };
        let argv = naivepost::produce_render::encode_command(
            &clip,
            &settings,
            &[("video", Some(path.as_str()))],
            None,
            &out,
        );
        let done = std::process::Command::new(&argv[0])
            .args(&argv[1..])
            .output()
            .unwrap();
        assert!(
            done.status.success(),
            "{}",
            String::from_utf8_lossy(&done.stderr)
        );
        loudest(&out, 1.0, 4.0)
    };
    let (full, ducked) = (encode(1.0), encode(0.05));
    assert!(ducked < full - 15.0, "{ducked} against {full}");
    let _ = std::fs::remove_dir_all(&dir);
}
