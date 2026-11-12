//! spec/04-prepare.md F1.14: line the recordings up by sound.
//!
//! The measuring is driven with made talk (noise in syllable-sized bursts), so every offset here is known
//! to the sample; the door is driven with real FLAC files through ffmpeg.

use std::fs;
use std::path::{Path, PathBuf};

use crate::common;
use naivepost::layout::Tree;
use naivepost::project::{Project, Source};
use naivepost::sync_sound::{self, Move, Verdict, RATE};

/// A small generator of its own, so the same seed is the same talk on every machine.
struct Dice(u64);
impl Dice {
    fn next(&mut self) -> f32 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 33) as f32 / (1u64 << 31) as f32) * 2.0 - 1.0
    }
}

/// How loud the talk is, 100 times a second: bursts of a fifth to half a second, some of them pauses.
fn loudness(seconds: f64, seed: u64) -> Vec<f32> {
    let mut dice = Dice(seed);
    let mut curve = Vec::new();
    while (curve.len() as f64) < seconds * 100.0 {
        let hold = 20 + ((dice.next() + 1.0) * 15.0) as usize;
        let level = if dice.next() > 0.4 {
            0.02
        } else {
            0.2 + (dice.next() + 1.0) * 0.3
        };
        curve.extend(std::iter::repeat_n(level, hold));
    }
    curve.truncate((seconds * 100.0) as usize);
    curve
}

/// Talk: noise through a gentle low-pass, as loud as `curve` says.
fn voiced(curve: &[f32], seed: u64) -> Vec<f32> {
    let mut dice = Dice(seed);
    let mut held = 0.0f32;
    let mut out = Vec::with_capacity(curve.len() * RATE / 100);
    for level in curve {
        for _ in 0..RATE / 100 {
            held = 0.7 * held + 0.3 * dice.next();
            out.push(held * level);
        }
    }
    out
}

fn talk(seconds: f64, seed: u64) -> Vec<f32> {
    voiced(&loudness(seconds, seed), seed ^ 0xabcdef)
}

fn samples(seconds: f64) -> usize {
    (seconds * RATE as f64).round() as usize
}

/// The pure measurement with a search these short recordings can afford in an unoptimised test build.
fn measured(reference: &[f32], other: &[f32]) -> Verdict {
    let rough = sync_sound::coarse(
        &sync_sound::envelope(reference),
        &sync_sound::envelope(other),
        0.0,
        20.0,
    )
    .map(|(offset, _)| offset)
    .expect("the two share sound");
    sync_sound::verdict(&sync_sound::windows(reference, other, rough))
}

/// S3/S4: a recorder started 4.4375 s after the camera is found to the sample.
#[test]
fn f1_14_s4_a_recording_started_later_is_found_to_the_sample() {
    let camera = talk(90.0, 7);
    let recorder = camera[samples(4.4375)..].to_vec();
    let Verdict::Found {
        offset,
        jumps,
        drift_ms_per_minute,
    } = measured(&camera, &recorder)
    else {
        panic!("the same talk is found");
    };
    assert!(
        (offset - 4.4375).abs() < 0.0005,
        "started 4.4375 s later: {offset}"
    );
    assert!(jumps.is_empty(), "nothing slips: {jumps:?}");
    assert_eq!(drift_ms_per_minute, None, "and nothing drifts");
}

/// S4: a recorder started before the camera comes out negative: its start is what has to go.
#[test]
fn f1_14_s4_a_recording_started_earlier_is_negative() {
    let camera = talk(90.0, 11);
    let mut recorder = talk(2.5, 99);
    recorder.extend_from_slice(&camera);
    let Verdict::Found { offset, .. } = measured(&camera, &recorder) else {
        panic!("the same talk is found");
    };
    assert!(
        (offset + 2.5).abs() < 0.0005,
        "started 2.5 s earlier: {offset}"
    );
}

