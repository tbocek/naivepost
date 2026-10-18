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
use crate::separate_media as media;
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
    let conf = crate::settings::read(&paths).unwrap_or_default();
    let outcome = split(
        &mut project,
        &tree,
        &SplitWhere {
            paths: &paths,
            model: services::sep_model(&conf),
            configured_ffmpeg: &conf.ffmpeg.clone(),
        },
        index,
        log,
    )?;
    for line in &outcome.logs {
        log(line);
    }
    // S7 landed: the row became two, and that is only true once the file says so.
    crate::project::save(&project, tree.dir())
}

/// Ask for the split, through the doors, and hand back what [`crate::separate`] decided.
/// The split's fixed surroundings, so [`split`] takes what it works ON rather than eight arguments.
struct SplitWhere<'a> {
    paths: &'a Paths,
    model: &'a str,
    configured_ffmpeg: &'a str,
}

/// S1-S7 for one row, sequenced. Flat and guard-clause first, because six legs fail for six different
/// reasons and each error has to say which one it was: decode (S1), measure the quiet stretches (S2),
/// cut at them and ask the model per piece (S3), join each half and mux the rest with the picture
/// (S4/S5), report the loudness of both (S6) and turn the row into two (S7). The ffmpeg legs are
/// [`crate::separate_media`], the wire legs [`crate::audio_task`], the decisions [`crate::separate`];
/// this function only sequences them.
fn split(
    project: &mut Project,
    tree: &Tree,
    where_: &SplitWhere<'_>,
    index: usize,
    log: &mut impl FnMut(&str),
) -> Result<Outcome, String> {
    let paths = where_.paths;
    let model = where_.model;
    let ffmpeg = media::program(where_.configured_ffmpeg);
    project
        .sources
        .get(index)
        .ok_or_else(|| "no such row to split".to_string())?;
    let source_path = project.sources[index].path.clone();
    let lane = crate::audio_task::source_lane(&source_path);
    let source = crate::layout::resolve(tree.dir(), tree.dir(), &source_path);
    if !source.exists() {
        return Err(format!(
            "{}: the recording the split row names is not there",
            source.display()
        ));
    }
    let video = crate::add_sources::is_video(Path::new(&source_path));
    // S4/S5's products, and the scratch beside them (§inventory/prepare.md B.1: `stems/<base>.work`).
    // Cleared on ENTRY: a run killed between the decode and the join leaves parts behind, and parts
    // from an earlier take would be joined into this one's file.
    let voice_path = tree.split_voice(&lane);
    let rest_path = tree.split_novoice(&lane, if video { "mkv" } else { "wav" });
    let work = media::work_dir(tree, &lane);
    // Both halves already on disk is not a re-run, it is a second split: without this check ▶ would cut
    // a voice off a voice and write `<base>.split-voice.split-voice.wav`. Asked BEFORE any ffmpeg runs,
    // because the answer costs nothing and the decode does not.
    if let Some(done) = separate::already_split(project, tree, tree.dir(), index) {
        return Ok(done);
    }
    media::clear_work(&work)?;
    // S1 once, then every piece is cut from THAT decode rather than re-decoded from the source.
    let decoded = media::decode(&ffmpeg, &source, &media::decoded_file(&work, &lane), log)?;
    let seconds = probe_duration(where_.configured_ffmpeg, &decoded, log, &|program, args| {
        crate::subprocess::run(program, args, |_| {})
    })?;
    // S2: where the recording is quiet long enough to cut in, at the same threshold the ASR cuts on.
    let silences = media::silences(&ffmpeg, &decoded)?;
    let plan = separate::chunk_plan(seconds, &silences);
    let logs = split_parts(
        &ffmpeg, tree, paths, model, &lane, &source, &decoded, &plan, &work, &voice_path, &rest_path,
        log,
    )?;
    // S6 wants the mix as it was recorded, which is the decode itself: same tool, same filter, so the
    // three numbers in the line are comparable.
    let mix = loudness(where_.configured_ffmpeg, &decoded, log);
    let outcome = separate::finish(
        project,
        tree,
        tree.dir(),
        index,
        &logs,
        &mix,
        |path| loudness(where_.configured_ffmpeg, path, log),
    );
    // Success-only cleanup: after a failure the parts stay, because they are what a person can look at
    // and what the resume rule reads.
    let _ = std::fs::remove_dir_all(&work);
    Ok(outcome)
}

