//! F4.4's wire: the three audio.cpp calls one spoken line needs, over real HTTP.
//!
//! [`crate::narrate_tts`] owns every S1–S6 rule and takes its two network steps as closures; this is the program's side of that seam. Settings are read on **every** call
//! (§02-services#1), it dials through [`crate::server_leg`] so each trip leaves its timed `requests.tsv` row (§09 §10), and hands answers to `narrate_tts::speak_line`.
//!
//! - [`health`] — S2: `GET /health`, then `GET /v1/models` (health alone green-lights a server with only step-1 models); verdict in `checks::tts_endpoint_verdict`.
//! - [`upload_reference`] — S3: `POST /v1/ui/upload` of `narrate/voice_ref.wav`, for EVERY line (`bodies::VOICE_REF_REUPLOADED_EVERY_LINE`): a server path dies with a restart.
//! - [`post_speech`] — S4/S5: `POST /v1/audio/speech`, answering `(status, bytes)`.
//!
//! A down server answers `Err` naming that server and the address tried, never `Ok` with nothing.

use crate::bodies::ServerPath;
use crate::layout::Tree;
use crate::narrate_tts::{self, Outcome};
use crate::requests::Request as TimedRow;
use crate::server_leg::{self, Dial};
use crate::services::{self, Endpoint, Kind, Server};
use crate::settings::{self, Conf};
use crate::voice_ref;

/// The step column of every row this module lands; the job stays `narrate`, the run the user started.
pub const STEP: &str = "speak";

/// The job column: F4.4 speaks lines of the narration record, so its rows belong to `narrate`.
pub const JOB: &str = "narrate";

/// The settings as they are ON DISK now, re-read per call. An unreadable file resolves as empty: a file caught mid-write must not stop a speak (as `settings_probe::current_conf`).
fn current_conf() -> Conf {
    settings::from_environment()
        .and_then(|paths| settings::read(&paths).ok())
        .unwrap_or_default()
}

/// audio.cpp's endpoint for this call: [`services::audio_endpoint`]'s precedence, unchanged. No override here, so a fake server replaces the real one by being the address.
fn endpoint(conf: &Conf) -> Endpoint {
    services::audio_endpoint(conf, |name| std::env::var(name).ok())
}

/// Ask audio.cpp one thing, timed and recorded against this session's `requests.tsv`.
///
/// The cancel closure is the live switch ([`crate::cancel_leg::cancel_check_now`]): §02-services#1 puts every request on the run context, so ⏹ ends the wait.
fn ask(
    tree: &Tree,
    base: &TimedRow,
    endpoint: &Endpoint,
    kind: Kind,
    filename: Option<&str>,
    body: Option<&[u8]>,
) -> Result<server_leg::Sent, String> {
    let cancelled = crate::cancel_leg::cancel_check_now();
    server_leg::call_dialed(
        tree,
        base,
        &Dial {
            server: Server::Audio,
            endpoint,
            kind,
            job: None,
            // Raw bytes: the voice reference is a WAV, and a lossy text copy of it is not one.
            body,
            filename,
            cancelled: &cancelled,
            liveness: None,
            waited_ms: 0,
            delta: None,
            budget: None,
            attempt: 1,
        },
    )
    .1
}

/// S2: is audio.cpp up, and what does it serve? Asked here, not read off cached state: speaking is not a repaint.
///
/// A `/health` that does not answer returns `Err` with `server_leg`'s sentence (naming the address tried), which the caller turns into the refusal.
pub fn health(tree: &Tree) -> Result<Vec<services::AudioModel>, String> {
    let conf = current_conf();
    let endpoint = endpoint(&conf);
    ask(
        tree,
        &row("health", ""),
        &endpoint,
        Kind::Health,
        None,
        None,
    )?;
    let models = ask(
        tree,
        &row("models", ""),
        &endpoint,
        Kind::ListModels,
        None,
        None,
    )?;
    Ok(services::parse_models(&models.body))
}