/// S4: where the camera's own sound loses a tenth of a second, the offset stays the start's and the jump is
/// named with its moment and its size.
#[test]
fn f1_14_s4_a_jump_in_the_cameras_sound_is_said_and_the_start_decides() {
    let whole = talk(120.0, 23);
    let recorder = whole[samples(3.0)..].to_vec();
    // The camera drops 0.1 s of sound at 60 s and keeps counting.
    let mut camera = whole[..samples(60.0)].to_vec();
    camera.extend_from_slice(&whole[samples(60.1)..]);
    let Verdict::Found { offset, jumps, .. } = measured(&camera, &recorder) else {
        panic!("the same talk is found");
    };
    assert!(
        (offset - 3.0).abs() < 0.0005,
        "the start's offset stands: {offset}"
    );
    assert_eq!(jumps.len(), 1, "one jump: {jumps:?}");
    assert!(
        (jumps[0].at - 60.0).abs() <= 10.0,
        "near the minute: {:?}",
        jumps[0]
    );
    assert!(
        (jumps[0].by + 0.1).abs() < 0.002,
        "a tenth of a second lost: {:?}",
        jumps[0]
    );
}

/// S4: two microphones in one room share how loud the talk is and not its samples; the offset is still
/// found, on the loudness.
#[test]
fn f1_14_s4_two_microphones_are_lined_up_on_the_loudness() {
    let curve = loudness(90.0, 31);
    let camera = voiced(&curve, 1);
    let lavalier = voiced(&curve[150..], 2);
    let found = sync_sound::windows(&camera, &lavalier, 1.5);
    assert!(
        found.iter().all(|window| !window.fine),
        "no sample agrees between two microphones"
    );
    let Verdict::Found { offset, .. } = sync_sound::verdict(&found) else {
        panic!("the loudness is enough");
    };
    assert!((offset - 1.5).abs() < 0.01, "within a hundredth: {offset}");
}

/// S4: two recordings of different talk are not placed.
#[test]
fn f1_14_s4_unrelated_sound_is_not_found() {
    assert_eq!(measured(&talk(60.0, 1), &talk(60.0, 2)), Verdict::NotFound);
}

/// S5: what is done about an offset depends on what the file is and where it lies.
#[test]
fn f1_14_s5_what_is_moved_and_how() {
    let sources = Path::new("/p/talk.naivepost/sources");
    let at = |name: &str| sources.join(name);
    assert_eq!(
        sync_sound::decide(&at("mic.flac"), sources, 0.004),
        Move::Aligned
    );
    assert_eq!(
        sync_sound::decide(&at("mic.flac"), sources, 4.4),
        Move::Sound
    );
    assert_eq!(
        sync_sound::decide(&at("mic.wav"), sources, -1.0),
        Move::Sound
    );
    assert_eq!(
        sync_sound::decide(&at("mic.mp3"), sources, 1.0),
        Move::LossySound
    );
    assert_eq!(
        sync_sound::decide(&at("cam.mkv"), sources, 0.015),
        Move::Aligned,
        "under half a frame"
    );
    assert_eq!(
        sync_sound::decide(&at("cam.mkv"), sources, 0.5),
        Move::Picture
    );
    assert_eq!(
        sync_sound::decide(Path::new("/card/mic.flac"), sources, 4.4),
        Move::Outside,
        "a file an older project references where it lies is never written"
    );

    let later = sync_sound::sound_args(
        &at("mic.flac"),
        &at("mic.flac.part.flac"),
        4.4373,
        44100,
        "flac",
    );
    assert!(
        later.contains(&"adelay=195685S:all=1".to_string()),
        "silence by whole samples: {later:?}"
    );
    let earlier = sync_sound::sound_args(&at("mic.wav"), &at("x.wav"), -2.0, 48000, "pcm_s24le");
    assert!(earlier.contains(&"atrim=start_sample=96000,asetpts=PTS-STARTPTS".to_string()));
    assert!(
        earlier.contains(&"pcm_s24le".to_string()),
        "a WAV keeps its samples' width"
    );

    let picture = sync_sound::picture_args(&at("cam.mkv"), &at("cam.mkv.part.mkv"), 0.5);
    assert!(picture.contains(&"tpad=start_duration=0.500000:color=black".to_string()));
    assert!(
        picture.contains(&"passthrough".to_string()),
        "its own frame times are kept"
    );
    assert!(picture.contains(&"16".to_string()), "CRF 16");

    assert_eq!(
        sync_sound::moved_line("final", "cam2", 4.4373, &Move::Sound),
        ">>> sync: final started 4.437 s after cam2 -- that much silence is put in front of the copy in sources/"
    );
}

