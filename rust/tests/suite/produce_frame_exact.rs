//! 08 §5 — frame-exact clips: each clip's frames come from the video's running clock, its sound is trimmed to the same
//! sample and kept uncompressed until the mux, and an MP4's AAC priming is trimmed away. First rewrite: each clip
//! rounded on its own and its AAC sound ran to its own length, about 11 ms a join (+0.31 s over 15 real 08-dex clips),
//! and the MP4's sound started 21 ms late.
//!
//! The last test runs the app's own plan and walk with real ffmpeg over a made source: a white frame and a click at
//! every whole second, the click's audio starting 22 ms after the picture as OBS records it.

use std::path::{Path, PathBuf};
use std::process::Command;

use naivepost::cut::{Cut, Seg};
use naivepost::produce_exec as exec;
use naivepost::produce_render::{self as render, frame_grid, mux_audio_filter, Clip};
use naivepost::project::{Container, FrameRate, Preset, Produce, Resolution, Subtitles};

use crate::common;

fn clips(lengths: &[f64]) -> Vec<Clip> {
    lengths
        .iter()
        .map(|&on_screen| Clip {
            on_screen,
            rate: 1.0,
            ..Default::default()
        })
        .collect()
}

#[test]
fn f5_2_s5_frames_follow_the_running_clock() {
    let mut c = clips(&[4.37, 2.51, 3.22]);
    frame_grid(&mut c, 30.0);
    let frames: Vec<usize> = c.iter().map(|clip| clip.frames).collect();
    // 4.37 s → frame 131; 6.88 s → 206 (75 more); 10.10 s → 303 (97 more): the video ends on the cut's own frame.
    assert_eq!(frames, [131, 75, 97]);
    assert!(c.iter().all(|clip| clip.fps == 30.0));
}

#[test]
fn f5_2_s5_rounding_never_builds_up() {
    // 200 clips of 1.017 s: rounded each on its own (31 frames = 1.0333 s) they would run 3.3 s long.
    let mut c = clips(&[1.017; 200]);
    frame_grid(&mut c, 30.0);
    let total: usize = c.iter().map(|clip| clip.frames).sum();
    assert!(
        (total as f64 / 30.0 - 203.4).abs() <= 0.5 / 30.0,
        "{total} frames"
    );
}

#[test]
fn f5_2_s8_only_an_mp4_trims_the_aac_priming() {
    assert!(mux_audio_filter(Container::Mp4).contains("atrim=start_sample=1024"));
    assert!(
        !mux_audio_filter(Container::Mkv).contains("atrim"),
        "Matroska keeps the delay itself"
    );
    assert!(
        !mux_audio_filter(Container::Webm).contains("atrim"),
        "so does WebM's Opus"
    );
}

#[test]
fn f5_2_s5_a_source_rate_is_read_as_a_number() {
    use naivepost::produce_probe::parse_rate;
    assert_eq!(parse_rate("60/1"), Some(60.0));
    assert!((parse_rate("30000/1001").unwrap() - 29.97).abs() < 0.001);
    assert_eq!(parse_rate("25"), Some(25.0));
    assert_eq!(parse_rate("0/0"), None);
}

fn have_ffmpeg() -> bool {
    Command::new("ffmpeg").arg("-version").output().is_ok()
}

