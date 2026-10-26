//! F1.6 Frames per video — the ▶ path, spec/04-prepare.md.
//!
//! `tests/suite/frames.rs` proves [`naivepost::frames`]'s rules with closures and no process. This file is
//! the other half: the frames stage walked exactly as a Prepare run walks it
//! (`run_stages::plan(Step::Prepare)` → `run_stages::drive_walk`) over a real session folder, against
//! stubbed ffmpeg/ffprobe, so what is asserted is the set of files the pass owns — `scenes.tsv`, the
//! `.frames` marker and the frame pictures themselves — plus the ffmpeg calls that made them. A rule test
//! cannot see the two bugs this file exists for: a scene sweep whose scores were never read, and a marker
//! that says "extracted" about frames nobody wrote.
//!
//! The three parameters the pass runs on are pinned here after their §10 tokens as well, from the
//! catalogue rather than from the constants: `P.machine.sceneThreshold` (a score must be OVER it),
//! `P.machine.sceneMinGapSeconds` (nearer changes merge into the first) and `P.eng.frameGridSeconds`
//! (the grid restarted at every change).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use crate::common;
use common::temp_dir_tagged;
use naivepost::layout::Tree;
use naivepost::project::{Project, Source};
use naivepost::requests;
use naivepost::run::{self, Subprocesses};
use naivepost::run_stages;
use naivepost::settings::{self, Conf, Paths};
use naivepost::{clock, frames, params};

/// The stamp the recording's name carries, which is what puts its frames on the file's own clock (§S4).
/// OBS's spelling, the one `clock::name_stamp` reads year-first; it is the head of [`LANE`].
const STAMP: &str = "2026-05-01 10-00-00";
/// The lane of every scene here: the stem of the recording's name, which is what names `scenes.tsv`,
/// `.frames` and the frame folder (§F1.3 stores under the lane).
const LANE: &str = "2026-05-01 10-00-00-lecture";
/// The row's path, relative to the session folder like every stored path.
fn recording() -> String {
    format!("sources/{LANE}.mkv")
}

/// The env and the working folder belong to the PROCESS and cargo runs these tests as threads, so the
/// tests that move them take them one at a time and put them back before asserting.
static ENV: Mutex<()> = Mutex::new(());
static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// One scene: a session folder with one footage row, a settings folder whose conf names this scene's
/// stub tools, and the recorded ffmpeg calls. `duration` is what the ffprobe stub answers, so it IS the
/// take's length as far as the program can know it.
struct Scene {
    root: PathBuf,
    /// The scene's own settings folder, so a test can re-read the conf the door re-reads per request (§1).
    paths: Paths,
    tools: PathBuf,
    tree: Tree,
    /// What the last `walk` logged, so a later assertion can look at the commands the walk echoed.
    last_log: Mutex<Vec<String>>,
}

