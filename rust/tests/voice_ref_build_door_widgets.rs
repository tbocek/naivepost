//! F4.6's BUILD DOOR over the real wire: with no `narrate/voice_ref.wav` on disk, the speak leg does
//! not stop at F4.4's "go build it" refusal — S1's "it is re-cut from the recording on the next line
//! spoken" means the line itself triggers the cut. This drives `speak_leg::speak` with the fake
//! audio.cpp on a loopback port and a fake ffmpeg in Settings, and shows both halves:
//!
//! 1. the missing reference really was cut — the fake ffmpeg ran, and `narrate/voice_ref_base.wav`
//!    and `narrate/voice_ref.wav` exist after the call, so the reference is made by the program and
//!    not by a test planting it;
//! 2. the line then went on to be spoken over the socket, which only happens because S1 stopped
//!    refusing.
//!
//! The other half of F4.4 S1 — a session with nothing to cut from still refuses, and refuses BEFORE
//! any dial — is `tests/f4_4_s1_no_reference_refuses_before_the_server_is_asked.rs`.
//!
//! One scenario per binary (cwd and XDG are process-wide), the shape `speak_tts_wire_widgets` uses.

#![allow(dead_code)]

use std::sync::atomic::{AtomicBool, Ordering};

use naivepost::settings;

#[allow(dead_code)]
mod common;
#[path = "speak_tts_wire_harness.rs"]
mod harness;

use harness::{wav_bytes, FakeAudio, Script};

static RAN_BUILD_DOOR: AtomicBool = AtomicBool::new(false);

/// A fake ffmpeg that records each call's argv and writes a real, server-readable wav to whatever
/// output path was passed last — which is how every pass here ends. Written into the scenario folder
/// and named in Settings, so the seam is the program's own (`FFMPEG` / `settings.ffmpeg`), the same
/// one the app uses in production: nothing in `src` knows the binary is a fake.
fn plant_fake_ffmpeg(root: &std::path::Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let script = root.join("fake-ffmpeg.sh");
    let log = root.join("ffmpeg-argv.log");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" >> '{}'\nfor a in \"$@\"; do out=\"$a\"; done\nprintf 'x' > \"$out\"\n",
            log.display()
        ),
    )
    .expect("fake ffmpeg written");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("+x");
    }
    (script, log)
}

fn run_round() {
    let fake = FakeAudio::start(Script::healthy());
    let fixture = harness::fixture("s1-build-door");
    harness::point_at(&fake.url());
    harness::point_model_at("index-tts2");

    let root = fixture.root.clone();
    let (script, log) = plant_fake_ffmpeg(&root);
    // The voice the session was left on: a slot the project tags. The build cuts from THAT recording.
    naivepost::narrate_data::write_voice(&fixture.tree, "narrator1").expect("voice.txt written");
    {
        let paths = harness::settings_folder();
        let mut conf = settings::read(&paths).expect("settings read back");
        conf.ffmpeg = script.to_string_lossy().into_owned();
        settings::save(&paths, &conf).expect("the fake ffmpeg is what Settings name");
    }

    // A project that tags a recording as narrator 1: that is what the reference gets cut from. The
    // diarized turns and the transcript are planted too, so S2's automatic pick has real solo seconds
    // and real words to clear its floors with, rather than the test telling the rule what to choose.
    let mut project = naivepost::project::Project::default();
    project.language = "pl".to_string();
    project.sources.push(naivepost::project::Source {
        path: "/media/narrator-mic.wav".into(),
        footage: false,
        narrator: 1,
        ..Default::default()
    });
    naivepost::project::save(&project, fixture.tree.dir()).expect("naivepost.json written");
    let lane = "narrator-mic";
    let turns = vec![
        naivepost::requests::Turn { start_sample: 0, end_sample: 96_000, speaker_id: 1 },
        naivepost::requests::Turn { start_sample: 200_000, end_sample: 296_000, speaker_id: 1 },
    ];
    naivepost::requests::write_turns(&fixture.tree, lane, &turns).expect("turns.json written");
    let lines = vec![
        naivepost::textfmt::Line {
            start: 0.0,
            end: 6.0,
            speaker: "narrator".into(),
            text: "one two three four five six seven eight nine ten".into(),
        },
        naivepost::textfmt::Line {
            start: 12.5,
            end: 18.5,
            speaker: "narrator".into(),
            text: "one two three four five six seven eight nine ten".into(),
        },
    ];
    std::fs::create_dir_all(fixture.tree.transcript_fixed_tsv(lane).parent().expect("dir"))
        .expect("transcript folder");
    naivepost::textfmt::write_lines(&lines, &fixture.tree.transcript_fixed_tsv(lane))
        .expect("transcript written");

    // The state F4.4 S1 looks at: NO reference on disk at all.
    assert!(
        !fixture.tree.voice_ref_wav().exists() && !fixture.tree.voice_ref_base_wav().exists(),
        "the scenario starts with no reference"
    );

    // The call. Everything after is the program's own legs: the reference build, then the speak.
    let outcome = harness::speak(&fixture, "calm", "pl");
    if let naivepost::narrate_tts::Outcome::Refused(why) = &outcome {
        panic!("F4.6's door should have built the reference instead of refusing: {why}");
    }

    let base = fixture.tree.voice_ref_base_wav();
    let served = fixture.tree.voice_ref_wav();
    assert!(base.is_file(), "the level pass wrote the base at {}", base.display());
    assert!(served.is_file(), "the shift pass wrote the served copy at {}", served.display());

    let argv = std::fs::read_to_string(&log).expect("the fake recorded its argv");
    assert!(
        argv.contains("loudnorm=I=-16"),
        "S3's level pass really ran: {argv}"
    );
    assert!(
        argv.contains("-ss") && argv.contains("-to"),
        "the pieces were cut at the seconds S2 chose, not the whole recording: {argv}"
    );
    assert!(
        argv.contains("concat"),
        "two pieces were stitched by ffmpeg's concat demuxer: {argv}"
    );

    // The reference the upload carried is the one that was just built, not a planted file: the fake
    // saw the upload and the speech post after the ffmpeg legs, in that order.
    let seen = fake.drain();
    let paths: Vec<String> = seen
        .iter()
        .map(|seen| format!("{} {}", seen.method, seen.path))
        .collect();
    assert!(
        paths.iter().any(|p| p.ends_with("/v1/ui/upload")),
        "the rebuilt reference was uploaded for the line: {paths:?}"
    );
    assert!(
        paths.iter().any(|p| p.ends_with("/v1/audio/speech")),
        "and the line was spoken over it: {paths:?}"
    );
    let ffmpeg_at = std::fs::metadata(&log).map(|m| m.len()).unwrap_or(0);
    assert!(ffmpeg_at > 0, "the build left its argv behind: {ffmpeg_at} bytes");

    RAN_BUILD_DOOR.store(true, Ordering::SeqCst);
}

#[test]
fn f4_6_s1_s3_the_missing_reference_is_cut_by_the_program_before_the_line_is_spoken() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(run_round);
    assert!(
        RAN_BUILD_DOOR.load(Ordering::SeqCst),
        "the F4.6 build-door block never ran"
    );
}
