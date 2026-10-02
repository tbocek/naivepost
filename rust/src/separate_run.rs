//! Voice separation on the run path: the walk's row that asks audio.cpp to split a recording.
//!
//! [`crate::separate`] holds the split's rules and takes a `join` closure "the real ▶ hands over";
//! §02-services.md §1 lists voice separation as one of the five things audio.cpp serves, and until
//! this module nothing did hand it over — `separate::separate` had no caller outside its tests. So
//! this is the ▶ half, on the same two pieces the speech row
//! already uses: [`stage`] turns the rows that ticked ✂ into planned tasks, and [`run_task`] is what
//! [`crate::run_stages`] calls when it walks one, which does the real upload, the real
//! `POST /v1/tasks/run`, and the real download of the two stems that come back.
//!
//! Nothing is answered here: every byte crosses through [`crate::audio_task`] and
//! [`crate::server_leg`], so pointing the Settings box at another address moves this work exactly as it
//! moves the transcription's.

use std::io::Read;
use std::path::{Path, PathBuf};

use crate::bodies::ServerPath;
use crate::layout::Tree;
use crate::probes;
use crate::project::Project;
use crate::requests::{self, Outcome as RowOutcome};
use crate::run_stages::{self, Stage, Task};
use crate::separate::{self, Outcome};
use crate::server_leg;
use crate::services::{self, Server};
use crate::settings::Paths;

/// What [`crate::run_stages`] reads in a task's `args[0]` to tell a separation row from the other
/// audio rows: same dial, different request, so the walk keeps one branch and the row says what it is.
pub const SEPARATE_MARK: &str = "separate";

/// The separation stage: one row per recording whose ✂ is ticked and still has a voice to split off.
///
/// It rides [`crate::runqueue::TRACK_STT`] because it is the same recording's audio work, and it is a
/// dial (`program: AUDIO_TASK`), not a spawn. A row whose scissors are dead is left out here for the
/// same reason the ✂ is greyed on the page: asking would produce a voice split off a voice.
pub fn stage(project: &Project) -> Stage {
    let tasks = project
        .sources
        .iter()
        .enumerate()
        .filter(|(_, source)| source.sepvoice && crate::sources::separable(source))
        .map(|(index, source)| Task {
            // The lane is the same word the recording's own rows use (§F1.3's stem rule), so one
            // recording reads as one thing in the run log however many stages touch it.
            label: format!("separating {}", crate::audio_task::source_lane(&source.path)),
            program: run_stages::AUDIO_TASK.to_string(),
            args: vec![
                SEPARATE_MARK.to_string(),
                source.path.clone(),
                index.to_string(),
            ],
            already_done: false,
            // The window's own project folder, handed over by [`run_stages::plan`] through
            // [`run_stages::Task::session`]; a split that wrote into the launcher's folder would split
            // the wrong session.
            session: None,
        })
        .collect();
    Stage {
        track: crate::runqueue::TRACK_STT,
        name: "separation",
        phase: 1,
        of: 2,
        kind: "recording",
        tasks,
    }
}

/// The row [`run_stages::drive_walk`] hands here: split one recording and say what came out.
///
/// The session's own `naivepost.json` is what is split — the walk's `project` argument belongs to the
/// page that was pressed, and the row index refers to the file the split will rewrite. F1.2 says the
/// "project saved" belongs to the caller of `separate`, so the two rows this turns into are saved here
/// once, after both halves are on disk, and never half-way.
pub fn run_task(task: &Task, log: &mut impl FnMut(&str)) -> Result<(), String> {
    // The machine's settings folder, always from the environment. The session is the working folder's
    // own: `startup::session_dir(cwd)`, which is where a run's session sits for the app as it is walked
    // today (the transcription dial resolves it the same way).
    let paths = crate::settings::from_environment()
        .ok_or("nowhere to keep the settings -- neither XDG_CONFIG_HOME nor HOME is set")?;
    let session = crate::startup::session_dir(&std::env::current_dir().unwrap_or_default());
    let tree = Tree::new(&session)
        .map_err(|_| format!("{}: not a project folder to split a recording in", session.display()))?;
    let mut project = crate::project::load(tree.dir())?;
    let index = task
        .args
        .get(2)
        .and_then(|raw| raw.parse::<usize>().ok())
        .ok_or("the split row names no recording")?;
    let source = project
        .sources
        .get(index)
        .ok_or("the split row names a recording that is not there")?;
    let lane = crate::audio_task::source_lane(&source.path);
    let decoded = decoded_audio(&tree, &lane);
    let conf = crate::settings::read(&paths).unwrap_or_default();
    let model = services::sep_model(&conf).to_string();
    let ffmpeg = conf.ffmpeg.clone();
    let outcome = split(
        &mut project,
        &SplitWhere {
            tree: &tree,
            paths: &paths,
            model: &model,
            configured_ffmpeg: &ffmpeg,
        },
        index,
        &decoded,
        log,
    )?;
    for line in &outcome.logs {
        log(line);
    }
    // S7 landed: the row became two, and that is only true once the file says so.
    crate::project::save(&project, tree.dir())
}