/// S3/S4: every chunk up to the model, every stem down, both halves assembled in place.
///
/// One request per chunk because that is what the ceiling is for ([`separate::CHUNK_MAX_SECONDS`]): a
/// stems task answers for the audio it was given, so a long take has to be cut, asked piece by piece,
/// and joined back. Chunks go up in order and their replies are kept in order, which is the only thing
/// that makes the join produce the recording rather than a shuffle of it. Returns the lines the caller
/// logs; the halves are on disk by then.
#[allow(clippy::too_many_arguments)]
fn split_parts(
    ffmpeg: &str,
    tree: &Tree,
    paths: &Paths,
    model: &str,
    lane: &str,
    source: &Path,
    decoded: &Path,
    plan: &[(f64, f64)],
    work: &Path,
    voice_path: &Path,
    rest_path: &Path,
    log: &mut impl FnMut(&str),
) -> Result<Vec<String>, String> {
    // Said before the first chunk goes up: minutes of GPU with nothing on screen looks exactly like a
    // hang, so the wait gets its number in front of it.
    let logs = vec![separate::start_log(lane, total(plan), plan.len(), model)];
    let held = Some(tree.clone());
    let mut voices: Vec<Half> = Vec::new();
    let mut rests: Vec<Half> = Vec::new();
    for (number, (from, to)) in plan.iter().enumerate() {
        let piece = work.join(media::chunk_file(lane, number));
        media::cut(ffmpeg, decoded, *from, *to, &piece, log)?;
        let name = piece.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let bytes = std::fs::read(&piece).map_err(|why| format!("{}: {why}", piece.display()))?;
        let upload = crate::audio_task::upload_bytes(&held, paths, &name, &bytes)?;
        let reply = crate::audio_task::dial(
            &held,
            paths,
            crate::audio_task::Ask { step: "Separate", job: &name, task: "separate" },
            model,
            &crate::audio_task::separation_body(model, &upload, &name),
        )?;
        let parsed: serde_json::Value =
            serde_json::from_str(&reply).map_err(|why| format!("the stems answer is not JSON: {why}"))?;
        let stems = crate::audio_task::chunk_stems(&parsed)?;
        // Which stem holds the speech is decided by the rule that owns the question, not by position:
        // a RoFormer answers `vocals`+`instrumental`, an HTDemucs four instruments.
        let ids = stem_names(&parsed, &stems);
        separate::pick_stems(&ids)?;
        voices.push(match stems.voice {
            Some(which) => Half::One(stems.voices[which].clone()),
            // A silent chunk has no voice file, but it still occupies its place in the join or the
            // voice half comes back shifted by however many gaps the recording has.
            None => Half::Parts(vec![]),
        });
        rests.push(match stems.rests.len() {
            0 => Half::Parts(vec![]),
            1 => Half::One(stems.rests[0].clone()),
            _ => Half::Parts(stems.rests.clone()),
        });
    }
    if voices.iter().all(Half::missing) {
        return Err(format!(
            "{lane}: the separation model sent no voice for any of the {} part(s) -- nothing to write",
            plan.len()
        ));
    }
    fetch_halves(ffmpeg, tree, paths, source, &voices, &rests, work, voice_path, rest_path, log)?;
    Ok(logs)
}

/// What one half's pieces came back as: one file already joined by the model, or the list of files that
/// still have to be joined in order (an HTDemucs rest is three stems per chunk).
enum Half {
    One(ServerPath),
    Parts(Vec<ServerPath>),
}

impl Half {
    fn missing(&self) -> bool {
        match self {
            Half::One(_) => false,
            Half::Parts(parts) => parts.is_empty(),
        }
    }
}

/// The whole recording's length, as the plan's last edge: the plan covers the file exactly, so its end
/// IS the duration, and reading it from the plan keeps the log line and the cuts describing one span.
fn total(plan: &[(f64, f64)]) -> f64 {
    plan.last().map(|(_, to)| *to).unwrap_or(0.0)
}

/// The ids [`separate::pick_stems`] reads, in the reply's own order: the names the server used, which
/// is what makes that rule's `vocals`/`instrumental` test meaningful rather than decorative.
fn stem_names(parsed: &serde_json::Value, stems: &crate::audio_task::ChunkStems) -> Vec<String> {
    let reply = parsed.get("result").cloned().unwrap_or_else(|| parsed.clone());
    let mut names: Vec<String> = Vec::new();
    if let Some(list) = reply.get("stems").and_then(serde_json::Value::as_array) {
        names = list
            .iter()
            .filter_map(|entry| {
                ["name", "stem", "track"]
                    .iter()
                    .filter_map(|key| entry.get(*key).and_then(serde_json::Value::as_str))
                    .next()
            })
            .map(str::to_string)
            .collect();
    } else if let Some(map) = reply.as_object() {
        names = map.keys().cloned().collect();
    }
    // A reply that named nothing still has to satisfy the rule, and the shape it arrived in says which
    // two roles it meant.
    if names.is_empty() {
        if stems.voice.is_some() {
            names.push("vocals".to_string());
        }
        if !stems.rests.is_empty() {
            names.push("instrumental".to_string());
        }
    }
    names
}