/// S3: put `narrate/voice_ref.wav` on the server and bring back the path the speech call will name.
///
/// The upload carries the file's bytes, not a local path: audio.cpp only sees inside its own container. A 401/403/405 is answered by [`services::upload_refused`] (the fix, not the status).
pub fn upload_reference(tree: &Tree) -> Result<ServerPath, String> {
    let conf = current_conf();
    let endpoint = endpoint(&conf);
    let reference = tree.voice_ref_wav();
    let bytes =
        std::fs::read(&reference).map_err(|why| format!("{}: {why}", reference.display()))?;
    let name = reference
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "voice_ref.wav".to_string());
    let reply = ask(
        tree,
        &row("upload", "voice_ref.wav"),
        &endpoint,
        Kind::Upload,
        Some(&name),
        Some(&bytes),
    )?;
    if let Some(refusal) = services::upload_refused(reply.status, &reply.body) {
        return Err(refusal);
    }
    if reply.status != 200 {
        return Err(format!(
            "uploading {name} to {} answered {}: {}",
            endpoint.url,
            reply.status,
            trim(&reply.body)
        ));
    }
    let parsed: serde_json::Value = serde_json::from_str(&reply.body).map_err(|_| {
        format!(
            "audio.cpp took {name} but answered something that is not JSON: {}",
            trim(&reply.body)
        )
    })?;
    let path = services::upload_path(&parsed)?;
    ServerPath::from_upload(&path)
        .ok_or_else(|| format!("the upload named {path:?}, which is not a server path"))
}

/// S4/S5: one `POST /v1/audio/speech`, answering the status and the reply BYTES.
///
/// Bytes, not a string: S5 counts them and a wav is not UTF-8, so a lossy read would measure the floor ([`narrate_tts::MIN_WAV_BYTES`]) against a different length. The session folder comes from [`hold_tree`], set first by [`speak`].
pub fn post_speech(body: &serde_json::Value) -> Result<(u16, Vec<u8>), String> {
    let tree = held_tree().expect("a speak sets its session tree before its speech leg runs");
    let conf = current_conf();
    speech_call(&tree, &endpoint(&conf), body)
}