/// The join [`crate::separate`] runs for one half: put this recording on the server once, then ask the
/// stems task for that upload's TWO halves in one request and write each one down.
///
/// This is why the run path writes its own join rather than asking per half: a flow that cuts one
/// chunk at a time can name the half it wants as the audio to split, whereas here the whole decoded
/// recording went up once and its upload's answer is the only
/// thing the server can name (§1's upload rule). The reply shape, the model choice and the dial are
/// still the ones that door owns — [`crate::audio_task::stem_paths`] and [`crate::audio_task::dial`] —
/// so there is one missing-model sentence and one `requests.tsv` writer, not two.
fn join_halves<'a>(
    tree: &'a Tree,
    paths: &'a Paths,
    model: &'a str,
    upload: ServerPath,
    voice_path: &'a Path,
    rest_path: &'a Path,
) -> impl FnMut(&Path) -> Result<(), String> + 'a {
    let held = Some(tree.clone());
    let mut asked = false;
    move |out: &Path| {
        if asked {
            // Second half: both stems were fetched with the one request below.
            return Ok(());
        }
        asked = true;
        let chunk = out
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let body = crate::audio_task::dial(
            &held,
            paths,
            crate::audio_task::Ask { step: "Separate", job: &chunk, task: "separate" },
            model,
            &crate::audio_task::separation_body(model, &upload, &chunk),
        )?;
        let parsed: serde_json::Value =
            serde_json::from_str(&body).map_err(|why| format!("the stems answer is not JSON: {why}"))?;
        let (_kind, voice, rest) = crate::audio_task::stem_paths(&parsed)?;
        fetch_stem(tree, paths, &voice, voice_path)?;
        fetch_stem(tree, paths, &rest, rest_path)
    }
}

/// Ask for the split, through the doors, and hand back what [`crate::separate`] decided.
/// The split's fixed surroundings, so [`split`] takes what it works ON rather than eight arguments.
struct SplitWhere<'a> {
    tree: &'a Tree,
    paths: &'a Paths,
    model: &'a str,
    configured_ffmpeg: &'a str,
}

fn split(
    project: &mut Project,
    where_: &SplitWhere<'_>,
    index: usize,
    decoded: &Option<PathBuf>,
    log: &mut impl FnMut(&str),
) -> Result<Outcome, String> {
    let tree = where_.tree;
    let paths = where_.paths;
    let model = where_.model;
    let configured_ffmpeg = where_.configured_ffmpeg;
    let lane = crate::audio_task::source_lane(&project.sources[index].path);
    let decoded = decoded
        .as_ref()
        .filter(|path| path.exists())
        .ok_or_else(|| format!("{}: no decoded audio to split", lane))?;
    // `subprocess::run` logs the command line itself (§8's subprocess rule), so this leg has nothing
    // extra to print; the `log` sink stays with the caller for its own lines.
    let seconds = probe_duration(configured_ffmpeg, decoded, log, &|program, args| {
        crate::subprocess::run(program, args, |_| {})
    })?;
    // The whole decoded recording goes up once: a stems task answers with the two halves of exactly
    // the audio it was given, and §1 puts no client timeout on an audio task, so the split of a long
    // take is one long request rather than a re-cut per chunk (`separate::chunk_plan` sizes that
    // ceiling for the flow that needs it; here the decoded file is the chunk).
    let bytes = std::fs::read(decoded)
        .map_err(|why| format!("{}: {why}", decoded.display()))?;
    let upload = crate::audio_task::upload_bytes(&Some(tree.clone()), paths, &lane, &bytes)?;
    // The halves `separate` is about to write; the join fills both from the one stems reply, and
    // `separate` makes the folder before it asks for either.
    let stem = crate::audio_task::source_lane(&project.sources[index].path);
    let container = if crate::add_sources::is_video(Path::new(&project.sources[index].path)) {
        "mkv"
    } else {
        "wav"
    };
    let voice_path = tree.split_voice(&stem);
    let rest_path = tree.split_novoice(&stem, container);
    let joined = join_halves(tree, paths, model, upload, &voice_path, &rest_path);
    crate::separate::separate(
        project,
        tree,
        tree.dir(),
        index,
        seconds,
        &[],
        model,
        joined,
        |path| loudness(configured_ffmpeg, path, log),
    )
}

