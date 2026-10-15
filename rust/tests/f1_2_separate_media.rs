//! F1.2 Voice separation — the media legs around the model call: `src/separate_media.rs`, plus the
//! reply shape [`naivepost::audio_task::chunk_stems`] reads.
//!
//! These are RULE tests: no socket, no project file, no fake server (the wire round trip is
//! `tests/separation_run.rs`'s job). What is proven here is the SHAPE of each ffmpeg command and what
//! each leg does with its answer, read back from a stub tool that logs its own argv — because "the run
//! called ffmpeg" says nothing if it called ffmpeg with the wrong arguments, and a stub that only
//! succeeds would let a leg report work it never did.
//!
//! Item ids follow §04-prepare.md's step numbers: **s1** decode, **s2** silences, **s3** cut/mix,
//! **s4** join and mux, plus the scratch folder's rules and the reply parse.

use std::path::{Path, PathBuf};

use naivepost::audio_task;
use naivepost::layout::Tree;
use naivepost::separate::{self, CHUNK_MAX_SECONDS};
use naivepost::separate_media as media;
#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;

/// A stub ffmpeg that behaves like the tool where a rule depends on it AND records every invocation,
/// one line per call, into `argv.log` next to it. `body` is what the script does after logging.
///
/// The behaviours are chosen by what the argv asks for, exactly as the real tool branches on them;
/// `exit 0` with no output file is how a leg's "it made nothing" branch gets exercised.
///
/// ONE folder per call, never one per tag: this binary runs its tests in threads over the same
/// `/tmp`, and two stubs sharing a folder both append to one `argv.log` (so `calls()` sees the other
/// test's command) and both write the same output path — which is how one run got `Text file busy`
/// and the next got a clean pass.
fn stub(tag: &str, body: &str) -> PathBuf {
    let dir = unique_dir(&format!("f12-media-{tag}"));
    let script = dir.join("ffmpeg");
    common::write_script(
        &script,
        format!(
            "#!/bin/sh\ncd \"{dir}\" || exit 70\nprintf '%s\\n' \"$*\" >> argv.log\n{body}\n",
            dir = dir.display()
        ),
    )
    .expect("the stub is written");
    std::fs::set_permissions(&script, std::os::unix::fs::PermissionsExt::from_mode(0o755))
        .expect("the stub runs");
    dir
}

/// A temp folder no other test in this binary can be using: the tag says who made it, and the pid plus
/// a per-call counter say which call made it.
fn unique_dir(tag: &str) -> PathBuf {
    static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    common::temp_dir_created(&format!("{tag}-{}-{n}", std::process::id()))
}

/// The stub's recorded calls, oldest first, one argv per line.
fn calls(dir: &Path) -> Vec<String> {
    let text = std::fs::read_to_string(dir.join("argv.log")).unwrap_or_default();
    text.lines().map(str::to_string).collect()
}

/// A file with recognisable bytes, so a copy can be told from a fresh empty file.
fn write(path: &Path, bytes: &[u8]) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("the parent folder");
    }
    std::fs::write(path, bytes).expect("the file is written");
}

/// Copy-the-input stub: creates its output from the first `-i`.
///
/// Written beside the target and renamed onto it rather than `cp`'d straight onto it: ETXTBSY (the
/// "Text file busy" that made this suite flaky) is what writing to an already-existing file returns
/// while another process still holds it open, and a rename replaces the entry instead of writing
/// through it, so it cannot meet that error.
const COPY: &str = r#"last=""
for a in "$@"; do last="$a"; done
first=""
pending=0
for a in "$@"; do
  if [ "$a" = "-i" ]; then pending=1; continue; fi
  if [ "$pending" = 1 ]; then pending=0; first="$a"; break; fi
done
if [ -f "$first" ]; then
  mkdir -p "$(dirname "$last")"
  tmp="$(dirname "$last")/np-part-$$"
  cp -- "$first" "$tmp" && mv -f -- "$tmp" "$last"
fi
"#;