fn ffmpeg(args: &[&str]) {
    let out = Command::new("ffmpeg")
        .args(["-v", "error", "-y"])
        .args(args)
        .output()
        .expect("ffmpeg");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// 20 s at 60 fps: a white frame at every whole second (three frames long), a 10 ms click at the same instants, the
/// sound starting 22 ms into the file as OBS writes it.
fn make_source(dir: &Path) -> PathBuf {
    let video = dir.join("v.mkv");
    let clicks = dir.join("c.wav");
    let src = dir.join("cam.mkv");
    ffmpeg(&["-f", "lavfi", "-i", "color=c=gray:s=640x360:r=60:d=20,drawbox=x=0:y=0:w=iw:h=ih:color=white:t=fill:enable='lt(mod(n,60),3)'",
        "-c:v", "libx264", "-preset", "ultrafast", "-bf", "3", "-g", "250", "-pix_fmt", "yuv420p", video.to_str().unwrap()]);
    ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        "aevalsrc='if(lt(mod(t,1),0.01),0.8*sin(2*PI*1000*t),0)':s=48000:d=20",
        "-c:a",
        "pcm_s16le",
        clicks.to_str().unwrap(),
    ]);
    ffmpeg(&[
        "-i",
        video.to_str().unwrap(),
        "-ss",
        "0.022",
        "-itsoffset",
        "0.022",
        "-i",
        clicks.to_str().unwrap(),
        "-map",
        "0:v",
        "-map",
        "1:a",
        "-c:v",
        "copy",
        "-c:a",
        "aac",
        src.to_str().unwrap(),
    ]);
    src
}

/// The final file's flash and click times: (white-frame onsets, click onsets), seconds on its own clock.
fn markers(file: &Path, fps: f64) -> (Vec<f64>, Vec<f64>, f64, f64) {
    let probe = |args: &[&str]| {
        String::from_utf8(
            Command::new("ffprobe")
                .args(["-v", "error"])
                .args(args)
                .arg(file)
                .output()
                .unwrap()
                .stdout,
        )
        .unwrap()
    };
    let a_start: f64 = probe(&[
        "-select_streams",
        "a:0",
        "-show_entries",
        "stream=start_time",
        "-of",
        "csv=p=0",
    ])
    .trim()
    .parse()
    .unwrap_or(0.0);
    // Mean luma per frame with its time, from 1-pixel frames.
    let raw = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(file)
        .args(["-an", "-vf", "scale=1:1,format=gray", "-f", "rawvideo", "-"])
        .output()
        .unwrap()
        .stdout;
    let times: Vec<f64> = probe(&[
        "-select_streams",
        "v:0",
        "-show_entries",
        "frame=pts_time",
        "-of",
        "csv=p=0",
    ])
    .lines()
    .filter_map(|l| l.trim().trim_end_matches(',').parse().ok())
    .collect();
    let mut flashes = Vec::new();
    for i in 0..raw.len().min(times.len()) {
        if raw[i] > 200 && (i == 0 || raw[i - 1] <= 200) {
            flashes.push(times[i]);
        }
    }
    let video_len = times.last().copied().unwrap_or(0.0) + 1.0 / fps;
    let pcm = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(file)
        .args(["-vn", "-ac", "1", "-ar", "48000", "-f", "f32le", "-"])
        .output()
        .unwrap()
        .stdout;
    let samples: Vec<f32> = pcm
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect();
    let mut clicks = Vec::new();
    let mut i = 0;
    while i < samples.len() {
        if samples[i].abs() > 0.2 {
            clicks.push(a_start + i as f64 / 48000.0);
            i += 4800;
        } else {
            i += 1;
        }
    }
    (
        flashes,
        clicks,
        video_len,
        a_start + samples.len() as f64 / 48000.0,
    )
}