/// The speech POST itself, tree explicit so [`speak`] can bind its session into the closure. Same endpoint rules, bearer header and cancel switch as every leg;
/// dialled by hand rather than through `server_leg::send` because a wav is binary and `Sent.body` is a `String`.
pub fn speech_call(
    tree: &Tree,
    endpoint: &Endpoint,
    body: &serde_json::Value,
) -> Result<(u16, Vec<u8>), String> {
    if crate::cancel_leg::cancelled() {
        return Err("speech: cancelled before sending".to_string());
    }
    let url = format!(
        "{}{}",
        endpoint.url.trim_end_matches('/'),
        Kind::Speech.path(None)
    );
    let builder = ureq::post(&url)
        .config()
        .timeout_connect(Some(server_leg::CONNECT_TIMEOUT))
        // `Kind::Speech` has no ceiling (a long line is long work, §1); only the connect is bounded.
        .timeout_global(services::timeout_for(Kind::Speech))
        .http_status_as_error(false)
        .build();
    let builder = match services::authorization(&endpoint.key) {
        Some(bearer) => builder.header("Authorization", bearer),
        None => builder,
    };
    let payload = body.to_string();
    // The model id goes on the row: the column that says WHICH voice was dialled.
    let model = body
        .get("model")
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_string();
    // §02 §1 on the voice: a TTS line takes its server BEFORE the socket opens and gives it back when the wav is in, or every synthesis queues inside audio.cpp unseen and `wait_s` stays empty.
    let (slot, waited) =
        match crate::slot_gate::hold(&endpoint.url, "speech", &|| crate::cancel_leg::cancelled()) {
            Ok(got) => got,
            // ⏹ landed while every voice slot was busy: nothing went out, so this is the refusal, not a server failure.
            Err(crate::slot_gate::Wait::Cancelled) => {
                return Err("speech: cancelled before sending".to_string())
            }
        };
    let base = row("speech", &model);
    let attempt = crate::request_timing::next_attempt(None);
    let started_ms = epoch_ms();
    // The wait is reported beside the call rather than folded into it (§10).
    let mut timer =
        crate::request_timing::Timer::queued((waited * 1000.0).round() as i64, started_ms);
    // One writer for the row however the dial ended: §09 §10 wants the request on file even when it died.
    let landed =
        |timer: &crate::request_timing::Timer, outcome: crate::requests::Outcome, got: i64| {
            let mut row = base.clone();
            row.sent_bytes = Some(payload.len() as i64);
            row.received_bytes = Some(got);
            // §10's run column, stamped by the same door every other leg uses.
            crate::request_timing::stamp_run(&mut row);
            let _ = crate::requests::record(
                tree,
                &crate::request_timing::finish_row(&row, timer, outcome, attempt),
            );
        };
    let response = match builder.send(payload.clone()) {
        Ok(response) => response,
        Err(why) => {
            timer.finished(epoch_ms());
            drop(slot);
            landed(&timer, crate::requests::Outcome::Error(why.to_string()), 0);
            return Err(format!("audio.cpp ({url}) is not answering: {why}"));
        }
    };
    let status = response.status().as_u16();
    let mut reader = response.into_body().into_reader();
    let mut bytes = Vec::new();
    match std::io::Read::read_to_end(&mut reader, &mut bytes) {
        Ok(_) => timer.first_byte(started_ms),
        Err(why) => {
            timer.finished(epoch_ms());
            drop(slot);
            landed(
                &timer,
                crate::requests::Outcome::Error(format!("mid-reply: {why}")),
                bytes.len() as i64,
            );
            return Err(format!("audio.cpp died mid-reply at {url}: {why}"));
        }
    }
    timer.finished(epoch_ms());
    // §4's hold covers the call only, so the slot goes back before the caller looks at the bytes.
    drop(slot);
    let outcome = if (200..300).contains(&status) {
        crate::requests::Outcome::Ok
    } else {
        crate::requests::Outcome::Error(format!("{status}"))
    };
    landed(&timer, outcome, bytes.len() as i64);
    if crate::cancel_leg::cancelled() {
        return Err("speech: cancelled mid-reply".to_string());
    }
    Ok((status, bytes))
}

/// The whole of F4.4 for one line over the real wire: S2, S3 and S4 dialled here, every rule (which step refused, which never ran) decided by [`narrate_tts::speak_line`].
///
/// `project_language` is P.policy.ttsLanguage and reaches [`narrate_tts::language`] unchanged; only an empty one falls back to "en".
pub fn speak(
    tree: &Tree,
    text: &str,
    emotion: &str,
    seed: u32,
    key: &str,
    project_language: &str,
) -> Outcome {
    hold_tree(tree);
    // S1 first, before ANY dial: with no reference there is nothing to clone, and a health check would put two requests on the wire for a line that cannot be spoken.
    if let Some(problem) = narrate_tts::reference_problem(tree) {
        // F4.6 S1: a voice pick re-cuts the reference on the next line, so a missing reference is BUILT here before it is refused; only a failed build says the F4.4 refusal, unchanged.
        match build_missing_reference(tree) {
            Ok(built) => {
                for line in &built.said {
                    crate::ui::window::log_line(line);
                }
                crate::ui::window::log_line(&voice_ref::built_log(&built, &source_file_name(tree)));
            }
            Err(why) => {
                crate::ui::window::log_line(&why);
                return Outcome::Refused(problem);
            }
        }
    }
    let conf = current_conf();
    let model = services::tts_model(&conf).to_string();
    let dial = endpoint(&conf);
    let models = match health(tree) {
        Ok(models) => models,
        Err(why) => return Outcome::Refused(why),
    };
    narrate_tts::speak_line(
        tree,
        text,
        emotion,
        seed,
        key,
        project_language,
        true,
        &models,
        &model,
        upload_reference,
        // The tree and endpoint are bound here so the closure keeps `post_speech`'s one-argument shape.
        move |body| speech_call(tree, &dial, body),
    )
}