/// S1: the decode goes to 44.1 kHz stereo pcm — `separate::decode_plan`'s own arguments, in the order
/// the rule states them, with the output path the caller named. Asserted from the logged argv rather
/// than from a re-derived string, so a change to either side shows up here.
#[test]
fn f1_2_s1_the_decode_runs_the_configured_ffmpeg_at_44_1khz_stereo() {
    let dir = stub("decode", COPY);
    let source = dir.join("lecture.mkv");
    write(&source, b"mkv-bytes");
    let out = dir.join("stems/lecture.work/lecture.pcm");
    let mut log: Vec<String> = Vec::new();

    let done = media::decode(&dir.join("ffmpeg").display().to_string(), &source, &out, &mut |line| {
        log.push(line.to_string())
    })
    .expect("the decode finishes");

    assert_eq!(done, out, "the decode hands back the file it wrote");
    assert_eq!(std::fs::read(&out).ok().as_deref(), Some("mkv-bytes".as_bytes()));
    let argv = calls(&dir);
    assert_eq!(argv.len(), 1, "one command for one decode: {argv:?}");
    // The plan's own tail, in order: no video, the rate, the channels, the raw format.
    assert!(
        argv[0].contains("-vn -ar 44100 -ac 2 -f s16le"),
        "the decode carries decode_plan()'s arguments verbatim: {}",
        argv[0]
    );
    assert!(argv[0].ends_with(&out.display().to_string()), "and names its output last: {}", argv[0]);
    assert!(argv[0].contains("-i"), "the recording is an input: {}", argv[0]);
    // §8: the command line is logged before it starts, so a slow or hung tool is identifiable in the log.
    assert!(
        log.iter().any(|line| line.contains("44100")),
        "the run's log carries the command: {log:?}"
    );
    assert_eq!(
        separate::decode_plan(),
        vec!["-vn", "-ar", "44100", "-ac", "2", "-f", "s16le"]
            .into_iter()
            .map(String::from)
            .collect::<Vec<_>>(),
        "and the plan itself is still the one §F1.2 S1 names"
    );
}

/// S1's failure branch: a tool that exits 0 without producing the file is an error naming the path.
/// Silence here would put an empty half through the rest of the split and blame the model later.
#[test]
fn f1_2_s1_a_decode_that_writes_nothing_is_an_error_naming_the_file() {
    let dir = stub("no-output", "exit 0");
    let source = dir.join("lecture.mkv");
    write(&source, b"mkv-bytes");
    let out = dir.join("stems/lecture.work/lecture.pcm");

    let err = media::decode(
        &dir.join("ffmpeg").display().to_string(),
        &source,
        &out,
        &mut |_| {},
    )
    .expect_err("no file means no decode");
    assert!(
        err.contains(&out.display().to_string()),
        "the reason names the file that is missing: {err}"
    );
}

/// S2: each quiet stretch is folded to its MIDPOINT — the point farthest from either edge, which is
/// what makes a cut there safe — using [`naivepost::asr::Silence`]'s own rule. Unrelated chatter is
/// ignored, several pairs keep their order, and a gap that never ends (the recording went quiet in its
/// last seconds) folds to its start.
#[test]
fn f1_2_s2_each_silence_folds_to_its_midpoint() {
    // The folding is asserted on ffmpeg's own text, because that is what the leg reads: `subprocess::run`
    // hands back stdout on success, and the detector writes to stderr, so a stub could not otherwise be
    // trusted to have shown the same lines the real tool would.
    let found = media::midpoints(concat!(
        "size=N/A time=00:00:05.00 speed=9.1x\n",
        "[silencedetect @ 0x1] silence_start: 10\n",
        "[silencedetect @ 0x1] silence_end: 14 | silence_duration: 4\n",
        "frame=  100 fps=30\n",
        "[silencedetect @ 0x1] silence_start: 100\n",
        "[silencedetect @ 0x1] silence_end: 102\n",
    ));
    assert_eq!(found, vec![12.0, 101.0], "both pairs at their middles: {found:?}");
    assert_eq!(
        media::midpoints("[silencedetect @ 0x1] silence_start: 640\n"),
        vec![640.0],
        "a gap the file never leaves gets its start"
    );
    assert!(media::midpoints("frame=  100 fps=30\n").is_empty(), "chatter alone is no silence");

    // And the leg that runs it asks for the app's own threshold, read off the module that owns it.
    let dir = stub("silences", COPY);
    let decoded = dir.join("lecture.pcm");
    write(&decoded, b"pcm");
    media::silences(&dir.join("ffmpeg").display().to_string(), &decoded).expect("the run answers");
    let argv = calls(&dir);
    assert_eq!(argv.len(), 1, "one pass over the decode: {argv:?}");
    assert!(
        argv[0].contains("-af silencedetect=n=-35dB:d=0.4 -f null -"),
        "the filter is the app's own quiet pair: {}",
        argv[0]
    );
}