/// Bring one stem down off the server and write it where the split wants it.
///
/// audio.cpp answers a stems task with a path inside its own container (§1: every task works in the
/// server's folders). How those bytes come back is not in §1's table, so this is a decision written
/// down: the server answers a plain GET of the path it just named (the same address an upload was
/// posted to), which is the one read that needs no invented endpoint. A refusal or a miss fails the
/// row — a half that was never downloaded is not a half.
fn fetch_stem(
    tree: &Tree,
    paths: &Paths,
    stem: &ServerPath,
    out: &Path,
) -> Result<(), String> {
    let endpoint = services::endpoint_for(paths, Server::Audio, |name| std::env::var(name).ok());
    let url = format!("{}{}", endpoint.url.trim_end_matches('/'), stem.as_str());
    // Same shape as `server_leg::send_pinged`: config first, then the headers on the request itself,
    // and no overall timeout because this is an audio read (§1).
    // Same shape as `server_leg::send_pinged`: the config is BUILT first, and the headers go on the
    // request that comes out of it (`config()` hands back a builder that has no `.header()`).
    // `http_status_as_error(false)` so a 404 arrives as a status we read and report, not as a lost body.
    let mut request = ureq::get(&url)
        .force_send_body()
        .config()
        .timeout_connect(Some(server_leg::CONNECT_TIMEOUT))
        .http_status_as_error(false)
        .build();
    if let Some(bearer) = services::authorization(&endpoint.key) {
        request = request.header("Authorization", bearer);
    }
    let started = std::time::Instant::now();
    let row = server_leg::base_row(Server::Audio, "", "stem", "", "Prepare", stem.as_str());
    let (status, bytes) = match request.send_empty() {
        Ok(response) => {
            let status = response.status().as_u16();
            let mut reader = response.into_body().into_reader();
            let mut buf: Vec<u8> = Vec::new();
            let mut chunk = [0u8; 65536];
            loop {
                match reader.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(read) => buf.extend_from_slice(&chunk[..read]),
                    Err(why) => {
                        let mut row = row.clone();
                        row.on_wire_s = Some(started.elapsed().as_millis() as f64 / 1000.0);
                        row.outcome = RowOutcome::Error(format!("{url}: {why}"));
                        let _ = requests::record(tree, &row);
                        return Err(format!("{url}: {why}"));
                    }
                }
            }
            (status, buf)
        }
        Err(why) => {
            let mut row = row.clone();
            row.on_wire_s = Some(started.elapsed().as_millis() as f64 / 1000.0);
            row.outcome = RowOutcome::Error(format!("{url}: {why}"));
            let _ = requests::record(tree, &row);
            return Err(format!("{url}: {why}"));
        }
    };
    record_stem_row(tree, &row, &url, bytes.len(), started, Ok(status));
    if !(200..300).contains(&status) {
        let flat = String::from_utf8_lossy(&bytes).trim().replace('\n', " ");
        return Err(format!(
            "{url} answered the stem {} with a {status}: {flat}",
            stem.as_str()
        ));
    }
    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir).map_err(|why| format!("{}: {why}", dir.display()))?;
    }
    std::fs::write(out, &bytes).map_err(|why| format!("{}: {why}", out.display()))?;
    Ok(())
}

/// The timed row for one stem read (§09 §10: every request, appended the moment it ends, whatever its
/// outcome). A GET sends no bytes, so only `received` is filled; the `job` column already names the
/// stem the base row was built with.
fn record_stem_row(
    tree: &Tree,
    base: &crate::requests::Request,
    url: &str,
    received: usize,
    started: std::time::Instant,
    outcome: Result<u16, String>,
) {
    let wire = started.elapsed().as_millis() as f64 / 1000.0;
    let mut row = base.clone();
    row.received_bytes = Some(received as i64);
    row.on_wire_s = Some(wire);
    row.outcome = match outcome {
        Ok(status) if (200..300).contains(&status) => RowOutcome::Ok,
        Ok(status) => RowOutcome::Error(format!("{status}")),
        Err(why) => RowOutcome::Error(format!("{url}: {why}")),
    };
    let _ = requests::record(tree, &row);
}

/// The decoded audio a split reads: §F1.3 S1's 16 kHz mono pcm when it is there. Its absence is the
/// honest reason a row gets nothing, in the same voice as the transcription row's.
fn decoded_audio(tree: &Tree, lane: &str) -> Option<PathBuf> {
    let wav = tree.voice16k_wav(lane);
    wav.exists().then_some(wav)
}

