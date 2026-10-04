//! F1.3 S1 — the decode that makes `voice16k.wav`: mono 16 kHz, once, unless it is already there.
//!
//! [`naivepost::decode_audio`] is the leg a ▶ Prepare run reaches first for a recording row (§04-prepare.md
//! F1.3 S1). These tests prove the command's SHAPE and the resume rule by reading back what a stub ffmpeg
//! was actually asked to do: a test that only asserted "the file exists afterwards" would pass on a decode
//! at the wrong rate, and a wrong rate shifts every word time the pass later writes.
//!
//! The round trip against a fake audio.cpp is `tests/f1_3_run_path.rs`; nothing here dials a server.

use std::path::{Path, PathBuf};

use naivepost::decode_audio;
use naivepost::layout::Tree;
use naivepost::transcribe;
#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::temp_dir_created as temp_dir;

/// A stub ffmpeg that logs each invocation (one argv per line into `argv.log`) and creates the file its
/// argv names, the way the tool does: the last argument is the output unless it is `-`.
fn stub(tag: &str) -> PathBuf {
    let dir = temp_dir(&format!("f13-decode-{tag}"));
    let script = dir.join("ffmpeg");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\ncd \"{dir}\" || exit 70\nprintf '%s\\n' \"$*\" >> argv.log\n{body}\n",
            dir = dir.display(),
            body = if tag == "silent-tool" {
                "exit 0"
            } else {
                "last=\"\"\nfor a in \"$@\"; do last=\"$a\"; done\n[ -n \"$last\" ] && [ \"$last\" != \"-\" ] && printf 'pcm' > \"$last\""
            }
        ),
    )
    .expect("the stub is written");
    std::fs::set_permissions(&script, std::os::unix::fs::PermissionsExt::from_mode(0o755))
        .expect("the stub runs");
    dir
}

/// The stub's recorded calls, oldest first.
fn calls(dir: &Path) -> Vec<String> {
    let text = std::fs::read_to_string(dir.join("argv.log")).unwrap_or_default();
    text.lines().map(str::to_string).collect()
}

/// A session folder holding one recording and NO decoded audio, which is the state S1 acts on.
fn session(tag: &str) -> (Tree, PathBuf) {
    let root = temp_dir(&format!("f13-proj-{tag}")).join("talk.naivepost");
    let tree = Tree::new(&root).expect("a .naivepost folder is a project");
    let source = root.join("sources/lecture.wav");
    std::fs::create_dir_all(source.parent().expect("a sources folder")).expect("the folder");
    std::fs::write(&source, b"mkv-bytes").expect("the recording");
    (tree, source)
}

/// S1: with no `voice16k.wav` the run decodes the source to mono 16 kHz — the rate
/// [`naivepost::transcribe::SAMPLE_RATE`] states, because that constant is what turns this file's
/// samples into seconds everywhere after this.
#[test]
fn f1_3_s1_the_run_decodes_the_source_to_mono_16khz_when_it_is_absent() {
    let tools = stub("decode");
    let (tree, source) = session("absent");
    assert!(!decode_audio::already_decoded(&tree, LANE), "the state S1 acts on");
    let mut log: Vec<String> = Vec::new();

    let pcm = decode_audio::ensure(
        &tree,
        LANE,
        &source,
        &tools.join("ffmpeg").display().to_string(),
        &mut |line| log.push(line.to_string()),
    )
    .expect("the decode runs");

    assert_eq!(pcm, tree.voice16k_wav(LANE), "the path every later step reads");
    assert!(pcm.exists(), "{} was written", pcm.display());
    assert!(decode_audio::already_decoded(&tree, LANE), "and now the marker is there");
    let argv = calls(&tools);
    assert_eq!(argv.len(), 1, "one command for one decode: {argv:?}");
    assert!(
        argv[0].contains("-ar 16000 -ac 1 -c:a pcm_s16le"),
        "mono 16 kHz signed 16-bit, in order: {}",
        argv[0]
    );
    assert!(argv[0].starts_with("-y -i "), "the recording is the input: {}", argv[0]);
    assert!(argv[0].ends_with(&pcm.display().to_string()), "and its output is last: {}", argv[0]);
    assert_eq!(
        decode_audio::decode_plan(),
        vec!["-vn", "-ar", "16000", "-ac", "1", "-c:a", "pcm_s16le"]
            .into_iter()
            .map(String::from)
            .collect::<Vec<_>>(),
        "the plan is built from SAMPLE_RATE, not a literal beside it"
    );
    assert_eq!(
        transcribe::SAMPLE_RATE,
        16_000,
        "and that constant is still the rate the word times are divided by"
    );
    // §8: the command line reaches the log before it runs, so a hung decode is identifiable.
    assert!(log.iter().any(|line| line.contains("-ar 16000")), "the run logged it: {log:?}");
}

const LANE: &str = "lecture";

/// S1's resume rule ("unless present"): an existing decode is returned untouched and the tool is never
/// started. Re-decoding a two-hour take on every ▶ would be the most expensive thing the run does.
#[test]
fn f1_3_s1_an_existing_voice16k_is_not_decoded_again() {
    let tools = stub("resume");
    let (tree, source) = session("present");
    let pcm = tree.voice16k_wav(LANE);
    std::fs::create_dir_all(pcm.parent().expect("the input folder")).expect("the folder");
    std::fs::write(&pcm, b"already-here").expect("the earlier decode");

    let back = decode_audio::ensure(
        &tree,
        LANE,
        &source,
        &tools.join("ffmpeg").display().to_string(),
        &mut |_| {},
    )
    .expect("nothing needed doing");

    assert_eq!(back, pcm);
    assert_eq!(std::fs::read(&pcm).ok().as_deref(), Some("already-here".as_bytes()), "untouched");
    assert!(calls(&tools).is_empty(), "no ffmpeg ran: {:?}", calls(&tools));
}

/// A tool that exits successfully without producing the file is an error naming the file. Silence here
/// would send an empty decode to the ASR box and blame the model for hearing nothing.
#[test]
fn f1_3_s1_a_decode_that_produces_nothing_is_an_error_naming_the_file() {
    let tools = stub("silent-tool");
    let (tree, source) = session("nothing");
    let err = decode_audio::ensure(
        &tree,
        LANE,
        &source,
        &tools.join("ffmpeg").display().to_string(),
        &mut |_| {},
    )
    .expect_err("no file means no decode");
    assert!(
        err.contains(&tree.voice16k_wav(LANE).display().to_string()),
        "the reason names the missing file: {err}"
    );
    assert!(!tree.voice16k_wav(LANE).exists(), "and nothing was left behind");
}