/// S2: a recording with nothing long enough to be quiet is a normal answer — an empty list, meaning
/// the cuts land on the clock — while a silencedetect that fails outright is reported, because a
/// silent-looking file whose measurement failed would otherwise be cut mid-word with no trace why.
#[test]
fn f1_2_s2_no_silence_is_an_answer_and_a_failed_measurement_is_an_error() {
    let dir = stub("quiet-none", "echo frame=  100 fps=30\n");
    let decoded = dir.join("lecture.pcm");
    write(&decoded, b"pcm");
    let found = media::silences(&dir.join("ffmpeg").display().to_string(), &decoded)
        .expect("nothing quiet is not a failure");
    assert!(found.is_empty(), "no gaps, so no moved edges: {found:?}");

    let broken = stub("quiet-fail", "echo \"Error opening input file\" >&2\nexit 69\n");
    let err = media::silences(
        &broken.join("ffmpeg").display().to_string(),
        &decoded,
    )
    .expect_err("a failed measurement must not read as silence");
    assert!(err.contains("silencedetect"), "the reason says which leg broke: {err}");
}

/// S3: a rest made of several stems is mixed with [`separate::amix`], `normalize=0` included, because
/// those stems were one recording a moment ago and must sum back to it. Asserted as the whole filter
/// string once, built through the rule that owns it.
#[test]
fn f1_2_s3_several_stems_become_one_amix_filter_per_input() {
    assert_eq!(media::mix_filter(1), "[0:a]amix=inputs=1:normalize=0");
    assert_eq!(
        media::mix_filter(3),
        "[0:a][1:a][2:a]amix=inputs=3:normalize=0",
        "every stem is an input and none is normalised away"
    );
    assert!(
        media::mix_filter(4).contains(&separate::amix(4)),
        "the mix is built through separate::amix, not restated"
    );
}

/// S3: a chunk is cut from the decode with [`naivepost::asr::cut_plan`]'s `-ss`/`-t` numbers (seek
/// before the input, length after), and its name carries the recording, since that name is what lands
/// in `requests.tsv`'s job column and the upload's filename header.
#[test]
fn f1_2_s3_a_chunk_is_cut_at_the_plan_times_and_named_after_its_recording() {
    let dir = stub("cut", COPY);
    let decoded = dir.join("lecture.pcm");
    write(&decoded, b"pcm-bytes");
    let piece = dir.join("stems/lecture.work").join(media::chunk_file("lecture", 2));
    assert_eq!(media::chunk_file("lecture", 2), "lecture-c02.wav", "lane first, then the part number");
    assert_eq!(media::chunk_name(2), naivepost::asr::part(2), "and the bare name stays asr::part's twin");

    media::cut(
        &dir.join("ffmpeg").display().to_string(),
        &decoded,
        300.0,
        420.5,
        &piece,
        &mut |_| {},
    )
    .expect("the cut runs");
    assert_eq!(std::fs::read(&piece).ok().as_deref(), Some("pcm-bytes".as_bytes()));
    let argv = calls(&dir);
    assert_eq!(argv.len(), 1, "one command per chunk: {argv:?}");
    assert!(
        argv[0].starts_with("-y -ss 300 -t 120.5 -i "),
        "cut_plan's numbers, seek before the input: {}",
        argv[0]
    );
}

/// S4: one part needs no ffmpeg at all — copying it is the same bytes the server sent — so the log
/// stays empty and the tool is never started.
#[test]
fn f1_2_s4_one_part_is_copied_without_calling_ffmpeg() {
    let dir = stub("join-one", COPY);
    let work = dir.join("stems/lecture.work");
    let part = work.join("p000.wav");
    write(&part, b"VOICE-BYTES");
    let out = dir.join("stems/lecture.split-voice.wav");
    let mut log: Vec<String> = Vec::new();

    media::join_parts(
        &dir.join("ffmpeg").display().to_string(),
        std::slice::from_ref(&part),
        &out,
        &work,
        &mut |line| log.push(line.to_string()),
    )
    .expect("one part joins");
    assert_eq!(std::fs::read(&out).ok().as_deref(), Some("VOICE-BYTES".as_bytes()));
    assert!(calls(&dir).is_empty(), "no command was run: {:?}", calls(&dir));
    assert!(log.is_empty(), "and nothing was logged about one: {log:?}");
}

