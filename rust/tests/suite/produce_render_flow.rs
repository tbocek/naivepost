//! F5.2 The render — spec/08-produce.md §F5.2 (wording also in spec/inventory/produce.md §B), checked
//! against `naivepost::produce_render`. One test per step, S1 to S10: what the scratch folder means, what
//! still has to be spoken, how a segment becomes a clip and a line gets fitted, where the cues are cut, what
//! one encode command looks like, how the clips join, when translation runs, what the mux carries, which
//! sidecars exist per language, and when the stamp may be written.
//!
//! Ids cited: `P.eng.minClipSeconds` (0.5), `P.policy.gameVolume` (0.22), `P.eng.laneMinMixSeconds` (0.1),
//! `P.eng.narrationMaxExtendSeconds` (4) and `tool:ffmpeg.encode` — the codec, preset and CRF arguments are
//! ffmpeg's own numbers, which is why they are asserted as strings rather than measured.

use crate::common;
use common::entry_at as entry;
use naivepost::cut::{Fx, Lane, Seg};
use naivepost::produce_render as r;
use naivepost::project::{self, Container, Produce, Subtitles};
use naivepost::render_fx;
use naivepost::tools::cutpass;

const ITEM: &str = "f5_2";

fn temp(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("np-f5-{}-{tag}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn seg(s: f64, e: f64) -> Seg {
    Seg {
        s,
        e,
        ins: String::new(),
        ..Default::default()
    }
}

fn lane(name: &str, at: f64, dur: f64) -> Lane {
    let mut row = Lane::default();
    row.name = name.to_string();
    row.src = "media/game.wav".to_string();
    row.at = at;
    row.dur = dur;
    row
}

/// No fx on any segment: the render's pass is about files, rates and lines, not effects.
fn no_fx(_: &Seg) -> Vec<Fx> {
    Vec::new()
}

// ---- S1 ---------------------------------------------------------------------------

#[test]
fn f5_2_s1_clears_the_scratch_folder() {
    assert_eq!(ITEM, "f5_2");
    // §F5.2 S1 / §B step 0: the scratch folder is emptied before anything is encoded into it.
    assert_eq!(r::SCRATCH, "produce/clips/");
    let dir = temp("s1").join("clips");
    std::fs::create_dir_all(&dir).unwrap();
    for name in ["c000_old.mp4", "c001_old.mp4", "final.srt"] {
        std::fs::write(dir.join(name), b"x").unwrap();
    }
    // A subfolder is not scratch: only files go.
    std::fs::create_dir_all(dir.join("keep")).unwrap();
    assert_eq!(r::clear_scratch(&dir).unwrap(), 3);
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
    assert!(dir.join("keep").is_dir());
    assert!(dir.is_dir(), "the folder itself survives its own clearing");

    // Nothing there yet is not an error: the render creates it when it writes.
    let missing =
        std::env::temp_dir().join(format!("np-f5-{}-s1-absent/clips", std::process::id()));
    assert_eq!(r::clear_scratch(&missing).unwrap(), 0);
    assert!(!missing.exists());
    std::fs::remove_dir_all(std::env::temp_dir().join(format!("np-f5-{}-s1", std::process::id())))
        .ok();
}

// ---- S2 ---------------------------------------------------------------------------

#[test]
fn f5_2_s2_captions_speak_nothing_otherwise_every_missing_line() {
    assert_eq!(ITEM, "f5_2");
    let lines = vec![
        entry(0.0, 4.0, 0.3, "the first line"),
        entry(4.0, 8.0, 0.3, ""),
        entry(8.0, 12.0, 0.3, "the third line"),
    ];

    // Captions-only voice: nothing is spoken at all — the lines ride the subtitle track (§5's short-circuit).
    assert_eq!(r::speak_job(true, &lines, |_| false), r::Speak::Nothing);

    // A real voice: every line with words and no wav, in the record's order. An empty line is a deliberate
    // silence and is never spoken — the same seam narration::lines_to_speak applies.
    let spoken = ["the first line".to_string()];
    let answer = r::speak_job(false, &lines, |line| spoken.contains(&line.text));
    assert_eq!(
        answer,
        r::Speak::Missing(vec!["the third line".to_string()])
    );
    assert_eq!(naivepost::narration::lines_to_speak(&lines).len(), 2);

    // Everything already spoken: an empty list rather than a Nothing that would skip the job's close.
    assert_eq!(
        r::speak_job(false, &lines[..1], |_| true),
        r::Speak::Missing(Vec::new())
    );

    // The job line and its fraction: speaking is 1/2 of the run.
    assert_eq!(r::speaking_job_line(1, 2), "speaking 1/2");
    assert_eq!(r::SPEAKING_FRACTION, 0.5);
    assert_eq!(
        r::synthesis_failure_log(3.4, "server down"),
        "synthesis for 3.4s: server down"
    );

    // Captions only + nothing in the video is the one case where the words reach no file at all.
    assert_eq!(
        r::captions_only_warning(Subtitles::None),
        Some(">>> captions only and nothing in the video \u{2014} the lines are in the .srt beside it")
    );
    assert_eq!(r::captions_only_warning(Subtitles::BurnedIn), None);
    assert_eq!(r::captions_only_warning(Subtitles::TrackInFile), None);
}

// ---- S3 ---------------------------------------------------------------------------

#[test]
fn f5_2_s3_one_clip_per_segment_and_the_drops() {
    assert_eq!(ITEM, "f5_2");
    // P.eng.minClipSeconds: the shortest clip the render makes. Its row lives in §10's effects section, so it
    // is read from params::effects() — params::find only searches Prepare's rows.
    assert_eq!(cutpass::MIN_CLIP_SECONDS, 0.5);
    let row = naivepost::params::effects()
        .into_iter()
        .find(|row| row.id == "P.eng.minClipSeconds")
        .expect("P.eng.minClipSeconds is catalogued");
    assert_eq!(row.spelled, "0.5");

    // Matched when the line spends at least half of *its own* length over the clip, and not otherwise: a
    // three-second line that only grazes a long clip's last second is not that clip's line.
    let clip = seg(0.0, 10.0);
    assert!(r::matches(&entry(8.0, 12.0, 0.0, "half in"), &clip));
    assert!(!r::matches(&entry(9.0, 12.0, 0.0, "a third in"), &clip));
    assert!(!r::matches(&entry(4.0, 4.0, 0.0, "no span at all"), &clip));
    assert!(!r::matches(&entry(20.0, 24.0, 0.0, "elsewhere"), &clip));
    // A line shorter than the clip only has to lie inside it: the whole of it is over the clip.
    assert!(r::matches(&entry(-5.0, 15.0, 0.0, "over the lot"), &clip));

    // Footage plans with its own rate; the frame box is render_fx's answer for the aspect.
    let mut fast = seg(0.0, 20.0);
    fast.rate = 2.0;
    let (clips, logs) = r::plan_clips(
        &[fast.clone()],
        no_fx,
        "16:9",
        |_| true,
        &[],
        &[],
        (1920, 1080),
        true,
        naivepost::produce_render::unknown_footage,
    );
    assert!(logs.is_empty(), "{logs:?}");
    assert_eq!(clips.len(), 1);
    assert_eq!(clips[0].no, 1);
    assert_eq!(clips[0].rate, 2.0);
    assert_eq!(clips[0].on_screen, 10.0);
    assert_eq!(clips[0].source, "");
    assert_eq!(clips[0].frame, render_fx::frame_box("16:9", 1920, 1080));
    assert_eq!(clips[0].frame, (1920, 1080));

    // A missing insert is skipped and named; the clips around it survive with their own numbers.
    let mut gone = seg(20.0, 30.0);
    gone.ins = "media/gone.mp4".to_string();
    gone.dur = 10.0;
    let keep = seg(30.0, 40.0);
    let (clips, logs) = r::plan_clips(
        &[seg(0.0, 10.0), gone, keep],
        no_fx,
        "source",
        |file| !file.contains("gone"),
        &[],
        &[],
        (1920, 1080),
        true,
        naivepost::produce_render::unknown_footage,
    );
    assert_eq!(clips.len(), 2);
    assert_eq!(
        logs,
        vec!["clip 2: media/gone.mp4 is not there any more \u{2014} skipped"]
    );
    // The numbers count segments asked for, so "clip 3" still means the third row.
    assert_eq!(clips[1].no, 3);

    // A pasted stretch keeps its `copy:` spelling and is never asked of the file system.
    let mut copy = seg(0.0, 5.0);
    copy.ins = "copy:12.5".to_string();
    copy.dur = 5.0;
    let (clips, logs) = r::plan_clips(
        &[copy],
        no_fx,
        "source",
        |_| false,
        &[],
        &[],
        (1920, 1080),
        true,
        naivepost::produce_render::unknown_footage,
    );
    assert!(logs.is_empty(), "{logs:?}");
    assert_eq!(clips[0].source, "copy:12.5");

    // Under the bound: dropped, and the log names the lines it took with it.
    let short = seg(60.0, 60.4);
    let lost = vec![entry(60.0, 63.0, 0.3, "a line on the dropped clip")];
    let (clips, logs) = r::plan_clips(
        &[short],
        no_fx,
        "source",
        |_| true,
        &[],
        &lost,
        (1920, 1080),
        true,
        naivepost::produce_render::unknown_footage,
    );
    assert!(clips.is_empty());
    assert_eq!(
        logs,
        vec!["clip 1 at 01:00 is 0.4 s \u{2014} too short to render, dropped \u{2014} the narration line(s) written on it are dropped with it"]
    );

    // Exactly the bound survives — the comparison is `<`, so 0.5 s renders.
    let edge = seg(60.0, 60.5);
    let (clips, logs) = r::plan_clips(
        &[edge],
        no_fx,
        "source",
        |_| true,
        &[],
        &[],
        (1920, 1080),
        true,
        naivepost::produce_render::unknown_footage,
    );
    assert_eq!(clips.len(), 1);
    assert!(logs.is_empty(), "{logs:?}");

    // A stop's rate of nought is not a clock: the footage runs on at 1× under its still.
    let mut stop = seg(0.0, 6.0);
    stop.rate = 0.0;
    let (clips, _) = r::plan_clips(
        &[stop],
        no_fx,
        "source",
        |_| true,
        &[],
        &[],
        (1920, 1080),
        true,
        naivepost::produce_render::unknown_footage,
    );
    assert_eq!(clips[0].rate, 1.0);

    // A lane the scene hears is on the clip; one it hushes is not.
    let lanes = vec![lane("Music", 0.0, 30.0), lane("Game", 0.0, 30.0)];
    let mut quiet = seg(0.0, 10.0);
    quiet.quiet = vec!["Game".to_string()];
    let (clips, _) = r::plan_clips(
        &[quiet],
        no_fx,
        "source",
        |_| true,
        &lanes,
        &[],
        (1920, 1080),
        true,
        naivepost::produce_render::unknown_footage,
    );
    assert_eq!(clips[0].lanes, vec!["Music".to_string()]);

    assert_eq!(r::no_clip_log(), "no clip could be placed on a recording");
}

// ---- S3 / F4.3: the fitting ladder -----------------------------------------------

#[test]
fn f5_2_s3b_the_fitting_ladder() {
    assert_eq!(ITEM, "f5_2");
    // The four numbers of F4.3's render half (§10).
    assert_eq!((r::LEAD_SECONDS, r::GAP_SECONDS), (0.3, 0.3));
    assert_eq!(r::MAX_EXTEND_SECONDS, 4.0);
    assert_eq!(r::MAX_TEMPO, 1.25);

    // Fits as placed: nothing moves and nothing is said.
    let (placed, logs) = r::fit(&[(0, 0.3, 2.0)], 10.0);
    assert!(logs.is_empty(), "{logs:?}");
    assert_eq!(
        (placed[0].at, placed[0].tempo, placed[0].extend),
        (0.3, 1.0, 0.0)
    );

    // Over the room by less than the ceiling: the clip grows and no log is written — growing is what the
    // render may do quietly. The line needs 6.0 s of speech + 0.2 s tail in 2.0 s, so 4.2 s would be needed
    // and §10's ceiling stops it at 4.0.
    let (placed, logs) = r::fit(&[(0, 0.3, 5.5)], 2.0);
    assert!(logs.is_empty(), "{logs:?}");
    assert_eq!(placed[0].extend, 4.0);
    assert_eq!(placed[0].tempo, 1.0);

    // Past the ceiling: slide the run earlier, down to the lead. Sliding alone cannot help when the line is
    // placed at the lead already, so this one goes straight to the tempo and says so twice — how far it could
    // not be moved, then how much faster it has to speak.
    let (placed, logs) = r::fit(&[(0, 0.3, 7.0)], 2.0);
    assert_eq!(logs.len(), 2, "{logs:?}");
    assert_eq!(
        logs[0],
        "clip 1: the narration does not fit where it was placed \u{2014} moved 0.0 s earlier"
    );
    assert_eq!(placed[0].at, r::LEAD_SECONDS);
    assert!(placed[0].extend <= r::MAX_EXTEND_SECONDS + 1e-9);

    // A line placed late enough to be slid does move, and the log spells the distance off: it only has to
    // come back as far as the grown clip needs, not all the way to the lead.
    let (placed, logs) = r::fit(&[(0, 5.0, 6.0)], 4.0);
    assert_eq!(logs.len(), 1, "{logs:?}");
    assert_eq!(
        logs[0],
        "clip 1: the narration does not fit where it was placed \u{2014} moved 3.2 s earlier"
    );
    assert!(
        placed[0].at > r::LEAD_SECONDS && placed[0].at < 5.0,
        "{}",
        placed[0].at
    );

    // Slid all the way to the lead: only when that is what the room asks for.
    let (placed, logs) = r::fit(&[(0, 5.0, 6.0)], 2.0);
    assert_eq!(placed[0].at, r::LEAD_SECONDS);
    assert!(
        logs.iter().any(|log| log.contains("moved 4.7 s earlier")),
        "{logs:?}"
    );

    // Too much speech for even a slid, grown clip: sped up by whatever fits, and never past the ceiling.
    let (placed, logs) = r::fit(&[(0, 5.0, 12.0)], 4.0);
    assert!(
        placed[0].tempo <= r::MAX_TEMPO + 1e-9,
        "{}",
        placed[0].tempo
    );
    assert!(placed[0].tempo > 1.0, "{:?}", placed[0].tempo);
    let last = logs.last().unwrap();
    assert!(last.starts_with("clip 1: narration "), "{last}");

    // A line needing more than the ceiling is still placed at no more than the ceiling.
    let (placed, _) = r::fit(&[(0, 5.0, 60.0)], 4.0);
    assert!(
        placed[0].tempo <= r::MAX_TEMPO + 1e-9,
        "{}",
        placed[0].tempo
    );

    // The sentence §F4.3 quotes, exactly.
    assert_eq!(
        r::sped_up_log(1, 6.0, 4.0, 1.25),
        "clip 1: narration 6.0 s does not fit 4.0 s \u{2014} sped up 1.25x"
    );
    assert_eq!(
        r::moved_log(1, 0.7),
        "clip 1: the narration does not fit where it was placed \u{2014} moved 0.7 s earlier"
    );

    // Two lines: the second never starts before the first's end + gap + tail, and the first never before
    // the lead.
    let (placed, _) = r::fit(&[(0, 0.0, 2.0), (1, 0.5, 2.0)], 20.0);
    assert_eq!(placed[0].at, r::LEAD_SECONDS);
    assert_eq!(
        placed[1].at,
        placed[0].at + 2.0 + r::GAP_SECONDS + r::TAIL_SECONDS
    );

    // A line with no synthesis takes no room: nothing moves and nothing is logged.
    let (placed, logs) = r::fit(&[(0, 9.0, 0.0)], 10.0);
    assert!(logs.is_empty(), "{logs:?}");
    assert_eq!((placed[0].at, placed[0].speech), (9.0, 0.0));

    // No lines at all: no placement, no log — an empty clip is not a fitting failure.
    let (placed, logs) = r::fit(&[], 10.0);
    assert!(placed.is_empty() && logs.is_empty());
}

// ---- S4 ---------------------------------------------------------------------------

#[test]
fn f5_2_s4_subtitles_on_the_produced_clock() {
    assert_eq!(ITEM, "f5_2");
    // A clip at 2× runs half as long, so its cues move with it: recording 1.0 s lands at produced 0.5 s.
    let first = r::Clip {
        no: 1,
        seg_s: 0.0,
        on_screen: 5.0,
        rate: 2.0,
        ..Default::default()
    };
    let second = r::Clip {
        no: 2,
        seg_s: 10.0,
        on_screen: 4.0,
        rate: 1.0,
        ..Default::default()
    };
    let clocks = r::produced_clocks(&[first, second]);
    assert_eq!(clocks[0].0, 0.0);
    assert_eq!(clocks[0].1, 5.0);
    // `on_screen` has the rate in it already: the next clip starts where this one ends on screen.
    assert_eq!(clocks[1].0, 5.0);

    assert_eq!(r::final_srt_name(), "final.srt");

    // Stale sidecars are deleted by exact name: the run's own files and nothing that matches a pattern.
    let names = r::stale_sidecars("final", &["de"]);
    assert_eq!(
        names,
        vec!["final.srt", "final.vtt", "final.de.srt", "final.de.vtt"]
    );
    assert!(names.iter().all(|name| !name.contains('*')), "{names:?}");
    // With no translation ticked there are still exactly two files beside the video.
    assert_eq!(
        r::stale_sidecars("final", &[]),
        vec!["final.srt", "final.vtt"]
    );
}

// ---- S5 ---------------------------------------------------------------------------

// ---- S6 ---------------------------------------------------------------------------

// ---- S7 ---------------------------------------------------------------------------

#[test]
fn f5_2_s7_translation_comes_after_the_encodes() {
    assert_eq!(ITEM, "f5_2");
    // The order is written down rather than implied by a callback's position: translate sits between the
    // join and the mux, so an encoder never idles behind the LLM gate.
    let stages = r::stages();
    let at = |name: &str| stages.iter().position(|(stage, _)| *stage == name).unwrap();
    assert_eq!(at("translating"), at("joining") + 1);
    assert_eq!(at("muxing"), at("translating") + 1);
    assert_eq!(stages.len(), 10);

    // The bar holds its fraction while the model works: after every encode, before the mux that needs it.
    assert!((r::TRANSLATE_FRACTION - 0.94).abs() < 1e-9);

    // The log names the language and how many cues went into the request.
    assert_eq!(
        r::translate_log(7, "de"),
        ">>> subtitles: translating 7 lines into de"
    );
}

// ---- S8 ---------------------------------------------------------------------------

#[test]
fn f5_2_s8_the_mux_carries_only_ticked_subtitles() {
    assert_eq!(ITEM, "f5_2");
    let video = std::path::Path::new("produce/video.mp4");
    let audio = std::path::Path::new("produce/audio.m4a");
    let out = std::path::Path::new("produce/final.mp4");
    let settings = Produce::default();

    // Nothing ticked: the video is repacked with its loudness fixed and no subtitle stream at all.
    let line = r::mux_command(video, audio, &[], &settings, out, &[]).join(" ");
    assert!(!line.contains("-c:s"), "{line}");
    assert!(!line.contains("mov_text"), "{line}");
    // The rewritten sound keeps the page's bitrate and layout, as every clip was encoded with.
    assert!(line.contains("-b:a 128k -ac 2"), "{line}");

    // Ticked and in-file: one track per language, mp4's own codec, each mapped with its language tag.
    let mut track = settings.clone();
    track.subtitles = Subtitles::TrackInFile;
    let tracks = [("de", std::path::Path::new("produce/final.de.srt"))];
    let line = r::mux_command(video, audio, &tracks, &track, out, &[]).join(" ");
    assert!(line.contains("produce/final.de.srt"), "{line}");
    assert!(line.contains("-c:s mov_text"), "{line}");
    // Every input before any output option (an option before an `-i` is that input's), and the track mapped
    // by its own input: the video, the sound, then the .srt as input 2.
    let srt = line
        .find("-i produce/final.de.srt")
        .expect("the track is an input");
    assert!(srt < line.find("-c:s").unwrap(), "{line}");
    assert!(srt < line.find("-map").unwrap(), "{line}");
    assert!(line.contains("-map 2:0"), "{line}");

    // Burned-in is the encoder's business (§F5.2 S5), so the mux adds nothing for it.
    let mut burned = settings.clone();
    burned.subtitles = Subtitles::BurnedIn;
    let line = r::mux_command(video, audio, &tracks, &burned, out, &[]).join(" ");
    assert!(!line.contains("-c:s"), "{line}");
    assert!(!line.contains("final.de.srt"), "{line}");

    // A webm carries no subtitle track whatever the tick says — the container rules answer, not a guess.
    let mut web = track.clone();
    web.container = Container::Webm;
    let line = r::mux_command(video, audio, &tracks, &web, out, &[]).join(" ");
    assert!(!line.contains("-c:s"), "{line}");
    assert_eq!(r::subtitle_codec(project::Container::Mp4), "mov_text");
    assert_eq!(r::subtitle_codec(project::Container::Mkv), "srt");

    // Loudness is this step's job and it is §B's filter verbatim, resample first so loudnorm sees a
    // constant rate.
    assert_eq!(
        r::LOUDNORM,
        "aresample=async=1:first_pts=0,loudnorm=I=-14:TP=-1.5:LRA=11"
    );
    let line = r::mux_command(video, audio, &[], &settings, out, &[]).join(" ");
    assert!(line.contains(r::LOUDNORM), "{line}");
    assert!(line.contains("-c:v copy"), "{line}");

    // The page asked for cues and there is nothing to write: a warning, because the video is fine.
    assert_eq!(
        r::no_subtitle_cue_log(),
        "!!! nothing to put in a subtitle: no narration, and no speech in the clips"
    );
}

// ---- S9 ---------------------------------------------------------------------------

#[test]
fn f5_2_s9_the_sidecars_written_per_language() {
    assert_eq!(ITEM, "f5_2");
    // §F5.2's file list: a .srt and a .vtt per language. The session's own language carries no code — it is
    // the track a player picks by default, so a second copy named after itself would say the same thing twice.
    assert_eq!(
        r::sidecar_names("my video", None),
        ["my video.srt", "my video.vtt"]
    );
    assert_eq!(
        r::sidecar_names("my video", Some("de")),
        ["my video.de.srt", "my video.de.vtt"]
    );

    // One line per language, naming both files: the .vtt is the one people look for when an embed shows none.
    assert_eq!(
        r::sidecar_log("my video", None),
        ">>> subtitles: my video.srt and .vtt"
    );
    assert_eq!(
        r::sidecar_log("my video", Some("de")),
        ">>> subtitles: my video.de.srt and .vtt"
    );

    // Stale files go by exact name, never a glob — a `*.de.srt` pattern would delete somebody else's work.
    let names = r::stale_sidecars("final", &["de"]);
    assert_eq!(
        names,
        vec!["final.srt", "final.vtt", "final.de.srt", "final.de.vtt"]
    );
    assert!(names.iter().all(|name| !name.contains('*')), "{names:?}");
}

// ---- S10 --------------------------------------------------------------------------

#[test]
fn f5_2_s10_checkpoints_and_the_stamp() {
    assert_eq!(ITEM, "f5_2");
    // The stamp is the resume marker: only the run that wrote the file may promise it is up to date.
    assert!(r::stamp_written(true, false));
    assert!(
        !r::stamp_written(false, false),
        "a run that encoded nothing proved nothing"
    );
    assert!(
        !r::stamp_written(true, true),
        "a failed run writes no promise"
    );

    // Five checkpoints, each between subprocesses so a stop never leaves half an ffmpeg command running;
    // the last sits before the model call because that is the one step that costs something unrecoverable.
    let points = r::checkpoints();
    assert_eq!(points.len(), 5);
    assert_eq!(points[0], "after each line spoken");
    assert_eq!(points[1], "after each clip encoded");
    assert_eq!(*points.last().unwrap(), "before the publish model call");

    // How a run ends: the file with its length and weight, or one of the two lines that are not "done".
    assert_eq!(r::STAGE_DONE, "done");
    assert_eq!(
        r::finished_log("final.mp4", 12.34, "18 MB"),
        ">>> final.mp4  (18 MB, took 12.3 s)"
    );
    assert_eq!(r::failure_log(), "production failed \u{2014} see log");
    assert_eq!(r::stopped_log(), "production stopped");

    // The ten steps and the five checkpoints are two different lists: the run walks one, the stop asks at
    // the other, and a test that pinned only their lengths could not tell them apart.
    let stages = r::stages();
    assert_eq!(stages.len(), 10);
    assert_eq!(stages[0].0, "scratch");
    assert_eq!(stages[9].0, "stamp");
}

#[test]
fn f5_2_module_runs_nothing() {
    // §F5.2's steps are ffmpeg subprocesses; this module only plans them, which is the whole reason they can
    // be tested with no recording and no encoder. What a test can check here is that nothing spawns at all.
    let source = include_str!("../../src/produce_render.rs");
    for forbidden in ["std::process", "Command::new"] {
        assert!(
            !source.contains(forbidden),
            "produce_render must not use {forbidden}"
        );
    }
}