/// S4: bring every stem down and assemble both halves. Each downloaded stem is written into the work
/// dir under a name that sorts in join order, so assembling a half is one concat of a sorted listing
/// and nothing is copied twice. Several stems for one chunk's rest are mixed PER CHUNK first, because
/// mixing everything at once would sum across chunks as well as across stems.
#[allow(clippy::too_many_arguments)]
fn fetch_halves(
    ffmpeg: &str,
    tree: &Tree,
    paths: &Paths,
    source: &Path,
    voices: &[Half],
    rests: &[Half],
    work: &Path,
    voice_path: &Path,
    rest_path: &Path,
    log: &mut impl FnMut(&str),
) -> Result<(), String> {
    assemble(ffmpeg, tree, paths, voices, &work.join("v"), work, log)?;
    assemble(ffmpeg, tree, paths, rests, &work.join("r"), work, log)?;
    // The voice half is always pcm-in-a-wav the model wrote, so it joins straight out; the REST of a
    // VIDEO goes through the mux below, because its picture comes from the source, not from the model.
    media::join_parts(ffmpeg, &sorted_parts(&work.join("v")), voice_path, work, log)?;
    let rest_parts = sorted_parts(&work.join("r"));
    if rest_parts.is_empty() {
        // An all-voice recording legitimately has no rest, and an empty file is a worse answer than a
        // silent one: write nothing and let the loudness pass say what it finds.
        return Ok(());
    }
    if rest_path.extension().map(|ext| ext == "mkv").unwrap_or(false) {
        // S4/S5: the video's own picture, copied, with the joined rest as FLAC sound (lossless, because
        // Cut splices on it later). `mux_video` names the source it takes the picture from.
        return media::mux_video(ffmpeg, source, &rest_joined(ffmpeg, work, &rest_parts, log)?, rest_path, log);
    }
    media::join_parts(ffmpeg, &rest_parts, rest_path, work, log)
}

/// The rest's pieces joined into one audio file in the work dir, ready to be muxed with the picture.
/// Skipped when there is only one piece, which is the usual case (one chunk, or a video of minutes).
fn rest_joined(
    ffmpeg: &str,
    work: &Path,
    parts: &[PathBuf],
    log: &mut impl FnMut(&str),
) -> Result<PathBuf, String> {
    if parts.len() == 1 {
        return Ok(parts[0].clone());
    }
    let joined = work.join("rest.wav");
    media::join_parts(ffmpeg, parts, &joined, work, log)?;
    Ok(joined)
}

/// One half's per-chunk outputs, each in place as `p###.*` so a listing of the folder is the timeline.
fn assemble(
    ffmpeg: &str,
    tree: &Tree,
    paths: &Paths,
    list: &[Half],
    folder: &Path,
    work: &Path,
    log: &mut impl FnMut(&str),
) -> Result<(), String> {
    std::fs::create_dir_all(folder).map_err(|why| format!("{}: {why}", folder.display()))?;
    for (number, half) in list.iter().enumerate() {
        let out = folder.join(format!("p{number:03}.wav"));
        match half {
            Half::One(path) => fetch_stem(tree, paths, path, &out)?,
            Half::Parts(stems) if stems.len() > 1 => {
                let mut locals: Vec<PathBuf> = Vec::new();
                for (which, stem) in stems.iter().enumerate() {
                    let local = work.join(format!("mix{number:03}-{which}.wav"));
                    fetch_stem(tree, paths, stem, &local)?;
                    locals.push(local);
                }
                media::mix(ffmpeg, &locals, &out, log)?;
            }
            Half::Parts(stems) => {
                if let Some(one) = stems.first() {
                    fetch_stem(tree, paths, one, &out)?;
                }
            }
        }
    }
    Ok(())
}

/// The folder's `p###.*` files in name order, which is chunk order.
fn sorted_parts(folder: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut names: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().starts_with('p'))
                .unwrap_or(false)
        })
        .collect();
    names.sort();
    names
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
                        crate::request_timing::stamp_run(&mut row);
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
            crate::request_timing::stamp_run(&mut row);
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
    // §10's run column, stamped by the same door every other leg uses.
    crate::request_timing::stamp_run(&mut row);
    row.outcome = match outcome {
        Ok(status) if (200..300).contains(&status) => RowOutcome::Ok,
        Ok(status) => RowOutcome::Error(format!("{status}")),
        Err(why) => RowOutcome::Error(format!("{url}: {why}")),
    };
    let _ = requests::record(tree, &row);
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