#[test]
fn f5_2_s5_the_video_is_as_long_as_the_cut_and_in_sync() {
    if !have_ffmpeg() {
        eprintln!("no ffmpeg here: the encode is not checked");
        return;
    }
    for (frame_rate, grid) in [(FrameRate::F30, 30.0), (FrameRate::Source, 60.0)] {
        let (root, tree) = common::tree_in(&format!("frame-exact-{grid}"));
        let src = make_source(&root);
        // Six pieces of odd lengths from all over the source.
        let pieces = [
            (1.13, 2.37),
            (4.41, 5.92),
            (6.02, 9.31),
            (10.5, 11.73),
            (12.2, 15.03),
            (16.6, 18.9),
        ];
        let cut = Cut {
            segs: pieces
                .iter()
                .map(|&(s, e)| Seg {
                    s,
                    e,
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        };
        let settings = Produce {
            container: Container::Mp4,
            frame_rate,
            preset: Preset::Ultrafast,
            resolution: Resolution::P720,
            subtitles: Subtitles::None,
            ..Default::default()
        };
        let project = naivepost::project::Project::default();
        let run = naivepost::produce_flow::snapshot(
            Some(cut),
            &tree,
            Vec::new(),
            settings,
            &project,
            None,
            "1".to_string(),
        );
        let source = src.display().to_string();
        let source_file = |clip: &Clip| Some((source.clone(), clip.seg_s));
        let refuse: &'static naivepost::produce_translate::Ask =
            Box::leak(Box::new(|_m: &str, _n: &[usize]| {
                Err("no translation here".to_string())
            }));
        let sources = Vec::new;
        let mats = exec::Materials {
            exists: &|_f| true,
            fx_for: &|_s: &Seg| Vec::new(),
            lanes: &[] as &[naivepost::cut::Lane],
            src_shape: (640, 360),
            wav_of: &|_e| None,
            wav_seconds: &|_e| None,
            footage_left: &render::unknown_footage,
            source_file: &source_file,
            on_recording: true,
            cues: exec::Cues::default(),
            sources: &sources,
            words: &|_c| Vec::new(),
            translate: refuse,
            probe: &|_f| None,
            filmed_runs: naivepost::produce_plan_logs::NO_TAPE,
            base_of: &|_s| "cam".to_string(),
        };
        let plan = exec::plan(&run, &tree, &mats);
        assert!(
            plan.clips.iter().all(|c| c.fps == grid),
            "the grid is {grid}: {:?}",
            plan.clips.iter().map(|c| c.fps).collect::<Vec<_>>()
        );
        let ran = exec::walk(
            &plan,
            &mats,
            |_l| {},
            |_s| Ok(()),
            |_a, _w| true,
            |command: &exec::Command| {
                let out = Command::new(&command.argv[0])
                    .args(&command.argv[1..])
                    .output()
                    .map_err(|e| e.to_string())?;
                out.status
                    .success()
                    .then_some(())
                    .ok_or_else(|| String::from_utf8_lossy(&out.stderr).into_owned())
            },
        );
        assert!(ran.muxed, "{grid}: {:?} {:?}", ran.failed, ran.logs);
        let cut_len: f64 = pieces.iter().map(|(s, e)| e - s).sum();
        let (flashes, clicks, video_len, audio_len) = markers(&plan.video, grid);
        assert!(
            (video_len - cut_len).abs() <= 0.5 / grid + 1e-6,
            "{grid}: video {video_len:.4} s for a {cut_len:.4} s cut"
        );
        assert!(
            (audio_len - video_len).abs() <= 0.025,
            "{grid}: sound {audio_len:.4} s, picture {video_len:.4} s"
        );
        // A flash lands on whole output frames, so one pair may be up to a frame (33 ms at 30 fps) apart; on average
        // the picture and the sound agree within 15 ms (the old path averaged 31-62 ms, the click detector adds ~5).
        let gaps: Vec<f64> = flashes
            .iter()
            .filter_map(|f| {
                let c = clicks
                    .iter()
                    .min_by(|a, b| (*a - f).abs().total_cmp(&(*b - f).abs()))?;
                ((c - f).abs() < 0.2).then_some(c - f)
            })
            .collect();
        assert!(
            gaps.len() >= 10,
            "{grid}: {} flash/click pairs found",
            gaps.len()
        );
        let mean = gaps.iter().sum::<f64>() / gaps.len() as f64;
        assert!(
            mean.abs() <= 0.015,
            "{grid}: the sound is {:.1} ms off the picture on average: {gaps:?}",
            mean * 1000.0
        );
        assert!(
            gaps.iter().all(|g| g.abs() <= 0.040),
            "{grid}: a pair is more than a frame apart: {gaps:?}"
        );
    }
}