/// S4: several parts go through the concat DEMUXER (no re-encode: they came off the same model), which
/// means a list file of `file '<path>'` lines in the work dir and one command reading it.
#[test]
fn f1_2_s4_several_parts_join_through_a_concat_list_in_the_work_dir() {
    let dir = stub("join-two", COPY);
    let work = dir.join("stems/lecture.work");
    let first = work.join("p000.wav");
    let second = work.join("p001.wav");
    write(&first, b"A");
    write(&second, b"B");
    let out = dir.join("stems/lecture.split-voice.wav");

    media::join_parts(
        &dir.join("ffmpeg").display().to_string(),
        &[first.clone(), second.clone()],
        &out,
        &work,
        &mut |_| {},
    )
    .expect("two parts join");
    let list = work.join("concat.txt");
    let body = std::fs::read_to_string(&list).expect("the list is written next to the parts");
    assert_eq!(
        body,
        format!("file '{}'\nfile '{}'\n", first.display(), second.display()),
        "in order, quoted the way the demuxer wants it"
    );
    let argv = calls(&dir);
    assert_eq!(argv.len(), 1, "one command for the join: {argv:?}");
    assert!(
        argv[0].contains("-f concat -safe 0 -i ") && argv[0].contains("-c copy"),
        "the demuxer, not the filter, and a stream copy: {}",
        argv[0]
    );
    assert!(argv[0].ends_with(&out.display().to_string()), "output last: {}", argv[0]);
}

/// S4/S5: a video's rest is muxed with the ORIGINAL picture copied and FLAC sound — the frames are
/// untouched (a re-encode costs minutes and quality for nothing) and the audio stays lossless because
/// Cut splices on it later.
#[test]
fn f1_2_s4_a_videos_rest_is_muxed_with_the_picture_copied_and_flac_sound() {
    let dir = stub("mux", COPY);
    let source = dir.join("lecture.mkv");
    let rest = dir.join("stems/lecture.work/rest.wav");
    write(&source, b"mkv");
    write(&rest, b"REST-BYTES");
    let out = dir.join("stems/lecture.split-novoice.mkv");

    media::mux_video(
        &dir.join("ffmpeg").display().to_string(),
        &source,
        &rest,
        &out,
        &mut |_| {},
    )
    .expect("the mux runs");
    let argv = calls(&dir);
    assert_eq!(argv.len(), 1, "one command: {argv:?}");
    for fragment in ["-map 0:v", "-map 1:a", "-c:v copy", "-c:a flac"] {
        assert!(argv[0].contains(fragment), "{fragment} belongs in the mux: {}", argv[0]);
    }
    assert!(
        argv[0].contains(&format!("-i {} -i {}", source.display(), rest.display())),
        "the recording first, so 0:v is its picture: {}",
        argv[0]
    );
}

/// §inventory/prepare.md B.1: the scratch lives in `stems/<base>.work` beside the products, and it is
// cleared on entry (junk from a killed run must not be joined into this one) but kept after a failure
/// (the parts are what a person can look at, and what the resume rule reads).
#[test]
fn f1_2_s4_the_work_dir_sits_beside_the_products_and_clears_on_entry_only() {
    let root = unique_dir("f12-work").join("talk.naivepost");
    let tree = Tree::new(&root).expect("a .naivepost folder is a project");
    let work = media::work_dir(&tree, "lecture");
    assert_eq!(work, tree.stems_dir().join("lecture.work"), "stems/<base>.work");
    assert_eq!(
        work.parent().expect("a parent"),
        tree.split_voice("lecture").parent().expect("the voice half's folder"),
        "beside the halves, so one folder holds everything a split made"
    );

    // Absent is the normal case, not a warning.
    media::clear_work(&work).expect("nothing to clear is fine");

    write(&work.join("lecture.pcm"), b"pcm");
    write(&work.join("p000.wav"), b"part");
    media::clear_work(&work).expect("the parts go");
    assert!(!work.exists(), "the whole scratch folder is gone: {:?}", work);
}