/// The made talk as a FLAC, through ffmpeg.
fn write_flac(at: &Path, talk: &[f32]) {
    let raw: Vec<u8> = talk
        .iter()
        .flat_map(|x| ((x * 32767.0) as i16).to_le_bytes())
        .collect();
    let pcm = at.with_extension("s16");
    fs::write(&pcm, raw).unwrap();
    let ok = std::process::Command::new("ffmpeg")
        .args([
            "-v", "error", "-y", "-f", "s16le", "-ar", "8000", "-ac", "1", "-i",
        ])
        .arg(&pcm)
        .arg(at)
        .status()
        .expect("ffmpeg runs")
        .success();
    assert!(ok, "ffmpeg wrote {}", at.display());
    fs::remove_file(pcm).unwrap();
}

fn seconds_of(path: &Path) -> f64 {
    let out = std::process::Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=nw=1:nk=1",
        ])
        .arg(path)
        .output()
        .expect("ffprobe runs");
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse()
        .expect("a length")
}

fn project_of(names: &[&str]) -> Project {
    Project {
        sources: names
            .iter()
            .map(|name| Source {
                path: format!("project:sources/{name}"),
                narrator: 1,
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    }
}

fn tools() -> sync_sound::Tools {
    sync_sound::Tools {
        ffmpeg: "ffmpeg".to_string(),
        ffprobe: "ffprobe".to_string(),
    }
}

fn session(tag: &str) -> (PathBuf, Tree) {
    let dir = common::temp_dir_created(tag).join("talk.naivepost");
    fs::create_dir_all(dir.join("sources")).unwrap();
    let tree = Tree::new(&dir).expect("a project folder");
    (dir, tree)
}

/// S5/S6 through the door: the later recording gets its silence in front, its old transcript is cleared, and
/// a second pass finds nothing left to do.
#[test]
fn f1_14_s5_the_copy_is_written_again_and_a_second_pass_leaves_it() {
    let (dir, tree) = session("sync_sound_door");
    let whole = talk(70.0, 5);
    write_flac(&dir.join("sources/cam.flac"), &whole);
    write_flac(&dir.join("sources/mic.flac"), &whole[samples(2.25)..]);
    let project = project_of(&["cam.flac", "mic.flac"]);
    // A transcript timed on the old start.
    let stale = tree.input_dir("mic");
    fs::create_dir_all(&stale).unwrap();
    fs::write(stale.join("words.json"), "[]").unwrap();
    let before = seconds_of(&dir.join("sources/mic.flac"));

    let mut said = Vec::new();
    sync_sound::pass(&tree, &project, &tools(), &mut |line: &str| {
        said.push(line.to_string())
    })
    .expect("the pass runs");
    let after = seconds_of(&dir.join("sources/mic.flac"));
    assert!(
        (after - before - 2.25).abs() < 0.001,
        "2.25 s of silence in front: {before} -> {after}"
    );
    assert!(
        said.iter().any(|line| line
            == ">>> sync: mic started 2.250 s after cam -- that much silence is put in front of the copy in sources/"),
        "said: {said:#?}"
    );
    assert!(
        !stale.exists(),
        "the transcript timed on the old start is gone"
    );
    assert!(said
        .iter()
        .any(|line| line.contains("mic's transcript and frames are made again")));
    assert!(
        (seconds_of(&dir.join("sources/cam.flac")) - 70.0).abs() < 0.001,
        "the reference is untouched"
    );
    let left: Vec<_> = fs::read_dir(dir.join("sources"))
        .unwrap()
        .flatten()
        .map(|e| e.file_name())
        .collect();
    assert_eq!(left.len(), 2, "nothing in flight is left behind: {left:?}");

    let mut again = Vec::new();
    sync_sound::pass(&tree, &project, &tools(), &mut |line: &str| {
        again.push(line.to_string())
    })
    .expect("the second pass runs");
    assert!(
        again
            .iter()
            .any(|line| line == ">>> sync: mic already lines up with cam"),
        "said: {again:#?}"
    );
    assert!(
        (seconds_of(&dir.join("sources/mic.flac")) - after).abs() < 1e-6,
        "and nothing is written"
    );
}

/// S1: takes one after another share no seconds, so nothing is measured and nothing is said.
#[test]
fn f1_14_s1_takes_one_after_another_are_left_alone() {
    let (dir, tree) = session("sync_sound_takes");
    write_flac(
        &dir.join("sources/2026-11-08 10-00-00.flac"),
        &talk(20.0, 1),
    );
    write_flac(
        &dir.join("sources/2026-11-08 10-05-00.flac"),
        &talk(20.0, 2),
    );
    let project = project_of(&["2026-11-08 10-00-00.flac", "2026-11-08 10-05-00.flac"]);
    let mut said = Vec::new();
    sync_sound::pass(&tree, &project, &tools(), &mut |line: &str| {
        said.push(line.to_string())
    })
    .expect("the pass runs");
    assert!(
        said.iter().all(|line| !line.contains("sync:")),
        "nothing to line up, nothing said: {said:#?}"
    );
}

/// The plan: one row before the frames whenever two sources could share seconds, none for one recording.
#[test]
fn f1_14_s1_the_stage_is_first_in_prepares_plan() {
    let two = project_of(&["cam.flac", "mic.flac"]);
    let plan = naivepost::run_stages::plan(naivepost::run::Step::Prepare, &two);
    assert_eq!(plan.first().map(|stage| stage.name), Some("sync"));
    assert_eq!(plan[0].tasks.len(), 1);
    let one = project_of(&["cam.flac"]);
    let plan = naivepost::run_stages::plan(naivepost::run::Step::Prepare, &one);
    assert!(plan.iter().all(|stage| stage.name != "sync"));
}

/// S3: a file whose sound cannot be read (a picture with no sound, a file ffmpeg refuses) is left where
/// its name puts it and Prepare goes on; it never fails the run.
#[test]
fn f1_14_s3_a_file_with_no_readable_sound_does_not_stop_the_run() {
    let (dir, tree) = session("sync_sound_unreadable");
    write_flac(&dir.join("sources/cam.flac"), &talk(20.0, 3));
    fs::write(dir.join("sources/junk.flac"), b"not a recording").unwrap();
    let project = project_of(&["cam.flac", "junk.flac"]);
    let mut said = Vec::new();
    sync_sound::pass(&tree, &project, &tools(), &mut |line: &str| {
        said.push(line.to_string())
    })
    .expect("the pass goes on");
    assert!(
        said.iter()
            .any(|line| line.starts_with(">>> sync: junk has no sound to line it up by")),
        "said: {said:#?}"
    );
}

/// A small picture with `sound` under it as its own (lossy) sound, as a camera file.
fn write_camera(at: &Path, seconds: u32, sound: &Path) {
    let ok = std::process::Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-f", "lavfi", "-i"])
        .arg(format!("testsrc=size=64x48:rate=10:duration={seconds}"))
        .arg("-i")
        .arg(sound)
        .args([
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-c:a",
            "aac",
            "-ar",
            "48000",
            "-shortest",
        ])
        .arg(at)
        .status()
        .expect("ffmpeg runs")
        .success();
    assert!(ok, "ffmpeg wrote {}", at.display());
}

fn md5_of(path: &Path, stream: &str) -> String {
    let out = std::process::Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(path)
        .args(["-map", stream, "-c", "copy", "-f", "md5", "-"])
        .output()
        .expect("ffmpeg runs");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn tag_of(path: &Path) -> String {
    let out = std::process::Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format_tags",
            "-of",
            "default=nw=1",
        ])
        .arg(path)
        .output()
        .expect("ffprobe runs");
    String::from_utf8_lossy(&out.stdout).to_string()
}

/// F1.15: a camera with its sound off is given the separate recording's sound, its pictures untouched;
/// the second such camera and the recording itself are listed as the same sound, so the words are
/// transcribed once; and a second pass writes nothing.
#[test]
fn f1_15_a_camera_with_its_sound_off_hears_the_separate_recording() {
    let (dir, tree) = session("sync_sound_lend");
    // Both cameras recorded the room's talk as their own sound; the recorder has it clean.
    write_flac(&dir.join("sources/final.flac"), &talk(60.0, 9));
    write_camera(
        &dir.join("sources/cam2.mkv"),
        60,
        &dir.join("sources/final.flac"),
    );
    write_camera(
        &dir.join("sources/cam1.mkv"),
        60,
        &dir.join("sources/final.flac"),
    );
    let mut project = project_of(&["cam2.mkv", "cam1.mkv", "final.flac"]);
    for camera in &mut project.sources[..2] {
        camera.footage = true;
        camera.sound_off = true;
    }
    let pictures = md5_of(&dir.join("sources/cam2.mkv"), "0:v");
    // A transcript of the camera's own sound, from before the button was turned off.
    fs::create_dir_all(tree.input_dir("cam2")).unwrap();
    fs::write(tree.words_json("cam2"), "{}").unwrap();

    let mut said = Vec::new();
    sync_sound::pass(&tree, &project, &tools(), &mut |line: &str| {
        said.push(line.to_string())
    })
    .expect("the pass runs");
    let lines: Vec<&String> = said
        .iter()
        .filter(|line| line.starts_with(">>> sound:"))
        .collect();
    assert!(
        lines
            .iter()
            .any(|line| line.starts_with(">>> sound: cam2 now carries final's sound")),
        "said: {said:#?}"
    );
    assert!(lines
        .iter()
        .any(|line| line.starts_with(">>> sound: cam1 now carries final's sound")));
    assert_eq!(
        md5_of(&dir.join("sources/cam2.mkv"), "0:v"),
        pictures,
        "the pictures are as they were"
    );
    assert!(
        tag_of(&dir.join("sources/cam2.mkv")).contains("=final"),
        "the copy says whose sound it carries"
    );
    assert!(
        !tree.words_json("cam2").exists(),
        "the transcript of its old sound is gone"
    );

    // The camera now sounds like the recording: measured against it, it lines up and matches.
    assert_eq!(
        sync_sound::same_as(&tree, "cam2"),
        None,
        "the first camera carries the words"
    );
    assert_eq!(sync_sound::same_as(&tree, "cam1").as_deref(), Some("cam2"));
    assert_eq!(sync_sound::same_as(&tree, "final").as_deref(), Some("cam2"));

    let written = fs::metadata(dir.join("sources/cam2.mkv"))
        .unwrap()
        .modified()
        .unwrap();
    let mut again = Vec::new();
    sync_sound::pass(&tree, &project, &tools(), &mut |line: &str| {
        again.push(line.to_string())
    })
    .expect("the second pass runs");
    assert!(
        again.iter().all(|line| !line.contains("now carries")),
        "nothing is written twice: {again:#?}"
    );
    assert_eq!(
        fs::metadata(dir.join("sources/cam2.mkv"))
            .unwrap()
            .modified()
            .unwrap(),
        written
    );
    assert!(
        again
            .iter()
            .any(|line| line == ">>> sync: final already lines up with cam2"),
        "and the camera's new sound is the recording's, to the sample: {again:#?}"
    );

    // The button turned back on: the list is gone and the rows are transcribed as themselves again.
    for camera in &mut project.sources[..2] {
        camera.sound_off = false;
    }
    sync_sound::pass(&tree, &project, &tools(), &mut |_: &str| {}).expect("the third pass runs");
    assert_eq!(sync_sound::same_as(&tree, "final"), None);
}

/// F1.15 S5: with its sound off and no separate recording over it, a camera keeps its own sound, said.
#[test]
fn f1_15_s5_a_camera_with_nothing_over_it_keeps_its_own_sound() {
    let (dir, tree) = session("sync_sound_alone");
    let own = dir.join("own.flac");
    write_flac(&own, &talk(5.0, 4));
    write_camera(&dir.join("sources/cam.mkv"), 5, &own);
    let mut project = project_of(&["cam.mkv"]);
    project.sources[0].footage = true;
    project.sources[0].sound_off = true;
    let before = fs::read(dir.join("sources/cam.mkv")).unwrap();
    let mut said = Vec::new();
    sync_sound::pass(&tree, &project, &tools(), &mut |line: &str| {
        said.push(line.to_string())
    })
    .expect("the pass runs");
    assert!(
        said.iter().any(|line| line
            == ">>> [cam] its own sound is off and no separate recording runs over it -- its own sound is used"),
        "said: {said:#?}"
    );
    assert_eq!(fs::read(dir.join("sources/cam.mkv")).unwrap(), before);
}