/// Under this many seconds a reference holds nothing to clone.
const MIN_REFERENCE_SECONDS: f64 = 0.1;

pub fn build_missing_reference(tree: &Tree) -> Result<voice_ref::Built, String> {
    // A press that changes the takes, the voice or the pitch while ffmpeg runs makes this build stale.
    loop {
        let turn = crate::narrate_data::reference_turn();
        let built = cut_reference(tree);
        if crate::narrate_data::reference_turn() == turn {
            if built.is_err() {
                crate::narrate_data::drop_reference(tree, false);
            }
            return built;
        }
        crate::narrate_data::drop_reference(tree, false);
    }
}

fn cut_reference(tree: &Tree) -> Result<voice_ref::Built, String> {
    let source = voice_source(tree)?;
    let lane = source_lane(&source);
    let turns = speaker_turns(tree, &lane);
    let words = transcript_words(tree, &lane);
    let conf = current_conf();
    let ffmpeg = ffmpeg_program(&conf);
    if voice_ref::voice_slot(tree).is_none() && !voice_ref::has_picked_takes(tree, &source) {
        // A voices-folder file with no picks is the reference whole.
        let built = voice_ref::build(
            tree,
            &source,
            &ffmpeg,
            crate::narrate_data::read_pitch(tree),
        )?;
        return Ok(built);
    }
    let built = voice_ref::recut_for_voice(
        tree,
        &ffmpeg,
        &source,
        &turns,
        |from, until| words_in(&words, from, until),
        // §6's narrowing: the transcript lines inside the turn give its spoken span; no lines means no candidate.
        |from, until| word_span(&words, from, until),
    )?;
    // ffmpeg answers success for seconds past a recording's end, leaving a file with no sound.
    if voice_ref::wav_seconds(&built.served).is_some_and(|seconds| seconds < MIN_REFERENCE_SECONDS)
    {
        return Err(format!(
            "{source} has no sound in the seconds picked for this voice"
        ));
    }
    Ok(built)
}

/// The span the transcript's own words occupy inside a window (earliest line start to latest line end fully inside); `None` when no words fall there.
pub fn word_span(lines: &[(f64, f64, usize)], from: f64, until: f64) -> Option<(f64, f64)> {
    let mut span: Option<(f64, f64)> = None;
    for (start, end, _) in lines.iter().filter(|(s, e, _)| *s >= from && *e <= until) {
        let (first, last) = span.get_or_insert((*start, *end));
        *first = first.min(*start);
        *last = last.max(*end);
    }
    span
}

/// §6 (`Reference build`): the log names the recording the reference was cut FROM, by file name, not path.
fn source_file_name(tree: &Tree) -> String {
    voice_source(tree)
        .ok()
        .map(|path| {
            std::path::Path::new(&path)
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or(path)
        })
        .unwrap_or_else(|| "the tagged recording".to_string())
}

/// The recording the project's voice is cut from: a narrator's own, or a file in the voices folder.
fn voice_source(tree: &Tree) -> Result<String, String> {
    let Some(slot) = voice_ref::voice_slot(tree) else {
        let voice = crate::narrate_data::read_voice(tree);
        let folder = crate::ui::narrate_voices::voices_folder();
        let file = folder.join(&voice);
        return if file.is_file() {
            Ok(file.display().to_string())
        } else {
            Err(format!(
                "voice {voice} is no longer in {} \u{2014} pick another",
                folder.display()
            ))
        };
    };
    let project =
        crate::project::load(tree.dir()).map_err(|_| voice_ref::nothing_tagged(slot as usize))?;
    let written = crate::sources::narrator_of(&project, slot)
        .ok_or_else(|| voice_ref::nothing_tagged(slot as usize))?;
    let root = std::env::current_dir().unwrap_or_default();
    Ok(crate::layout::resolve(&root, tree.dir(), written)
        .display()
        .to_string())
}