/// Seconds of an audio file, by ffprobe — the tool §1 names for reading, from the same folder as the
/// configured ffmpeg ([`services::ffprobe_for`]'s rule).
///
/// §8's duration-probing rule lives here: a recorder killed mid-write leaves a container whose header
/// carries no length (it is written when the file is closed), so a header that answers nothing pays for
/// one decode pass — ffprobe over the stream entries reads the whole stream and counts it. The answer
/// is remembered per path + size + mtime (`probes::Cache`): a re-recorded or truncated file misses,
/// and the same file never pays for the decode twice in one run.
///
/// `run` is the leg that starts ffprobe, supplied by the caller for the same reason `source_probe`
/// takes its own: the rule (header, else decode, cached) must be testable without a real ffprobe.
/// The leg that starts one ffprobe: (program, args) -> stdout. Named because the type is worth
/// reading once (`source_probe::run_ffprobe` matches it), and so a test can hand in an answer.
pub type RunTool = dyn Fn(&str, &[&str]) -> Result<String, String>;

pub fn probe_duration(
    configured_ffmpeg: &str,
    path: &Path,
    log: &mut impl FnMut(&str),
    run: &RunTool,
) -> Result<f64, String> {
    // The command line is logged by `run`'s own leg (`subprocess::run` prints it before starting,
    // §8's subprocess rule); this rule adds no line of its own while the probe runs.
    let _ = log;
    let shown = path.display().to_string();
    // The key needs real metadata; a file we cannot stat gets measured uncached rather than skipped.
    let stamp = std::fs::metadata(path).ok().map(|meta| {
        let mtime = meta
            .modified()
            .ok()
            .and_then(|when| when.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|since| since.as_secs() as i64)
            .unwrap_or(0);
        probes::key(&shown, meta.len() as i64, mtime)
    });
    if let Some(found) = stamp.as_ref().and_then(|key| durations().lookup(key)) {
        return Ok(found);
    }
    let ffprobe = services::ffprobe_for(configured_ffmpeg, "ffprobe");
    let header = run(
        &ffprobe,
        &["-show_entries", "format=duration", "-of", "default=nw=1:nk=1", &shown],
    )
    .ok()
    .and_then(|answer| answer.trim().parse::<f64>().ok())
    .filter(|secs| *secs > 0.0);
    // Only a header that said nothing pays for the read of every frame.
    let decoded = match header {
        Some(_) => None,
        None => run(
            &ffprobe,
            &[
                "-select_streams",
                "a",
                "-show_entries",
                "stream=duration",
                "-of",
                "default=nw=1:nk=1",
                &shown,
            ],
        )
        .ok()
        .and_then(|answer| answer.trim().lines().next().and_then(|line| line.trim().parse::<f64>().ok()))
        .filter(|secs| *secs > 0.0),
    };
    let (secs, how) = probes::duration(&shown, header, decoded)?;
    if let Some(key) = stamp {
        durations().store(key, secs);
    }
    // One line when a decode was paid for: "why did this take so long" has an answer in the log then,
    // and nothing is printed when the header answered as it always does.
    if matches!(how, probes::Source::Decoded) {
        log(&format!(">>> {shown} has no header duration; measured by decoding"));
    }
    Ok(secs)
}

/// This run's measured durations (§8: "cached per path+size+mtime like every probe"). Process-wide
/// because a split runs on whichever thread the queue put it on, and a second measurement of the same
/// bytes would cost a second decode there.
fn durations() -> std::sync::MutexGuard<'static, probes::Cache> {
    static CACHE: std::sync::OnceLock<std::sync::Mutex<probes::Cache>> = std::sync::OnceLock::new();
    match CACHE.get_or_init(|| std::sync::Mutex::new(probes::Cache::new())).lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// ffmpeg's `volumedetect` mean of a file, as [`crate::separate`] wants it: the number as ffmpeg
/// printed it, or the sentence that says it could not be measured (S6 treats an unmeasurable level as
/// nothing to say, never as silence).
fn loudness(configured_ffmpeg: &str, path: &Path, log: &mut impl FnMut(&str)) -> String {
    let ffmpeg = if configured_ffmpeg.trim().is_empty() {
        run_stages::FFMPEG.to_string()
    } else {
        configured_ffmpeg.trim().to_string()
    };
    let shown = [
        "-i",
        &path.display().to_string(),
        "-af",
        "volumedetect",
        "-f",
        "null",
        "-",
    ];
    match crate::subprocess::run(&ffmpeg, &shown, |line| log(line)) {
        Ok(out) => separate::mean_db(&out).map(|db| format!("{db} dB")).unwrap_or_default(),
        Err(_) => String::new(),
    }
}
