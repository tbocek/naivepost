//! F1.2 Voice separation (rows with 🗣) — spec/04-prepare.md.
//!
//! Every step of the split lives in [`naivepost::separate`]; ▶ only forwards to it. So these checks
//! assert what the plan, the stem names, the log lines and the two rows say, with ffmpeg's decode and
//! the audio server's answer handed over as arguments — never that a model was actually asked
//! (spec/00-principles.md §5).

use std::fs;
use std::path::{Path, PathBuf};

use naivepost::layout::Tree;
use naivepost::project::{Project, Source};
use naivepost::separate::{self, CHUNK_MAX_SECONDS};

fn temp_dir(tag: &str) -> PathBuf {
    let dir = PathBuf::from(format!(
        "/tmp/naivepost-separate-{}-{}",
        tag,
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A project folder holding the two recordings a session might split: one video with a timestamp in
/// its name (the shape S5 cares about) and one plain audio file.
fn project_folder(tag: &str) -> (PathBuf, Tree) {
    let root = temp_dir(tag);
    let dir = root.join("talk.naivepost");
    fs::create_dir_all(dir.join("sources")).unwrap();
    for name in ["mic.wav", "lecture.mkv"] {
        fs::write(dir.join("sources").join(name), b"audio").unwrap();
    }
    let tree = Tree::new(&dir).expect("a .naivepost folder is a project");
    (root, tree)
}

fn source(path: &str, footage: bool, narrator: u32) -> Source {
    Source {
        path: path.to_string(),
        footage,
        narrator,
        sepvoice: true,
        tracks: vec![],
    }
}

fn project_with(sources: Vec<Source>) -> Project {
    Project {
        sources,
        ..Project::default()
    }
}

/// S4/S5's join step, as a test answers it: the bytes are ffmpeg's, so here they are a word.
fn joined(bytes: &'static [u8]) -> impl FnMut(&Path) -> Result<(), String> {
    move |path| fs::write(path, bytes).map_err(|err| format!("{}: {err}", path.display()))
}

/// S6's `volumedetect` reading, as a test answers it: a level per file name, so the mix, the voice and
/// the rest can each be answered with their own.
fn levels(levels: Vec<(&'static str, &'static str)>) -> impl FnMut(&Path) -> String {
    move |path| {
        let name = path.to_string_lossy().to_string();
        levels
            .iter()
            .find(|(needle, _)| name.contains(needle))
            .map(|(_, level)| level.to_string())
            .unwrap_or_else(|| "-20.0 dB".to_string())
    }
}

fn stems(path: &str) -> Vec<String> {
    path.split(", ").map(str::to_string).collect()
}

/// S1: what goes up is 44.1 kHz stereo pcm — the separation models are music models, and handing them
/// the 16 kHz mono the ASR uses would be asking them to separate something they have never heard.
#[test]
fn f1_2_s1_the_audio_going_up_is_44_1khz_stereo() {
    assert_eq!(separate::SAMPLE_RATE, "44100");
    assert_eq!(separate::CHANNELS, "2");

    let plan = separate::decode_plan();
    let args = plan.join(" ");
    assert!(args.contains("-ar 44100"), "the rate is set: {args}");
    assert!(args.contains("-ac 2"), "two channels: {args}");
    assert!(args.contains("-vn"), "no picture goes to an audio model: {args}");
}

/// S2: the ceiling on one request, and a cut that moves to a silence rather than to a word.
#[test]
fn f1_2_s2_chunks_stay_under_the_ceiling_and_are_cut_at_silences() {
    // P.machine.sepChunkMaxSeconds
    assert_eq!(CHUNK_MAX_SECONDS, 300.0);

    assert_eq!(separate::chunk_plan(40.0, &[]), vec![(0.0, 40.0)]);
    // Exactly the ceiling is one request, not two of which the second is empty.
    assert_eq!(separate::chunk_plan(300.0, &[]), vec![(0.0, 300.0)]);

    let pieces = separate::chunk_plan(640.0, &[]);
    assert_eq!(pieces.len(), 3, "640 s is three requests: {pieces:?}");
    assert!(
        pieces.iter().all(|(from, to)| to - from <= CHUNK_MAX_SECONDS),
        "no piece goes over the ceiling: {pieces:?}"
    );
    assert_eq!(pieces.first().unwrap().0, 0.0);
    assert_eq!(pieces.last().unwrap().1, 640.0);

    // A silence near the nominal edge is where the cut goes instead...
    let pieces = separate::chunk_plan(640.0, &[292.0]);
    assert_eq!(pieces[0].1, 292.0, "the cut moved to the silence");
    // ...and one further away than the seek window is not considered at all, which is what keeps a
    // piece from ever growing past the ceiling.
    let pieces = separate::chunk_plan(640.0, &[250.0]);
    assert_eq!(pieces[0].1, 300.0, "a far silence leaves the nominal edge: {pieces:?}");
    // The recording is cut into whole pieces, so a piece never runs past its end either.
    let last = *pieces.last().unwrap();
    assert!(last.0 < last.1, "no empty tail: {pieces:?}");
}

/// S3: which stem is the voice and which is everything else — a RoFormer's two names, or an HTDemucs
/// four where the rest has to be mixed back together first.
#[test]
fn f1_2_s3_the_voice_stem_and_the_rest_are_picked() {
    let (voice, rest) = separate::pick_stems(&stems("vocals, instrumental")).unwrap();
    assert_eq!(voice, "vocals");
    assert_eq!(rest, vec!["instrumental".to_string()]);

    // The names come from whichever model the server happens to hold, so case is not a question.
    let (voice, rest) = separate::pick_stems(&stems("Voice, INSTRUMENTAL")).unwrap();
    assert_eq!(voice, "Voice");
    assert_eq!(rest, vec!["INSTRUMENTAL".to_string()]);

    // Four stems: the voice is one of them and the rest is the other three, mixed as one file.
    let (voice, rest) = separate::pick_stems(&stems("vocals, drums, bass, other")).unwrap();
    assert_eq!(voice, "vocals");
    assert_eq!(rest, vec!["drums", "bass", "other"]);
    assert_eq!(separate::amix(rest.len()), "amix=inputs=3:normalize=0");
}

/// S3: a model that gave no voice is refused here rather than at the render, minutes later.
#[test]
fn f1_2_s3_a_stem_set_with_no_voice_is_refused() {
    let err = separate::pick_stems(&stems("drums, bass, other")).unwrap_err();
    assert!(err.contains("none of them is the voice"), "{err}");
    assert!(err.contains("drums, bass, other"), "the names are quoted: {err}");
}

/// S3: and a model that gave only the voice is refused too — the recording without the voice is the
/// other half of this, and there is no other half.
#[test]
fn f1_2_s3_a_voice_with_no_other_half_is_refused() {
    let err = separate::pick_stems(&stems("vocals")).unwrap_err();
    assert!(err.contains("there is no other half"), "{err}");
    assert!(err.contains("\"vocals\""), "the stem that came back alone: {err}");
}

/// S4/S5: the halves land under `stems/`, named after the source with its timestamp intact, and a
/// video's rest keeps a picture — so it comes back as mkv while an audio file stays a wav.
#[test]
fn f1_2_s4_a_videos_rest_is_muxed_as_mkv_and_an_audio_keeps_wav() {
    let (root, tree) = project_folder("container");
    let mut project = project_with(vec![source("project:sources/lecture.mkv", true, 0)]);

    let outcome = separate::separate(
        &mut project,
        &tree,
        &root,
        0,
        40.0,
        &[],
        "bs-roformer",
        joined(b"pcm"),
        levels(vec![]),
    )
    .expect("the split runs");
    assert!(outcome.rest.ends_with(".split-novoice.mkv"), "{}", outcome.rest);

    let mut project = project_with(vec![source("project:sources/mic.wav", false, 1)]);
    let outcome = separate::separate(
        &mut project,
        &tree,
        &root,
        0,
        40.0,
        &[],
        "bs-roformer",
        joined(b"pcm"),
        levels(vec![]),
    )
    .expect("the split runs");
    assert!(outcome.rest.ends_with(".split-novoice.wav"), "{}", outcome.rest);
    // The voice half is always a wav: it is what Narrate reads.
    assert!(outcome.voice.ends_with(".split-voice.wav"), "{}", outcome.voice);
}

/// S5: both halves are on disk where the tree says they belong, timestamp and all — that is what
/// "both halves already there?" asks about on the next run.
#[test]
fn f1_2_s5_both_halves_land_in_stems_with_the_timestamp_kept() {
    let (root, tree) = project_folder("stamps");
    let name = "lecture_2024-06-01_10-11-12.mkv";
    fs::write(tree.dir().join("sources").join(name), b"audio").unwrap();
    let mut project = project_with(vec![source(&format!("project:sources/{name}"), true, 0)]);

    let outcome = separate::separate(
        &mut project,
        &tree,
        &root,
        0,
        40.0,
        &[],
        "bs-roformer",
        joined(b"pcm"),
        levels(vec![]),
    )
    .expect("the split runs");

    let voice = tree.split_voice(name.trim_end_matches(".mkv"));
    let rest = tree.split_novoice(name.trim_end_matches(".mkv"), "mkv");
    assert!(voice.to_string_lossy().ends_with(".split-voice.wav"));
    assert!(rest.to_string_lossy().ends_with(".split-novoice.mkv"));
    assert!(voice.exists(), "{} was not written", voice.display());
    assert!(rest.exists(), "{} was not written", rest.display());
    assert_eq!(
        outcome.voice,
        "project:stems/lecture_2024-06-01_10-11-12.split-voice.wav"
    );
}

/// S6: what is about to happen is said before the first chunk goes up — minutes of GPU with nothing on
/// screen is otherwise indistinguishable from a hang — and what came back is said after.
#[test]
fn f1_2_s6_the_log_says_what_is_about_to_happen_and_what_came_back() {
    let (root, tree) = project_folder("log");
    let mut project = project_with(vec![source("project:sources/lecture.mkv", true, 0)]);

    let outcome = separate::separate(
        &mut project,
        &tree,
        &root,
        0,
        640.0,
        &[],
        "bs-roformer",
        joined(b"pcm"),
        levels(vec![
            ("split-voice.wav", "-22.0 dB"),
            ("split-novoice.mkv", "-19.0 dB"),
        ]),
    )
    .expect("the split runs");

    let start = ">>> [lecture.mkv] splitting the voice off 640.0 s in 3 part(s) (bs-roformer)";
    let index = outcome
        .logs
        .iter()
        .position(|line| line == start)
        .unwrap_or_else(|| panic!("no start line in {:?}", outcome.logs));
    let split = outcome
        .logs
        .iter()
        .position(|line| line.starts_with(">>> [lecture.mkv] split into "))
        .unwrap_or_else(|| panic!("no split line in {:?}", outcome.logs));
    assert!(index < split, "the promise comes before the result");
    let loudness = outcome
        .logs
        .iter()
        .find(|line| line.contains("the mix averaged"))
        .expect("a loudness report");
    assert!(loudness.contains("the voice half -22.0 dB"), "{loudness}");
    assert!(loudness.contains("the rest -19.0 dB"), "{loudness}");

    let split = &outcome.logs[split];
    assert!(split.contains(".split-novoice.mkv"), "{split}");
    assert!(split.contains(".split-voice.wav"), "{split}");
}

/// S6: the balance between the halves is worth a sentence. A rest 13 dB under the mix is a recording
/// the model heard as voice from end to end, and the half named for the room has next to nothing in
/// it — found by ear a day later, that reads as "the footage is silent", and it is not the footage.
#[test]
fn f1_2_s6_a_rest_far_under_the_mix_warns() {
    let warning = separate::lopsided("-20.0 dB", "-31.0 dB").expect("11 dB down is worth saying");
    assert!(warning.contains("11 dB under the mix"), "{warning}");
    assert!(warning.contains("cut from the original instead of the split"), "{warning}");

    assert!(separate::lopsided("-20.0 dB", "-25.0 dB").is_none(), "5 dB is a normal split");
    // Exactly at the bound it is said: the spec says ">= 10 dB".
    assert!(separate::lopsided("-20.0 dB", "-30.0 dB").is_some());
    // A level that could not be measured says nothing — a missing number is not evidence.
    assert!(separate::lopsided("", "-31.0 dB").is_none());
    assert!(separate::lopsided("-20.0 dB", "n/a").is_none());

    let report = separate::loudness_report("lecture.mkv", "-20.0 dB", "-21.0 dB", "-35.0 dB");
    assert!(report.starts_with(">>> [lecture.mkv] the mix averaged -20.0 dB"), "{report}");
    assert!(report.ends_with("cut from the original instead of the split."), "{report}");
}

/// S6/S7: both halves on disk means nothing is redone — one line in the log and the row stays as it
/// was, because a split that ran twice would put a voice through the model again.
#[test]
fn f1_2_s6_both_halves_there_is_a_line_and_no_work() {
    let (root, tree) = project_folder("already");
    fs::create_dir_all(tree.stems_dir()).unwrap();
    fs::write(tree.split_voice("lecture"), b"voice").unwrap();
    fs::write(tree.split_novoice("lecture", "mkv"), b"rest").unwrap();
    let mut project = project_with(vec![source("project:sources/lecture.mkv", true, 1)]);

    let outcome = separate::separate(
        &mut project,
        &tree,
        &root,
        0,
        640.0,
        &[],
        "bs-roformer",
        joined(b"pcm"),
        levels(vec![]),
    )
    .expect("nothing to do is not a failure");

    assert_eq!(outcome.logs, vec![">>> lecture.mkv already split".to_string()]);
    assert_eq!(project.sources.len(), 1, "the row stayed one row");
    assert!(project.sources[0].sepvoice, "untouched: no wish was granted or cleared");
    assert_eq!(project.sources[0].path, "project:sources/lecture.mkv");
}

/// S7: the row becomes two ordinary rows, so nothing downstream is taught about stems — the rest
/// where the file was with its 🎥, the voice as a track of its own with the 🎤 slot, and neither
/// asking to be split again.
#[test]
fn f1_2_s7_one_row_becomes_two_and_the_wish_is_cleared() {
    let (root, tree) = project_folder("rows");
    let mut project = project_with(vec![source("project:sources/lecture.mkv", true, 2)]);

    separate::separate(
        &mut project,
        &tree,
        &root,
        0,
        40.0,
        &[],
        "bs-roformer",
        joined(b"pcm"),
        levels(vec![]),
    )
    .expect("the split runs");

    assert_eq!(project.sources.len(), 2);
    let rest = &project.sources[0];
    let voice = &project.sources[1];
    assert!(rest.path.ends_with(".split-novoice.mkv"), "{}", rest.path);
    assert!(voice.path.ends_with(".split-voice.wav"), "{}", voice.path);
    assert!(rest.footage, "the rest keeps the footage");
    assert!(!voice.footage, "a wav is never footage");
    assert_eq!(rest.narrator, 0, "the rest takes nobody's slot");
    assert_eq!(voice.narrator, 2, "the voice keeps the slot the row held");
    assert!(!rest.sepvoice && !voice.sepvoice, "the wish is cleared on both");
    assert!(rest.tracks.is_empty() && voice.tracks.is_empty());

    // A second ▶ over the two new rows splits neither: their names are already split products.
    let again = separate::separate(
        &mut project,
        &tree,
        &root,
        1,
        40.0,
        &[],
        "bs-roformer",
        joined(b"pcm"),
        levels(vec![]),
    );
    assert!(again.is_err(), "a voice has no voice left to split off");
    assert_eq!(project.sources.len(), 2, "and nothing was nested");
}

/// S7: both halves are inside the project, so they survive F0.10's Save as — a stored absolute path
/// would leave them behind when the folder moved.
#[test]
fn f1_2_s7_the_stored_paths_stay_inside_the_project() {
    let (root, tree) = project_folder("stored");
    let mut project = project_with(vec![source("project:sources/mic.wav", false, 1)]);

    let outcome = separate::separate(
        &mut project,
        &tree,
        &root,
        0,
        40.0,
        &[],
        "bs-roformer",
        joined(b"pcm"),
        levels(vec![]),
    )
    .expect("the split runs");

    assert!(outcome.rest.starts_with("project:stems/"), "{}", outcome.rest);
    assert!(outcome.voice.starts_with("project:stems/"), "{}", outcome.voice);
    for row in &project.sources {
        assert!(row.path.starts_with("project:stems/"), "{}", row.path);
    }
}