/// The two-way answer: one voice, one rest, and the voice index points at the only voice there is.
#[test]
fn f1_2_s3_a_two_way_reply_names_its_voice_and_its_rest() {
    let reply: serde_json::Value =
        serde_json::from_str(r#"{"voice":"/uploads/lecture/voice.wav","rest":"/uploads/lecture/rest.wav"}"#)
            .expect("a reply");
    let stems = audio_task::chunk_stems(&reply).expect("the flat shape parses");
    assert_eq!(stems.voice, Some(0), "the single voice is index 0");
    assert_eq!(stems.voices.len(), 1);
    assert_eq!(stems.rests.len(), 1);
    assert_eq!(stems.voices[0].as_str(), "/uploads/lecture/voice.wav");
    assert_eq!(stems.rests[0].as_str(), "/uploads/lecture/rest.wav");
}

/// A RoFormer's named set arrives in whatever order the server chose: the voice index must point at the
/// VOCALS entry wherever it sits, and the rests keep the non-voice order they came in.
#[test]
fn f1_2_s3_a_named_stem_set_finds_the_voice_wherever_it_sits() {
    let reply: serde_json::Value = serde_json::from_str(
        r#"{"stems":[{"name":"instrumental","path":"/u/inst.wav"},{"name":"vocals","path":"/u/voc.wav"}]}"#,
    )
    .expect("a reply");
    let stems = audio_task::chunk_stems(&reply).expect("the named shape parses");
    assert_eq!(stems.voice, Some(0), "index INTO the voice list, not into the reply");
    assert_eq!(stems.voices[0].as_str(), "/u/voc.wav", "and it is the vocals file");
    assert_eq!(stems.rests.len(), 1);
    assert_eq!(stems.rests[0].as_str(), "/u/inst.wav");
}

/// An HTDemucs answer is four instruments with no names field: one of them is the voice and the other
/// three are the rest, in the order the reply gave them, because the app mixes them and a shuffle would
/// be audible.
#[test]
fn f1_2_s3_a_flat_four_stem_answer_keeps_three_as_the_rest_in_order() {
    let reply: serde_json::Value = serde_json::from_str(
        r#"{"drums":"/u/d.wav","bass":"/u/b.wav","other":"/u/o.wav","vocals":"/u/v.wav"}"#,
    )
    .expect("a reply");
    let stems = audio_task::chunk_stems(&reply).expect("the flat object parses");
    assert_eq!(stems.voices.len(), 1, "exactly one voice: {:?}", stems.voices);
    assert_eq!(stems.voices[0].as_str(), "/u/v.wav");
    assert_eq!(stems.voice, Some(0));
    let rests: Vec<&str> = stems.rests.iter().map(|path| path.as_str()).collect();
    // serde_json keeps an object's insertion order only behind its preserve_order feature, so what is
    // pinned here is the rule (every non-voice stem, none lost or duplicated), not the reply's order.
    assert_eq!(rests.len(), 3, "three rests: {rests:?}");
    for want in ["/u/d.wav", "/u/b.wav", "/u/o.wav"] {
        assert!(rests.contains(&want), "{want} is part of the rest: {rests:?}");
    }
}

/// A chunk of pure silence answers with no paths at all. That is NOT an error — the recording has gaps,
/// and treating one as a failure would fail a whole take for having a pause in it.
#[test]
fn f1_2_s3_a_silent_chunk_is_not_an_error() {
    let reply: serde_json::Value =
        serde_json::from_str(r#"{"voice":null,"silence":true}"#).expect("a reply");
    let stems = audio_task::chunk_stems(&reply).expect("silence is an answer");
    assert_eq!(stems.voice, None, "no voice to place");
    assert!(stems.voices.is_empty() && stems.rests.is_empty(), "and no files either");
}

/// A reply with no paths and no sign of meaning silence is a mistake, and it is shown: an empty answer
/// quietly accepted is how a wrong guess gets filed as work (the same rule `asr_answer` applies).
#[test]
fn f1_2_s3_a_reply_with_no_paths_at_all_is_refused_showing_the_reply() {
    let reply: serde_json::Value = serde_json::from_str(r#"{"result":"busy"}"#).expect("a reply");
    let err = audio_task::chunk_stems(&reply).expect_err("that is not a stem answer");
    assert!(err.contains("busy"), "the server's own sentence is in the reason: {err}");
    assert!(err.contains("no stem paths"), "and it says what was missing: {err}");
}

/// The ceiling the plan is sized by, pinned here so a chunk count asserted anywhere else is anchored to
/// the same number.
#[test]
fn f1_2_s2_the_ceiling_the_chunks_are_cut_to_is_the_model_s_own() {
    assert_eq!(CHUNK_MAX_SECONDS, 300.0, "P.machine.separationChunkSeconds");
    // A silence nearer the nominal edge moves the cut onto itself; the plan still covers the whole file.
    let moved = separate::chunk_plan(640.0, &[298.0]);
    assert_eq!(moved.first().expect("a first piece").1, 298.0, "moved to the silence");
    assert_eq!(moved.last().expect("a last piece").1, 640.0, "and it ends where the file does");
    // No silences at all: the edges stay on the clock, and no piece breaks the ceiling.
    let plain = separate::chunk_plan(640.0, &[]);
    assert_eq!(plain, vec![(0.0, 300.0), (300.0, 600.0), (600.0, 640.0)]);
    for (from, to) in &plain {
        assert!(to - from <= CHUNK_MAX_SECONDS, "a piece over the ceiling: {plain:?}");
    }
}