impl Scene {
    fn new(duration: &str) -> Self {
        let root = temp_dir_tagged(&format!(
            "f16-run-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        let session = naivepost::startup::session_dir(&root);
        std::fs::create_dir_all(session.join("sources")).expect("the sources folder");
        std::fs::write(session.join(recording()), b"mkv-bytes").expect("the recording");
        // The conf gets the ROOTS: `settings::paths_from` appends the `naivepost` folder itself, so
        // handing it `cfg/naivepost` would read a default conf off the real `$HOME`.
        let paths = Paths {
            config_dir: root.join("cfg/naivepost"),
            data_dir: root.join("data/naivepost"),
        };
        let tools = stubs(&format!("{}-{}", std::process::id(), duration), duration);
        let mut conf = Conf::default();
        conf.ffmpeg = tools.join("ffmpeg").to_string_lossy().into_owned();
        settings::save(&paths, &conf).expect("the conf is written");
        naivepost::project::save(
            &Project {
                sources: vec![Source {
                    path: recording(),
                    footage: true,
                    narrator: 0,
                    sepvoice: false,
                    tracks: vec![],
                }],
                ..Project::default()
            },
            &session,
        )
        .expect("the project file");
        let tree = Tree::new(session).expect("the session tree");
        Scene {
            root,
            paths,
            tools,
            tree,
            last_log: Mutex::new(Vec::new()),
        }
    }

    /// Lay out a scene whose row names a video that is NOT on disk: the honest-failure case.
    fn missing_video() -> Self {
        let scene = Scene::new("20.0");
        std::fs::remove_file(naivepost::startup::session_dir(&scene.root).join(recording()))
            .expect("gone");
        scene
    }

    /// Point the process at this scene exactly as the app resolves both.
    fn enter(&self) {
        std::env::set_var("XDG_CONFIG_HOME", self.root.join("cfg"));
        std::env::set_var("XDG_DATA_HOME", self.root.join("data"));
        std::env::set_current_dir(&self.root).expect("the scene is the working folder");
    }

    fn leave(&self) {
        std::env::remove_var("XDG_CONFIG_HOME");
        std::env::remove_var("XDG_DATA_HOME");
        std::env::set_current_dir(env!("CARGO_MANIFEST_DIR")).expect("back to the crate folder");
    }

    /// The frames stage only, planned from the project file the way ▶ plans it.
    fn stages(&self) -> Vec<run_stages::Stage> {
        let project = naivepost::project::load(&naivepost::startup::session_dir(&self.root))
            .expect("the project");
        run_stages::plan(run::Step::Prepare, &project)
            .into_iter()
            .filter(|stage| stage.name == "frames")
            .collect()
    }

    /// Walk it. Returns the drive's answer and every line the walk logged, oldest first — the same pair
    /// `say_outcome` prints for the run bar, so a failure is a value a test can name.
    ///
    /// The lines come back with ffmpeg's command echoes left in, because that is what the walk logs; the
    /// tests below read them with [`lines_of`] rather than by position, since one row is many commands.
    fn walk(&self) -> (run_stages::Drive, Vec<String>) {
        let mut queue = naivepost::runqueue::Queue::new();
        let mut procs = Subprocesses::default();
        let mut log: Vec<String> = Vec::new();
        let drive = run_stages::drive_walk(
            &self.stages(),
            &mut queue,
            || false,
            || false,
            |line| {
                log.push(line.to_string());
            },
            &mut procs,
        );
        run_stages::say_outcome(&drive, |line| log.push(line.to_string()));
        *self.last_log.lock().expect("the log lock") = log.clone();
        (drive, log)
    }

    /// Every ffmpeg call the stub was asked to make, oldest first, one argv per line.
    fn calls(&self) -> Vec<String> {
        std::fs::read_to_string(self.tools.join("argv.log"))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    /// Every line the last walk logged, including ffmpeg's command echoes — which is where the tools'
    /// own paths appear, since the stub's `argv.log` holds arguments only.
    fn log_lines(&self) -> Vec<String> {
        self.last_log.lock().expect("the log lock").clone()
    }

    /// The frame pictures on disk, sorted by name (which is sorted by second, S4's naming).
    fn frames_on_disk(&self) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(self.tree.frames_dir(LANE))
            .map(|entries| {
                entries
                    .filter_map(|entry| entry.ok())
                    .map(|entry| entry.file_name().to_string_lossy().into_owned())
                    .filter(|name| name.ends_with(".jpg"))
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }
}

/// The lines that are the PASS's own sentences: its `>>> [lane]` lines and its recorded progress strings,
/// with ffmpeg's command echoes (indented four) and the raw `-progress` output left out, because one row
/// is dozens of commands and a test that read "the last line" would be reading whichever ffmpeg ran last.
fn said(log: &[String]) -> Vec<&str> {
    log.iter()
        .map(String::as_str)
        .filter(|line| {
            line.starts_with(">>> [") || line.starts_with("!!! ") || line.starts_with("extracting ")
        })
        .collect()
}

/// Walk a scene under the one guard the env needs, and hand back what the walk came to.
fn walk(scene: &Scene) -> (run_stages::Drive, Vec<String>) {
    let _guard = ENV.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    scene.enter();
    let outcome = scene.walk();
    scene.leave();
    outcome
}

/// The instant the source starts at — the session's zero plus its offset, which the door names frames by.
/// Computed the same way the door computes it: the clock over the WHOLE session's base names.
fn clock_start(scene: &Scene) -> f64 {
    let project = naivepost::project::load(&naivepost::startup::session_dir(&scene.root))
        .expect("the project");
    let names: Vec<&str> = project
        .sources
        .iter()
        .map(|source| source.path.rsplit('/').next().unwrap_or(&source.path))
        .collect();
    let clock = clock::clock(&names);
    clock.zero + clock.offset(0)
}

/// ffmpeg's `metadata=print` output for a short clip, copied from `tests/suite/frames.rs`: four frames a
/// second, one animated slide change around 1.0 s (over `P.machine.sceneThreshold` on several frames
/// running, so they merge), a score exactly ON the threshold at 2.0 s, a second change at 5.0 s, and the take's
/// last picture at 19.75 s (F1.6 S3: the grid ends at the picture's end).
fn scene_log() -> String {
    [
        "frame:0    pts:0       pts_time:0",
        "lavfi.scd.score=0.118",
        "frame:1    pts:400     pts_time:0.25",
        "lavfi.scd.score=0.104",
        "frame:4    pts:1600    pts_time:1.0",
        "streamindex:0   streamid:N/A",
        "tag:lavfi.scd.score=58.310",
        "frame:5    pts:2000    pts_time:1.25",
        "tag:lavfi.scd.score=41.200",
        "frame:6    pts:2400    pts_time:1.4",
        "tag:lavfi.scd.score=12.900",
        "frame:8    pts:3200    pts_time:2.0",
        "tag:lavfi.scd.score=1.0",
        "frame:9    pts:3600    pts_time:2.25",
        "tag:lavfi.scd.score=0.090",
        "frame:20   pts:8000    pts_time:5.0",
        "tag:lavfi.scd.score=77.400",
        // The sweep scores every picture to the end of the take, and the grid runs to its last one.
        "frame:79   pts:31600   pts_time:19.75",
        "tag:lavfi.scd.score=0.050",
    ]
    .join("\n")
}

/// The stub tools. `ffprobe` answers `duration`; `ffmpeg` logs every argv into `argv.log`, answers a
/// `scdet` call with [`scene_log`] on stdout (where `metadata=print:file=-` really puts it), and treats
/// any other call as a frame extraction: it prints an `-progress` line and writes the file named just
/// before `-progress`. A stub that only succeeded would let a pass that wrote no pictures look like work.
///
/// The script is written to a fixed path and called through a wrapper whose OWN name has no spaces in it:
/// this recording's stamped name (`2026-05-01 10-00-00-lecture.mkv`) carries a space, and a conf naming a
/// tool path with a space is what the program passes straight to `Command::new`.
fn stubs(tag: &str, duration: &str) -> PathBuf {
    // A DISTINCT folder per call: a shared tag would have the next scene's `remove_dir_all` delete the
    // stubs a previous scene's conf still names.
    let dir = temp_dir_tagged(&format!("np-f16-tools-{tag}"));
    let probe = dir.join("ffprobe");
    common::write_script(&probe, format!("#!/bin/sh\necho \"{duration}\"\n"))
        .expect("the probe stub");
    // Base64 keeps the score report (quotes, tabs, newlines) out of the shell quoting entirely.
    let report = base64_line(&scene_log());
    // The frame's path is the argument BEFORE `-progress` (`extract_plan` ends with the output and the
    // door appends its progress flag), so `out` is reset at the flag rather than left holding it.
    let script = dir.join("ffmpeg.sh");
    common::write_script(
        &script,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"{dir}/argv.log\"\ncase \"$*\" in\n  *scdet*)\n  printf '%s' \"$(printf '{report}' | base64 -d)\"\n  exit 0\n    ;;\nesac\nout=''\nprev=''\nfor a in \"$@\"; do\n  if [ \"$a\" = \"-progress\" ]; then break; fi\n  out=\"$a\"\ndone\nprintf 'out_time_us=250000\\n'\nmkdir -p \"$(dirname -- \"$out\")\"\nprintf JPEG > \"$out\"\n[ -f \"$out\" ]\n",
            dir = dir.display()
        ),
    )
    .expect("the ffmpeg stub");
    std::fs::set_permissions(&script, std::os::unix::fs::PermissionsExt::from_mode(0o755))
        .expect("the script runs");
    // The path Settings holds: short, no spaces, execs the script with every argument quoted.
    let ffmpeg = dir.join("ffmpeg");
    common::write_script(
        &ffmpeg,
        format!("#!/bin/sh\nexec \"{}\" \"$@\"\n", script.display()),
    )
    .expect("the ffmpeg wrapper");
    for tool in [&probe, &ffmpeg] {
        std::fs::set_permissions(tool, std::os::unix::fs::PermissionsExt::from_mode(0o755))
            .expect("the stub runs");
    }
    dir
}

/// Base64 of a payload, so a stub script can carry ffmpeg's metadata report verbatim without any shell
/// having to re-quote its quotes, tabs and newlines.
fn base64_line(text: &str) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes = text.as_bytes();
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let mut buf = [0u8; 3];
        buf[..chunk.len()].copy_from_slice(chunk);
        let bits = ((buf[0] as u32) << 16) | ((buf[1] as u32) << 8) | buf[2] as u32;
        for index in 0..4 {
            if index > chunk.len() {
                out.push('=');
                continue;
            }
            out.push(TABLE[((bits >> (18 - 6 * index)) & 0x3f) as usize] as char);
        }
    }
    out
}

/// §10's spelled values for the three numbers this pass runs on, taken from the catalogue so the rows are
/// checked after their tokens and the run is compared against §10 rather than against `frames.rs`.
fn catalogued(id: &str) -> String {
    params::find(id)
        .unwrap_or_else(|| panic!("{id} is not catalogued"))
        .spelled
}

/// §F1.6 S2 through the real door: the sweep covers the whole video at four frames a second, and
/// `scenes.tsv` holds the time and score of each change — merged inside the gap, and with nothing at all
/// for the score that merely reached the threshold.
#[test]
fn f1_6_s2_the_scene_pass_runs_over_the_whole_video_and_scenes_tsv_holds_each_change() {
    assert_eq!(
        catalogued("P.machine.sceneThreshold"),
        "1",
        "the row spells 1.0 as §10 does"
    );
    assert_eq!(
        catalogued("P.machine.sceneMinGapSeconds"),
        "0.5",
        "and the gap as §10 does"
    );
    let scene = Scene::new("20.0");
    let (drive, log) = walk(&scene);
    assert!(
        matches!(drive, run_stages::Drive::Done { .. }),
        "the frames stage finished: {drive:?} / {log:?}"
    );
    // Every tool the walk ran came from the folder the conf names — proof the door read its ffmpeg (and
    // ffprobe, which `services::ffprobe_for` takes from the same folder) from Settings rather than from
    // whatever happens to be on PATH. The log lines are the walk's command echoes, indented four.
    let box_dir = Path::new(&scene.conf().ffmpeg)
        .parent()
        .expect("a tool folder")
        .to_path_buf();
    let commands: Vec<String> = scene
        .log_lines()
        .into_iter()
        .filter(|line| line.starts_with("    "))
        .collect();
    assert!(
        commands.len() >= 2,
        "the walk logged no commands: {:?}",
        commands
    );
    assert!(
        commands
            .iter()
            .all(|line| line.contains(&box_dir.display().to_string())),
        "a tool off PATH was called: {commands:?}"
    );

    // Exactly one sweep, built by `frames::scene_pass_plan`: 4 fps, the throwaway 640-wide scale, and
    // the detector reporting EVERY score so the threshold is applied once, in the program.
    let sweeps: Vec<String> = scene
        .calls()
        .into_iter()
        .filter(|line| line.contains("scdet"))
        .collect();
    assert_eq!(
        sweeps.len(),
        1,
        "one sweep over the whole video: {sweeps:?}"
    );
    let sweep = &sweeps[0];
    assert!(sweep.contains("fps=4"), "four frames a second: {sweep}");
    assert!(
        sweep.contains("scale=640"),
        "the throwaway stream is 640 wide: {sweep}"
    );
    assert!(
        sweep.contains("scdet=threshold=0"),
        "every score is reported: {sweep}"
    );
    assert!(
        sweep.contains("-i"),
        "the sweep is asked about the video itself: {sweep}"
    );

    // `scenes.tsv`: the two changes the log holds, merged across the gap, and NOT the 2.0 s score that
    // sits exactly on `P.machine.sceneThreshold` (over it is the bound).
    let scenes = requests::read_scenes(&scene.tree, LANE).expect("readable");
    let times: Vec<f64> = scenes.iter().map(|change| change.time).collect();
    assert_eq!(
        times,
        vec![1.0, 5.0],
        "the merged changes, and no row for the start: {scenes:?}"
    );
    assert_eq!(
        scenes[0].score, 58.310,
        "the FIRST of a merged run keeps its score"
    );
    let text = std::fs::read_to_string(scene.tree.scenes_tsv(LANE)).expect("on disk");
    assert_eq!(
        text, "1.00\t58.31\n5.00\t77.4\n",
        "`time\\tscore` per row: {text:?}"
    );
    // And the merge is the parameterised one: nearer than the gap merges, further apart does not.
    assert_eq!(
        frames::scene_changes(&frames::parse_scores(&scene_log()), 1.0, 0.5).len(),
        scenes.len(),
        "the file holds exactly what the rule makes"
    );
}

/// §F1.6 S3 through the real door: the pictures on disk are the grid restarted at every change, they are
/// extracted unscaled, and the walk's log carries both the worker progress and ffmpeg's own `-progress`
/// reading — proof the door parsed it rather than ignoring it.
#[test]
fn f1_6_s3_frames_land_on_the_grid_restarted_at_every_change_and_the_pass_reports_progress() {
    assert_eq!(
        catalogued("P.eng.frameGridSeconds"),
        "0.25",
        "the grid is §10's 0.25 s"
    );
    let scene = Scene::new("20.0");
    let (drive, log) = walk(&scene);
    assert!(
        matches!(drive, run_stages::Drive::Done { .. }),
        "{drive:?} / {log:?}"
    );

    let changes: Vec<f64> = requests::read_scenes(&scene.tree, LANE)
        .expect("scenes.tsv")
        .into_iter()
        .map(|change| change.time)
        .collect();
    let grid: f64 = catalogued("P.eng.frameGridSeconds")
        .parse()
        .expect("a number");
    let wanted = frames::frame_seconds(&changes, 20.0, grid);
    let on_disk = scene.frames_on_disk();
    assert_eq!(
        on_disk.len(),
        wanted.len(),
        "one file per planned second: {} on disk, {} planned",
        on_disk.len(),
        wanted.len()
    );
    // The grid restarts at the change: the change's own second is a frame, and so is the step after it.
    let start = clock_start(&scene);
    for change in &changes {
        let name = frames::frame_file(start, *change);
        assert!(
            on_disk.contains(&name),
            "the first picture of a new scene is there: {name} not in {on_disk:?}"
        );
    }
    // Worker progress AND ffmpeg's own `-progress`: the second is only in the log if the door read it.
    assert!(
        log.iter()
            .any(|line| line.starts_with("extracting frames 1/")),
        "the chunk progress never reached the run: {log:?}"
    );
    assert!(
        log.iter()
            .any(|line| line.contains("frames extracted (ffmpeg ") && line.contains(" s)")),
        "ffmpeg's -progress was never parsed: {log:?}"
    );
    // No frame extraction scales or re-times the video: the model's copies are made in F1.7.
    for line in scene.calls().iter().filter(|line| !line.contains("scdet")) {
        assert!(
            !line.contains("scale="),
            "an extraction scaled the frame: {line}"
        );
        assert!(
            !line.contains("fps="),
            "an extraction re-timed the video: {line}"
        );
        assert!(
            line.contains("-progress"),
            "an extraction reported no progress: {line}"
        );
    }
}

/// §F1.6 S4 through the real door: every frame is named by its exact second on the FILE's clock — the
/// stamped instant the recording's own name claims, not the session's zero — and the name reads back.
#[test]
fn f1_6_s4_every_frame_is_named_by_its_second_on_the_files_clock() {
    let scene = Scene::new("20.0");
    let (drive, log) = walk(&scene);
    assert!(
        matches!(drive, run_stages::Drive::Done { .. }),
        "{drive:?} / {log:?}"
    );
    // The recording's own instant, the one its name claims: the first rewrite named frames by the session
    // offset instead (0 for the only source), so every name read `1970-01-01_…`.
    let start = clock::name_stamp(&recording())
        .expect("the row's name has to carry a stamp for this to mean anything");
    assert!(
        start >= clock::SESSION_NAMED_BEFORE,
        "a real instant, not a session second: {start}"
    );
    assert_eq!(
        STAMP.split(' ').next().unwrap(),
        "2026-05-01",
        "the date half first, §8's rule"
    );

    let changes: Vec<f64> = requests::read_scenes(&scene.tree, LANE)
        .expect("scenes.tsv")
        .into_iter()
        .map(|change| change.time)
        .collect();
    let grid: f64 = catalogued("P.eng.frameGridSeconds")
        .parse()
        .expect("a number");
    let wanted = frames::frame_seconds(&changes, 20.0, grid);
    let on_disk = scene.frames_on_disk();
    for at in [wanted[0], *wanted.last().expect("more than one frame")] {
        let name = frames::frame_file(start, at);
        assert!(
            on_disk.contains(&name),
            "{name} ({at} s) is not among {on_disk:?}"
        );
        assert_eq!(
            name.len(),
            "YYYY-MM-DD_HH-MM-SS.mmm.jpg".len(),
            "{name} is not the shape"
        );
        // The name reads back as the instant it claims, which is what keeps a later frame from renaming
        // the ones already on disk.
        let read_back = clock::read_frame(name.trim_end_matches(".jpg")).expect("a readable stamp");
        assert!(
            (read_back - (start + at)).abs() < 0.001,
            "{name} reads back as {read_back}"
        );
    }
    // The first frame is the recording's own start, down to the millisecond field.
    assert_eq!(
        on_disk[0],
        format!("{}.jpg", clock::frame_name(start)),
        "the first picture: {on_disk:?}"
    );
    assert_eq!(
        on_disk[0], "2026-05-01_10-00-00.000.jpg",
        "its real date and time"
    );
}

/// §F1.6 S5 and S1 through the real door: the marker is written last and says what the pass did, and a
/// second walk over the same session skips — visibly, and without asking ffmpeg again.
#[test]
fn f1_6_s5_the_marker_is_written_last_and_a_second_walk_skips_the_pass() {
    let scene = Scene::new("20.0");
    let (drive, log) = walk(&scene);
    assert!(
        matches!(drive, run_stages::Drive::Done { .. }),
        "{drive:?} / {log:?}"
    );
    let on_disk = scene.frames_on_disk();
    let changes = requests::read_scenes(&scene.tree, LANE)
        .expect("scenes.tsv")
        .len();
    assert_eq!(
        said(&log).last().copied().unwrap_or_default(),
        frames::done_log(LANE, on_disk.len(), changes).as_str(),
        "the pass said what it made, last: {:?}",
        said(&log)
    );
    let marker = requests::read_frames_marker(&scene.tree, LANE).expect("readable");
    let grid: f64 = catalogued("P.eng.frameGridSeconds").parse().unwrap();
    let threshold: f64 = catalogued("P.machine.sceneThreshold").parse().unwrap();
    assert_eq!(
        marker,
        Some((grid, threshold)),
        "the marker holds `<grid>|<threshold>`"
    );
    assert!(
        scene.tree.frames_marker(LANE).is_file(),
        "and the file is there"
    );

    // Second walk: S1's skip. The skip asks NOTHING of ffmpeg — not even the sweep, because the marker
    // already says these scenes are these scenes — so `argv.log` must not gain one line.
    let before = scene.calls().len();
    let (_drive, log) = walk(&scene);
    assert!(
        log.iter()
            .any(|line| line.contains("frames already extracted") && line.contains("skipping")),
        "the skip was never said: {log:?}"
    );
    assert_eq!(
        log.iter()
            .filter(|line| line.contains("frames already extracted"))
            .count(),
        1,
        "exactly one skip line: {log:?}"
    );
    assert_eq!(
        log.iter()
            .find(|line| line.contains("frames already extracted"))
            .map(String::as_str),
        Some(frames::already_log(LANE, grid, threshold).as_str()),
        "the skip names the grid and threshold it matched"
    );
    assert_eq!(
        scene.calls().len(),
        before,
        "nothing was re-extracted: {:?}",
        &scene.calls()[before.min(scene.calls().len())..]
    );
}

/// §F1.6 S3's short branch through the real door: a take shorter than one grid step has one picture,
/// its first, and nothing else — no grid, no extra seeks.
/// A take shorter than one grid step still gets its sweep: the scores are what `scenes.tsv` records, and
/// a 0.1 s clip answers two of them at 4 fps — one change over `P.machine.sceneThreshold`, which is why
/// the assertion below reads the count out of the file rather than assuming zero.
#[test]
fn f1_6_s2_a_short_take_gets_its_first_frame_alone() {
    let scene = Scene::new("0.1");
    let (drive, log) = walk(&scene);
    assert!(
        matches!(drive, run_stages::Drive::Done { .. }),
        "{drive:?} / {:?}",
        said(&log)
    );
    let on_disk = scene.frames_on_disk();
    assert_eq!(
        on_disk,
        vec![frames::frame_file(clock_start(&scene), 0.0)],
        "one frame, the first: {on_disk:?}"
    );
    // One chunk's worth of work for one frame: no grid means nothing to split.
    assert_eq!(
        said(&log)
            .iter()
            .filter(|line| line.starts_with("extracting frames"))
            .count(),
        1,
        "one worker for one frame: {:?}",
        said(&log)
    );
    let changes = requests::read_scenes(&scene.tree, LANE)
        .expect("scenes.tsv")
        .len();
    assert_eq!(
        said(&log).last().copied().unwrap_or_default(),
        frames::done_log(LANE, 1, changes).as_str(),
        "it said one frame and the changes it found: {:?}",
        said(&log)
    );
}

/// A row whose video is gone fails the stage with the file named, through the walk ▶ runs — §Failure-is-
/// specific would rather say which file is missing than blame the tool that never got to run.
#[test]
fn f1_6_s2_a_missing_video_names_itself_and_fails_the_stage() {
    let scene = Scene::missing_video();
    let (drive, log) = walk(&scene);
    match drive {
        run_stages::Drive::Failed { stage, error } => {
            assert_eq!(stage, "frames", "the frames stage failed: {stage}");
            assert!(
                error.contains("no video to frame"),
                "it named the reason: {error}"
            );
            assert!(
                error.contains(&format!("{LANE}:")),
                "it named the row: {error}"
            );
            assert!(
                error.contains(&recording()),
                "it named the missing file: {error}"
            );
        }
        other => panic!("a missing video must fail the stage, not {other:?} / {log:?}"),
    }
    // Nothing was written for a row that could not be framed.
    assert!(
        !scene.tree.frames_marker(LANE).exists(),
        "no marker without frames"
    );
    assert!(scene.frames_on_disk().is_empty(), "no pictures either");
}

/// What the scene's conf says — the door re-reads it per request (§1), so a test that rewrites it changes
/// the next walk. Read through the SCENE so every scene's own settings folder is what is read.
impl Scene {
    pub fn conf(&self) -> Conf {
        conf_of(&self.paths)
    }
}

/// What a conf says — the door re-reads it per request (§1), so a test that rewrites it changes the next
/// walk.
pub fn conf_of(paths: &Paths) -> Conf {
    settings::read(paths).unwrap_or_default()
}

/// §F1.6 S4 for a folder the first rewrite extracted: its frames carry session seconds printed as 1970 dates.
/// The resumed pass renames them in place to the recording's own date and time, the scaled copies with them,
/// without asking ffmpeg again, and says how many it renamed.
#[test]
fn f1_6_s4_a_1970_named_folder_is_renamed_to_the_recordings_date_on_resume() {
    let scene = Scene::new("20.0");
    let (drive, _) = walk(&scene);
    assert!(matches!(drive, run_stages::Drive::Done { .. }), "{drive:?}");
    let real = scene.frames_on_disk();
    // Put the folder back the way the first rewrite named it: session seconds (0 for the only source).
    let start = clock_start(&scene);
    let dir = scene.tree.frames_dir(LANE);
    let llm = scene.tree.llm_frames_dir(LANE);
    std::fs::create_dir_all(&llm).unwrap();
    for name in &real {
        let at = clock::read_frame(name.trim_end_matches(".jpg")).unwrap() - start;
        let old = format!("{}.jpg", clock::frame_name(at));
        assert!(old.starts_with("1970-01-01_"), "{old}");
        std::fs::rename(dir.join(name), dir.join(&old)).unwrap();
        std::fs::write(llm.join(&old), b"scaled").unwrap();
    }
    let calls_before = scene.calls().len();

    let (drive, log) = walk(&scene);
    assert!(matches!(drive, run_stages::Drive::Done { .. }), "{drive:?}");
    assert_eq!(
        scene.frames_on_disk(),
        real,
        "every frame has its real name back"
    );
    let mut copies: Vec<String> = std::fs::read_dir(&llm)
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    copies.sort();
    assert_eq!(copies, real, "the scaled copies carry the same names");
    assert_eq!(
        scene.calls().len(),
        calls_before,
        "renamed, not extracted again"
    );
    assert!(
        log.iter()
            .any(|line| line.contains(&format!("{} frame(s) renamed", real.len() * 2))),
        "the rename is said: {log:?}"
    );
}