/// Prepare keys everything per source by the file's stem; `prepare/inputs/<lane>/` is named after it.
fn source_lane(source: &str) -> String {
    std::path::Path::new(source)
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| source.to_string())
}

/// The diarized turns of one source, in seconds: `turns.json` stores samples of the 16 kHz mono file Prepare transcribed (§6). No file is no turns (voice_ref's S2 answer).
fn speaker_turns(tree: &Tree, lane: &str) -> Vec<(f64, f64, u32)> {
    const PREPARED_RATE: f64 = 16_000.0;
    let Ok(turns) = crate::requests::read_turns(tree, lane) else {
        return Vec::new();
    };
    turns
        .iter()
        .map(|turn| {
            (
                turn.start_sample as f64 / PREPARED_RATE,
                turn.end_sample as f64 / PREPARED_RATE,
                turn.speaker_id,
            )
        })
        .collect()
}

/// The transcript's lines, seconds and words, for the words-per-second floor: the fixed file when there is one, else the raw transcript; neither means zero words.
fn transcript_words(tree: &Tree, lane: &str) -> Vec<(f64, f64, usize)> {
    for path in [
        tree.transcript_fixed_tsv(lane),
        tree.transcript_tsv(lane),
        tree.commentary_fixed_tsv(lane),
    ] {
        if let Ok(lines) = crate::textfmt::read_lines(&path) {
            return lines
                .iter()
                .map(|line| (line.start, line.end, line.text.split_whitespace().count()))
                .collect();
        }
    }
    Vec::new()
}

/// How many transcript words fall inside a candidate window (S2's P.eng.refMinWordsPerSecond is checked against it).
fn words_in(lines: &[(f64, f64, usize)], from: f64, until: f64) -> usize {
    lines
        .iter()
        .filter(|(start, end, _)| *start >= from && *end <= until)
        .map(|(_, _, count)| *count)
        .sum()
}

/// The ffmpeg this call runs: the Settings box, else the bare program name so PATH answers; a missing program is named by `voice_ref`'s error.
fn ffmpeg_program(conf: &settings::Conf) -> String {
    if conf.ffmpeg.trim().is_empty() {
        "ffmpeg".to_string()
    } else {
        conf.ffmpeg.clone()
    }
}

thread_local! {
    /// The session the current speak records into, read by [`post_speech`]: the closure `narrate_tts::speak_line` takes carries the body alone.
    static SPEAK_TREE: std::cell::RefCell<Option<Tree>> = const { std::cell::RefCell::new(None) };
}

/// Remember the session for [`post_speech`]. Set by [`speak`] before it starts any leg.
pub fn hold_tree(tree: &Tree) {
    SPEAK_TREE.with(|cell| *cell.borrow_mut() = Some(tree.clone()));
}

pub fn held_tree() -> Option<Tree> {
    SPEAK_TREE.with(|cell| cell.borrow().clone())
}

/// A row for one of this module's calls: service audio, step [`STEP`], job [`JOB`]; only the model id travels here (the URL is in the timing columns).
fn row(kind: &str, model: &str) -> TimedRow {
    server_leg::base_row(Server::Audio, model, kind, "", STEP, JOB)
}

/// A reply trimmed and shortened enough for a log line.
fn trim(body: &str) -> String {
    let one = body.trim();
    if one.chars().count() > 200 {
        let cut: String = one.chars().take(200).collect();
        format!("{cut}…")
    } else {
        one.to_string()
    }
}

/// Milliseconds since the epoch, for the row's `started` column (`server_leg` keeps its copy private).
fn epoch_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_millis() as i64)
        .unwrap_or_default()
}
