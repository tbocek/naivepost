//! The voice reference through real ffmpeg: whose voice it holds, measured by pitch.

use std::path::Path;
use std::process::Command;

use naivepost::layout::Tree;
use naivepost::narrate_data::{self, Take};
use naivepost::speak_leg::build_missing_reference;
use naivepost::voice_ref;

use crate::common;

fn tone(hz: u32, seconds: u32, out: &Path) {
    std::fs::create_dir_all(out.parent().unwrap()).unwrap();
    let source = format!("sine=frequency={hz}:duration={seconds}");
    let made = Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-f", "lavfi", "-i", &source])
        .arg(out)
        .status()
        .expect("ffmpeg runs");
    assert!(made.success());
}

/// Which of these pitches the file holds most of.
fn strongest(file: &Path, pitches: &[u32]) -> u32 {
    let level = |hz: u32| -> f64 {
        let out = Command::new("ffmpeg")
            .args(["-hide_banner", "-i"])
            .arg(file)
            .arg("-af")
            .arg(format!("bandpass=f={hz}:width_type=h:w=40,volumedetect"))
            .args(["-f", "null", "-"])
            .output()
            .expect("ffmpeg runs");
        String::from_utf8_lossy(&out.stderr)
            .split("max_volume:")
            .nth(1)
            .and_then(|rest| rest.split_whitespace().next())
            .and_then(|db| db.parse().ok())
            .unwrap_or(f64::NEG_INFINITY)
    };
    *pitches
        .iter()
        .max_by(|a, b| level(**a).total_cmp(&level(**b)))
        .unwrap()
}

fn project(tag: &str) -> (std::path::PathBuf, Tree) {
    let root = common::temp_dir_created(tag);
    let dir = root.join("voice.naivepost");
    std::fs::create_dir_all(&dir).unwrap();
    tone(500, 20, &dir.join("one.wav"));
    tone(2000, 20, &dir.join("two.wav"));
    std::fs::write(
        dir.join("naivepost.json"),
        r#"{"sources": [{"path": "project:one.wav", "narrator": 1},
                        {"path": "project:two.wav", "narrator": 2}]}"#,
    )
    .unwrap();
    let tree = Tree::new(&dir).unwrap();
    (root, tree)
}

fn pick(tree: &Tree, recording: &str, picked: &[(f64, f64)]) {
    let takes: Vec<Take> = picked.iter().map(|&(s, e)| Take { s, e }).collect();
    narrate_data::save_takes(tree, recording, &takes).unwrap();
}

#[test]
fn each_narrator_is_cut_from_their_own_project_recording() {
    let (root, tree) = project("reference-narrator");
    narrate_data::write_voice(&tree, "narrator2").unwrap();
    pick(&tree, "two", &[(1.0, 6.0)]);
    let built = build_missing_reference(&tree).expect("the second narrator builds");
    assert_eq!(strongest(&built.served, &[500, 2000]), 2000);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn new_takes_replace_the_old_parts_and_a_shift_builds() {
    let (root, tree) = project("reference-parts");
    let dir = tree.dir().to_path_buf();
    pick(&tree, "one", &[(1.0, 4.0), (6.0, 9.0)]);
    build_missing_reference(&tree).expect("two takes build");
    let first = voice_ref::wav_seconds(&narrate_data::served_reference(&tree)).unwrap();

    // Same parts, other sound: a part left standing would still read as the old tone.
    tone(1200, 20, &dir.join("one.wav"));
    voice_ref::invalidate_for_change(&tree, voice_ref::Changed::Takes);
    pick(&tree, "one", &[(2.0, 3.0), (7.0, 8.0)]);
    let built = build_missing_reference(&tree).expect("the new takes build");
    assert_eq!(strongest(&built.served, &[500, 1200]), 1200);
    let second = voice_ref::wav_seconds(&built.served).unwrap();
    assert!(second < first - 2.0, "{first} then {second}");

    narrate_data::write_pitch(&tree, 2.0).unwrap();
    voice_ref::invalidate_for_change(&tree, voice_ref::Changed::Takes);
    build_missing_reference(&tree).expect("a shifted reference builds");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_voices_folder_file_is_the_reference_whole() {
    let (root, tree) = project("reference-file");
    // Each suite file runs in a process of its own, so this reaches only these tests.
    unsafe { std::env::set_var(naivepost::settings::LEGACY_MODELS, &root) };
    let folder = naivepost::ui::narrate_voices::voices_folder();
    assert!(folder.starts_with(&root), "{}", folder.display());
    std::fs::create_dir_all(&folder).unwrap();
    let file = folder.join("np-test-reference-file.wav");
    tone(1200, 5, &file);
    narrate_data::write_voice(&tree, "np-test-reference-file.wav").unwrap();
    let built = build_missing_reference(&tree).expect("a file voice needs no narrator");
    assert_eq!(strongest(&built.served, &[500, 1200, 2000]), 1200);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn takes_past_the_end_leave_no_reference_to_clone() {
    let (root, tree) = project("reference-past-end");
    pick(&tree, "one", &[(50.0, 55.0)]);
    let why = build_missing_reference(&tree).unwrap_err();
    assert!(why.contains("no sound"), "{why}");
    assert!(narrate_tts_problem(&tree), "nothing looks usable");
    let _ = std::fs::remove_dir_all(root);
}

fn narrate_tts_problem(tree: &Tree) -> bool {
    naivepost::narrate_tts::reference_problem(tree).is_some()
}

#[test]
fn a_wav_is_measured_by_its_own_channels_and_chunks() {
    let root = common::temp_dir_created("wav-seconds");
    let stereo = root.join("stereo.wav");
    let made = Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-f", "lavfi", "-i"])
        .arg("sine=frequency=440:duration=10:sample_rate=24000")
        .args(["-ac", "2"])
        .arg(&stereo)
        .status()
        .expect("ffmpeg runs");
    assert!(made.success());
    let seconds = voice_ref::wav_seconds(&stereo).unwrap();
    assert!((seconds - 10.0).abs() < 0.05, "{seconds}");
    let _ = std::fs::remove_dir_all(root);
}
